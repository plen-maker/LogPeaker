//! Plugin-facing API for benchpeek.
//!
//! In v0 a "decoder" is a compiled-in Rust type implementing [`Decoder`].
//! v1 moves this exact shape behind a WASM component boundary so decoders
//! can ship independently and in any language.

/// Static description of one signal a decoder can produce.
#[derive(Debug, Clone)]
pub struct SignalMeta {
    pub name: String,
    pub unit: Option<String>,
    /// Lower edge of the healthy range, if known.
    pub min: Option<f64>,
    /// Upper edge of the healthy range, if known.
    pub max: Option<f64>,
}

impl SignalMeta {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            unit: None,
            min: None,
            max: None,
        }
    }

    pub fn unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    /// Declare the healthy operating range; benchpeek turns this into a
    /// default warning rule when no explicit rule covers the signal.
    pub fn healthy(mut self, min: f64, max: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }
}

/// One decoded data point.
#[derive(Debug, Clone)]
pub struct Sample {
    pub signal: String,
    pub value: f64,
    /// Seconds since the start of the session.
    pub t: f64,
}

impl Sample {
    pub fn new(signal: impl Into<String>, value: f64, t: f64) -> Self {
        Self {
            signal: signal.into(),
            value,
            t,
        }
    }
}

/// Turns a raw byte stream from a transport into [`Sample`]s.
pub trait Decoder: Send {
    /// Human-readable decoder name.
    fn name(&self) -> &str;

    /// Signals this decoder can emit. Used to seed the signal table and
    /// derive default rules.
    fn signals(&self) -> Vec<SignalMeta>;

    /// Feed a chunk of freshly-read bytes; return any samples completed by it.
    /// `t` is the session timestamp at which the chunk was read.
    fn decode(&mut self, bytes: &[u8], t: f64) -> Vec<Sample>;
}
