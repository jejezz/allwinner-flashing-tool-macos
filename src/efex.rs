//! EFEX protocol client (VID:PID 1f3a:efe8, same as FEL).
//!
//! Wire format and command layout taken directly from the vendor's own
//! server-side implementation:
//!   u-boot-2018/drivers/sunxi_usb/usb_efex.h  (structs, opcodes, tags)
//!   u-boot-2018/drivers/sunxi_usb/usb_efex.c  (dispatch logic, confirms
//!     fes_down's `type` field selects MBR/BOOT1/BOOT0/ERASE handling,
//!     and that MBR/BOOT1/BOOT0 map to sunxi_sprite_download_{mbr,uboot,boot0})
//!
//! The CBW/CSW transport envelope is byte-identical to plain FEL's
//! AW_USB_REQUEST/AW_USB_RESPONSE (see fel.rs) — same 32-byte command
//! envelope with "AWUC" magic, same 13-byte "AWUS" status reply. EFEX just
//! additionally uses the `tag` and `cmd_len` fields that FEL's client
//! leaves at fixed values. See docs/T527-T507-FEL-EFEX-기술조사.md section 11.6.

// The opcode/tag tables below are transcribed in full from the vendor's
// usb_efex.h so the command set stays documented in one place; the flashing
// path only exercises a subset.
#![allow(dead_code)]

use anyhow::{anyhow, bail, Context, Result};
use rusb::{DeviceHandle, GlobalContext};
use std::time::Duration;

const VENDOR_ID: u16 = 0x1f3a;
const PRODUCT_ID: u16 = 0xefe8;

const CBW_MAGIC: u32 = 0x4355_5741; // "AWUC"
const CSW_MAGIC: u32 = 0x5355_5741; // "AWUS"
const CSW_LEN: usize = 13;

// Names/values match usb_efex.h exactly, from the DEVICE's perspective:
// TRANSMIT = device transmits = host receives (bulk IN).
// RECEIVE  = device receives  = host sends (bulk OUT).
// (Cross-checked against fel_lib.c: AW_USB_WRITE=0x12 is used there whenever
// the host is about to bulk_send, i.e. exactly TL_CMD_RECEIVE's value.)
const TL_CMD_TRANSMIT: u8 = 0x11;
const TL_CMD_RECEIVE: u8 = 0x12;

const USB_TIMEOUT: Duration = Duration::from_secs(30);

// ---- APP-layer common commands ----
pub const APP_CMD_VERIFY_DEV: u16 = 0x0001;
pub const APP_CMD_SWITCH_ROLE: u16 = 0x0002;
pub const APP_CMD_IS_READY: u16 = 0x0003;
pub const APP_CMD_GET_CMD_SET_VER: u16 = 0x0004;
pub const APP_CMD_DISCONNECT: u16 = 0x0010;

// ---- FEX_CMD_* opcodes ----
pub const FEX_CMD_FES_TRANS: u16 = 0x0201;
pub const FEX_CMD_FES_RUN: u16 = 0x0202;
pub const FEX_CMD_FES_DOWN: u16 = 0x0206;
pub const FEX_CMD_FES_UP: u16 = 0x0207;
pub const FEX_CMD_FES_QUERY_STORAGE: u16 = 0x0209;
pub const FEX_CMD_FES_VERIFY_VALUE: u16 = 0x020C;
pub const FEX_CMD_FES_VERIFY_STATUS: u16 = 0x020D;
pub const FEX_CMD_FES_TOOL_MODE: u16 = 0x020F;
pub const FEX_CMD_FES_FLASH_SET_ON: u16 = 0x020A;
pub const FEX_CMD_FES_FLASH_SET_OFF: u16 = 0x020B;

/// spare_head.h WORK_MODE_USB_TOOL_PRODUCT — selects the "mass production
/// tool" branch of fes_tool_mode's handler (not the "upgrade tool" or
/// "erase key" branches).
const WORK_MODE_USB_TOOL_PRODUCT: u32 = 0x04;

