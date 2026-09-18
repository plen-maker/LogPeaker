//! Hand-painted widgets used by the workspace UI. Everything is placed with
//! explicit rectangles so the layout follows the 1280x800 design grid, and
//! every hit area is at least 44x44 logical px.

use eframe::egui::{
    self, epaint::Shadow, Align2, Color32, CornerRadius, Id, Painter, Pos2, Rect, Response, Sense,
    Stroke, StrokeKind, Ui,
};

use super::theme::*;

pub fn cr(r: f32) -> CornerRadius {
    CornerRadius::same(r.round().clamp(0.0, 255.0) as u8)
}

/// Soft shadow + fill + hairline border + faint top highlight.
pub fn panel(p: &Painter, rect: Rect, radius: f32, fill: Color32) {
    p.add(
        Shadow { offset: [0, 10], blur: 28, spread: 0, color: Color32::from_black_alpha(110) }
            .as_shape(rect, cr(radius)),
    );
    p.rect_filled(rect, cr(radius), fill);
    p.rect_stroke(rect, cr(radius), Stroke::new(1.0, border()), StrokeKind::Inside);
    p.line_segment(
        [
            Pos2::new(rect.left() + radius, rect.top() + 1.0),
            Pos2::new(rect.right() - radius, rect.top() + 1.0),
        ],
        Stroke::new(1.0, Color32::from_white_alpha(26)),
    );
}

pub fn focus_ring(p: &Painter, rect: Rect, radius: f32) {
    p.rect_stroke(
        rect.expand(3.0),
        cr(radius + 3.0),
        Stroke::new(2.0, TEXT),
        StrokeKind::Outside,
    );
}

/// Filled off-white button with dark label and optional trailing icon.
pub fn primary_button(ui: &mut Ui, id: Id, rect: Rect, label: &str, trailing: Option<char>) -> Response {
    let r = ui.interact(rect, id, Sense::click());
    let h = hv(ui, id.with("h"), r.hovered(), 120.0);
    let press = ui.ctx().animate_value_with_time(
        id.with("p"),
        if r.is_pointer_button_down_on() { 0.985 } else { 1.0 },
        if reduce_motion() { 0.0 } else { 0.08 },
    );
    let rr = scale_rect(rect, press);
    let p = ui.painter();
    p.rect_filled(rr, cr(12.0), mix(PRIMARY, Color32::WHITE, h * 0.55));
    let cy = rr.center().y;
    let font_ = font(15.0, "inter_medium");
    let galley = p.layout_no_wrap(label.to_string(), font_.clone(), ON_PRIMARY);
    let extra = if trailing.is_some() { 26.0 } else { 0.0 };
    let total = galley.size().x + extra;
    let x0 = rr.center().x - total / 2.0;
    text(p, Pos2::new(x0, cy), Align2::LEFT_CENTER, label, font_, ON_PRIMARY);
    if let Some(ic) = trailing {
        icon(p, ic, Pos2::new(x0 + galley.size().x + 18.0, cy), 18.0, ON_PRIMARY);
    }
    if r.has_focus() {
        focus_ring(p, rr, 12.0);
    }
    r
}

/// Outlined / ghost button with optional leading icon.
pub fn ghost_button(ui: &mut Ui, id: Id, rect: Rect, label: &str, leading: Option<char>) -> Response {
    let r = ui.interact(rect, id, Sense::click());
    let h = hv(ui, id.with("h"), r.hovered(), 120.0);
    let press = ui.ctx().animate_value_with_time(
        id.with("p"),
        if r.is_pointer_button_down_on() { 0.985 } else { 1.0 },
        if reduce_motion() { 0.0 } else { 0.08 },
    );
    let rr = scale_rect(rect, press);
    let p = ui.painter();
    p.rect_filled(rr, cr(12.0), Color32::from_white_alpha((h * 16.0) as u8));
    p.rect_stroke(
        rr,
        cr(12.0),
        Stroke::new(1.0, Color32::from_white_alpha(36 + (h * 26.0) as u8)),
        StrokeKind::Inside,
    );
    let font_ = font(14.0, "inter_medium");
    let galley = p.layout_no_wrap(label.to_string(), font_.clone(), TEXT);
    let extra = if leading.is_some() { 26.0 } else { 0.0 };
    let x0 = rr.center().x - (galley.size().x + extra) / 2.0;
    if let Some(ic) = leading {
        icon(p, ic, Pos2::new(x0 + 9.0, rr.center().y), 17.0, TEXT);
    }
    text(p, Pos2::new(x0 + extra, rr.center().y), Align2::LEFT_CENTER, label, font_, TEXT);
    if r.has_focus() {
        focus_ring(p, rr, 12.0);
    }
    r
}

