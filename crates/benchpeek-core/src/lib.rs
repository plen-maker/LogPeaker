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

mod kicad;
pub use kicad::{Net, NetList, NetNode};

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

/// Severity of one log line, same three-level scale as [`Health`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Fault,
}

/// One line from a watched log stream (e.g. `journalctl -f` over SSH).
#[derive(Debug, Clone)]
pub struct LogEvent {
    /// Seconds since the watch started.
    pub t: f64,
    pub level: LogLevel,
    pub message: String,
}

/// Keyword-based severity classifier. Deliberately simple and
/// format-agnostic (works on journalctl, dmesg, or a plain app log) rather
/// than parsing any one log format's structured priority field.
pub fn classify_line(line: &str) -> LogLevel {
    let lower = line.to_lowercase();
    let has_any = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    if has_any(&[
        "error", "fail", "panic", "fatal", "crit", "segfault", "denied",
    ]) {
        LogLevel::Fault
    } else if has_any(&["warn"]) {
        LogLevel::Warn
    } else {
        LogLevel::Info
    }
}

/// Rolling window of recent log lines.
pub struct LogStore {
    pub events: VecDeque<LogEvent>,
    capacity: usize,
}

impl LogStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity.min(1024)),
            capacity,
        }
    }

    pub fn push(&mut self, e: LogEvent) {
        if self.events.len() == self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(e);
    }

    /// `(faults, warns)` currently held in the window.
    pub fn counts(&self) -> (usize, usize) {
        self.events.iter().fold((0, 0), |(f, w), e| match e.level {
            LogLevel::Fault => (f + 1, w),
            LogLevel::Warn => (f, w + 1),
            LogLevel::Info => (f, w),
        })
    }
}

impl Default for LogStore {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

/// One step of a [`TestSequence`]: hold `signal` within `[min, max]`
/// continuously for `hold_secs` before it passes, or fail if that hasn't
/// happened within `timeout_secs` of becoming the active step.
#[derive(Debug, Clone, Deserialize)]
pub struct TestStep {
    pub signal: String,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub hold_secs: f64,
    pub timeout_secs: f64,
    /// Shown in the report; defaults to the signal name if omitted.
    #[serde(default)]
    pub label: Option<String>,
}

impl TestStep {
    fn display_label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.signal)
    }
}

/// An ordered, named list of [`TestStep`]s, loadable from TOML (`[[step]]`
/// tables) the same way [`RuleSet`] loads `[[rule]]` tables.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TestSequence {
    pub name: String,
    #[serde(default, rename = "step")]
    pub steps: Vec<TestStep>,
}

impl TestSequence {
    pub fn from_toml_str(s: &str) -> Result<Self> {
        Ok(toml::from_str(s)?)
    }

    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_toml_str(&std::fs::read_to_string(path)?)
    }
}

/// Where one [`TestStep`] currently stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    /// Not yet the active step.
    Pending,
    /// Active step, waiting for its condition to hold long enough.
    Running,
    Passed,
    Failed,
}

/// The outcome of one step, kept for the report even after the runner has
/// moved past it.
#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub status: StepStatus,
    /// Session time the step became active.
    pub started_t: Option<f64>,
    /// Session time the step passed or failed.
    pub ended_t: Option<f64>,
    pub detail: String,
}

impl StepOutcome {
    fn pending() -> Self {
        Self {
            status: StepStatus::Pending,
            started_t: None,
            ended_t: None,
            detail: String::new(),
        }
    }
}

/// Drives a [`TestSequence`] against a [`SignalStore`]'s live values, one
/// step at a time. Stops at the first failed step, same as a typical
/// power-on self-test script - a later step's precondition usually depends
/// on an earlier one having actually passed.
pub struct SequenceRunner {
    sequence: TestSequence,
    current: usize,
    in_range_since: Option<f64>,
    outcomes: Vec<StepOutcome>,
    done: bool,
}

