mod bootstrap;
mod efex;
mod event;
mod fel;
mod imagewty;
mod sdcard;
mod sparse;
mod sunxi_head;
mod sys_partition;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use event::Reporter;
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "aw-tool",
    version = env!("AW_VERSION"),
    about = "Allwinner T507/T527 FEL/EFEX flashing helper"
)]
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
        /// fes1 load/run address. Default: auto-detected from the device's
        /// soc_id (0x4c000 for sun55iw3/T527/A523, 0x28000 for
        /// sun50iw9/T507/H616).
        #[arg(long)]
        fes1_addr: Option<String>,
        /// u-boot load/run address (default: 0x4a000000, shared by all
        /// known SoCs).
        #[arg(long)]
        uboot_addr: Option<String>,
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

    /// List disks that could be a card-fusing target (safe, read-only). Each
    /// reports its size, whether it is removable media, and whether it is the
    /// disk the running OS lives on.
    ListDisks,

    /// DESTRUCTIVE: card fusing. Write boot0 (work_mode patched to
    /// CARD_PRODUCT), boot_package (twice) and the whole firmware image to an
    /// SD card at fixed sectors (16 / 32800 / 24576 / 40960); the board then
    /// installs itself from the card at power-on. Everything on the card is
    /// lost. Raw disk access needs administrator rights (Windows) or sudo
    /// (macOS). The system disk is always refused.
    FlashSdcard {
        image: PathBuf,
        /// Target disk exactly as printed by `list-disks`
        /// (Windows `\\.\PhysicalDriveN`, macOS `/dev/diskN`).
        disk: String,
        #[arg(long, default_value = "boot0_sdcard.fex")]
        boot0_item: String,
        #[arg(long, default_value = "boot_package.fex")]
        boot_package_item: String,
        /// Allow a disk that is not reported as removable media or exceeds
        /// --max-size-bytes. Never lifts the system-disk or too-small refusals.
        #[arg(long)]
        force: bool,
        /// Refuse disks larger than this many bytes (default 256 GiB;
        /// 0 = no limit).
        #[arg(long, default_value_t = sdcard::DEFAULT_MAX_SIZE_BYTES)]
        max_size_bytes: u64,
        /// Skip reading the card back after writing. The read-back is what
        /// catches a card or reader that silently drops writes, so leave it on
        /// unless time matters more.
        #[arg(long)]
        no_verify: bool,
    },

    /// Check an SD card without writing to it: read it back (bypassing the OS
    /// cache) and judge the boot chain the way the board's BootROM and u-boot
    /// would — boot0 and boot_package headers and checksums, an IMAGEWTY
    /// firmware. With `--image`, also require the card to hold exactly that
    /// image. Needs no board. Raw disk access needs administrator rights
    /// (Windows asks) or sudo (macOS).
    VerifySdcard {
        /// Disk exactly as printed by `list-disks`.
        disk: String,
        /// The image the card should have been written from.
        #[arg(long)]
        image: Option<PathBuf>,
        #[arg(long, default_value = "boot0_sdcard.fex")]
        boot0_item: String,
        #[arg(long, default_value = "boot_package.fex")]
        boot_package_item: String,
    },

    /// Internal: the elevated half of `flash-sdcard` on Windows. Started by
    /// `flash-sdcard` itself through a UAC prompt; not meant to be run by hand.
    #[command(hide = true)]
    FlashSdcardWorker {
        #[arg(long)]
        port: u16,
        #[arg(long)]
        token: String,
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

/// Write a partition, expanding it first if it is an Android sparse image
/// (`super.fex` is one). Streams straight out of the image file — `item`
/// is read on demand rather than already being in memory, so this can be
/// called right after resolving `item` from the item table, with no
/// separate "load the whole image" pass first.
///
/// Progress is reported per 64 KB chunk against the number of bytes actually
/// sent, which for a sparse image is the payload size — not the file size and
/// not the expanded size, since DONT_CARE regions are skipped entirely.
fn write_partition_auto(
    dev: &mut efex::EfexDevice,
    image: &imagewty::ImageWty,
    item: &imagewty::Item,
    start_sector: u64,
    name: &str,
    rep: &Reporter,
) -> Result<WriteOutcome> {
    // Sparse images start with a 4-byte magic; a genuinely 4-byte-or-smaller
    // partition can't be one (the real header is 28 bytes), so there's
    // nothing to probe for those.
    let is_sparse = item.original_length >= 4 && {
        let mut magic = [0u8; 4];
        image.open_item(item)?.read_exact(&mut magic)?;
        sparse::is_sparse(&magic)
    };

    if is_sparse {
        let mut source = image.open_item(item)?;
        let img = sparse::parse_streaming(&mut source)?;
        let payload: u64 = img
            .segments
            .iter()
            .map(|s| match s {
                sparse::Segment::Raw { len, .. } => *len,
                sparse::Segment::Fill { len, .. } => *len,
            })
            .sum();
        let mut written = 0u64;
        dev.write_partition_sparse(start_sector, &mut source, &img, &mut |n| {
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
        let mut source = image.open_item(item)?;
        let total = item.original_length as u64;
        let mut written = 0u64;
        dev.write_partition_stream(start_sector, &mut source, total, &mut |n| {
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
            let bytes = found.original_length as u64;

            let mut dev = efex::EfexDevice::open()?;
            dev.flash_set_on()?;
            let start = std::time::Instant::now();
            rep.partition_begin(1, 1, &name, bytes);
            let outcome =
                write_partition_auto(&mut dev, &img, found, part.start_sector, &name, rep)?;
            let elapsed = start.elapsed();
            let mb = bytes as f64 / (1024.0 * 1024.0);
            rep.partition_end(1, 1, &name, bytes, outcome.format, elapsed.as_secs_f64());
            rep.prose(format!(
                "wrote partition '{name}' ({bytes} bytes from {filename}, {}) at sector 0x{:x} in {:.1}s ({:.2} MB/s)",
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
                fes1_addr.map(|s| parse_hex_addr(&s)).transpose()?,
                uboot_addr.map(|s| parse_hex_addr(&s)).transpose()?,
                rep,
            )?;
        }

        Command::FlashSetEraseFlag { flag } => {
            let mut dev = efex::EfexDevice::open()?;
            dev.flash_set_on()?;
            dev.set_erase_flag(flag)?;
            rep.step("erase_flag", format!("erase flag set to {flag}"));
        }

        Command::ListDisks => {
            let disks = sdcard::enumerate_disks()?;
            if rep.json_mode() {
                let list: Vec<_> = disks.iter().map(|d| d.to_json()).collect();
                rep.data("disks", serde_json::json!({ "disks": list }));
            } else if disks.is_empty() {
                println!("no disks found");
            } else {
                for d in &disks {
                    println!(
                        "{}  {:>8.1} GB  removable={}  system={}  {}",
                        d.id,
                        d.size_bytes as f64 / 1e9,
                        d.removable,
                        d.is_system_disk,
                        d.display_name
                    );
                }
            }
        }

        Command::FlashSdcardWorker { port, token } => {
            // Its result goes back over the socket; there is no console to
            // print to, so the error is deliberately not returned.
            let _ = sdcard::run_worker(port, &token);
        }

        Command::FlashSdcard {
            image,
            disk,
            boot0_item,
            boot_package_item,
            force,
            max_size_bytes,
            no_verify,
        } => {
            let img = imagewty::ImageWty::open(&image)?;
            let boot0_raw = img.read_item(
                img.find(&boot0_item)
                    .with_context(|| format!("{boot0_item} not found in image"))?,
            )?;
            let boot_package_raw = img.read_item(
                img.find(&boot_package_item)
                    .with_context(|| format!("{boot_package_item} not found in image"))?,
            )?;
            // What PhoenixCard writes: boot0 and the partition table as the image
            // has them, the boot_package with the card work mode in its main slot.
            let mbr = img.read_item(
                img.find("sunxi_mbr.fex")
                    .context("sunxi_mbr.fex not found in image")?,
            )?;
            let blobs = sdcard::Blobs {
                boot0: sdcard::card_boot0(&boot0_raw)?,
                boot_package: boot_package_raw,
                mbr,
            };
            let firmware_len = std::fs::metadata(&image)
                .with_context(|| format!("reading {:?}", image))?
                .len();
            sdcard::validate_layout(
                blobs.boot0.len() as u64,
                blobs.boot_package.len() as u64,
                blobs.mbr.len() as u64,
            )?;
            // The board's BootROM and u-boot refuse a boot0 or boot_package that
            // fails these; better to learn it here than from a dead card.
            sdcard::check_card_boot0(&blobs.boot0)
                .context("the card's boot0 would not pass the BootROM checks")?;
            sdcard::check_toc1(&blobs.boot_package)
                .context("this image's boot_package would not pass the u-boot checks")?;
            rep.step("image_ok", "boot0 and boot_package pass the boot-chain header checks");

            let disks = sdcard::enumerate_disks()?;
            let target = disks
                .iter()
                .find(|d| sdcard::same_id(&d.id, &disk))
                .with_context(|| format!("disk '{disk}' not found; run `aw-tool list-disks`"))?;
            sdcard::check_disk_safety(
                target,
                sdcard::required_bytes(firmware_len),
                max_size_bytes,
                force,
            )?;
            rep.step(
                "safety_check_passed",
                format!(
                    "{} ({}, {} GB) passed the safety checks",
                    target.id,
                    target.display_name,
                    target.size_bytes / 1_000_000_000
                ),
            );

            sdcard::flash(
                target,
                &blobs,
                &image,
                firmware_len,
                &sdcard::FlashParams {
                    force,
                    max_size_bytes,
                    verify: !no_verify,
                },
                rep,
            )?;
            rep.step(
                "card_written",
                if no_verify {
                    "card written, not verified — insert it into the board and power on (no USB needed)"
                } else {
                    "card written and verified — insert it into the board and power on (no USB needed)"
                },
            );
        }

        Command::VerifySdcard {
            disk,
            image,
            boot0_item,
            boot_package_item,
        } => {
            let disks = sdcard::enumerate_disks()?;
            let target = disks
                .iter()
                .find(|d| sdcard::same_id(&d.id, &disk))
                .with_context(|| format!("disk '{disk}' not found; run `aw-tool list-disks`"))?;
            match image {
                Some(image) => {
                    let img = imagewty::ImageWty::open(&image)?;
                    let boot0_raw = img.read_item(
                        img.find(&boot0_item)
                            .with_context(|| format!("{boot0_item} not found in image"))?,
                    )?;
                    let boot_package = img.read_item(
                        img.find(&boot_package_item)
                            .with_context(|| format!("{boot_package_item} not found in image"))?,
                    )?;
                    let mbr = img.read_item(
                        img.find("sunxi_mbr.fex")
                            .context("sunxi_mbr.fex not found in image")?,
                    )?;
                    let blobs = sdcard::Blobs {
                        boot0: sdcard::card_boot0(&boot0_raw)?,
                        boot_package,
                        mbr,
                    };
                    let firmware_len = std::fs::metadata(&image)
                        .with_context(|| format!("reading {:?}", image))?
                        .len();
                    sdcard::verify_card(target, Some((&blobs, &image, firmware_len)), rep)?;
                }
                None => sdcard::verify_card(target, None, rep)?,
            }
            rep.step("card_verified", "the card checks out");
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
            let boot1_item = img.find("boot_package.fex").context("boot_package.fex not found")?;
            let boot0_item_ref = img
                .find(&boot0_item)
                .with_context(|| format!("{boot0_item} not found"))?;

            // Resolve every job up front so we fail fast (missing file, bad
            // sys_partition.fex) before touching the device at all — but only
            // the `Item` metadata (offset/length, already known from the item
            // table), not the payload bytes themselves. Those get streamed
            // straight off disk during the write loop below, so there's no
            // separate "load the image" pass (and no multi-GB buffer) at all.
            struct Job<'a> {
                name: String,
                start_sector: u64,
                item: &'a imagewty::Item,
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
                jobs.push(Job { name: p.name.clone(), start_sector: p.start_sector, item });
            }

            if jobs.is_empty() {
                bail!("nothing selected to write");
            }

            // MBR/BOOT1/BOOT0 are small (at most a few MB) next to the
            // partitions, so reading them fully up front is cheap and keeps
            // the write loop below simple.
            let mbr_data = img.read_item(mbr_item)?;
            let boot1_data = img.read_item(boot1_item)?;
            let boot0_data = img.read_item(boot0_item_ref)?;

            let total_bytes: u64 = jobs.iter().map(|j| j.item.original_length as u64).sum();
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
                bootstrap::fel_to_efex(&img, None, None, rep)?;
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
                let bytes = job.item.original_length as u64;
                rep.partition_begin(index, jobs.len(), &job.name, bytes);
                let started = std::time::Instant::now();
                let outcome = write_partition_auto(
                    &mut dev,
                    &img,
                    job.item,
                    job.start_sector,
                    &job.name,
                    rep,
                )
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
