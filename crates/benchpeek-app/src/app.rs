//! The egui application: pumps samples out of the active source, stores
//! them, evaluates rules every frame, and renders the four panels
//! (controls / signal table / plot / diagnosis).

use std::collections::HashMap;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use benchpeek_core::{
    evaluate, Diagnosis, Health, Recorder, Rule, RuleSet, SignalStore, DEFAULT_CAPACITY,
};
use benchpeek_plugin::Decoder;
use eframe::egui::{self, Color32, RichText};
use egui_plot::{Legend, Line, Plot, PlotPoints};

use crate::source::{self, SourceHandle, SourceKind};

#[derive(Clone, Copy, PartialEq, Eq)]
enum DecoderKind {
    /// `ascii-kv`, compiled straight into the app.
    Builtin,
    /// A `benchpeek:decoder` component loaded from disk at start-time.
    WasmPlugin,
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

    // View state.
    window_secs: f64,
    visible: HashMap<String, bool>,
    paused: bool,
    status: String,
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
            window_secs: 30.0,
            visible: HashMap::new(),
            paused: false,
            status: "idle".to_string(),
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
        Health::Ok => Color32::from_rgb(120, 200, 120),
        Health::Warn => Color32::from_rgb(232, 190, 90),
        Health::Fault => Color32::from_rgb(232, 110, 110),
    }
}

impl BenchpeekApp {
    /// Build the decoder selected in the DECODER panel: either the
    /// compiled-in `ascii-kv`, or a `benchpeek:decoder` component loaded
    /// from `plugin_path`. Both implement the same `Decoder` trait, so the
    /// rest of the app (source, store, rules) can't tell them apart.
    fn make_decoder(&self) -> anyhow::Result<Box<dyn Decoder>> {
        match self.decoder_kind {
            DecoderKind::Builtin => Ok(Box::new(ascii_kv::AsciiKv::new())),
            DecoderKind::WasmPlugin => {
                Ok(Box::new(benchpeek_wasm_host::WasmDecoder::load(&self.plugin_path)?))
            }
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
        ui.heading("benchpeek");
        ui.label(RichText::new(&self.status).weak());
        ui.separator();

        ui.label(RichText::new("DECODER").strong());
        ui.radio_value(&mut self.decoder_kind, DecoderKind::Builtin, "Built-in (ascii-kv)");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.decoder_kind, DecoderKind::WasmPlugin, "WASM plugin");
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
        ui.horizontal(|ui| {
            ui.label("Window (s)");
            ui.add(egui::Slider::new(&mut self.window_secs, 5.0..=120.0));
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
                    let pts: PlotPoints = sig
                        .points
                        .iter()
                        .filter(|p| p[0] >= t_min)
                        .map(|p| [p[0], p[1]])
                        .collect();
                    plot_ui.line(Line::new(sig.meta.name.clone(), pts));
                }
            });
    }
}

impl eframe::App for BenchpeekApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_auto_mode();
        self.pump();
        self.diagnoses = evaluate(&self.rules, &self.store);

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
        egui::CentralPanel::default().show(ui, |ui| self.plot_ui(ui));

        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
}
