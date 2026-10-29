//! Streaming a card write to a separate, elevated writer process.
//!
//! Raw disk access needs administrator rights, but the process the user runs
//! must stay unelevated: an elevated process cannot see the user's mapped
//! network drives (where firmware images live) and ignores drag-and-drop from
//! the unelevated Explorer. So the unelevated side reads the image and streams
//! it over a loopback socket to a small elevated worker that does nothing but
//! touch the disk and report progress.
//!
//! ```text
//! worker -> parent   <token>\n
//! parent -> worker   <header json>\n  [boot0  boot_package  sunxi_mbr  firmware  — write only]
//! worker -> parent   NDJSON events, then done | error
//! ```
//!
//! The worker does not trust the parent: it repeats the disk safety checks
//! itself (see `run_worker`). Cancelling is the parent going away — the worker
//! sees the stream end mid-image and stops.

use super::verify::{self, RegionDigest};
use super::{write_card_layout, Blobs, CardError};
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    /// Write the layout (and read it back, unless told not to).
    Write,
    /// Read the card and report what it holds; write nothing.
    Verify,
}

pub struct Header {
    pub op: Op,
    pub disk: String,
    pub force: bool,
    pub max_size_bytes: u64,
    pub boot0_len: u64,
    pub boot_package_len: u64,
    pub mbr_len: u64,
    pub firmware_len: u64,
    /// Write only: read the card back and compare once it is written.
    pub verify: bool,
}

impl Header {
    fn to_json(&self) -> Value {
        json!({
            "op": if self.op == Op::Write { "write" } else { "verify" },
            "disk": self.disk,
            "force": self.force,
            "max_size_bytes": self.max_size_bytes,
            "boot0_len": self.boot0_len,
            "boot_package_len": self.boot_package_len,
            "mbr_len": self.mbr_len,
            "firmware_len": self.firmware_len,
            "verify": self.verify,
        })
    }

