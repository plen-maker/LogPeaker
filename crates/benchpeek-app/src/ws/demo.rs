//! DEMO data source. Everything the workspace UI shows about the "device" is
//! produced here, deterministically, and the UI labels it as demo data. A real
//! backend would replace this module behind the same accessors.

use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Clone)]
pub struct LogLine {
    pub id: u64,
    /// Milliseconds since local midnight.
    pub ms: u32,
    pub proc_: &'static str,
    pub msg: String,
    pub level: Level,
}

pub fn fmt_ts(ms: u32) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02}.{:03}", s / 3600 % 24, s / 60 % 60, s % 60, ms % 1000)
}

pub struct Issue {
    pub title: &'static str,
    pub service: &'static str,
    pub summary: &'static str,
    /// Probable cause is an inference; `observed` is the actual log message.
    pub probable_cause: &'static str,
    pub log_id: u64,
    pub ms: u32,
    pub observed: String,
}

pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next() % (hi - lo).max(1) as u64) as u32
    }
    pub fn unit(&mut self) -> f32 {
        (self.next() % 10_000) as f32 / 10_000.0
    }
}

const INFO: &[(&str, &str)] = &[
    ("kernel", "usb 1-1: new high-speed USB device number 3 using ehci-platform"),
    ("kernel", "mmc0: new ultra high speed SDR104 SDHC card at address 0001"),
    ("kernel", "stm32-dwmac 482c0000.ethernet eth0: Link is Up - 1Gbps/Full"),
    ("systemd[1]", "Started Session 4 of User root."),
    ("systemd[1]", "Starting Rotate log files..."),
    ("systemd[1]", "logrotate.service: Deactivated successfully."),
    ("systemd[1]", "Finished Rotate log files."),
    ("weston[688]", "output 'DSI-1' enabled with head(s) DSI-1"),
    ("weston[688]", "libinput: event3 - goodix-ts: device added"),
    ("sshd[731]", "Accepted publickey for root from 192.168.7.2 port 51422 ssh2"),
    ("sshd[731]", "pam_unix(sshd:session): session opened for user root"),
    ("chronyd[540]", "Selected source 162.159.200.1 (time.cloudflare.com)"),
    ("chronyd[540]", "System clock wrong by 0.000214 seconds"),
    ("dbus-daemon[401]", "[system] Successfully activated service 'org.freedesktop.timedate1'"),
    ("systemd-journald[214]", "Runtime Journal (/run/log/journal) is 8.0M, max 76.4M."),
    ("cpufreq", "policy0: switching governor to schedutil"),
    ("thermal", "cpu-thermal: temperature 47.2 C (below trip point 95.0 C)"),
    ("optee", "OP-TEE: revision 4.0 (gfdd4b9c)"),
    ("gpu-gcnano", "job queue idle, powering down clock domain"),
    ("systemd-networkd[412]", "eth0: Gained carrier"),
];

pub struct Demo {
    pub lines: VecDeque<LogLine>,
    pub issues: Vec<Issue>,
    rng: Rng,
    next_id: u64,
    next_ms: u32,
    next_at: f64,
    pub cpu_hist: VecDeque<f32>,
    pub cpu: f32,
    pub mem_used_mb: f32,
    pub mem_total_mb: f32,
    pub since_start_lines: u64,
    metric_at: f64,
    last_metric: f64,
}

pub fn now_ms() -> u32 {
    use chrono::Timelike;
    let n = chrono::Local::now();
    n.num_seconds_from_midnight() * 1000 + n.timestamp_subsec_millis()
}

