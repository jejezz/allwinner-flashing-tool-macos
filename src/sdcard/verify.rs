//! Checking a card without a board.
//!
//! Two independent questions, both answerable from the host:
//!
//! * **Did the card keep what was written?** The regions are read back with the
//!   OS cache bypassed and compared, 64 KB at a time, with what was sent. This
//!   is the failure that matters in practice — a card or reader that quietly
//!   drops writes.
//! * **Would the boot chain accept it?** boot0 and the TOC1 boot_package are
//!   put through the same header checks the BootROM / card sprite apply
//!   (`sunxi_verify_checksum`, magics, lengths), and the firmware region must
//!   start with an IMAGEWTY header. This needs no image to compare against.
//!
//! What it cannot say is whether the board's own firmware then behaves — that
//! needs the board. It does remove "the card never got the data" and "the
//! data was never a valid boot chain" from the list of suspects.

use super::{CardError, BOOT0_LBA, BOOT_PACKAGE_BACKUP_LBA, BOOT_PACKAGE_LBA, FIRMWARE_LBA};
use crate::event::Reporter;
use crate::sunxi_head;
use crate::sys_partition::SECTOR_SIZE;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{Read, Seek, SeekFrom};

/// Granularity of the comparison, and of the offset reported for a mismatch.
pub const CHUNK: usize = 64 * 1024;

const TOC1_MAGIC: u32 = 0x8911_9800;
const TOC1_END: u32 = 0x3b45_494d;
const TOC1_HEAD_LEN: usize = 64;
const TOC1_ITEM_LEN: usize = 368;
const BOOT0_MAGIC: &[u8; 8] = b"eGON.BT0";
const IMAGEWTY_MAGIC: &[u8; 8] = b"IMAGEWTY";

// --- CRC-32 ----------------------------------------------------------------

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = crc_table();

pub fn crc32(data: &[u8]) -> u32 {
    !data.iter().fold(!0u32, |c, &b| {
        CRC_TABLE[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8)
    })
}

// --- Digests ---------------------------------------------------------------

/// What one region holds, as a CRC per [`CHUNK`]. A chunk is the bytes as they
/// sit on the card, so a short tail is zero-padded to a whole sector first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionDigest {
    pub name: String,
    pub lba: u64,
    pub len: u64,
    pub crcs: Vec<u32>,
}

impl RegionDigest {
    pub fn to_json(&self) -> Value {
        json!({ "name": self.name, "lba": self.lba, "len": self.len, "crcs": self.crcs })
    }

    pub fn from_json(v: &Value) -> Result<Self> {
        Ok(Self {
            name: v["name"].as_str().context("digest without a name")?.to_string(),
            lba: v["lba"].as_u64().context("digest without an lba")?,
            len: v["len"].as_u64().context("digest without a length")?,
            crcs: v["crcs"]
                .as_array()
                .context("digest without crcs")?
                .iter()
                .map(|c| c.as_u64().map(|c| c as u32).context("bad crc"))
                .collect::<Result<_>>()?,
        })
    }
}

pub fn pad_to_sector(len: usize) -> usize {
    len.div_ceil(SECTOR_SIZE as usize) * SECTOR_SIZE as usize
}

/// The CRC of one chunk as it is stored: `data` zero-padded to a sector.
pub fn chunk_crc(data: &[u8]) -> u32 {
    let padded = pad_to_sector(data.len());
    if padded == data.len() {
        return crc32(data);
    }
    let mut buf = data.to_vec();
    buf.resize(padded, 0);
    crc32(&buf)
}

/// Digest of data about to be (or just) written, read from `src`.
pub fn digest_source(name: &str, lba: u64, len: u64, src: &mut dyn Read) -> Result<RegionDigest> {
    let mut buf = vec![0u8; CHUNK];
    let mut crcs = Vec::new();
    let mut left = len;
    while left > 0 {
        let n = left.min(CHUNK as u64) as usize;
        src.read_exact(&mut buf[..n])
            .with_context(|| format!("reading {name} to checksum it"))?;
        crcs.push(chunk_crc(&buf[..n]));
        left -= n as u64;
    }
    Ok(RegionDigest { name: name.to_string(), lba, len, crcs })
}

