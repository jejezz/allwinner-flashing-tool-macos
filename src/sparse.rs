//! Android sparse image parser.
//!
//! `super.fex` (and potentially other large partition images) is shipped as an
//! Android *sparse* image, not a raw partition image: a 28-byte header, then a
//! sequence of 12-byte chunk headers describing how the raw image is
//! reconstructed. The real PhoenixSuit expands this while writing (the vendor
//! u-boot has the matching support: `sprite/sparse/`, `unsparse_checksum()`,
//! `sunxi_sprite_part_sparsedata_verify()`); writing the container verbatim
//! puts the sparse header where the partition's real content belongs.
//!
//! For `super` that content is the liblp metadata: on a correctly flashed
//! device sector 0 of the partition is zero-filled and offset 4096 carries the
//! geometry magic "gDla" (0x616c4467). With the container written raw, offset 0
//! is the sparse magic instead, first-stage init cannot find the dynamic
//! partition metadata, and it reboots to the bootloader before attempting any
//! mount — see docs/T527-T507-FEL-EFEX-기술조사.md.
//!
//! DONT_CARE chunks are skipped entirely: the erase pass (erase_flag=1) has
//! already cleared those regions, which is also what the vendor tool leaves
//! behind.

use anyhow::{bail, Context, Result};
use std::io::{Read, Seek, SeekFrom};

const SPARSE_MAGIC: u32 = 0xED26_FF3A;

const CHUNK_TYPE_RAW: u16 = 0xCAC1;
const CHUNK_TYPE_FILL: u16 = 0xCAC2;
const CHUNK_TYPE_DONT_CARE: u16 = 0xCAC3;
const CHUNK_TYPE_CRC32: u16 = 0xCAC4;

/// One region of the expanded (raw) image that actually has to be written.
/// `out_offset` is the byte offset within the destination partition.
#[derive(Debug)]
pub enum Segment {
    /// `file_offset`/`len` locate the bytes in the *container* file (the
    /// IMAGEWTY image, not just this item) — read them from there on demand
    /// when writing rather than holding them in memory ahead of time.
    Raw { out_offset: u64, file_offset: u64, len: u64 },
    /// `len` bytes of `value` repeated (little-endian, 4 bytes at a time).
    Fill { out_offset: u64, value: u32, len: u64 },
}

pub struct SparseImage {
    /// Total size of the expanded image in bytes.
    pub expanded_len: u64,
    pub segments: Vec<Segment>,
}

/// True if `data` starts with the Android sparse magic.
pub fn is_sparse(data: &[u8]) -> bool {
    data.len() >= 4
        && u32::from_le_bytes(data[0..4].try_into().unwrap()) == SPARSE_MAGIC
}

