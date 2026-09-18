//! The seven pages plus the diagnostics side panel.

use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::demo::{self, Level};
use super::theme::*;
use super::widgets::*;
use super::{Page, Workspace, DIAG_W};
use crate::icons as ic;

fn heading(p: &egui::Painter, r: Rect, title: &str, sub: &str) -> f32 {
    text(p, Pos2::new(r.left(), r.top() + 2.0), Align2::LEFT_TOP, title, font(30.0, "inter_light"), TEXT);
    text(p, Pos2::new(r.left(), r.top() + 46.0), Align2::LEFT_TOP, sub, font(14.0, "inter"), TEXT2);
    r.top() + 90.0
}

/// Monochrome line-art of a development board (own drawing, no vendor photo).
fn draw_board(p: &egui::Painter, r: Rect) {
    let line = Stroke::new(1.3, Color32::from_white_alpha(150));
    let soft = Stroke::new(1.0, Color32::from_white_alpha(70));
    let pcb = Rect::from_center_size(r.center(), Vec2::new(r.width() * 0.92, r.height() * 0.86));
    p.rect_filled(pcb, cr(9.0), Color32::from_white_alpha(10));
    p.rect_stroke(pcb, cr(9.0), line, StrokeKind::Inside);
    for c in [pcb.left_top(), pcb.right_top(), pcb.left_bottom(), pcb.right_bottom()] {
        let d = Vec2::new(if c.x < pcb.center().x { 9.0 } else { -9.0 }, if c.y < pcb.center().y { 9.0 } else { -9.0 });
        p.circle_stroke(c + d, 3.0, soft);
    }
    // SoC + package rings.
    let soc = Rect::from_center_size(pcb.center() + Vec2::new(-6.0, -2.0), Vec2::new(46.0, 46.0));
    p.rect_stroke(soc.expand(6.0), cr(4.0), soft, StrokeKind::Inside);
    p.rect_filled(soc, cr(4.0), Color32::from_white_alpha(28));
    p.rect_stroke(soc, cr(4.0), line, StrokeKind::Inside);
    p.circle_stroke(soc.left_top() + Vec2::new(8.0, 8.0), 2.5, soft);
    // Memory chips.
    for k in 0..2 {
        let m = Rect::from_min_size(pcb.left_top() + Vec2::new(12.0 + k as f32 * 26.0, 14.0), Vec2::new(20.0, 14.0));
        p.rect_stroke(m, cr(2.0), soft, StrokeKind::Inside);
    }
    // USB-C + header + display connector.
    let usb = Rect::from_min_size(pcb.right_top() + Vec2::new(-30.0, 22.0), Vec2::new(30.0, 16.0));
    p.rect_stroke(usb, cr(4.0), line, StrokeKind::Inside);
    let eth = Rect::from_min_size(pcb.right_top() + Vec2::new(-34.0, 50.0), Vec2::new(34.0, 26.0));
    p.rect_stroke(eth, cr(3.0), line, StrokeKind::Inside);
    for i in 0..10 {
        let x = pcb.left() + 16.0 + i as f32 * 8.0;
        p.rect_stroke(Rect::from_min_size(Pos2::new(x, pcb.bottom() - 16.0), Vec2::new(4.0, 6.0)), 0.0, soft, StrokeKind::Inside);
    }
    p.line_segment([soc.right_center() + Vec2::new(6.0, 0.0), usb.left_center()], soft);
    p.line_segment([soc.center_bottom() + Vec2::new(0.0, 6.0), Pos2::new(soc.center().x, pcb.bottom() - 18.0)], soft);
}

fn card_bg(ui: &mut Ui, id: Id, rect: Rect) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let h = hv(ui, id.with("h"), resp.hovered(), 120.0);
    let p = ui.painter();
    p.add(egui::epaint::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(90) }.as_shape(rect, cr(14.0)));
    p.rect_filled(rect, cr(14.0), mix(PANEL, RAISED, h));
    p.rect_stroke(rect, cr(14.0), Stroke::new(1.0, Color32::from_white_alpha(32 + (h * 26.0) as u8)), StrokeKind::Inside);
    p.line_segment(
        [Pos2::new(rect.left() + 14.0, rect.top() + 1.0), Pos2::new(rect.right() - 14.0, rect.top() + 1.0)],
        Stroke::new(1.0, Color32::from_white_alpha(24)),
    );
    if resp.has_focus() {
        focus_ring(p, rect, 14.0);
    }
    resp
}

impl Workspace {
    // ------------------------------------------------------------- Home ---

