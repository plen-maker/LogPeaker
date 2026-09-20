//! Floating layers: device menu, Control Center, file context menu, Quick
//! Look, delete confirmation and the toast. Every layer eases in with the
//! shared curve; scale/translate use egui's per-layer transform so hit-testing
//! follows the animation.

use eframe::egui::{
    self, emath::TSTransform, Align2, Color32, Context, Id, LayerId, Order, Pos2, Rect, Sense, Stroke, Ui,
    Vec2,
};

use super::theme::*;
use super::widgets::*;
use super::{CcView, Page, Workspace};
use crate::icons as ic;

const INNER: Color32 = Color32::from_rgb(0x27, 0x2B, 0x2D);

fn layer(ctx: &Context, id: &'static str, order: Order, t: f32, pivot: Pos2, scale0: f32, dy: f32, add: impl FnOnce(&mut Ui)) {
    egui::Area::new(Id::new(id)).order(order).fixed_pos(Pos2::ZERO).constrain(false).show(ctx, |ui| {
        let s = scale0 + (1.0 - scale0) * t;
        let tr = TSTransform::new(pivot.to_vec2() * (1.0 - s) + Vec2::new(0.0, dy * (1.0 - t)), s);
        ctx.set_transform_layer(ui.layer_id(), tr);
        ui.set_opacity(t);
        add(ui);
    });
}

fn menu_row(ui: &mut Ui, id: Id, rect: Rect, glyph: char, title: &str, sub: Option<&str>, trailing: Option<char>) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let h = hv(ui, id.with("h"), resp.hovered(), 120.0);
    let p = ui.painter();
    p.rect_filled(rect, cr(10.0), Color32::from_white_alpha((h * 20.0) as u8));
    icon(p, glyph, Pos2::new(rect.left() + 24.0, rect.center().y), 18.0, TEXT);
    match sub {
        Some(s) => {
            text(p, Pos2::new(rect.left() + 48.0, rect.center().y - 9.0), Align2::LEFT_CENTER, title, font(14.0, "inter_medium"), TEXT);
            text(p, Pos2::new(rect.left() + 48.0, rect.center().y + 10.0), Align2::LEFT_CENTER, s, font(12.0, "inter"), TEXT2);
        }
        None => {
            text(p, Pos2::new(rect.left() + 48.0, rect.center().y), Align2::LEFT_CENTER, title, font(14.0, "inter_medium"), TEXT);
        }
    }
    if let Some(t) = trailing {
        icon(p, t, Pos2::new(rect.right() - 22.0, rect.center().y), 16.0, TEXT2);
    }
    if resp.has_focus() {
        focus_ring(p, rect, 10.0);
    }
    resp
}

fn cc_tile(ui: &mut Ui, id: &'static str, rect: Rect, glyph: char, title: &str, status: &str, on: bool) -> (bool, bool) {
    let body = Rect::from_min_max(rect.min, Pos2::new(rect.right() - 40.0, rect.bottom()));
    let chev = Rect::from_min_max(Pos2::new(rect.right() - 44.0, rect.top()), rect.max);
    let r1 = ui.interact(body, Id::new((id, "tog")), Sense::click());
    let r2 = ui.interact(chev, Id::new((id, "chev")), Sense::click());
    let h = hv(ui, Id::new((id, "h")), r1.hovered() || r2.hovered(), 120.0);
    let p = ui.painter();
    p.rect_filled(rect, cr(14.0), mix(INNER, Color32::from_rgb(0x30, 0x34, 0x36), h));
    let cx = Pos2::new(rect.left() + 38.0, rect.center().y);
    p.circle_filled(cx, 21.0, if on { PRIMARY } else { Color32::from_white_alpha(22) });
    icon(p, glyph, cx, 21.0, if on { ON_PRIMARY } else { TEXT });
    text(p, Pos2::new(rect.left() + 70.0, rect.center().y - 9.0), Align2::LEFT_CENTER, title, font(14.0, "inter_medium"), TEXT);
    text(p, Pos2::new(rect.left() + 70.0, rect.center().y + 11.0), Align2::LEFT_CENTER, status, font(12.0, "inter"), TEXT2);
    icon(p, ic::CHEVRON_RIGHT, Pos2::new(rect.right() - 20.0, rect.center().y), 16.0, TEXT3);
    if r1.has_focus() {
        focus_ring(p, rect, 14.0);
    }
    (r1.clicked(), r2.clicked())
}