// ---- fes_down `type` tags (SUNXI_EFEX_*_TAG) ----
pub const TAG_DRAM: u32 = 0x7f00;
pub const TAG_MBR: u32 = 0x7f01;
pub const TAG_BOOT1: u32 = 0x7f02;
pub const TAG_BOOT0: u32 = 0x7f03;
pub const TAG_ERASE: u32 = 0x7f04;
const DRAM_MASK: u32 = 0x7f00;
/// OR this into `type` on the LAST (or only) fes_down chunk of a transfer;
/// without it the device just accumulates the chunk into an internal buffer
/// instead of committing it to the destination (see usb_efex.c's fes_down
/// handler: only `type == (DRAM_MASK | TRANS_FINISH_TAG)` sets
/// `dram_trans_buffer = addr` for a plain DRAM write).
pub const TRANS_FINISH_TAG: u32 = 0x10000;

// ---- verify_dev response modes ----
pub const MODE_NULL: u16 = 0x00;
pub const MODE_FEL: u16 = 0x01;
pub const MODE_SRV: u16 = 0x02;

pub struct EfexDevice {
    handle: DeviceHandle<GlobalContext>,
    ep_in: u8,
    ep_out: u8,
    next_tag: u32,
}

#[derive(Debug)]
pub struct VerifyDevInfo {
    pub tag: [u8; 8], // "AWUSBFEX"
    pub platform_id_hw: u32,
    pub platform_id_fw: u32,
    pub mode: u16,
}

impl EfexDevice {
    /// Open the device. Call this AFTER the primer u-boot has reached
    /// `run usb efex` — the chip re-enumerates at this point, so any
    /// previously-open FEL handle is stale and must be reopened fresh.
    pub fn open() -> Result<Self> {
        let handle = rusb::open_device_with_vid_pid(VENDOR_ID, PRODUCT_ID)
            .ok_or_else(|| anyhow!("EFEX device (1f3a:efe8) not found — is it past 'run usb efex'?"))?;
        handle.claim_interface(0).context("claiming USB interface 0")?;

        let device = handle.device();
        let config = device.active_config_descriptor()?;
        let mut ep_in = None;
        let mut ep_out = None;
        for iface in config.interfaces() {
            for setting in iface.descriptors() {
                for ep in setting.endpoint_descriptors() {
                    if ep.transfer_type() != rusb::TransferType::Bulk {
                        continue;
                    }
                    match ep.direction() {
                        rusb::Direction::In => ep_in = Some(ep.address()),
                        rusb::Direction::Out => ep_out = Some(ep.address()),
                    }
                }
            }
        }

        Ok(Self {
            handle,
            ep_in: ep_in.context("no bulk IN endpoint")?,
            ep_out: ep_out.context("no bulk OUT endpoint")?,
            next_tag: 1,
        })
    }

    fn bulk_send(&self, data: &[u8]) -> Result<()> {
        let mut sent = 0;
        while sent < data.len() {
            sent += self
                .handle
                .write_bulk(self.ep_out, &data[sent..], USB_TIMEOUT)
                .context("bulk_send")?;
        }
        Ok(())
    }

    fn bulk_recv(&self, buf: &mut [u8]) -> Result<()> {
        let mut recv = 0;
        while recv < buf.len() {
            recv += self
                .handle
                .read_bulk(self.ep_in, &mut buf[recv..], USB_TIMEOUT)
                .context("bulk_recv")?;
        }
        Ok(())
    }

    fn next_tag(&mut self) -> u32 {
        let t = self.next_tag;
        self.next_tag = self.next_tag.wrapping_add(1);
        t
    }

    /// Build and send a 32-byte CBW, matching `struct sunxi_efex_cbw_t`.
    fn send_cbw(&self, tag: u32, direction: u8, data_len: u32) -> Result<()> {
        let mut cbw = [0u8; 32];
        cbw[0..4].copy_from_slice(&CBW_MAGIC.to_le_bytes());
        cbw[4..8].copy_from_slice(&tag.to_le_bytes());
        cbw[8..12].copy_from_slice(&data_len.to_le_bytes());
        // reserved_1(2) + reserved_2(1) at [12..15] stay zero
        cbw[15] = 0x0c; // cmd_len, matches the fixed value FEL uses (0x0c000000 as unknown1)
                        // cmd_package: direction(1) + resv(1) + dataLen(4) + resv2(10), at [16..32]
        cbw[16] = direction;
        cbw[18..22].copy_from_slice(&data_len.to_le_bytes());
        self.bulk_send(&cbw)
    }

