//! Parser for `sys_partition.fex` — the partition table spec bundled in the
//! IMAGEWTY image (see docs/T527-T507-FEL-EFEX-기술조사.md section 5).
//!
//! Computes each partition's absolute starting sector, matching the real
//! device's own MBR dump exactly (verified in section 11.2's UART log:
//! bootloader_a addrlo=0x8000 == mbr_size_sectors, each subsequent partition
//! starts right after the previous one, sequential, no gaps). The one
//! special case is a trailing partition with no `size` field (`UDISK` in
//! this SDK) — it has no fixed size (fills whatever remains on the medium)
//! and its offset is still computed the same way, just with `size_sectors:
//! None`.

use anyhow::{bail, Context, Result};

pub const SECTOR_SIZE: u64 = 512;

#[derive(Debug, Clone)]
pub struct Partition {
    pub name: String,
    /// None for the trailing "fill remaining space" entry (e.g. UDISK).
    pub size_sectors: Option<u64>,
    pub downloadfile: Option<String>,
    pub user_type: u32,
    pub keydata: u32,
    pub ro: u32,
    /// Absolute starting sector, computed by sequential accumulation
    /// starting right after the MBR region.
    pub start_sector: u64,
}

pub struct SysPartition {
    pub mbr_size_sectors: u64,
    pub partitions: Vec<Partition>,
}

impl SysPartition {
    pub fn parse(text: &str) -> Result<Self> {
        let mut mbr_size_kb: Option<u64> = None;
        let mut partitions = Vec::new();

        // Current partition being accumulated (fields seen since the last
        // "[partition]" line), and whether we're inside [mbr] or [partition].
        #[derive(PartialEq)]
        enum Section {
            None,
            Mbr,
            Partition,
        }
        let mut section = Section::None;
        let mut cur_name: Option<String> = None;
        let mut cur_size: Option<String> = None;
        let mut cur_downloadfile: Option<String> = None;
        let mut cur_user_type: u32 = 0;
        let mut cur_keydata: u32 = 0;
        let mut cur_ro: u32 = 0;

        fn flush(
            partitions: &mut Vec<Partition>,
            name: &mut Option<String>,
            size: &mut Option<String>,
            downloadfile: &mut Option<String>,
            user_type: &mut u32,
            keydata: &mut u32,
            ro: &mut u32,
        ) -> Result<()> {
            if let Some(name) = name.take() {
                let size_sectors = match size.take() {
                    Some(s) => Some(parse_size_to_sectors(&s)?),
                    None => None,
                };
                partitions.push(Partition {
                    name,
                    size_sectors,
                    downloadfile: downloadfile.take(),
                    user_type: *user_type,
                    keydata: *keydata,
                    ro: *ro,
                    start_sector: 0, // filled in below after all parsing
                });
            }
            *user_type = 0;
            *keydata = 0;
            *ro = 0;
            Ok(())
        }

        for raw_line in text.lines() {
            // Strip ';'-style comments (the whole line format uses these).
            let line = raw_line.split(';').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }

            if line.eq_ignore_ascii_case("[mbr]") {
                flush(
                    &mut partitions,
                    &mut cur_name,
                    &mut cur_size,
                    &mut cur_downloadfile,
                    &mut cur_user_type,
                    &mut cur_keydata,
                    &mut cur_ro,
                )?;
                section = Section::Mbr;
                continue;
            }
            if line.eq_ignore_ascii_case("[partition]") {
                flush(
                    &mut partitions,
                    &mut cur_name,
                    &mut cur_size,
                    &mut cur_downloadfile,
                    &mut cur_user_type,
                    &mut cur_keydata,
                    &mut cur_ro,
                )?;
                section = Section::Partition;
                continue;
            }
            if line.starts_with('[') {
                // [partition_start], [partition_end], or anything else we
                // don't specifically handle — flush whatever partition was
                // accumulating and stop paying attention until the next
                // recognized section header.
                flush(
                    &mut partitions,
                    &mut cur_name,
                    &mut cur_size,
                    &mut cur_downloadfile,
                    &mut cur_user_type,
                    &mut cur_keydata,
                    &mut cur_ro,
                )?;
                section = Section::None;
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim();

            match section {
                Section::Mbr if key.eq_ignore_ascii_case("size") => {
                    mbr_size_kb = Some(
                        value
                            .parse()
                            .with_context(|| format!("bad [mbr] size '{value}'"))?,
                    );
                }
                Section::Partition => match key.to_ascii_lowercase().as_str() {
                    "name" => cur_name = Some(value.to_string()),
                    "size" => cur_size = Some(value.to_string()),
                    "downloadfile" => cur_downloadfile = Some(value.to_string()),
                    "user_type" => cur_user_type = parse_int(value)?,
                    "keydata" => cur_keydata = parse_int(value)?,
                    "ro" => cur_ro = parse_int(value)?,
                    _ => {}
                },
                _ => {}
            }
        }
        flush(
            &mut partitions,
            &mut cur_name,
            &mut cur_size,
            &mut cur_downloadfile,
            &mut cur_user_type,
            &mut cur_keydata,
            &mut cur_ro,
        )?;

        let mbr_size_kb = mbr_size_kb.context("no [mbr] size found")?;
        let mbr_size_sectors = mbr_size_kb * 1024 / SECTOR_SIZE;

        // Sequential offset accumulation, starting right after the MBR region.
        let mut cursor = mbr_size_sectors;
        for p in &mut partitions {
            p.start_sector = cursor;
            if let Some(sectors) = p.size_sectors {
                cursor += sectors;
            }
            // else: trailing "fill remaining" entry (UDISK) — nothing follows it in practice.
        }

        Ok(Self {
            mbr_size_sectors,
            partitions,
        })
    }

    pub fn find(&self, name: &str) -> Option<&Partition> {
        self.partitions.iter().find(|p| p.name == name)
    }
}

/// Parse a size string per sys_partition.fex convention: a bare number is in
/// SECTORS (512B); a trailing B/K/M/G (case-insensitive) suffix scales a
/// (possibly fractional, e.g. "3.5G") value given in that unit.
fn parse_size_to_sectors(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num_part, mult): (&str, f64) = if let Some(n) = s.strip_suffix(['K', 'k']) {
        (n, 1024.0)
    } else if let Some(n) = s.strip_suffix(['M', 'm']) {
        (n, 1024.0 * 1024.0)
    } else if let Some(n) = s.strip_suffix(['G', 'g']) {
        (n, 1024.0 * 1024.0 * 1024.0)
    } else if let Some(n) = s.strip_suffix(['B', 'b']) {
        (n, 1.0)
    } else {
        (s, SECTOR_SIZE as f64)
    };
    let value: f64 = num_part
        .trim()
        .parse()
        .with_context(|| format!("bad size value '{s}'"))?;
    let bytes = value * mult;
    if bytes % (SECTOR_SIZE as f64) != 0.0 {
        bail!("size '{s}' ({bytes} bytes) is not a whole number of sectors");
    }
    Ok((bytes / SECTOR_SIZE as f64) as u64)
}

fn parse_int(s: &str) -> Result<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).with_context(|| format!("bad hex int '{s}'"))
    } else {
        s.parse().with_context(|| format!("bad int '{s}'"))
    }
}