    pub(super) fn home_page(&mut self, ui: &mut Ui, r: Rect, now: f64) {
        use chrono::Timelike;
        let p = ui.painter().clone();
        let local = chrono::Local::now();
        let greet = match local.hour() {
            5..=11 => "Good morning.",
            12..=17 => "Good afternoon.",
            _ => "Good evening.",
        };
        text(&p, Pos2::new(r.left(), r.top() + 4.0), Align2::LEFT_TOP, greet, font(46.0, "inter_light"), TEXT);
        text(&p, Pos2::new(r.left(), r.top() + 66.0), Align2::LEFT_TOP, local.format("%A, %B %-d").to_string(), font(15.0, "inter"), TEXT2);

        // Device panel.
        let dp = Rect::from_min_size(Pos2::new(r.left(), r.top() + 118.0), Vec2::new(r.width(), 172.0));
        panel(&p, dp, 16.0, PANEL);
        draw_board(&p, Rect::from_min_size(dp.min + Vec2::new(28.0, 24.0), Vec2::new(176.0, 124.0)));
        let tx = dp.left() + 236.0;
        text(&p, Pos2::new(tx, dp.top() + 38.0), Align2::LEFT_CENTER, "yocto-devkit", font(24.0, "inter_medium"), TEXT);
        text(&p, Pos2::new(tx, dp.top() + 70.0), Align2::LEFT_CENTER, "STM32MP257F-DK", font(14.0, "inter"), TEXT2);
        let pill = Rect::from_min_size(Pos2::new(tx, dp.top() + 96.0), Vec2::new(170.0, 30.0));
        p.rect_stroke(pill, cr(15.0), Stroke::new(1.0, Color32::from_white_alpha(50)), StrokeKind::Inside);
        p.circle_filled(Pos2::new(pill.left() + 16.0, pill.center().y), 4.0, TEXT);
        text(&p, Pos2::new(pill.left() + 30.0, pill.center().y), Align2::LEFT_CENTER, "Connected via USB", font(13.0, "inter"), TEXT2);
        let br = Rect::from_center_size(Pos2::new(dp.right() - 28.0 - 90.0, dp.center().y), Vec2::new(180.0, 50.0));
        if primary_button(ui, Id::new("start_session"), br, "Start session", Some(ic::ARROW_RIGHT)).clicked() {
            self.replay = None;
            self.start_capture(now);
            self.go(Page::Monitor, now);
        }

        // Quick actions.
        let gap = 16.0;
        let cw = (r.width() - gap * 2.0) / 3.0;
        let y = dp.bottom() + 20.0;
        let items = [
            (ic::TERMINAL, "Capture logs", "Save system logs", Page::Monitor),
            (ic::STETHOSCOPE, "Run diagnostics", "Check system health", Page::Diagnostics),
            (ic::FOLDER, "Browse files", "Explore device storage", Page::Files),
        ];
        for (i, (glyph, title, sub, target)) in items.iter().enumerate() {
            let cr_ = Rect::from_min_size(Pos2::new(r.left() + i as f32 * (cw + gap), y), Vec2::new(cw, 88.0));
            let resp = card_bg(ui, Id::new(("qa", i)), cr_);
            let p = ui.painter();
            p.circle_filled(Pos2::new(cr_.left() + 42.0, cr_.center().y), 22.0, Color32::from_white_alpha(14));
            icon(p, *glyph, Pos2::new(cr_.left() + 42.0, cr_.center().y), 20.0, TEXT);
            text(p, Pos2::new(cr_.left() + 78.0, cr_.center().y - 10.0), Align2::LEFT_CENTER, *title, font(15.0, "inter_medium"), TEXT);
            text(p, Pos2::new(cr_.left() + 78.0, cr_.center().y + 11.0), Align2::LEFT_CENTER, *sub, font(13.0, "inter"), TEXT2);
            icon(p, ic::CHEVRON_RIGHT, Pos2::new(cr_.right() - 26.0, cr_.center().y), 18.0, TEXT3);
            if resp.clicked() {
                if i == 0 {
                    self.replay = None;
                    self.start_capture(now);
                }
                self.go(*target, now);
            }
        }
    }

    // ---------------------------------------------------------- Devices ---

