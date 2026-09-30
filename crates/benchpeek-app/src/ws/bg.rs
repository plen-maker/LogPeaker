//! Procedural background: a very dark, faintly glowing folded surface. Pure
//! greys, generated once at start-up into a small texture that the GPU
//! upscales - no per-frame blur.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

pub fn make(ctx: &egui::Context) -> TextureHandle {
    const W: usize = 512;
    const H: usize = 320;
    // A handful of fold lines: (angle, offset, strength).
    let folds: [(f32, f32, f32); 6] = [
        (0.35, 0.30, 1.0),
        (-0.55, 0.62, 0.8),
        (1.05, 0.15, 0.7),
        (0.10, 0.78, 0.9),
        (-1.15, 0.45, 0.6),
        (0.62, 0.92, 0.7),
    ];
    let mut px = Vec::with_capacity(W * H);
    for y in 0..H {
        for x in 0..W {
            let u = x as f32 / W as f32;
            let v = y as f32 / H as f32;
            let mut b = 9.0_f32; // base ~ #090B0C
            for (i, (a, off, k)) in folds.iter().enumerate() {
                let (s, c) = a.sin_cos();
                let d = (u - 0.5) * c * 1.6 + (v - 0.5) * s - (off - 0.5) * 0.9;
                // Facet shading: each side of a fold is a slightly different plane.
                let side = 1.0 / (1.0 + (-d * 26.0).exp());
                b += side * 6.0 * k * if i % 2 == 0 { 1.0 } else { -0.6 };
                // Soft crease highlight along the fold.
                b += (-(d * 46.0).powi(2)).exp() * 13.0 * k;
            }
            // Glow drifting from the upper right, vignette elsewhere.
            let g = (-(((u - 0.82).powi(2) * 2.2) + ((v - 0.18).powi(2) * 3.4)) * 5.0).exp();
            b += g * 16.0;
            let vig = 1.0 - 0.55 * (((u - 0.5).powi(2) + (v - 0.5).powi(2)) * 1.8);
            let n = ((x * 7 + y * 13) % 5) as f32 * 0.25; // tiny dither, avoids banding
            let val = ((b * vig).clamp(5.0, 62.0) + n) as u8;
            px.push(egui::Color32::from_rgb(val, (val as f32 * 1.03) as u8, (val as f32 * 1.05) as u8));
        }
    }
    ctx.load_texture(
        "ws_bg",
        ColorImage { size: [W, H], source_size: egui::vec2(W as f32, H as f32), pixels: px },
        TextureOptions::LINEAR,
    )
}