impl Demo {
    pub fn new() -> Self {
        let mut d = Self {
            lines: VecDeque::new(),
            issues: Vec::new(),
            rng: Rng::new(7),
            next_id: 1,
            next_ms: 0,
            next_at: 0.0,
            cpu_hist: VecDeque::from(vec![22.0; 28]),
            cpu: 22.0,
            mem_used_mb: 1410.0,
            mem_total_mb: 4096.0,
            since_start_lines: 0,
            metric_at: 0.0,
            last_metric: 0.0,
        };
        let end = now_ms();
        d.next_ms = end.saturating_sub(20 * 60 * 1000);
        let total = 1200;
        let dhcp_at = total - 190;
        let nd_at = total - 178;
        for i in 0..total {
            d.next_ms += d.rng.range(400, 1900);
            if i == dhcp_at {
                let id = d.push(
                    "systemd-networkd[412]",
                    "eth0: DHCPv4 client: no lease received, request timed out".into(),
                    Level::Error,
                );
                d.issues.push(Issue {
                    title: "DHCP timeout",
                    service: "systemd-networkd",
                    summary: "No response from DHCP server",
                    probable_cause: "The DHCP server is unreachable or the Ethernet link dropped during lease renewal. Check the cable, the switch port and that a DHCP server is running on the network.",
                    log_id: id,
                    ms: d.next_ms,
                    observed: "eth0: DHCPv4 client: no lease received, request timed out".into(),
                });
            } else if i == nd_at {
                let id = d.push(
                    "systemd[1]",
                    "systemd-networkd.service: Failed with result 'exit-code'.".into(),
                    Level::Error,
                );
                d.issues.insert(0, Issue {
                    title: "Network service failed",
                    service: "networkd.service",
                    summary: "Service exited with a non-zero status",
                    probable_cause: "networkd stopped after the DHCP request timed out and its restart limit was reached. Restoring the network and restarting the service should clear it.",
                    log_id: id,
                    ms: d.next_ms,
                    observed: "systemd-networkd.service: Failed with result 'exit-code'.".into(),
                });
            } else if i > dhcp_at && i < nd_at && i % 4 == 0 {
                d.push("systemd-networkd[412]", "eth0: DHCPv4 client: retrying discover".into(), Level::Warn);
            } else {
                d.push_random();
            }
        }
        // Anchor the newest generated line just before "now".
        let delta = end as i64 - 1500 - d.next_ms as i64;
        let shift = |ms: u32| (ms as i64 + delta).max(0) as u32;
        for l in d.lines.iter_mut() {
            l.ms = shift(l.ms);
        }
        for i in d.issues.iter_mut() {
            i.ms = shift(i.ms);
        }
        d.next_ms = shift(d.next_ms);
        d
    }

    fn push(&mut self, proc_: &'static str, msg: String, level: Level) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.since_start_lines += 1;
        self.lines.push_back(LogLine { id, ms: self.next_ms, proc_, msg, level });
        if self.lines.len() > 50_000 {
            self.lines.pop_front();
        }
        id
    }

    fn push_random(&mut self) {
        let k = self.rng.range(0, INFO.len() as u32) as usize;
        let (p, m) = INFO[k];
        self.push(p, m.to_string(), Level::Info);
    }

    /// Advance the demo device. `capturing` gates new log lines.
    pub fn tick(&mut self, now: f64, capturing: bool) {
        if capturing && now >= self.next_at {
            self.next_ms = now_ms().max(self.next_ms + 1);
            self.push_random();
            self.next_at = now + 0.35 + self.rng.unit() as f64 * 0.9;
        }
        if now >= self.metric_at {
            self.metric_at = now + 0.6;
            self.last_metric = now;
            let target = 24.0 + 14.0 * ((now * 0.35).sin() as f32) + self.rng.unit() * 10.0;
            self.cpu += (target - self.cpu) * 0.5;
            self.cpu_hist.push_back(self.cpu);
            while self.cpu_hist.len() > 28 {
                self.cpu_hist.pop_front();
            }
            self.mem_used_mb = (self.mem_used_mb + (self.rng.unit() - 0.5) * 12.0).clamp(1300.0, 1600.0);
        }
    }

