//! Streaming a card write to a separate, elevated writer process.
//!
//! Raw disk access needs administrator rights, but the process the user runs
//! must stay unelevated: an elevated process cannot see the user's mapped
//! network drives (where firmware images live) and ignores drag-and-drop from
//! the unelevated Explorer. So the unelevated side reads the image and streams
//! it over a loopback socket to a small elevated worker that does nothing but
//! write the disk and report progress.
//!
//! ```text
//! worker -> parent   <token>\n
//! parent -> worker   <header json>\n  boot0 bytes  boot_package bytes  firmware bytes
//! worker -> parent   NDJSON events (card_plan, progress, ..., then done | error)
//! ```
//!
//! The worker does not trust the parent: it repeats the disk safety checks
//! itself (see `run_worker`). Cancelling is the parent going away — the worker
//! sees the stream end mid-image and stops.

use super::{write_card_layout, SafetyError};
use crate::event::Reporter;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::any::Any;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const CHUNK: usize = 64 * 1024;

pub struct Header {
    pub disk: String,
    pub force: bool,
    pub max_size_bytes: u64,
    pub boot0_len: u64,
    pub boot_package_len: u64,
    pub firmware_len: u64,
}

impl Header {
    fn to_json(&self) -> Value {
        json!({
            "disk": self.disk,
            "force": self.force,
            "max_size_bytes": self.max_size_bytes,
            "boot0_len": self.boot0_len,
            "boot_package_len": self.boot_package_len,
            "firmware_len": self.firmware_len,
        })
    }

    fn from_json(v: &Value) -> Result<Self> {
        let num = |k: &str| {
            v[k].as_u64()
                .with_context(|| format!("header field '{k}' missing"))
        };
        Ok(Self {
            disk: v["disk"].as_str().context("header field 'disk' missing")?.to_string(),
            force: v["force"].as_bool().unwrap_or(false),
            max_size_bytes: num("max_size_bytes")?,
            boot0_len: num("boot0_len")?,
            boot_package_len: num("boot_package_len")?,
            firmware_len: num("firmware_len")?,
        })
    }
}

/// A destination the card layout can be written to.
pub trait DiskWrite: Write + Seek {
    /// Push everything to the device before declaring success.
    fn sync(&mut self) -> io::Result<()> {
        self.flush()
    }
}

impl DiskWrite for File {
    fn sync(&mut self) -> io::Result<()> {
        self.sync_all()
    }
}

/// An opened disk plus whatever must stay alive while it is written (Windows
/// volume locks). `out` is declared first so it closes before the guard drops.
pub struct Target {
    pub out: Box<dyn DiskWrite>,
    /// Never read: it is here to be dropped after `out`.
    #[allow(dead_code)]
    pub guard: Option<Box<dyn Any>>,
}

// --- Worker (elevated) side ------------------------------------------------

/// Serve one write. `open` validates the header and opens the disk; it runs
/// before any image data is read, so a refused disk costs nothing.
pub fn worker_session(
    stream: TcpStream,
    token: &str,
    open: &mut dyn FnMut(&Header) -> Result<Target>,
) -> Result<()> {
    let mut to_parent = stream.try_clone().context("cloning the socket")?;
    writeln!(to_parent, "{token}").context("sending the token")?;
    let mut from_parent = BufReader::new(stream);

    let mut line = String::new();
    from_parent.read_line(&mut line).context("reading the header")?;
    let header = Header::from_json(&serde_json::from_str(&line).context("parsing the header")?)?;

    let events = to_parent.try_clone().context("cloning the socket")?;
    let rep = Reporter::to_writer(Box::new(events));

    let result = (|| -> Result<()> {
        let mut target = open(&header)?;
        let mut boot0 = vec![0u8; header.boot0_len as usize];
        from_parent.read_exact(&mut boot0).context("reading boot0")?;
        let mut boot_package = vec![0u8; header.boot_package_len as usize];
        from_parent
            .read_exact(&mut boot_package)
            .context("reading boot_package")?;
        let mut firmware = (&mut from_parent).take(header.firmware_len);
        write_card_layout(
            &mut target.out,
            &boot0,
            &boot_package,
            &mut firmware,
            header.firmware_len,
            &rep,
        )?;
        target.out.sync().context("flushing the card after writing")?;
        Ok(())
    })();

    match &result {
        Ok(()) => rep.done(),
        Err(e) => rep.fail(e),
    }
    // Closing with unread data still queued makes the OS reset the connection,
    // which can discard the verdict above before the parent reads it. Say we
    // are done sending, then let the parent finish and close first.
    let _ = to_parent.shutdown(Shutdown::Write);
    let _ = io::copy(&mut from_parent, &mut io::sink());
    result
}