    pub(super) fn devices_page(&mut self, ui: &mut Ui, r: Rect, now: f64) {
        let p = ui.painter().clone();
        let y0 = heading(&p, r, "Devices", "Connected boards and connection details");
        let main = Rect::from_min_size(Pos2::new(r.left(), y0), Vec2::new(r.width(), 232.0));
        panel(&p, main, 16.0, PANEL);
        draw_board(&p, Rect::from_min_size(main.min + Vec2::new(28.0, 28.0), Vec2::new(150.0, 106.0)));
        let tx = main.left() + 210.0;
        text(&p, Pos2::new(tx, main.top() + 40.0), Align2::LEFT_CENTER, "STM32MP257F-DK", font(20.0, "inter_medium"), TEXT);
        text(&p, Pos2::new(tx, main.top() + 68.0), Align2::LEFT_CENTER, "yocto-devkit  ·  Connected via USB", font(13.5, "inter"), TEXT2);
        let rows = [("Port", "/dev/ttyACM0 (demo)"), ("Address", "192.168.7.1 (demo)"), ("Image", "st-image-weston (demo)"), ("Kernel", "6.6 yocto-standard (demo)")];
        for (i, (k, v)) in rows.iter().enumerate() {
            let y = main.top() + 106.0 + i as f32 * 26.0;
            text(&p, Pos2::new(tx, y), Align2::LEFT_CENTER, *k, font(13.0, "inter"), TEXT3);
            text(&p, Pos2::new(tx + 96.0, y), Align2::LEFT_CENTER, *v, font(13.0, "inter"), TEXT);
        }
        let bx = main.right() - 24.0 - 160.0;
        if ghost_button(ui, Id::new("dev_mon"), Rect::from_min_size(Pos2::new(bx, main.top() + 28.0), Vec2::new(160.0, 44.0)), "Live monitor", Some(ic::ACTIVITY)).clicked() {
            self.go(Page::Monitor, now);
        }
        if ghost_button(ui, Id::new("dev_files"), Rect::from_min_size(Pos2::new(bx, main.top() + 80.0), Vec2::new(160.0, 44.0)), "Browse files", Some(ic::FOLDER)).clicked() {
            self.go(Page::Files, now);
        }

        let second = Rect::from_min_size(Pos2::new(r.left(), main.bottom() + 16.0), Vec2::new(r.width(), 92.0));
        panel(&p, second, 16.0, PANEL);
        icon(&p, ic::SERVER, Pos2::new(second.left() + 42.0, second.center().y), 26.0, TEXT3);
        text(&p, Pos2::new(second.left() + 76.0, second.center().y - 11.0), Align2::LEFT_CENTER, "Custom Board", font(16.0, "inter_medium"), TEXT);
        text(&p, Pos2::new(second.left() + 76.0, second.center().y + 12.0), Align2::LEFT_CENTER, "Not connected", font(13.0, "inter"), TEXT2);
        if ghost_button(ui, Id::new("dev_connect"), Rect::from_center_size(Pos2::new(second.right() - 24.0 - 60.0, second.center().y), Vec2::new(120.0, 44.0)), "Connect", None).clicked() {
            self.show_toast("Custom Board isn't reachable", "Real device backend not available in demo mode", false, now);
        }
        if ghost_button(ui, Id::new("dev_add"), Rect::from_min_size(Pos2::new(r.left(), second.bottom() + 16.0), Vec2::new(160.0, 44.0)), "Add device", Some(ic::PLUS)).clicked() {
            self.open_guide(now);
        }
    }

    // ---------------------------------------------------------- Monitor ---

