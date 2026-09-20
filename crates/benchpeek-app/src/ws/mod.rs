//! Monochrome "Workspace" UI: a single working shell (sidebar, status bar,
//! pages, overlays) laid out on a 1280x800 design grid. All device data comes
//! from `demo` and is labelled as such.

/// Issues of the current data source: none for a live SSH device (no fault
/// rules are wired to it yet), the scripted ones for the demo.
macro_rules! issues {
    ($s:expr) => {
        if $s.live.is_some() {
            &[][..]
        } else {
            &$s.demo.issues[..]
        }
    };
}

mod bg;
mod files;
mod live;
mod demo;
mod overlays;
mod pages;
mod theme;
mod widgets;

use std::collections::HashSet;

use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::icons as ic;
use demo::{Demo, LogLine, Node, Session};
use live::Live;
use theme::*;
use widgets::*;

pub const DESIGN_W: f32 = 1280.0;
pub const DESIGN_H: f32 = 800.0;
pub const SIDEBAR_W: f32 = 232.0;
pub const TOPBAR_H: f32 = 58.0;
pub const MARGIN: f32 = 44.0;
pub const DIAG_W: f32 = 330.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Home,
    Devices,
    Monitor,
    Diagnostics,
    Files,
    Sessions,
    Settings,
}

impl Page {
    pub const NAV: [Page; 6] = [
        Page::Home,
        Page::Devices,
        Page::Monitor,
        Page::Diagnostics,
        Page::Files,
        Page::Sessions,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Devices => "Devices",
            Page::Monitor => "Live monitor",
            Page::Diagnostics => "Diagnostics",
            Page::Files => "Files",
            Page::Sessions => "Sessions",
            Page::Settings => "Settings",
        }
    }
    pub fn icon(self) -> char {
        match self {
            Page::Home => ic::HOUSE,
            Page::Devices => ic::MICROCHIP,
            Page::Monitor => ic::ACTIVITY,
            Page::Diagnostics => ic::STETHOSCOPE,
            Page::Files => ic::FOLDER,
            Page::Sessions => ic::HISTORY,
            Page::Settings => ic::SETTINGS,
        }
    }
}

pub struct Toast {
    pub title: String,
    pub sub: String,
    pub view_session: bool,
    pub shown_at: f64,
}

pub struct Replay {
    pub name: String,
    pub lines: Vec<LogLine>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CcView {
    Main,
    Wifi,
    Bluetooth,
}

pub struct Workspace {
    pub page: Page,
    page_t0: f64,
    nav_y: Tween,

    pub demo: Demo,
    bg: Option<egui::TextureHandle>,

    // Live monitor
    pub paused: bool,
    pub capture_on: bool,
    pub selected_log: Option<u64>,
    pub scroll_to_log: Option<u64>,
    /// Set after a jump-to-line so the list stops following the tail until the user scrolls back down.
    pub pin_scroll: bool,
    pub replay: Option<Replay>,
    capture_started: f64,

    // Diagnostics
    pub diag_open: bool,
    diag_w: Tween,
    pub sel_issue: usize,
    pub recheck_at: Option<f64>,

    // Files
    pub fs: Node,
    pub cwd: Vec<String>,
    pub sel_file: Option<String>,
    pub search: String,
    pub grid_view: bool,
    pub sort_key: usize,
    pub sort_desc: bool,
    pub ctx_menu: Option<(Pos2, String)>,
    ctx_t: Tween,
    pub quick_look: Option<String>,
    ql_t: Tween,
    pub confirm_delete: Option<String>,
    conf_t: Tween,
    pub live: Option<Live>,
    pub connect_open: bool,
    connect_t: Tween,
    pub conn_host: String,
    pub conn_user: String,
    pub conn_key: String,
    pub conn_error: Option<String>,
    pub guide_open: bool,
    guide_t: Tween,
    guide_t0: f64,
    guide_tex: Option<egui::TextureHandle>,
    guide_frame: Option<usize>,
    pub refocus_files: bool,

    // Sessions
    pub sessions: Vec<Session>,

    // Overlays
    pub device_menu: bool,
    dev_t: Tween,
    pub cc_open: bool,
    cc_t: Tween,
    pub cc_view: CcView,
    pub wifi_on: bool,
    pub bt_on: bool,
    pub brightness: f32,
    pub volume: f32,
    pub toast: Option<Toast>,
    toast_t: Tween,