    /// Read and validate the 13-byte CSW, matching `struct sunxi_efex_csw_t`.
    fn read_csw(&self) -> Result<u8> {
        let mut buf = [0u8; CSW_LEN];
        self.bulk_recv(&mut buf)?;
        let magic = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        if magic != CSW_MAGIC {
            bail!("bad CSW magic: {:#010x}", magic);
        }
        Ok(buf[12]) // status: CSW_STATUS_PASS=0 / CSW_STATUS_FAIL=1
    }

    /// Send `data` as the payload of a host-sends transfer (CBW + bulk-out + CSW).
    fn write_payload(&mut self, data: &[u8]) -> Result<()> {
        let tag = self.next_tag();
        self.send_cbw(tag, TL_CMD_RECEIVE, data.len() as u32)?; // device RECEIVEs = host sends
        self.bulk_send(data)?;
        let status = self.read_csw()?;
        if status != 0 {
            bail!("device reported CSW_STATUS_FAIL after write");
        }
        Ok(())
    }

    /// Read back `len` bytes as the payload of a host-receives transfer.
    fn read_payload(&mut self, len: usize) -> Result<Vec<u8>> {
        let tag = self.next_tag();
        self.send_cbw(tag, TL_CMD_TRANSMIT, len as u32)?; // device TRANSMITs = host receives
        let mut buf = vec![0u8; len];
        self.bulk_recv(&mut buf)?;
        let status = self.read_csw()?;
        if status != 0 {
            bail!("device reported CSW_STATUS_FAIL after read");
        }
        Ok(buf)
    }

    /// MUST be called after every complete command (whether it ends in a
    /// send-data or receive-data phase) before issuing the next one.
    ///
    /// Root cause (found by tracing sunxi_efex_state_loop()): once a command's
    /// data phase finishes, the code sets `sunxi_usb_efex_app_step =
    /// SUNXI_USB_EFEX_APPS_STATUS` but the STATUS-state handler that actually
    /// sends the CSW never resets `app_step` back to APPS_IDLE. The *next*
    /// command's CBW naturally has direction=RECEIVE (it's sending new cmd
    /// bytes to the device) — but while app_step is stuck at APPS_STATUS, the
    /// SETUP-state dispatcher only accepts direction=TRANSMIT there (see the
    /// `else if (app_step == SUNXI_USB_EFEX_APPS_STATUS)` branch), rejecting
    /// RECEIVE with "usb transfer direction is transmit only\n" and no reply
    /// — which is exactly the timeout every second-command-in-a-session hit.
    ///
    /// The fix: send one extra CBW with direction=TRANSMIT declaring 8 bytes
    /// (matching `__sunxi_usb_efex_fill_status()`'s `Status_t`, 8 bytes) —
    /// this both satisfies that direction check (resetting app_step to
    /// APPS_IDLE immediately) and drives one more SEND_DATA+STATUS cycle,
    /// after which the device is finally ready for a fresh command.
    fn flush_status(&mut self) -> Result<()> {
        self.read_payload(8)?;
        Ok(())
    }