impl Workspace {
    pub(super) fn overlays(&mut self, ctx: &Context, full: Rect, now: f64) {
        // Targets follow the state flags every frame, so Esc/backdrop closes ease out too.
        self.cc_t.set(if self.cc_open { 1.0 } else { 0.0 }, now, 230.0);
        self.dev_t.set(if self.device_menu { 1.0 } else { 0.0 }, now, 180.0);
        self.ql_t.set(if self.quick_look.is_some() { 1.0 } else { 0.0 }, now, 210.0);
        self.ctx_t.set(if self.ctx_menu.is_some() { 1.0 } else { 0.0 }, now, 140.0);
        self.conf_t.set(if self.confirm_delete.is_some() { 1.0 } else { 0.0 }, now, 200.0);
        let (cc_t, dev_t, ql_t, ctx_t, conf_t) = (
            self.cc_t.get(now),
            self.dev_t.get(now),
            self.ql_t.get(now),
            self.ctx_t.get(now),
            self.conf_t.get(now),
        );

        // Dim behind Control Center / Quick Look / dialog; interactive only while something is open.
        let dim = (cc_t * 90.0).max(ql_t * 80.0).max(conf_t * 140.0) as u8;
        if dim > 0 {
            ctx.layer_painter(LayerId::new(Order::Middle, Id::new("dim_paint"))).rect_filled(full, 0.0, Color32::from_black_alpha(dim));
        }
        let anything_open = self.connect_open || self.guide_open || self.cc_open || self.device_menu || self.ctx_menu.is_some() || self.quick_look.is_some() || self.confirm_delete.is_some();
        if anything_open {
            let mut close = false;
            egui::Area::new(Id::new("backdrop")).order(Order::Middle).fixed_pos(full.min).constrain(false).show(ctx, |ui| {
                let r = ui.allocate_response(full.size(), Sense::click());
                close = r.clicked();
            });
            if close {
                if self.connect_open {
                    self.connect_open = false;
                } else if self.guide_open {
                    self.guide_open = false;
                } else if self.confirm_delete.is_some() {
                    self.confirm_delete = None;
                } else if self.quick_look.is_some() {
                    self.quick_look = None;
                    self.refocus_files = true;
                }
                self.cc_open = false;
                self.device_menu = false;
                self.ctx_menu = None;
            }
        }

        self.device_menu_ui(ctx, full, dev_t, now);
        self.control_center_ui(ctx, full, cc_t, now);
        self.context_menu_ui(ctx, full, ctx_t, now);
        self.quick_look_ui(ctx, full, ql_t, now);
        self.confirm_ui(ctx, full, conf_t, now);
        self.guide_ui(ctx, full, now);
        self.connect_ui(ctx, full, now);
        self.toast_ui(ctx, full, now);
    }

    fn device_menu_ui(&mut self, ctx: &Context, full: Rect, t: f32, now: f64) {
        if t < 0.002 {
            return;
        }
        let (w, h) = (304.0, 8.0 + 64.0 + 64.0 + 13.0 + 52.0 + 8.0);
        let bottom = full.bottom() - 16.0 - 64.0 - 10.0;
        let rect = Rect::from_min_size(Pos2::new(full.left() + 28.0, bottom - h), Vec2::new(w, h));
        let mut act = 0;
        layer(ctx, "dev_menu", Order::Foreground, t, rect.left_bottom(), 0.97, 0.0, |ui| {
            panel(ui.painter(), rect, 16.0, RAISED);
            let mut y = rect.top() + 8.0;
            let row = |y: f32| Rect::from_min_size(Pos2::new(rect.left() + 8.0, y), Vec2::new(w - 16.0, 64.0));
            if menu_row(ui, Id::new("dm_active"), row(y), ic::MICROCHIP, "STM32MP257F-DK", Some("Connected via USB"), Some(ic::CHECK)).clicked() {
                act = 1;
            }
            y += 64.0;
            if menu_row(ui, Id::new("dm_custom"), row(y), ic::SERVER, "Custom Board", Some("Not connected"), None).clicked() {
                act = 2;
            }
            y += 64.0 + 6.0;
            ui.painter().line_segment([Pos2::new(rect.left() + 16.0, y), Pos2::new(rect.right() - 16.0, y)], Stroke::new(1.0, hairline()));
            y += 7.0;
            if menu_row(ui, Id::new("dm_add"), Rect::from_min_size(Pos2::new(rect.left() + 8.0, y), Vec2::new(w - 16.0, 52.0)), ic::PLUS, "Add device", None, None).clicked() {
                act = 3;
            }
        });
        match act {
            1 => self.device_menu = false,
            2 => {
                self.device_menu = false;
                if self.live.is_some() {
                    self.go(Page::Devices, now);
                } else {
                    self.open_connect(now);
                }
            }
            3 => {
                self.device_menu = false;
                self.open_guide(now);
            }
            _ => {}
        }
    }

