//! Minimal FEL protocol client (VID:PID 1f3a:efe8), reimplemented from the
//! wire format in sunxi-tools' fel_lib.c (AW_USB_* request envelope +
//! AW_FEL_* request struct). Only what's needed to prove live communication
//! and eventually stage fes1/u-boot uploads — not a full sunxi-fel port.

// The request codes and version fields below mirror sunxi-tools' fel_lib.c so
// the wire format stays documented in one place; the bootstrap only exercises
// version/write/exec.
#![allow(dead_code)]

use anyhow::{anyhow, bail, Context, Result};
use rusb::{DeviceHandle, GlobalContext};
use std::time::Duration;

const AW_USB_VENDOR_ID: u16 = 0x1f3a;
const AW_USB_PRODUCT_ID: u16 = 0xefe8;

const AW_USB_READ: u16 = 0x11;
const AW_USB_WRITE: u16 = 0x12;

pub const AW_FEL_VERSION: u32 = 0x001;
pub const AW_FEL_1_WRITE: u32 = 0x101;
pub const AW_FEL_1_EXEC: u32 = 0x102;
pub const AW_FEL_1_READ: u32 = 0x103;

const USB_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct FelVersion {
    pub signature: [u8; 8],
    pub soc_id: u32,
    pub unknown_0a: u32,
    pub protocol: u16,
    pub unknown_12: u8,
    pub unknown_13: u8,
    pub scratchpad: u32,
}

pub struct FelDevice {
    handle: DeviceHandle<GlobalContext>,
    ep_in: u8,
    ep_out: u8,
}

impl FelDevice {
    pub fn open() -> Result<Self> {
        let handle = rusb::open_device_with_vid_pid(AW_USB_VENDOR_ID, AW_USB_PRODUCT_ID)
            .ok_or_else(|| anyhow!("Allwinner USB FEL device (1f3a:efe8) not found"))?;

        handle
            .claim_interface(0)
            .context("claiming USB interface 0 (is another tool holding it?)")?;

        let device = handle.device();
        let config = device
            .active_config_descriptor()
            .context("reading active config descriptor")?;

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

        let ep_in = ep_in.context("no bulk IN endpoint found")?;
        let ep_out = ep_out.context("no bulk OUT endpoint found")?;

        Ok(Self { handle, ep_in, ep_out })
    }

    fn bulk_send(&self, data: &[u8]) -> Result<()> {
        let mut sent_total = 0;
        while sent_total < data.len() {
            let sent = self
                .handle
                .write_bulk(self.ep_out, &data[sent_total..], USB_TIMEOUT)
                .context("usb_bulk_send")?;
            sent_total += sent;
        }
        Ok(())
    }

    fn bulk_recv(&self, buf: &mut [u8]) -> Result<()> {
        let mut recv_total = 0;
        while recv_total < buf.len() {
            let recv = self
                .handle
                .read_bulk(self.ep_in, &mut buf[recv_total..], USB_TIMEOUT)
                .context("usb_bulk_recv")?;
            recv_total += recv;
        }
        Ok(())
    }

    /// AW_USB_REQUEST envelope (32 bytes, packed):
    /// signature[8]="AWUC" + length(u32) + unknown1=0x0c000000(u32)
    /// + request(u16) + length2=length(u32) + pad[10]
    fn send_usb_request(&self, request: u16, length: u32) -> Result<()> {
        let mut req = [0u8; 32];
        req[0..4].copy_from_slice(b"AWUC");
        req[8..12].copy_from_slice(&length.to_le_bytes());
        req[12..16].copy_from_slice(&0x0c000000u32.to_le_bytes());
        req[16..18].copy_from_slice(&request.to_le_bytes());
        req[18..22].copy_from_slice(&length.to_le_bytes());
        // req[22..32] pad, already zero
        self.bulk_send(&req)
    }

    fn read_usb_response(&self) -> Result<()> {
        let mut buf = [0u8; 13];
        self.bulk_recv(&mut buf)?;
        if &buf[0..4] != b"AWUS" {
            bail!("bad USB response envelope: {:02x?}", buf);
        }
        Ok(())
    }

    fn usb_write(&self, data: &[u8]) -> Result<()> {
        self.send_usb_request(AW_USB_WRITE, data.len() as u32)?;
        self.bulk_send(data)?;
        self.read_usb_response()
    }

    fn usb_read(&self, buf: &mut [u8]) -> Result<()> {
        self.send_usb_request(AW_USB_READ, buf.len() as u32)?;
        self.bulk_recv(buf)?;
        self.read_usb_response()
    }

    /// aw_fel_request: request(u32) + address(u32) + length(u32) + pad(u32), 16 bytes
    fn send_fel_request(&self, request: u32, address: u32, length: u32) -> Result<()> {
        let mut req = [0u8; 16];
        req[0..4].copy_from_slice(&request.to_le_bytes());
        req[4..8].copy_from_slice(&address.to_le_bytes());
        req[8..12].copy_from_slice(&length.to_le_bytes());
        self.usb_write(&req)
    }

    fn read_fel_status(&self) -> Result<()> {
        let mut buf = [0u8; 8];
        self.usb_read(&mut buf)
    }

    pub fn get_version(&self) -> Result<FelVersion> {
        self.send_fel_request(AW_FEL_VERSION, 0, 0)?;
        let mut buf = [0u8; 32];
        self.usb_read(&mut buf)?;
        self.read_fel_status()?;

        let mut signature = [0u8; 8];
        signature.copy_from_slice(&buf[0..8]);
        let soc_id_raw = u32::from_le_bytes(buf[8..12].try_into().unwrap());
        Ok(FelVersion {
            signature,
            soc_id: (soc_id_raw >> 8) & 0xFFFF,
            unknown_0a: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
            protocol: u16::from_le_bytes(buf[16..18].try_into().unwrap()),
            unknown_12: buf[18],
            unknown_13: buf[19],
            scratchpad: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
        })
    }

    pub fn read_memory(&self, address: u32, buf: &mut [u8]) -> Result<()> {
        self.send_fel_request(AW_FEL_1_READ, address, buf.len() as u32)?;
        self.usb_read(buf)?;
        self.read_fel_status()
    }

    pub fn write_memory(&self, address: u32, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        self.send_fel_request(AW_FEL_1_WRITE, address, data.len() as u32)?;
        self.usb_write(data)?;
        self.read_fel_status()
    }

    pub fn execute(&self, address: u32) -> Result<()> {
        self.send_fel_request(AW_FEL_1_EXEC, address, 0)?;
        self.read_fel_status()
    }
}
