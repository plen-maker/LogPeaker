//! Live SWD memory monitoring, built up in verifiable steps rather than in
//! one untested leap: prove the ST-Link's USB protocol is actually
//! understood before building a `Source`/`Decoder` around it.
//!
//! ST-Link's USB command set isn't ST-documented; every byte layout here is
//! taken from OpenOCD's real driver (`src/jtag/drivers/stlink_usb.c`), not
//! guessed - see each command's doc comment for exactly which function it
//! came from. This module only speaks to the ST-Link's own vendor interface
//! (bulk OUT `0x01` / bulk IN `0x81` on interface 0) - the same physical
//! channel `source.rs`'s raw USB transport already reads, just with actual
//! commands on the wire instead of a generic byte sink.
//!
//! Two command families exist, with different response shapes:
//! - Plain queries like `GET_VERSION_EX` (`0xFB`) answer directly with
//!   their data - no separate status byte.
//! - Everything under `STLINK_DEBUG_COMMAND` (`0xF2`, i.e. actual SWD/JTAG
//!   debug-port operations) answers with a status byte first
//!   (`STLINK_DEBUG_ERR_OK`/`STLINK_DEBUG_ERR_FAULT`), checked by
//!   [`check_debug_status`].
//!
//! Implemented so far: version query, entering SWD debug mode, reading the
//! target's IDCODE (the standard "is the debug link actually up, and what's
//! on the other end" check every SWD tool does first). **Not yet
//! implemented: reading target memory.** OpenOCD's `read_mem32` needs a
//! second USB transaction (a follow-up status query) whose exact byte
//! layout wasn't pinned down with enough confidence to ship - better to
//! land the verified half than guess at the rest.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use nusb::transfer::{Bulk, In, Out};
use nusb::{Interface, MaybeFuture};

/// STLINK_APIV3_GET_VERSION_EX - a harmless, read-only status query every
/// real ST-Link host tool (OpenOCD, STM32CubeProgrammer) sends on connect.
const STLINK_APIV3_GET_VERSION_EX: u8 = 0xFB;

/// Every SWD/JTAG debug-port command is a sub-command of this one.
const STLINK_DEBUG_COMMAND: u8 = 0xF2;
/// Sub-command: enter a debug mode (SWD or JTAG).
const STLINK_DEBUG_APIV2_ENTER: u8 = 0x30;
/// Enter SWD mode specifically, without resetting the target first - a
/// reset is an intrusive action a passive monitoring tool shouldn't take
/// just to read memory.
const STLINK_DEBUG_ENTER_SWD_NO_RESET: u8 = 0xA3;
/// Sub-command: read the SWD/JTAG IDCODE(s) of whatever's connected.
const STLINK_DEBUG_APIV2_READ_IDCODES: u8 = 0x31;
/// First byte of a `STLINK_DEBUG_COMMAND` response: the operation succeeded.
const STLINK_DEBUG_ERR_OK: u8 = 0x80;
/// First byte of a `STLINK_DEBUG_COMMAND` response: the operation faulted.
const STLINK_DEBUG_ERR_FAULT: u8 = 0x81;

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

/// An opened, interface-claimed ST-Link, ready to exchange commands.
/// Deliberately not `Send`/kept open across threads yet - each call opens
/// fresh (see [`Self::open`]'s callers), matching how `source.rs`'s USB
/// transport already treats a device as cheap to reopen rather than a
/// long-lived resource to manage carefully.
pub struct StlinkHandle {
    interface: Interface,
}

impl StlinkHandle {
    /// Opens `vendor_id:product_id` and claims interface 0 - the ST-Link's
    /// own vendor-specific interface, not a USB-CDC one.
    pub fn open(vendor_id: u16, product_id: u16) -> Result<Self> {
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
        Ok(Self { interface })
    }

    /// Sends `cmd` (zero-padded to the fixed 16-byte command size every
    /// ST-Link command uses) on bulk OUT `0x01`, then reads back up to
    /// `max_resp` bytes on bulk IN `0x81`.
    fn command(&self, cmd: &[u8], max_resp: usize) -> Result<Vec<u8>> {
        let mut cmd_ep = self
            .interface
            .endpoint::<Bulk, Out>(0x01)
            .context("opening bulk OUT ep 0x01")?;
        let mut buf = vec![0u8; 16];
        buf[..cmd.len()].copy_from_slice(cmd);
        let sent = cmd_ep.transfer_blocking(buf.into(), Duration::from_secs(1));
        sent.status.context("writing command")?;

        let mut resp_ep = self
            .interface
            .endpoint::<Bulk, In>(0x81)
            .context("opening bulk IN ep 0x81")?;
        let alloc_len = resp_ep.max_packet_size().max(max_resp).max(64);
        let rbuf = resp_ep.allocate(alloc_len);
        let completion = resp_ep.transfer_blocking(rbuf, Duration::from_secs(1));
        completion.status.context("reading response")?;
        Ok(completion.buffer[..completion.actual_len].to_vec())
    }

    /// Version query - see [`StlinkVersion`].
    pub fn version(&self) -> Result<StlinkVersion> {
        parse_version_ex(&self.command(&[STLINK_APIV3_GET_VERSION_EX], 12)?)
    }

