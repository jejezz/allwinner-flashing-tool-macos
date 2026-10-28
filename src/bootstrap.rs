//! FEL → EFEX bootstrap.
//!
//! Getting a board from the boot ROM's FEL mode into EFEX (where the flashing
//! commands live) takes a two-stage upload, both staged straight out of the
//! IMAGEWTY image:
//!
//!   1. `fes1.fex`   — patched to work_mode=USB_PRODUCT, uploaded to a
//!                     SoC-specific fes1 address and executed. Initialises
//!                     DRAM (~6 s).
//!   2. `u-boot.fex` — same patch, uploaded to `UBOOT_ADDR` and executed.
//!                     The chip re-enumerates and comes up in EFEX.
//!
//! Both files get work_mode patched (offset 224) and their checksum recomputed
//! — see `sunxi_head`. Note these patched copies are *primers* that only ever
//! live in DRAM; the BOOT0/BOOT1 written to storage during flashing must be
//! the untouched originals (docs section 11.2).
//!
//! Load addresses are SoC-specific and were taken from the vendor sources
//! rather than guessed: `include/configs/sun55iw3p1.h` gives
//! `CONFIG_FES1_RUN_ADDR = 0x44000 + 0x8000 = 0x4C000` (confirmed by
//! `fes/fes1.lds`: ". = 0x4c000"), and `u-boot.lds` / `.config`
//! (`CONFIG_SYS_TEXT_BASE`) both give 0x4A000000. Passing fes1 to the address
//! `sunxi-fel spl` assumes (0x44000, which is boot0's) hangs the board.
//!
//! H616/T507 (soc_id 0x1823, same die family) uses a different fes1 address:
//! `spl-pub/board/{h616,t507}/common.mk` gives `CFG_FES1_RUN_ADDR = 0x28000`.
//! Using the sun55iw3 address (0x4C000) on this SoC writes fes1 into
//! unmapped SRAM, which the BROM never acknowledges — the symptom is a
//! `usb_bulk_send: Operation timed out` on the fes1 upload. u-boot's load
//! address (`CONFIG_SYS_TEXT_BASE`) is 0x4A000000 on both families, so it
//! does not need a per-SoC branch.
//!
//! `fel_to_efex` reads `soc_id` off the device itself (via `FelDevice::
//! get_version`) and picks the right fes1 address automatically; pass
//! `Some(addr)` to override for an SoC not in `default_fes1_addr`.

use anyhow::{bail, Context, Result};
use std::time::{Duration, Instant};

use crate::efex::{EfexDevice, MODE_SRV};
use crate::event::Reporter;
use crate::fel::FelDevice;
use crate::imagewty::ImageWty;
use crate::sunxi_head;

/// fes1 load/run address for sun55iw3 (T527/A523).
pub const FES1_ADDR_SUN55IW3: u32 = 0x0004_C000;
/// fes1 load/run address for sun50iw9 (T507/H616).
pub const FES1_ADDR_SUN50IW9: u32 = 0x0002_8000;
/// u-boot load/run address — shared by sun55iw3 and sun50iw9.
pub const UBOOT_ADDR: u32 = 0x4A00_0000;

/// `soc_id` as reported by `FelDevice::get_version` (BROM version query).
const SOC_ID_A523: u32 = 0x1890;
const SOC_ID_H616: u32 = 0x1823;

/// Pick the fes1 load/run address for a `soc_id` read off the device.
fn default_fes1_addr(soc_id: u32) -> Result<u32> {
    match soc_id {
        SOC_ID_A523 => Ok(FES1_ADDR_SUN55IW3),
        SOC_ID_H616 => Ok(FES1_ADDR_SUN50IW9),
        other => bail!(
            "unknown soc_id 0x{other:04x} — fes1 load address not known for this SoC; \
             pass --fes1-addr/--uboot-addr explicitly (see spl-pub/board/*/common.mk \
             CFG_FES1_RUN_ADDR in the vendor SDK)"
        ),
    }
}

