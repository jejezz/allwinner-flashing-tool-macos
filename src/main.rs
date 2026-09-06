mod bootstrap;
mod efex;
mod fel;
mod imagewty;
mod sparse;
mod sunxi_head;
mod sys_partition;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "aw-tool", about = "Allwinner T507/T527 FEL/EFEX flashing helper")]
struct Cli {
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
        /// Skip the automatic FEL→EFEX bootstrap and require the device to
        /// already be in EFEX.
        #[arg(long)]
        no_bootstrap: bool,
    },
}

/// Write a partition payload, expanding it first if it is an Android sparse
/// image (`super.fex` is one). Returns a short description of which path was
/// taken, for the progress line.
fn write_partition_auto(
    dev: &mut efex::EfexDevice,
    start_sector: u64,
    data: &[u8],
) -> Result<String> {
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
        dev.write_partition_sparse(start_sector, &img)?;
        Ok(format!(
            "sparse -> {} MB expanded, {} MB written",
            img.expanded_len / (1024 * 1024),
            payload / (1024 * 1024)
        ))
    } else {
        dev.write_partition(start_sector, data)?;
        Ok("raw".to_string())
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::List { image } => {
            let img = imagewty::ImageWty::open(&image)?;
            println!("{} items:", img.items.len());
            for it in &img.items {
                println!(
                    "  {:8} {:18} {:30} offset={:<10} stored={:<10} original={}",
                    it.maintype, it.subtype, it.filename, it.file_offset, it.stored_length, it.original_length
                );
            }
        }

        Command::Extract { image, item, out } => {
            let img = imagewty::ImageWty::open(&image)?;
            let found = img
                .find(&item)
                .with_context(|| format!("item '{item}' not found in {:?}", image))?;
            let data = img.read_item(found)?;
            std::fs::write(&out, &data).with_context(|| format!("writing {:?}", out))?;
            println!("extracted {} bytes -> {:?}", data.len(), out);
        }

        Command::PatchWorkmode { input, out, mode } => {
            let mode_val = parse_work_mode(&mode)?;
            let mut buf = std::fs::read(&input).with_context(|| format!("reading {:?}", input))?;

            let before = sunxi_head::read_work_mode(&buf)?;
            let before_sum = sunxi_head::read_checksum(&buf)?;

            sunxi_head::set_work_mode(&mut buf, mode_val)?;
            let after_sum = sunxi_head::fix_checksum(&mut buf)?;

            std::fs::write(&out, &buf).with_context(|| format!("writing {:?}", out))?;
            println!(
                "work_mode: 0x{before:02x} -> 0x{mode_val:02x}\ncheck_sum: 0x{before_sum:08x} -> 0x{after_sum:08x}\nwrote {:?}",
                out
            );
        }

        Command::FelVersion => {
            let dev = fel::FelDevice::open()?;
            let ver = dev.get_version()?;
            println!(
                "signature={:?} soc_id=0x{:04x} protocol=0x{:04x} scratchpad=0x{:x}",
                String::from_utf8_lossy(&ver.signature),
                ver.soc_id,
                ver.protocol,
                ver.scratchpad
            );
        }

        Command::EfexVerifyDev => {
            let mut dev = efex::EfexDevice::open()?;
            let info = dev.verify_dev()?;
            println!(
                "tag={:?} platform_id_hw=0x{:08x} platform_id_fw=0x{:08x} mode=0x{:02x}",
                String::from_utf8_lossy(&info.tag),
                info.platform_id_hw,
                info.platform_id_fw,
                info.mode
            );
        }

        Command::EfexQueryStorage => {
            let mut dev = efex::EfexDevice::open()?;
            let storage_type = dev.query_storage()?;
            println!("storage_type = {}", storage_type);
        }

        Command::ListPartitions { sys_partition_fex } => {
            let text = std::fs::read_to_string(&sys_partition_fex)
                .with_context(|| format!("reading {:?}", sys_partition_fex))?;
            let sp = sys_partition::SysPartition::parse(&text)?;
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

        Command::FlashMbr { image } => {
            let img = imagewty::ImageWty::open(&image)?;
            let item = img
                .find("sunxi_mbr.fex")
                .context("sunxi_mbr.fex not found in image")?;
            let data = img.read_item(item)?;
            let mut dev = efex::EfexDevice::open()?;
            dev.flash_set_on()?;
            dev.write_mbr(&data)?;
            println!("wrote MBR ({} bytes) from sunxi_mbr.fex", data.len());
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
            println!("wrote BOOT1 ({} bytes) from boot_package.fex", data.len());
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
            println!("wrote BOOT0 ({} bytes) from {item}", data.len());
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
            let kind = write_partition_auto(&mut dev, part.start_sector, &data)?;
            println!("write path: {kind}");
            let elapsed = start.elapsed();
            let mb = data.len() as f64 / (1024.0 * 1024.0);
            println!(
                "wrote partition '{name}' ({} bytes from {filename}) at sector 0x{:x} in {:.1}s ({:.2} MB/s)",
                data.len(),
                part.start_sector,
                elapsed.as_secs_f64(),
                mb / elapsed.as_secs_f64()
            );
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
            )?;
        }

        Command::FlashSetEraseFlag { flag } => {
            let mut dev = efex::EfexDevice::open()?;
            dev.flash_set_on()?;
            dev.set_erase_flag(flag)?;
            println!("erase flag set to {flag}");
        }

        Command::FlashAll {
            image,
            sys_partition_fex,
            erase_flag,
            skip_larger_than,
            boot0_item,
            reboot,
            no_bootstrap,
        } => {
            let sp_text = std::fs::read_to_string(&sys_partition_fex)
                .with_context(|| format!("reading {:?}", sys_partition_fex))?;
            let sp = sys_partition::SysPartition::parse(&sp_text)?;
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
                let item = img
                    .find(filename)
                    .with_context(|| format!("{filename} (for partition '{}') not found in image", p.name))?;
                if skip_larger_than > 0 && item.original_length as u64 > skip_larger_than {
                    println!(
                        "skipping '{}' ({filename}, {} bytes > limit)",
                        p.name, item.original_length
                    );
                    continue;
                }
                let data = img.read_item(item)?;
                jobs.push(Job {
                    name: p.name.clone(),
                    start_sector: p.start_sector,
                    data,
                });
            }

            let total_bytes: u64 = jobs.iter().map(|j| j.data.len() as u64).sum();
            println!(
                "plan: MBR({} B) + {} partitions ({} MB total) + BOOT1({} B) + BOOT0({} B)",
                mbr_data.len(),
                jobs.len(),
                total_bytes / (1024 * 1024),
                boot1_data.len(),
                boot0_data.len()
            );

            if !no_bootstrap {
                bootstrap::fel_to_efex(&img, bootstrap::FES1_ADDR, bootstrap::UBOOT_ADDR)?;
            }

            let mut dev = efex::EfexDevice::open()?;
            dev.flash_set_on()?;
            println!("[1] flash_set_on OK");

            dev.set_erase_flag(erase_flag)?;
            println!("[2] erase_flag={erase_flag} OK");

            dev.write_mbr(&mbr_data)?;
            println!("[3] MBR OK");

            for (i, job) in jobs.iter().enumerate() {
                let kind = write_partition_auto(&mut dev, job.start_sector, &job.data)
                    .with_context(|| {
                        format!(
                            "writing partition '{}' at sector 0x{:x}",
                            job.name, job.start_sector
                        )
                    })?;
                println!(
                    "[4.{}/{}] partition '{}' OK ({} bytes, {kind})",
                    i + 1,
                    jobs.len(),
                    job.name,
                    job.data.len()
                );
            }

            dev.write_boot1(&boot1_data)?;
            println!("[5] BOOT1 OK");

            dev.write_boot0(&boot0_data)?;
            println!("[6] BOOT0 OK");

            dev.flash_set_off()?;
            println!("[7] flash_set_off OK");

            if reboot {
                dev.trigger_reboot()?;
                println!("[8] reboot triggered");
            } else {
                println!("[8] reboot skipped (--reboot=false)");
            }
        }
    }

    Ok(())
}
