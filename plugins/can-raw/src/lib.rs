//! `can-raw` decoder: turns `candump`-style `<id>#<hex data>` lines into one
//! [`Sample`] per frame, named `can_<id in hex>`.
//!
//! This is deliberately undecoded: with no DBC, a frame's data bytes are
//! just packed into a number (little-endian) rather than split into the
//! physical signals a real DBC would define. It exists to make a CAN bus
//! visible in benchpeek (table, plot, rules, diagnosis) before a per-bus
//! decoder exists for it.
//!
//! Example line (as `benchpeek-app`'s CAN source emits it, one per frame):
//! ```text
//! 301#0102030405060708
//! ```

use benchpeek_plugin::{Decoder, Sample, SignalMeta};

pub struct CanRaw {
    buf: Vec<u8>,
}

impl CanRaw {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }
}

impl Default for CanRaw {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for CanRaw {
    fn name(&self) -> &str {
        "can-raw"
    }

    fn signals(&self) -> Vec<SignalMeta> {
        // CAN IDs are only known once frames start arriving.
        Vec::new()
    }

    fn decode(&mut self, bytes: &[u8], t: f64) -> Vec<Sample> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();

        while let Some(nl) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=nl).collect();
            let line = String::from_utf8_lossy(&line[..line.len() - 1]);
            if let Some(sample) = parse_frame_line(&line, t) {
                out.push(sample);
            }
        }

        // Drop a runaway line that never terminates.
        if self.buf.len() > 64 * 1024 {
            self.buf.clear();
        }

        out
    }
}

fn parse_frame_line(line: &str, t: f64) -> Option<Sample> {
    let (id_hex, data_hex) = line.split_once('#')?;
    let id = u32::from_str_radix(id_hex, 16).ok()?;
    if data_hex.len() % 2 != 0 || data_hex.len() > 16 {
        return None;
    }
    let mut value: u64 = 0;
    for i in (0..data_hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&data_hex[i..i + 2], 16).ok()?;
        value |= (byte as u64) << (i * 4);
    }
    Some(Sample::new(format!("can_{id:X}"), value as f64, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_frame_per_line() {
        let mut d = CanRaw::new();
        let out = d.decode(b"301#0102030405060708\n", 1.0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].signal, "can_301");
        assert_eq!(out[0].value, 0x0807060504030201u64 as f64);
        assert_eq!(out[0].t, 1.0);
    }

    #[test]
    fn ignores_malformed_lines() {
        let mut d = CanRaw::new();
        let out = d.decode(b"not-a-frame\n123#ZZ\n", 1.0);
        assert!(out.is_empty());
    }

    #[test]
    fn buffers_partial_lines_across_calls() {
        let mut d = CanRaw::new();
        assert!(d.decode(b"301#0A", 1.0).is_empty());
        let out = d.decode(b"0B\n", 2.0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].value, 0x0B0Au64 as f64);
    }

    #[test]
    fn short_frames_leave_high_bytes_zero() {
        let mut d = CanRaw::new();
        let out = d.decode(b"7DF#0201\n", 1.0);
        assert_eq!(out[0].value, 0x0102u64 as f64);
    }
}