    // Misc
    pub reduce_motion: bool,
    pub touched: HashSet<&'static str>,
    shot: Shot,
}

struct Shot {
    dir: Option<std::path::PathBuf>,
    stage: usize,
    t0: Option<f64>,
    requested: usize,
    saved: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    design: Rect,
}

impl Default for Shot {
    fn default() -> Self {
        Self { dir: None, stage: 0, t0: None, requested: 0, saved: Default::default(), design: Rect::NOTHING }
    }
}

impl Workspace {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.override_text_color = Some(TEXT);
        visuals.selection.bg_fill = Color32::from_white_alpha(60);
        visuals.selection.stroke = Stroke::new(1.0, TEXT);
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        cc.egui_ctx.set_visuals_of(egui::Theme::Dark, visuals);

        let reduce = std::env::var_os("BENCHPEEK_REDUCE_MOTION").is_some();
        set_reduce_motion(reduce);
        let dir = std::env::var_os("BENCHPEEK_SHOT_DIR").map(std::path::PathBuf::from);
        let mut app = Self {
            page: Page::Home,
            page_t0: -10.0,
            nav_y: Tween::new(86.0),
            demo: Demo::new(),
            bg: Some(bg::make(&cc.egui_ctx)),
            paused: false,
            capture_on: true,
            selected_log: None,
            scroll_to_log: None,
            pin_scroll: false,
            replay: None,
            capture_started: 0.0,
            diag_open: false,
            diag_w: Tween::new(0.0),
            sel_issue: 0,
            recheck_at: None,
            fs: demo::make_fs(),
            cwd: vec!["var".into(), "log".into()],
            sel_file: Some("syslog".into()),
            search: String::new(),
            grid_view: false,
            sort_key: 0,
            sort_desc: false,
            ctx_menu: None,
            ctx_t: Tween::new(0.0),
            quick_look: None,
            ql_t: Tween::new(0.0),
            confirm_delete: None,
            conf_t: Tween::new(0.0),
            live: None,
            connect_open: false,
            connect_t: Tween::new(0.0),
            conn_host: String::new(),
            conn_user: "root".to_string(),
            conn_key: String::new(),
            conn_error: None,
            guide_open: false,
            guide_t: Tween::new(0.0),
            guide_t0: 0.0,
            guide_tex: None,
            guide_frame: None,
            refocus_files: false,
            sessions: demo::make_sessions(),
            device_menu: false,
            dev_t: Tween::new(0.0),
            cc_open: false,
            cc_t: Tween::new(0.0),
            cc_view: CcView::Main,
            wifi_on: true,
            bt_on: false,
            brightness: 0.8,
            volume: 0.55,
            toast: None,
            toast_t: Tween::new(0.0),
            reduce_motion: reduce,
            touched: HashSet::new(),
            shot: Shot { dir, design: Rect::NOTHING, ..Default::default() },
        };
        // BENCHPEEK_LIVE=user@host [BENCHPEEK_SSH_KEY=path]: start connected (kiosk / testing).
        if let Ok(target) = std::env::var("BENCHPEEK_LIVE") {
            if let Some((user, host)) = target.split_once('@') {
                app.live = Some(Live::connect(host.to_string(), user.to_string(), std::env::var("BENCHPEEK_SSH_KEY").ok()));
            }
        }
        app
    }

    pub fn go(&mut self, page: Page, now: f64) {
        if self.page != page {
            self.page = page;
            self.page_t0 = now;
        }
        self.device_menu = false;
    }

    pub fn open_guide(&mut self, now: f64) {
        self.guide_open = true;
        self.guide_t0 = now;
        self.guide_frame = None;
        self.device_menu = false;
    }

    pub fn open_connect(&mut self, now: f64) {
        self.connect_open = true;
        self.conn_error = None;
        self.device_menu = false;
        self.touched.remove("conn_focused");
        let _ = now;
    }

    pub fn disconnect_live(&mut self) {
        if let Some(mut l) = self.live.take() {
            // Joining the SSH threads can take a moment; keep the UI responsive.
            std::thread::spawn(move || l.stop());
        }
        self.selected_log = None;
        self.pin_scroll = false;
    }

    pub fn show_toast(&mut self, title: &str, sub: &str, view_session: bool, now: f64) {
        self.toast = Some(Toast { title: title.into(), sub: sub.into(), view_session, shown_at: now });
        self.toast_t.snap(0.0);
        self.toast_t.set(1.0, now, 250.0);
    }

