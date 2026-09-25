//! Step 1 of live SWD memory monitoring: prove the ST-Link's USB protocol
//! is actually understood, before building a `Source`/`Decoder` around it.
//!
//! ST-Link's USB command set isn't ST-documented; the byte layout here is
//! taken from OpenOCD's real driver (`src/jtag/drivers/stlink_usb.c`,
//! `stlink_usb_version`), not guessed. This module only speaks to the
//! ST-Link's own vendor interface (bulk OUT `0x01` / bulk IN `0x81` on
//! interface 0) - the same physical channel `source.rs`'s raw USB transport
//! already reads, just with an actual command on the wire instead of a
//! generic byte sink. It does not yet do anything with the debug port
//! itself (SWD memory read/write) - that's the next step once this one is
//! confirmed against real hardware.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use nusb::transfer::{Bulk, In, Out};
use nusb::MaybeFuture;

/// STLINK_APIV3_GET_VERSION_EX - a harmless, read-only status query every
/// real ST-Link host tool (OpenOCD, STM32CubeProgrammer) sends on connect.
const STLINK_APIV3_GET_VERSION_EX: u8 = 0xFB;

/// Parsed `STLINK_APIV3_GET_VERSION_EX` response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StlinkVersion {
    pub major: u8,
    pub swim: u8,
    pub jtag: u8,
    pub msd: u8,
    pub bridge: u8,
    pub vendor_id: u16,
    pub product_id: u16,
}

/// Opens `vendor_id:product_id`, claims interface 0 (the ST-Link's own
/// vendor-specific interface - not a USB-CDC one), and reads back its
/// version. One-shot: opens and closes the device, doesn't keep it claimed.
pub fn probe_version(vendor_id: u16, product_id: u16) -> Result<StlinkVersion> {
    let info = nusb::list_devices()
        .wait()
        .context("listing USB devices")?
        .find(|d| d.vendor_id() == vendor_id && d.product_id() == product_id)
        .ok_or_else(|| anyhow!("no USB device {vendor_id:04x}:{product_id:04x}"))?;
    let device = info
        .open()
        .wait()
        .with_context(|| format!("opening USB device {vendor_id:04x}:{product_id:04x}"))?;
    let interface = device
        .claim_interface(0)
        .wait()
        .context("claiming ST-Link interface 0")?;

    let mut cmd_ep = interface
        .endpoint::<Bulk, Out>(0x01)
        .context("opening bulk OUT ep 0x01")?;
    let mut cmd = vec![0u8; 16];
    cmd[0] = STLINK_APIV3_GET_VERSION_EX;
    let sent = cmd_ep.transfer_blocking(cmd.into(), Duration::from_secs(1));
    sent.status.context("writing GET_VERSION_EX command")?;

    let mut resp_ep = interface
        .endpoint::<Bulk, In>(0x81)
        .context("opening bulk IN ep 0x81")?;
    let buf = resp_ep.allocate(resp_ep.max_packet_size().max(64));
    let completion = resp_ep.transfer_blocking(buf, Duration::from_secs(1));
    completion
        .status
        .context("reading GET_VERSION_EX response")?;

    parse_version_ex(&completion.buffer[..completion.actual_len])
}

/// Layout from OpenOCD's `stlink_usb_version()`: byte 0 major version,
/// 1 SWIM sub-version, 2 JTAG sub-version, 3 mass-storage sub-version,
/// 4 bridge sub-version, bytes 8-9 VID (LE), bytes 10-11 PID (LE).
/// Bytes 5-7 are reserved/unused by every ST-Link host tool that decodes
/// this response, so they're not exposed here either.
fn parse_version_ex(resp: &[u8]) -> Result<StlinkVersion> {
    if resp.len() < 12 {
        return Err(anyhow!(
            "GET_VERSION_EX response too short: {} byte(s), expected at least 12",
            resp.len()
        ));
    }
    Ok(StlinkVersion {
        major: resp[0],
        swim: resp[1],
        jtag: resp[2],
        msd: resp[3],
        bridge: resp[4],
        vendor_id: u16::from_le_bytes([resp[8], resp[9]]),
        product_id: u16::from_le_bytes([resp[10], resp[11]]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_response() {
        // major=3, swim=0, jtag=13, msd=0, bridge=0, then 3 reserved bytes,
        // then VID 0x0483 / PID 0x3753 little-endian - a real STLink-V3
        // shape, hand-built rather than captured, since the byte values
        // themselves aren't the point (parse_version_ex's job is fixed
        // offsets/widths, not validating firmware version numbers).
        let resp = [3, 0, 13, 0, 0, 0, 0, 0, 0x83, 0x04, 0x53, 0x37];
        let v = parse_version_ex(&resp).unwrap();
        assert_eq!(v.major, 3);
        assert_eq!(v.jtag, 13);
        assert_eq!(v.vendor_id, 0x0483);
        assert_eq!(v.product_id, 0x3753);
    }

    #[test]
    fn rejects_a_short_response() {
        assert!(parse_version_ex(&[1, 2, 3]).is_err());
    }

    /// Confirms the actual protocol, not just the parser: writes a real
    /// GET_VERSION_EX to a real ST-Link over real usbfs and checks the
    /// device's own reported VID:PID matches what we opened it as -
    /// meaningless unless the command was framed and sent correctly and
    /// the device understood it. Ignored by default since it needs
    /// hardware; this repo's dev sandbox has a simulated ST-Link V3 at
    /// 0483:3753.
    #[test]
    #[ignore = "requires an ST-Link at USB 0483:3753 (VID:PID) - run explicitly with `cargo test -p benchpeek-app -- --ignored probes_a_real_stlinks_version`"]
    fn probes_a_real_stlinks_version() {
        let v = probe_version(0x0483, 0x3753).expect("probe real ST-Link");
        println!(
            "ST-Link vJ{}.M{}.B{} (SWIM v{}, MSD v{}) at {:04x}:{:04x}",
            v.jtag, v.major, v.bridge, v.swim, v.msd, v.vendor_id, v.product_id
        );
        assert_eq!(v.vendor_id, 0x0483);
        assert_eq!(v.product_id, 0x3753);
        // A real device reports a real firmware major version, not a
        // stub/zeroed response.
        assert!(v.major > 0, "major version was 0 - looks like a stub reply");
    }
}
