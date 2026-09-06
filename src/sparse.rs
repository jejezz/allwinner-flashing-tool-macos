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

const SPARSE_MAGIC: u32 = 0xED26_FF3A;

const CHUNK_TYPE_RAW: u16 = 0xCAC1;
const CHUNK_TYPE_FILL: u16 = 0xCAC2;
const CHUNK_TYPE_DONT_CARE: u16 = 0xCAC3;
const CHUNK_TYPE_CRC32: u16 = 0xCAC4;

/// One region of the expanded (raw) image that actually has to be written.
/// `out_offset` is the byte offset within the destination partition.
#[derive(Debug)]
pub enum Segment<'a> {
    Raw { out_offset: u64, data: &'a [u8] },
    /// `len` bytes of `value` repeated (little-endian, 4 bytes at a time).
    Fill { out_offset: u64, value: u32, len: u64 },
}

pub struct SparseImage<'a> {
    /// Total size of the expanded image in bytes.
    pub expanded_len: u64,
    pub segments: Vec<Segment<'a>>,
}

/// True if `data` starts with the Android sparse magic.
pub fn is_sparse(data: &[u8]) -> bool {
    data.len() >= 4
        && u32::from_le_bytes(data[0..4].try_into().unwrap()) == SPARSE_MAGIC
}

/// Parse a sparse image, returning the regions that need writing.
pub fn parse(data: &[u8]) -> Result<SparseImage<'_>> {
    if data.len() < 28 {
        bail!("too short to be a sparse image ({} bytes)", data.len());
    }
    let magic = u32::from_le_bytes(data[0..4].try_into().unwrap());
    if magic != SPARSE_MAGIC {
        bail!("not a sparse image (magic {magic:#010x})");
    }
    let file_hdr_sz = u16::from_le_bytes(data[8..10].try_into().unwrap()) as usize;
    let chunk_hdr_sz = u16::from_le_bytes(data[10..12].try_into().unwrap()) as usize;
    let blk_sz = u32::from_le_bytes(data[12..16].try_into().unwrap()) as u64;
    let total_blks = u32::from_le_bytes(data[16..20].try_into().unwrap()) as u64;
    let total_chunks = u32::from_le_bytes(data[20..24].try_into().unwrap());

    if blk_sz == 0 || blk_sz % crate::sys_partition::SECTOR_SIZE != 0 {
        bail!("sparse block size {blk_sz} is not a multiple of the 512B sector size");
    }
    if chunk_hdr_sz < 12 {
        bail!("bad sparse chunk header size {chunk_hdr_sz}");
    }

    let mut segments = Vec::new();
    let mut pos = file_hdr_sz;
    let mut out_blk: u64 = 0;

    for i in 0..total_chunks {
        let hdr = data
            .get(pos..pos + chunk_hdr_sz)
            .with_context(|| format!("truncated at chunk {i} header"))?;
        let chunk_type = u16::from_le_bytes(hdr[0..2].try_into().unwrap());
        let chunk_blks = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as u64;
        let total_sz = u32::from_le_bytes(hdr[8..12].try_into().unwrap()) as usize;
        let payload_len = total_sz
            .checked_sub(chunk_hdr_sz)
            .with_context(|| format!("chunk {i} total_sz {total_sz} < header size"))?;
        let payload_at = pos + chunk_hdr_sz;
        let payload = data
            .get(payload_at..payload_at + payload_len)
            .with_context(|| format!("truncated at chunk {i} payload"))?;

        let out_offset = out_blk * blk_sz;
        match chunk_type {
            CHUNK_TYPE_RAW => {
                let want = chunk_blks * blk_sz;
                if payload_len as u64 != want {
                    bail!(
                        "chunk {i}: RAW payload {payload_len} bytes, expected {want}"
                    );
                }
                segments.push(Segment::Raw {
                    out_offset,
                    data: payload,
                });
            }
            CHUNK_TYPE_FILL => {
                if payload_len != 4 {
                    bail!("chunk {i}: FILL payload {payload_len} bytes, expected 4");
                }
                segments.push(Segment::Fill {
                    out_offset,
                    value: u32::from_le_bytes(payload[0..4].try_into().unwrap()),
                    len: chunk_blks * blk_sz,
                });
            }
            // Nothing to write: the erase pass already cleared these regions.
            CHUNK_TYPE_DONT_CARE => {}
            // Checksum-only chunk, carries no output blocks of its own.
            CHUNK_TYPE_CRC32 => {}
            other => bail!("chunk {i}: unknown chunk type {other:#06x}"),
        }

        out_blk += chunk_blks;
        pos = payload_at + payload_len;
    }

    if out_blk != total_blks {
        bail!(
            "sparse chunks expand to {out_blk} blocks, header says {total_blks}"
        );
    }

    Ok(SparseImage {
        expanded_len: total_blks * blk_sz,
        segments,
    })
}
