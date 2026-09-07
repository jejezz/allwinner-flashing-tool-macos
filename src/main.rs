mod bootstrap;
mod efex;
mod event;
mod fel;
mod imagewty;
mod sparse;
mod sunxi_head;
mod sys_partition;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use event::Reporter;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "aw-tool", about = "Allwinner T507/T527 FEL/EFEX flashing helper")]
struct Cli {
    /// Emit newline-delimited JSON events on stdout instead of prose, one
    /// object per line, each tagged with an `event` key. Intended for a GUI
    /// front-end driving this tool as a subprocess.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List all items in an IMAGEWTY firmware image.
    List { image: PathBuf },

    /// Extract a named item (e.g. "u-boot.fex", "sys_partition.fex") from an image.
    Extract {
        image: PathBuf,
        /// Exact filename as it appears in the item table.
        item: String,
        #[arg(short, long)]
        out: PathBuf,
    },

    /// Patch work_mode in a u-boot/boot0 payload and recompute its checksum.
    PatchWorkmode {
        input: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
        /// work_mode value: boot | usb-product | card-product | usb-debug | <hex>
        #[arg(short, long, default_value = "usb-product")]
        mode: String,
    },

    /// Report what is connected right now — `fel`, `efex`, or `none` — in one
    /// shot, without touching storage. Safe to call repeatedly; a front-end
    /// polls this to know when a board has been plugged in and which mode it
    /// came up in. Exit status is 0 even when nothing is connected.
    Probe,

    /// Query FEL version from a connected device (VID:PID 1f3a:efe8).
    FelVersion,

    /// Query EFEX device identity/mode (safe, read-only; use after the
    /// device has reached "run usb efex" on the patched-uboot primer).
    EfexVerifyDev,

    /// Ask an EFEX device which storage it booted from. Use this to pick the
    /// right BOOT0 variant (`boot0_sdcard.fex` vs `boot0_nand.fex`) when
    /// flashing a board you haven't characterised yet.
    EfexQueryStorage,

    /// Parse a sys_partition.fex file and print each partition's computed
    /// start sector (offline, no hardware) — verify against a real MBR dump
    /// before ever writing anything.
    ListPartitions { sys_partition_fex: PathBuf },

    /// DESTRUCTIVE: erase + write the GPT/MBR from `sunxi_mbr.fex` in the
    /// image. Run flash-set-on first is NOT needed here — this call does it
    /// internally. Requires the device already past `run usb efex`.
    FlashMbr { image: PathBuf },

    /// DESTRUCTIVE: write BOOT1 (boot_package.fex, ORIGINAL/unpatched copy —
    /// do not pass the FEL-primer file with work_mode patched to 0x10).
    FlashBoot1 { image: PathBuf },

    /// DESTRUCTIVE: write BOOT0. `--item` selects which IMAGEWTY item to use
    /// (e.g. "boot0_sdcard.fex" or "boot0_nand.fex" — match the device's
    /// actual storage type, see `efex-query-storage`).
    FlashBoot0 {
        image: PathBuf,
        #[arg(long, default_value = "boot0_sdcard.fex")]
        item: String,
    },

    /// DESTRUCTIVE: write one partition's data at its computed sector offset.
    /// Looks up `name` in sys_partition_fex for the offset, then extracts
    /// that partition's `downloadfile` from the image (or `--item` to
    /// override) and writes it via fes_down.
    FlashPartition {
        image: PathBuf,
        sys_partition_fex: PathBuf,
        name: String,
        #[arg(long)]
        item: Option<String>,
    },

    /// Take a board from FEL mode into EFEX, staging the fes1 and u-boot
    /// primers straight out of the image (no external sunxi-fel needed).
    /// A no-op if the device is already in EFEX. `flash-all` does this on its
    /// own; use this when you just want the device in EFEX.
    Bootstrap {
        image: PathBuf,
        /// fes1 load/run address (default: sun55iw3/T527).
        #[arg(long, default_value = "0x4c000")]
        fes1_addr: String,
        /// u-boot load/run address (default: sun55iw3/T527).
        #[arg(long, default_value = "0x4a000000")]
        uboot_addr: String,
    },

    /// Set the erase flag (Format Fusing vs Overwrite-only — see docs
    /// section 12). Pass 1 for a full format, 0 for overwrite-only.
    FlashSetEraseFlag { flag: u32 },


