//! Remote build-server helpers for the Yocto dashboard: a fire-and-forget
//! command trigger (for kicking off a detached build) and periodic host
//! stats polling over SSH. Log streaming reuses `source::LogWatcher` as-is
//! (tailing a build log is the same mechanism as tailing `journalctl`).

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Runs `command` on `user@host` over SSH, fire-and-forget - for kicking
/// off a detached remote job (e.g. `docker exec -d ... bitbake ...`)
/// without waiting for or reporting its result. Key-based auth only, same
/// as the rest of benchpeek's SSH usage.
pub fn run_remote_command(host: String, user: String, command: String) {
    thread::spawn(move || {
        let target = format!("{user}@{host}");
        let _ = Command::new("ssh")
            .args(crate::source::ssh_identity_args())
            .args([
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=5",
                "-o",
                "StrictHostKeyChecking=accept-new",
                &target,
                &command,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    });
}

#[derive(Clone, Default)]
pub struct DeviceStats {
    pub reachable: bool,
    pub cpu_load_pct: Option<f32>,
    pub mem_used_pct: Option<f32>,
    pub disk_used_pct: Option<f32>,
}

pub struct StatsWatcher {
    pub rx: Receiver<DeviceStats>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl StatsWatcher {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for StatsWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Polls `user@host` every `interval` for CPU/RAM/disk usage via a single
/// SSH round-trip. Deliberately not a persistent connection - a dashboard
/// stat that's fine being a few seconds stale doesn't need one.
pub fn spawn_stats_poll(host: String, user: String, interval: Duration) -> StatsWatcher {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let stop_thread = stop.clone();

    let handle = thread::spawn(move || {
        while !stop_thread.load(Ordering::Relaxed) {
            if tx.send(poll_once(&host, &user)).is_err() {
                break;
            }
            thread::sleep(interval);
        }
    });

    StatsWatcher {
        rx,
        stop,
        handle: Some(handle),
    }
}

fn poll_once(host: &str, user: &str) -> DeviceStats {
    let target = format!("{user}@{host}");
    let output = Command::new("ssh")
        .args(crate::source::ssh_identity_args())
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "StrictHostKeyChecking=accept-new",
            &target,
            "nproc; cat /proc/loadavg; free -b; df -B1 /",
        ])
        .output();

    let Ok(output) = output else {
        return DeviceStats::default();
    };
    if !output.status.success() {
        return DeviceStats {
            reachable: false,
            ..Default::default()
        };
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = text.lines();

    let nproc: f32 = lines
        .next()
        .and_then(|l| l.trim().parse().ok())
        .unwrap_or(1.0);
    let cpu_load_pct = lines
        .next()
        .and_then(|l| l.split_whitespace().next()?.parse::<f32>().ok())
        .map(|load1| (load1 / nproc.max(1.0) * 100.0).min(999.0));

    // `free -b` output: header, then "Mem: total used free shared buff/cache available".
    let mem_used_pct = lines.clone().find(|l| l.starts_with("Mem:")).and_then(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        let total: f64 = f.get(1)?.parse().ok()?;
        let used: f64 = f.get(2)?.parse().ok()?;
        (total > 0.0).then_some((used / total * 100.0) as f32)
    });

    // `df -B1 /` output: header, then a data line with an "NN%" use column.
    // (The header's "Use%" also contains '%' but does not parse as a number.)
    let disk_used_pct = lines.clone().find_map(|l| {
        l.split_whitespace()
            .find_map(|tok| tok.strip_suffix('%')?.parse::<f32>().ok())
    });

    DeviceStats {
        reachable: true,
        cpu_load_pct,
        mem_used_pct,
        disk_used_pct,
    }
}
