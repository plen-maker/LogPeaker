//! benchpeek: an open, extensible board-diagnostics cockpit.
//!
//! v0 wiring: a source thread (simulated / serial / replay) feeds samples
//! through a channel into the egui UI, which stores them, evaluates rules,
//! and shows a live plot plus a human-readable diagnosis panel.

mod app;
mod files;
mod onboarding;
mod source;
mod theme;
mod yocto;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_title("benchpeek"),
        ..Default::default()
    };
    eframe::run_native(
        "benchpeek",
        native_options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::BenchpeekApp::default()))
        }),
    )
}