    /// APP_LAYER_COMMEN_CMD_VERIFY_DEV: query device identity/current mode.
    /// Request: struct verify_dev_cmd_s (16B) = app_cmd+tag+reserved[12]
    /// Response: struct verify_dev_data_s (32B) = tag[8]+hw(4)+fw(4)+mode(2)+...
    pub fn verify_dev(&mut self) -> Result<VerifyDevInfo> {
        let mut req = [0u8; 16];
        req[0..2].copy_from_slice(&APP_CMD_VERIFY_DEV.to_le_bytes());
        self.write_payload(&req)?;

        let resp = self.read_payload(32)?;
        let mut tag = [0u8; 8];
        tag.copy_from_slice(&resp[0..8]);
        let info = VerifyDevInfo {
            tag,
            platform_id_hw: u32::from_le_bytes(resp[8..12].try_into().unwrap()),
            platform_id_fw: u32::from_le_bytes(resp[12..16].try_into().unwrap()),
            mode: u16::from_le_bytes(resp[16..18].try_into().unwrap()),
        };
        self.flush_status()?;
        Ok(info)
    }

    /// FEX_CMD_fes_query_storage (0x0209): query storage type (4-byte response).
    /// Diagnostic value: this is a FEX_CMD_* (not APP_LAYER_COMMON_*) that,
    /// like verify_dev, just does cmd->response with no receive-data phase —
    /// isolates whether the fes_down hang is about the FEX_CMD_* namespace
    /// in general or specifically about its receive-data follow-up.
    pub fn query_storage(&mut self) -> Result<u32> {
        let mut req = [0u8; 16];
        req[0..2].copy_from_slice(&FEX_CMD_FES_QUERY_STORAGE.to_le_bytes());
        self.write_payload(&req)?;
        let resp = self.read_payload(4)?;
        let val = u32::from_le_bytes(resp[0..4].try_into().unwrap());
        self.flush_status()?;
        Ok(val)
    }

    /// FEX_CMD_fes_flash_set_on (0x020A): calls `sunxi_sprite_init(0)` on the
    /// device — the flash/MBR subsystem's own init. Hypothesis: fes_down
    /// (which writes to MBR/BOOT0/BOOT1/flash sectors, all backed by this
    /// subsystem) may hang without this being called first. No data phase —
    /// app_next_status goes straight to APPS_STATUS.
    pub fn flash_set_on(&mut self) -> Result<()> {
        let mut req = [0u8; 16];
        req[0..2].copy_from_slice(&FEX_CMD_FES_FLASH_SET_ON.to_le_bytes());
        self.write_payload(&req)?;
        self.flush_status()
    }

    /// FEX_CMD_fes_flash_set_off (0x020B): calls `sunxi_sprite_exit(1)`.
    pub fn flash_set_off(&mut self) -> Result<()> {
        let mut req = [0u8; 16];
        req[0..2].copy_from_slice(&FEX_CMD_FES_FLASH_SET_OFF.to_le_bytes());
        self.write_payload(&req)?;
        self.flush_status()
    }

    /// FEX_CMD_fes_tool_mode (0x020F) with tool_mode=WORK_MODE_USB_TOOL_PRODUCT
    /// and next_mode=1 (nonzero): sets `sunxi_efex_next_action = REBOOT` and
    /// `app_next_status = APPS_EXIT` (usb_efex.c:1386-1432) — this is exactly
    /// what the real PhoenixSuit log showed right before "HELLO! BOOT0 is
    /// starting!". The device may disconnect/reboot before (or during) the
    /// final status flush, so a timeout on the flush here is treated as an
    /// expected outcome, not an error — the write_payload succeeding is what
    /// actually matters.
    pub fn trigger_reboot(&mut self) -> Result<()> {
        let mut req = [0u8; 16];
        req[0..2].copy_from_slice(&FEX_CMD_FES_TOOL_MODE.to_le_bytes());
        req[4..8].copy_from_slice(&WORK_MODE_USB_TOOL_PRODUCT.to_le_bytes());
        req[8..12].copy_from_slice(&1i32.to_le_bytes()); // next_mode = 1 (nonzero)
        self.write_payload(&req)?;
        // Best-effort: the device may already be tearing down USB for reboot.
        let _ = self.flush_status();
        Ok(())
    }

