//! The egui application: pumps samples out of the active source, stores
//! them, evaluates rules every frame, and renders the four panels
//! (controls / signal table / plot / diagnosis).

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use benchpeek_core::{
    evaluate, Diagnosis, Health, LogLevel, LogStore, Recorder, Rule, RuleSet, SignalStore,
    DEFAULT_CAPACITY,
};
use benchpeek_plugin::Decoder;
use eframe::egui::{self, Color32, RichText};
use egui_plot::{Legend, Line, LineStyle, Plot, PlotPoints};

use crate::source::{self, LogWatcher, SourceHandle, SourceKind};
use crate::theme;
use crate::yocto::{self, DeviceStats, StatsWatcher};

#[derive(Clone, Copy, PartialEq, Eq)]
enum DecoderKind {
    /// `ascii-kv`, compiled straight into the app.
    Builtin,
    /// A `benchpeek:decoder` component loaded from disk at start-time.
    WasmPlugin,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CentralView {
    Plot,
    Logs,
    History,
}

/// Top-level app mode: live board diagnostics vs. the Yocto build
/// dashboard. Different enough concerns (and different enough visual
/// density) that they get separate layouts rather than sharing panels.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AppMode {
    Diagnostics,
    Yocto,
}

pub struct BenchpeekApp {
    store: SignalStore,
    rules: RuleSet,
    diagnoses: Vec<Diagnosis>,
    source: Option<SourceHandle>,

    // Decoder config.
    decoder_kind: DecoderKind,
    plugin_path: String,

    // Source config.
    port: String,
    baud: u32,
    replay_path: String,
    ports: Vec<String>,

    // Auto mode: watch for a serial port appearing and connect to it.
    auto_mode: bool,
    auto_last_scan: Instant,
    active_port: Option<String>,

    // Recording.
    record_path: String,
    recorder: Option<Recorder>,

    // Log watch: independent of the signal source above.
    log_store: LogStore,
    log_watch: Option<LogWatcher>,
    log_host: String,
    log_user: String,
    log_command: String,

    // Diagnosis history: logs health transitions (not the live snapshot,
    // which `diagnoses` already covers).
    diag_history: VecDeque<DiagEvent>,
    prev_health: HashMap<String, Health>,
    app_start: Instant,

    // View state.
    window_secs: f64,
    visible: HashMap<String, bool>,
    paused: bool,
    status: String,
    central_view: CentralView,
    mode: AppMode,
    welcome: crate::onboarding::Welcome,
    files: crate::files::FilesPanel,
    yocto_files: bool,
    welcome_ports: Vec<String>,
    welcome_scan: Instant,

    // Yocto dashboard: build server connection + last-triggered build.
    yocto_host: String,
    yocto_user: String,
    yocto_container: String,
    yocto_workdir: String,
    yocto_init_cmd: String,
    yocto_target: String,
    yocto_log_path: String,
    stats_watch: Option<StatsWatcher>,
    stats: DeviceStats,
    build_log: Option<LogWatcher>,
    build_log_store: LogStore,
}

impl Default for BenchpeekApp {
    fn default() -> Self {
        Self {
            store: SignalStore::new(DEFAULT_CAPACITY),
            rules: default_rules(),
            diagnoses: Vec::new(),
            source: None,
            decoder_kind: DecoderKind::Builtin,
            plugin_path: "target/plugins/ascii-kv.wasm".to_string(),
            port: String::new(),
            baud: 115_200,
            replay_path: "session.jsonl".to_string(),
            ports: list_ports(),
            auto_mode: false,
            auto_last_scan: Instant::now(),
            active_port: None,
            record_path: "session.jsonl".to_string(),
            recorder: None,
            log_store: LogStore::new(4096),
            log_watch: None,
            log_host: String::new(),
            log_user: "root".to_string(),
            log_command: "journalctl -f -o cat".to_string(),
            diag_history: VecDeque::new(),
            prev_health: HashMap::new(),
            app_start: Instant::now(),
            window_secs: 30.0,
            visible: HashMap::new(),
            paused: false,
            status: "idle".to_string(),
            central_view: CentralView::Plot,
            mode: AppMode::Diagnostics,
            welcome: Default::default(),
            files: Default::default(),
            yocto_files: false,
            welcome_ports: Vec::new(),
            welcome_scan: Instant::now() - Duration::from_secs(2),
            yocto_host: String::new(),
            yocto_user: "yocto".to_string(),
            yocto_container: "stm32mp2-builder".to_string(),
            yocto_workdir: "/yocto-st/projects/stm32mp2-62".to_string(),
            yocto_init_cmd: "source layers/openembedded-core/oe-init-build-env build-62"
                .to_string(),
            yocto_target: "st-image-weston".to_string(),
            yocto_log_path: "/tmp/bitbake-build.log".to_string(),
            stats_watch: None,
            stats: DeviceStats::default(),
            build_log: None,
            build_log_store: LogStore::new(4096),
        }
    }
}