/// A buffer whose address is a multiple of 4096. Reads that bypass the OS cache
/// (Windows `FILE_FLAG_NO_BUFFERING`) insist on a sector-aligned address.
pub struct AlignedBuf {
    raw: Vec<u8>,
    off: usize,
    len: usize,
}

impl AlignedBuf {
    pub fn new(len: usize) -> Self {
        let raw = vec![0u8; len + 4096];
        let off = raw.as_ptr().align_offset(4096);
        Self { raw, off, len }
    }

    pub fn bytes(&mut self) -> &mut [u8] {
        &mut self.raw[self.off..self.off + self.len]
    }
}

/// Digest of what the card actually holds, read back with progress reported as
/// `verify:<name>`.
pub fn digest_card<R: Read + Seek>(
    card: &mut R,
    name: &str,
    lba: u64,
    len: u64,
    rep: &Reporter,
) -> Result<RegionDigest> {
    card.seek(SeekFrom::Start(lba * SECTOR_SIZE))
        .with_context(|| format!("seeking to LBA {lba} to read back {name}"))?;
    let mut buf = AlignedBuf::new(CHUNK);
    let progress_name = format!("verify:{name}");
    let mut crcs = Vec::new();
    let mut done = 0u64;
    while done < len {
        let n = (len - done).min(CHUNK as u64) as usize;
        let padded = pad_to_sector(n);
        let slice = &mut buf.bytes()[..padded];
        card.read_exact(slice)
            .with_context(|| format!("reading {name} back from the card at byte {done}"))?;
        crcs.push(crc32(slice));
        done += n as u64;
        rep.progress(&progress_name, done, len);
    }
    rep.progress_done(&progress_name, len);
    Ok(RegionDigest { name: name.to_string(), lba, len, crcs })
}

/// The first place the card differs from what was written, or `Ok`.
pub fn compare(expected: &[RegionDigest], actual: &[RegionDigest]) -> Result<()> {
    for want in expected {
        let got = actual
            .iter()
            .find(|d| d.name == want.name)
            .with_context(|| format!("no read-back of {}", want.name))?;
        if let Some(i) = want.crcs.iter().zip(&got.crcs).position(|(a, b)| a != b) {
            let offset = (i * CHUNK) as u64;
            return Err(CardError::new(
                "verify_failed",
                format!(
                    "{} on the card differs from the image at byte {} (LBA {}). The card did not \
                     keep what was written — try another card or reader",
                    want.name,
                    offset,
                    want.lba + offset / SECTOR_SIZE
                ),
            )
            .into());
        }
        if want.crcs.len() != got.crcs.len() {
            bail!("{}: read back {} chunks, expected {}", want.name, got.crcs.len(), want.crcs.len());
        }
    }
    Ok(())
}