    pub(super) fn monitor_page(&mut self, ui: &mut Ui, r: Rect, now: f64) {
        let phase = self.demo.phase(now);
        let p = ui.painter().clone();
        let gap = 16.0;
        let mw = (r.width() - gap) / 2.0;
        let mh = 100.0;

        // CPU.
        let cpu = Rect::from_min_size(r.min, Vec2::new(mw, mh));
        panel(&p, cpu, 14.0, PANEL);
        text(&p, Pos2::new(cpu.left() + 22.0, cpu.top() + 26.0), Align2::LEFT_CENTER, "CPU", font(13.0, "inter"), TEXT2);
        text(&p, Pos2::new(cpu.left() + 22.0, cpu.top() + 62.0), Align2::LEFT_CENTER, format!("{:.0}%", self.demo.cpu), font(30.0, "inter_light"), TEXT);
        // Bars scroll continuously between samples (drawn one sample behind, so the
        // newest bar slides in from the right instead of popping).
        let (bar_w, bar_gap) = (5.0, 3.0);
        let step = bar_w + bar_gap;
        let fit = (((cpu.width() - 22.0 - 118.0 - 22.0) / step).floor() as usize).clamp(6, 28);
        let hist: Vec<f32> = self.demo.cpu_hist.iter().rev().take(fit + 1).rev().copied().collect();
        let n = hist.len();
        let bx1 = cpu.right() - 22.0;
        let bars_area = Rect::from_min_max(Pos2::new(bx1 - fit as f32 * step + bar_gap, cpu.top() + 8.0), Pos2::new(bx1 + 1.0, cpu.bottom() - 8.0));
        let bp = p.with_clip_rect(bars_area);
        for (i, v) in hist.iter().enumerate() {
            let x = bx1 - bar_w - (n - 1 - i) as f32 * step + (1.0 - phase) * step;
            let h = (v / 100.0 * 56.0).clamp(3.0, 56.0);
            let last = i + 1 == n;
            bp.rect_filled(
                Rect::from_min_max(Pos2::new(x, cpu.bottom() - 20.0 - h), Pos2::new(x + bar_w, cpu.bottom() - 20.0)),
                cr(1.5),
                if last { TEXT } else { Color32::from_white_alpha(70) },
            );
        }

        // Memory.
        let mem = Rect::from_min_size(Pos2::new(cpu.right() + gap, r.top()), Vec2::new(mw, mh));
        panel(&p, mem, 14.0, PANEL);
        let frac = self.demo.mem_used_mb / self.demo.mem_total_mb;
        text(&p, Pos2::new(mem.left() + 22.0, mem.top() + 26.0), Align2::LEFT_CENTER, "Memory", font(13.0, "inter"), TEXT2);
        text(&p, Pos2::new(mem.right() - 22.0, mem.top() + 26.0), Align2::RIGHT_CENTER, format!("{:.0}% used", frac * 100.0), font(13.0, "inter"), TEXT2);
        text(
            &p,
            Pos2::new(mem.left() + 22.0, mem.top() + 56.0),
            Align2::LEFT_CENTER,
            format!("{:.1} / {:.1} GB", self.demo.mem_used_mb / 1024.0, self.demo.mem_total_mb / 1024.0),
            font(26.0, "inter_light"),
            TEXT,
        );
        segments(&p, Rect::from_min_size(Pos2::new(mem.left() + 22.0, mem.bottom() - 20.0), Vec2::new(mem.width() - 44.0, 6.0)), 24, frac);

        // Log window.
        let bottom_h = 52.0;
        let logr = Rect::from_min_max(Pos2::new(r.left(), cpu.bottom() + gap), Pos2::new(r.right(), r.bottom() - bottom_h - 12.0));
        panel(&p, logr, 14.0, Color32::from_rgb(0x0E, 0x10, 0x11));
        let mut inner = logr.shrink2(Vec2::new(6.0, 8.0));
        if let Some(rep) = &self.replay {
            let banner = Rect::from_min_size(inner.min, Vec2::new(inner.width(), 44.0));
            p.rect_filled(banner, cr(10.0), Color32::from_white_alpha(12));
            icon(&p, ic::HISTORY, Pos2::new(banner.left() + 24.0, banner.center().y), 17.0, TEXT);
            text(&p, Pos2::new(banner.left() + 46.0, banner.center().y), Align2::LEFT_CENTER, format!("Replaying session: {}", rep.name), font(13.5, "inter_medium"), TEXT);
            if ghost_button(ui, Id::new("back_live"), Rect::from_center_size(Pos2::new(banner.right() - 76.0, banner.center().y), Vec2::new(128.0, 36.0)), "Back to live", None).clicked() {
                self.replay = None;
                self.selected_log = None;
            }
            inner.min.y += 52.0;
        }
        let row_h = 28.0;
        let (sa, sb): (&[demo::LogLine], &[demo::LogLine]) = match &self.replay {
            Some(rep) => (&rep.lines[..], &[]),
            None => self.demo.lines.as_slices(),
        };
        let total = sa.len() + sb.len();
        let selected = self.selected_log;
        let mut clicked: Option<u64> = None;
        // Jump-to-line (from Inspect): stop following the tail so the target stays in view.
        let mut jump_offset: Option<f32> = None;
        if let Some(id) = self.scroll_to_log.take() {
            let idx = match &self.replay {
                Some(_) => sa.iter().position(|l| l.id == id),
                None => self.demo.index_of(id),
            };
            if let Some(i) = idx {
                jump_offset = Some((i as f32 * row_h - inner.height() / 2.0).max(0.0));
                self.pin_scroll = true;
            }
        }
        let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(!self.pin_scroll);
        if let Some(off) = jump_offset {
            area = area.vertical_scroll_offset(off);
        }
        let jumped = jump_offset.is_some();
        let mut lui = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        lui.set_clip_rect(inner);
        lui.spacing_mut().item_spacing.y = 0.0;
        let scroll_out = area.show_rows(&mut lui, row_h, total, |ui, range| {
            for i in range {
                let l = if i < sa.len() { &sa[i] } else { &sb[i - sa.len()] };
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
                let hover = resp.hovered();
                let sel = selected == Some(l.id);
                let pt = ui.painter();
                if sel {
                    pt.rect_filled(rect, cr(7.0), SELECTED);
                } else if hover {
                    pt.rect_filled(rect, cr(7.0), Color32::from_white_alpha(9));
                }
                let cy = rect.center().y;
                let err = l.level == Level::Error;
                let warn = l.level == Level::Warn;
                if err {
                    icon(pt, ic::CIRCLE_ALERT, Pos2::new(rect.left() + 18.0, cy), 14.0, TEXT);
                } else if warn {
                    icon(pt, ic::TRIANGLE_ALERT, Pos2::new(rect.left() + 18.0, cy), 13.0, TEXT2);
                }
                let mono = font(12.5, "mono");
                text(pt, Pos2::new(rect.left() + 38.0, cy), Align2::LEFT_CENTER, demo::fmt_ts(l.ms), mono.clone(), if sel || err { TEXT2 } else { TEXT3 });
                let clip = pt.with_clip_rect(rect);
                text(&clip, Pos2::new(rect.left() + 168.0, cy), Align2::LEFT_CENTER, ellipsize_mono(l.proc_, 12.5, 188.0), mono.clone(), if err || sel { TEXT } else { TEXT2 });
                let mx = rect.left() + 168.0 + 196.0;
                let msg = ellipsize_mono(&l.msg, 12.5, rect.right() - mx - 14.0);
                text(&clip, Pos2::new(mx, cy), Align2::LEFT_CENTER, msg, mono, if err || sel { TEXT } else { TEXT2 });
                if resp.clicked() {
                    clicked = Some(l.id);
                }
            }
        });
        if clicked.is_some() {
            self.selected_log = clicked;
        }
        if !jumped && self.pin_scroll && scroll_out.state.offset.y + scroll_out.inner_rect.height() >= scroll_out.content_size.y - 4.0 {
            // The user scrolled back to the end: resume following new lines.
            self.pin_scroll = false;
        }

        // Bottom bar.
        let by = r.bottom() - bottom_h + 4.0;
        let (lbl, glyph) = if self.paused { ("Resume capture", ic::PLAY) } else { ("Pause capture", ic::PAUSE) };
        if ghost_button(ui, Id::new("pause_cap"), Rect::from_min_size(Pos2::new(r.left(), by), Vec2::new(176.0, 44.0)), lbl, Some(glyph)).clicked() {
            self.paused = !self.paused;
        }
        let clear = Rect::from_min_size(Pos2::new(r.right() - 96.0, by), Vec2::new(96.0, 44.0));
        if ghost_button(ui, Id::new("clear_log"), clear, "Clear", Some(ic::X)).clicked() && self.replay.is_none() {
            self.demo.clear();
            self.selected_log = None;
            self.pin_scroll = false;
        }
        text(ui.painter(), Pos2::new(clear.left() - 18.0, by + 22.0), Align2::RIGHT_CENTER, format!("{} lines", group(total)), font(13.0, "inter"), TEXT2);
        if !self.capture_on && self.replay.is_none() {
            text(ui.painter(), Pos2::new(r.left() + 196.0, by + 22.0), Align2::LEFT_CENTER, "Log capture is off - turn it on in Control Center", font(13.0, "inter"), TEXT3);
        } else if self.paused {
            text(ui.painter(), Pos2::new(r.left() + 196.0, by + 22.0), Align2::LEFT_CENTER, "Paused - incoming lines are not being recorded", font(13.0, "inter"), TEXT3);
        }
    }