// --- Parent (unelevated) side ----------------------------------------------

#[derive(Default)]
struct Outcome {
    done: bool,
    error: Option<(String, Option<String>)>,
}

fn static_kind(kind: &str) -> Option<&'static str> {
    ["is_system_disk", "too_small", "not_removable", "too_large", "size_mismatch"]
        .into_iter()
        .find(|k| *k == kind)
}

/// Re-express one event from the worker through the parent's own reporter, so
/// the front-end sees exactly what a direct write would have produced.
fn relay(ev: &Value, rep: &Reporter, outcome: &mut Outcome) {
    let name = ev["name"].as_str().unwrap_or("");
    let num = |k: &str| ev[k].as_u64().unwrap_or(0);
    match ev["event"].as_str().unwrap_or("") {
        "card_plan" => {
            let regions: Vec<(&str, u64)> = ev["regions"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|r| (r["name"].as_str().unwrap_or(""), r["bytes"].as_u64().unwrap_or(0)))
                        .collect()
                })
                .unwrap_or_default();
            rep.card_plan(&regions);
        }
        "partition_begin" => {
            rep.partition_begin(num("index") as usize, num("total") as usize, name, num("bytes"))
        }
        "progress" => {
            let (written, total) = (num("written"), num("total"));
            if written >= total {
                rep.progress_done(name, total);
            } else {
                rep.progress(name, written, total);
            }
        }
        "partition_end" => {
            let seconds = ev["seconds"].as_f64().unwrap_or(0.0);
            rep.partition_end(
                num("index") as usize,
                num("total") as usize,
                name,
                num("bytes"),
                ev["format"].as_str().unwrap_or("raw"),
                seconds,
            );
            rep.prose(format!(
                "  [{}/{}] {name}: {} bytes ({seconds:.1}s)",
                num("index"),
                num("total"),
                num("bytes")
            ));
        }
        "step" => rep.step(
            ev["step"].as_str().unwrap_or(""),
            ev["message"].as_str().unwrap_or(""),
        ),
        "log" => rep.log(ev["message"].as_str().unwrap_or("")),
        "error" => {
            outcome.error = Some((
                ev["message"].as_str().unwrap_or("the card writer failed").to_string(),
                ev["kind"].as_str().map(str::to_string),
            ));
        }
        "done" => outcome.done = true,
        _ => {}
    }
}

fn drain(rx: &Receiver<Value>, rep: &Reporter, outcome: &mut Outcome) {
    while let Ok(ev) = rx.try_recv() {
        relay(&ev, rep, outcome);
    }
}

fn into_result(outcome: Outcome) -> Result<()> {
    if let Some((message, kind)) = outcome.error {
        return Err(match kind.as_deref().and_then(static_kind) {
            Some(kind) => SafetyError::new(kind, message).into(),
            None => anyhow!(message),
        });
    }
    if outcome.done {
        return Ok(());
    }
    bail!("the elevated card writer ended without reporting a result")
}