/// Is a device already sitting in EFEX?
///
/// FEL and EFEX share VID:PID and the same CBW envelope, so the probe has to
/// look at the reported mode: FEL answers a version request with 0x01, EFEX's
/// verify_dev answers 0x02 (MODE_SRV). Probing a FEL device this way is
/// harmless — it just reads back a version.
pub fn in_efex_mode() -> bool {
    match EfexDevice::open() {
        Ok(mut dev) => matches!(dev.verify_dev(), Ok(info) if info.mode == MODE_SRV),
        Err(_) => false,
    }
}

/// Take a board from FEL to EFEX, staging both primers out of `image`.
/// Returns without doing anything if the device is already in EFEX.
///
/// `fes1_addr`/`uboot_addr` of `None` auto-detect from the device's
/// `soc_id` (see `default_fes1_addr`); pass `Some(addr)` to override.
pub fn fel_to_efex(
    image: &ImageWty,
    fes1_addr: Option<u32>,
    uboot_addr: Option<u32>,
    rep: &Reporter,
) -> Result<()> {
    if in_efex_mode() {
        rep.step("bootstrap_skipped", "bootstrap: device already in EFEX mode, skipping");
        return Ok(());
    }

    let fel = FelDevice::open().context(
        "no FEL device found — hold the FEL button/pin while connecting power, \
         or the board may already be past FEL",
    )?;
    let version = fel.get_version()?;
    rep.step(
        "fel_found",
        format!("bootstrap: FEL device found (soc_id=0x{:04x})", version.soc_id),
    );

    let fes1_addr = match fes1_addr {
        Some(a) => a,
        None => default_fes1_addr(version.soc_id)?,
    };
    let uboot_addr = uboot_addr.unwrap_or(UBOOT_ADDR);

    let fes1 = primer(image, "fes1.fex")?;
    fel.write_memory(fes1_addr, &fes1)
        .with_context(|| format!("uploading fes1 primer to {fes1_addr:#x}"))?;
    fel.execute(fes1_addr)?;
    rep.step(
        "fes1_running",
        format!(
            "bootstrap: fes1 running at {fes1_addr:#x} ({} bytes), initialising DRAM...",
            fes1.len()
        ),
    );
    drop(fel);

    let fel = wait_for_fel(Duration::from_secs(30))
        .context("FEL did not come back after running fes1 (DRAM init failed?)")?;

    let uboot = primer(image, "u-boot.fex")?;
    fel.write_memory(uboot_addr, &uboot)
        .with_context(|| format!("uploading u-boot primer to {uboot_addr:#x}"))?;
    fel.execute(uboot_addr)?;
    rep.step(
        "uboot_running",
        format!(
            "bootstrap: u-boot running at {uboot_addr:#x} ({} bytes), waiting for EFEX...",
            uboot.len()
        ),
    );
    drop(fel);

    wait_for_efex(Duration::from_secs(30))
        .context("device never reached EFEX mode after running the u-boot primer")?;
    rep.step("efex_reached", "bootstrap: EFEX mode reached");
    Ok(())
}

/// Pull an item out of the image and patch it into a FEL primer:
/// work_mode = USB_PRODUCT, checksum recomputed.
fn primer(image: &ImageWty, filename: &str) -> Result<Vec<u8>> {
    let item = image
        .find(filename)
        .with_context(|| format!("{filename} not found in image"))?;
    let mut data = image.read_item(item)?;
    sunxi_head::set_work_mode(&mut data, sunxi_head::WORK_MODE_USB_PRODUCT)
        .with_context(|| format!("patching work_mode in {filename}"))?;
    sunxi_head::fix_checksum(&mut data)
        .with_context(|| format!("recomputing checksum for {filename}"))?;
    Ok(data)
}

fn wait_for_fel(timeout: Duration) -> Result<FelDevice> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(dev) = FelDevice::open() {
            if dev.get_version().is_ok() {
                return Ok(dev);
            }
        }
        if Instant::now() >= deadline {
            bail!("timed out after {:?} waiting for the FEL device", timeout);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn wait_for_efex(timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if in_efex_mode() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("timed out after {:?} waiting for EFEX mode", timeout);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