    // ------------------------------------------------ Diagnostics panel ---

    pub(super) fn diag_panel(&mut self, ui: &mut Ui, rect: Rect, now: f64) {
        let p = ui.painter().with_clip_rect(rect);
        let x0 = rect.right() - DIAG_W;
        p.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(0x14, 0x17, 0x18, 245));
        p.line_segment([Pos2::new(rect.left() + 0.5, rect.top()), Pos2::new(rect.left() + 0.5, rect.bottom())], Stroke::new(1.0, border()));
        text(&p, Pos2::new(x0 + 24.0, rect.top() + 34.0), Align2::LEFT_CENTER, "Diagnostics", font(16.0, "inter_medium"), TEXT);
        if icon_button(ui, Id::new("diag_close"), Pos2::new(x0 + DIAG_W - 34.0, rect.top() + 34.0), ic::X, 18.0, TEXT2).clicked() {
            self.diag_open = false;
        }
        let cx = x0 + DIAG_W / 2.0;
        let n = self.demo.issues.len();
        icon(&p, ic::CIRCLE_ALERT, Pos2::new(cx, rect.top() + 106.0), 52.0, TEXT);
        text(&p, Pos2::new(cx, rect.top() + 162.0), Align2::CENTER_CENTER, format!("{n} issues detected"), font(20.0, "inter_medium"), TEXT);
        text(&p, Pos2::new(cx, rect.top() + 188.0), Align2::CENTER_CENTER, "System requires attention", font(13.5, "inter"), TEXT2);