    /// Keep the UI at the 1280x800 design proportions whatever the window size.
    fn apply_scale(&self, ctx: &egui::Context) {
        let native = ctx.native_pixels_per_point().unwrap_or(1.0);
        let px = ctx.content_rect().size() * ctx.pixels_per_point();
        let target = (px.x / DESIGN_W).min(px.y / DESIGN_H).max(0.3);
        let zoom = target / native;
        if (ctx.zoom_factor() - zoom).abs() > 0.004 {
            ctx.set_zoom_factor(zoom);
        }
    }

    fn frame(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.apply_scale(&ctx);
        let now = ctx.input(|i| i.time);
        self.demo.tick(now, self.capture_on && !self.paused && self.replay.is_none());
        if let Some(l) = &mut self.live {
            l.pump(now, self.capture_on && !self.paused);
        }
        self.handle_keys(&ctx, now);

        // The whole UI lives on a fixed 1280x800 design rectangle, centred in
        // the window (bars appear only when the window is not 16:10).
        let avail = ui.max_rect();
        let full = Rect::from_center_size(
            avail.center(),
            Vec2::new(DESIGN_W.min(avail.width()), DESIGN_H.min(avail.height())),
        );
        self.shot.design = full;
        let painter = ui.painter().clone();
        if let Some(t) = &self.bg {
            painter.image(t.id(), avail, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        } else {
            painter.rect_filled(avail, 0.0, BG);
        }
        // Sidebar and top-bar fills reach the window edges.
        painter.rect_filled(
            Rect::from_min_max(avail.min, Pos2::new(full.left() + SIDEBAR_W, avail.bottom())),
            0.0,
            SIDEBAR,
        );

        // Diagnostics side panel width follows a tween; the page adapts.
        self.diag_w.set(if self.diag_open && self.page == Page::Monitor { DIAG_W } else { 0.0 }, now, 270.0);
        let dw = self.diag_w.get(now);

        let page_rect = Rect::from_min_max(
            Pos2::new(full.left() + SIDEBAR_W, full.top() + TOPBAR_H),
            Pos2::new(full.right() - dw, full.bottom()),
        );
        self.page_ui(ui, page_rect, now);
        if dw > 0.5 {
            self.diag_panel(ui, Rect::from_min_max(Pos2::new(full.right() - dw, full.top() + TOPBAR_H), full.max), now);
        }
        self.sidebar(ui, full, now);
        self.topbar(ui, full, now);
        self.overlays(&ctx, full, now);

        // Display brightness: a real, full-screen dim.
        let dim = ((1.0 - self.brightness) * 0.6 * 255.0) as u8;
        if dim > 0 {
            ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("dim")))
                .rect_filled(full, 0.0, Color32::from_black_alpha(dim));
        }

        self.screenshot_harness(&ctx, now);

        // Animations repaint continuously; otherwise a slow tick keeps the
        // clock, log and metrics live.
        let animating = self.nav_y.running(now)
            || self.diag_w.running(now)
            || self.ctx_t.running(now)
            || self.ql_t.running(now)
            || self.conf_t.running(now)
            || self.guide_t.running(now)
            || self.connect_t.running(now)
            || self.live.is_some()
            || self.guide_open
            || self.dev_t.running(now)
            || self.cc_t.running(now)
            || self.toast_t.running(now)
            || now - self.page_t0 < 0.3
            || self.recheck_at.is_some();
        if animating {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context, now: f64) {
        let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if esc {
            if self.connect_open {
                self.connect_open = false;
            } else if self.guide_open {
                self.guide_open = false;
            } else if self.confirm_delete.is_some() {
                self.confirm_delete = None;
            } else if self.quick_look.is_some() {
                self.quick_look = None;
                self.refocus_files = true;
            } else if self.ctx_menu.is_some() {
                self.ctx_menu = None;
            } else if self.cc_open {
                self.cc_open = false;
            } else if self.device_menu {
                self.device_menu = false;
            }
        }
        let _ = now;
    }

    // ------------------------------------------------------------ chrome --

