//! Data sources. Each runs on its own thread and pushes [`Sample`]s through
//! an mpsc channel. Serial data goes through the pluggable [`Decoder`];
//! the simulated and replay sources emit samples directly.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use benchpeek_core::{classify_line, load_replay, Decoder, LogEvent, Sample};

pub enum SourceKind {
    /// A fake board: three plausible signals with a battery fault injected
    /// after 15 s so the rule engine has something to catch.
    Simulated,
    Serial {
        port: String,
        baud: u32,
    },
    Replay {
        path: String,
    },
}

/// Owns the source thread; stops it on `stop()` or drop.
pub struct SourceHandle {
    pub rx: Receiver<Sample>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl SourceHandle {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for SourceHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn spawn(kind: SourceKind, mut decoder: Box<dyn Decoder>) -> SourceHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let stop_thread = stop.clone();

    let handle = thread::spawn(move || {
        let start = Instant::now();
        let result = match kind {
            SourceKind::Simulated => run_sim(decoder.as_mut(), &tx, &stop_thread, start),
            SourceKind::Serial { port, baud } => {
                run_serial(&port, baud, decoder.as_mut(), &tx, &stop_thread, start)
            }
            SourceKind::Replay { path } => run_replay(&path, &tx, &stop_thread),
        };
        if let Err(e) = result {
            eprintln!("benchpeek source error: {e:#}");
        }
    });

    SourceHandle {
        rx,
        stop,
        handle: Some(handle),
    }
}

/// Emits the same `KEY=VALUE` text a real board would send over UART, run
/// through the active decoder. This exercises the exact decode path serial
/// input would (built-in or WASM plugin alike) without needing hardware.
fn run_sim(
    decoder: &mut dyn Decoder,
    tx: &Sender<Sample>,
    stop: &AtomicBool,
    start: Instant,
) -> Result<()> {
    while !stop.load(Ordering::Relaxed) {
        let t = start.elapsed().as_secs_f64();
        let noise = || (fastrand::f64() - 0.5) * 0.1;

        // Battery holds at 12.6 V, then sags past the 11.5 V limit after 15 s.
        let vbat = if t < 15.0 {
            12.6
        } else {
            12.6 - (t - 15.0) * 0.12
        };
        let vbat = (vbat + noise()).max(0.0);
        let rpm = 850.0 + (t * 1.7).sin() * 40.0 + noise() * 60.0;
        let temp = 40.0 + (t * 0.2).sin() * 6.0 + noise() * 10.0;

        let line = format!("VBAT={vbat:.3} RPM={rpm:.1} TEMP={temp:.2}\n");
        for s in decoder.decode(line.as_bytes(), t) {
            let _ = tx.send(s);
        }

        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

/// Boards commonly reset/re-enumerate their USB-CDC interface right when a
/// host first opens the port (e.g. a DTR-triggered target reset on ST-Link
/// VCPs), which can surface as a transient read error milliseconds in. A
/// permanently-dead thread would make "auto mode" useless against that, so
/// any I/O error here reopens the port instead of giving up; the caller's
/// hotplug watch (port disappears from the OS list) is what actually stops
/// retrying once the board is truly gone.
fn run_serial(
    port: &str,
    baud: u32,
    decoder: &mut dyn Decoder,
    tx: &Sender<Sample>,
    stop: &AtomicBool,
    start: Instant,
) -> Result<()> {
    while !stop.load(Ordering::Relaxed) {
        if let Err(e) = serial_session(port, baud, decoder, tx, stop, start) {
            eprintln!("benchpeek source: {port} error, reconnecting: {e:#}");
            thread::sleep(Duration::from_millis(300));
        }
    }
    Ok(())
}

fn serial_session(
    port: &str,
    baud: u32,
    decoder: &mut dyn Decoder,
    tx: &Sender<Sample>,
    stop: &AtomicBool,
    start: Instant,
) -> Result<()> {
    let mut sp = serialport::new(port, baud)
        .timeout(Duration::from_millis(200))
        .open()?;
    let mut buf = [0u8; 2048];
    while !stop.load(Ordering::Relaxed) {
        match sp.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                let t = start.elapsed().as_secs_f64();
                for s in decoder.decode(&buf[..n], t) {
                    let _ = tx.send(s);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Owns the log-watch thread; stops it (and kills the underlying `ssh`) on
/// `stop()` or drop. Independent of [`SourceHandle`] - watching a board's
/// logs over SSH and reading its serial telemetry are separate concerns,
/// often against different hosts entirely, so both can run at once.
pub struct LogWatcher {
    pub rx: Receiver<LogEvent>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    handle: Option<JoinHandle<()>>,
}

impl LogWatcher {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Unblocks the thread's blocking read on the ssh child's stdout -
        // the atomic flag alone can't interrupt an in-progress pipe read.
        if let Some(child) = self.child.lock().unwrap().as_mut() {
            let _ = child.kill();
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for LogWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Watches `command`'s output on `user@host` over `ssh` (e.g. `journalctl -f
/// -o cat`), classifying each line's severity by keyword. Key-based auth
/// only (`BatchMode=yes`) - a spawned, non-interactive process has no tty to
/// prompt a password into, so it fails fast instead of hanging; set up
/// `ssh-copy-id` first if the target only has password auth today.
pub fn spawn_log_watch(host: String, user: String, command: String) -> LogWatcher {
    let stop = Arc::new(AtomicBool::new(false));
    let child_slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let (tx, rx) = std::sync::mpsc::channel();
    let stop_thread = stop.clone();
    let child_thread = child_slot.clone();

    let handle = thread::spawn(move || {
        let start = Instant::now();
        while !stop_thread.load(Ordering::Relaxed) {
            if let Err(e) = log_session(
                &host,
                &user,
                &command,
                &tx,
                &stop_thread,
                &child_thread,
                start,
            ) {
                eprintln!("benchpeek log watch: {host} error, reconnecting: {e:#}");
                thread::sleep(Duration::from_millis(1000));
            }
        }
    });

    LogWatcher {
        rx,
        stop,
        child: child_slot,
        handle: Some(handle),
    }
}

fn log_session(
    host: &str,
    user: &str,
    command: &str,
    tx: &Sender<LogEvent>,
    stop: &AtomicBool,
    child_slot: &Mutex<Option<Child>>,
    start: Instant,
) -> Result<()> {
    let target = format!("{user}@{host}");
    let mut child = Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "StrictHostKeyChecking=accept-new",
            &target,
            command,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawning ssh to {target}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("ssh child has no stdout"))?;
    *child_slot.lock().unwrap() = Some(child);

    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let t = start.elapsed().as_secs_f64();
        let level = classify_line(&line);
        if tx
            .send(LogEvent {
                t,
                level,
                message: line,
            })
            .is_err()
        {
            break;
        }
    }

    // kill() is a no-op if ssh already exited on its own (e.g. auth
    // rejected it and closed stdout) - either way, wait() reaps it and
    // gives us the real exit status.
    let mut child_opt = child_slot.lock().unwrap().take();
    if let Some(c) = &mut child_opt {
        let _ = c.kill();
    }
    let status = child_opt.map(|mut c| c.wait());

    if stop.load(Ordering::Relaxed) {
        return Ok(());
    }
    match status {
        Some(Ok(s)) if s.success() => Ok(()),
        Some(Ok(s)) => Err(anyhow!("ssh exited with {s}")),
        Some(Err(e)) => Err(e).context("waiting on ssh child"),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 127.0.0.1 refuses batch-mode auth (no key set up for this bogus
    /// user) essentially immediately, so by the time we call `stop()` the
    /// watcher is very likely sitting in its 1s reconnect backoff sleep.
    /// `stop()` must still return promptly - it must not block on that
    /// sleep, and it must not busy-loop retrying with zero backoff on a
    /// persistent auth failure either (a real bug caught by reasoning
    /// through this exact scenario: the first version of `log_session`
    /// never checked ssh's exit status, so an immediate auth failure was
    /// indistinguishable from a clean shutdown and skipped the backoff).
    /// Direct unit test of the bug the above test's doc comment describes:
    /// `log_session` must return `Err` on a rejected connection so the
    /// caller's retry loop actually sleeps before trying again.
    #[test]
    fn failed_connection_is_an_error_not_ok() {
        let stop = AtomicBool::new(false);
        let child_slot: Mutex<Option<Child>> = Mutex::new(None);
        let (tx, _rx) = std::sync::mpsc::channel();
        let result = log_session(
            "127.0.0.1",
            "nonexistent-benchpeek-test-user",
            "true",
            &tx,
            &stop,
            &child_slot,
            Instant::now(),
        );
        assert!(
            result.is_err(),
            "a rejected ssh connection must surface as Err, not Ok, or the retry loop skips its backoff"
        );
    }

    #[test]
    fn stop_is_prompt_during_reconnect_backoff() {
        let mut watcher = spawn_log_watch(
            "127.0.0.1".to_string(),
            "nonexistent-benchpeek-test-user".to_string(),
            "true".to_string(),
        );
        thread::sleep(Duration::from_millis(200));
        let started_stop = Instant::now();
        watcher.stop();
        assert!(
            started_stop.elapsed() < Duration::from_secs(2),
            "stop() blocked - likely waiting out the reconnect backoff sleep"
        );
    }
}

fn run_replay(path: &str, tx: &Sender<Sample>, stop: &AtomicBool) -> Result<()> {
    let samples = load_replay(path)?;
    let mut prev_t: Option<f64> = None;
    for s in samples {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Some(pt) = prev_t {
            let dt = (s.t - pt).clamp(0.0, 1.0);
            if dt > 0.0 {
                thread::sleep(Duration::from_secs_f64(dt));
            }
        }
        prev_t = Some(s.t);
        if tx.send(s).is_err() {
            break;
        }
    }
    Ok(())
}
