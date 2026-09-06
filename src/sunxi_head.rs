//! Patch and re-checksum the `spare_boot_ctrl_head` / `spare_boot_data_head`
//! that prefixes boot0/u-boot payloads.
//!
//! Layout and offsets verified against the real T527 SDK
//! (u-boot-2018/include/private_uboot.h) and cross-checked byte-for-byte
//! against both the raw build output and the final packed production image
//! — see docs/T527-T507-FEL-EFEX-기술조사.md section 3-4.
//!
//! ```text
//! spare_boot_ctrl_head (48 bytes, offset 0):
//!   +0   jump_instruction   u32
//!   +4   magic[8]                    "uboot" / "eGON.BT0" ...
//!   +12  check_sum          u32      <- recomputed by fix_checksum()
//!   +16  align_size         u32
//!   +20  length             u32      <- checksum covers bytes [0, length)
//!   +24  uboot_length       u32
//!   +28  version[8]
//!   +36  platform[8]
//!   +44  reserved           i32
//! spare_boot_data_head (offset 48):
//!   +48+176 = 224   work_mode   i32  <- WORK_MODE_BOOT=0x00, WORK_MODE_USB_PRODUCT=0x10
//! ```

use anyhow::{bail, Result};

pub const CHECK_SUM_OFFSET: usize = 12;
pub const LENGTH_OFFSET: usize = 20;
pub const WORK_MODE_OFFSET: usize = 224;

pub const STAMP_VALUE: u32 = 0x5F0A_6C39;

pub const WORK_MODE_BOOT: u32 = 0x00;
pub const WORK_MODE_USB_PRODUCT: u32 = 0x10;
pub const WORK_MODE_CARD_PRODUCT: u32 = 0x11;
pub const WORK_MODE_USB_DEBUG: u32 = 0x12;

/// Read the current work_mode value.
pub fn read_work_mode(buf: &[u8]) -> Result<u32> {
    if buf.len() < WORK_MODE_OFFSET + 4 {
        bail!("buffer too small to contain work_mode field");
    }
    Ok(u32::from_le_bytes(
        buf[WORK_MODE_OFFSET..WORK_MODE_OFFSET + 4].try_into().unwrap(),
    ))
}

/// Patch work_mode in place. Does NOT recompute the checksum — call
/// `fix_checksum` afterwards.
pub fn set_work_mode(buf: &mut [u8], mode: u32) -> Result<()> {
    if buf.len() < WORK_MODE_OFFSET + 4 {
        bail!("buffer too small to contain work_mode field");
    }
    buf[WORK_MODE_OFFSET..WORK_MODE_OFFSET + 4].copy_from_slice(&mode.to_le_bytes());
    Ok(())
}

/// Port of `gen_check_sum()` from u-boot-2018/tools/mksunxiboot.c:
/// sum every little-endian u32 word across `length` bytes, with the
/// check_sum field itself held at STAMP_VALUE while summing.
pub fn fix_checksum(buf: &mut [u8]) -> Result<u32> {
    if buf.len() < LENGTH_OFFSET + 4 {
        bail!("buffer too small to contain length field");
    }
    let length = u32::from_le_bytes(buf[LENGTH_OFFSET..LENGTH_OFFSET + 4].try_into().unwrap());
    if length % 4 != 0 {
        bail!("length field ({length}) is not 4-byte aligned");
    }
    let length = length as usize;
    if buf.len() < length {
        bail!(
            "buffer ({} bytes) shorter than header's length field ({} bytes)",
            buf.len(),
            length
        );
    }

    buf[CHECK_SUM_OFFSET..CHECK_SUM_OFFSET + 4].copy_from_slice(&STAMP_VALUE.to_le_bytes());

    let mut sum: u32 = 0;
    for chunk in buf[..length].chunks_exact(4) {
        sum = sum.wrapping_add(u32::from_le_bytes(chunk.try_into().unwrap()));
    }

    buf[CHECK_SUM_OFFSET..CHECK_SUM_OFFSET + 4].copy_from_slice(&sum.to_le_bytes());
    Ok(sum)
}

pub fn read_checksum(buf: &[u8]) -> Result<u32> {
    if buf.len() < CHECK_SUM_OFFSET + 4 {
        bail!("buffer too small to contain check_sum field");
    }
    Ok(u32::from_le_bytes(
        buf[CHECK_SUM_OFFSET..CHECK_SUM_OFFSET + 4].try_into().unwrap(),
    ))
}