/// Starter rules for the simulated board; also a worked example of the format.
fn default_rules() -> RuleSet {
    RuleSet {
        rules: vec![
            Rule {
                signal: "VBAT".into(),
                min: Some(11.5),
                max: Some(15.0),
                message: "Battery voltage out of range - check charging system / battery health"
                    .into(),
                severity: "fault".into(),
            },
            Rule {
                signal: "TEMP".into(),
                min: None,
                max: Some(85.0),
                message: "Temperature high - check cooling / airflow".into(),
                severity: "warn".into(),
            },
            Rule {
                signal: "RPM".into(),
                min: Some(150.0),
                max: Some(6000.0),
                message: "RPM implausible - sensor or wiring fault".into(),
                severity: "warn".into(),
            },
        ],
    }
}

fn list_ports() -> Vec<String> {
    serialport::available_ports()
        .map(|ps| ps.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default()
}

/// USB serial devices only (excludes platform ports like `/dev/ttyS0`,
/// which exist on almost every PC whether or not anything real is wired to
/// them and would otherwise "win" auto mode's port pick since it's
/// arbitrary OS enumeration order, not alphabetical).
fn list_usb_ports() -> Vec<String> {
    serialport::available_ports()
        .map(|ps| {
            ps.into_iter()
                .filter(|p| matches!(p.port_type, serialport::SerialPortType::UsbPort(_)))
                .map(|p| p.port_name)
                .collect()
        })
        .unwrap_or_default()
}

fn health_color(h: Health) -> Color32 {
    match h {
        Health::Ok => theme::OK,
        Health::Warn => theme::WARN,
        Health::Fault => theme::FAULT,
    }
}

/// Color for a rule's own threshold guide line, based on its configured
/// severity rather than any live value.
fn rule_color(rule: &Rule) -> Color32 {
    if rule.severity == "warn" {
        theme::WARN
    } else {
        theme::FAULT
    }
}

/// A labeled percentage bar for the Yocto dashboard's device-status card.
/// Plain progress bars instead of circular gauges on purpose - the
/// reference mockup this was modeled on was criticized as too busy;
/// egui's built-in bar reads just as clearly with far less custom painting.
fn stat_bar(ui: &mut egui::Ui, label: &str, pct: Option<f32>) {
    ui.horizontal(|ui| {
        ui.add_sized([50.0, 0.0], egui::Label::new(label));
        match pct {
            Some(p) => {
                let color = if p >= 90.0 {
                    theme::FAULT
                } else if p >= 75.0 {
                    theme::WARN
                } else {
                    theme::ACCENT
                };
                ui.add(
                    egui::ProgressBar::new((p / 100.0).clamp(0.0, 1.0))
                        .text(format!("{p:.0}%"))
                        .fill(color),
                );
            }
            None => {
                ui.label(RichText::new("-").color(theme::WEAK_TEXT));
            }
        }
    });
}

fn health_rank(h: Health) -> u8 {
    match h {
        Health::Ok => 0,
        Health::Warn => 1,
        Health::Fault => 2,
    }
}

/// One health transition for a signal, kept for the History tab.
struct DiagEvent {
    t: f64,
    signal: String,
    health: Health,
    message: String,
}

impl BenchpeekApp {
    /// Build the decoder selected in the DECODER panel: either the
    /// compiled-in `ascii-kv`, or a `benchpeek:decoder` component loaded
    /// from `plugin_path`. Both implement the same `Decoder` trait, so the
    /// rest of the app (source, store, rules) can't tell them apart.
    fn make_decoder(&self) -> anyhow::Result<Box<dyn Decoder>> {
        match self.decoder_kind {
            DecoderKind::Builtin => Ok(Box::new(ascii_kv::AsciiKv::new())),
            DecoderKind::WasmPlugin => Ok(Box::new(benchpeek_wasm_host::WasmDecoder::load(
                &self.plugin_path,
            )?)),
        }
    }

    fn start(&mut self, kind: SourceKind) {
        self.stop();
        let decoder = match self.make_decoder() {
            Ok(d) => d,
            Err(e) => {
                self.status = format!("decoder error: {e:#}");
                return;
            }
        };
        let decoder_label = decoder.name().to_string();
        self.active_port = match &kind {
            SourceKind::Serial { port, .. } => Some(port.clone()),
            _ => None,
        };
        let source_label = match &kind {
            SourceKind::Simulated => "simulated board".to_string(),
            SourceKind::Serial { port, baud } => format!("serial {port} @ {baud}"),
            SourceKind::Replay { path } => format!("replay {path}"),
        };
        self.source = Some(source::spawn(kind, decoder));
        self.status = format!("running: {source_label} [{decoder_label}]");
        self.paused = false;
    }

    fn stop(&mut self) {
        if let Some(mut s) = self.source.take() {
            s.stop();
        }
        self.active_port = None;
        if !self.status.starts_with("running") {
            return;
        }
        self.status = "stopped".to_string();
    }

    /// When `auto_mode` is on: connect to the first USB serial device seen,
    /// and go back to watching if it disappears (board unplugged). Only
    /// considers real USB devices - `/dev/ttyS0`-style platform ports exist
    /// on almost every PC regardless of hardware and would otherwise win
    /// on arbitrary OS enumeration order. Polls at most a few times a
    /// second - hotplug scanning is cheap but no need to do it every frame.
    fn poll_auto_mode(&mut self) {
        if !self.auto_mode {
            return;
        }
        if self.auto_last_scan.elapsed() < Duration::from_millis(750) {
            return;
        }
        self.auto_last_scan = Instant::now();

        let ports = list_usb_ports();

        if let Some(active) = &self.active_port {
            if !ports.contains(active) {
                self.stop();
                self.status = "auto: device unplugged, watching...".to_string();
            }
        }

        if self.source.is_none() {
            match ports.first() {
                Some(port) => {
                    self.port = port.clone();
                    self.start(SourceKind::Serial {
                        port: port.clone(),
                        baud: self.baud,
                    });
                }
                None => self.status = "auto: watching for a USB serial device...".to_string(),
            }
        }
    }

    /// Drain the source channel into the store (and the recorder, if active).
    fn pump(&mut self) {
        if self.paused || self.source.is_none() {
            return;
        }
        let mut batch = Vec::new();
        let mut disconnected = false;
        {
            let src = self.source.as_ref().unwrap();
            loop {
                match src.rx.try_recv() {
                    Ok(s) => {
                        batch.push(s);
                        if batch.len() >= 20_000 {
                            break;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        for sample in batch {
            if let Some(rec) = &mut self.recorder {
                let _ = rec.write(&sample);
            }
            self.visible.entry(sample.signal.clone()).or_insert(true);
            self.store.ingest(&sample);
        }
        if disconnected {
            self.status = "source finished".to_string();
        }
    }

    fn start_log_watch(&mut self) {
        self.stop_log_watch();
        self.log_watch = Some(source::spawn_log_watch(
            self.log_host.clone(),
            self.log_user.clone(),
            self.log_command.clone(),
        ));
    }

    fn stop_log_watch(&mut self) {
        if let Some(mut w) = self.log_watch.take() {
            w.stop();
        }
    }

    /// Drain the log-watch channel into the log store, independently of
    /// `pump()` - a log watch can be running with or without a signal
    /// source active.
    fn pump_logs(&mut self) {
        let Some(watch) = &self.log_watch else { return };
        loop {
            match watch.rx.try_recv() {
                Ok(e) => self.log_store.push(e),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    /// Record health transitions (new fault/warn, escalation, or clearing)
    /// into `diag_history`. `diagnoses` only ever holds *current* violations
    /// (see `evaluate`), so anything missing from it this frame that was
    /// present last frame has cleared.
    fn track_diag_history(&mut self) {
        let now = self.app_start.elapsed().as_secs_f64();

        let mut current: HashMap<&str, (Health, &str)> = HashMap::new();
        for d in &self.diagnoses {
            let entry = current
                .entry(d.signal.as_str())
                .or_insert((d.health, d.message.as_str()));
            if health_rank(d.health) > health_rank(entry.0) {
                *entry = (d.health, d.message.as_str());
            }
        }

        let mut events = Vec::new();
        for (&signal, &(health, message)) in &current {
            let prev = self.prev_health.get(signal).copied().unwrap_or(Health::Ok);
            if health != prev {
                events.push(DiagEvent {
                    t: now,
                    signal: signal.to_string(),
                    health,
                    message: message.to_string(),
                });
            }
        }
        let cleared: Vec<String> = self
            .prev_health
            .keys()
            .filter(|s| !current.contains_key(s.as_str()))
            .cloned()
            .collect();
        for signal in &cleared {
            events.push(DiagEvent {
                t: now,
                signal: signal.clone(),
                health: Health::Ok,
                message: "back within range".to_string(),
            });
        }

        for signal in cleared {
            self.prev_health.remove(&signal);
        }
        for (&signal, &(health, _)) in &current {
            self.prev_health.insert(signal.to_string(), health);
        }

        for e in events {
            if self.diag_history.len() >= 500 {
                self.diag_history.pop_front();
            }
            self.diag_history.push_back(e);
        }
    }

    fn start_yocto_stats(&mut self) {
        self.stop_yocto_stats();
        if self.yocto_host.is_empty() {
            return;
        }
        self.stats_watch = Some(yocto::spawn_stats_poll(
            self.yocto_host.clone(),
            self.yocto_user.clone(),
            Duration::from_secs(5),
        ));
    }

    fn stop_yocto_stats(&mut self) {
        if let Some(mut w) = self.stats_watch.take() {
            w.stop();
        }
        self.stats = DeviceStats::default();
    }

    fn pump_yocto_stats(&mut self) {
        let Some(watch) = &self.stats_watch else {
            return;
        };
        // Only the latest snapshot matters - drain to the last one.
        while let Ok(s) = watch.rx.try_recv() {
            self.stats = s;
        }
    }

    /// The exact `docker exec -d ...` wrapper used manually against this
    /// project earlier - init the build env, run bitbake, log the exit
    /// code. Fire-and-forget: `start_build` doesn't wait for this to
    /// return, it just kicks it off, then starts tailing the log file.
    fn build_trigger_command(&self) -> String {
        format!(
            "docker exec -d -w {workdir} {container} bash -c \"{init} > /dev/null 2>&1; bitbake {target} > {log} 2>&1; echo __EXIT=\\$? >> {log}\"",
            workdir = self.yocto_workdir,
            container = self.yocto_container,
            init = self.yocto_init_cmd,
            target = self.yocto_target,
            log = self.yocto_log_path,
        )
    }

    fn start_build(&mut self) {
        if self.yocto_host.is_empty() {
            return;
        }
        yocto::run_remote_command(
            self.yocto_host.clone(),
            self.yocto_user.clone(),
            self.build_trigger_command(),
        );
        self.start_build_log_watch();
    }

    fn start_build_log_watch(&mut self) {
        self.stop_build_log_watch();
        let tail_cmd = format!(
            "docker exec {} tail -F {}",
            self.yocto_container, self.yocto_log_path
        );
        self.build_log = Some(source::spawn_log_watch(
            self.yocto_host.clone(),
            self.yocto_user.clone(),
            tail_cmd,
        ));
    }

    fn stop_build_log_watch(&mut self) {
        if let Some(mut w) = self.build_log.take() {
            w.stop();
        }
    }

    fn pump_build_log(&mut self) {
        let Some(watch) = &self.build_log else { return };
        loop {
            match watch.rx.try_recv() {
                Ok(e) => self.build_log_store.push(e),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    /// Dot color for the top bar: reflects connection state, not signal
    /// health - a live serial link with every value in range is still
    /// worth showing as "connected", distinct from "idle".
    fn connection_color(&self) -> Color32 {
        if self.status.starts_with("running") {
            theme::ACCENT
        } else if self.status.starts_with("decoder error") || self.status.contains("error") {
            theme::FAULT
        } else if self.status.starts_with("auto:") {
            theme::WARN
        } else {
            theme::IDLE
        }
    }

    fn topbar_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(dot_rect.center(), 4.5, self.connection_color());
            ui.add_space(4.0);
            ui.label(RichText::new("benchpeek").strong().size(15.0));
            ui.separator();
            ui.selectable_value(&mut self.mode, AppMode::Diagnostics, "Diagnostics");
            ui.selectable_value(&mut self.mode, AppMode::Yocto, "Yocto");
            if ui.button("Connection guide").clicked() {
                self.welcome.replay();
            }
            ui.separator();
            ui.label(RichText::new(&self.status).color(theme::WEAK_TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (diag_faults, diag_warns) =
                    self.diagnoses
                        .iter()
                        .fold((0, 0), |(f, w), d| match d.health {
                            Health::Fault => (f + 1, w),
                            Health::Warn => (f, w + 1),
                            Health::Ok => (f, w),
                        });
                let (log_faults, log_warns) = self.log_store.counts();
                let faults = diag_faults + log_faults;
                let warns = diag_warns + log_warns;
                if faults > 0 {
                    ui.label(
                        RichText::new(format!("{faults} FAULT"))
                            .color(theme::FAULT)
                            .strong(),
                    );
                } else if warns > 0 {
                    ui.label(
                        RichText::new(format!("{warns} WARN"))
                            .color(theme::WARN)
                            .strong(),
                    );
                } else {
                    ui.label(RichText::new("all clear").color(theme::OK));
                }
            });
        });
    }

    fn health_of(&self, signal: &str) -> Health {
        let mut h = Health::Ok;
        for d in &self.diagnoses {
            if d.signal == signal {
                match d.health {
                    Health::Fault => return Health::Fault,
                    Health::Warn => h = Health::Warn,
                    Health::Ok => {}
                }
            }
        }
        h
    }

    fn controls_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("DECODER").strong());
        ui.radio_value(
            &mut self.decoder_kind,
            DecoderKind::Builtin,
            "Built-in (ascii-kv)",
        );
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut self.decoder_kind,
                DecoderKind::WasmPlugin,
                "WASM plugin",
            );
            ui.text_edit_singleline(&mut self.plugin_path);
        });
        ui.label(
            RichText::new("Takes effect on the next Start/Open/Replay below.")
                .weak()
                .small(),
        );

        ui.separator();
        ui.label(RichText::new("SOURCE").strong());
        if ui.checkbox(&mut self.auto_mode, "Auto mode").changed() && self.auto_mode {
            self.auto_last_scan = Instant::now() - Duration::from_secs(1);
        }
        ui.label(
            RichText::new("Connects to the first serial port it sees, reconnects on unplug.")
                .weak()
                .small(),
        );
        if ui.button("Start simulated board").clicked() {
            self.start(SourceKind::Simulated);
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Port");
            let ports = self.ports.clone();
            egui::ComboBox::from_id_salt("port")
                .selected_text(if self.port.is_empty() {
                    "-".to_string()
                } else {
                    self.port.clone()
                })
                .show_ui(ui, |ui| {
                    for p in ports {
                        ui.selectable_value(&mut self.port, p.clone(), p);
                    }
                });
            if ui.small_button("scan").clicked() {
                self.ports = list_ports();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Baud");
            ui.add(egui::DragValue::new(&mut self.baud).range(300..=4_000_000));
        });
        if ui.button("Open serial").clicked() && !self.port.is_empty() {
            self.start(SourceKind::Serial {
                port: self.port.clone(),
                baud: self.baud,
            });
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("File");
            ui.text_edit_singleline(&mut self.replay_path);
        });
        if ui.button("Replay file").clicked() {
            self.start(SourceKind::Replay {
                path: self.replay_path.clone(),
            });
        }

        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button(if self.paused { "Resume" } else { "Pause" })
                .clicked()
            {
                self.paused = !self.paused;
            }
            if ui.button("Stop").clicked() {
                self.stop();
            }
            if ui.button("Clear").clicked() {
                self.store = SignalStore::new(DEFAULT_CAPACITY);
                self.visible.clear();
            }
        });

        ui.separator();
        ui.label(RichText::new("RECORDING").strong());
        ui.text_edit_singleline(&mut self.record_path);
        let recording = self.recorder.is_some();
        if ui
            .button(if recording {
                "Stop recording"
            } else {
                "Start recording"
            })
            .clicked()
        {
            if recording {
                if let Some(mut r) = self.recorder.take() {
                    let _ = r.flush();
                }
            } else {
                match Recorder::create(&self.record_path) {
                    Ok(r) => self.recorder = Some(r),
                    Err(e) => self.status = format!("record error: {e}"),
                }
            }
        }

        ui.separator();
        ui.label(RichText::new("LOG WATCH").strong());
        ui.label(
            RichText::new("Tails a remote board's log over SSH (key-based auth). Independent of the SOURCE above - can run at the same time.")
                .weak()
                .small(),
        );
        ui.horizontal(|ui| {
            ui.label("Host");
            ui.text_edit_singleline(&mut self.log_host);
        });
        ui.horizontal(|ui| {
            ui.label("User");
            ui.text_edit_singleline(&mut self.log_user);
        });
        ui.horizontal(|ui| {
            ui.label("Command");
            ui.text_edit_singleline(&mut self.log_command);
        });
        let watching = self.log_watch.is_some();
        ui.horizontal(|ui| {
            if ui
                .button(if watching {
                    "Stop watch"
                } else {
                    "Start watch"
                })
                .clicked()
            {
                if watching {
                    self.stop_log_watch();
                } else if !self.log_host.is_empty() {
                    self.start_log_watch();
                }
            }
            if ui.button("Clear log").clicked() {
                self.log_store = LogStore::new(4096);
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Window (s)");
            ui.add(egui::Slider::new(&mut self.window_secs, 5.0..=120.0));
        });
    }

    fn logs_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Logs");
            let status = if self.log_watch.is_some() {
                format!("watching {}@{}", self.log_user, self.log_host)
            } else {
                "not watching".to_string()
            };
            ui.label(RichText::new(status).color(theme::WEAK_TEXT));
        });
        ui.separator();
        if self.log_store.events.is_empty() {
            ui.label(
                RichText::new("No log lines yet. Set a host in LOG WATCH and Start watch.").weak(),
            );
            return;
        }
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for e in &self.log_store.events {
                    let color = match e.level {
                        LogLevel::Fault => theme::FAULT,
                        LogLevel::Warn => theme::WARN,
                        LogLevel::Info => theme::WEAK_TEXT,
                    };
                    ui.label(
                        RichText::new(format!("[{:>8.3}] {}", e.t, e.message))
                            .monospace()
                            .color(color),
                    );
                }
            });
    }

    fn history_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Diagnosis history");
        ui.separator();
        if self.diag_history.is_empty() {
            ui.label(RichText::new("No fault/warning transitions yet.").weak());
            return;
        }
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for e in &self.diag_history {
                    let color = health_color(e.health);
                    let tag = match e.health {
                        Health::Fault => "FAULT",
                        Health::Warn => "WARN",
                        Health::Ok => "CLEARED",
                    };
                    ui.label(
                        RichText::new(format!(
                            "[{:>8.3}] {tag:<7} {} - {}",
                            e.t, e.signal, e.message
                        ))
                        .monospace()
                        .color(color),
                    );
                }
            });
    }

    fn yocto_ui(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("yocto_controls")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| {
                ui.label(RichText::new("BUILD SERVER").strong());
                ui.horizontal(|ui| {
                    ui.label("Host");
                    ui.text_edit_singleline(&mut self.yocto_host);
                });
                ui.horizontal(|ui| {
                    ui.label("User");
                    ui.text_edit_singleline(&mut self.yocto_user);
                });
                let watching = self.stats_watch.is_some();
                ui.horizontal(|ui| {
                    if ui
                        .button(if watching { "Disconnect" } else { "Connect" })
                        .clicked()
                    {
                        if watching {
                            self.stop_yocto_stats();
                        } else {
                            self.start_yocto_stats();
                        }
                    }
                });

                ui.separator();
                ui.label(RichText::new("DEVICE STATUS").strong());
                let dot = if self.stats.reachable {
                    theme::ACCENT
                } else {
                    theme::IDLE
                };
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, dot);
                    ui.label(if self.stats.reachable {
                        "Connected"
                    } else {
                        "Not connected"
                    });
                });
                stat_bar(ui, "CPU", self.stats.cpu_load_pct);
                stat_bar(ui, "RAM", self.stats.mem_used_pct);
                stat_bar(ui, "Disk /", self.stats.disk_used_pct);

                ui.separator();
                ui.label(RichText::new("YOCTO PROJECT").strong());
                ui.horizontal(|ui| {
                    ui.label("Container");
                    ui.text_edit_singleline(&mut self.yocto_container);
                });
                ui.horizontal(|ui| {
                    ui.label("Workdir");
                    ui.text_edit_singleline(&mut self.yocto_workdir);
                });
                ui.horizontal(|ui| {
                    ui.label("Init");
                    ui.text_edit_singleline(&mut self.yocto_init_cmd);
                });
                ui.horizontal(|ui| {
                    ui.label("Target");
                    ui.text_edit_singleline(&mut self.yocto_target);
                });
                ui.horizontal(|ui| {
                    ui.label("Log");
                    ui.text_edit_singleline(&mut self.yocto_log_path);
                });

                ui.add_space(6.0);
                let building = self.build_log.is_some();
                ui.horizontal(|ui| {
                    if ui.button("Start build").clicked() && !self.yocto_host.is_empty() {
                        self.start_build();
                    }
                    if building && ui.button("Stop watching log").clicked() {
                        self.stop_build_log_watch();
                    }
                    if ui.button("Clear log").clicked() {
                        self.build_log_store = LogStore::new(4096);
                    }
                });
                ui.label(
                    RichText::new("Start build kicks the job off detached, then tails its log - it doesn't wait for it to finish.")
                        .weak()
                        .small(),
                );
            });

        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.yocto_files, false, "Build Log");
            ui.selectable_value(&mut self.yocto_files, true, "Files & Transfer");
        });
        ui.separator();
        if self.yocto_files {
            self.files.ui(ui, &self.yocto_host, &self.yocto_user);
            return;
        }
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading("Build Log");
                let (faults, warns) = self.build_log_store.counts();
                if faults > 0 {
                    ui.label(
                        RichText::new(format!("{faults} error(s)"))
                            .color(theme::FAULT)
                            .strong(),
                    );
                } else if warns > 0 {
                    ui.label(
                        RichText::new(format!("{warns} warning(s)"))
                            .color(theme::WARN)
                            .strong(),
                    );
                }
            });
            ui.separator();
            if self.build_log_store.events.is_empty() {
                ui.label(RichText::new("No build output yet. Set Host, then Start build.").weak());
                return;
            }
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for e in &self.build_log_store.events {
                        let color = match e.level {
                            LogLevel::Fault => theme::FAULT,
                            LogLevel::Warn => theme::WARN,
                            LogLevel::Info => theme::WEAK_TEXT,
                        };
                        ui.label(RichText::new(&e.message).monospace().small().color(color));
                    }
                });
        });
    }

    fn signal_table_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Signals");
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let names: Vec<String> = self.store.order.clone();
            for name in names {
                let Some((val, unit)) = self.store.signals.get(&name).map(|s| {
                    (
                        s.last().unwrap_or(f64::NAN),
                        s.meta.unit.clone().unwrap_or_default(),
                    )
                }) else {
                    continue;
                };
                let color = health_color(self.health_of(&name));
                ui.horizontal(|ui| {
                    let vis = self.visible.entry(name.clone()).or_insert(true);
                    ui.checkbox(vis, "");
                    ui.colored_label(color, &name);
                });
                ui.label(
                    RichText::new(format!("    {val:.3} {unit}"))
                        .monospace()
                        .color(color),
                );
                ui.add_space(2.0);
            }
        });
    }

    fn diagnosis_ui(&mut self, ui: &mut egui::Ui) {
        let (faults, warns) = self
            .diagnoses
            .iter()
            .fold((0, 0), |(f, w), d| match d.health {
                Health::Fault => (f + 1, w),
                Health::Warn => (f, w + 1),
                Health::Ok => (f, w),
            });
        ui.horizontal(|ui| {
            ui.heading("Diagnosis");
            ui.label(RichText::new(format!("{faults} fault(s), {warns} warning(s)")).strong());
        });
        ui.separator();
        if self.diagnoses.is_empty() {
            ui.label(
                RichText::new("All monitored signals within range.")
                    .color(health_color(Health::Ok)),
            );
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for d in &self.diagnoses {
                let color = health_color(d.health);
                let tag = match d.health {
                    Health::Fault => "FAULT",
                    Health::Warn => "WARN",
                    Health::Ok => "OK",
                };
                ui.label(
                    RichText::new(format!("[{tag}] {} = {:.3}", d.signal, d.value))
                        .color(color)
                        .strong(),
                );
                ui.label(format!("       {}", d.message));
                ui.add_space(4.0);
            }
        });
    }

    fn plot_ui(&self, ui: &mut egui::Ui) {
        let now = self
            .store
            .iter()
            .filter_map(|s| s.last_t())
            .fold(0.0_f64, f64::max);
        let t_min = now - self.window_secs;
        Plot::new("signals_plot")
            .legend(Legend::default())
            .show(ui, |plot_ui| {
                for sig in self.store.iter() {
                    if !self.visible.get(&sig.meta.name).copied().unwrap_or(true) {
                        continue;
                    }
                    let color = health_color(self.health_of(&sig.meta.name));

                    // Rule thresholds as faint dashed guides, so the healthy
                    // range is visible on the plot itself, not just implied
                    // by the diagnosis panel.
                    if let Some(rule) = self.rules.rules.iter().find(|r| r.signal == sig.meta.name)
                    {
                        let guide_color = rule_color(rule).linear_multiply(0.55);
                        if let Some(min) = rule.min {
                            plot_ui.hline(
                                egui_plot::HLine::new(format!("{} min", sig.meta.name), min)
                                    .color(guide_color)
                                    .style(LineStyle::dashed_loose()),
                            );
                        }
                        if let Some(max) = rule.max {
                            plot_ui.hline(
                                egui_plot::HLine::new(format!("{} max", sig.meta.name), max)
                                    .color(guide_color)
                                    .style(LineStyle::dashed_loose()),
                            );
                        }
                    }

                    let pts: PlotPoints = sig
                        .points
                        .iter()
                        .filter(|p| p[0] >= t_min)
                        .map(|p| [p[0], p[1]])
                        .collect();
                    plot_ui.line(Line::new(sig.meta.name.clone(), pts).color(color));
                }
            });
    }
}