    /// FEX_CMD_fes_down (0x0206): announce an incoming write of `len` bytes.
    /// `dest_type` is either one of the TAG_* constants (special destination:
    /// MBR/BOOT1/BOOT0/ERASE/DRAM) or a plain sector-offset write when
    /// `dest_type` does not have the 0x7f00 mask bits set, in which case
    /// `addr` is the absolute starting SECTOR number (matches sys_partition.fex
    /// addrlo, and the real MBR dump's `part[N] addrlo` values).
    /// Per usb_efex.c's fes_down handler, `len` is always a byte count.
    fn fes_down_announce(&mut self, addr: u32, len: u32, dest_type: u32) -> Result<()> {
        let mut req = [0u8; 16];
        req[0..2].copy_from_slice(&FEX_CMD_FES_DOWN.to_le_bytes());
        req[4..8].copy_from_slice(&addr.to_le_bytes());
        req[8..12].copy_from_slice(&len.to_le_bytes());
        req[12..16].copy_from_slice(&dest_type.to_le_bytes());
        self.write_payload(&req)
    }

    /// Full fes_down + bulk data phase: announce, then send the actual bytes,
    /// then flush_status() to leave the device ready for the next command.
    pub fn fes_down(&mut self, addr: u32, dest_type: u32, data: &[u8]) -> Result<()> {
        self.fes_down_announce(addr, data.len() as u32, dest_type)?;
        self.write_payload(data)?;
        self.flush_status()
    }

    /// DRAM-masked tags (MBR/BOOT1/BOOT0/ERASE) support multi-chunk sends:
    /// the fes_down dispatch (usb_efex.c:1110-1144) only resets the receive
    /// pointer to `base_recv_buffer` on the exact value `DRAM_MASK|FINISH_TAG`
    /// (the plain-DRAM-write special case) — for any *other* DRAM-masked
    /// type (MBR/BOOT1/BOOT0/ERASE) it instead takes the accumulating branch
    /// (`act_recv_buffer = base_recv_buffer + to_be_recved_size`), letting
    /// `to_be_recved_size` grow across several fes_down calls that all share
    /// the same base type. `dram_data_recv_finish()` (and hence
    /// `sunxi_sprite_download_{mbr,uboot,boot0}()`) only actually fires on
    /// the chunk that carries TRANS_FINISH_TAG, using the FULL accumulated
    /// buffer at that point — so only the *last* chunk needs the flag set.
    /// Every chunk (not just the last) still needs its own flush_status()
    /// afterward, per the same app_step-reset requirement as everything else.
    fn fes_down_chunked(&mut self, base_type: u32, data: &[u8]) -> Result<()> {
        let chunks: Vec<&[u8]> = data.chunks(Self::FLASH_CHUNK_BYTES).collect();
        let last = chunks.len().saturating_sub(1);
        for (i, chunk) in chunks.into_iter().enumerate() {
            let ty = if i == last {
                base_type | TRANS_FINISH_TAG
            } else {
                base_type
            };
            self.fes_down_announce(0, chunk.len() as u32, ty)?;
            self.write_payload(chunk)?;
            self.flush_status()
                .with_context(|| format!("flushing after chunk {i} (type=0x{ty:x})"))?;
        }
        Ok(())
    }

    /// Write the GPT/MBR blob (the pre-built `sunxi_mbr.fex` from the image —
    /// NOT something we construct ourselves; see docs section 7/14.6).
    /// Server dispatches this to `sunxi_sprite_verify_mbr()` +
    /// `sunxi_sprite_download_mbr()` (usb_efex.c:1778-1808). `addr` is
    /// ignored by the server for this tag, per the same code path.
    pub fn write_mbr(&mut self, data: &[u8]) -> Result<()> {
        self.fes_down_chunked(TAG_MBR, data)
    }

    /// Write BOOT1 (`boot_package.fex`, the ORIGINAL unpatched file —
    /// work_mode=0x00 — never the FEL-primer copy with work_mode patched to
    /// 0x10). Dispatches to `sunxi_sprite_download_uboot()`.
    pub fn write_boot1(&mut self, data: &[u8]) -> Result<()> {
        self.fes_down_chunked(TAG_BOOT1, data)
    }

