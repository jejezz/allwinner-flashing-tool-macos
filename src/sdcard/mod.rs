//! Card fusing: write boot0 + boot_package + firmware.img to a raw SD card at
//! fixed LBAs, so the board's own u-boot installs the image into its eMMC when
//! the card is inserted at power-on. No host/board protocol is involved.
//!
//! Layout and the source references behind it: docs/sdcard-fusing-조사.md.
//! The physical write is the dangerous part of this tool — a wrong target is an
//! unrecoverable loss on the host — so every refusal lives in
//! [`check_disk_safety`], a pure function that is unit-tested on its own.

use crate::event::Reporter;
use crate::sunxi_head;
use crate::sys_partition::SECTOR_SIZE;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Instant;

mod relay;
use relay::Target;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows as platform;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as platform;

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use super::{DiskInfo, OpenDisk};
    use anyhow::{bail, Result};

    pub fn enumerate() -> Result<Vec<DiskInfo>> {
        bail!("SD-card fusing is only supported on Windows and macOS")
    }

    pub fn open_for_write(_disk: &DiskInfo) -> Result<OpenDisk> {
        bail!("SD-card fusing is only supported on Windows and macOS")
    }
}

pub const BOOT0_LBA: u64 = 16;
pub const BOOT_PACKAGE_BACKUP_LBA: u64 = 24576;
pub const BOOT_PACKAGE_LBA: u64 = 32800;
pub const FIRMWARE_LBA: u64 = 40960;

/// Default ceiling for `--max-size-bytes`: above any real SD card in use, below
/// the internal drives that are the usual wrong pick.
pub const DEFAULT_MAX_SIZE_BYTES: u64 = 256 * 1024 * 1024 * 1024;

const CHUNK: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskInfo {
    /// Exactly what the user passes back to `flash-sdcard`
    /// (Windows `\\.\PhysicalDriveN`, macOS `/dev/diskN`).
    pub id: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub removable: bool,
    pub is_system_disk: bool,
    pub vendor: Option<String>,
    pub model: Option<String>,
}

impl DiskInfo {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "display_name": self.display_name,
            "size_bytes": self.size_bytes,
            "removable": self.removable,
            "is_system_disk": self.is_system_disk,
            "vendor": self.vendor,
            "model": self.model,
        })
    }
}

/// A refused write, tagged so a front-end can explain which rule fired.
#[derive(Debug)]
pub struct SafetyError {
    pub kind: &'static str,
    message: String,
}

impl SafetyError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for SafetyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SafetyError {}

/// A disk opened for writing. `guard` keeps platform state (Windows volume
/// locks) alive for as long as the file is.
pub struct OpenDisk {
    pub file: File,
    pub guard: Option<Box<dyn std::any::Any>>,
}

pub fn enumerate_disks() -> Result<Vec<DiskInfo>> {
    platform::enumerate()
}

