//! `ascii-kv` decoder: newline-delimited `KEY=VALUE` telemetry.
//!
//! Example line (as sent by a board over UART):
//! ```text
//! VBAT=12.34 RPM=850 TEMP=41.2
//! ```
//! Each token becomes one [`Sample`]; unparseable tokens are skipped.

use benchpeek_plugin::{Decoder, Sample, SignalMeta};

pub struct AsciiKv {
    buf: Vec<u8>,
    known: Vec<SignalMeta>,
}

impl AsciiKv {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            known: Vec::new(),
        }
    }

    /// Declare a signal up front (name, unit, healthy range).
    pub fn with_signal(mut self, meta: SignalMeta) -> Self {
        self.known.push(meta);
        self
    }
}

impl Default for AsciiKv {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for AsciiKv {
    fn name(&self) -> &str {
        "ascii-kv"
    }

    fn signals(&self) -> Vec<SignalMeta> {
        self.known.clone()
    }

    fn decode(&mut self, bytes: &[u8], t: f64) -> Vec<Sample> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();

        while let Some(nl) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=nl).collect();
            let line = String::from_utf8_lossy(&line[..line.len() - 1]);
            for tok in line.split_whitespace() {
                let Some((k, v)) = tok.split_once('=') else {
                    continue;
                };
                if let Ok(value) = v.trim().parse::<f64>() {
                    out.push(Sample::new(k.trim(), value, t));
                }
            }
        }

        // Drop a runaway line that never terminates.
        if self.buf.len() > 64 * 1024 {
            self.buf.clear();
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lines_and_tokens() {
        let mut d = AsciiKv::new();
        let out = d.decode(b"VBAT=12.3 RPM=850\nTEMP=", 1.0);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].signal, "VBAT");
        assert_eq!(out[1].value, 850.0);
        // "TEMP=" is buffered until its newline arrives.
        let out = d.decode(b"41.5\n", 2.0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].signal, "TEMP");
        assert_eq!(out[0].t, 2.0);
    }
}