impl SequenceRunner {
    pub fn start(sequence: TestSequence) -> Self {
        let done = sequence.steps.is_empty();
        let outcomes = sequence
            .steps
            .iter()
            .map(|_| StepOutcome::pending())
            .collect();
        Self {
            sequence,
            current: 0,
            in_range_since: None,
            outcomes,
            done,
        }
    }

    pub fn name(&self) -> &str {
        &self.sequence.name
    }

    pub fn outcomes(&self) -> &[StepOutcome] {
        &self.outcomes
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    /// `None` while running; `Some(true)` once every step has passed,
    /// `Some(false)` once a step has failed.
    pub fn overall_passed(&self) -> Option<bool> {
        if !self.done {
            return None;
        }
        Some(self.outcomes.iter().all(|o| o.status == StepStatus::Passed))
    }

    /// Advance the runner against `store`'s current values at session time
    /// `t`. A no-op once [`Self::is_done`].
    pub fn tick(&mut self, store: &SignalStore, t: f64) {
        if self.done {
            return;
        }
        let idx = self.current;
        let step = &self.sequence.steps[idx];
        let started = *self.outcomes[idx].started_t.get_or_insert(t);
        self.outcomes[idx].status = StepStatus::Running;

        let value = store.signals.get(&step.signal).and_then(|s| s.last());
        let in_range = value.is_some_and(|v| {
            let out_low = step.min.is_some_and(|m| v < m);
            let out_high = step.max.is_some_and(|m| v > m);
            !out_low && !out_high
        });

        if in_range {
            let since = *self.in_range_since.get_or_insert(t);
            if t - since >= step.hold_secs {
                let held = t - since;
                self.resolve(idx, StepStatus::Passed, t, format!("held {held:.1}s"));
                self.advance();
                return;
            }
        } else {
            self.in_range_since = None;
        }

        if t - started >= step.timeout_secs {
            let detail = match value {
                Some(v) => format!("timed out at {:.3} ({})", v, step.display_label()),
                None => format!("timed out: no data for {}", step.display_label()),
            };
            self.resolve(idx, StepStatus::Failed, t, detail);
            self.done = true;
        }
    }

    fn resolve(&mut self, idx: usize, status: StepStatus, t: f64, detail: String) {
        let outcome = &mut self.outcomes[idx];
        outcome.status = status;
        outcome.ended_t = Some(t);
        outcome.detail = detail;
    }

    fn advance(&mut self) {
        self.current += 1;
        self.in_range_since = None;
        if self.current >= self.sequence.steps.len() {
            self.done = true;
        }
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

#[cfg(test)]
mod sequence_tests {
    use super::*;

    fn store_with(signal: &str, t: f64, value: f64) -> SignalStore {
        let mut store = SignalStore::new(64);
        store.ingest(&Sample::new(signal, value, t));
        store
    }

    #[test]
    fn passes_immediately_with_no_hold_required() {
        let seq = TestSequence {
            name: "t".into(),
            steps: vec![TestStep {
                signal: "VBAT".into(),
                min: Some(12.0),
                max: Some(13.0),
                hold_secs: 0.0,
                timeout_secs: 5.0,
                label: None,
            }],
        };
        let mut runner = SequenceRunner::start(seq);
        runner.tick(&store_with("VBAT", 0.0, 12.5), 0.0);
        assert_eq!(runner.outcomes()[0].status, StepStatus::Passed);
        assert_eq!(runner.overall_passed(), Some(true));
    }

    #[test]
    fn hold_resets_when_value_drifts_out_of_range() {
        let seq = TestSequence {
            name: "t".into(),
            steps: vec![TestStep {
                signal: "VBAT".into(),
                min: Some(12.0),
                max: Some(13.0),
                hold_secs: 2.0,
                timeout_secs: 10.0,
                label: None,
            }],
        };
        let mut runner = SequenceRunner::start(seq);
        runner.tick(&store_with("VBAT", 0.0, 12.5), 0.0);
        assert_eq!(runner.outcomes()[0].status, StepStatus::Running);
        // Drifts out of range at t=1: the 2s hold has to restart.
        runner.tick(&store_with("VBAT", 1.0, 13.5), 1.0);
        runner.tick(&store_with("VBAT", 1.5, 12.5), 1.5);
        // Only 1.5s of continuous in-range time so far (1.5 -> 3.0): not enough yet.
        runner.tick(&store_with("VBAT", 3.0, 12.5), 3.0);
        assert_eq!(runner.outcomes()[0].status, StepStatus::Running);
        runner.tick(&store_with("VBAT", 3.6, 12.5), 3.6);
        assert_eq!(runner.outcomes()[0].status, StepStatus::Passed);
    }

    #[test]
    fn fails_on_timeout_with_no_data() {
        let seq = TestSequence {
            name: "t".into(),
            steps: vec![TestStep {
                signal: "MISSING".into(),
                min: Some(1.0),
                max: Some(2.0),
                hold_secs: 0.0,
                timeout_secs: 3.0,
                label: None,
            }],
        };
        let mut runner = SequenceRunner::start(seq);
        let empty = SignalStore::new(64);
        runner.tick(&empty, 0.0);
        assert_eq!(runner.outcomes()[0].status, StepStatus::Running);
        runner.tick(&empty, 3.0);
        assert_eq!(runner.outcomes()[0].status, StepStatus::Failed);
        assert!(runner.outcomes()[0].detail.contains("no data"));
        assert_eq!(runner.overall_passed(), Some(false));
        assert!(runner.is_done());
    }

    #[test]
    fn stops_at_first_failure_and_does_not_start_later_steps() {
        let seq = TestSequence {
            name: "t".into(),
            steps: vec![
                TestStep {
                    signal: "A".into(),
                    min: Some(100.0),
                    max: Some(200.0),
                    hold_secs: 0.0,
                    timeout_secs: 1.0,
                    label: None,
                },
                TestStep {
                    signal: "B".into(),
                    min: Some(0.0),
                    max: Some(1.0),
                    hold_secs: 0.0,
                    timeout_secs: 1.0,
                    label: None,
                },
            ],
        };
        let mut runner = SequenceRunner::start(seq);
        let store = store_with("A", 0.0, 0.0); // out of range -> will time out
        runner.tick(&store, 0.0);
        runner.tick(&store, 1.0);
        assert_eq!(runner.outcomes()[0].status, StepStatus::Failed);
        assert_eq!(runner.outcomes()[1].status, StepStatus::Pending);
        assert!(runner.is_done());
    }

    #[test]
    fn advances_through_multiple_steps() {
        let seq = TestSequence {
            name: "t".into(),
            steps: vec![
                TestStep {
                    signal: "A".into(),
                    min: Some(0.0),
                    max: Some(1.0),
                    hold_secs: 0.0,
                    timeout_secs: 1.0,
                    label: None,
                },
                TestStep {
                    signal: "B".into(),
                    min: Some(0.0),
                    max: Some(1.0),
                    hold_secs: 0.0,
                    timeout_secs: 1.0,
                    label: None,
                },
            ],
        };
        let mut runner = SequenceRunner::start(seq);
        let mut store = SignalStore::new(64);
        store.ingest(&Sample::new("A", 0.5, 0.0));
        runner.tick(&store, 0.0);
        assert_eq!(runner.outcomes()[0].status, StepStatus::Passed);
        assert_eq!(runner.outcomes()[1].status, StepStatus::Pending);

        store.ingest(&Sample::new("B", 0.5, 0.1));
        runner.tick(&store, 0.1);
        assert_eq!(runner.outcomes()[1].status, StepStatus::Passed);
        assert_eq!(runner.overall_passed(), Some(true));
    }

    #[test]
    fn parses_from_toml() {
        let toml = r#"
            name = "power-on"

            [[step]]
            signal = "VBAT"
            min = 12.0
            max = 13.0
            hold_secs = 1.0
            timeout_secs = 5.0
            label = "battery settles"
        "#;
        let seq = TestSequence::from_toml_str(toml).unwrap();
        assert_eq!(seq.name, "power-on");
        assert_eq!(seq.steps.len(), 1);
        assert_eq!(seq.steps[0].label.as_deref(), Some("battery settles"));
    }
}
