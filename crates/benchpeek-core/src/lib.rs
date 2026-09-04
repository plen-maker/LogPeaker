//! Core engine for benchpeek: the signal store (time-series ring buffers),
//! the rule evaluator that turns values into human-readable diagnoses, and
//! session record/replay.

use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub use benchpeek_plugin::{Decoder, Sample, SignalMeta};

/// Default number of points kept per signal.
pub const DEFAULT_CAPACITY: usize = 8192;

/// A single signal's rolling history plus its metadata.
pub struct Signal {
    pub meta: SignalMeta,
    /// Ring buffer of `[t, value]`, oldest at the front.
    pub points: VecDeque<[f64; 2]>,
    pub capacity: usize,
}

impl Signal {
    fn new(meta: SignalMeta, capacity: usize) -> Self {
        Self {
            meta,
            points: VecDeque::with_capacity(capacity.min(1024)),
            capacity,
        }
    }

    fn push(&mut self, t: f64, v: f64) {
        if self.points.len() == self.capacity {
            self.points.pop_front();
        }
        self.points.push_back([t, v]);
    }

    pub fn last(&self) -> Option<f64> {
        self.points.back().map(|p| p[1])
    }

    pub fn last_t(&self) -> Option<f64> {
        self.points.back().map(|p| p[0])
    }
}

/// Holds every signal seen this session, in first-seen order.
pub struct SignalStore {
    pub signals: HashMap<String, Signal>,
    /// First-seen order, for stable UI listing.
    pub order: Vec<String>,
    capacity: usize,
}

impl SignalStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            signals: HashMap::new(),
            order: Vec::new(),
            capacity,
        }
    }

    /// Register a signal with known metadata. No-op if already present.
    pub fn register(&mut self, meta: SignalMeta) {
        if !self.signals.contains_key(&meta.name) {
            self.order.push(meta.name.clone());
            let name = meta.name.clone();
            self.signals.insert(name, Signal::new(meta, self.capacity));
        }
    }

    /// Append a sample, auto-registering the signal if it is new.
    pub fn ingest(&mut self, s: &Sample) {
        if !self.signals.contains_key(&s.signal) {
            self.register(SignalMeta::new(s.signal.clone()));
        }
        if let Some(sig) = self.signals.get_mut(&s.signal) {
            sig.push(s.t, s.value);
        }
    }

    /// Iterate signals in first-seen order.
    pub fn iter(&self) -> impl Iterator<Item = &Signal> {
        self.order.iter().filter_map(move |n| self.signals.get(n))
    }
}

impl Default for SignalStore {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

/// Severity of a diagnosis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Ok,
    Warn,
    Fault,
}

/// A watch on a single signal's latest value.
#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    /// Signal name this rule watches.
    pub signal: String,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    /// Shown to the user when the rule trips.
    pub message: String,
    /// `"warn"` or `"fault"` (default `"fault"`).
    #[serde(default = "default_severity")]
    pub severity: String,
}

fn default_severity() -> String {
    "fault".to_string()
}

impl Rule {
    fn severity_health(&self) -> Health {
        match self.severity.as_str() {
            "warn" => Health::Warn,
            _ => Health::Fault,
        }
    }

    /// Returns a diagnosis if `value` violates this rule.
    pub fn check(&self, value: f64) -> Option<Diagnosis> {
        let out_low = self.min.is_some_and(|m| value < m);
        let out_high = self.max.is_some_and(|m| value > m);
        if out_low || out_high {
            Some(Diagnosis {
                health: self.severity_health(),
                signal: self.signal.clone(),
                message: self.message.clone(),
                value,
            })
        } else {
            None
        }
    }
}

/// A collection of rules, loadable from TOML (`[[rule]]` tables).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RuleSet {
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

impl RuleSet {
    pub fn from_toml_str(s: &str) -> Result<Self> {
        Ok(toml::from_str(s)?)
    }

    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_toml_str(&std::fs::read_to_string(path)?)
    }
}

/// Build fallback warning rules from any signal that declared a healthy range.
pub fn rules_from_meta(store: &SignalStore) -> RuleSet {
    let mut rules = Vec::new();
    for sig in store.iter() {
        if sig.meta.min.is_some() || sig.meta.max.is_some() {
            rules.push(Rule {
                signal: sig.meta.name.clone(),
                min: sig.meta.min,
                max: sig.meta.max,
                message: format!("{} outside declared healthy range", sig.meta.name),
                severity: "warn".to_string(),
            });
        }
    }
    RuleSet { rules }
}

/// A tripped rule, ready to show to the user.
#[derive(Debug, Clone)]
pub struct Diagnosis {
    pub health: Health,
    pub signal: String,
    pub message: String,
    pub value: f64,
}

/// Evaluate every rule against the latest value of its signal.
pub fn evaluate(rules: &RuleSet, store: &SignalStore) -> Vec<Diagnosis> {
    let mut out = Vec::new();
    for rule in &rules.rules {
        if let Some(sig) = store.signals.get(&rule.signal) {
            if let Some(v) = sig.last() {
                if let Some(d) = rule.check(v) {
                    out.push(d);
                }
            }
        }
    }
    out
}

#[derive(Serialize, Deserialize)]
struct WireSample {
    signal: String,
    value: f64,
    t: f64,
}

/// Appends samples to a JSON-lines file for later replay.
pub struct Recorder {
    w: BufWriter<File>,
}

impl Recorder {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            w: BufWriter::new(File::create(path)?),
        })
    }

    pub fn write(&mut self, s: &Sample) -> Result<()> {
        let line = serde_json::to_string(&WireSample {
            signal: s.signal.clone(),
            value: s.value,
            t: s.t,
        })?;
        self.w.write_all(line.as_bytes())?;
        self.w.write_all(b"\n")?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.w.flush()?;
        Ok(())
    }
}

/// Load a JSON-lines session file into memory, ordered as written.
pub fn load_replay(path: impl AsRef<Path>) -> Result<Vec<Sample>> {
    let r = BufReader::new(File::open(path)?);
    let mut out = Vec::new();
    for line in r.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let ws: WireSample = serde_json::from_str(&line)?;
        out.push(Sample {
            signal: ws.signal,
            value: ws.value,
            t: ws.t,
        });
    }
    Ok(out)
}