/// Parse a sparse image's structure from a seekable stream, without reading
/// any RAW chunk's payload bytes into memory — only the (small) chunk
/// headers and FILL values are actually read; RAW payloads are skipped over
/// with a seek, and `Segment::Raw` instead records *where* in `r` those
/// bytes live, so the caller can stream them straight to the device later.
///
/// `r` must be positioned at the start of the sparse container (e.g. via
/// `ImageWty::open_item`); its absolute position is what `file_offset`
/// values are recorded in.
pub fn parse_streaming<R: Read + Seek>(mut r: R) -> Result<SparseImage> {
    let mut file_hdr = [0u8; 28];
    r.read_exact(&mut file_hdr).context("reading sparse file header")?;
    let magic = u32::from_le_bytes(file_hdr[0..4].try_into().unwrap());
    if magic != SPARSE_MAGIC {
        bail!("not a sparse image (magic {magic:#010x})");
    }
    let file_hdr_sz = u64::from(u16::from_le_bytes(file_hdr[8..10].try_into().unwrap()));
    let chunk_hdr_sz = u64::from(u16::from_le_bytes(file_hdr[10..12].try_into().unwrap()));
    let blk_sz = u64::from(u32::from_le_bytes(file_hdr[12..16].try_into().unwrap()));
    let total_blks = u64::from(u32::from_le_bytes(file_hdr[16..20].try_into().unwrap()));
    let total_chunks = u32::from_le_bytes(file_hdr[20..24].try_into().unwrap());

    if blk_sz == 0 || blk_sz % crate::sys_partition::SECTOR_SIZE != 0 {
        bail!("sparse block size {blk_sz} is not a multiple of the 512B sector size");
    }
    if chunk_hdr_sz < 12 {
        bail!("bad sparse chunk header size {chunk_hdr_sz}");
    }
    // The 28-byte array above already covers the fixed file header fields;
    // a larger file_hdr_sz just means trailing padding to skip.
    if file_hdr_sz < 28 {
        bail!("sparse file header size {file_hdr_sz} smaller than the fixed 28-byte header");
    } else if file_hdr_sz > 28 {
        r.seek(SeekFrom::Current((file_hdr_sz - 28) as i64))
            .context("skipping sparse file header padding")?;
    }
    let chunk_hdr_pad = chunk_hdr_sz - 12;

    let mut segments = Vec::new();
    let mut out_blk: u64 = 0;

    for i in 0..total_chunks {
        let mut hdr = [0u8; 12];
        r.read_exact(&mut hdr).with_context(|| format!("reading chunk {i} header"))?;
        if chunk_hdr_pad > 0 {
            r.seek(SeekFrom::Current(chunk_hdr_pad as i64))
                .with_context(|| format!("skipping chunk {i} header padding"))?;
        }
        let chunk_type = u16::from_le_bytes(hdr[0..2].try_into().unwrap());
        let chunk_blks = u64::from(u32::from_le_bytes(hdr[4..8].try_into().unwrap()));
        let total_sz = u64::from(u32::from_le_bytes(hdr[8..12].try_into().unwrap()));
        let payload_len = total_sz
            .checked_sub(chunk_hdr_sz)
            .with_context(|| format!("chunk {i} total_sz {total_sz} < header size"))?;

        let out_offset = out_blk * blk_sz;
        match chunk_type {
            CHUNK_TYPE_RAW => {
                let want = chunk_blks * blk_sz;
                if payload_len != want {
                    bail!("chunk {i}: RAW payload {payload_len} bytes, expected {want}");
                }
                let file_offset = r
                    .stream_position()
                    .with_context(|| format!("reading position before chunk {i} payload"))?;
                segments.push(Segment::Raw { out_offset, file_offset, len: payload_len });
                r.seek(SeekFrom::Current(payload_len as i64))
                    .with_context(|| format!("skipping chunk {i} payload"))?;
            }
            CHUNK_TYPE_FILL => {
                if payload_len != 4 {
                    bail!("chunk {i}: FILL payload {payload_len} bytes, expected 4");
                }
                let mut v = [0u8; 4];
                r.read_exact(&mut v)
                    .with_context(|| format!("reading chunk {i} FILL value"))?;
                segments.push(Segment::Fill {
                    out_offset,
                    value: u32::from_le_bytes(v),
                    len: chunk_blks * blk_sz,
                });
            }
            // Nothing to write: the erase pass already cleared these regions.
            CHUNK_TYPE_DONT_CARE => {
                if payload_len != 0 {
                    bail!("chunk {i}: DONT_CARE payload {payload_len} bytes, expected 0");
                }
            }
            // Checksum-only chunk, carries no output blocks of its own.
            CHUNK_TYPE_CRC32 => {
                if payload_len != 4 {
                    bail!("chunk {i}: CRC32 payload {payload_len} bytes, expected 4");
                }
                r.seek(SeekFrom::Current(payload_len as i64))
                    .with_context(|| format!("skipping chunk {i} CRC32 payload"))?;
            }
            other => bail!("chunk {i}: unknown chunk type {other:#06x}"),
        }

        out_blk += chunk_blks;
    }

    if out_blk != total_blks {
        bail!("sparse chunks expand to {out_blk} blocks, header says {total_blks}");
    }

    Ok(SparseImage {
        expanded_len: total_blks * blk_sz,
        segments,
    })
}