    /// 0..1 progress between two metric samples, for smooth chart scrolling.
    pub fn phase(&self, now: f64) -> f32 {
        ((now - self.last_metric) / 0.6).clamp(0.0, 1.0) as f32
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        let first = self.lines.front()?.id;
        if id < first {
            return None;
        }
        let idx = (id - first) as usize;
        (idx < self.lines.len()).then_some(idx)
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// A deterministic set of lines for a saved session (used by Replay).
    pub fn session_lines(seed: u64, count: usize, errors: usize) -> Vec<LogLine> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::new();
        let mut ms = 9 * 3600 * 1000 + rng.range(0, 3600 * 1000);
        let err_at: Vec<usize> = (0..errors).map(|k| (k + 1) * count / (errors + 1)).collect();
        for i in 0..count {
            ms += rng.range(300, 2200);
            let (p, m, l) = if err_at.contains(&i) {
                ("systemd[1]", "unit entered failed state".to_string(), Level::Error)
            } else {
                let (p, m) = INFO[rng.range(0, INFO.len() as u32) as usize];
                (p, m.to_string(), Level::Info)
            };
            out.push(LogLine { id: i as u64 + 1, ms, proc_: p, msg: m, level: l });
        }
        out
    }
}

// ---------------------------------------------------------------- files ----

#[derive(Clone)]
pub struct Node {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    /// Seconds ago (relative to app start), for "Modified".
    pub age_s: u64,
    pub content: String,
    pub children: Vec<Node>,
}

impl Node {
    fn file(name: &str, age_s: u64, content: String) -> Self {
        Self { name: name.into(), dir: false, size: content.len() as u64, age_s, content, children: vec![] }
    }
    fn dir(name: &str, age_s: u64, children: Vec<Node>) -> Self {
        Self { name: name.into(), dir: true, size: 0, age_s, content: String::new(), children }
    }
}

fn gen_log(seed: u64, lines: usize) -> String {
    let mut rng = Rng::new(seed);
    let mut s = String::new();
    let mut sec = 6 * 3600 + rng.range(0, 3600);
    for _ in 0..lines {
        sec += rng.range(1, 9);
        let (p, m) = INFO[rng.range(0, INFO.len() as u32) as usize];
        s.push_str(&format!("Sep 18 {:02}:{:02}:{:02} yocto-devkit {}: {}\n", sec / 3600 % 24, sec / 60 % 60, sec % 60, p, m));
    }
    s
}

pub fn make_fs() -> Node {
    let boot = {
        let mut s = String::new();
        for (i, l) in [
            "[  OK  ] Mounted /boot.",
            "[  OK  ] Reached target Local File Systems.",
            "         Starting Create Volatile Files and Directories...",
            "[  OK  ] Finished Create Volatile Files and Directories.",
            "[  OK  ] Started Network Configuration.",
            "[FAILED] Failed to start Wait for Network to be Configured.",
            "[  OK  ] Reached target Multi-User System.",
            "[  OK  ] Started Weston, a Wayland compositor.",
        ]
        .iter()
        .enumerate()
        {
            s.push_str(l);
            s.push('\n');
            let _ = i;
        }
        s
    };
    let journal = Node::dir(
        "journal",
        3_600,
        vec![
            Node { size: 8_388_608, ..Node::file("system.journal", 240, "Binary journal - preview is not available for binary files in the demo.\n".into()) },
            Node { size: 4_194_304, ..Node::file("user-0.journal", 900, "Binary journal - preview is not available for binary files in the demo.\n".into()) },
        ],
    );
    let log = Node::dir(
        "log",
        120,
        vec![
            journal,
            Node::file("boot.log", 7_200, boot),
            Node::file("kern.log", 300, gen_log(3, 320)),
            Node::file("syslog", 45, gen_log(5, 480)),
        ],
    );
    let var = Node::dir(
        "var",
        120,
        vec![
            log,
            Node::dir("lib", 86_400, vec![Node::file("dhcpcd.leases", 4_000, "# demo lease store (empty)\n".into())]),
            Node::dir("cache", 86_400, vec![]),
        ],
    );
    let etc = Node::dir(
        "etc",
        172_800,
        vec![
            Node::file("hostname", 172_800, "yocto-devkit\n".into()),
            Node::file("os-release", 172_800, "ID=poky\nNAME=\"Poky (Yocto Project Reference Distro)\"\n".into()),
        ],
    );
    let home = Node::dir("home", 90_000, vec![Node::dir("root", 600, vec![])]);
    Node::dir("Device", 0, vec![etc, home, var])
}