    fn control_center_ui(&mut self, ctx: &Context, full: Rect, t: f32, now: f64) {
        if t < 0.002 {
            return;
        }
        let rect = Rect::from_min_size(Pos2::new(full.right() - 16.0 - 380.0, full.top() + 66.0), Vec2::new(380.0, 474.0));
        let mut wifi_on = self.wifi_on;
        let mut bt_on = self.bt_on;
        let (mut brightness, mut volume) = (self.brightness, self.volume);
        let mut view = self.cc_view;
        let mut capture = self.capture_on;
        let mut go_devices = false;
        let capture_before = capture;
        layer(ctx, "control_center", Order::Foreground, t, Pos2::new(rect.right(), rect.top()), 0.98, -8.0, |ui| {
            panel(ui.painter(), rect, 20.0, Color32::from_rgb(0x1B, 0x1E, 0x20));
            let x = rect.left() + 14.0;
            let iw = rect.width() - 28.0;
            match view {
                CcView::Main => {
                    let mut y = rect.top() + 14.0;
                    let tw = (iw - 10.0) / 2.0;
                    let (t1, c1) = cc_tile(ui, "cc_wifi", Rect::from_min_size(Pos2::new(x, y), Vec2::new(tw, 84.0)), if wifi_on { ic::WIFI } else { ic::WIFI_OFF }, "Wi-Fi", if wifi_on { "yocto-lab" } else { "Off" }, wifi_on);
                    let (t2, c2) = cc_tile(ui, "cc_bt", Rect::from_min_size(Pos2::new(x + tw + 10.0, y), Vec2::new(tw, 84.0)), if bt_on { ic::BLUETOOTH } else { ic::BLUETOOTH_OFF }, "Bluetooth", if bt_on { "On" } else { "Off" }, bt_on);
                    if t1 { wifi_on = !wifi_on; }
                    if t2 { bt_on = !bt_on; }
                    if c1 { view = CcView::Wifi; }
                    if c2 { view = CcView::Bluetooth; }
                    y += 84.0 + 10.0;

                    let er = Rect::from_min_size(Pos2::new(x, y), Vec2::new(iw, 56.0));
                    let resp = ui.interact(er, Id::new("cc_eth"), Sense::click());
                    let h = hv(ui, Id::new("cc_eth_h"), resp.hovered(), 120.0);
                    let p = ui.painter();
                    p.rect_filled(er, cr(14.0), mix(INNER, Color32::from_rgb(0x30, 0x34, 0x36), h));
                    p.circle_filled(Pos2::new(er.left() + 30.0, er.center().y), 17.0, PRIMARY);
                    icon(p, ic::ETHERNET_PORT, Pos2::new(er.left() + 30.0, er.center().y), 17.0, ON_PRIMARY);
                    text(p, Pos2::new(er.left() + 58.0, er.center().y - 9.0), Align2::LEFT_CENTER, "Ethernet", font(14.0, "inter_medium"), TEXT);
                    text(p, Pos2::new(er.left() + 58.0, er.center().y + 10.0), Align2::LEFT_CENTER, "Connected (eth0)", font(12.0, "inter"), TEXT2);
                    icon(p, ic::CHEVRON_RIGHT, Pos2::new(er.right() - 20.0, er.center().y), 16.0, TEXT3);
                    if resp.has_focus() { focus_ring(p, er, 14.0); }
                    if resp.clicked() { go_devices = true; }
                    y += 56.0 + 10.0;

                    for (k, (label, glyph)) in [("Display brightness", ic::SUN), ("Volume", ic::VOLUME_2)].iter().enumerate() {
                        let br = Rect::from_min_size(Pos2::new(x, y), Vec2::new(iw, 64.0));
                        ui.painter().rect_filled(br, cr(14.0), INNER);
                        text(ui.painter(), Pos2::new(br.left() + 16.0, br.top() + 20.0), Align2::LEFT_CENTER, *label, font(13.0, "inter_medium"), TEXT);
                        let v = if k == 0 { &mut brightness } else { &mut volume };
                        text(ui.painter(), Pos2::new(br.right() - 16.0, br.top() + 20.0), Align2::RIGHT_CENTER, format!("{:.0}%", *v * 100.0), font(12.5, "inter"), TEXT2);
                        icon(ui.painter(), *glyph, Pos2::new(br.left() + 26.0, br.bottom() - 18.0), 17.0, TEXT2);
                        slider(ui, Id::new(("cc_slider", k)), Rect::from_min_max(Pos2::new(br.left() + 50.0, br.bottom() - 26.0), Pos2::new(br.right() - 20.0, br.bottom() - 10.0)), v);
                        y += 64.0 + 6.0;
                    }
                    y += 4.0;

                    let lr = Rect::from_min_size(Pos2::new(x, y), Vec2::new(iw, 56.0));
                    ui.painter().rect_filled(lr, cr(14.0), INNER);
                    icon(ui.painter(), ic::TERMINAL, Pos2::new(lr.left() + 30.0, lr.center().y), 19.0, TEXT);
                    text(ui.painter(), Pos2::new(lr.left() + 58.0, lr.center().y - 9.0), Align2::LEFT_CENTER, "Log capture", font(14.0, "inter_medium"), TEXT);
                    text(ui.painter(), Pos2::new(lr.left() + 58.0, lr.center().y + 10.0), Align2::LEFT_CENTER, if capture { "Recording device logs" } else { "Off - turning on starts a capture" }, font(12.0, "inter"), TEXT2);
                    if toggle(ui, Id::new("cc_capture"), Pos2::new(lr.right() - 16.0, lr.center().y), capture).clicked() {
                        capture = !capture;
                    }
                    y += 56.0 + 10.0;

                    let sr = Rect::from_min_size(Pos2::new(x, y), Vec2::new(iw, 76.0));
                    ui.painter().rect_filled(sr, cr(14.0), INNER);
                    icon(ui.painter(), ic::HARD_DRIVE, Pos2::new(sr.left() + 28.0, sr.top() + 26.0), 18.0, TEXT);
                    text(ui.painter(), Pos2::new(sr.left() + 52.0, sr.top() + 26.0), Align2::LEFT_CENTER, "Storage (eMMC)", font(14.0, "inter_medium"), TEXT);
                    text(ui.painter(), Pos2::new(sr.right() - 16.0, sr.top() + 26.0), Align2::RIGHT_CENTER, "42 / 64 GB", font(13.0, "inter"), TEXT2);
                    progress(ui.painter(), Rect::from_min_size(Pos2::new(sr.left() + 16.0, sr.top() + 50.0), Vec2::new(sr.width() - 32.0, 6.0)), 42.0 / 64.0);
                }
                CcView::Wifi | CcView::Bluetooth => {
                    let is_wifi = view == CcView::Wifi;
                    if icon_button(ui, Id::new("cc_back"), Pos2::new(x + 22.0, rect.top() + 36.0), ic::CHEVRON_LEFT, 20.0, TEXT).clicked() {
                        view = CcView::Main;
                    }
                    text(ui.painter(), Pos2::new(x + 52.0, rect.top() + 36.0), Align2::LEFT_CENTER, if is_wifi { "Wi-Fi" } else { "Bluetooth" }, font(17.0, "inter_medium"), TEXT);
                    let on = if is_wifi { &mut wifi_on } else { &mut bt_on };
                    if toggle(ui, Id::new("cc_sub_toggle"), Pos2::new(rect.right() - 20.0, rect.top() + 36.0), *on).clicked() {
                        *on = !*on;
                    }
                    let list: &[(&str, &str)] = if is_wifi {
                        &[("yocto-lab", "Connected"), ("lab-guest", "Secured"), ("workshop-5G", "Secured"), ("Hidden network", "Enter name")]
                    } else {
                        &[("Keyboard K380", "Paired"), ("Headset", "Not connected"), ("Debug probe", "Not connected")]
                    };
                    let mut y = rect.top() + 80.0;
                    for (name, st) in list {
                        let rr = Rect::from_min_size(Pos2::new(x, y), Vec2::new(iw, 54.0));
                        let dim = !*on;
                        let resp = ui.interact(rr, Id::new(("cc_net", *name)), Sense::click());
                        let h = hv(ui, Id::new(("cc_net_h", *name)), resp.hovered(), 120.0);
                        let p = ui.painter();
                        p.rect_filled(rr, cr(12.0), mix(INNER, Color32::from_rgb(0x30, 0x34, 0x36), h));
                        icon(p, if is_wifi { ic::WIFI } else { ic::BLUETOOTH }, Pos2::new(rr.left() + 28.0, rr.center().y), 18.0, if dim { TEXT3 } else { TEXT });
                        text(p, Pos2::new(rr.left() + 54.0, rr.center().y - 9.0), Align2::LEFT_CENTER, *name, font(14.0, "inter_medium"), if dim { TEXT3 } else { TEXT });
                        text(p, Pos2::new(rr.left() + 54.0, rr.center().y + 10.0), Align2::LEFT_CENTER, *st, font(12.0, "inter"), TEXT2);
                        if *st == "Connected" || *st == "Paired" {
                            icon(p, ic::CHECK, Pos2::new(rr.right() - 22.0, rr.center().y), 17.0, if dim { TEXT3 } else { TEXT });
                        }
                        y += 60.0;
                    }
                    text(ui.painter(), Pos2::new(x + 6.0, rect.bottom() - 24.0), Align2::LEFT_CENTER, "Demo list - no radio access in this build", font(12.0, "inter"), TEXT3);
                }
            }
        });
        self.wifi_on = wifi_on;
        self.bt_on = bt_on;
        self.brightness = brightness;
        self.volume = volume;
        self.cc_view = view;
        if capture != capture_before {
            if capture {
                self.replay = None;
                self.start_capture(now);
            } else {
                self.stop_capture_and_save(now);
            }
        }
        if go_devices {
            self.cc_open = false;
            self.go(Page::Devices, now);
        }
    }