    /// Switches the ST-Link into SWD debug mode, without resetting the
    /// target. From OpenOCD's `stlink_usb_mode_enter`: a 3-byte command
    /// (`STLINK_DEBUG_COMMAND`, `STLINK_DEBUG_APIV2_ENTER`,
    /// `STLINK_DEBUG_ENTER_SWD_NO_RESET`), status-byte response. Must
    /// succeed before [`Self::read_idcode`] or any other debug-port
    /// operation means anything.
    pub fn enter_swd_debug(&self) -> Result<()> {
        let resp = self.command(
            &[
                STLINK_DEBUG_COMMAND,
                STLINK_DEBUG_APIV2_ENTER,
                STLINK_DEBUG_ENTER_SWD_NO_RESET,
            ],
            2,
        )?;
        check_debug_status(&resp)
    }

    /// Reads the SWD IDCODE of whatever's on the other end of the debug
    /// port - the standard "is the link actually up, and what's connected"
    /// check every SWD tool does right after entering debug mode. From
    /// OpenOCD's `stlink_usb_idcode`: a 2-byte command
    /// (`STLINK_DEBUG_COMMAND`, `STLINK_DEBUG_APIV2_READ_IDCODES`), a
    /// 12-byte response with the 32-bit IDCODE at offset 4 (little-endian).
    pub fn read_idcode(&self) -> Result<u32> {
        parse_idcode(&self.command(&[STLINK_DEBUG_COMMAND, STLINK_DEBUG_APIV2_READ_IDCODES], 12)?)
    }
}

/// Convenience one-shot: open, read the version, done. See
/// [`StlinkHandle::open`]/[`StlinkHandle::version`] to do more than one
/// thing against the same device without reopening it.
pub fn probe_version(vendor_id: u16, product_id: u16) -> Result<StlinkVersion> {
    StlinkHandle::open(vendor_id, product_id)?.version()
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

/// Every `STLINK_DEBUG_COMMAND` response starts with a status byte -
/// `STLINK_DEBUG_ERR_OK` (0x80) or `STLINK_DEBUG_ERR_FAULT` (0x81) being
/// the two OpenOCD's `stlink_usb_error_check` names explicitly; anything
/// else is surfaced as-is rather than guessed at.
fn check_debug_status(resp: &[u8]) -> Result<()> {
    match resp.first() {
        Some(&STLINK_DEBUG_ERR_OK) => Ok(()),
        Some(&STLINK_DEBUG_ERR_FAULT) => Err(anyhow!("ST-Link reported a fault (0x81)")),
        Some(&other) => Err(anyhow!(
            "unexpected ST-Link debug status byte: {other:#04x}"
        )),
        None => Err(anyhow!("empty response to a debug command")),
    }
}

/// IDCODE at offset 4 (little-endian) of a `READ_IDCODES` response.
fn parse_idcode(resp: &[u8]) -> Result<u32> {
    if resp.len() < 8 {
        return Err(anyhow!(
            "READ_IDCODES response too short: {} byte(s), expected at least 8",
            resp.len()
        ));
    }
    Ok(u32::from_le_bytes([resp[4], resp[5], resp[6], resp[7]]))
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

    #[test]
    fn debug_status_ok_and_fault_are_told_apart() {
        assert!(check_debug_status(&[0x80]).is_ok());
        assert!(check_debug_status(&[0x81]).is_err());
        assert!(check_debug_status(&[0x42]).is_err());
        assert!(check_debug_status(&[]).is_err());
    }

    #[test]
    fn parses_idcode_at_offset_4() {
        // A realistic-shaped IDCODE (0x6BA02477 is the widely-seen ARM
        // CoreSight SW-DP id on Cortex-M parts) - the point here is the
        // offset/endianness, not a verified STM32MP2 value.
        let resp = [0x80, 0, 0, 0, 0x77, 0x24, 0xA0, 0x6B];
        assert_eq!(parse_idcode(&resp).unwrap(), 0x6BA0_2477);
    }

    #[test]
    fn rejects_a_short_idcode_response() {
        assert!(parse_idcode(&[0x80, 0, 0]).is_err());
    }

    /// Confirms the actual protocol, not just the parser: writes a real
    /// GET_VERSION_EX to a real ST-Link over real usbfs and checks the
    /// device's own reported VID:PID matches what we opened it as -
    /// meaningless unless the command was framed and sent correctly and
    /// the device understood it. Ignored by default since it needs
    /// hardware; this repo's dev sandbox had a simulated ST-Link V3 at
    /// 0483:3753 earlier in this project's life, but not in every sandbox
    /// instance - see CLAUDE.md for the current state of this gap.
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

    /// The actual milestone this module exists for: prove the debug link
    /// comes up and identifies a real core, not just that the version
    /// query round-trips. Ignored for the same hardware-availability
    /// reason as `probes_a_real_stlinks_version`.
    #[test]
    #[ignore = "requires an ST-Link at USB 0483:3753 (VID:PID) - run explicitly with `cargo test -p benchpeek-app -- --ignored enters_swd_and_reads_a_real_idcode`"]
    fn enters_swd_and_reads_a_real_idcode() {
        let h = StlinkHandle::open(0x0483, 0x3753).expect("open real ST-Link");
        h.enter_swd_debug().expect("enter SWD debug mode");
        let idcode = h.read_idcode().expect("read IDCODE");
        println!("IDCODE = {idcode:#010x}");
        assert_ne!(
            idcode, 0,
            "an all-zero IDCODE means nothing answered on SWD"
        );
        assert_ne!(
            idcode, 0xFFFF_FFFF,
            "an all-ones IDCODE means the SWD lines are floating/unconnected"
        );
    }
}