pub fn fmt_size(n: u64) -> String {
    if n == 0 {
        "--".into()
    } else if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / 1048576.0)
    }
}

pub fn fmt_age(s: u64) -> String {
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86399 => format!("{} h ago", s / 3600),
        _ => format!("{} d ago", s / 86400),
    }
}

// ------------------------------------------------------------- sessions ----

pub struct Session {
    pub name: String,
    pub when: String,
    pub duration_s: u32,
    pub errors: usize,
    pub lines: usize,
    pub seed: u64,
}

pub fn make_sessions() -> Vec<Session> {
    let mk = |n: &str, w: &str, d: u32, e: usize, l: usize, s: u64| Session {
        name: n.into(),
        when: w.into(),
        duration_s: d,
        errors: e,
        lines: l,
        seed: s,
    };
    vec![
        mk("Boot & network bring-up", "Today, 18:12", 1_284, 2, 640, 11),
        mk("Weston stress run", "Yesterday, 22:40", 3_722, 0, 1_910, 12),
        mk("Thermal soak test", "Sep 16, 09:05", 7_205, 1, 3_400, 13),
        mk("USB gadget enumeration", "Sep 15, 14:31", 512, 0, 260, 14),
        mk("OP-TEE first boot", "Sep 12, 17:48", 940, 3, 480, 15),
    ]
}

pub fn fmt_dur(s: u32) -> String {
    if s >= 3600 {
        format!("{} h {:02} min", s / 3600, s / 60 % 60)
    } else {
        format!("{} min {:02} s", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_point_at_their_log_lines() {
        let d = Demo::new();
        assert_eq!(d.issues.len(), 2);
        for i in &d.issues {
            let idx = d.index_of(i.log_id).expect("issue line is in the buffer");
            let l = &d.lines[idx];
            assert_eq!(l.msg, i.observed);
            assert_eq!(l.ms, i.ms);
            assert!(l.level == Level::Error);
        }
    }

    #[test]
    fn dhcp_timeout_precedes_the_service_failure() {
        let d = Demo::new();
        let failed = d.issues.iter().find(|i| i.title == "Network service failed").unwrap();
        let dhcp = d.issues.iter().find(|i| i.title == "DHCP timeout").unwrap();
        assert!(dhcp.ms < failed.ms, "cause must come before effect");
    }

    #[test]
    fn timestamps_are_monotonic_and_not_in_the_future() {
        let d = Demo::new();
        let mut prev = 0;
        for l in &d.lines {
            assert!(l.ms >= prev);
            prev = l.ms;
        }
        assert!(prev <= now_ms());
    }

    #[test]
    fn formatting_helpers() {
        assert_eq!(fmt_ts(3_723_004), "01:02:03.004");
        assert_eq!(fmt_size(0), "--");
        assert_eq!(fmt_size(1536), "1.5 KB");
        assert_eq!(fmt_age(30), "just now");
        assert_eq!(fmt_age(7200), "2 h ago");
        assert_eq!(fmt_dur(3725), "1 h 02 min");
    }

    #[test]
    fn session_replay_is_deterministic() {
        let a = Demo::session_lines(11, 100, 2);
        let b = Demo::session_lines(11, 100, 2);
        assert_eq!(a.len(), 100);
        assert_eq!(a.iter().map(|l| l.ms).collect::<Vec<_>>(), b.iter().map(|l| l.ms).collect::<Vec<_>>());
        assert_eq!(a.iter().filter(|l| l.level == Level::Error).count(), 2);
    }
}
