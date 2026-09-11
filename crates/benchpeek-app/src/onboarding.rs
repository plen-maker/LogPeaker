//! Lightweight vector onboarding. Stylized board, not a mechanical/CAD model.
//! Connector identities: ST UM3385 rev 2, figure 4 (CN21 ST-LINK, CN15 USB3).
use eframe::egui::{self, Align2, Color32, FontId, Rect, Stroke, Vec2};
use std::time::Instant;

pub struct Welcome {
    pub visible: bool,
    start: Instant,
    reduced_motion: bool,
}
impl Default for Welcome {
    fn default() -> Self {
        Self {
            visible: true,
            start: Instant::now(),
            reduced_motion: false,
        }
    }
}
impl Welcome {
    pub fn replay(&mut self) {
        self.visible = true;
        self.start = Instant::now();
    }
    /// Returns true when the user asks the app to enable serial auto-connect.
    pub fn ui(&mut self, ui: &mut egui::Ui, ports: &[String]) -> bool {
        let mut connect = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("benchpeek").size(22.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Skip intro").clicked() {
                    self.visible = false;
                }
                ui.checkbox(&mut self.reduced_motion, "Reduce motion");
            });
        });
        ui.add_space(12.0);
        let t = if self.reduced_motion {
            5.0
        } else {
            self.start.elapsed().as_secs_f32()
        };
        let h = (ui.available_height() - 165.0).max(130.0);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), h), egui::Sense::hover());
        draw(
            ui.painter().with_clip_rect(rect),
            rect,
            t,
            self.reduced_motion,
        );
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.heading(if t < 2.0 {
                "Meet your board."
            } else {
                "Plug in the device"
            });
            ui.label("Connect your computer to CN21 — USB-C ST-LINK / power.");
            ui.weak("STM32MP257F-DK · stylized connection guide");
            if ports.is_empty() {
                ui.label("Waiting for a USB serial device…");
            } else {
                ui.label(format!("USB serial port available: {}", ports.join(", ")));
            }
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - 350.0) / 2.0).max(0.0));
                if ui.button("Enable auto-connect").clicked() {
                    connect = true;
                    self.visible = false;
                }
                if ui.button("Open dashboard").clicked() {
                    self.visible = false;
                }
                if ui.button("Replay").clicked() {
                    self.start = Instant::now();
                }
            });
            ui.weak("Serial-port detection does not verify board identity or telemetry.");
        });
        connect
    }
}
fn ease(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    v * v * (3.0 - 2.0 * v)
}
fn draw(p: egui::Painter, rect: Rect, t: f32, still: bool) {
    let bg = Color32::from_rgb(12, 19, 28);
    let mint = Color32::from_rgb(85, 230, 183);
    p.rect_filled(rect, 16, bg);
    for x in (0..rect.width() as usize).step_by(32) {
        for y in (0..rect.height() as usize).step_by(32) {
            p.circle_filled(
                rect.min + Vec2::new(x as f32, y as f32),
                0.8,
                Color32::from_rgb(30, 42, 51),
            );
        }
    }
    let travel = ease((t - 1.2) / 2.8);
    let scale = (rect.width() / 760.0).min(rect.height() / 470.0) * (1.0 + travel * 1.35);
    // Start overhead; move camera toward the ST-LINK connector on the right edge.
    let focus = Vec2::new(240.0 + travel * 190.0, 150.0 + travel * 65.0);
    let at = |x: f32, y: f32| rect.center() + (Vec2::new(x, y) - focus) * scale;
    let box_at =
        |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(at(x, y), Vec2::new(w, h) * scale);
    p.rect_filled(
        box_at(-5.0, 8.0, 490.0, 300.0),
        12,
        Color32::from_black_alpha(100),
    );
    p.rect_filled(
        box_at(0.0, 0.0, 480.0, 300.0),
        10,
        Color32::from_rgb(29, 92, 88),
    );
    p.rect_stroke(
        box_at(0.0, 0.0, 480.0, 300.0),
        10,
        Stroke::new(1.5, Color32::from_rgb(82, 148, 131)),
        egui::StrokeKind::Inside,
    );
    for (x, y) in [(16., 16.), (464., 16.), (16., 284.), (464., 284.)] {
        p.circle_filled(at(x, y), 7. * scale, Color32::from_rgb(189, 177, 132));
        p.circle_filled(at(x, y), 3.8 * scale, bg);
    }
    for i in 0..19 {
        let y = 40. + i as f32 * 11.;
        p.line_segment(
            [at(58., y), at(155. + i as f32 * 4., y)],
            Stroke::new(scale * 0.65, Color32::from_rgb(49, 115, 106)),
        );
    }
    for (x, y, w, h, label) in [
        (175., 92., 88., 88., "STM32\nMP257F"),
        (175., 42., 65., 28., "LPDDR4"),
        (95., 125., 45., 52., "eMMC"),
        (361., 184., 45., 45., "ST-LINK"),
        (289., 107., 38., 38., "PMIC"),
    ] {
        p.rect_filled(box_at(x, y, w, h), 3, Color32::from_rgb(23, 30, 35));
        p.rect_stroke(
            box_at(x, y, w, h),
            3,
            Stroke::new(scale, Color32::from_gray(100)),
            egui::StrokeKind::Inside,
        );
        p.text(
            at(x + w / 2., y + h / 2.),
            Align2::CENTER_CENTER,
            label,
            FontId::monospace(8. * scale),
            Color32::from_gray(210),
        );
    }
    p.rect_filled(
        box_at(120., 262., 210., 20.),
        2,
        Color32::from_rgb(22, 27, 29),
    );
    for i in 0..20 {
        for j in 0..2 {
            p.circle_filled(
                at(126. + i as f32 * 10.2, 268. + j as f32 * 8.),
                1.8 * scale,
                Color32::from_rgb(212, 180, 111),
            );
        }
    }
    // Other edge connectors provide orientation without competing with CN21.
    for (x, y, w, h, label) in [
        (35., -9., 65., 48., "ETH"),
        (310., -9., 48., 46., "USB-A"),
        (400., -8., 32., 26., "CN15"),
        (0., 194., 30., 52., "HDMI"),
    ] {
        p.rect_filled(box_at(x, y, w, h), 3, Color32::from_rgb(152, 168, 174));
        p.text(
            at(x + w / 2., y + h / 2.),
            Align2::CENTER_CENTER,
            label,
            FontId::monospace(7. * scale),
            Color32::from_rgb(22, 32, 38),
        );
    }
    p.text(
        at(32., 239.),
        Align2::LEFT_CENTER,
        "STM32MP257F-DK",
        FontId::monospace(12. * scale),
        Color32::from_rgb(188, 221, 207),
    );
    let port = at(474., 218.);
    let pulse = if still {
        0.5
    } else {
        (t * 2.6).sin() * 0.5 + 0.5
    };
    for i in (1..=3).rev() {
        p.circle_filled(
            port,
            (18. + i as f32 * 8. + pulse * 4.) * scale,
            Color32::from_rgba_unmultiplied(85, 230, 183, 9),
        );
    }
    p.rect_filled(
        box_at(456., 204., 30., 28.),
        6,
        Color32::from_rgb(191, 204, 209),
    );
    p.rect_filled(
        box_at(473., 209., 10., 18.),
        4,
        Color32::from_rgb(15, 25, 32),
    );
    p.line_segment(
        [at(478., 212.), at(478., 224.)],
        Stroke::new(2. * scale, mint),
    );
    p.circle_stroke(port, (26. + pulse * 4.) * scale, Stroke::new(1.4, mint));
    if travel > 0.7 {
        let offset = if still { 0. } else { (t * 1.8).sin() * 5. };
        p.line_segment(
            [at(530. + offset, 218.), at(610., 218.)],
            Stroke::new(8. * scale, Color32::from_rgb(56, 69, 78)),
        );
        p.rect_filled(
            box_at(514. + offset, 207., 32., 22.),
            5,
            Color32::from_rgb(112, 126, 136),
        );
        p.rect_filled(
            box_at(506. + offset, 211., 11., 14.),
            3,
            Color32::from_rgb(192, 206, 211),
        );
    }
    p.text(
        rect.min + Vec2::new(22., 22.),
        Align2::LEFT_TOP,
        if travel < 0.5 {
            "01 / BOARD OVERVIEW"
        } else {
            "02 / USB CONNECTION"
        },
        FontId::monospace(12.),
        mint,
    );
    p.text(
        rect.left_bottom() + Vec2::new(22., -22.),
        Align2::LEFT_BOTTOM,
        "CN21  •  ST-LINK / POWER",
        FontId::monospace(13.),
        Color32::WHITE,
    );
}
#[cfg(test)]
mod preview {
    use super::*;
    use egui::epaint::Shape;
    use std::fmt::Write;
    fn color(c: Color32) -> String {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        format!("rgba({r},{g},{b},{})", a as f32 / 255.)
    }
    fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    fn shape(s: &Shape, out: &mut String) {
        match s {
            Shape::Vec(v) => {
                for s in v {
                    shape(s, out);
                }
            }
            Shape::Rect(r) => {
                let _=write!(out,"<rect x='{}' y='{}' width='{}' height='{}' rx='{}' fill='{}' stroke='{}' stroke-width='{}'/>",r.rect.min.x,r.rect.min.y,r.rect.width(),r.rect.height(),r.corner_radius.nw,color(r.fill),color(r.stroke.color),r.stroke.width);
            }
            Shape::Circle(c) => {
                let _ = write!(
                    out,
                    "<circle cx='{}' cy='{}' r='{}' fill='{}' stroke='{}' stroke-width='{}'/>",
                    c.center.x,
                    c.center.y,
                    c.radius,
                    color(c.fill),
                    color(c.stroke.color),
                    c.stroke.width
                );
            }
            Shape::LineSegment { points, stroke } => {
                let _ = write!(
                    out,
                    "<path d='M {} {} L {} {}' stroke='{}' stroke-width='{}'/>",
                    points[0].x,
                    points[0].y,
                    points[1].x,
                    points[1].y,
                    color(stroke.color),
                    stroke.width
                );
            }
            Shape::Text(t) => {
                let size = t
                    .galley
                    .job
                    .sections
                    .first()
                    .map(|s| s.format.font_id.size)
                    .unwrap_or(14.);
                let c = t.override_text_color.unwrap_or(t.fallback_color);
                for (i, line) in t.galley.job.text.lines().enumerate() {
                    let _=write!(out,"<text x='{}' y='{}' fill='{}' font-size='{}' font-family='DejaVu Sans'>{}</text>",t.pos.x+t.galley.rect.min.x,t.pos.y+size*(1.+i as f32),color(c),size,escape(line));
                }
            }
            _ => {}
        }
    }
    #[test]
    #[ignore = "Manual SVG layout export; set BENCHPEEK_PREVIEW_DIR"]
    fn export_frames() {
        let dir = std::env::var("BENCHPEEK_PREVIEW_DIR").expect("output directory");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, t) in [("overview", 0.), ("usb", 5.)] {
            let ctx = egui::Context::default();
            crate::theme::apply(&ctx);
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1180., 760.),
            ));
            let mut welcome = Welcome::default();
            welcome.start = Instant::now() - std::time::Duration::from_secs_f32(t);
            let mut result = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    welcome.ui(ui, &[]);
                });
            });
            let mut svg =
                String::from("<svg xmlns='http://www.w3.org/2000/svg' width='1180' height='760'>");
            for (i, s) in result.shapes.iter().enumerate() {
                let r = s.clip_rect;
                let _=write!(svg,"<defs><clipPath id='c{i}'><rect x='{}' y='{}' width='{}' height='{}'/></clipPath></defs><g clip-path='url(#c{i})'>",r.min.x,r.min.y,r.width(),r.height());
                shape(&s.shape, &mut svg);
                svg.push_str("</g>");
            }
            svg.push_str("</svg>");
            result.textures_delta.clear();
            std::fs::write(format!("{dir}/{name}.svg"), svg).unwrap();
        }
    }
}
