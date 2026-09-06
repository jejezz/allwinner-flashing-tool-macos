//! FEL → EFEX bootstrap.
//!
//! Getting a board from the boot ROM's FEL mode into EFEX (where the flashing
//! commands live) takes a two-stage upload, both staged straight out of the
//! IMAGEWTY image:
//!
//!   1. `fes1.fex`   — patched to work_mode=USB_PRODUCT, uploaded to
//!                     `FES1_ADDR` and executed. Initialises DRAM (~6 s).
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

use anyhow::{bail, Context, Result};
use std::time::{Duration, Instant};

use crate::efex::{EfexDevice, MODE_SRV};
use crate::event::Reporter;
use crate::fel::FelDevice;
use crate::imagewty::ImageWty;
use crate::sunxi_head;

/// fes1 load/run address for sun55iw3 (T527/A523).
pub const FES1_ADDR: u32 = 0x0004_C000;
/// u-boot load/run address for sun55iw3 (T527/A523).
pub const UBOOT_ADDR: u32 = 0x4A00_0000;

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
pub fn fel_to_efex(
    image: &ImageWty,
    fes1_addr: u32,
    uboot_addr: u32,
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