    fn context_menu_ui(&mut self, ctx: &Context, full: Rect, t: f32, now: f64) {
        let Some((pos, name)) = self.ctx_menu.clone() else {
            if t > 0.002 {
                // Closing: nothing to draw once the target is gone.
            }
            return;
        };
        let is_dir = self.cwd_node().children.iter().find(|c| c.name == name).map_or(false, |c| c.dir);
        let (w, h) = (228.0, 8.0 + 44.0 * 4.0 + 13.0 + 44.0 + 8.0);
        let x = pos.x.min(full.right() - w - 12.0).max(full.left() + 12.0);
        let y = pos.y.min(full.bottom() - h - 12.0).max(full.top() + 12.0);
        let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
        let mut act = 0;
        layer(ctx, "file_menu", Order::Foreground, t, rect.left_top(), 0.96, 0.0, |ui| {
            panel(ui.painter(), rect, 14.0, RAISED);
            let mut yy = rect.top() + 8.0;
            let items = [(1, ic::FOLDER_OPEN, "Open"), (2, ic::EYE, "Quick Look"), (3, ic::COPY, "Copy path"), (4, ic::SQUARE_ARROW_OUT_UP_RIGHT, "Export")];
            for (code, glyph, label) in items {
                if menu_row(ui, Id::new(("fm", code)), Rect::from_min_size(Pos2::new(rect.left() + 6.0, yy), Vec2::new(w - 12.0, 44.0)), glyph, label, None, None).clicked() {
                    act = code;
                }
                yy += 44.0;
            }
            yy += 6.0;
            ui.painter().line_segment([Pos2::new(rect.left() + 14.0, yy), Pos2::new(rect.right() - 14.0, yy)], Stroke::new(1.0, hairline()));
            yy += 7.0;
            if menu_row(ui, Id::new(("fm", 5)), Rect::from_min_size(Pos2::new(rect.left() + 6.0, yy), Vec2::new(w - 12.0, 44.0)), ic::TRASH_2, "Delete", None, None).clicked() {
                act = 5;
            }
        });
        if act != 0 {
            self.ctx_menu = None;
        }
        match act {
            1 => self.open_entry(&name, now),
            2 if !is_dir => self.open_quick_look(&name, now),
            2 => self.show_toast("Quick Look works on files", "Open the folder instead", false, now),
            3 => {
                let path = self.path_string(&name);
                ctx.copy_text(path.clone());
                self.show_toast("Path copied", &path, false, now);
            }
            4 => {
                if is_dir {
                    self.show_toast("Folders can't be exported yet", "Demo backend exports single files", false, now);
                } else {
                    let dir = std::env::temp_dir().join("benchpeek-export");
                    let target = dir.join(&name);
                    match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&target, self.file_content(&name))) {
                        Ok(()) => self.show_toast("Exported", &target.display().to_string(), false, now),
                        Err(e) => self.show_toast("Export failed", &e.to_string(), false, now),
                    }
                }
            }
            5 => {
                self.confirm_delete = Some(name);
                self.conf_t.snap(0.0);
                self.touched.remove("conf_focused");
            }
            _ => {}
        }
    }

    fn quick_look_ui(&mut self, ctx: &Context, full: Rect, t: f32, _now: f64) {
        if t < 0.002 {
            return;
        }
        let Some(name) = self.quick_look.clone().or_else(|| self.sel_file.clone()) else { return };
        let content = self.file_content(&name);
        let lines: Vec<&str> = content.lines().collect();
        let (w, h) = (760.0, 500.0);
        let cx = full.left() + SIDEBAR_W_F + (full.width() - SIDEBAR_W_F) / 2.0;
        let rect = Rect::from_min_size(Pos2::new(cx - w / 2.0, full.top() + 84.0), Vec2::new(w, h));
        if self.quick_look.is_some() && !self.touched.contains("ql_focused") {
            ctx.memory_mut(|m| m.request_focus(Id::new("ql_close")));
            self.touched.insert("ql_focused");
        }
        let mut close = false;
        layer(ctx, "quick_look", Order::Foreground, t, rect.center(), 0.97, 0.0, |ui| {
            panel(ui.painter(), rect, 18.0, RAISED);
            let hdr = Rect::from_min_size(rect.min, Vec2::new(w, 58.0));
            icon(ui.painter(), ic::FILE_TEXT, Pos2::new(hdr.left() + 30.0, hdr.center().y), 19.0, TEXT2);
            text(ui.painter(), Pos2::new(hdr.left() + 56.0, hdr.center().y), Align2::LEFT_CENTER, &name, font(15.0, "inter_medium"), TEXT);
            text(ui.painter(), Pos2::new(hdr.right() - 70.0, hdr.center().y), Align2::RIGHT_CENTER, format!("{} lines", lines.len()), font(12.5, "inter"), TEXT3);
            if icon_button(ui, Id::new("ql_close"), Pos2::new(hdr.right() - 34.0, hdr.center().y), ic::X, 19.0, TEXT2).clicked() {
                close = true;
            }
            ui.painter().line_segment([Pos2::new(hdr.left() + 1.0, hdr.bottom()), Pos2::new(hdr.right() - 1.0, hdr.bottom())], Stroke::new(1.0, hairline()));
            let body = Rect::from_min_max(Pos2::new(rect.left() + 12.0, hdr.bottom() + 8.0), Pos2::new(rect.right() - 12.0, rect.bottom() - 14.0));
            ui.painter().rect_filled(body, cr(10.0), Color32::from_rgb(0x12, 0x14, 0x15));
            let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink2(Vec2::new(4.0, 6.0))));
            bui.set_clip_rect(body);
            bui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(&mut bui, 22.0, lines.len().max(1), |ui, range| {
                for i in range {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
                    let pt = ui.painter().with_clip_rect(r);
                    text(&pt, Pos2::new(r.left() + 40.0, r.center().y), Align2::RIGHT_CENTER, i + 1, font(12.0, "mono"), TEXT3);
                    let line = ellipsize_mono(lines.get(i).copied().unwrap_or(""), 12.0, r.width() - 70.0);
                    text(&pt, Pos2::new(r.left() + 54.0, r.center().y), Align2::LEFT_CENTER, line, font(12.0, "mono"), TEXT);
                }
            });
        });
        if close {
            self.quick_look = None;
            self.refocus_files = true;
        }
    }

    fn confirm_ui(&mut self, ctx: &Context, full: Rect, t: f32, now: f64) {
        if t < 0.002 {
            return;
        }
        let Some(name) = self.confirm_delete.clone() else { return };
        let (w, h) = (440.0, 214.0);
        let rect = Rect::from_center_size(full.center(), Vec2::new(w, h));
        if !self.touched.contains("conf_focused") {
            ctx.memory_mut(|m| m.request_focus(Id::new("conf_cancel")));
            self.touched.insert("conf_focused");
        }
        let mut act = 0;
        layer(ctx, "confirm", Order::Foreground, t, rect.center(), 0.97, 0.0, |ui| {
            panel(ui.painter(), rect, 18.0, RAISED);
            icon(ui.painter(), ic::TRASH_2, Pos2::new(rect.left() + 42.0, rect.top() + 46.0), 24.0, TEXT);
            text(ui.painter(), Pos2::new(rect.left() + 74.0, rect.top() + 46.0), Align2::LEFT_CENTER, format!("Delete \"{name}\"?"), font(18.0, "inter_medium"), TEXT);
            text(ui.painter(), Pos2::new(rect.left() + 32.0, rect.top() + 92.0), Align2::LEFT_CENTER, "This removes it from the device's file system.", font(13.5, "inter"), TEXT2);
            text(ui.painter(), Pos2::new(rect.left() + 32.0, rect.top() + 114.0), Align2::LEFT_CENTER, "Demo file system - nothing on real hardware changes.", font(12.5, "inter"), TEXT3);
            let by = rect.bottom() - 24.0 - 48.0;
            if ghost_button(ui, Id::new("conf_cancel"), Rect::from_min_size(Pos2::new(rect.right() - 32.0 - 128.0 - 12.0 - 128.0, by), Vec2::new(128.0, 48.0)), "Cancel", None).clicked() {
                act = 1;
            }
            if primary_button(ui, Id::new("conf_delete"), Rect::from_min_size(Pos2::new(rect.right() - 32.0 - 128.0, by), Vec2::new(128.0, 48.0)), "Delete", None).clicked() {
                act = 2;
            }
        });
        match act {
            1 => self.confirm_delete = None,
            2 => {
                self.confirm_delete = None;
                self.delete_file(&name);
                self.refocus_files = false;
                self.show_toast("Deleted", &self.path_string(&name), false, now);
            }
            _ => {}
        }
    }

    fn toast_ui(&mut self, ctx: &Context, full: Rect, now: f64) {
        let Some(toast) = &self.toast else { return };
        if now - toast.shown_at > 5.5 && self.toast_t.target() > 0.5 {
            self.toast_t.set(0.0, now, 220.0);
        }
        let t = self.toast_t.get(now);
        if t < 0.002 && self.toast_t.target() < 0.5 {
            self.toast = None;
            return;
        }
        let (title, sub, has_view) = (toast.title.clone(), toast.sub.clone(), toast.view_session);
        let rect = Rect::from_min_size(Pos2::new(full.right() - 24.0 - 356.0, full.bottom() - 24.0 - 108.0), Vec2::new(356.0, 108.0));
        let mut act = 0;
        layer(ctx, "toast", Order::Foreground, t, rect.center(), 1.0, 12.0, |ui| {
            panel(ui.painter(), rect, 16.0, RAISED);
            let p = ui.painter().clone();
            p.circle_stroke(Pos2::new(rect.left() + 34.0, rect.top() + 34.0), 15.0, Stroke::new(1.6, TEXT));
            icon(&p, ic::CHECK, Pos2::new(rect.left() + 34.0, rect.top() + 34.0), 16.0, TEXT);
            text(&p, Pos2::new(rect.left() + 62.0, rect.top() + 28.0), Align2::LEFT_CENTER, &title, font(15.0, "inter_medium"), TEXT);
            let clip = p.with_clip_rect(Rect::from_min_max(Pos2::new(rect.left() + 62.0, rect.top()), Pos2::new(rect.right() - 46.0, rect.bottom())));
            text(&clip, Pos2::new(rect.left() + 62.0, rect.top() + 52.0), Align2::LEFT_CENTER, &sub, font(11.5, "mono"), TEXT2);
            if icon_button(ui, Id::new("toast_close"), Pos2::new(rect.right() - 26.0, rect.top() + 26.0), ic::X, 16.0, TEXT3).clicked() {
                act = 1;
            }
            if has_view
                && ghost_button(ui, Id::new("toast_view"), Rect::from_min_size(Pos2::new(rect.left() + 62.0, rect.bottom() - 44.0), Vec2::new(128.0, 32.0)), "View session", None).clicked()
            {
                act = 2;
            }
        });
        match act {
            1 => self.toast_t.set(0.0, now, 200.0),
            2 => {
                self.toast_t.set(0.0, now, 200.0);
                self.cc_open = false;
                self.go(Page::Sessions, now);
            }
            _ => {}
        }
    }
}

