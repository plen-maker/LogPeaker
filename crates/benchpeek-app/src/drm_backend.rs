//! Runs benchpeek as its own DRM/KMS output, bypassing Wayland entirely.
//!
//! On the STM32MP257F-DK, the vendor GC-Nano EGL driver segfaults deep in its
//! own Wayland-client integration (`wl_proxy_create_wrapper`, SEGV_MAPERR)
//! whenever benchpeek runs as a Wayland client under meerkat-compositor -
//! confirmed on real hardware 2026-09-25, with the crash unchanged across a
//! wgpu-vs-glow fix, an `EGL_PLATFORM` hint, and disabling window decorations.
//! No source or vendor docs are available to fix the driver itself.
//!
//! meerkat-compositor never hits this: wlroots drives the display straight
//! over DRM/GBM and never goes through the driver's Wayland platform code.
//! This module does the same thing egui-side: open `/dev/dri/card0`, mode-set
//! the connected output, and drive EGL/GLES via a GBM surface instead of a
//! Wayland one. Only one process can hold the DRM master at a time, so
//! meerkat-compositor must not be running while this is active.
//!
//! Touch input (`touch_input.rs`) and real page-flipping are both wired in;
//! see `main.rs` for how the app itself (`ws::Workspace` by default,
//! `app::BenchpeekApp` under `--classic`) is selected.

use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::num::NonZeroU32;
use std::os::unix::io::{AsFd, BorrowedFd};
use std::ptr::NonNull;
use std::sync::Arc;

use anyhow::{anyhow, Context as _};
use drm::control::{self, connector, crtc, Device as ControlDevice, Mode};
use drm::Device as BasicDevice;
use eframe::App as _;
use gbm::{AsRaw, BufferObjectFlags, Format};
use glow::HasContext as _;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentGlContext,
};
use glutin::display::{Display, DisplayApiPreference, GlDisplay};
use glutin::surface::{GlSurface, SurfaceAttributesBuilder, WindowSurface};
use raw_window_handle::{GbmDisplayHandle, GbmWindowHandle, RawDisplayHandle, RawWindowHandle};