    /// Write BOOT0 (`boot0_sdcard.fex` or `boot0_nand.fex` depending on
    /// storage type — original file). Dispatches to
    /// `sunxi_sprite_download_boot0()`.
    pub fn write_boot0(&mut self, data: &[u8]) -> Result<()> {
        self.fes_down_chunked(TAG_BOOT0, data)
    }

    /// Set/clear the erase flag (device tree "eraseflag" property) that later
    /// governs whether the partition write pass does a full format or an
    /// overwrite-only pass — this is exactly the "Format Fusing vs Overwrite
    /// only" choice from the PhoenixSuit UI (docs section 12).
    /// `flag`: nonzero = format, 0 = overwrite-only (per usb_efex.c:1824-1847,
    /// note the origin flag is only overwritten when it was 0 or 1).
    pub fn set_erase_flag(&mut self, flag: u32) -> Result<()> {
        self.fes_down_chunked(TAG_ERASE, &flag.to_le_bytes())
    }

    /// Server-side receive buffer is SUNXI_EFEX_RECV_MEM_SIZE (4MB, or 2MB
    /// under CONFIG_SUNXI_SPINOR) and the flash path uses only half of it
    /// (`base_recv_buffer + SUNXI_EFEX_RECV_MEM_SIZE/2`, usb_efex.c:1133).
    /// That header-derived 2MB ceiling turned out not to be the real limit
    /// in practice, though: live testing on real hardware found 64KB
    /// succeeds but 256KB hangs the device (bulk_send timeout, needs a full
    /// reboot) — some other constraint (real eMMC write latency exceeding
    /// our 10s per-transfer timeout, or a USB/DMA chunk limit) caps it much
    /// lower than the theoretical buffer size. Each fes_down call for the
    /// flash path is independently a *complete* write (`sunxi_flash_write()`
    /// fires unconditionally once its own data arrives, no FINISH_TAG concept
    /// here unlike DRAM/MBR/BOOT) — so a large partition must be split into
    /// multiple whole fes_down calls, each targeting its own advancing
    /// sector offset.
    /// REVERTED to 64KB after live testing at 2MB: the device's own UART
    /// log showed a genuine SD/MMC-controller-level failure —
    /// `mmc 2 data timeout`, `smc 2 err, cmd 25, STO` (CMD25 = WRITE_MULTIPLE_
    /// BLOCK), `mmc write failed` — confirming this is a real hardware/driver
    /// write-size limit, not a USB transport timeout or buffer artifact we
    /// could paper over with a longer client-side timeout. 64KB is the
    /// largest size verified to work end-to-end on real hardware.
    const FLASH_CHUNK_BYTES: usize = 64 * 1024;

    /// Write a regular partition's data at `start_sector` (absolute sector
    /// number, matching sys_partition.fex's computed offsets — see
    /// src/sys_partition.rs). `dest_type=0` (no DRAM_MASK bits) routes
    /// through the plain `sunxi_flash_write(flash_start, flash_sectors, ...)`
    /// path. Chunked per FLASH_CHUNK_BYTES; each chunk is its own complete
    /// fes_down (announce+data+flush), sector offset advancing by
    /// chunk_len/512 each time.
    pub fn write_partition(&mut self, start_sector: u64, data: &[u8]) -> Result<()> {
        self.write_at_sector(start_sector, data)
    }

    /// Write `data` starting at an absolute sector, split into
    /// FLASH_CHUNK_BYTES pieces (each its own complete fes_down).
    fn write_at_sector(&mut self, start_sector: u64, data: &[u8]) -> Result<()> {
        use crate::sys_partition::SECTOR_SIZE;

        for (i, chunk) in data.chunks(Self::FLASH_CHUNK_BYTES).enumerate() {
            let chunk_sector_offset = (i * Self::FLASH_CHUNK_BYTES) as u64 / SECTOR_SIZE;
            let sector = start_sector + chunk_sector_offset;
            let addr = u32::try_from(sector)
                .map_err(|_| anyhow!("sector {sector} does not fit in u32"))?;
            self.fes_down(addr, 0, chunk)
                .with_context(|| format!("writing chunk {i} at sector 0x{sector:x}"))?;
        }
        Ok(())
    }