impl Workspace {
    /// "Connect a device" guide: plays the Blender-rendered CN15 plug-in movie.
    fn guide_ui(&mut self, ctx: &Context, full: Rect, now: f64) {
        self.guide_t.set(if self.guide_open { 1.0 } else { 0.0 }, now, 220.0);
        let t = self.guide_t.get(now);
        if t < 0.002 {
            return;
        }
        // Decode the frame for the current time (held on the last frame; reduce-motion shows it directly).
        let secs = if reduce_motion() { 99.0 } else { (now - self.guide_t0) as f32 };
        let frame = ((secs * crate::onboarding::FPS) as usize).min(crate::onboarding::FRAME_COUNT - 1);
        if self.guide_frame != Some(frame) {
            self.guide_frame = Some(frame);
            if let Ok(img) = crate::onboarding::decode_frame(frame) {
                match &mut self.guide_tex {
                    Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
                    None => self.guide_tex = Some(ctx.load_texture("guide_movie", img, egui::TextureOptions::LINEAR)),
                }
            }
        }
        let (w, h) = (760.0, 620.0);
        let rect = Rect::from_center_size(full.center() + Vec2::new(SIDEBAR_W_F / 2.0, 0.0), Vec2::new(w, h));
        let tex = self.guide_tex.as_ref().map(|t| t.id());
        let mut act = 0;
        layer(ctx, "guide", Order::Foreground, t, rect.center(), 0.97, 0.0, |ui| {
            panel(ui.painter(), rect, 20.0, RAISED);
            text(ui.painter(), Pos2::new(rect.left() + 32.0, rect.top() + 36.0), Align2::LEFT_CENTER, "Connect a device", font(18.0, "inter_medium"), TEXT);
            if icon_button(ui, Id::new("guide_close"), Pos2::new(rect.right() - 34.0, rect.top() + 36.0), ic::X, 19.0, TEXT2).clicked() {
                act = 1;
            }
            let movie = Rect::from_min_size(Pos2::new(rect.left() + 20.0, rect.top() + 72.0), Vec2::new(w - 40.0, (w - 40.0) * 540.0 / 960.0));
            ui.painter().rect_filled(movie, cr(12.0), Color32::BLACK);
            if let Some(id) = tex {
                ui.painter().add(egui::epaint::RectShape::filled(movie, cr(12.0), Color32::WHITE).with_texture(id, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))));
            }
            ui.painter().rect_stroke(movie, cr(12.0), Stroke::new(1.0, border()), egui::StrokeKind::Inside);
            let ty = movie.bottom() + 26.0;
            text(ui.painter(), Pos2::new(rect.left() + 32.0, ty), Align2::LEFT_CENTER, "Connect your computer to CN15 - USB-C OTG / device.", font(14.0, "inter_medium"), TEXT);
            text(ui.painter(), Pos2::new(rect.left() + 32.0, ty + 24.0), Align2::LEFT_CENTER, "The other connectors on the board are inputs (power, ST-LINK). Automatic device detection is not wired in this demo build.", font(12.0, "inter"), TEXT3);
            let by = rect.bottom() - 24.0 - 44.0;
            if ghost_button(ui, Id::new("guide_replay"), Rect::from_min_size(Pos2::new(rect.right() - 32.0 - 108.0 - 12.0 - 108.0, by), Vec2::new(108.0, 44.0)), "Replay", Some(ic::REFRESH_CW)).clicked() {
                act = 2;
            }
            if primary_button(ui, Id::new("guide_done"), Rect::from_min_size(Pos2::new(rect.right() - 32.0 - 108.0, by), Vec2::new(108.0, 44.0)), "Done", None).clicked() {
                act = 1;
            }
        });
        match act {
            1 => self.guide_open = false,
            2 => {
                self.guide_t0 = now;
                self.guide_frame = None;
            }
            _ => {}
        }
    }
}