    fn sidebar(&mut self, ui: &mut Ui, full: Rect, now: f64) {
        let p = ui.painter().clone();
        let rect = Rect::from_min_max(full.min, Pos2::new(full.left() + SIDEBAR_W, full.bottom()));
        p.rect_filled(rect, 0.0, SIDEBAR);
        p.line_segment(
            [Pos2::new(rect.right() - 0.5, rect.top()), Pos2::new(rect.right() - 0.5, rect.bottom())],
            Stroke::new(1.0, hairline()),
        );

        // Brand.
        icon(&p, ic::BOX, Pos2::new(full.left() + 36.0, full.top() + 36.0), 22.0, TEXT);
        text(&p, Pos2::new(full.left() + 58.0, full.top() + 36.0), Align2::LEFT_CENTER, "Workspace", font(17.0, "inter_medium"), TEXT);

        // Selection pill first, so rows draw on top of it.
        let nav_top = full.top() + 86.0;
        let settings_top = full.bottom() - 16.0 - 64.0 - 12.0 - 50.0;
        let y_of = |pg: Page| -> f32 {
            match pg {
                Page::Settings => settings_top,
                other => nav_top + Page::NAV.iter().position(|q| *q == other).unwrap() as f32 * 50.0,
            }
        };
        self.nav_y.set(y_of(self.page) - full.top(), now, 200.0);
        let py = full.top() + self.nav_y.get(now);
        p.rect_filled(
            Rect::from_min_size(Pos2::new(full.left() + 12.0, py), Vec2::new(SIDEBAR_W - 24.0, 50.0)),
            cr(12.0),
            SELECTED,
        );

        let mut go_to = None;
        let mut items: Vec<Page> = Page::NAV.to_vec();
        items.push(Page::Settings);
        for pg in items {
            let r = Rect::from_min_size(Pos2::new(full.left() + 12.0, y_of(pg)), Vec2::new(SIDEBAR_W - 24.0, 50.0));
            let resp = ui.interact(r, Id::new(("nav", pg.label())), Sense::click());
            let h = hv(ui, Id::new(("navh", pg.label())), resp.hovered() && pg != self.page, 120.0);
            if h > 0.0 {
                p.rect_filled(r, cr(12.0), Color32::from_white_alpha((h * 12.0) as u8));
            }
            let sel = pg == self.page;
            let col = if sel { TEXT } else { mix(TEXT2, TEXT, h) };
            icon(&p, pg.icon(), Pos2::new(r.left() + 28.0, r.center().y), 20.0, col);
            text(&p, Pos2::new(r.left() + 54.0, r.center().y), Align2::LEFT_CENTER, pg.label(), font(15.0, if sel { "inter_medium" } else { "inter" }), col);
            if resp.has_focus() {
                focus_ring(&p, r, 12.0);
            }
            if resp.clicked() {
                go_to = Some(pg);
            }
        }
        if let Some(pg) = go_to {
            self.go(pg, now);
        }

        // Device selector.
        let dr = Rect::from_min_size(
            Pos2::new(full.left() + 12.0, full.bottom() - 16.0 - 64.0),
            Vec2::new(SIDEBAR_W - 24.0, 64.0),
        );
        let resp = ui.interact(dr, Id::new("dev_sel"), Sense::click());
        let h = hv(ui, Id::new("dev_sel_h"), resp.hovered() || self.device_menu, 120.0);
        p.rect_filled(dr, cr(14.0), mix(PANEL, RAISED, h));
        p.rect_stroke(dr, cr(14.0), Stroke::new(1.0, Color32::from_white_alpha(30 + (h * 20.0) as u8)), egui::StrokeKind::Inside);
        icon(&p, ic::MICROCHIP, Pos2::new(dr.left() + 28.0, dr.center().y), 22.0, TEXT);
        text(&p, Pos2::new(dr.left() + 50.0, dr.center().y - 9.0), Align2::LEFT_CENTER, "STM32MP257F-DK", font(13.5, "inter_medium"), TEXT);
        text(&p, Pos2::new(dr.left() + 50.0, dr.center().y + 10.0), Align2::LEFT_CENTER, "2 devices", font(12.0, "inter"), TEXT2);
        icon(&p, ic::CHEVRON_RIGHT, Pos2::new(dr.right() - 20.0, dr.center().y), 16.0, TEXT2);
        if resp.has_focus() {
            focus_ring(&p, dr, 14.0);
        }
        if resp.clicked() {
            self.device_menu = !self.device_menu;
            self.dev_t.set(if self.device_menu { 1.0 } else { 0.0 }, now, 180.0);
        }
        if self.device_menu && self.dev_t.target() < 0.5 {
            self.dev_t.set(1.0, now, 180.0);
        }
    }