    /// Write an Android *sparse* image, expanding it as we go.
    ///
    /// `super.fex` ships as a sparse container, not a raw partition image: the
    /// real PhoenixSuit unsparses it while writing (the vendor u-boot carries
    /// the matching `sprite/sparse/` support). Writing the container verbatim
    /// leaves the sparse header where liblp's metadata belongs, so first-stage
    /// init cannot find the dynamic partitions and reboots to the bootloader
    /// before mounting anything — see docs section on the super/sparse fix.
    ///
    /// DONT_CARE regions are skipped: erase_flag=1 already cleared them.
    pub fn write_partition_sparse(
        &mut self,
        start_sector: u64,
        img: &crate::sparse::SparseImage<'_>,
    ) -> Result<()> {
        use crate::sparse::Segment;
        use crate::sys_partition::SECTOR_SIZE;

        for (i, seg) in img.segments.iter().enumerate() {
            match seg {
                Segment::Raw { out_offset, data } => {
                    let sector = start_sector + out_offset / SECTOR_SIZE;
                    self.write_at_sector(sector, data).with_context(|| {
                        format!("sparse segment {i}: raw at partition offset {out_offset}")
                    })?;
                }
                Segment::Fill {
                    out_offset,
                    value,
                    len,
                } => {
                    let pattern: Vec<u8> = value
                        .to_le_bytes()
                        .iter()
                        .cycle()
                        .take(*len as usize)
                        .copied()
                        .collect();
                    let sector = start_sector + out_offset / SECTOR_SIZE;
                    self.write_at_sector(sector, &pattern).with_context(|| {
                        format!("sparse segment {i}: fill at partition offset {out_offset}")
                    })?;
                }
            }
        }
        Ok(())
    }

    /// FEX_CMD_fes_verify_value (0x020C): ask device for CRC of `size` bytes
    /// starting at `start`. `start`'s unit matches whatever the preceding
    /// fes_down used (sector number for flash writes).
    /// Request: fes_cmd_verify_value_t = cmd+tag+start(u32)+size(i64), 16B.
    ///
    /// UNVERIFIED — this pair has never completed successfully against real
    /// hardware. Driving it left the device stuck printing
    /// `SUNXI_USB_EFEX_APPS_STATUS: INVALID direction`, i.e. our idea of the
    /// status/data phases here does not match the firmware's. Adding
    /// flush_status() around them (the fix that works for every other command)
    /// did not help. Work out the real phase sequence from usb_efex.c before
    /// using these; the server side (`sunxi_sprite_part_rawdata_verify`) is a
    /// pure read+checksum with no side effects, so nothing depends on them.
    pub fn verify_value(&mut self, start: u32, size: i64) -> Result<()> {
        let mut req = [0u8; 16];
        req[0..2].copy_from_slice(&FEX_CMD_FES_VERIFY_VALUE.to_le_bytes());
        req[4..8].copy_from_slice(&start.to_le_bytes());
        req[8..16].copy_from_slice(&size.to_le_bytes());
        self.write_payload(&req)
    }

    /// FEX_CMD_fes_verify_status (0x020D): poll result of the last verify_value.
    /// Response: fes_efex_verify_t = flag(u32)+fes_crc(i32)+media_crc(i32), 12B.
    /// UNVERIFIED — see verify_value().
    pub fn verify_status(&mut self) -> Result<(i32, i32)> {
        let mut req = [0u8; 16];
        req[0..2].copy_from_slice(&FEX_CMD_FES_VERIFY_STATUS.to_le_bytes());
        self.write_payload(&req)?;

        let resp = self.read_payload(12)?;
        let flag = u32::from_le_bytes(resp[0..4].try_into().unwrap());
        if flag != 0x6a61_7603 {
            bail!("verify not complete yet (flag={:#x})", flag);
        }
        let fes_crc = i32::from_le_bytes(resp[4..8].try_into().unwrap());
        let media_crc = i32::from_le_bytes(resp[8..12].try_into().unwrap());
        Ok((fes_crc, media_crc))
    }
}