impl Workspace {
    /// "Connect a device" over SSH: host / user / optional key file.
    fn connect_ui(&mut self, ctx: &Context, full: Rect, now: f64) {
        self.connect_t.set(if self.connect_open { 1.0 } else { 0.0 }, now, 200.0);
        let t = self.connect_t.get(now);
        if t < 0.002 {
            return;
        }
        let (w, h) = (500.0, 392.0);
        let rect = Rect::from_center_size(full.center() + Vec2::new(SIDEBAR_W_F / 2.0, 0.0), Vec2::new(w, h));
        if self.connect_open && !self.touched.contains("conn_focused") {
            ctx.memory_mut(|m| m.request_focus(Id::new("conn_host")));
            self.touched.insert("conn_focused");
        }
        let mut host = std::mem::take(&mut self.conn_host);
        let mut user = std::mem::take(&mut self.conn_user);
        let mut key = std::mem::take(&mut self.conn_key);
        let error = self.conn_error.clone();
        let mut act = 0;
        layer(ctx, "connect", Order::Foreground, t, rect.center(), 0.97, 0.0, |ui| {
            panel(ui.painter(), rect, 20.0, RAISED);
            text(ui.painter(), Pos2::new(rect.left() + 32.0, rect.top() + 38.0), Align2::LEFT_CENTER, "Connect over SSH", font(18.0, "inter_medium"), TEXT);
            if icon_button(ui, Id::new("conn_close"), Pos2::new(rect.right() - 34.0, rect.top() + 38.0), ic::X, 19.0, TEXT2).clicked() {
                act = 1;
            }
            text(ui.painter(), Pos2::new(rect.left() + 32.0, rect.top() + 72.0), Align2::LEFT_CENTER, "Live logs (journalctl) and CPU/RAM. Key-based login only.", font(12.5, "inter"), TEXT3);
            let mut y = rect.top() + 100.0;
            for (label, value, id, hint, pw) in [
                ("Host", &mut host, "conn_host", "192.168.7.1", false),
                ("User", &mut user, "conn_user", "root", false),
                ("Key file (optional)", &mut key, "conn_key", "~/.ssh/id_ed25519", false),
            ] {
                let _ = pw;
                text(ui.painter(), Pos2::new(rect.left() + 32.0, y + 6.0), Align2::LEFT_CENTER, label, font(12.0, "inter_medium"), TEXT2);
                let field = Rect::from_min_size(Pos2::new(rect.left() + 32.0, y + 20.0), Vec2::new(w - 64.0, 44.0));
                ui.painter().rect_filled(field, cr(10.0), Color32::from_rgb(0x12, 0x14, 0x15));
                ui.painter().rect_stroke(field, cr(10.0), Stroke::new(1.0, border()), egui::StrokeKind::Inside);
                let mut fui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(field.shrink2(Vec2::new(14.0, 4.0)))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                fui.visuals_mut().override_text_color = Some(TEXT);
                fui.add(
                    egui::TextEdit::singleline(value)
                        .id(Id::new(id))
                        .hint_text(hint)
                        .frame(egui::Frame::NONE)
                        .vertical_align(egui::Align::Center)
                        .font(font(14.0, "inter"))
                        .desired_width(w - 100.0),
                );
                y += 74.0;
            }
            if let Some(e) = &error {
                text(ui.painter(), Pos2::new(rect.left() + 32.0, y + 2.0), Align2::LEFT_CENTER, e, font(12.5, "inter"), TEXT);
            }
            let by = rect.bottom() - 24.0 - 44.0;
            if ghost_button(ui, Id::new("conn_cancel"), Rect::from_min_size(Pos2::new(rect.right() - 32.0 - 116.0 - 12.0 - 116.0, by), Vec2::new(116.0, 44.0)), "Cancel", None).clicked() {
                act = 1;
            }
            if primary_button(ui, Id::new("conn_go"), Rect::from_min_size(Pos2::new(rect.right() - 32.0 - 116.0, by), Vec2::new(116.0, 44.0)), "Connect", None).clicked()
                || (ui.input(|i| i.key_pressed(egui::Key::Enter)) && ui.memory(|m| m.focused().is_some()))
            {
                act = 2;
            }
        });
        self.conn_host = host;
        self.conn_user = user;
        self.conn_key = key;
        match act {
            1 => self.connect_open = false,
            2 => {
                let (h, u) = (self.conn_host.trim().to_string(), self.conn_user.trim().to_string());
                if h.is_empty() || u.is_empty() {
                    self.conn_error = Some("Enter a host and a user.".to_string());
                } else {
                    self.disconnect_live();
                    let key = (!self.conn_key.trim().is_empty()).then(|| self.conn_key.trim().to_string());
                    self.live = Some(super::live::Live::connect(h.clone(), u.clone(), key));
                    self.replay = None;
                    self.connect_open = false;
                    self.capture_on = true;
                    self.paused = false;
                    self.go(Page::Monitor, now);
                    self.show_toast("Connecting", &format!("{u}@{h}"), false, now);
                }
            }
            _ => {}
        }
    }
}

const SIDEBAR_W_F: f32 = super::SIDEBAR_W;
