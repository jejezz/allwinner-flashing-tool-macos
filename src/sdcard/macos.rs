//! macOS disk enumeration and raw-disk opening, via `diskutil` plists.

use super::{DiskInfo, OpenDisk};
use anyhow::{bail, Context, Result};
use plist::Value;
use std::fs::OpenOptions;
use std::process::Command;

fn diskutil(args: &[&str]) -> Result<Value> {
    let out = Command::new("diskutil")
        .args(args)
        .output()
        .context("running diskutil")?;
    if !out.status.success() {
        bail!(
            "diskutil {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Value::from_reader(std::io::Cursor::new(out.stdout)).context("parsing diskutil plist")
}

fn flag(info: &Value, key: &str) -> bool {
    info.as_dictionary()
        .and_then(|d| d.get(key))
        .and_then(Value::as_boolean)
        .unwrap_or(false)
}

fn text(info: &Value, key: &str) -> Option<String> {
    info.as_dictionary()
        .and_then(|d| d.get(key))
        .and_then(Value::as_string)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn size(info: &Value) -> u64 {
    let dict = info.as_dictionary();
    ["TotalSize", "Size"]
        .iter()
        .find_map(|k| dict.and_then(|d| d.get(k)).and_then(Value::as_unsigned_integer))
        .unwrap_or(0)
}

/// "disk0s2" -> "disk0"
fn whole_disk(identifier: &str) -> &str {
    let end = identifier
        .get(4..)
        .and_then(|rest| rest.find('s'))
        .map(|i| i + 4)
        .unwrap_or(identifier.len());
    &identifier[..end]
}

/// Whole disks the running macOS lives on. Failing to find out is an error, not
/// a guess.
fn boot_disks() -> Result<Vec<String>> {
    let info = diskutil(&["info", "-plist", "/"])?;
    let mut disks = Vec::new();
    if let Some(parent) = text(&info, "ParentWholeDisk") {
        disks.push(parent);
    }
    if let Some(stores) = info
        .as_dictionary()
        .and_then(|d| d.get("APFSPhysicalStores"))
        .and_then(Value::as_array)
    {
        for store in stores {
            if let Some(id) = text(store, "APFSPhysicalStore") {
                disks.push(whole_disk(&id).to_string());
            }
        }
    }
    if disks.is_empty() {
        bail!("could not determine which disk macOS boots from");
    }
    Ok(disks)
}

pub fn enumerate() -> Result<Vec<DiskInfo>> {
    let boot = boot_disks()?;
    let list = diskutil(&["list", "-plist"])?;
    let whole: Vec<String> = list
        .as_dictionary()
        .and_then(|d| d.get("WholeDisks"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_string().map(str::to_string)).collect())
        .unwrap_or_default();

    let mut disks = Vec::new();
    for id in whole {
        let info = diskutil(&["info", "-plist", &id])?;
        // Synthesized APFS containers and mounted disk images are not devices
        // anyone burns a card onto.
        if text(&info, "VirtualOrPhysical").as_deref() == Some("Virtual") {
            continue;
        }
        let internal = flag(&info, "Internal");
        let removable = flag(&info, "Removable") || flag(&info, "RemovableMedia");
        let model = text(&info, "MediaName");
        disks.push(DiskInfo {
            display_name: model.clone().unwrap_or_else(|| format!("/dev/{id}")),
            is_system_disk: boot.contains(&id) || (internal && !removable),
            removable,
            size_bytes: size(&info),
            vendor: None,
            model,
            id: format!("/dev/{id}"),
        });
    }
    Ok(disks)
}

/// The raw (uncached) device for reading back. Unmounted first, or macOS
/// remounts the card and fights over it.
pub fn open_for_read(disk: &DiskInfo) -> Result<std::fs::File> {
    let _ = Command::new("diskutil").args(["unmountDisk", &disk.id]).status();
    let raw = disk.id.replacen("/dev/disk", "/dev/rdisk", 1);
    OpenOptions::new()
        .read(true)
        .open(&raw)
        .with_context(|| format!("opening {raw} for reading (raw disk access needs root: run with sudo)"))
}

pub fn open_for_write(disk: &DiskInfo) -> Result<OpenDisk> {
    let status = Command::new("diskutil")
        .args(["unmountDisk", &disk.id])
        .status()
        .context("running diskutil unmountDisk")?;
    if !status.success() {
        bail!("could not unmount {} — close anything using the card", disk.id);
    }
    // The unbuffered character device is what bulk writes want.
    let raw = disk.id.replacen("/dev/disk", "/dev/rdisk", 1);
    let file = OpenOptions::new()
        .write(true)
        .open(&raw)
        .with_context(|| format!("opening {raw} for writing (raw disk access needs root: run with sudo)"))?;
    Ok(OpenDisk { file, guard: None })
}
