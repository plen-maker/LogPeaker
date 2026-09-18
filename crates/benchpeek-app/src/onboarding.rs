//! MB1605C animation: full board transitions to transparent context around CN15 USB OTG.
//! Connector identities: ST UM3385 rev 2, figure 4 (CN21 ST-LINK, CN15 USB3).
use eframe::egui::{self, Color32, Rect, Vec2};
use std::time::Instant;

const MOVIE: &[u8] = include_bytes!("../assets/board_intro.bpk");
pub(crate) const FRAME_COUNT: usize = 480;
pub(crate) const FPS: f32 = 60.0;

pub struct Welcome {
    pub visible: bool,
    start: Instant,
    reduced_motion: bool,
    texture: Option<egui::TextureHandle>,
    loaded_frame: Option<usize>,
    image_error: bool,
}
impl Default for Welcome {
    fn default() -> Self {
        Self {
            visible: true,
            start: Instant::now(),
            reduced_motion: false,
            texture: None,
            loaded_frame: None,
            image_error: false,
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
            FRAME_COUNT as f32 / FPS
        } else {
            self.start.elapsed().as_secs_f32()
        };
        let h = (ui.available_height() - 165.0).max(130.0);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), h), egui::Sense::hover());
        let frame = ((t * FPS) as usize).min(FRAME_COUNT - 1);
        if self.loaded_frame != Some(frame) {
            self.loaded_frame = Some(frame);
            match decode_frame(frame) {
                Ok(image) => {
                    if let Some(texture) = &mut self.texture {
                        texture.set(image, egui::TextureOptions::LINEAR);
                    } else {
                        self.texture = Some(ui.ctx().load_texture(
                            "board_intro",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    self.image_error = false;
                }
                Err(()) => self.image_error = true,
            }
        }
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0, Color32::BLACK);
        if let Some(texture) = &self.texture {
            let scale = (rect.width() / 960.0).min(rect.height() / 540.0);
            let image_rect = Rect::from_center_size(rect.center(), Vec2::new(960.0, 540.0) * scale);
            painter.image(
                texture.id(),
                image_rect,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        if self.image_error {
            ui.label("The connection animation could not be loaded. Connect USB-C to CN15.");
        }
        if !self.reduced_motion && frame < FRAME_COUNT - 1 {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f32(1.0 / FPS));
        }
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label("Connect your computer to CN15 — USB-C OTG / device.");
            ui.weak("STM32MP257F-DK · CN15 connection guide");
            if ports.is_empty() {
                ui.label("Waiting for a USB serial device…");
            } else {
                ui.label(format!("USB serial port available: {}", ports.join(", ")));
            }
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - 350.0) / 2.0).max(0.0));
                if ui.button("Enable serial auto-connect").clicked() {
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
// Pack layout: BPK2, frame count (u32 LE), N pairs of absolute offset and length (u32 LE), JPEG payloads.
pub(crate) fn decode_frame(frame: usize) -> Result<egui::ColorImage, ()> {
    let word = |at: usize| -> Result<usize, ()> {
        let bytes: [u8; 4] = MOVIE
            .get(at..at + 4)
            .ok_or(())?
            .try_into()
            .map_err(|_| ())?;
        Ok(u32::from_le_bytes(bytes) as usize)
    };
    if MOVIE.get(..4) != Some(b"BPK2") || word(4)? != FRAME_COUNT || frame >= FRAME_COUNT {
        return Err(());
    }
    let start = word(8 + frame * 8)?;
    let end = start.checked_add(word(12 + frame * 8)?).ok_or(())?;
    let encoded = MOVIE.get(start..end).ok_or(())?;
    let image = image::load_from_memory_with_format(encoded, image::ImageFormat::Jpeg)
        .map_err(|_| ())?
        .into_rgb8();
    Ok(egui::ColorImage::from_rgb(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_movie_decodes_every_frame() {
        for frame in 0..FRAME_COUNT {
            assert_eq!(
                decode_frame(frame).unwrap().size,
                [960, 540],
                "frame {frame}"
            );
        }
        assert!(decode_frame(FRAME_COUNT).is_err());
        assert!(decode_frame(usize::MAX).is_err());
    }

    #[test]
    fn reduced_motion_holds_final_frame_and_replay_restarts() {
        let ctx = egui::Context::default();
        let mut welcome = Welcome::default();
        welcome.reduced_motion = true;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            welcome.ui(ui, &[]);
        });
        output.textures_delta.clear();
        assert_eq!(welcome.loaded_frame, Some(FRAME_COUNT - 1));
        assert!(!welcome.image_error);
        assert!(welcome.texture.is_some());
        welcome.reduced_motion = false;
        welcome.visible = false;
        welcome.replay();
        assert!(welcome.visible);
        assert!(welcome.start.elapsed().as_secs_f32() < 1.0);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            welcome.ui(ui, &[]);
        });
        output.textures_delta.clear();
        assert!(welcome.loaded_frame.unwrap() < 60);
        assert!(!welcome.image_error);
    }
}
