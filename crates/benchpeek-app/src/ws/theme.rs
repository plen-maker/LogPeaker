//! Monochrome design tokens, fonts, easing and small animation helpers for
//! the workspace UI. Colours are exactly the palette from the design brief.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui::{
    Align2, Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Id, Painter,
    Pos2, Rect, Ui,
};

pub const BG: Color32 = Color32::from_rgb(0x09, 0x0B, 0x0C);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x12, 0x15, 0x16);
pub const PANEL: Color32 = Color32::from_rgb(0x17, 0x1A, 0x1C);
pub const RAISED: Color32 = Color32::from_rgb(0x20, 0x24, 0x26);
pub const SELECTED: Color32 = Color32::from_rgb(0x33, 0x37, 0x39);
pub const TEXT: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF0);
pub const TEXT2: Color32 = Color32::from_rgb(0xA2, 0xA6, 0xA8);
pub const TEXT3: Color32 = Color32::from_rgb(0x72, 0x78, 0x7B);
pub const PRIMARY: Color32 = Color32::from_rgb(0xEC, 0xED, 0xEB);
pub const ON_PRIMARY: Color32 = Color32::from_rgb(0x0D, 0x0F, 0x10);

/// rgba(255,255,255,0.14)
pub fn border() -> Color32 {
    Color32::from_white_alpha(36)
}
pub fn hairline() -> Color32 {
    Color32::from_white_alpha(20)
}

static REDUCE_MOTION: AtomicBool = AtomicBool::new(false);
pub fn set_reduce_motion(v: bool) {
    REDUCE_MOTION.store(v, Ordering::Relaxed);
}
pub fn reduce_motion() -> bool {
    REDUCE_MOTION.load(Ordering::Relaxed)
}

pub fn fam(name: &str) -> FontFamily {
    FontFamily::Name(name.into())
}
pub fn font(size: f32, family: &str) -> FontId {
    FontId::new(size, fam(family))
}

pub fn install_fonts(ctx: &Context) {
    let mut fd = FontDefinitions::default();
    let mut add = |key: &str, bytes: &'static [u8]| {
        fd.font_data
            .insert(key.to_string(), Arc::new(FontData::from_static(bytes)));
        fd.families
            .insert(fam(key), vec![key.to_string()]);
    };
    add("inter", include_bytes!("../../assets/fonts/Inter-Regular.ttf"));
    add("inter_light", include_bytes!("../../assets/fonts/Inter-Light.ttf"));
    add("inter_medium", include_bytes!("../../assets/fonts/Inter-Medium.ttf"));
    add("inter_semibold", include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"));
    add("mono", include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf"));
    add("icons", include_bytes!("../../assets/fonts/lucide.ttf"));
    fd.families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "inter".to_string());
    fd.families
        .get_mut(&FontFamily::Monospace)
        .unwrap()
        .insert(0, "mono".to_string());
    ctx.set_fonts(fd);
}

/// cubic-bezier(0.22, 1, 0.36, 1) - the single ease-out curve used everywhere.
pub fn ease(x: f32) -> f32 {
    let (x1, y1, x2, y2) = (0.22_f32, 1.0_f32, 0.36_f32, 1.0_f32);
    let x = x.clamp(0.0, 1.0);
    let bez = |a: f32, b: f32, t: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    };
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    let mut t = x;
    for _ in 0..24 {
        let bx = bez(x1, x2, t);
        if (bx - x).abs() < 1e-4 {
            break;
        }
        if bx < x {
            lo = t;
        } else {
            hi = t;
        }
        t = 0.5 * (lo + hi);
    }
    bez(y1, y2, t)
}

/// A value that eases toward a target with the shared curve.
#[derive(Clone, Copy)]
pub struct Tween {
    from: f32,
    to: f32,
    t0: f64,
    dur: f32,
}

impl Tween {
    pub fn new(v: f32) -> Self {
        Self { from: v, to: v, t0: 0.0, dur: 0.0 }
    }
    pub fn set(&mut self, target: f32, now: f64, dur_ms: f32) {
        if (target - self.to).abs() < 1e-4 {
            return;
        }
        self.from = self.get(now);
        self.to = target;
        self.t0 = now;
        self.dur = if reduce_motion() { 0.0 } else { dur_ms / 1000.0 };
    }
    pub fn snap(&mut self, v: f32) {
        *self = Self::new(v);
    }
    pub fn get(&self, now: f64) -> f32 {
        if self.dur <= 0.0 {
            return self.to;
        }
        let p = ((now - self.t0) / self.dur as f64).clamp(0.0, 1.0) as f32;
        self.from + (self.to - self.from) * ease(p)
    }
    pub fn running(&self, now: f64) -> bool {
        self.dur > 0.0 && now - self.t0 < self.dur as f64
    }
    pub fn target(&self) -> f32 {
        self.to
    }
}

/// 0..1 eased hover/active value for a widget id.
pub fn hv(ui: &Ui, id: Id, on: bool, ms: f32) -> f32 {
    let t = if reduce_motion() { 0.0 } else { ms / 1000.0 };
    ui.ctx().animate_bool_with_time_and_easing(id, on, t, ease)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
}

pub fn text(p: &Painter, pos: Pos2, a: Align2, s: impl ToString, f: FontId, c: Color32) -> Rect {
    p.text(pos, a, s.to_string(), f, c)
}

pub fn icon(p: &Painter, c: char, center: Pos2, size: f32, color: Color32) {
    p.text(center, Align2::CENTER_CENTER, c.to_string(), font(size, "icons"), color);
}

pub fn scale_rect(r: Rect, s: f32) -> Rect {
    Rect::from_center_size(r.center(), r.size() * s)
}

/// Truncate monospace text to fit `max_w` (JetBrains Mono advance is 0.6 em).
pub fn ellipsize_mono(s: &str, size: f32, max_w: f32) -> String {
    let n = (max_w / (size * 0.6)).floor().max(1.0) as usize;
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_is_monotonic_and_pinned_at_the_ends() {
        assert!(ease(0.0).abs() < 1e-3);
        assert!((ease(1.0) - 1.0).abs() < 1e-3);
        let mut prev = 0.0;
        for i in 0..=100 {
            let v = ease(i as f32 / 100.0);
            assert!(v >= prev - 1e-4, "not monotonic at {i}");
            prev = v;
        }
        // Ease-out: fast start.
        assert!(ease(0.25) > 0.5);
    }

    #[test]
    fn tween_reaches_target_and_reports_running() {
        set_reduce_motion(false);
        let mut t = Tween::new(0.0);
        t.set(1.0, 10.0, 200.0);
        assert!(t.running(10.05));
        assert!(t.get(10.0) < 0.01);
        assert!((t.get(10.3) - 1.0).abs() < 1e-3);
        assert!(!t.running(10.3));
    }

    #[test]
    fn ellipsize_keeps_short_text_and_truncates_long() {
        assert_eq!(ellipsize_mono("abc", 12.0, 200.0), "abc");
        let out = ellipsize_mono("abcdefghijklmnopqrstuvwxyz", 10.0, 60.0);
        assert_eq!(out.chars().count(), 10);
        assert!(out.ends_with('\u{2026}'));
    }
}