impl eframe::App for BenchpeekApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_auto_mode();
        self.pump();
        self.pump_logs();
        self.pump_yocto_stats();
        self.pump_build_log();
        self.diagnoses = evaluate(&self.rules, &self.store);
        self.track_diag_history();

        if self.welcome.visible {
            if self.welcome_scan.elapsed() >= Duration::from_secs(1) {
                self.welcome_ports = list_usb_ports();
                self.welcome_scan = Instant::now();
            }
            egui::CentralPanel::default().show(ui, |ui| {
                if self.welcome.ui(ui, &self.welcome_ports) {
                    self.auto_mode = true;
                    self.auto_last_scan = Instant::now() - Duration::from_secs(2);
                }
            });
            ui.ctx().request_repaint_after(Duration::from_millis(33));
            return;
        }

        egui::Panel::top("topbar")
            .resizable(false)
            .default_size(30.0)
            .show(ui, |ui| self.topbar_ui(ui));

        match self.mode {
            AppMode::Diagnostics => {
                egui::Panel::left("controls")
                    .resizable(true)
                    .default_size(290.0)
                    .show(ui, |ui| self.controls_ui(ui));
                egui::Panel::right("signal_table")
                    .resizable(true)
                    .default_size(240.0)
                    .show(ui, |ui| self.signal_table_ui(ui));
                egui::Panel::bottom("diagnosis")
                    .resizable(true)
                    .default_size(170.0)
                    .show(ui, |ui| self.diagnosis_ui(ui));
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.central_view, CentralView::Plot, "Plot");
                        ui.selectable_value(&mut self.central_view, CentralView::Logs, "Logs");
                        ui.selectable_value(
                            &mut self.central_view,
                            CentralView::History,
                            "History",
                        );
                    });
                    ui.separator();
                    match self.central_view {
                        CentralView::Plot => self.plot_ui(ui),
                        CentralView::Logs => self.logs_ui(ui),
                        CentralView::History => self.history_ui(ui),
                    }
                });
            }
            AppMode::Yocto => {
                egui::CentralPanel::default().show(ui, |ui| self.yocto_ui(ui));
            }
        }

        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
}