/// The four regions of a card in write order, from the lengths of what goes in
/// them: (name, first LBA, bytes). boot0 is last on purpose — until it lands
/// the BootROM finds no boot0, so an interrupted write leaves a card that does
/// not boot rather than one that boots half an image.
pub fn region_list(boot0_len: u64, boot_package_len: u64, firmware_len: u64) -> [(&'static str, u64, u64); 4] {
    [
        ("firmware", FIRMWARE_LBA, firmware_len),
        ("boot_package", BOOT_PACKAGE_LBA, boot_package_len),
        ("boot_package_backup", BOOT_PACKAGE_BACKUP_LBA, boot_package_len),
        ("boot0", BOOT0_LBA, boot0_len),
    ]
}

/// After a write: read every region back from the card and compare it with what
/// was written, then judge the boot chain as the board would.
pub fn verify_written<R: Read + Seek>(card: &mut R, written: &[RegionDigest], rep: &Reporter) -> Result<()> {
    rep.step("verifying", "reading the card back to check it");
    let mut actual = Vec::new();
    for w in written {
        actual.push(digest_card(card, &w.name, w.lba, w.len, rep)?);
    }
    compare(written, &actual)?;
    rep.step("content_ok", "the card holds exactly what was written");
    check_card_structure(card).map_err(|e| CardError::new("structure_invalid", format!("{e:#}")))?;
    rep.step("structure_ok", "boot0, boot_package and firmware headers are valid");
    Ok(())
}

// --- Boot-chain structure ----------------------------------------------------

fn u32_at(b: &[u8], off: usize) -> u32 {
    b.get(off..off + 4)
        .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
        .unwrap_or(0)
}

/// Where boot0's header keeps its length. Not 20: that is u-boot's header
/// (`spare_boot_ctrl_head`, which has an `align_size` first). boot0 uses
/// `boot_file_head_t`, whose length follows the checksum directly.
const BOOT0_LENGTH_OFFSET: usize = 16;

/// boot0 as the build produced it: the eGON.BT0 header and a checksum over the
/// whole length it declares. A sanity check on the image before anything is
/// written; the card's boot0 is judged by [`check_card_boot0`].
pub fn check_shipped_boot0(buf: &[u8]) -> Result<()> {
    if buf.get(4..12) != Some(&BOOT0_MAGIC[..]) {
        bail!("boot0 does not start with the eGON.BT0 header");
    }
    let length = u32_at(buf, BOOT0_LENGTH_OFFSET) as usize;
    if length < 0x100 || length > buf.len() {
        bail!("boot0 header length {length} does not fit its data ({} bytes)", buf.len());
    }
    if !sunxi_head::checksum_ok(buf, length, sunxi_head::CHECK_SUM_OFFSET) {
        bail!("boot0 checksum is wrong");
    }
    Ok(())
}

/// boot0 as a fusing card carries it: byte-identical to the shipped
/// `boot0_sdcard.fex`, confirmed against a card PhoenixCard wrote (a full
/// 61440-byte diff showed zero differences). The SD-card build of boot0
/// already ships with the card work mode and a valid checksum baked in by
/// the SDK — patching `offset 224` (verified only against u-boot's header,
/// never against boot0's) and recomputing the checksum over the wrong
/// `length` field corrupts both and makes the BootROM reject it, which is
/// why a card written that way silently falls through to eMMC instead of
/// entering `WORK_MODE_CARD_PRODUCT`. Do not patch it.
pub fn card_boot0(shipped: &[u8]) -> Result<Vec<u8>> {
    check_shipped_boot0(shipped).context("the image's boot0")?;
    Ok(shipped.to_vec())
}

/// What the card's boot0 must look like: the same thing [`check_shipped_boot0`]
/// checks, since the card carries it unpatched.
pub fn check_card_boot0(buf: &[u8]) -> Result<()> {
    check_shipped_boot0(buf)
}

/// Where the `u-boot` item sits inside a TOC1: (offset, length).
fn uboot_item(buf: &[u8]) -> Result<(usize, usize)> {
    let items = u32_at(buf, 32) as usize;
    for i in 0..items {
        let at = TOC1_HEAD_LEN + i * TOC1_ITEM_LEN;
        let name = buf.get(at..at + 64).context("item table runs past the data")?;
        let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(64)];
        if name == b"u-boot" {
            return Ok((u32_at(buf, at + 64) as usize, u32_at(buf, at + 68) as usize));
        }
    }
    bail!("boot_package has no u-boot item")
}