/// Drive the worker on the other end of `stream`: authenticate it, send the
/// data, and relay its progress until it reports a result.
pub fn parent_session(
    stream: TcpStream,
    token: &str,
    header: &Header,
    boot0: &[u8],
    boot_package: &[u8],
    firmware: &mut dyn Read,
    rep: &Reporter,
) -> Result<()> {
    stream.set_read_timeout(Some(AUTH_TIMEOUT))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line).context("waiting for the card writer's token")?;
    if line.trim() != token {
        bail!("the card writer failed authentication");
    }
    stream.set_read_timeout(None)?;

    let (tx, rx) = mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in reader.lines().map_while(|l| l.ok()) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        }
    });

    let mut outcome = Outcome::default();
    let mut writer = stream;
    let sent = (|| -> io::Result<()> {
        writeln!(writer, "{}", header.to_json())?;
        writer.write_all(boot0)?;
        writer.write_all(boot_package)?;
        let mut buf = vec![0u8; CHUNK];
        let mut left = header.firmware_len;
        while left > 0 {
            // A refusal from the worker arrives as an event; stop feeding it.
            drain(&rx, rep, &mut outcome);
            if outcome.error.is_some() {
                return Ok(());
            }
            let want = left.min(CHUNK as u64) as usize;
            firmware.read_exact(&mut buf[..want])?;
            writer.write_all(&buf[..want])?;
            left -= want as u64;
        }
        writer.flush()
    })();

    if let Err(e) = sent {
        // A broken pipe usually means the worker bailed and said why.
        while outcome.error.is_none() {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(ev) => relay(&ev, rep, &mut outcome),
                Err(_) => break,
            }
        }
        if outcome.error.is_none() {
            return Err(e).context("streaming the image to the card writer");
        }
    }

    while !outcome.done && outcome.error.is_none() {
        match rx.recv_timeout(Duration::from_secs(600)) {
            Ok(ev) => relay(&ev, rep, &mut outcome),
            Err(RecvTimeoutError::Timeout) => bail!("the card writer stopped responding"),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = writer.shutdown(Shutdown::Both);
    into_result(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdcard::{
        BOOT0_LBA, BOOT_PACKAGE_BACKUP_LBA, BOOT_PACKAGE_LBA, FIRMWARE_LBA,
    };
    use std::io::{Cursor, SeekFrom};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    /// A "disk" the test can inspect after the worker thread is done with it.
    struct Shared(Arc<Mutex<Cursor<Vec<u8>>>>);

    impl Write for Shared {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Seek for Shared {
        fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
            self.0.lock().unwrap().seek(p)
        }
    }

    impl DiskWrite for Shared {}

    /// (worker end, parent end) of a real loopback connection.
    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let worker = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (parent, _) = listener.accept().unwrap();
        (worker, parent)
    }

    fn header(boot0: &[u8], bp: &[u8], fw: &[u8]) -> Header {
        Header {
            disk: "disk-under-test".into(),
            force: false,
            max_size_bytes: 0,
            boot0_len: boot0.len() as u64,
            boot_package_len: bp.len() as u64,
            firmware_len: fw.len() as u64,
        }
    }

    #[test]
    fn the_image_reaches_the_worker_at_the_fixed_lbas() {
        let disk = Arc::new(Mutex::new(Cursor::new(Vec::new())));
        let (worker_end, parent_end) = pair();
        let for_worker = disk.clone();
        let worker = thread::spawn(move || {
            worker_session(worker_end, "tok", &mut |_| {
                Ok(Target {
                    out: Box::new(Shared(for_worker.clone())),
                    guard: None,
                })
            })
        });

        let boot0 = vec![0xB0u8; 4096];
        let bp = vec![0xB1u8; 3000];
        let mut fw = vec![0x77u8; 3_000_000];
        fw[..8].copy_from_slice(b"IMAGEWTY");

        parent_session(
            parent_end,
            "tok",
            &header(&boot0, &bp, &fw),
            &boot0,
            &bp,
            &mut &fw[..],
            &Reporter::new(false),
        )
        .unwrap();
        worker.join().unwrap().unwrap();

        let bytes = disk.lock().unwrap().get_ref().clone();
        let at = |lba: u64| (lba * 512) as usize;
        assert_eq!(&bytes[at(BOOT0_LBA)..at(BOOT0_LBA) + 4096], &boot0[..]);
        assert_eq!(&bytes[at(BOOT_PACKAGE_LBA)..at(BOOT_PACKAGE_LBA) + 3000], &bp[..]);
        assert_eq!(
            &bytes[at(BOOT_PACKAGE_BACKUP_LBA)..at(BOOT_PACKAGE_BACKUP_LBA) + 3000],
            &bp[..]
        );
        assert_eq!(&bytes[at(FIRMWARE_LBA)..at(FIRMWARE_LBA) + fw.len()], &fw[..]);
    }

    #[test]
    fn a_refused_disk_comes_back_as_the_same_safety_error() {
        let (worker_end, parent_end) = pair();
        let worker = thread::spawn(move || {
            worker_session(worker_end, "tok", &mut |_| {
                Err(SafetyError::new("is_system_disk", "that is the system disk").into())
            })
        });

        // Big enough that the parent is still sending when the refusal lands.
        let boot0 = vec![0u8; 4096];
        let bp = vec![0u8; 4096];
        let fw = vec![1u8; 20_000_000];
        let err = parent_session(
            parent_end,
            "tok",
            &header(&boot0, &bp, &fw),
            &boot0,
            &bp,
            &mut &fw[..],
            &Reporter::new(false),
        )
        .unwrap_err();

        let safety = err.downcast_ref::<SafetyError>().expect("a SafetyError");
        assert_eq!(safety.kind, "is_system_disk");
        assert!(format!("{err:#}").contains("system disk"));
        assert!(worker.join().unwrap().is_err());
    }

    #[test]
    fn a_worker_with_the_wrong_token_is_refused() {
        let (worker_end, parent_end) = pair();
        let worker = thread::spawn(move || {
            worker_session(worker_end, "not-the-token", &mut |_| unreachable!())
        });
        let data = vec![0u8; 4096];
        let err = parent_session(
            parent_end,
            "tok",
            &header(&data, &data, &data),
            &data,
            &data,
            &mut &data[..],
            &Reporter::new(false),
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("authentication"));
        let _ = worker.join().unwrap();
    }
}