    /// DESTRUCTIVE: flash a whole board. Bootstraps FEL→EFEX if needed, then
    /// runs the full fusing sequence — erase_flag, MBR, every partition with a
    /// downloadfile in sys_partition.fex (sparse images are expanded), BOOT1,
    /// BOOT0 — and optionally reboots. This is the main command.
    FlashAll {
        image: PathBuf,
        sys_partition_fex: PathBuf,
        /// 1 = full format, 0 = overwrite-only (docs section 12).
        #[arg(long, default_value = "1")]
        erase_flag: u32,
        /// Skip any partition whose downloadfile is larger than this many
        /// bytes (use to exclude e.g. super.fex/vmlinux.fex for a faster
        /// partial run). 0 = no limit (write everything).
        #[arg(long, default_value = "0")]
        skip_larger_than: u64,
        #[arg(long, default_value = "boot0_sdcard.fex")]
        boot0_item: String,
        /// Reboot the device once everything is written.
        #[arg(long, default_value = "true")]
        reboot: bool,
        /// Write ONLY these partitions (comma-separated names from
        /// sys_partition.fex). Requires `--erase-flag 0`: with a full format
        /// every partition is erased first, so one left out would be blank
        /// rather than preserved.
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,

        /// Write everything EXCEPT these partitions — the way to keep a
        /// partition the board has modified (`env`, `userdata`) while
        /// updating the rest. Same `--erase-flag 0` requirement as `--only`.
        #[arg(long, value_delimiter = ',')]
        skip: Vec<String>,

        /// Skip the automatic FEL→EFEX bootstrap and require the device to
        /// already be in EFEX.
        #[arg(long)]
        no_bootstrap: bool,
    },
}

/// Which partitions a run should write.
enum Selection {
    All,
    Only(Vec<String>),
    Except(Vec<String>),
}

