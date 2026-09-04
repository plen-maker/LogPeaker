//! Data sources. Each runs on its own thread and pushes [`Sample`]s through
//! an mpsc channel. Serial data goes through the pluggable [`Decoder`];
//! the simulated and replay sources emit samples directly.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::Result;
use benchpeek_core::{load_replay, Decoder, Sample};

pub enum SourceKind {
    /// A fake board: three plausible signals with a battery fault injected
    /// after 15 s so the rule engine has something to catch.
    Simulated,
    Serial { port: String, baud: u32 },
    Replay { path: String },
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
fn run_sim(decoder: &mut dyn Decoder, tx: &Sender<Sample>, stop: &AtomicBool, start: Instant) -> Result<()> {
    while !stop.load(Ordering::Relaxed) {
        let t = start.elapsed().as_secs_f64();
        let noise = || (fastrand::f64() - 0.5) * 0.1;

        // Battery holds at 12.6 V, then sags past the 11.5 V limit after 15 s.
        let vbat = if t < 15.0 { 12.6 } else { 12.6 - (t - 15.0) * 0.12 };
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
