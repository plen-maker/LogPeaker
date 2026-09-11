//! Visual identity: a dark "diagnostics cockpit" look instead of default
//! egui gray, plus a small connection-state palette used by the top bar.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(16, 18, 21);
pub const PANEL_BG: Color32 = Color32::from_rgb(22, 25, 29);
pub const WIDGET_BG: Color32 = Color32::from_rgb(30, 34, 39);
pub const WIDGET_HOVER: Color32 = Color32::from_rgb(40, 46, 52);
pub const WIDGET_ACTIVE: Color32 = Color32::from_rgb(50, 57, 64);
pub const STROKE: Color32 = Color32::from_rgb(52, 58, 66);
pub const TEXT: Color32 = Color32::from_rgb(220, 224, 228);
pub const WEAK_TEXT: Color32 = Color32::from_rgb(140, 148, 156);
pub const ACCENT: Color32 = Color32::from_rgb(72, 211, 201);

pub const OK: Color32 = Color32::from_rgb(120, 205, 130);
pub const WARN: Color32 = Color32::from_rgb(232, 190, 90);
pub const FAULT: Color32 = Color32::from_rgb(235, 108, 108);
pub const IDLE: Color32 = Color32::from_rgb(100, 106, 114);

pub fn apply(ctx: &egui::Context) {
    // benchpeek is a deliberately dark cockpit look - don't follow the OS
    // light/dark preference.
    ctx.set_theme(egui::ThemePreference::Dark);

    let mut visuals = egui::Visuals::dark();

    visuals.override_text_color = Some(TEXT);
    visuals.weak_text_color = Some(WEAK_TEXT);
    visuals.window_fill = PANEL_BG;
    visuals.panel_fill = BG;
    visuals.extreme_bg_color = Color32::from_rgb(10, 11, 13);
    visuals.faint_bg_color = WIDGET_BG;
    visuals.code_bg_color = WIDGET_BG;
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = WARN;
    visuals.error_fg_color = FAULT;
    visuals.selection.bg_fill = ACCENT.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    visuals.widgets.noninteractive.bg_fill = PANEL_BG;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL_BG;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, STROKE);

    visuals.widgets.inactive.bg_fill = WIDGET_BG;
    visuals.widgets.inactive.weak_bg_fill = WIDGET_BG;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, STROKE);

    visuals.widgets.hovered.bg_fill = WIDGET_HOVER;
    visuals.widgets.hovered.weak_bg_fill = WIDGET_HOVER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);

    visuals.widgets.active.bg_fill = WIDGET_ACTIVE;
    visuals.widgets.active.weak_bg_fill = WIDGET_ACTIVE;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, ACCENT);

    visuals.widgets.open.bg_fill = WIDGET_BG;
    visuals.widgets.open.weak_bg_fill = WIDGET_BG;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT);

    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(4);
    }
    visuals.window_corner_radius = CornerRadius::same(6);
    visuals.menu_corner_radius = CornerRadius::same(4);

    ctx.set_visuals_of(egui::Theme::Dark, visuals);

    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.window_margin = Margin::same(10);
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(17.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(13.5, FontFamily::Monospace),
    );
    ctx.set_style_of(egui::Theme::Dark, style);
}
