//! benchpeek: an open, extensible board-diagnostics cockpit.
//!
//! Two front ends share the crate: the monochrome Workspace UI (default,
//! `ws::Workspace`, designed for the STM32MP257F-DK's 1024x600 touchscreen)
//! and the original signal cockpit (`--classic`, `app::BenchpeekApp`).

mod app;
#[cfg(target_os = "linux")]
mod drm_backend;
mod files;
mod icons;
mod onboarding;
mod source;
mod stlink;
mod theme;
#[cfg(target_os = "linux")]
mod touch_input;
mod ws;
mod yocto;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    log::info!("benchpeek starting up");

    let classic = std::env::args().any(|a| a == "--classic");

    // BENCHPEEK_DRM opts into driving the display straight over DRM/KMS+GBM
    // instead of running as a Wayland client - see drm_backend.rs for why:
    // the STM32MP257F-DK's vendor EGL driver segfaults in its own
    // Wayland-client code, and this path never goes near it. Only one
    // process can hold the DRM master, so meerkat-compositor must not be
    // running when this is used.
    #[cfg(target_os = "linux")]
    if std::env::var_os("BENCHPEEK_DRM").is_some() {
        log::info!("BENCHPEEK_DRM set: driving /dev/dri/card0 directly, not via Wayland");
        return drm_backend::run(move |ctx| {
            if classic {
                theme::apply(ctx);
                Box::new(app::BenchpeekApp::default())
            } else {
                Box::new(ws::Workspace::new(ctx))
            }
        });
    }

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("benchpeek"),
        ..Default::default()
    };
    eframe::run_native(
        "benchpeek",
        native_options,
        Box::new(move |cc| {
            if classic {
                theme::apply(&cc.egui_ctx);
                Ok(Box::new(app::BenchpeekApp::default()))
            } else {
                Ok(Box::new(ws::Workspace::new(&cc.egui_ctx)))
            }
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe::run_native failed: {e}"))
}
