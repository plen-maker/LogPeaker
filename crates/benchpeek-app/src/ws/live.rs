//! Live SSH link to a real device: streams `journalctl` into the Live monitor
//! and polls CPU/RAM/disk, reusing the app's existing `LogWatcher` and
//! `StatsWatcher` (key-based auth only). Everything else in the Workspace UI
//! (files, sessions, diagnostics rules) is still demo data.

use std::collections::VecDeque;
use std::time::Duration;

use benchpeek_core::{LogEvent, LogLevel};

use super::demo::{now_ms, Level, LogLine};
use crate::source::{self, LogWatcher};
use crate::yocto::{self, DeviceStats, StatsWatcher};

pub const STATS_INTERVAL_S: f64 = 2.0;
const MAX_LINES: usize = 20_000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    Connected,
    Unreachable,
}

pub struct Live {
    pub host: String,
    pub user: String,
    log: LogWatcher,
    stats: StatsWatcher,
    pub lines: VecDeque<LogLine>,
    next_id: u64,
    pub stats_now: DeviceStats,
    got_stats: bool,
    pub cpu_hist: VecDeque<f32>,
    last_metric: f64,
}

impl Live {
    pub fn connect(host: String, user: String, identity: Option<String>) -> Self {
        source::set_ssh_identity(identity);
        let log = source::spawn_log_watch(
            host.clone(),
            user.clone(),
            "journalctl -f -n 300 -o short --no-pager".to_string(),
        );
        let stats = yocto::spawn_stats_poll(host.clone(), user.clone(), Duration::from_secs_f64(STATS_INTERVAL_S));
        Self {
            host,
            user,
            log,
            stats,
            lines: VecDeque::new(),
            next_id: 1,
            stats_now: DeviceStats::default(),
            got_stats: false,
            cpu_hist: VecDeque::from(vec![0.0; 28]),
            last_metric: 0.0,
        }
    }

    pub fn target(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }

    pub fn state(&self) -> LinkState {
        match (self.got_stats, self.stats_now.reachable) {
            (false, _) => LinkState::Connecting,
            (true, true) => LinkState::Connected,
            (true, false) => LinkState::Unreachable,
        }
    }

    /// 0..1 progress between two stats samples, for smooth chart scrolling.
    pub fn phase(&self, now: f64) -> f32 {
        ((now - self.last_metric) / STATS_INTERVAL_S).clamp(0.0, 1.0) as f32
    }

    pub fn pump(&mut self, now: f64, ingest_logs: bool) {
        while let Ok(e) = self.log.rx.try_recv() {
            if ingest_logs {
                self.push(e);
            }
        }
        while let Ok(s) = self.stats.rx.try_recv() {
            self.got_stats = true;
            if s.reachable {
                if let Some(c) = s.cpu_load_pct {
                    self.cpu_hist.push_back(c.clamp(0.0, 100.0));
                    while self.cpu_hist.len() > 28 {
                        self.cpu_hist.pop_front();
                    }
                    self.last_metric = now;
                }
            }
            self.stats_now = s;
        }
    }

    fn push(&mut self, e: LogEvent) {
        let parsed = split_short(&e.message);
        let level = match e.level {
            LogLevel::Fault => Level::Error,
            LogLevel::Warn => Level::Warn,
            LogLevel::Info => Level::Info,
        };
        let id = self.next_id;
        self.next_id += 1;
        let ms = parsed.secs.map_or_else(now_ms, |s| s * 1000);
        self.lines.push_back(LogLine { id, ms, proc_: parsed.proc_, msg: parsed.msg, level });
        if self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
    }

    pub fn stop(&mut self) {
        self.log.stop();
        self.stats.stop();
    }
}

/// One parsed `journalctl -o short` line.
pub struct Parsed {
    /// Seconds since local midnight, from the line's own timestamp.
    pub secs: Option<u32>,
    pub proc_: String,
    pub msg: String,
}

/// "Sep 20 19:05:01 host proc[123]: text" -> timestamp, process, message.
/// Unparseable lines keep the whole text as the message.
pub fn split_short(line: &str) -> Parsed {
    let whole = || Parsed { secs: None, proc_: String::new(), msg: line.to_string() };
    let mut rest = line.trim_start();
    let mut head = [""; 4]; // month, day, time, host (the day may be space-padded)
    for h in head.iter_mut() {
        let Some(end) = rest.find(char::is_whitespace) else { return whole() };
        *h = &rest[..end];
        rest = rest[end..].trim_start();
    }
    let hms: Vec<u32> = head[2].split(':').filter_map(|p| p.parse().ok()).collect();
    if hms.len() != 3 {
        return whole();
    }
    let secs = Some(hms[0] * 3600 + hms[1] * 60 + hms[2]);
    match rest.split_once(": ") {
        Some((p, m)) => Parsed { secs, proc_: p.to_string(), msg: m.to_string() },
        None => Parsed { secs, proc_: String::new(), msg: rest.to_string() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_journal_short_line() {
        let p = split_short("Sep 20 19:05:01 chris-pc sshd-session[34591]: Accepted publickey for yocto");
        assert_eq!(p.proc_, "sshd-session[34591]");
        assert_eq!(p.msg, "Accepted publickey for yocto");
        assert_eq!(p.secs, Some(19 * 3600 + 5 * 60 + 1));
    }

    #[test]
    fn handles_padded_day_and_kernel_lines() {
        let p = split_short("Sep  2 09:01:10 board kernel: usb 1-1: new device");
        assert_eq!(p.proc_, "kernel");
        assert_eq!(p.msg, "usb 1-1: new device");
        assert_eq!(p.secs, Some(9 * 3600 + 60 + 10));
    }

    #[test]
    fn keeps_unparseable_lines_whole() {
        let p = split_short("-- Journal begins at Mon 2026-09-14 --");
        assert_eq!(p.proc_, "");
        assert!(p.msg.contains("Journal begins"));
        assert_eq!(p.secs, None);
    }
}
