//! Progress reporting, for humans and for a GUI front-end.
//!
//! Long-running commands report through a [`Reporter`] instead of printing
//! directly. With `--json` it writes newline-delimited JSON to stdout — one
//! self-describing object per line, each with an `event` key — so a front-end
//! can drive a progress bar without screen-scraping prose that we're free to
//! reword. Without it, the same information is printed as before.

use serde_json::{json, Value};
use std::cell::Cell;
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

/// Minimum gap between `progress` events. `super` moves ~1 GB in 64 KB chunks,
/// i.e. ~15k callbacks for a single partition; unthrottled, serialising those
/// would cost more than the flashing does.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

pub struct Reporter {
    json: bool,
    /// Human mode only: a `\r` progress line is on screen and the next
    /// ordinary line has to break out of it first.
    line_pending: Cell<bool>,
    last_progress: Cell<Option<Instant>>,
}

impl Reporter {
    pub fn new(json: bool) -> Self {
        Self {
            json,
            line_pending: Cell::new(false),
            last_progress: Cell::new(None),
        }
    }

    fn emit(&self, value: Value) {
        println!("{value}");
        // A front-end reading the pipe should see events as they happen, not
        // when the block buffer fills — stdout is not line-buffered when it is
        // a pipe rather than a terminal.
        let _ = std::io::stdout().flush();
    }

    /// End any in-progress `\r` line before printing something else.
    fn clear_line(&self) {
        if self.line_pending.replace(false) {
            println!();
        }
    }

    /// True when the caller should produce structured output via
    /// [`Reporter::data`] instead of printing prose.
    pub fn json_mode(&self) -> bool {
        self.json
    }

    /// A one-shot structured result for an informational command (image
    /// contents, partition table, device identity). Only meaningful in JSON
    /// mode — in prose mode the command prints its own human output instead.
    pub fn data(&self, event: &str, mut value: Value) {
        if !self.json {
            return;
        }
        if let Some(obj) = value.as_object_mut() {
            obj.insert("event".to_string(), json!(event));
        }
        self.emit(value);
    }

    /// Human-only prose. Emits nothing in JSON mode — use it for wording that
    /// merely restates a structured event a front-end has already received.
    pub fn prose(&self, message: impl AsRef<str>) {
        if self.json {
            return;
        }
        self.clear_line();
        println!("{}", message.as_ref());
    }

    /// Free-form informational output.
    pub fn log(&self, message: impl AsRef<str>) {
        let message = message.as_ref();
        if self.json {
            self.emit(json!({ "event": "log", "message": message }));
        } else {
            self.clear_line();
            println!("{message}");
        }
    }

    /// A named milestone in a multi-step operation. `id` is the stable name a
    /// front-end can switch on; `message` is the prose for a human.
    pub fn step(&self, id: &str, message: impl AsRef<str>) {
        let message = message.as_ref();
        if self.json {
            self.emit(json!({ "event": "step", "step": id, "message": message }));
        } else {
            self.clear_line();
            println!("{message}");
        }
    }

    /// What a `flash-all` run is about to do, announced before the device is
    /// touched, so a front-end can size its progress bar up front.
    pub fn plan(
        &self,
        names: &[&str],
        total_bytes: u64,
        mbr_bytes: usize,
        boot1_bytes: usize,
        boot0_bytes: usize,
        partial: bool,
    ) {
        if self.json {
            self.emit(json!({
                "event": "plan",
                "partitions": names.len(),
                "names": names,
                "total_bytes": total_bytes,
                "mbr_bytes": mbr_bytes,
                "boot1_bytes": boot1_bytes,
                "boot0_bytes": boot0_bytes,
                "partial": partial,
            }));
        } else {
            self.clear_line();
            println!(
                "plan: MBR({mbr_bytes} B) + {} partitions ({} MB total) \
                 + BOOT1({boot1_bytes} B) + BOOT0({boot0_bytes} B)",
                names.len(),
                total_bytes / (1024 * 1024)
            );
            if partial {
                println!("      선택된 파티션만 기록: {}", names.join(", "));
            }
        }
    }

    pub fn partition_begin(&self, index: usize, total: usize, name: &str, bytes: u64) {
        self.last_progress.set(None);
        if self.json {
            self.emit(json!({
                "event": "partition_begin",
                "index": index,
                "total": total,
                "name": name,
                "bytes": bytes,
            }));
        }
        // Human mode says nothing here: the completion line below carries
        // everything, and the progress line already names the partition.
    }

    /// JSON only — the caller prints its own prose, since the wording differs
    /// between `flash-all` (a numbered step) and `flash-partition` (a summary).
    pub fn partition_end(
        &self,
        index: usize,
        total: usize,
        name: &str,
        bytes: u64,
        format: &str,
        seconds: f64,
    ) {
        if self.json {
            self.emit(json!({
                "event": "partition_end",
                "index": index,
                "total": total,
                "name": name,
                "bytes": bytes,
                "format": format,
                "seconds": seconds,
            }));
        }
    }

    /// Byte-level progress within the partition currently being written.
    /// Throttled to [`PROGRESS_INTERVAL`]; call [`Reporter::progress_done`] to
    /// emit the final point unconditionally.
    pub fn progress(&self, name: &str, written: u64, total: u64) {
        let now = Instant::now();
        if let Some(last) = self.last_progress.get() {
            if now.duration_since(last) < PROGRESS_INTERVAL {
                return;
            }
        }
        self.last_progress.set(Some(now));
        self.write_progress(name, written, total);
    }

    /// Emit the 100% point, bypassing the throttle.
    pub fn progress_done(&self, name: &str, total: u64) {
        self.write_progress(name, total, total);
    }

    fn write_progress(&self, name: &str, written: u64, total: u64) {
        if self.json {
            self.emit(json!({
                "event": "progress",
                "name": name,
                "written": written,
                "total": total,
            }));
            return;
        }
        // Prose mode: only worth drawing on a terminal. Redirected to a file
        // this would be thousands of near-identical lines.
        let mut stdout = std::io::stdout();
        if !stdout.is_terminal() {
            return;
        }
        let pct = if total == 0 {
            100
        } else {
            written * 100 / total
        };
        let _ = write!(
            stdout,
            "\r    {name}: {pct}% ({} / {} MB)   ",
            written / (1024 * 1024),
            total / (1024 * 1024)
        );
        let _ = stdout.flush();
        self.line_pending.set(true);
    }

    /// The command finished successfully.
    pub fn done(&self) {
        if self.json {
            self.emit(json!({ "event": "done" }));
        } else {
            self.clear_line();
        }
    }

    /// The command failed. `error` is reported with its full anyhow context
    /// chain, which is where the actionable detail usually lives.
    pub fn fail(&self, error: &anyhow::Error) {
        if self.json {
            let causes: Vec<String> = error.chain().map(|c| c.to_string()).collect();
            self.emit(json!({
                "event": "error",
                "message": format!("{error:#}"),
                "causes": causes,
            }));
        } else {
            self.clear_line();
            eprintln!("Error: {error:#}");
        }
    }
}
