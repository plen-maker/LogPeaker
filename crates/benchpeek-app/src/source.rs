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
use nusb::transfer::{Bulk, BulkOrInterrupt, In, Interrupt, TransferError};
use nusb::MaybeFuture;
#[cfg(target_os = "linux")]
use socketcan::{CanSocket, EmbeddedFrame, Frame, ShouldRetry, Socket};

/// Which transfer type a raw USB endpoint uses - fixed by the device's own
/// descriptor, so the app has to be told which one to ask for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UsbEndpointKind {
    Bulk,
    Interrupt,
}

/// Identifies one USB device + endpoint to read from. Bundled into one
/// struct (rather than five same-typed parameters) so a transposed
/// argument at a call site is a type error, not a silent mismatch.
#[derive(Clone, Copy)]
struct UsbTarget {
    vendor_id: u16,
    product_id: u16,
    interface: u8,
    endpoint: u8,
    kind: UsbEndpointKind,
}

pub enum SourceKind {
    /// A fake board: three plausible signals with a battery fault injected
    /// after 15 s so the rule engine has something to catch.
    Simulated,
    Serial {
        port: String,
        baud: u32,
    },
    /// A SocketCAN interface (`can0`, `vcan0`, ...); frames are rendered as
    /// `candump`-style ASCII lines and run through the same [`Decoder`]
    /// every other source uses (see `can_frame_line`).
    Can {
        interface: String,
    },
    /// A raw (non-serial) USB device's bulk or interrupt IN endpoint - for
    /// a device that doesn't enumerate as USB-CDC (already covered by
    /// `Serial`), e.g. a vendor-specific debug/telemetry interface. Bytes
    /// go straight to the [`Decoder`] with no framing translation, same as
    /// serial - a raw USB transfer already *is* a chunk of bytes.
    Usb {
        vendor_id: u16,
        product_id: u16,
        interface: u8,
        endpoint: u8,
        kind: UsbEndpointKind,
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
            SourceKind::Can { interface } => {
                run_can(&interface, decoder.as_mut(), &tx, &stop_thread, start)
            }
            SourceKind::Usb {
                vendor_id,
                product_id,
                interface,
                endpoint,
                kind,
            } => run_usb(
                &UsbTarget {
                    vendor_id,
                    product_id,
                    interface,
                    endpoint,
                    kind,
                },
                decoder.as_mut(),
                &tx,
                &stop_thread,
                start,
            ),
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

/// Retries `session` with a fixed backoff on error until `stop` is set,
/// logging each failure with `label` - the shared shape behind every
/// source's "keep trying to (re)connect" behavior (serial/CAN/USB): a
/// disconnected board, an interface that isn't up yet, or a device that
/// hasn't been plugged in shouldn't kill the source thread, just make it
/// keep trying.
fn with_reconnect(label: &str, stop: &AtomicBool, mut session: impl FnMut() -> Result<()>) {
    while !stop.load(Ordering::Relaxed) {
        if let Err(e) = session() {
            eprintln!("benchpeek source: {label} error, reconnecting: {e:#}");
            thread::sleep(Duration::from_millis(300));
        }
    }
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
    with_reconnect(port, stop, || {
        serial_session(port, baud, &mut *decoder, tx, stop, start)
    });
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

/// A CAN interface can come up after benchpeek starts (`ip link set can0
/// up` run separately, or a USB-CAN adapter plugged in later), so a failed
/// open retries instead of giving up, same as the serial source.
fn run_can(
    interface: &str,
    decoder: &mut dyn Decoder,
    tx: &Sender<Sample>,
    stop: &AtomicBool,
    start: Instant,
) -> Result<()> {
    with_reconnect(&format!("can {interface}"), stop, || {
        can_session(interface, &mut *decoder, tx, stop, start)
    });
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn can_session(_i: &str, _d: &mut dyn Decoder, _tx: &Sender<Sample>, _stop: &AtomicBool, _start: Instant) -> Result<()> {
    anyhow::bail!("SocketCAN is Linux-only")
}

#[cfg(target_os = "linux")]
fn can_session(
    interface: &str,
    decoder: &mut dyn Decoder,
    tx: &Sender<Sample>,
    stop: &AtomicBool,
    start: Instant,
) -> Result<()> {
    let sock = CanSocket::open(interface).with_context(|| format!("opening {interface}"))?;
    sock.set_read_timeout(Duration::from_millis(200))?;
    while !stop.load(Ordering::Relaxed) {
        match sock.read_frame() {
            // Error/remote frames carry no signal data to forward.
            Ok(frame) if frame.is_error_frame() || frame.is_remote_frame() => {}
            Ok(frame) => {
                let t = start.elapsed().as_secs_f64();
                let line = can_frame_line(frame.raw_id(), frame.data());
                for s in decoder.decode(line.as_bytes(), t) {
                    let _ = tx.send(s);
                }
            }
            Err(e) if e.should_retry() => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Renders one CAN frame as a `candump`-style ASCII line (`<id>#<hex
/// data>`) so it flows through the same byte-stream [`Decoder`] every other
/// source uses, instead of adding a second, frame-shaped decoding path.
fn can_frame_line(id: u32, data: &[u8]) -> String {
    use std::fmt::Write;
    let mut line = format!("{id:X}#");
    for b in data {
        let _ = write!(line, "{b:02X}");
    }
    line.push('\n');
    line
}

/// A USB device can be unplugged/replugged at any point, so a failed open
/// or a stalled/disconnected endpoint retries instead of giving up, same
/// as the serial and CAN sources.
fn run_usb(
    target: &UsbTarget,
    decoder: &mut dyn Decoder,
    tx: &Sender<Sample>,
    stop: &AtomicBool,
    start: Instant,
) -> Result<()> {
    let label = format!("usb {:04x}:{:04x}", target.vendor_id, target.product_id);
    with_reconnect(&label, stop, || {
        usb_session(target, &mut *decoder, tx, stop, start)
    });
    Ok(())
}

fn usb_session(
    target: &UsbTarget,
    decoder: &mut dyn Decoder,
    tx: &Sender<Sample>,
    stop: &AtomicBool,
    start: Instant,
) -> Result<()> {
    let UsbTarget {
        vendor_id,
        product_id,
        interface: interface_num,
        endpoint,
        kind,
    } = *target;

    let info = nusb::list_devices()
        .wait()
        .context("listing USB devices")?
        .find(|d| d.vendor_id() == vendor_id && d.product_id() == product_id)
        .ok_or_else(|| anyhow!("no USB device {vendor_id:04x}:{product_id:04x}"))?;
    let device = info
        .open()
        .wait()
        .with_context(|| format!("opening USB device {vendor_id:04x}:{product_id:04x}"))?;
    let interface = device
        .claim_interface(interface_num)
        .wait()
        .with_context(|| format!("claiming USB interface {interface_num}"))?;

    match kind {
        UsbEndpointKind::Bulk => {
            let ep = interface
                .endpoint::<Bulk, In>(endpoint)
                .with_context(|| format!("opening bulk IN endpoint {endpoint:#04x}"))?;
            usb_read_loop(ep, decoder, tx, stop, start)
        }
        UsbEndpointKind::Interrupt => {
            let ep = interface
                .endpoint::<Interrupt, In>(endpoint)
                .with_context(|| format!("opening interrupt IN endpoint {endpoint:#04x}"))?;
            usb_read_loop(ep, decoder, tx, stop, start)
        }
    }
}

/// Shared read loop for bulk and interrupt IN endpoints - identical past
/// the type-level `EpType`, which just picks which transfer type the OS
/// submits.
fn usb_read_loop<EpType: BulkOrInterrupt>(
    mut ep: nusb::Endpoint<EpType, In>,
    decoder: &mut dyn Decoder,
    tx: &Sender<Sample>,
    stop: &AtomicBool,
    start: Instant,
) -> Result<()> {
    // A multiple of max_packet_size, as `submit` requires for IN transfers;
    // comfortably larger than one packet so a burst doesn't immediately
    // split across reads.
    let chunk_len = ep.max_packet_size().max(64) * 8;
    // Allocated once and handed back by every `Completion` (success or
    // cancelled-by-timeout alike), so a fast device streaming well under
    // the 200ms timeout doesn't churn a fresh heap allocation every pass.
    let mut buf = ep.allocate(chunk_len);
    while !stop.load(Ordering::Relaxed) {
        let completion = ep.transfer_blocking(buf, Duration::from_millis(200));
        match completion.status {
            Ok(()) => {
                let t = start.elapsed().as_secs_f64();
                for s in decoder.decode(&completion.buffer[..completion.actual_len], t) {
                    let _ = tx.send(s);
                }
            }
            // The blocking wait's own timeout, not a device error - loop
            // back around to recheck `stop`.
            Err(TransferError::Cancelled) => {}
            Err(e) => return Err(e.into()),
        }
        buf = completion.buffer;
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
static SSH_IDENTITY: Mutex<Option<String>> = Mutex::new(None);

/// Sets the private key file every SSH helper (log watch, stats poll, remote
/// commands) passes with `-i`; `None`/empty means "use ssh's own defaults".
pub fn set_ssh_identity(path: Option<String>) {
    *SSH_IDENTITY.lock().unwrap() = path.filter(|p| !p.trim().is_empty());
}

pub fn ssh_identity_args() -> Vec<String> {
    match SSH_IDENTITY.lock().unwrap().clone() {
        Some(p) => vec!["-i".into(), p, "-o".into(), "IdentitiesOnly=yes".into()],
        None => Vec::new(),
    }
}

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
        .args(ssh_identity_args())
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

    #[test]
    fn can_frame_line_is_candump_format() {
        assert_eq!(can_frame_line(0x301, &[1, 2, 0xAB]), "301#0102AB\n");
        assert_eq!(can_frame_line(0x18FEF100, &[]), "18FEF100#\n");
    }

    /// Decoder that turns every chunk into one `Sample` recording its
    /// length, so a test can tell real bytes came back over the wire
    /// without caring about their content.
    struct RecordChunkLen;
    impl Decoder for RecordChunkLen {
        fn name(&self) -> &str {
            "test"
        }
        fn signals(&self) -> Vec<benchpeek_core::SignalMeta> {
            Vec::new()
        }
        fn decode(&mut self, bytes: &[u8], t: f64) -> Vec<Sample> {
            vec![Sample::new("usb_chunk_len", bytes.len() as f64, t)]
        }
    }

    /// Exercises `usb_read_loop` - the same code the app uses - against a
    /// real USB device over real usbfs, not just parsing logic: sends
    /// ST-Link's well-known, read-only `STLINK_GET_VERSION` (0xF1) command
    /// on bulk OUT ep 0x01, then confirms `usb_read_loop` actually receives
    /// the device's response on bulk IN ep 0x81 and runs it through a
    /// `Decoder`. Ignored by default since it needs an ST-Link (V2 or V3)
    /// attached; this repo's dev sandbox has a simulated one at 0483:3753.
    #[test]
    #[ignore = "requires an ST-Link at USB 0483:3753 (VID:PID) - run explicitly with `cargo test -p benchpeek-app -- --ignored usb_read_loop_receives_a_real_device_response`"]
    fn usb_read_loop_receives_a_real_device_response() {
        use nusb::transfer::Out;

        let info = nusb::list_devices()
            .wait()
            .expect("list USB devices")
            .find(|d| d.vendor_id() == 0x0483 && d.product_id() == 0x3753)
            .expect("no ST-Link (0483:3753) attached");
        let device = info.open().wait().expect("open ST-Link");
        let interface = device
            .claim_interface(0)
            .wait()
            .expect("claim ST-Link interface 0");

        let mut cmd_ep = interface
            .endpoint::<Bulk, Out>(0x01)
            .expect("bulk OUT ep 0x01");
        // STLINK_GET_VERSION, padded to the command packet size every
        // ST-Link host tool (OpenOCD, STM32CubeProgrammer, ...) uses.
        let mut cmd = vec![0u8; 16];
        cmd[0] = 0xF1;
        let sent = cmd_ep.transfer_blocking(cmd.into(), Duration::from_secs(1));
        sent.status.expect("writing GET_VERSION command");

        let in_ep = interface
            .endpoint::<Bulk, In>(0x81)
            .expect("bulk IN ep 0x81");

        let (tx, rx) = std::sync::mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let start = Instant::now();
        let handle = thread::spawn(move || {
            let mut decoder = RecordChunkLen;
            usb_read_loop(in_ep, &mut decoder, &tx, &stop_thread, start)
        });

        let sample = rx
            .recv_timeout(Duration::from_secs(3))
            .expect("no response read back from ST-Link's bulk IN endpoint");
        stop.store(true, Ordering::Relaxed);
        handle
            .join()
            .unwrap()
            .expect("usb_read_loop returned an error");

        assert_eq!(sample.signal, "usb_chunk_len");
        assert!(
            sample.value > 0.0,
            "GET_VERSION response should be a non-empty packet"
        );
    }

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