        let mut y = rect.top() + 218.0;
        for i in 0..n {
            let row = Rect::from_min_size(Pos2::new(x0 + 16.0, y), Vec2::new(DIAG_W - 32.0, 60.0));
            let resp = ui.interact(row, Id::new(("issue", i)), Sense::click());
            let h = hv(ui, Id::new(("issue_h", i)), resp.hovered(), 120.0);
            let sel = i == self.sel_issue;
            if sel {
                p.rect_filled(row, cr(12.0), SELECTED);
            } else if h > 0.0 {
                p.rect_filled(row, cr(12.0), Color32::from_white_alpha((h * 12.0) as u8));
            }
            let is = &self.demo.issues[i];
            icon(&p, ic::TRIANGLE_ALERT, Pos2::new(row.left() + 24.0, row.center().y), 17.0, if sel { TEXT } else { TEXT2 });
            text(&p, Pos2::new(row.left() + 48.0, row.center().y - 10.0), Align2::LEFT_CENTER, is.title, font(14.0, "inter_medium"), TEXT);
            text(&p, Pos2::new(row.left() + 48.0, row.center().y + 11.0), Align2::LEFT_CENTER, if i == 0 { is.service } else { is.summary }, font(12.5, "inter"), TEXT2);
            if resp.has_focus() {
                focus_ring(&p, row, 12.0);
            }
            if resp.clicked() {
                self.sel_issue = i;
            }
            y += 64.0;
        }
        if n == 0 {
            return;
        }
        let is = &self.demo.issues[self.sel_issue.min(n - 1)];
        y += 10.0;
        p.line_segment([Pos2::new(x0 + 24.0, y), Pos2::new(x0 + DIAG_W - 24.0, y)], Stroke::new(1.0, hairline()));
        y += 16.0;
        for (k, v) in [("Name", is.title.to_string()), ("Service", is.service.to_string()), ("Time", demo::fmt_ts(is.ms))] {
            text(&p, Pos2::new(x0 + 24.0, y + 8.0), Align2::LEFT_CENTER, k, font(12.5, "inter"), TEXT3);
            text(&p, Pos2::new(x0 + 96.0, y + 8.0), Align2::LEFT_CENTER, v, font(13.0, "inter"), TEXT);
            y += 24.0;
        }
        y += 8.0;
        text(&p, Pos2::new(x0 + 24.0, y + 6.0), Align2::LEFT_CENTER, "OBSERVED IN LOG", font(10.5, "inter_semibold"), TEXT3);
        y += 20.0;
        let g = p.layout(is.observed.clone(), font(12.0, "mono"), TEXT, DIAG_W - 48.0 - 24.0);
        let bh = g.size().y + 20.0;
        let box_ = Rect::from_min_size(Pos2::new(x0 + 24.0, y), Vec2::new(DIAG_W - 48.0, bh));
        p.rect_filled(box_, cr(10.0), Color32::from_white_alpha(12));
        p.galley(box_.min + Vec2::new(12.0, 10.0), g, TEXT);
        y += bh + 14.0;
        text(&p, Pos2::new(x0 + 24.0, y + 6.0), Align2::LEFT_CENTER, "PROBABLE CAUSE  ·  inferred", font(10.5, "inter_semibold"), TEXT3);
        y += 20.0;
        let g = p.layout(is.probable_cause.to_string(), font(13.0, "inter"), TEXT2, DIAG_W - 48.0);
        p.galley(Pos2::new(x0 + 24.0, y), g, TEXT2);

