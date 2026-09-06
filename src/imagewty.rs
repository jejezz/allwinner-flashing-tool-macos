//! IMAGEWTY container parser.
//!
//! Verified against a real T527 production image
//! (t527_android13_pluto_wallpad_uart0.img) — see docs/T527-T507-FEL-EFEX-기술조사.md
//! section 7. Item records are fixed 1024-byte entries; this parser locates the
//! table by scanning for the `00 01 00 00 00 04 00 00` marker pair that precedes
//! every record's maintype/subtype/filename fields, rather than trusting a fixed
//! header size (the plain 0x60-byte main header does not directly abut the table).

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

const MAGIC: &[u8; 8] = b"IMAGEWTY";
const ITEM_COUNT_OFFSET: usize = 0x3c;
const ITEM_RECORD_SIZE: usize = 1024;
const RECORD_MARKER: [u8; 8] = [0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00];
const RECORD_MARKER_REL_OFFSET: usize = 28; // marker sits at +28 within a record
const MAINTYPE_OFFSET: usize = 36;
const MAINTYPE_LEN: usize = 8;
const SUBTYPE_OFFSET: usize = 44;
const SUBTYPE_LEN: usize = 16;
const FILENAME_OFFSET: usize = 64;
const STORED_LEN_OFFSET: usize = 0x140;
const ORIGINAL_LEN_OFFSET: usize = 0x148;
const FILE_OFFSET_OFFSET: usize = 0x150;

/// How much of the file's head to scan when locating the item table.
/// 51 items * 1024B fits comfortably inside 512KiB even with a generous preamble.
const SCAN_WINDOW: usize = 512 * 1024;

#[derive(Debug, Clone)]
pub struct Item {
    pub maintype: String,
    pub subtype: String,
    pub filename: String,
    pub stored_length: u32,
    pub original_length: u32,
    pub file_offset: u32,
}

pub struct ImageWty {
    path: std::path::PathBuf,
    pub items: Vec<Item>,
}

fn trim_c_str(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

impl ImageWty {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut f = File::open(&path).with_context(|| format!("opening {:?}", path))?;

        let mut head = vec![0u8; SCAN_WINDOW];
        let n = f.read(&mut head)?;
        head.truncate(n);

        if head.len() < 0x60 || &head[0..8] != MAGIC {
            bail!("not an IMAGEWTY image (bad magic)");
        }

        let item_count = u32::from_le_bytes(
            head[ITEM_COUNT_OFFSET..ITEM_COUNT_OFFSET + 4]
                .try_into()
                .unwrap(),
        ) as usize;

        // Locate the first item record by scanning for the marker pair.
        let table_start = head
            .windows(RECORD_MARKER.len())
            .position(|w| w == RECORD_MARKER)
            .map(|pos| pos - RECORD_MARKER_REL_OFFSET)
            .context("could not locate item table (marker not found in scan window)")?;

        let mut items = Vec::with_capacity(item_count);
        for i in 0..item_count {
            let rec_start = table_start + i * ITEM_RECORD_SIZE;
            let rec_end = rec_start + ITEM_RECORD_SIZE;
            if rec_end > head.len() {
                bail!(
                    "item table extends past scan window (increase SCAN_WINDOW); \
                     parsed {} of {} items",
                    i,
                    item_count
                );
            }
            let rec = &head[rec_start..rec_end];

            let maintype = trim_c_str(&rec[MAINTYPE_OFFSET..MAINTYPE_OFFSET + MAINTYPE_LEN]);
            let subtype = trim_c_str(&rec[SUBTYPE_OFFSET..SUBTYPE_OFFSET + SUBTYPE_LEN]);
            let filename = trim_c_str(&rec[FILENAME_OFFSET..]);
            let stored_length =
                u32::from_le_bytes(rec[STORED_LEN_OFFSET..STORED_LEN_OFFSET + 4].try_into().unwrap());
            let original_length = u32::from_le_bytes(
                rec[ORIGINAL_LEN_OFFSET..ORIGINAL_LEN_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            let file_offset = u32::from_le_bytes(
                rec[FILE_OFFSET_OFFSET..FILE_OFFSET_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );

            items.push(Item {
                maintype,
                subtype,
                filename,
                stored_length,
                original_length,
                file_offset,
            });
        }

        Ok(Self { path, items })
    }

    /// Find an item by exact filename match (e.g. "u-boot.fex", "sys_partition.fex").
    pub fn find(&self, filename: &str) -> Option<&Item> {
        self.items.iter().find(|it| it.filename == filename)
    }

    /// Read an item's payload out of the image.
    pub fn read_item(&self, item: &Item) -> Result<Vec<u8>> {
        let mut f = File::open(&self.path)?;
        f.seek(SeekFrom::Start(item.file_offset as u64))?;
        let mut buf = vec![0u8; item.original_length as usize];
        f.read_exact(&mut buf)
            .with_context(|| format!("reading payload for {}", item.filename))?;
        Ok(buf)
    }
}
