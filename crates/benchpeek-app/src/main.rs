//! benchpeek: an open, extensible board-diagnostics cockpit.
//!
//! v0 wiring: a source thread (simulated / serial / replay) feeds samples
//! through a channel into the egui UI, which stores them, evaluates rules,
//! and shows a live plot plus a human-readable diagnosis panel.

mod app;
#[cfg(target_os = "linux")]
mod drm_backend;
mod files;
mod onboarding;
mod source;
mod stlink;
mod theme;
mod yocto;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    log::info!("benchpeek starting up");

    // BENCHPEEK_DRM opts into driving the display straight over DRM/KMS+GBM
    // instead of running as a Wayland client - see drm_backend.rs for why:
    // the STM32MP257F-DK's vendor EGL driver segfaults in its own
    // Wayland-client code, and this path never goes near it. Only one
    // process can hold the DRM master, so meerkat-compositor must not be
    // running when this is used.
    #[cfg(target_os = "linux")]
    if std::env::var_os("BENCHPEEK_DRM").is_some() {
        log::info!("BENCHPEEK_DRM set: driving /dev/dri/card0 directly, not via Wayland");
        return drm_backend::run(app::BenchpeekApp::default());
    }

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
    .map_err(|e| anyhow::anyhow!("eframe::run_native failed: {e}"))
}