    fn from_json(v: &Value) -> Result<Self> {
        let num = |k: &str| {
            v[k].as_u64()
                .with_context(|| format!("header field '{k}' missing"))
        };
        Ok(Self {
            op: match v["op"].as_str() {
                Some("write") => Op::Write,
                Some("verify") => Op::Verify,
                other => bail!("unknown card operation {other:?}"),
            },
            disk: v["disk"].as_str().context("header field 'disk' missing")?.to_string(),
            force: v["force"].as_bool().unwrap_or(false),
            max_size_bytes: num("max_size_bytes")?,
            boot0_len: num("boot0_len")?,
            boot_package_len: num("boot_package_len")?,
            mbr_len: num("mbr_len")?,
            firmware_len: num("firmware_len")?,
            verify: v["verify"].as_bool().unwrap_or(true),
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

pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// An opened disk plus whatever must stay alive while it is written (Windows
/// volume locks). `out` is declared first so it closes before the guard drops.
pub struct Target {
    pub out: Box<dyn DiskWrite>,
    /// Never read: it is here to be dropped after `out`.
    #[allow(dead_code)]
    pub guard: Option<Box<dyn Any>>,
}

/// What the elevated worker needs from the machine it runs on. Production opens
/// real disks after the safety checks; tests substitute an in-memory one.
pub trait WorkerHost {
    fn open_write(&mut self, header: &Header) -> Result<Target>;
    /// Read access that bypasses the OS cache, so what comes back is what the
    /// card holds and not what was just written.
    fn open_read(&mut self, header: &Header) -> Result<Box<dyn ReadSeek>>;
}

// --- Worker (elevated) side ------------------------------------------------

/// The regions of a card, with the lengths the header announced.
fn regions_of(h: &Header) -> [(&'static str, u64, u64); 5] {
    verify::region_list(h.boot0_len, h.boot_package_len, h.mbr_len, h.firmware_len)
}

/// Serve one request. The host validates the header and opens the disk before
/// any image data is read, so a refused disk costs nothing.
pub fn worker_session(stream: TcpStream, token: &str, host: &mut dyn WorkerHost) -> Result<()> {
    let mut to_parent = stream.try_clone().context("cloning the socket")?;
    writeln!(to_parent, "{token}").context("sending the token")?;
    let mut from_parent = BufReader::new(stream);

    let mut line = String::new();
    from_parent.read_line(&mut line).context("reading the header")?;
    let header = Header::from_json(&serde_json::from_str(&line).context("parsing the header")?)?;

    let events = to_parent.try_clone().context("cloning the socket")?;
    let rep = Reporter::to_writer(Box::new(events));

    let result = match header.op {
        Op::Write => (|| -> Result<()> {
            let mut target = host.open_write(&header)?;
            let mut boot0 = vec![0u8; header.boot0_len as usize];
            from_parent.read_exact(&mut boot0).context("reading boot0")?;
            let mut boot_package = vec![0u8; header.boot_package_len as usize];
            from_parent
                .read_exact(&mut boot_package)
                .context("reading boot_package")?;
            let mut mbr = vec![0u8; header.mbr_len as usize];
            from_parent.read_exact(&mut mbr).context("reading the partition table")?;
            let blobs = Blobs { boot0, boot_package, mbr };
            let mut firmware = (&mut from_parent).take(header.firmware_len);
            let written = write_card_layout(
                &mut target.out,
                &blobs,
                &mut firmware,
                header.firmware_len,
                &rep,
            )?;
            target.out.sync().context("flushing the card after writing")?;
            if header.verify {
                let mut card = host.open_read(&header)?;
                verify::verify_written(&mut card, &written, &rep)?;
            }
            Ok(())
        })(),
        Op::Verify => (|| -> Result<()> {
            let mut card = host.open_read(&header)?;
            rep.step("verifying", "reading the card back");
            let mut digests = Vec::new();
            for (name, lba, len) in regions_of(&header) {
                digests.push(verify::digest_card(&mut card, name, lba, len, &rep)?);
            }
            rep.data(
                "digests",
                json!({ "regions": digests.iter().map(|d| d.to_json()).collect::<Vec<_>>() }),
            );
            // Judged separately so a mismatch and a structural fault are both
            // reported rather than the first hiding the second.
            let structure = verify::check_card_structure(&mut card);
            rep.data(
                "structure",
                json!({
                    "ok": structure.is_ok(),
                    "message": structure.as_ref().err().map(|e| format!("{e:#}")),
                }),
            );
            Ok(())
        })(),
    };

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
    digests: Option<Vec<RegionDigest>>,
    structure: Option<std::result::Result<(), String>>,
}

fn static_kind(kind: &str) -> Option<&'static str> {
    [
        "is_system_disk",
        "too_small",
        "not_removable",
        "too_large",
        "size_mismatch",
        "verify_failed",
        "structure_invalid",
    ]
    .into_iter()
    .find(|k| *k == kind)
}

/// Re-express one event from the worker through the parent's own reporter, so
/// the front-end sees exactly what a direct run would have produced.
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
        "digests" => {
            outcome.digests = ev["regions"]
                .as_array()
                .and_then(|a| a.iter().map(|r| RegionDigest::from_json(r).ok()).collect());
        }
        "structure" => {
            outcome.structure = Some(if ev["ok"].as_bool().unwrap_or(false) {
                Ok(())
            } else {
                Err(ev["message"].as_str().unwrap_or("the boot chain is invalid").to_string())
            });
        }
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

fn into_result(outcome: &mut Outcome) -> Result<()> {
    if let Some((message, kind)) = outcome.error.take() {
        return Err(match kind.as_deref().and_then(static_kind) {
            Some(kind) => CardError::new(kind, message).into(),
            None => anyhow!(message),
        });
    }
    if outcome.done {
        return Ok(());
    }
    bail!("the elevated card writer ended without reporting a result")
}

/// Block until the worker reports `done` or `error` (or goes away).
fn wait_for_result(rx: &Receiver<Value>, rep: &Reporter, outcome: &mut Outcome) -> Result<()> {
    while !outcome.done && outcome.error.is_none() {
        match rx.recv_timeout(Duration::from_secs(900)) {
            Ok(ev) => relay(&ev, rep, outcome),
            Err(RecvTimeoutError::Timeout) => bail!("the card writer stopped responding"),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

/// Check the worker's token and start listening to what it says.
fn authenticate(stream: TcpStream, token: &str) -> Result<(TcpStream, Receiver<Value>)> {
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
    Ok((stream, rx))
}

/// Drive a write on the worker at the other end of `stream`: authenticate it,
/// send the data, and relay its progress until it reports a result.
pub fn parent_session(
    stream: TcpStream,
    token: &str,
    header: &Header,
    blobs: &Blobs,
    firmware: &mut dyn Read,
    rep: &Reporter,
) -> Result<()> {
    let (mut writer, rx) = authenticate(stream, token)?;
    let mut outcome = Outcome::default();
    let sent = (|| -> io::Result<()> {
        writeln!(writer, "{}", header.to_json())?;
        writer.write_all(&blobs.boot0)?;
        writer.write_all(&blobs.boot_package)?;
        writer.write_all(&blobs.mbr)?;
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

    wait_for_result(&rx, rep, &mut outcome)?;
    let _ = writer.shutdown(Shutdown::Both);
    into_result(&mut outcome)
}

/// What the worker found on the card.
pub struct CardReport {
    pub digests: Option<Vec<RegionDigest>>,
    pub structure: Option<std::result::Result<(), String>>,
}

/// Have the worker read the card back and report. The caller [`judge`]s it, so
/// it can work out what the card should hold while the worker is reading.
pub fn parent_verify(
    stream: TcpStream,
    token: &str,
    header: &Header,
    rep: &Reporter,
) -> Result<CardReport> {
    let (mut writer, rx) = authenticate(stream, token)?;
    writeln!(writer, "{}", header.to_json()).context("sending the request")?;
    let mut outcome = Outcome::default();
    wait_for_result(&rx, rep, &mut outcome)?;
    let _ = writer.shutdown(Shutdown::Both);
    into_result(&mut outcome)?;
    Ok(CardReport { digests: outcome.digests, structure: outcome.structure })
}

/// Compare what the card holds with what it should, then its structure.
pub fn judge(
    expected: Option<&[RegionDigest]>,
    actual: Option<&[RegionDigest]>,
    structure: Option<std::result::Result<(), String>>,
    rep: &Reporter,
) -> Result<()> {
    if let Some(expected) = expected {
        let actual = actual.context("the card writer sent no read-back")?;
        verify::compare(expected, actual)?;
        rep.step("content_ok", "the card holds exactly what the image says");
    }
    match structure {
        Some(Ok(())) => rep.step("structure_ok", "boot0, boot_package, partition table and firmware headers are valid"),
        Some(Err(message)) => {
            return Err(CardError::new("structure_invalid", message).into());
        }
        None => bail!("the card writer sent no structure check"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdcard::{BOOT0_LBA, BOOT_PACKAGE_BACKUP_LBA, BOOT_PACKAGE_LBA, FIRMWARE_LBA, MBR_LBA};
    use std::io::{Cursor, SeekFrom};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    /// A "disk" the test can inspect (and damage) after the worker is done.
    #[derive(Clone)]
    struct Shared(Arc<Mutex<Cursor<Vec<u8>>>>);

    impl Write for Shared {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for Shared {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            self.0.lock().unwrap().read(b)
        }
    }

    impl Seek for Shared {
        fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
            self.0.lock().unwrap().seek(p)
        }
    }

    impl DiskWrite for Shared {}

    struct FakeHost {
        disk: Shared,
        refuse: Option<&'static str>,
        /// Flip a byte as the card is read back, as a failing card would.
        corrupt_on_read: Option<usize>,
    }

    impl WorkerHost for FakeHost {
        fn open_write(&mut self, _: &Header) -> Result<Target> {
            if let Some(kind) = self.refuse {
                return Err(CardError::new(kind, "that is the system disk").into());
            }
            Ok(Target { out: Box::new(self.disk.clone()), guard: None })
        }

        fn open_read(&mut self, _: &Header) -> Result<Box<dyn ReadSeek>> {
            let mut bytes = self.disk.0.lock().unwrap().get_ref().clone();
            if let Some(at) = self.corrupt_on_read {
                bytes[at] ^= 0xFF;
            }
            Ok(Box::new(Cursor::new(bytes)))
        }
    }

    /// (worker end, parent end) of a real loopback connection.
    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let worker = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (parent, _) = listener.accept().unwrap();
        (worker, parent)
    }

    fn new_disk() -> Shared {
        Shared(Arc::new(Mutex::new(Cursor::new(Vec::new()))))
    }

    fn header(op: Op, b: &Blobs, fw_len: usize) -> Header {
        Header {
            op,
            disk: "disk-under-test".into(),
            force: false,
            max_size_bytes: 0,
            boot0_len: b.boot0.len() as u64,
            boot_package_len: b.boot_package.len() as u64,
            mbr_len: b.mbr.len() as u64,
            firmware_len: fw_len as u64,
            verify: true,
        }
    }

    fn blobs() -> (Blobs, Vec<u8>) {
        let mut fw = vec![0x77u8; 3_000_000];
        fw[..8].copy_from_slice(b"IMAGEWTY");
        let mut mbr = vec![0x33u8; 65_536];
        mbr[8..16].copy_from_slice(b"softw411");
        let b = Blobs {
            boot0: verify::tests::good_boot0(),
            boot_package: verify::tests::good_toc1(),
            mbr,
        };
        (b, fw)
    }

    fn expected_digests(b: &Blobs, fw: &[u8]) -> Vec<RegionDigest> {
        let main = verify::card_boot_package(&b.boot_package).unwrap();
        let d = |n, lba, data: &[u8]| {
            verify::digest_source(n, lba, data.len() as u64, &mut &data[..]).unwrap()
        };
        vec![
            d("firmware", FIRMWARE_LBA, fw),
            d("sunxi_mbr", MBR_LBA, &b.mbr),
            d("boot_package", BOOT_PACKAGE_LBA, &main),
            d("boot_package_backup", BOOT_PACKAGE_BACKUP_LBA, &b.boot_package),
            d("boot0", BOOT0_LBA, &b.boot0),
        ]
    }

    fn write_through_worker(host: FakeHost, b: &Blobs, fw: &[u8]) -> Result<()> {
        let (worker_end, parent_end) = pair();
        let worker = thread::spawn(move || {
            let mut host = host;
            worker_session(worker_end, "tok", &mut host)
        });
        let result = parent_session(
            parent_end,
            "tok",
            &header(Op::Write, b, fw.len()),
            b,
            &mut &fw[..],
            &Reporter::new(false),
        );
        let _ = worker.join();
        result
    }

    #[test]
    fn the_image_reaches_the_worker_at_the_fixed_lbas_and_reads_back_clean() {
        let (b, fw) = blobs();
        let disk = new_disk();
        let host = FakeHost { disk: disk.clone(), refuse: None, corrupt_on_read: None };
        write_through_worker(host, &b, &fw).unwrap();

        let bytes = disk.0.lock().unwrap().get_ref().clone();
        let at = |lba: u64| (lba * 512) as usize;
        assert_eq!(&bytes[at(BOOT0_LBA)..at(BOOT0_LBA) + b.boot0.len()], &b.boot0[..]);
        assert_eq!(&bytes[at(MBR_LBA)..at(MBR_LBA) + b.mbr.len()], &b.mbr[..]);
        assert_eq!(
            &bytes[at(BOOT_PACKAGE_BACKUP_LBA)..at(BOOT_PACKAGE_BACKUP_LBA) + b.boot_package.len()],
            &b.boot_package[..]
        );
        let main = &bytes[at(BOOT_PACKAGE_LBA)..at(BOOT_PACKAGE_LBA) + b.boot_package.len()];
        assert_eq!(main, &verify::card_boot_package(&b.boot_package).unwrap()[..]);
        assert_eq!(&bytes[at(FIRMWARE_LBA)..at(FIRMWARE_LBA) + fw.len()], &fw[..]);
    }

    #[test]
    fn a_card_that_returns_other_bytes_than_were_written_is_caught() {
        let (b, fw) = blobs();
        let flipped = (FIRMWARE_LBA * 512) as usize + 70_000;
        let host = FakeHost {
            disk: new_disk(),
            refuse: None,
            corrupt_on_read: Some(flipped),
        };
        let err = write_through_worker(host, &b, &fw).unwrap_err();
        let card = err.downcast_ref::<CardError>().expect("a CardError");
        assert_eq!(card.kind, "verify_failed");
        // 70000 falls in the second 64 KB chunk.
        assert!(format!("{err:#}").contains("byte 65536"), "{err:#}");
    }

    #[test]
    fn a_refused_disk_comes_back_as_the_same_safety_error() {
        let (b, _) = blobs();
        let fw = vec![1u8; 20_000_000]; // still being sent when the refusal lands
        let host = FakeHost { disk: new_disk(), refuse: Some("is_system_disk"), corrupt_on_read: None };
        let err = write_through_worker(host, &b, &fw).unwrap_err();
        assert_eq!(err.downcast_ref::<CardError>().unwrap().kind, "is_system_disk");
        assert!(format!("{err:#}").contains("system disk"));
    }

    #[test]
    fn a_worker_with_the_wrong_token_is_refused() {
        let (worker_end, parent_end) = pair();
        let worker = thread::spawn(move || {
            let mut host = FakeHost { disk: new_disk(), refuse: None, corrupt_on_read: None };
            worker_session(worker_end, "not-the-token", &mut host)
        });
        let data = vec![0u8; 4096];
        let b = Blobs { boot0: data.clone(), boot_package: data.clone(), mbr: data.clone() };
        let err = parent_session(
            parent_end,
            "tok",
            &header(Op::Write, &b, data.len()),
            &b,
            &mut &data[..],
            &Reporter::new(false),
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("authentication"));
        let _ = worker.join();
    }

    fn written_disk() -> (Shared, Blobs, Vec<u8>) {
        let (b, fw) = blobs();
        let disk = new_disk();
        let host = FakeHost { disk: disk.clone(), refuse: None, corrupt_on_read: None };
        write_through_worker(host, &b, &fw).unwrap();
        (disk, b, fw)
    }

    fn verify_through_worker(host: FakeHost, expected: Option<&[RegionDigest]>, h: Header) -> Result<()> {
        let (worker_end, parent_end) = pair();
        let worker = thread::spawn(move || {
            let mut host = host;
            worker_session(worker_end, "tok", &mut host)
        });
        let rep = Reporter::new(false);
        let report = parent_verify(parent_end, "tok", &h, &rep);
        let _ = worker.join();
        let report = report?;
        judge(expected, report.digests.as_deref(), report.structure, &rep)
    }

    #[test]
    fn verifying_an_untouched_card_against_its_image_passes() {
        let (disk, b, fw) = written_disk();
        let host = FakeHost { disk, refuse: None, corrupt_on_read: None };
        let expected = expected_digests(&b, &fw);
        verify_through_worker(host, Some(&expected), header(Op::Verify, &b, fw.len())).unwrap();
    }

    #[test]
    fn verifying_without_an_image_still_judges_the_structure() {
        let (disk, b, fw) = written_disk();
        let host = FakeHost { disk, refuse: None, corrupt_on_read: None };
        verify_through_worker(host, None, header(Op::Verify, &b, fw.len())).unwrap();
    }

    #[test]
    fn a_card_that_drifted_from_the_image_is_reported_with_its_location() {
        let (disk, b, fw) = written_disk();
        let expected = expected_digests(&b, &fw);
        let host = FakeHost {
            disk,
            refuse: None,
            corrupt_on_read: Some((FIRMWARE_LBA * 512) as usize + 5),
        };
        let err = verify_through_worker(host, Some(&expected), header(Op::Verify, &b, fw.len()))
            .unwrap_err();
        assert_eq!(err.downcast_ref::<CardError>().unwrap().kind, "verify_failed");
        assert!(format!("{err:#}").contains("firmware"), "{err:#}");
    }

    #[test]
    fn a_card_with_a_broken_boot_chain_fails_the_structure_check() {
        let (disk, b, fw) = written_disk();
        let host = FakeHost {
            disk,
            refuse: None,
            corrupt_on_read: Some((BOOT_PACKAGE_LBA * 512) as usize + 100),
        };
        let err = verify_through_worker(host, None, header(Op::Verify, &b, fw.len())).unwrap_err();
        assert_eq!(err.downcast_ref::<CardError>().unwrap().kind, "structure_invalid");
        assert!(format!("{err:#}").contains("boot_package"), "{err:#}");
    }

    #[test]
    fn a_card_whose_main_package_lacks_the_card_work_mode_fails_the_structure_check() {
        // What Hawk Flasher used to write: the shipped package in both slots.
        let (disk, b, fw) = written_disk();
        {
            let mut d = disk.0.lock().unwrap();
            let at = (BOOT_PACKAGE_LBA * 512) as usize;
            let shipped = b.boot_package.clone();
            d.get_mut()[at..at + shipped.len()].copy_from_slice(&shipped);
        }
        let host = FakeHost { disk, refuse: None, corrupt_on_read: None };
        let err = verify_through_worker(host, None, header(Op::Verify, &b, fw.len())).unwrap_err();
        assert_eq!(err.downcast_ref::<CardError>().unwrap().kind, "structure_invalid");
        assert!(format!("{err:#}").contains("work mode"), "{err:#}");
    }
}
