//! benchpeek: an open, extensible board-diagnostics cockpit.
//!
//! Two front ends share the crate: the monochrome Workspace UI (default) and
//! the original signal cockpit (`--classic`).

mod app;
mod files;
mod icons;
mod onboarding;
mod source;
mod theme;
mod ws;
mod yocto;

fn main() -> eframe::Result<()> {
    let classic = std::env::args().any(|a| a == "--classic");
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("benchpeek"),
        ..Default::default()
    };
    if classic {
        eframe::run_native(
            "benchpeek",
            native_options,
            Box::new(|cc| {
                theme::apply(&cc.egui_ctx);
                Ok(Box::new(app::BenchpeekApp::default()))
            }),
        )
    } else {
        eframe::run_native(
            "benchpeek",
            native_options,
            Box::new(|cc| Ok(Box::new(ws::Workspace::new(cc)))),
        )
    }
}
