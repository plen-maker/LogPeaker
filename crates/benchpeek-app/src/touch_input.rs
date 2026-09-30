//! Reads the board's built-in ILI210x touchscreen (`/dev/input/touchscreen0`,
//! a stable symlink to whichever `eventN` it enumerates as) and turns it into
//! egui pointer events, for the [`crate::drm_backend`] path where there's no
//! Wayland/X11 compositor to do this for us.
//!
//! v0: single-touch only, mapped to egui's primary pointer button. The
//! ILI210x is a Type B (multi-touch protocol) device - `ABS_MT_TRACKING_ID`
//! going to -1 is how it reports "finger lifted", not a `BTN_TOUCH` release
//! (some panels send both; this only relies on tracking-id, which this one
//! is confirmed to send).

use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use evdev::{AbsoluteAxisCode, Device, EventSummary};

/// Spawns a background thread reading the touchscreen and returns a receiver
/// of raw `(x, y, pressed)` samples in the device's own coordinate space -
/// scaling to screen pixels happens in [`TouchInput::poll`], since that's
/// where the current screen size is known.
pub struct TouchInput {
    rx: Receiver<TouchSample>,
    x_range: (i32, i32),
    y_range: (i32, i32),
    last: Option<TouchSample>,
}

#[derive(Clone, Copy, Debug)]
struct TouchSample {
    x: i32,
    y: i32,
    pressed: bool,
}

impl TouchInput {
    /// Opens the touchscreen and starts reading it in the background.
    /// Returns `Ok(None)` (not an error) if no touchscreen is present, so
    /// callers on boards/setups without one just get no touch input rather
    /// than a hard failure.
    pub fn open() -> anyhow::Result<Option<Self>> {
        let path = "/dev/input/touchscreen0";
        if !std::path::Path::new(path).exists() {
            return Ok(None);
        }
        let device = Device::open(path)?;

        let mut x_range = (0, 1);
        let mut y_range = (0, 1);
        for (code, info) in device.get_absinfo()? {
            match code {
                AbsoluteAxisCode::ABS_MT_POSITION_X => {
                    x_range = (info.minimum(), info.maximum())
                }
                AbsoluteAxisCode::ABS_MT_POSITION_Y => {
                    y_range = (info.minimum(), info.maximum())
                }
                _ => {}
            }
        }

        let (tx, rx): (Sender<TouchSample>, Receiver<TouchSample>) = std::sync::mpsc::channel();
        thread::Builder::new()
            .name("touchscreen".into())
            .spawn(move || read_loop(device, tx))?;

        Ok(Some(Self {
            rx,
            x_range,
            y_range,
            last: None,
        }))
    }

    /// Drains any touch samples received since the last call and appends the
    /// corresponding egui pointer events, scaled into `screen_size` pixels.
    pub fn poll(&mut self, screen_size: egui::Vec2, events_out: &mut Vec<egui::Event>) {
        while let Ok(sample) = self.rx.try_recv() {
            let was_pressed = self.last.is_some_and(|s| s.pressed);
            let pos = egui::pos2(
                remap(sample.x, self.x_range, 0.0..=screen_size.x),
                remap(sample.y, self.y_range, 0.0..=screen_size.y),
            );

            events_out.push(egui::Event::PointerMoved(pos));
            if sample.pressed && !was_pressed {
                events_out.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            } else if !sample.pressed && was_pressed {
                events_out.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
                events_out.push(egui::Event::PointerGone);
            }
            self.last = Some(sample);
        }
    }
}

fn remap(value: i32, from: (i32, i32), to: std::ops::RangeInclusive<f32>) -> f32 {
    let (from_min, from_max) = (from.0 as f32, from.1 as f32);
    let span = (from_max - from_min).max(1.0);
    let t = (value as f32 - from_min) / span;
    *to.start() + t.clamp(0.0, 1.0) * (*to.end() - *to.start())
}

fn read_loop(mut device: Device, tx: Sender<TouchSample>) {
    let mut x = 0;
    let mut y = 0;
    let mut pressed = false;
    loop {
        let events = match device.fetch_events() {
            Ok(events) => events,
            Err(_) => return, // device gone (unplugged, etc.) - stop quietly
        };
        let mut dirty = false;
        for event in events {
            match event.destructure() {
                EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_MT_POSITION_X, v) => {
                    x = v;
                    dirty = true;
                }
                EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_MT_POSITION_Y, v) => {
                    y = v;
                    dirty = true;
                }
                EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_MT_TRACKING_ID, v) => {
                    pressed = v != -1;
                    dirty = true;
                }
                _ => {}
            }
        }
        if dirty && tx.send(TouchSample { x, y, pressed }).is_err() {
            return; // receiver dropped - benchpeek is shutting down
        }
    }
}