        let log_id = is.log_id;
        let btn = Rect::from_min_size(Pos2::new(x0 + 24.0, rect.bottom() - 24.0 - 50.0), Vec2::new(DIAG_W - 48.0, 50.0));
        if primary_button(ui, Id::new("inspect"), btn, "Inspect", None).clicked() {
            self.replay = None;
            self.selected_log = Some(log_id);
            self.scroll_to_log = Some(log_id);
        }
        let _ = now;
    }

    // ------------------------------------------------------ Diagnostics ---

    pub(super) fn diagnostics_page(&mut self, ui: &mut Ui, r: Rect, now: f64) {
        let p = ui.painter().clone();
        let y0 = heading(&p, r, "Diagnostics", "Health checks and detected issues");
        let n = self.demo.issues.len();
        let sum = Rect::from_min_size(Pos2::new(r.left(), y0), Vec2::new(r.width(), 96.0));
        panel(&p, sum, 16.0, PANEL);
        icon(&p, ic::CIRCLE_ALERT, Pos2::new(sum.left() + 48.0, sum.center().y), 34.0, TEXT);
        text(&p, Pos2::new(sum.left() + 90.0, sum.center().y - 11.0), Align2::LEFT_CENTER, format!("{n} issues detected"), font(20.0, "inter_medium"), TEXT);
        text(&p, Pos2::new(sum.left() + 90.0, sum.center().y + 14.0), Align2::LEFT_CENTER, "System requires attention", font(13.5, "inter"), TEXT2);
        let busy = self.recheck_at.is_some();
        if let Some(t0) = self.recheck_at {
            let f = ((now - t0) / 1.8).clamp(0.0, 1.0) as f32;
            progress(&p, Rect::from_min_size(Pos2::new(sum.right() - 24.0 - 260.0 - 152.0, sum.center().y - 3.0), Vec2::new(120.0, 6.0)), f);
            if f >= 1.0 {
                self.recheck_at = None;
                self.show_toast("Checks finished", "Demo device still reports 2 issues", false, now);
            }
        }
        if ghost_button(ui, Id::new("recheck"), Rect::from_center_size(Pos2::new(sum.right() - 24.0 - 64.0, sum.center().y), Vec2::new(128.0, 44.0)), if busy { "Checking..." } else { "Re-run" }, Some(ic::REFRESH_CW)).clicked() && !busy {
            self.recheck_at = Some(now);
        }
        let mut y = sum.bottom() + 16.0;
        let mut inspect = None;
        for i in 0..n {
            let is = &self.demo.issues[i];
            let card = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), 172.0));
            panel(&p, card, 16.0, PANEL);
            icon(&p, ic::TRIANGLE_ALERT, Pos2::new(card.left() + 34.0, card.top() + 36.0), 20.0, TEXT);
            text(&p, Pos2::new(card.left() + 62.0, card.top() + 36.0), Align2::LEFT_CENTER, is.title, font(16.0, "inter_medium"), TEXT);
            text(&p, Pos2::new(card.left() + 62.0, card.top() + 58.0), Align2::LEFT_CENTER, format!("{}  ·  {}", is.service, demo::fmt_ts(is.ms)), font(12.5, "inter"), TEXT3);
            text(&p, Pos2::new(card.left() + 34.0, card.top() + 88.0), Align2::LEFT_CENTER, "OBSERVED", font(10.5, "inter_semibold"), TEXT3);
            text(&p, Pos2::new(card.left() + 120.0, card.top() + 88.0), Align2::LEFT_CENTER, &is.observed, font(12.5, "mono"), TEXT);
            text(&p, Pos2::new(card.left() + 34.0, card.top() + 116.0), Align2::LEFT_CENTER, "PROBABLE CAUSE", font(10.5, "inter_semibold"), TEXT3);
            let g = p.layout(is.probable_cause.to_string(), font(12.5, "inter"), TEXT2, card.width() - 152.0 - 40.0);
            p.galley(Pos2::new(card.left() + 152.0, card.top() + 106.0), g, TEXT2);
            if primary_button(ui, Id::new(("inspect_page", i)), Rect::from_center_size(Pos2::new(card.right() - 24.0 - 60.0, card.top() + 44.0), Vec2::new(120.0, 44.0)), "Inspect", None).clicked() {
                inspect = Some(i);
            }
            y += 188.0;
        }
        if let Some(i) = inspect {
            self.sel_issue = i;
            self.replay = None;
            self.selected_log = Some(self.demo.issues[i].log_id);
            self.scroll_to_log = self.selected_log;
            self.diag_open = true;
            self.go(Page::Monitor, now);
        }
    }

    // ------------------------------------------------------- Sessions ----

    pub(super) fn sessions_page(&mut self, ui: &mut Ui, r: Rect, now: f64) {
        let p = ui.painter().clone();
        let mut y = heading(&p, r, "Sessions", "Earlier captures from this workspace");
        let mut open = None;
        for (i, s) in self.sessions.iter().enumerate() {
            if y + 76.0 > r.bottom() + 20.0 {
                break;
            }
            let row = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), 72.0));
            let resp = card_bg(ui, Id::new(("sess", i)), row);
            let pt = ui.painter();
            icon(pt, ic::HISTORY, Pos2::new(row.left() + 36.0, row.center().y), 20.0, TEXT2);
            text(pt, Pos2::new(row.left() + 66.0, row.center().y - 10.0), Align2::LEFT_CENTER, &s.name, font(15.0, "inter_medium"), TEXT);
            text(pt, Pos2::new(row.left() + 66.0, row.center().y + 12.0), Align2::LEFT_CENTER, &s.when, font(12.5, "inter"), TEXT3);
            let cx = row.left() + row.width() * 0.52;
            text(pt, Pos2::new(cx, row.center().y), Align2::LEFT_CENTER, demo::fmt_dur(s.duration_s), font(13.5, "inter"), TEXT2);
            let ex = row.left() + row.width() * 0.68;
            if s.errors > 0 {
                icon(pt, ic::TRIANGLE_ALERT, Pos2::new(ex, row.center().y), 15.0, TEXT);
                text(pt, Pos2::new(ex + 16.0, row.center().y), Align2::LEFT_CENTER, if s.errors == 1 { "1 error".to_string() } else { format!("{} errors", s.errors) }, font(13.5, "inter_medium"), TEXT);
            } else {
                text(pt, Pos2::new(ex, row.center().y), Align2::LEFT_CENTER, "No errors", font(13.5, "inter"), TEXT3);
            }
            let ob = Rect::from_center_size(Pos2::new(row.right() - 24.0 - 48.0, row.center().y), Vec2::new(96.0, 44.0));
            let open_clicked = ghost_button(ui, Id::new(("sess_open", i)), ob, "Open", None).clicked();
            if open_clicked || resp.double_clicked() {
                open = Some(i);
            }
            y += 84.0;
        }
        if let Some(i) = open {
            let s = &self.sessions[i];
            let lines = demo::Demo::session_lines(s.seed, s.lines.min(1500), s.errors);
            self.replay = Some(super::Replay { name: s.name.clone(), lines });
            self.selected_log = None;
            self.go(Page::Monitor, now);
        }
    }

    // ------------------------------------------------------- Settings ----

    pub(super) fn settings_page(&mut self, ui: &mut Ui, r: Rect, now: f64) {
        let p = ui.painter().clone();
        let y = heading(&p, r, "Settings", "Appearance, connections, storage and system");
        let half = (r.width() - 16.0) / 2.0;
        let mk = |x: f32, y: f32, h: f32| Rect::from_min_size(Pos2::new(x, y), Vec2::new(half, h));

        // Appearance.
        let a = mk(r.left(), y, 164.0);
        panel(&p, a, 16.0, PANEL);
        text(&p, Pos2::new(a.left() + 24.0, a.top() + 28.0), Align2::LEFT_CENTER, "Appearance", font(15.0, "inter_medium"), TEXT);
        text(&p, Pos2::new(a.left() + 24.0, a.top() + 74.0), Align2::LEFT_CENTER, "Reduce motion", font(14.0, "inter"), TEXT);
        text(&p, Pos2::new(a.left() + 24.0, a.top() + 96.0), Align2::LEFT_CENTER, "Replaces animations with instant changes", font(12.5, "inter"), TEXT3);
        if toggle(ui, Id::new("set_reduce"), Pos2::new(a.right() - 24.0, a.top() + 82.0), self.reduce_motion).clicked() {
            self.reduce_motion = !self.reduce_motion;
            set_reduce_motion(self.reduce_motion);
        }
        text(&p, Pos2::new(a.left() + 24.0, a.top() + 136.0), Align2::LEFT_CENTER, "Theme: monochrome (fixed)", font(12.5, "inter"), TEXT3);

        // Connections.
        let c = mk(r.left() + half + 16.0, y, 164.0);
        panel(&p, c, 16.0, PANEL);
        text(&p, Pos2::new(c.left() + 24.0, c.top() + 28.0), Align2::LEFT_CENTER, "Connections", font(15.0, "inter_medium"), TEXT);
        let rows = [("Wi-Fi", self.wifi_on), ("Bluetooth", self.bt_on)];
        for (i, (label, on)) in rows.iter().enumerate() {
            let cy = c.top() + 68.0 + i as f32 * 38.0;
            text(&p, Pos2::new(c.left() + 24.0, cy), Align2::LEFT_CENTER, *label, font(14.0, "inter"), TEXT);
            if toggle(ui, Id::new(("set_conn", i)), Pos2::new(c.right() - 24.0, cy), *on).clicked() {
                if i == 0 {
                    self.wifi_on = !self.wifi_on;
                } else {
                    self.bt_on = !self.bt_on;
                }
            }
        }
        text(&p, Pos2::new(c.left() + 24.0, c.top() + 140.0), Align2::LEFT_CENTER, "Ethernet: Connected (eth0)", font(12.5, "inter"), TEXT3);

        // Storage.
        let s = mk(r.left(), y + 180.0, 132.0);
        panel(&p, s, 16.0, PANEL);
        text(&p, Pos2::new(s.left() + 24.0, s.top() + 28.0), Align2::LEFT_CENTER, "Storage (eMMC)", font(15.0, "inter_medium"), TEXT);
        text(&p, Pos2::new(s.right() - 24.0, s.top() + 28.0), Align2::RIGHT_CENTER, "42 / 64 GB", font(13.5, "inter"), TEXT2);
        progress(&p, Rect::from_min_size(Pos2::new(s.left() + 24.0, s.top() + 60.0), Vec2::new(s.width() - 48.0, 8.0)), 42.0 / 64.0);
        text(&p, Pos2::new(s.left() + 24.0, s.top() + 96.0), Align2::LEFT_CENTER, "Demo value - not read from a real device", font(12.5, "inter"), TEXT3);

        // System.
        let sy = mk(r.left() + half + 16.0, y + 180.0, 132.0);
        panel(&p, sy, 16.0, PANEL);
        text(&p, Pos2::new(sy.left() + 24.0, sy.top() + 28.0), Align2::LEFT_CENTER, "System", font(15.0, "inter_medium"), TEXT);
        let info = [
            ("Data source", "Demo (simulated device)".to_string()),
            ("Backend", "Not connected".to_string()),
            ("App version", env!("CARGO_PKG_VERSION").to_string()),
        ];
        for (i, (k, v)) in info.iter().enumerate() {
            let cy = sy.top() + 58.0 + i as f32 * 22.0;
            text(&p, Pos2::new(sy.left() + 24.0, cy), Align2::LEFT_CENTER, *k, font(13.0, "inter"), TEXT3);
            text(&p, Pos2::new(sy.left() + 130.0, cy), Align2::LEFT_CENTER, v, font(13.0, "inter"), TEXT);
        }
        let _ = now;
    }
}

pub(super) fn group(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