/// Thin wrapper so `drm`'s device traits (which key off `AsFd`) have
/// something to attach to - the same pattern `drm-rs`'s own examples use.
struct Card(File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl BasicDevice for Card {}
impl ControlDevice for Card {}

/// `build_app` gets the real `egui::Context` this loop creates below - it
/// can't run any earlier, since neither `ws::Workspace::new` nor
/// `app::BenchpeekApp` (for `--classic`) need anything else from an
/// `eframe::CreationContext`, so there's no need to fake one up.
pub fn run(build_app: impl FnOnce(&egui::Context) -> Box<dyn eframe::App>) -> anyhow::Result<()> {
    let card = Card(
        OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/dri/card0")
            .context("opening /dev/dri/card0 - is meerkat-compositor still running?")?,
    );
    // Reading resources/connectors works for any fd, but mode-setting
    // (set_crtc, page_flip) needs this process to hold the DRM master lock -
    // otherwise it fails with EACCES even as root.
    card.acquire_master_lock()
        .context("acquire_master_lock - is another compositor still holding it?")?;

    let res = card.resource_handles().context("DRM resource_handles")?;
    let connectors: Vec<connector::Info> = res
        .connectors()
        .iter()
        .flat_map(|c| card.get_connector(*c, true))
        .collect();
    let con = connectors
        .iter()
        .find(|c| c.state() == connector::State::Connected)
        .ok_or_else(|| anyhow!("no connected DRM connector"))?;
    let &mode: &Mode = con
        .modes()
        .first()
        .ok_or_else(|| anyhow!("connector has no modes"))?;
    let (width, height) = mode.size();
    let (width, height) = (width as u32, height as u32);

    let crtcs: Vec<crtc::Info> = res.crtcs().iter().flat_map(|c| card.get_crtc(*c)).collect();
    let crtc = crtcs.first().ok_or_else(|| anyhow!("no DRM crtc"))?;

    let gbm = gbm::Device::new(card).context("gbm::Device::new")?;
    let gbm_surface = gbm
        .create_surface::<()>(
            width,
            height,
            Format::Xrgb8888,
            BufferObjectFlags::RENDERING | BufferObjectFlags::SCANOUT,
        )
        .context("gbm create_surface")?;

    let raw_display = RawDisplayHandle::Gbm(GbmDisplayHandle::new(
        NonNull::new(gbm.as_raw() as *mut _).ok_or_else(|| anyhow!("null gbm device"))?,
    ));
    let raw_window = RawWindowHandle::Gbm(GbmWindowHandle::new(
        NonNull::new(gbm_surface.as_raw() as *mut _).ok_or_else(|| anyhow!("null gbm surface"))?,
    ));

    // Safety: `raw_display`/`raw_window` stay valid for as long as `gbm` and
    // `gbm_surface` are alive, which outlive every glutin object below.
    let gl_display = unsafe { Display::new(raw_display, DisplayApiPreference::Egl) }
        .context("glutin EGL Display::new")?;
    let config = unsafe { gl_display.find_configs(ConfigTemplateBuilder::new().build()) }
        .context("find_configs")?
        .next()
        .ok_or_else(|| anyhow!("no matching EGL config"))?;

    // The GC-Nano GPU only ships a GLES driver (no desktop GL) - request it
    // directly instead of letting glutin try desktop GL first and fall back.
    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(None))
        .build(Some(raw_window));
    let not_current_context = unsafe { gl_display.create_context(&config, &context_attributes) }
        .context("create_context")?;

    let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window,
        NonZeroU32::new(width).ok_or_else(|| anyhow!("zero width"))?,
        NonZeroU32::new(height).ok_or_else(|| anyhow!("zero height"))?,
    );
    let gl_surface = unsafe { gl_display.create_window_surface(&config, &surface_attributes) }
        .context("create_window_surface")?;
    let gl_context = not_current_context
        .make_current(&gl_surface)
        .context("make_current")?;

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            CString::new(s)
                .map(|cs| gl_display.get_proc_address(&cs) as *const _)
                .unwrap_or(std::ptr::null())
        })
    };
    let gl = Arc::new(gl);
    let mut painter = egui_glow::Painter::new(gl.clone(), "", None, false)
        .map_err(|e| anyhow!("egui_glow::Painter::new: {e}"))?;

    let egui_ctx = egui::Context::default();
    let mut app = build_app(&egui_ctx);

    log::info!("DRM backend up: {width}x{height} on {:?}", con.interface());

    let mut touch = crate::touch_input::TouchInput::open()
        .context("opening touchscreen")?;
    match &touch {
        Some(_) => log::info!("touchscreen input active"),
        None => log::warn!("no /dev/input/touchscreen0 - running without touch input"),
    }

    // Keeps the currently scanned-out buffer's gbm reference alive; dropping
    // the previous one releases it back to the swap chain (see
    // `gbm::Surface::lock_front_buffer`'s docs - the release happens in
    // `BufferObject`'s `Drop`, not via an explicit call).
    let mut _scanned_out: Option<gbm::BufferObject<()>> = None;
    let mut crtc_configured = false;
    let screen_size_px = egui::vec2(width as f32, height as f32);

    // This panel has no OS-level DPI concept, so the native scale is just a
    // fixed 1.0 - `apply_scale`-style logic (`ws::Workspace` scales its whole
    // 1280x800 design down to fit here via `egui::Context::set_zoom_factor`)
    // still works on top of it, adjusting `pixels_per_point` from there.
    let mut viewports = egui::ViewportIdMap::default();
    viewports.insert(
        egui::ViewportId::ROOT,
        egui::ViewportInfo {
            native_pixels_per_point: Some(1.0),
            ..Default::default()
        },
    );

    loop {
        // `RawInput::screen_rect` must be in *points*, not physical pixels
        // (see its doc comment) - dividing by the context's current
        // pixels_per_point (which `ws::Workspace::apply_scale` changes via
        // `set_zoom_factor` to fit its 1280x800 design onto this 1024x600
        // panel) keeps that conversion correct every frame. Feeding the raw
        // pixel size here instead compounds each frame (apply_scale reads it
        // back out scaled by the *previous* frame's pixels_per_point), which
        // spirals the zoom down to its floor within a few frames - confirmed
        // on real hardware 2026-09-30 as the UI rendering tiny in a corner.
        let ppp = egui_ctx.pixels_per_point();
        let screen_size_pts = screen_size_px / ppp;

        let mut events = Vec::new();
        if let Some(touch) = &mut touch {
            touch.poll(screen_size_pts, &mut events);
        }

        let raw_input = egui::RawInput {
            viewport_id: egui::ViewportId::ROOT,
            viewports: viewports.clone(),
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size_pts)),
            events,
            ..Default::default()
        };

        let mut frame = eframe::Frame::_new_kittest();
        let mut full_output = egui_ctx.run_ui(raw_input, |ui| {
            app.ui(ui, &mut frame);
        });

        let clipped_primitives =
            egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

        unsafe {
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        painter.paint_and_update_textures(
            [width, height],
            full_output.pixels_per_point,
            &clipped_primitives,
            &mut full_output.textures_delta,
        );

        gl_surface
            .swap_buffers(&gl_context)
            .context("swap_buffers")?;

        // Safety: called exactly once per `swap_buffers`, per this crate's
        // own safety contract.
        let bo = unsafe { gbm_surface.lock_front_buffer() }
            .map_err(|e| anyhow!("lock_front_buffer: {e}"))?;
        let fb = gbm.add_framebuffer(&bo, 24, 32).context("add_framebuffer")?;

        if crtc_configured {
            // Queue the flip and block until the DRM event says it actually
            // happened (at the next vblank) - this is what paces the loop to
            // the display's refresh rate instead of spinning unthrottled.
            gbm.page_flip(crtc.handle(), fb, control::PageFlipFlags::EVENT, None)
                .context("page_flip")?;
            for event in gbm.receive_events().context("receive_events")? {
                if let control::Event::PageFlip(_) = event {
                    break;
                }
            }
        } else {
            // First frame only: page_flip needs a CRTC already showing a
            // framebuffer to flip *from*, so this has to be set_crtc once.
            gbm.set_crtc(crtc.handle(), Some(fb), (0, 0), &[con.handle()], Some(mode))
                .context("set_crtc")?;
            crtc_configured = true;
        }

        _scanned_out = Some(bo);
    }
}