/// What the card sprite requires of the TOC1 boot_package: magic, checksum over
/// `valid_len`, items that lie inside it, and a u-boot item whose own header
/// checksum holds. The card carries it exactly as the image has it.
pub fn check_toc1(buf: &[u8]) -> Result<()> {
    if u32_at(buf, 16) != TOC1_MAGIC {
        bail!("boot_package is not a TOC1 (magic {:#x})", u32_at(buf, 16));
    }
    if u32_at(buf, 60) != TOC1_END {
        bail!("boot_package header is not terminated correctly");
    }
    let valid_len = u32_at(buf, 36) as usize;
    if valid_len < TOC1_HEAD_LEN || valid_len > buf.len() {
        bail!("boot_package valid_len {valid_len} does not fit its data ({} bytes)", buf.len());
    }
    if !sunxi_head::checksum_ok(buf, valid_len, 20) {
        bail!("boot_package checksum is wrong (the card sprite would report 'toc1 checksum is error')");
    }
    let items = u32_at(buf, 32) as usize;
    if TOC1_HEAD_LEN + items * TOC1_ITEM_LEN > valid_len {
        bail!("boot_package lists {items} items but is only {valid_len} bytes");
    }
    for i in 0..items {
        let at = TOC1_HEAD_LEN + i * TOC1_ITEM_LEN;
        let (offset, len) = (u32_at(buf, at + 64) as usize, u32_at(buf, at + 68) as usize);
        if offset.checked_add(len).map_or(true, |end| end > valid_len) {
            bail!("boot_package item {i} ({offset}+{len}) runs past valid_len {valid_len}");
        }
    }

    let (offset, len) = uboot_item(buf)?;
    let uboot = &buf[offset..offset + len];
    if uboot.get(4..9) != Some(&b"uboot"[..]) {
        bail!("the u-boot item has no u-boot header (the card sprite: 'uboot magic is error')");
    }
    let declared = u32_at(uboot, sunxi_head::LENGTH_OFFSET) as usize;
    if declared != len || !sunxi_head::checksum_ok(uboot, len, sunxi_head::CHECK_SUM_OFFSET) {
        bail!("the u-boot item's own header checksum is wrong");
    }
    Ok(())
}

fn read_at<R: Read + Seek>(card: &mut R, lba: u64, len: usize) -> Result<Vec<u8>> {
    card.seek(SeekFrom::Start(lba * SECTOR_SIZE))?;
    let mut buf = AlignedBuf::new(pad_to_sector(len));
    let slice = buf.bytes();
    card.read_exact(slice)
        .with_context(|| format!("reading LBA {lba} from the card"))?;
    Ok(slice[..len].to_vec())
}

