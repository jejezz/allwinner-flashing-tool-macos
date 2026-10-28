//! Windows disk enumeration and raw-disk opening.
//!
//! Disks are probed as `\\.\PhysicalDrive0..31`; properties come straight from
//! `DeviceIoControl`. Control codes are spelled out as constants (they are
//! `CTL_CODE(...)` macros in the SDK headers) so the only external surface is
//! `DeviceIoControl` itself.

use super::relay::{self, Header};
use super::{DiskInfo, FlashParams, OpenDisk};
use crate::event::Reporter;
use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::net::TcpListener;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::Cryptography::BCryptGenRandom;
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::UI::Shell::ShellExecuteW;

/// How long to wait for the elevated writer to appear: the user has to find and
/// answer the UAC prompt first.
const WORKER_START_TIMEOUT: Duration = Duration::from_secs(120);
const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x2;

const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
const IOCTL_DISK_GET_DRIVE_GEOMETRY_EX: u32 = 0x0007_00A0;
const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 = 0x0056_0000;
const FSCTL_LOCK_VOLUME: u32 = 0x0009_0018;
const FSCTL_DISMOUNT_VOLUME: u32 = 0x0009_0020;

const SHARE_READ_WRITE: u32 = 0x1 | 0x2;
const MAX_DISKS: u32 = 32;

const BUS_TYPE_SD: u32 = 0x0C;
const BUS_TYPE_MMC: u32 = 0x0D;

fn ioctl(file: &File, code: u32, input: &[u8], output: &mut [u8]) -> std::io::Result<usize> {
    let mut returned = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle() as _,
            code,
            if input.is_empty() {
                std::ptr::null()
            } else {
                input.as_ptr().cast()
            },
            input.len() as u32,
            if output.is_empty() {
                std::ptr::null_mut()
            } else {
                output.as_mut_ptr().cast()
            },
            output.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(returned as usize)
    }
}

/// Open for ioctl queries only (access 0): needs no administrator rights.
fn open_query(path: &str) -> std::io::Result<File> {
    OpenOptions::new()
        .access_mode(0)
        .share_mode(SHARE_READ_WRITE)
        .open(path)
}

fn physical_path(n: u32) -> String {
    format!(r"\\.\PhysicalDrive{n}")
}

fn u32_at(buf: &[u8], off: usize) -> u32 {
    buf.get(off..off + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .unwrap_or(0)
}

fn c_string_at(buf: &[u8], off: u32) -> Option<String> {
    let off = off as usize;
    if off == 0 || off >= buf.len() {
        return None;
    }
    let tail = &buf[off..];
    let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
    let s = String::from_utf8_lossy(&tail[..end]).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Disk numbers a volume's extents live on.
fn volume_disk_numbers(volume: &File) -> Result<Vec<u32>> {
    let mut out = [0u8; 1024];
    ioctl(volume, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, &[], &mut out)
        .context("IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS")?;
    // VOLUME_DISK_EXTENTS: u32 count, 4 bytes padding, then 24-byte extents
    // whose first field is the u32 disk number.
    let count = (u32_at(&out, 0) as usize).min((out.len() - 8) / 24);
    Ok((0..count).map(|i| u32_at(&out, 8 + i * 24)).collect())
}

/// Disks the running Windows lives on. Failing to find out is an error, not a
/// guess: the caller would otherwise be flying blind on the one check that must
/// never be wrong.
fn system_disk_numbers() -> Result<Vec<u32>> {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let volume = open_query(&format!(r"\\.\{drive}"))
        .with_context(|| format!("opening the system volume {drive} to find the system disk"))?;
    volume_disk_numbers(&volume)
}

fn describe(n: u32, system: &[u32]) -> Option<DiskInfo> {
    let path = physical_path(n);
    let file = open_query(&path).ok()?;

    // DISK_GEOMETRY_EX: a 24-byte DISK_GEOMETRY, then DiskSize (i64). Unlike
    // IOCTL_DISK_GET_LENGTH_INFO this needs no read access, so it works without
    // administrator rights.
    let mut geometry = [0u8; 128];
    let size_bytes = ioctl(&file, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, &[], &mut geometry)
        .map(|_| i64::from_le_bytes(geometry[24..32].try_into().unwrap()).max(0) as u64)
        .unwrap_or(0);

    // STORAGE_PROPERTY_QUERY { PropertyId: StorageDeviceProperty = 0,
    // QueryType: PropertyStandardQuery = 0, AdditionalParameters[1] }.
    let query = [0u8; 12];
    let mut desc = [0u8; 1024];
    let (removable, vendor, model) =
        match ioctl(&file, IOCTL_STORAGE_QUERY_PROPERTY, &query, &mut desc) {
            Ok(returned) => {
                let desc = &desc[..returned.min(desc.len())];
                // STORAGE_DEVICE_DESCRIPTOR: RemovableMedia at +10,
                // Vendor/ProductId string offsets at +12/+16, BusType at +28.
                let removable_media = desc.get(10).copied().unwrap_or(0) != 0;
                let bus = u32_at(desc, 28);
                (
                    removable_media || bus == BUS_TYPE_SD || bus == BUS_TYPE_MMC,
                    c_string_at(desc, u32_at(desc, 12)),
                    c_string_at(desc, u32_at(desc, 16)),
                )
            }
            Err(_) => (false, None, None),
        };

    let display_name = match (&vendor, &model) {
        (Some(v), Some(m)) => format!("{v} {m}"),
        (None, Some(m)) => m.clone(),
        (Some(v), None) => v.clone(),
        (None, None) => path.clone(),
    };
    Some(DiskInfo {
        id: path,
        display_name,
        size_bytes,
        removable,
        is_system_disk: system.contains(&n),
        vendor,
        model,
    })
}

pub fn enumerate() -> Result<Vec<DiskInfo>> {
    let system = system_disk_numbers()?;
    Ok((0..MAX_DISKS).filter_map(|n| describe(n, &system)).collect())
}

/// Whether this process holds an elevated (administrator) token.
pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// 128 random bits from the system generator, as hex. It is what stops another
/// local process from posing as the elevated writer on the loopback port.
fn random_token() -> Result<String> {
    let mut bytes = [0u8; 16];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        bail!("could not generate a session token (status {status:#x})");
    }
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// Start this same executable again, elevated and with no window, as the card
/// writer. Blocks only while the UAC prompt is up.
fn spawn_elevated_worker(port: u16, token: &str) -> Result<()> {
    let exe = std::env::current_exe().context("locating aw-tool")?;
    let verb = wide("runas");
    let file = wide(&exe);
    let params = wide(format!("flash-sdcard-worker --port {port} --token {token}"));
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            0, // SW_HIDE
        )
    } as isize;
    // ShellExecute reports failure with a value of 32 or less. Declining the
    // UAC prompt comes back as access denied.
    if result <= 32 {
        bail!(
            "administrator permission was not granted — raw disk writes need it \
             (UAC prompt declined or unavailable, code {result})"
        );
    }
    Ok(())
}