pub fn same_id(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Bytes the card must hold: everything up to the firmware start, plus the
/// firmware itself rounded up to a whole sector.
pub fn required_bytes(firmware_len: u64) -> u64 {
    FIRMWARE_LBA * SECTOR_SIZE + firmware_len.div_ceil(SECTOR_SIZE) * SECTOR_SIZE
}

/// Refuse anything that is not clearly an SD card that is big enough.
/// `is_system_disk` and a too-small disk can never be overridden; `force` only
/// waives the removable and size-ceiling checks.
pub fn check_disk_safety(
    disk: &DiskInfo,
    required_bytes: u64,
    max_size_bytes: u64,
    force: bool,
) -> std::result::Result<(), SafetyError> {
    if disk.is_system_disk {
        return Err(SafetyError::new(
            "is_system_disk",
            format!("{} is the system disk; refusing to write to it", disk.id),
        ));
    }
    if disk.size_bytes < required_bytes {
        return Err(SafetyError::new(
            "too_small",
            format!(
                "{} is {} bytes, but the card layout needs at least {} bytes",
                disk.id, disk.size_bytes, required_bytes
            ),
        ));
    }
    if !disk.removable && !force {
        return Err(SafetyError::new(
            "not_removable",
            format!(
                "{} is not reported as removable media; it may be an internal disk \
                 rather than an SD card. Pass --force only if you have verified it",
                disk.id
            ),
        ));
    }
    if max_size_bytes != 0 && disk.size_bytes > max_size_bytes && !force {
        return Err(SafetyError::new(
            "too_large",
            format!(
                "{} is {} bytes, above the {} byte limit (--max-size-bytes); \
                 it is probably not an SD card",
                disk.id, disk.size_bytes, max_size_bytes
            ),
        ));
    }
    Ok(())
}

/// Re-read the disk list right before writing and confirm the target is still
/// the same device the user was shown (a card swapped in the meantime would
/// otherwise inherit the earlier safety check).
fn recheck(disk: &DiskInfo) -> Result<()> {
    let now = enumerate_disks()?;
    match now.iter().find(|d| same_id(&d.id, &disk.id)) {
        Some(cur) if cur == disk => Ok(()),
        _ => Err(SafetyError::new(
            "size_mismatch",
            format!("{} changed or disappeared since it was checked", disk.id),
        )
        .into()),
    }
}

fn open_for_write(disk: &DiskInfo) -> Result<OpenDisk> {
    recheck(disk)?;
    platform::open_for_write(disk)
}

/// What the safety checks need beyond the disk itself, carried to the elevated
/// writer so it can repeat them.
pub struct FlashParams {
    pub force: bool,
    pub max_size_bytes: u64,
}

fn open_target(disk: &DiskInfo) -> Result<Target> {
    let opened = open_for_write(disk)?;
    Ok(Target {
        out: Box::new(opened.file),
        guard: opened.guard,
    })
}

/// Write the card layout to `disk`. Without administrator rights (Windows) the
/// write is handed to an elevated copy of this program; the image is still read
/// here, so network drives and the user's own session stay in play.
pub fn flash(
    disk: &DiskInfo,
    boot0: &[u8],
    boot_package: &[u8],
    image: &Path,
    firmware_len: u64,
    params: &FlashParams,
    rep: &Reporter,
) -> Result<()> {
    #[cfg(windows)]
    if !platform::is_elevated() {
        return platform::flash_via_worker(
            disk,
            boot0,
            boot_package,
            image,
            firmware_len,
            params,
            rep,
        );
    }
    let mut firmware =
        File::open(image).with_context(|| format!("opening {:?}", image))?;
    let mut target = open_target(disk)?;
    write_card_layout(&mut target.out, boot0, boot_package, &mut firmware, firmware_len, rep)?;
    target.out.sync().context("flushing the card after writing")?;
    Ok(())
}

/// The elevated half of [`flash`]: connect back to the parent, take the data it
/// streams, and write it. It does not trust the parent — the disk is looked up
/// and safety-checked again here, with the parent's own `--force` and size
/// limit but never a way around the system-disk and too-small refusals.
pub fn run_worker(port: u16, token: &str) -> Result<()> {
    let stream =
        TcpStream::connect(("127.0.0.1", port)).context("connecting to the parent process")?;
    relay::worker_session(stream, token, &mut |h| {
        let disks = enumerate_disks()?;
        let disk = disks
            .iter()
            .find(|d| same_id(&d.id, &h.disk))
            .with_context(|| format!("disk '{}' not found", h.disk))?;
        check_disk_safety(disk, required_bytes(h.firmware_len), h.max_size_bytes, h.force)?;
        open_target(disk)
    })
}

/// boot0 as the card needs it: work_mode = CARD_PRODUCT, checksum recomputed.
pub fn card_boot0(raw: &[u8]) -> Result<Vec<u8>> {
    let mut data = raw.to_vec();
    sunxi_head::set_work_mode(&mut data, sunxi_head::WORK_MODE_CARD_PRODUCT)
        .context("patching work_mode in boot0")?;
    sunxi_head::fix_checksum(&mut data).context("recomputing boot0 checksum")?;
    Ok(data)
}

/// The fixed LBAs leave a bounded window for each blob; one that overruns would
/// silently overwrite its neighbour.
pub fn validate_layout(boot0_len: u64, boot_package_len: u64) -> Result<()> {
    let sectors = |len: u64| len.div_ceil(SECTOR_SIZE);
    if BOOT0_LBA + sectors(boot0_len) > BOOT_PACKAGE_BACKUP_LBA {
        bail!("boot0 ({boot0_len} bytes) overruns the boot_package backup at LBA {BOOT_PACKAGE_BACKUP_LBA}");
    }
    let bp = sectors(boot_package_len);
    if BOOT_PACKAGE_BACKUP_LBA + bp > BOOT_PACKAGE_LBA {
        bail!("boot_package ({boot_package_len} bytes) overruns its backup slot");
    }
    if BOOT_PACKAGE_LBA + bp > FIRMWARE_LBA {
        bail!("boot_package ({boot_package_len} bytes) overruns the firmware start at LBA {FIRMWARE_LBA}");
    }
    Ok(())
}

/// Copy `len` bytes from `src` to the sector at `lba`, in 64 KB chunks. A raw
/// device only accepts whole-sector writes, so a short tail is zero-padded.
fn write_region<W: Write + Seek>(
    out: &mut W,
    lba: u64,
    src: &mut dyn Read,
    len: u64,
    name: &str,
    rep: &Reporter,
) -> Result<()> {
    out.seek(SeekFrom::Start(lba * SECTOR_SIZE))
        .with_context(|| format!("seeking to LBA {lba} for {name}"))?;
    let mut buf = vec![0u8; CHUNK];
    let mut done = 0u64;
    while done < len {
        let want = (len - done).min(CHUNK as u64) as usize;
        src.read_exact(&mut buf[..want])
            .with_context(|| format!("reading {name} at byte {done}"))?;
        let padded = want.div_ceil(SECTOR_SIZE as usize) * SECTOR_SIZE as usize;
        buf[want..padded].fill(0);
        out.write_all(&buf[..padded])
            .with_context(|| format!("writing {name} at byte {done}"))?;
        done += want as u64;
        rep.progress(name, done, len);
    }
    rep.progress_done(name, len);
    Ok(())
}

/// Write the whole card layout. boot0 goes last: until it lands the BootROM
/// finds no valid boot0 on the card, so an interrupted run leaves a card that
/// simply does not boot instead of one that boots into half an image.
pub fn write_card_layout<W: Write + Seek>(
    out: &mut W,
    boot0: &[u8],
    boot_package: &[u8],
    firmware: &mut dyn Read,
    firmware_len: u64,
    rep: &Reporter,
) -> Result<()> {
    validate_layout(boot0.len() as u64, boot_package.len() as u64)?;

    let bp_len = boot_package.len() as u64;
    let regions: [(&str, u64, u64); 4] = [
        ("firmware", FIRMWARE_LBA, firmware_len),
        ("boot_package", BOOT_PACKAGE_LBA, bp_len),
        ("boot_package_backup", BOOT_PACKAGE_BACKUP_LBA, bp_len),
        ("boot0", BOOT0_LBA, boot0.len() as u64),
    ];
    rep.card_plan(&regions.map(|(name, _, bytes)| (name, bytes)));

    for (i, (name, lba, bytes)) in regions.iter().enumerate() {
        rep.partition_begin(i + 1, regions.len(), name, *bytes);
        let started = Instant::now();
        match *name {
            "firmware" => write_region(out, *lba, firmware, *bytes, name, rep)?,
            "boot0" => write_region(out, *lba, &mut &boot0[..], *bytes, name, rep)?,
            _ => write_region(out, *lba, &mut &boot_package[..], *bytes, name, rep)?,
        }
        let seconds = started.elapsed().as_secs_f64();
        rep.partition_end(i + 1, regions.len(), name, *bytes, "raw", seconds);
        rep.prose(format!(
            "  [{}/{}] {name}: {bytes} bytes @ LBA {lba} ({seconds:.1}s)",
            i + 1,
            regions.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn disk() -> DiskInfo {
        DiskInfo {
            id: "disk-under-test".into(),
            display_name: "Test".into(),
            size_bytes: 8 * 1024 * 1024 * 1024,
            removable: true,
            is_system_disk: false,
            vendor: None,
            model: None,
        }
    }

    const REQUIRED: u64 = 64 * 1024 * 1024;
    const MAX: u64 = DEFAULT_MAX_SIZE_BYTES;

    fn kind(r: std::result::Result<(), SafetyError>) -> Option<&'static str> {
        r.err().map(|e| e.kind)
    }

    #[test]
    fn good_card_passes_without_force() {
        assert_eq!(kind(check_disk_safety(&disk(), REQUIRED, MAX, false)), None);
    }

    #[test]
    fn system_disk_is_refused_even_with_force() {
        let d = DiskInfo {
            is_system_disk: true,
            ..disk()
        };
        for force in [false, true] {
            assert_eq!(
                kind(check_disk_safety(&d, REQUIRED, MAX, force)),
                Some("is_system_disk")
            );
        }
    }

    #[test]
    fn too_small_is_refused_even_with_force() {
        let d = DiskInfo {
            size_bytes: REQUIRED - 1,
            ..disk()
        };
        for force in [false, true] {
            assert_eq!(
                kind(check_disk_safety(&d, REQUIRED, MAX, force)),
                Some("too_small")
            );
        }
    }

    #[test]
    fn non_removable_needs_force() {
        let d = DiskInfo {
            removable: false,
            ..disk()
        };
        assert_eq!(
            kind(check_disk_safety(&d, REQUIRED, MAX, false)),
            Some("not_removable")
        );
        assert_eq!(kind(check_disk_safety(&d, REQUIRED, MAX, true)), None);
    }

    #[test]
    fn oversize_needs_force_and_zero_disables_the_limit() {
        let d = DiskInfo {
            size_bytes: MAX + 1,
            ..disk()
        };
        assert_eq!(
            kind(check_disk_safety(&d, REQUIRED, MAX, false)),
            Some("too_large")
        );
        assert_eq!(kind(check_disk_safety(&d, REQUIRED, MAX, true)), None);
        assert_eq!(kind(check_disk_safety(&d, REQUIRED, 0, false)), None);
    }

    #[test]
    fn system_check_wins_over_removable_flag() {
        let d = DiskInfo {
            is_system_disk: true,
            removable: true,
            ..disk()
        };
        assert_eq!(
            kind(check_disk_safety(&d, REQUIRED, MAX, true)),
            Some("is_system_disk")
        );
    }

    #[test]
    fn layout_windows_are_enforced() {
        assert!(validate_layout(69_632, 1_000_000).is_ok());
        assert!(validate_layout((BOOT_PACKAGE_BACKUP_LBA - BOOT0_LBA) * 512 + 1, 1).is_err());
        assert!(validate_layout(512, (BOOT_PACKAGE_LBA - BOOT_PACKAGE_BACKUP_LBA) * 512 + 1).is_err());
    }

    #[test]
    fn required_bytes_rounds_firmware_up_to_a_sector() {
        assert_eq!(required_bytes(0), 40960 * 512);
        assert_eq!(required_bytes(1), 40960 * 512 + 512);
        assert_eq!(required_bytes(1024), 40960 * 512 + 1024);
    }

    /// Minimal blob with a valid boot header: `length` (offset 20) covers it all.
    fn fake_boot0(len: usize) -> Vec<u8> {
        let mut b = vec![0xA5u8; len];
        b[4..12].copy_from_slice(b"eGON.BT0");
        b[20..24].copy_from_slice(&(len as u32).to_le_bytes());
        b[sunxi_head::WORK_MODE_OFFSET..sunxi_head::WORK_MODE_OFFSET + 4]
            .copy_from_slice(&sunxi_head::WORK_MODE_BOOT.to_le_bytes());
        b
    }

    #[test]
    fn card_boot0_sets_work_mode_and_a_valid_checksum() {
        let raw = fake_boot0(4096);
        let patched = card_boot0(&raw).unwrap();
        assert_eq!(
            sunxi_head::read_work_mode(&patched).unwrap(),
            sunxi_head::WORK_MODE_CARD_PRODUCT
        );
        let mut again = patched.clone();
        let sum = sunxi_head::fix_checksum(&mut again).unwrap();
        assert_eq!(sunxi_head::read_checksum(&patched).unwrap(), sum);
        assert_eq!(raw.len(), patched.len());
    }

    #[test]
    fn layout_lands_at_the_fixed_lbas() {
        let boot0 = card_boot0(&fake_boot0(4096)).unwrap();
        let mut bp = vec![0x5Au8; 3000]; // deliberately not sector-aligned
        bp[..8].copy_from_slice(b"sunxi-pk");
        let mut fw = vec![0x77u8; 100_000];
        fw[..8].copy_from_slice(b"IMAGEWTY");

        let mut out = Cursor::new(Vec::new());
        let rep = Reporter::new(false);
        write_card_layout(&mut out, &boot0, &bp, &mut &fw[..], fw.len() as u64, &rep).unwrap();
        let disk = out.into_inner();

        let at = |lba: u64| (lba * SECTOR_SIZE) as usize;
        assert_eq!(&disk[at(BOOT0_LBA)..at(BOOT0_LBA) + boot0.len()], &boot0[..]);
        assert_eq!(&disk[at(BOOT_PACKAGE_LBA)..at(BOOT_PACKAGE_LBA) + bp.len()], &bp[..]);
        assert_eq!(
            &disk[at(BOOT_PACKAGE_BACKUP_LBA)..at(BOOT_PACKAGE_BACKUP_LBA) + bp.len()],
            &bp[..]
        );
        assert_eq!(&disk[at(FIRMWARE_LBA)..at(FIRMWARE_LBA) + fw.len()], &fw[..]);
        assert_eq!(&disk[at(FIRMWARE_LBA)..at(FIRMWARE_LBA) + 8], b"IMAGEWTY");
        assert_eq!(&disk[at(BOOT0_LBA) + 4..at(BOOT0_LBA) + 12], b"eGON.BT0");
        assert_eq!(
            sunxi_head::read_work_mode(&disk[at(BOOT0_LBA)..]).unwrap(),
            sunxi_head::WORK_MODE_CARD_PRODUCT
        );

        // Every write is a whole number of sectors, tails zero-padded.
        assert_eq!(disk.len() as u64 % SECTOR_SIZE, 0);
        let fw_end = at(FIRMWARE_LBA) + fw.len();
        assert!(disk[fw_end..].iter().all(|&b| b == 0));
        let bp_end = at(BOOT_PACKAGE_LBA) + bp.len();
        assert!(disk[bp_end..bp_end.next_multiple_of(512)].iter().all(|&b| b == 0));
    }

    #[test]
    fn the_gaps_between_regions_are_left_alone() {
        let boot0 = card_boot0(&fake_boot0(4096)).unwrap();
        let bp = vec![1u8; 4096];
        let fw = vec![2u8; 4096];
        let mut out = Cursor::new(vec![0xEEu8; 41_000 * 512]);
        let rep = Reporter::new(false);
        write_card_layout(&mut out, &boot0, &bp, &mut &fw[..], fw.len() as u64, &rep).unwrap();
        let disk = out.into_inner();
        let at = |lba: u64| (lba * SECTOR_SIZE) as usize;
        // Before boot0, between boot0 and the backup, between backup and main.
        assert!(disk[..at(BOOT0_LBA)].iter().all(|&b| b == 0xEE));
        assert!(disk[at(BOOT0_LBA) + 4096..at(BOOT_PACKAGE_BACKUP_LBA)]
            .iter()
            .all(|&b| b == 0xEE));
        assert!(disk[at(BOOT_PACKAGE_BACKUP_LBA) + 4096..at(BOOT_PACKAGE_LBA)]
            .iter()
            .all(|&b| b == 0xEE));
    }
}