/// The boot chain as it sits on the card, judged the way the board would: no
/// image needed. Reads each header first to learn how much to fetch.
pub fn check_card_structure<R: Read + Seek>(card: &mut R) -> Result<()> {
    let head = read_at(card, BOOT0_LBA, SECTOR_SIZE as usize)?;
    let boot0_len = u32_at(&head, BOOT0_LENGTH_OFFSET) as usize;
    if boot0_len == 0 || boot0_len > 1 << 20 {
        bail!("card sector {BOOT0_LBA} holds no boot0 (header length {boot0_len})");
    }
    check_card_boot0(&read_at(card, BOOT0_LBA, boot0_len)?).context("boot0 on the card")?;

    for (lba, what) in [
        (BOOT_PACKAGE_LBA, "boot_package"),
        (BOOT_PACKAGE_BACKUP_LBA, "boot_package backup"),
    ] {
        let head = read_at(card, lba, SECTOR_SIZE as usize)?;
        let valid = u32_at(&head, 36) as usize;
        if valid < TOC1_HEAD_LEN || valid > 16 << 20 {
            bail!("card sector {lba} holds no {what} (valid_len {valid})");
        }
        check_toc1(&read_at(card, lba, valid)?).with_context(|| format!("{what} on the card"))?;
    }

    let fw = read_at(card, FIRMWARE_LBA, SECTOR_SIZE as usize)?;
    if fw.get(..8) != Some(&IMAGEWTY_MAGIC[..]) {
        bail!("card sector {FIRMWARE_LBA} does not hold a firmware image (no IMAGEWTY header)");
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn crc32_matches_the_standard_test_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn a_short_chunk_is_checksummed_as_stored_zero_padded() {
        let mut padded = vec![7u8; 700];
        padded.resize(1024, 0);
        assert_eq!(chunk_crc(&[7u8; 700]), crc32(&padded));
    }

    /// A header that passes the same check as the real thing.
    fn with_sum(mut b: Vec<u8>, length: usize, sum_offset: usize) -> Vec<u8> {
        b[sum_offset..sum_offset + 4].copy_from_slice(&sunxi_head::STAMP_VALUE.to_le_bytes());
        let sum = b[..length]
            .chunks_exact(4)
            .fold(0u32, |s, w| s.wrapping_add(u32::from_le_bytes(w.try_into().unwrap())));
        b[sum_offset..sum_offset + 4].copy_from_slice(&sum.to_le_bytes());
        b
    }

    /// boot0 as the build ships it: `length` at 16 covers everything, and the
    /// sum covers all of it too.
    pub fn shipped_boot0() -> Vec<u8> {
        let mut b = vec![0x11u8; 4096];
        b[4..12].copy_from_slice(BOOT0_MAGIC);
        b[16..20].copy_from_slice(&4096u32.to_le_bytes());
        b[20..24].copy_from_slice(&48u32.to_le_bytes()); // pub_head_size
        with_sum(b, 4096, 12)
    }

    /// boot0 as the card carries it.
    pub fn good_boot0() -> Vec<u8> {
        card_boot0(&shipped_boot0()).unwrap()
    }

    const UBOOT_AT: usize = TOC1_HEAD_LEN + TOC1_ITEM_LEN;
    const UBOOT_LEN: usize = 2048;

    /// A TOC1 with one `u-boot` item.
    pub fn good_toc1() -> Vec<u8> {
        let len = UBOOT_AT + UBOOT_LEN;
        let mut b = vec![0u8; len];
        b[16..20].copy_from_slice(&TOC1_MAGIC.to_le_bytes());
        b[32..36].copy_from_slice(&1u32.to_le_bytes());
        b[36..40].copy_from_slice(&(len as u32).to_le_bytes());
        b[60..64].copy_from_slice(&TOC1_END.to_le_bytes());
        let item = TOC1_HEAD_LEN;
        b[item..item + 6].copy_from_slice(b"u-boot");
        b[item + 64..item + 68].copy_from_slice(&(UBOOT_AT as u32).to_le_bytes());
        b[item + 68..item + 72].copy_from_slice(&(UBOOT_LEN as u32).to_le_bytes());

        let mut uboot = vec![0x22u8; UBOOT_LEN];
        uboot[4..9].copy_from_slice(b"uboot");
        uboot[20..24].copy_from_slice(&(UBOOT_LEN as u32).to_le_bytes());
        let uboot = with_sum(uboot, UBOOT_LEN, 12);
        b[UBOOT_AT..].copy_from_slice(&uboot);
        with_sum(b, len, 20)
    }

    #[test]
    fn a_valid_boot0_and_toc1_pass() {
        check_shipped_boot0(&shipped_boot0()).unwrap();
        check_card_boot0(&good_boot0()).unwrap();
        check_toc1(&good_toc1()).unwrap();
    }

    #[test]
    fn the_card_boot0_is_byte_identical_to_shipped() {
        // Confirmed against a real PhoenixCard-written card: a full diff of
        // its boot0 against the image's unpatched boot0_sdcard.fex showed
        // zero differing bytes.
        assert_eq!(shipped_boot0(), good_boot0());
    }

    #[test]
    fn boot0_failures_are_named() {
        let mut bad_magic = shipped_boot0();
        bad_magic[4] = b'X';
        assert!(check_shipped_boot0(&bad_magic).unwrap_err().to_string().contains("eGON.BT0"));
        assert!(check_card_boot0(&bad_magic).unwrap_err().to_string().contains("eGON.BT0"));

        let mut flipped = shipped_boot0();
        flipped[1000] ^= 1;
        assert!(check_shipped_boot0(&flipped).unwrap_err().to_string().contains("checksum"));
        assert!(check_card_boot0(&flipped).unwrap_err().to_string().contains("checksum"));
    }

    #[test]
    fn toc1_failures_are_named() {
        let mut flipped = good_toc1();
        flipped[100] ^= 1;
        assert!(check_toc1(&flipped).unwrap_err().to_string().contains("checksum"));

        let mut bad_magic = good_toc1();
        bad_magic[16] ^= 1;
        assert!(check_toc1(&bad_magic).unwrap_err().to_string().contains("TOC1"));

        let mut item_past_end = good_toc1();
        let at = TOC1_HEAD_LEN + 68;
        item_past_end[at..at + 4].copy_from_slice(&9999u32.to_le_bytes());
        let len = item_past_end.len();
        let item_past_end = with_sum(item_past_end, len, 20);
        assert!(check_toc1(&item_past_end).unwrap_err().to_string().contains("past valid_len"));
    }

    #[test]
    fn a_damaged_u_boot_header_is_caught_even_when_the_toc1_sum_is_redone() {
        let mut b = good_toc1();
        b[UBOOT_AT + 500] ^= 1; // inside the u-boot item, its own sum now wrong
        let len = b.len();
        let b = with_sum(b, len, 20); // ...but the TOC1 sum was recomputed over it
        assert!(check_toc1(&b).unwrap_err().to_string().contains("u-boot item"));
    }

    #[test]
    fn a_boot_package_without_a_u_boot_is_refused() {
        let mut b = good_toc1();
        b[TOC1_HEAD_LEN] = b'x'; // the item is no longer called u-boot
        let len = b.len();
        let b = with_sum(b, len, 20);
        assert!(check_toc1(&b).is_err());
    }

    fn digest_of(data: &[u8], lba: u64) -> RegionDigest {
        digest_source("firmware", lba, data.len() as u64, &mut &data[..]).unwrap()
    }

    #[test]
    fn a_flipped_byte_is_reported_at_its_chunk_and_lba() {
        let good = vec![5u8; CHUNK * 3 + 100];
        let mut bad = good.clone();
        bad[CHUNK * 2 + 17] ^= 0xFF;
        let err = compare(&[digest_of(&good, 40960)], &[digest_of(&bad, 40960)]).unwrap_err();
        let card = err.downcast_ref::<CardError>().expect("a CardError");
        assert_eq!(card.kind, "verify_failed");
        let msg = err.to_string();
        assert!(msg.contains(&format!("byte {}", CHUNK * 2)), "{msg}");
        assert!(msg.contains(&format!("LBA {}", 40960 + (CHUNK * 2) as u64 / 512)), "{msg}");
        compare(&[digest_of(&good, 40960)], &[digest_of(&good, 40960)]).unwrap();
    }

    #[test]
    fn reading_the_card_back_matches_the_source_digest() {
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let mut card = Cursor::new(vec![0u8; 2_000_000]);
        card.set_position(1024 * 512);
        std::io::Write::write_all(&mut card, &data).unwrap();
        // 300000 is not a sector multiple: the stored tail is zero-padded.
        let rep = Reporter::new(false);
        let read = digest_card(&mut card, "firmware", 1024, data.len() as u64, &rep).unwrap();
        compare(&[digest_of(&data, 1024)], &[read]).unwrap();
    }

    #[test]
    fn the_whole_card_structure_checks_out_and_a_flip_is_caught() {
        let boot0 = good_boot0();
        let toc1 = good_toc1();
        let mut card = vec![0u8; (FIRMWARE_LBA as usize + 8) * 512];
        let put = |card: &mut Vec<u8>, lba: u64, data: &[u8]| {
            let at = (lba * 512) as usize;
            card[at..at + data.len()].copy_from_slice(data);
        };
        put(&mut card, BOOT0_LBA, &boot0);
        put(&mut card, BOOT_PACKAGE_LBA, &toc1);
        put(&mut card, BOOT_PACKAGE_BACKUP_LBA, &toc1);
        put(&mut card, FIRMWARE_LBA, IMAGEWTY_MAGIC);
        check_card_structure(&mut Cursor::new(card.clone())).unwrap();

        let mut no_fw = card.clone();
        no_fw[(FIRMWARE_LBA * 512) as usize] = 0;
        assert!(check_card_structure(&mut Cursor::new(no_fw)).is_err());

        let mut bad_backup = card;
        bad_backup[(BOOT_PACKAGE_BACKUP_LBA * 512) as usize + 100] ^= 1;
        let err = check_card_structure(&mut Cursor::new(bad_backup)).unwrap_err();
        assert!(format!("{err:#}").contains("backup"), "{err:#}");
    }
}