/// Hand the write to an elevated copy of this program and stream the image to
/// it. The image is read here, in the user's own session.
pub fn flash_via_worker(
    disk: &DiskInfo,
    boot0: &[u8],
    boot_package: &[u8],
    image: &Path,
    firmware_len: u64,
    params: &FlashParams,
    rep: &Reporter,
) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("opening a local socket")?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let token = random_token()?;

    let mut firmware = File::open(image).with_context(|| format!("opening {:?}", image))?;
    rep.step(
        "elevating",
        "starting the elevated card writer — approve the permission prompt",
    );
    spawn_elevated_worker(port, &token)?;

    let deadline = Instant::now() + WORKER_START_TIMEOUT;
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    bail!("the elevated card writer never started");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e).context("waiting for the elevated card writer"),
        }
    };
    stream.set_nonblocking(false)?;

    let header = Header {
        disk: disk.id.clone(),
        force: params.force,
        max_size_bytes: params.max_size_bytes,
        boot0_len: boot0.len() as u64,
        boot_package_len: boot_package.len() as u64,
        firmware_len,
    };
    relay::parent_session(stream, &token, &header, boot0, boot_package, &mut firmware, rep)
}

fn disk_number(id: &str) -> Result<u32> {
    id.to_ascii_lowercase()
        .strip_prefix(r"\\.\physicaldrive")
        .and_then(|n| n.parse().ok())
        .with_context(|| format!("'{id}' is not a \\\\.\\PhysicalDriveN path"))
}

/// Lock and dismount every mounted volume on the disk. Windows refuses raw
/// writes into a mounted volume's sectors, and the card's old partitions sit
/// right where the new layout goes.
fn lock_volumes(disk: u32) -> Result<Vec<File>> {
    let mut locked = Vec::new();
    for letter in 'A'..='Z' {
        let path = format!(r"\\.\{letter}:");
        let Ok(probe) = open_query(&path) else {
            continue;
        };
        let on_disk = volume_disk_numbers(&probe)
            .map(|n| n.contains(&disk))
            .unwrap_or(false);
        drop(probe);
        if !on_disk {
            continue;
        }
        let volume = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(SHARE_READ_WRITE)
            .open(&path)
            .with_context(|| format!("opening volume {letter}: for locking"))?;
        ioctl(&volume, FSCTL_LOCK_VOLUME, &[], &mut [])
            .with_context(|| format!("locking volume {letter}: — close anything using the card"))?;
        ioctl(&volume, FSCTL_DISMOUNT_VOLUME, &[], &mut [])
            .with_context(|| format!("dismounting volume {letter}:"))?;
        locked.push(volume);
    }
    Ok(locked)
}

pub fn open_for_write(disk: &DiskInfo) -> Result<OpenDisk> {
    let n = disk_number(&disk.id)?;
    if system_disk_numbers()?.contains(&n) {
        bail!("{} is the system disk; refusing to open it for writing", disk.id);
    }
    let locked = lock_volumes(n)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(SHARE_READ_WRITE)
        .open(&disk.id)
        .with_context(|| {
            format!(
                "opening {} for writing (raw disk access needs an administrator prompt)",
                disk.id
            )
        })?;
    Ok(OpenDisk {
        file,
        guard: Some(Box::new(locked)),
    })
}