    fn topbar(&mut self, ui: &mut Ui, full: Rect, now: f64) {
        let p = ui.painter().clone();
        let bar = Rect::from_min_max(
            Pos2::new(full.left() + SIDEBAR_W, full.top()),
            Pos2::new(full.right(), full.top() + TOPBAR_H),
        );
        p.rect_filled(bar, 0.0, Color32::from_rgba_unmultiplied(9, 11, 12, 150));
        p.line_segment([Pos2::new(bar.left(), bar.bottom() - 0.5), Pos2::new(bar.right(), bar.bottom() - 0.5)], Stroke::new(1.0, hairline()));
        let cy = bar.center().y;
        let x = bar.left() + MARGIN;
        icon(&p, ic::MICROCHIP, Pos2::new(x + 9.0, cy), 18.0, TEXT2);
        let device_name = match (&self.live, self.page) {
            (Some(l), Page::Monitor) => l.target(),
            _ => "STM32MP257F-DK".to_string(),
        };
        let r = text(&p, Pos2::new(x + 26.0, cy), Align2::LEFT_CENTER, device_name, font(14.0, "inter_medium"), TEXT);
        p.line_segment([Pos2::new(r.right() + 14.0, cy - 9.0), Pos2::new(r.right() + 14.0, cy + 9.0)], Stroke::new(1.0, border()));
        text(&p, Pos2::new(r.right() + 28.0, cy), Align2::LEFT_CENTER, self.page.label(), font(14.0, "inter"), TEXT2);

        // Right cluster: clock, then USB / Wi-Fi / speaker (opens Control Center).
        let mut rx = bar.right() - MARGIN + 6.0;
        let clock = chrono::Local::now().format("%H:%M").to_string();
        let cr_ = text(&p, Pos2::new(rx, cy), Align2::RIGHT_CENTER, clock, font(14.0, "inter_medium"), TEXT);
        rx = cr_.left() - 16.0;
        let cluster = Rect::from_min_max(Pos2::new(rx - 108.0, cy - 22.0), Pos2::new(rx + 4.0, cy + 22.0));
        let resp = ui.interact(cluster, Id::new("cc_trigger"), Sense::click());
        let h = hv(ui, Id::new("cc_trigger_h"), resp.hovered() || self.cc_open, 120.0);
        p.rect_filled(cluster.shrink2(Vec2::new(0.0, 4.0)), cr(10.0), Color32::from_white_alpha((h * 22.0) as u8));
        let ccol = mix(TEXT2, TEXT, h);
        icon(&p, ic::USB, Pos2::new(cluster.left() + 24.0, cy), 17.0, ccol);
        icon(&p, if self.wifi_on { ic::WIFI } else { ic::WIFI_OFF }, Pos2::new(cluster.left() + 56.0, cy), 17.0, ccol);
        icon(&p, ic::VOLUME_2, Pos2::new(cluster.left() + 88.0, cy), 17.0, ccol);
        if resp.has_focus() {
            focus_ring(&p, cluster.shrink2(Vec2::new(0.0, 4.0)), 10.0);
        }
        if resp.clicked() {
            self.cc_open = !self.cc_open;
            self.cc_view = CcView::Main;
            self.cc_t.set(if self.cc_open { 1.0 } else { 0.0 }, now, 230.0);
        }
        rx = cluster.left() - 12.0;

        // Issue chip (opens the diagnostics panel).
        let n = issues!(self).len();
        if n > 0 {
            let label = format!("{n} issues");
            let g = p.layout_no_wrap(label.clone(), font(13.0, "inter_medium"), TEXT);
            let w = g.size().x + 46.0;
            let chip = Rect::from_min_max(Pos2::new(rx - w, cy - 17.0), Pos2::new(rx, cy + 17.0));
            let hit = Rect::from_center_size(chip.center(), Vec2::new(chip.width(), 44.0));
            let resp = ui.interact(hit, Id::new("issue_chip"), Sense::click());
            let h = hv(ui, Id::new("issue_chip_h"), resp.hovered(), 120.0);
            p.rect_filled(chip, cr(17.0), Color32::from_white_alpha(10 + (h * 14.0) as u8));
            p.rect_stroke(chip, cr(17.0), Stroke::new(1.0, Color32::from_white_alpha(40 + (h * 30.0) as u8)), egui::StrokeKind::Inside);
            icon(&p, ic::TRIANGLE_ALERT, Pos2::new(chip.left() + 19.0, cy), 15.0, TEXT);
            text(&p, Pos2::new(chip.left() + 34.0, cy), Align2::LEFT_CENTER, label, font(13.0, "inter_medium"), TEXT);
            if resp.has_focus() {
                focus_ring(&p, chip, 17.0);
            }
            if resp.clicked() {
                self.diag_open = true;
                self.go(Page::Monitor, now);
            }
            rx = chip.left() - 10.0;
        }

        // DEMO badge - always visible: all device data here is simulated.
        // Only the Live monitor shows real data; every other page is still demo.
        let badge = match (&self.live, self.page) {
            (Some(l), Page::Monitor) => format!("LIVE  {}", l.target()),
            _ => "DEMO DATA".to_string(),
        };
        let g = p.layout_no_wrap(badge.clone(), font(11.0, "inter_semibold"), TEXT2);
        let w = g.size().x + 20.0;
        let chip = Rect::from_min_max(Pos2::new(rx - w, cy - 11.0), Pos2::new(rx, cy + 11.0));
        p.rect_stroke(chip, cr(11.0), Stroke::new(1.0, Color32::from_white_alpha(60)), egui::StrokeKind::Inside);
        text(&p, chip.center(), Align2::CENTER_CENTER, badge, font(11.0, "inter_semibold"), if self.live.is_some() && self.page == Page::Monitor { TEXT } else { TEXT2 });
    }

    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, now: f64) {
        let p = if reduce_motion() { 1.0 } else { ((now - self.page_t0) / 0.22).clamp(0.0, 1.0) as f32 };
        let e = ease(p);
        let shifted = rect.translate(Vec2::new(0.0, (1.0 - e) * 8.0));
        let inner = Rect::from_min_max(
            Pos2::new(shifted.left() + MARGIN, shifted.top() + 36.0),
            Pos2::new(shifted.right() - MARGIN, shifted.bottom() - 28.0),
        );
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        cui.set_clip_rect(rect);
        cui.set_opacity(e);
        match self.page {
            Page::Home => self.home_page(&mut cui, inner, now),
            Page::Devices => self.devices_page(&mut cui, inner, now),
            Page::Monitor => self.monitor_page(&mut cui, inner, now),
            Page::Diagnostics => self.diagnostics_page(&mut cui, inner, now),
            Page::Files => self.files_page(&mut cui, inner, now),
            Page::Sessions => self.sessions_page(&mut cui, inner, now),
            Page::Settings => self.settings_page(&mut cui, inner, now),
        }
    }

    // ---------------------------------------------------------- captures --

    pub fn start_capture(&mut self, now: f64) {
        self.capture_on = true;
        self.paused = false;
        self.capture_started = now;
    }

    pub fn stop_capture_and_save(&mut self, now: f64) {
        self.capture_on = false;
        let dur = (now - self.capture_started).max(1.0) as u32 + 42;
        let name = format!("Capture {}", chrono::Local::now().format("%H:%M"));
        let path = format!("~/sessions/{}.bpk", chrono::Local::now().format("%Y-%m-%d_%H%M"));
        self.sessions.insert(
            0,
            Session { name, when: "Today, just now".into(), duration_s: dur, errors: issues!(self).len(), lines: self.demo.lines.len().min(2000), seed: 99 },
        );
        self.show_toast("Capture saved", &path, true, now);
    }

    // ---------------------------------------------------- screenshot mode --

    /// Development aid: with BENCHPEEK_SHOT_DIR set, walks through the four
    /// reference states plus the extra pages, saves 2560x1600 PNGs of the
    /// design rectangle, then exits once every shot is on disk.
    fn screenshot_harness(&mut self, ctx: &egui::Context, now: f64) {
        let Some(dir) = self.shot.dir.clone() else { return };
        for ev in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot { image, user_data, .. } = ev {
                let name = user_data
                    .data
                    .as_ref()
                    .and_then(|d| d.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "shot".into());
                let ppp = ctx.pixels_per_point();
                let d = self.shot.design;
                let saved = self.shot.saved.clone();
                let path = dir.join(format!("{name}.png"));
                // Image work is slow in debug builds; keep it off the UI thread.
                std::thread::spawn(move || {
                    let (w, h) = (image.width() as u32, image.height() as u32);
                    let mut buf = image::RgbaImage::new(w, h);
                    for (i, px) in image.pixels.iter().enumerate() {
                        buf.put_pixel(i as u32 % w, i as u32 / w, image::Rgba(px.to_array()));
                    }
                    let x0 = ((d.left() * ppp).round().max(0.0) as u32).min(w.saturating_sub(1));
                    let y0 = ((d.top() * ppp).round().max(0.0) as u32).min(h.saturating_sub(1));
                    let cw = ((d.width() * ppp).round() as u32).min(w - x0).max(1);
                    let ch = ((d.height() * ppp).round() as u32).min(h - y0).max(1);
                    let crop = image::imageops::crop_imm(&buf, x0, y0, cw, ch).to_image();
                    let out = image::imageops::resize(&crop, 2560, 1600, image::imageops::FilterType::Triangle);
                    let _ = out.save(path);
                    saved.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                });
            }
        }
        let t0 = *self.shot.t0.get_or_insert(now);
        let t = now - t0;
        let at = |x: f64| t >= x;
        let mut snap: Option<&'static str> = None;
        let mut adv = true;
        match self.shot.stage {
            0 if at(1.2) => snap = Some("01_home"),
            1 if at(2.0) => { self.device_menu = true; }
            2 if at(2.9) => snap = Some("02_home_device_menu"),
            3 if at(3.3) => { self.device_menu = false; self.go(Page::Monitor, now); self.diag_open = true; }
            4 if at(5.0) => snap = Some("03_monitor_diagnostics"),
            5 if at(5.4) => {
                let id = self.demo.issues[0].log_id;
                self.selected_log = Some(id);
                self.scroll_to_log = Some(id);
            }
            6 if at(6.8) => snap = Some("04_monitor_inspect"),
            7 if at(7.2) => { self.diag_open = false; self.go(Page::Files, now); self.open_quick_look("syslog", now); }
            8 if at(8.6) => snap = Some("05_files_quicklook"),
            9 if at(9.0) => { self.quick_look = None; self.sel_file = Some("kern.log".into()); self.ctx_menu = Some((Pos2::new(640.0, 300.0), "kern.log".into())); }
            10 if at(10.0) => snap = Some("06_files_menu"),
            11 if at(10.4) => { self.ctx_menu = None; self.go(Page::Home, now); self.cc_open = true; self.show_toast("Capture saved", "~/sessions/2026-09-18_2104.bpk", true, now); }
            12 if at(12.0) => snap = Some("07_control_center_toast"),
            13 if at(12.4) => { self.cc_open = false; self.go(Page::Devices, now); }
            14 if at(13.2) => snap = Some("08_devices"),
            15 if at(13.6) => self.go(Page::Diagnostics, now),
            16 if at(14.4) => snap = Some("09_diagnostics"),
            17 if at(14.8) => self.go(Page::Sessions, now),
            18 if at(15.6) => snap = Some("10_sessions"),
            19 if at(16.0) => self.go(Page::Settings, now),
            20 if at(16.8) => snap = Some("11_settings"),
            21 if at(17.4) => { self.go(Page::Home, now); }
            22 if at(17.9) => { ctx.memory_mut(|m| m.request_focus(Id::new(("nav", "Files")))); }
            23 if at(18.4) => snap = Some("12_focus_ring"),
            24 if at(18.8) => { self.go(Page::Files, now); snap = Some("13_transition_mid"); }
            25 if at(19.4) => { self.go(Page::Devices, now); self.open_guide(now); }
            26 if at(24.6) => snap = Some("14_connect_guide_mid"),
            27 if at(28.0) => snap = Some("15_connect_guide_end"),
            28 if at(29.0) => { self.go(Page::Devices, now); self.guide_open = false; self.open_connect(now); self.conn_host = "192.168.7.1".into(); }
            29 if at(30.2) => snap = Some("16_connect_dialog"),
            30 if (self.shot.saved.load(std::sync::atomic::Ordering::SeqCst) >= self.shot.requested && at(31.0)) || at(90.0) => std::process::exit(0),
            _ => adv = false,
        }
        if let Some(name) = snap {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(name.to_string())));
            self.shot.requested += 1;
        }
        if adv {
            self.shot.stage += 1;
        }
    }
}

impl eframe::App for Workspace {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG))
            .show(ui, |ui| self.frame(ui));
    }
}