/// Round-cornered icon-only button; hit area is at least 44x44.
pub fn icon_button(ui: &mut Ui, id: Id, center: Pos2, glyph: char, size: f32, color: Color32) -> Response {
    let rect = Rect::from_center_size(center, egui::vec2(44.0, 44.0));
    let r = ui.interact(rect, id, Sense::click());
    let h = hv(ui, id.with("h"), r.hovered(), 120.0);
    let p = ui.painter();
    p.rect_filled(
        Rect::from_center_size(center, egui::vec2(36.0, 36.0)),
        cr(10.0),
        Color32::from_white_alpha((h * 20.0) as u8),
    );
    icon(p, glyph, center, size, mix(color, TEXT, h));
    if r.has_focus() {
        focus_ring(p, Rect::from_center_size(center, egui::vec2(36.0, 36.0)), 10.0);
    }
    r
}

/// 46x28 switch. Returns the response (click toggles `on` in the caller).
pub fn toggle(ui: &mut Ui, id: Id, right_center: Pos2, on: bool) -> Response {
    let rect = Rect::from_center_size(
        Pos2::new(right_center.x - 23.0, right_center.y),
        egui::vec2(46.0, 28.0),
    );
    let hit = Rect::from_center_size(rect.center(), egui::vec2(54.0, 44.0));
    let r = ui.interact(hit, id, Sense::click());
    let t = hv(ui, id.with("t"), on, 180.0);
    let p = ui.painter();
    p.rect_filled(rect, cr(14.0), mix(Color32::from_rgb(0x3A, 0x3E, 0x40), PRIMARY, t));
    let kx = rect.left() + 14.0 + t * 18.0;
    p.circle_filled(Pos2::new(kx, rect.center().y), 10.0, mix(Color32::from_rgb(0xD8, 0xDA, 0xD9), ON_PRIMARY, t));
    if r.has_focus() {
        focus_ring(p, rect, 14.0);
    }
    r
}

/// Thin horizontal slider, value 0..1. Returns true when the value changed.
pub fn slider(ui: &mut Ui, id: Id, rect: Rect, value: &mut f32) -> bool {
    let hit = Rect::from_min_max(
        Pos2::new(rect.left() - 10.0, rect.center().y - 22.0),
        Pos2::new(rect.right() + 10.0, rect.center().y + 22.0),
    );
    let r = ui.interact(hit, id, Sense::click_and_drag());
    let mut changed = false;
    if r.dragged() || r.clicked() || r.is_pointer_button_down_on() {
        if let Some(pos) = r.interact_pointer_pos() {
            let v = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            if (v - *value).abs() > 1e-4 {
                *value = v;
                changed = true;
            }
        }
    }
    if r.has_focus() {
        let step = ui.input(|i| {
            (i.key_pressed(egui::Key::ArrowRight) as i32 - i.key_pressed(egui::Key::ArrowLeft) as i32) as f32
        });
        if step != 0.0 {
            *value = (*value + step * 0.05).clamp(0.0, 1.0);
            changed = true;
        }
    }
    let h = hv(ui, id.with("h"), r.hovered() || r.dragged(), 120.0);
    let p = ui.painter();
    let track = Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
    p.rect_filled(track, cr(2.0), Color32::from_rgb(0x3A, 0x3E, 0x40));
    let fill = Rect::from_min_max(track.min, Pos2::new(rect.left() + rect.width() * *value, track.max.y));
    p.rect_filled(fill, cr(2.0), mix(TEXT2, TEXT, h));
    let kc = Pos2::new(rect.left() + rect.width() * *value, rect.center().y);
    p.circle_filled(kc, 9.0 + h * 1.0, TEXT);
    if r.has_focus() {
        p.circle_stroke(kc, 13.0, Stroke::new(2.0, TEXT));
    }
    changed
}

/// Segmented horizontal meter made of `segments` rounded bars.
pub fn segments(p: &Painter, rect: Rect, segments: usize, filled: f32) {
    let gap = 3.0;
    let w = (rect.width() - gap * (segments as f32 - 1.0)) / segments as f32;
    for i in 0..segments {
        let x = rect.left() + i as f32 * (w + gap);
        let on = (i as f32 + 0.5) / segments as f32 <= filled;
        p.rect_filled(
            Rect::from_min_size(Pos2::new(x, rect.top()), egui::vec2(w, rect.height())),
            cr(2.0),
            if on { TEXT } else { Color32::from_white_alpha(28) },
        );
    }
}

pub fn progress(p: &Painter, rect: Rect, frac: f32) {
    p.rect_filled(rect, cr(rect.height() / 2.0), Color32::from_white_alpha(28));
    let f = Rect::from_min_size(rect.min, egui::vec2(rect.width() * frac.clamp(0.0, 1.0), rect.height()));
    p.rect_filled(f, cr(rect.height() / 2.0), TEXT);
}