impl Selection {
    /// Resolve `--only` / `--skip` against the erase flag and the partition
    /// table.
    ///
    /// Unknown names are an error rather than a silent no-op: a typo in
    /// `--skip env` would otherwise quietly overwrite the very partition the
    /// caller was trying to protect.
    fn resolve(
        only: Vec<String>,
        skip: Vec<String>,
        erase_flag: u32,
        sp: &sys_partition::SysPartition,
    ) -> Result<Self> {
        if only.is_empty() && skip.is_empty() {
            return Ok(Selection::All);
        }
        if !only.is_empty() && !skip.is_empty() {
            bail!("--only and --skip cannot be combined");
        }
        if erase_flag != 0 {
            bail!(
                "--only/--skip need `--erase-flag 0`.\n\
                 A full format (erase_flag=1) erases every partition before writing, so a \
                 partition left out would end up blank, not preserved."
            );
        }

        let names = if only.is_empty() { &skip } else { &only };
        for name in names {
            if sp.find(name).is_none() {
                bail!(
                    "partition '{name}' is not in sys_partition.fex (known: {})",
                    sp.partitions
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }

        Ok(if only.is_empty() {
            Selection::Except(skip)
        } else {
            Selection::Only(only)
        })
    }

    fn includes(&self, name: &str) -> bool {
        match self {
            Selection::All => true,
            Selection::Only(names) => names.iter().any(|n| n == name),
            Selection::Except(names) => !names.iter().any(|n| n == name),
        }
    }

    fn is_partial(&self) -> bool {
        !matches!(self, Selection::All)
    }
}

/// Which path `write_partition_auto` took, for reporting.
struct WriteOutcome {
    /// Stable machine-readable tag: "raw" or "sparse".
    format: &'static str,
    /// Prose for a human ("raw", or the sparse expansion summary).
    detail: String,
}

/// Write a partition payload, expanding it first if it is an Android sparse
/// image (`super.fex` is one).
///
/// Progress is reported per 64 KB chunk against the number of bytes actually
/// sent, which for a sparse image is the payload size — not the file size and
/// not the expanded size, since DONT_CARE regions are skipped entirely.
fn write_partition_auto(
    dev: &mut efex::EfexDevice,
    start_sector: u64,
    data: &[u8],
    name: &str,
    rep: &Reporter,
) -> Result<WriteOutcome> {
    if sparse::is_sparse(data) {
        let img = sparse::parse(data)?;
        let payload: u64 = img
            .segments
            .iter()
            .map(|s| match s {
                sparse::Segment::Raw { data, .. } => data.len() as u64,
                sparse::Segment::Fill { len, .. } => *len,
            })
            .sum();
        let mut written = 0u64;
        dev.write_partition_sparse(start_sector, &img, &mut |n| {
            written += n as u64;
            rep.progress(name, written, payload);
        })?;
        rep.progress_done(name, payload);
        Ok(WriteOutcome {
            format: "sparse",
            detail: format!(
                "sparse -> {} MB expanded, {} MB written",
                img.expanded_len / (1024 * 1024),
                payload / (1024 * 1024)
            ),
        })
    } else {
        let total = data.len() as u64;
        let mut written = 0u64;
        dev.write_partition(start_sector, data, &mut |n| {
            written += n as u64;
            rep.progress(name, written, total);
        })?;
        rep.progress_done(name, total);
        Ok(WriteOutcome {
            format: "raw",
            detail: "raw".to_string(),
        })
    }
}

fn parse_hex_addr(s: &str) -> Result<u32> {
    let trimmed = s.trim_start_matches("0x");
    u32::from_str_radix(trimmed, 16).with_context(|| format!("invalid hex address '{s}'"))
}

fn parse_work_mode(s: &str) -> Result<u32> {
    Ok(match s {
        "boot" => sunxi_head::WORK_MODE_BOOT,
        "usb-product" => sunxi_head::WORK_MODE_USB_PRODUCT,
        "card-product" => sunxi_head::WORK_MODE_CARD_PRODUCT,
        "usb-debug" => sunxi_head::WORK_MODE_USB_DEBUG,
        other => {
            let trimmed = other.trim_start_matches("0x");
            u32::from_str_radix(trimmed, 16)
                .with_context(|| format!("invalid work_mode '{other}'"))?
        }
    })
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let rep = Reporter::new(cli.json);
    match run(cli.command, &rep) {
        Ok(()) => {
            rep.done();
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            // Reported as a terminating event rather than propagated, so a
            // front-end reading the JSON stream always sees a final `error`
            // object instead of having to also parse stderr.
            rep.fail(&e);
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(command: Command, rep: &Reporter) -> Result<()> {
    match command {
        Command::List { image } => {
            let img = imagewty::ImageWty::open(&image)?;
            if rep.json_mode() {
                let items: Vec<_> = img
                    .items
                    .iter()
                    .map(|it| {
                        serde_json::json!({
                            "maintype": it.maintype,
                            "subtype": it.subtype,
                            "filename": it.filename,
                            "file_offset": it.file_offset,
                            "stored_length": it.stored_length,
                            "original_length": it.original_length,
                        })
                    })
                    .collect();
                rep.data("items", serde_json::json!({ "items": items }));
            } else {
                println!("{} items:", img.items.len());
                for it in &img.items {
                    println!(
                        "  {:8} {:18} {:30} offset={:<10} stored={:<10} original={}",
                        it.maintype, it.subtype, it.filename, it.file_offset, it.stored_length, it.original_length
                    );
                }
            }
        }

        Command::Extract { image, item, out } => {
            let img = imagewty::ImageWty::open(&image)?;
            let found = img
                .find(&item)
                .with_context(|| format!("item '{item}' not found in {:?}", image))?;
            let data = img.read_item(found)?;
            std::fs::write(&out, &data).with_context(|| format!("writing {:?}", out))?;
            rep.log(format!("extracted {} bytes -> {:?}", data.len(), out));
        }

        Command::PatchWorkmode { input, out, mode } => {
            let mode_val = parse_work_mode(&mode)?;
            let mut buf = std::fs::read(&input).with_context(|| format!("reading {:?}", input))?;

            let before = sunxi_head::read_work_mode(&buf)?;
            let before_sum = sunxi_head::read_checksum(&buf)?;

            sunxi_head::set_work_mode(&mut buf, mode_val)?;
            let after_sum = sunxi_head::fix_checksum(&mut buf)?;

            std::fs::write(&out, &buf).with_context(|| format!("writing {:?}", out))?;
            rep.log(format!(
                "work_mode: 0x{before:02x} -> 0x{mode_val:02x}\ncheck_sum: 0x{before_sum:08x} -> 0x{after_sum:08x}\nwrote {:?}",
                out
            ));
        }

        Command::Probe => {
            // Deliberately never an error: "nothing connected" is a normal
            // state for a front-end that polls while waiting for the user to
            // plug a board in.
            if bootstrap::in_efex_mode() {
                let mut dev = efex::EfexDevice::open()?;
                let info = dev.verify_dev()?;
                if rep.json_mode() {
                    rep.data(
                        "probe",
                        serde_json::json!({
                            "state": "efex",
                            "mode": info.mode,
                            "platform_id_hw": info.platform_id_hw,
                            "platform_id_fw": info.platform_id_fw,
                        }),
                    );
                } else {
                    println!("efex (mode=0x{:02x})", info.mode);
                }
            } else if let Ok(dev) = fel::FelDevice::open() {
                let ver = dev.get_version()?;
                if rep.json_mode() {
                    rep.data(
                        "probe",
                        serde_json::json!({
                            "state": "fel",
                            "soc_id": ver.soc_id,
                            "protocol": ver.protocol,
                        }),
                    );
                } else {
                    println!("fel (soc_id=0x{:04x})", ver.soc_id);
                }
            } else if rep.json_mode() {
                rep.data("probe", serde_json::json!({ "state": "none" }));
            } else {
                println!("none");
            }
        }

        Command::FelVersion => {
            let dev = fel::FelDevice::open()?;
            let ver = dev.get_version()?;
            if rep.json_mode() {
                rep.data(
                    "fel_version",
                    serde_json::json!({
                        "signature": String::from_utf8_lossy(&ver.signature),
                        "soc_id": ver.soc_id,
                        "protocol": ver.protocol,
                        "scratchpad": ver.scratchpad,
                    }),
                );
            } else {
                println!(
                    "signature={:?} soc_id=0x{:04x} protocol=0x{:04x} scratchpad=0x{:x}",
                    String::from_utf8_lossy(&ver.signature),
                    ver.soc_id,
                    ver.protocol,
                    ver.scratchpad
                );
            }
        }

        Command::EfexVerifyDev => {
            let mut dev = efex::EfexDevice::open()?;
            let info = dev.verify_dev()?;
            if rep.json_mode() {
                rep.data(
                    "verify_dev",
                    serde_json::json!({
                        "tag": String::from_utf8_lossy(&info.tag),
                        "platform_id_hw": info.platform_id_hw,
                        "platform_id_fw": info.platform_id_fw,
                        "mode": info.mode,
                    }),
                );
            } else {
                println!(
                    "tag={:?} platform_id_hw=0x{:08x} platform_id_fw=0x{:08x} mode=0x{:02x}",
                    String::from_utf8_lossy(&info.tag),
                    info.platform_id_hw,
                    info.platform_id_fw,
                    info.mode
                );
            }
        }

        Command::EfexQueryStorage => {
            let mut dev = efex::EfexDevice::open()?;
            let storage_type = dev.query_storage()?;
            if rep.json_mode() {
                rep.data(
                    "storage",
                    serde_json::json!({ "storage_type": storage_type }),
                );
            } else {
                println!("storage_type = {}", storage_type);
            }
        }

        Command::ListPartitions { sys_partition_fex } => {
            let text = std::fs::read_to_string(&sys_partition_fex)
                .with_context(|| format!("reading {:?}", sys_partition_fex))?;
            let sp = sys_partition::SysPartition::parse(&text)?;
            if rep.json_mode() {
                let parts: Vec<_> = sp
                    .partitions
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "name": p.name,
                            "start_sector": p.start_sector,
                            "size_sectors": p.size_sectors,
                            "downloadfile": p.downloadfile,
                            "user_type": p.user_type,
                            "keydata": p.keydata,
                            "ro": p.ro,
                        })
                    })
                    .collect();
                rep.data(
                    "partitions",
                    serde_json::json!({
                        "mbr_size_sectors": sp.mbr_size_sectors,
                        "partitions": parts,
                    }),
                );
            } else {
                println!(
                    "mbr_size_sectors=0x{:x} ({} bytes)",
                    sp.mbr_size_sectors,
                    sp.mbr_size_sectors * sys_partition::SECTOR_SIZE
                );
                for p in &sp.partitions {
                    let size_str = match p.size_sectors {
                        Some(s) => format!("0x{:x}", s),
                        None => "(fills remainder)".to_string(),
                    };
                    println!(
                        "  {:20} start=0x{:<10x} size={:<14} downloadfile={:<20} user_type={} keydata={} ro={}",
                        p.name,
                        p.start_sector,
                        size_str,
                        p.downloadfile.as_deref().unwrap_or("-"),
                        p.user_type,
                        p.keydata,
                        p.ro
                    );
                }
            }
        }

        Command::FlashMbr { image } => {
            let img = imagewty::ImageWty::open(&image)?;
            let item = img
                .find("sunxi_mbr.fex")
                .context("sunxi_mbr.fex not found in image")?;
            let data = img.read_item(item)?;
            let mut dev = efex::EfexDevice::open()?;
            dev.flash_set_on()?;
            dev.write_mbr(&data)?;
            rep.step("mbr", format!("wrote MBR ({} bytes) from sunxi_mbr.fex", data.len()));
        }

        Command::FlashBoot1 { image } => {
            let img = imagewty::ImageWty::open(&image)?;
            let item = img
                .find("boot_package.fex")
                .context("boot_package.fex not found in image")?;
            let data = img.read_item(item)?;
            let mut dev = efex::EfexDevice::open()?;
            dev.flash_set_on()?;
            dev.write_boot1(&data)?;
            rep.step(
                "boot1",
                format!("wrote BOOT1 ({} bytes) from boot_package.fex", data.len()),
            );
        }

        Command::FlashBoot0 { image, item } => {
            let img = imagewty::ImageWty::open(&image)?;
            let found = img
                .find(&item)
                .with_context(|| format!("{item} not found in image"))?;
            let data = img.read_item(found)?;
            let mut dev = efex::EfexDevice::open()?;
            dev.flash_set_on()?;
            dev.write_boot0(&data)?;
            rep.step("boot0", format!("wrote BOOT0 ({} bytes) from {item}", data.len()));
        }

        Command::FlashPartition {
            image,
            sys_partition_fex,
            name,
            item,
        } => {
            let sp_text = std::fs::read_to_string(&sys_partition_fex)
                .with_context(|| format!("reading {:?}", sys_partition_fex))?;
            let sp = sys_partition::SysPartition::parse(&sp_text)?;
            let part = sp
                .find(&name)
                .with_context(|| format!("partition '{name}' not found in sys_partition.fex"))?;

            let filename = item
                .or_else(|| part.downloadfile.clone())
                .with_context(|| format!("partition '{name}' has no downloadfile and no --item given"))?;

            let img = imagewty::ImageWty::open(&image)?;
            let found = img
                .find(&filename)
                .with_context(|| format!("{filename} not found in image"))?;
            let data = img.read_item(found)?;

            let mut dev = efex::EfexDevice::open()?;
            dev.flash_set_on()?;
            let start = std::time::Instant::now();
            rep.partition_begin(1, 1, &name, data.len() as u64);
            let outcome = write_partition_auto(&mut dev, part.start_sector, &data, &name, rep)?;
            let elapsed = start.elapsed();
            let mb = data.len() as f64 / (1024.0 * 1024.0);
            rep.partition_end(
                1,
                1,
                &name,
                data.len() as u64,
                outcome.format,
                elapsed.as_secs_f64(),
            );
            rep.prose(format!(
                "wrote partition '{name}' ({} bytes from {filename}, {}) at sector 0x{:x} in {:.1}s ({:.2} MB/s)",
                data.len(),
                outcome.detail,
                part.start_sector,
                elapsed.as_secs_f64(),
                mb / elapsed.as_secs_f64()
            ));
        }

        Command::Bootstrap {
            image,
            fes1_addr,
            uboot_addr,
        } => {
            let img = imagewty::ImageWty::open(&image)?;
            bootstrap::fel_to_efex(
                &img,
                parse_hex_addr(&fes1_addr)?,
                parse_hex_addr(&uboot_addr)?,
                rep,
            )?;
        }

        Command::FlashSetEraseFlag { flag } => {
            let mut dev = efex::EfexDevice::open()?;
            dev.flash_set_on()?;
            dev.set_erase_flag(flag)?;
            rep.step("erase_flag", format!("erase flag set to {flag}"));
        }

        Command::FlashAll {
            image,
            sys_partition_fex,
            erase_flag,
            skip_larger_than,
            boot0_item,
            reboot,
            only,
            skip,
            no_bootstrap,
        } => {
            let sp_text = std::fs::read_to_string(&sys_partition_fex)
                .with_context(|| format!("reading {:?}", sys_partition_fex))?;
            let sp = sys_partition::SysPartition::parse(&sp_text)?;
            let selection = Selection::resolve(only, skip, erase_flag, &sp)?;
            let img = imagewty::ImageWty::open(&image)?;

            let mbr_item = img.find("sunxi_mbr.fex").context("sunxi_mbr.fex not found")?;
            let mbr_data = img.read_item(mbr_item)?;
            let boot1_item = img.find("boot_package.fex").context("boot_package.fex not found")?;
            let boot1_data = img.read_item(boot1_item)?;
            let boot0_item_ref = img
                .find(&boot0_item)
                .with_context(|| format!("{boot0_item} not found"))?;
            let boot0_data = img.read_item(boot0_item_ref)?;

            // Resolve all partition payloads up front so we fail fast (missing
            // file, bad sys_partition.fex) before touching the device at all.
            struct Job {
                name: String,
                start_sector: u64,
                data: Vec<u8>,
            }
            let mut jobs = Vec::new();
            for p in &sp.partitions {
                let Some(filename) = &p.downloadfile else { continue };
                if !selection.includes(&p.name) {
                    rep.log(format!("keeping '{}' (not selected)", p.name));
                    continue;
                }
                let item = img
                    .find(filename)
                    .with_context(|| format!("{filename} (for partition '{}') not found in image", p.name))?;
                if skip_larger_than > 0 && item.original_length as u64 > skip_larger_than {
                    rep.log(format!(
                        "skipping '{}' ({filename}, {} bytes > limit)",
                        p.name, item.original_length
                    ));
                    continue;
                }
                let data = img.read_item(item)?;
                jobs.push(Job {
                    name: p.name.clone(),
                    start_sector: p.start_sector,
                    data,
                });
            }

            if jobs.is_empty() {
                bail!("nothing selected to write");
            }

            let total_bytes: u64 = jobs.iter().map(|j| j.data.len() as u64).sum();
            let names: Vec<&str> = jobs.iter().map(|j| j.name.as_str()).collect();
            rep.plan(
                &names,
                total_bytes,
                mbr_data.len(),
                boot1_data.len(),
                boot0_data.len(),
                selection.is_partial(),
            );

            if !no_bootstrap {
                bootstrap::fel_to_efex(&img, bootstrap::FES1_ADDR, bootstrap::UBOOT_ADDR, rep)?;
            }

            let mut dev = efex::EfexDevice::open()?;
            dev.flash_set_on()?;
            rep.step("flash_set_on", "[1] flash_set_on OK");

            dev.set_erase_flag(erase_flag)?;
            rep.step("erase_flag", format!("[2] erase_flag={erase_flag} OK"));

            dev.write_mbr(&mbr_data)?;
            rep.step("mbr", "[3] MBR OK");

            for (i, job) in jobs.iter().enumerate() {
                let index = i + 1;
                let bytes = job.data.len() as u64;
                rep.partition_begin(index, jobs.len(), &job.name, bytes);
                let started = std::time::Instant::now();
                let outcome =
                    write_partition_auto(&mut dev, job.start_sector, &job.data, &job.name, rep)
                        .with_context(|| {
                            format!(
                                "writing partition '{}' at sector 0x{:x}",
                                job.name, job.start_sector
                            )
                        })?;
                rep.partition_end(
                    index,
                    jobs.len(),
                    &job.name,
                    bytes,
                    outcome.format,
                    started.elapsed().as_secs_f64(),
                );
                rep.prose(format!(
                    "[4.{index}/{}] partition '{}' OK ({bytes} bytes, {})",
                    jobs.len(),
                    job.name,
                    outcome.detail
                ));
            }

            dev.write_boot1(&boot1_data)?;
            rep.step("boot1", "[5] BOOT1 OK");

            dev.write_boot0(&boot0_data)?;
            rep.step("boot0", "[6] BOOT0 OK");

            dev.flash_set_off()?;
            rep.step("flash_set_off", "[7] flash_set_off OK");

            if reboot {
                dev.trigger_reboot()?;
                rep.step("reboot", "[8] reboot triggered");
            } else {
                rep.step("reboot_skipped", "[8] reboot skipped (--reboot not given)");
            }
        }
    }

    Ok(())
}
