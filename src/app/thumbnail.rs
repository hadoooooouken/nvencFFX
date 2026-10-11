//! Frame preview above the trim slider while the mouse button is held on it
//! (`_on_slider_click`, `_schedule_thumbnail_preview`, `_show_thumbnail_preview`,
//! `_hide_thumbnail_preview`).

use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};
use eframe::egui::{
    self, Align2, ColorImage, CornerRadius, FontId, Rect, TextureHandle, TextureOptions, Vec2, pos2, vec2,
};
use crate::core::thumbnail::{self, Frame};
use crate::ui::theme::colors;

/// Drags ask for a frame once the handle rests this long (`delay=200`).
const DEBOUNCE: Duration = Duration::from_millis(200);
/// The preview box (`352x198`), 10 px above the slider.
const BOX: Vec2 = vec2(352.0, 198.0);
const GAP: f32 = 10.0;

/// A frame request: where the box goes (x of its center) and which time to show.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Request {
    x: f32,
    seconds: f64,
}

pub(super) struct Thumbnail {
    /// The mouse button is down on the slider.
    held: bool,
    slider: Rect,
    /// The next frame to grab, and from when.
    wanted: Option<(Request, Instant)>,
    /// The frame being grabbed.
    pending: Option<(Request, Receiver<Option<Frame>>)>,
    /// What the box shows: the frame, or `None` for "Preview unavailable".
    shown: Option<(f32, Option<TextureHandle>)>,
}

impl Default for Thumbnail {
    fn default() -> Self {
        Self { held: false, slider: Rect::NOTHING, wanted: None, pending: None, shown: None }
    }
}

impl Thumbnail {
    /// Called every frame with the slider: a press asks for a frame at once, a drag
    /// after [`DEBOUNCE`], a release hides the box.
    pub fn track(&mut self, slider: Rect, held: bool, moved: bool, x: f32, seconds: f64) {
        let now = Instant::now();
        self.slider = slider;
        let request = Request { x, seconds };
        if held && !self.held {
            self.wanted = Some((request, now));
        } else if held && moved {
            self.wanted = Some((request, now + DEBOUNCE));
        } else if !held && self.held {
            self.wanted = None;
            self.shown = None;
        }
        self.held = held;
    }

    /// Starts due requests (one ffmpeg at a time) and takes finished frames.
    pub fn poll(&mut self, ctx: &egui::Context, ffmpeg: &str, input: &str) {
        if let Some((request, rx)) = &self.pending
            && let Ok(frame) = rx.try_recv()
        {
            let x = request.x;
            self.pending = None;
            if self.held {
                let texture = frame.map(|f| {
                    let image = ColorImage::from_rgba_unmultiplied([f.width, f.height], &f.rgba);
                    ctx.load_texture("trim_thumbnail", image, TextureOptions::LINEAR)
                });
                self.shown = Some((x, texture));
            }
        }
        let Some((request, due)) = self.wanted else { return };
        if !self.held || input.is_empty() || ffmpeg.is_empty() {
            self.wanted = None;
            return;
        }
        let now = Instant::now();
        if now < due {
            ctx.request_repaint_after(due - now);
            return;
        }
        if self.pending.is_some() {
            return;
        }
        self.wanted = None;
        let (tx, rx) = channel();
        let (ffmpeg, input, repaint) = (ffmpeg.to_owned(), input.to_owned(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(thumbnail::grab(&ffmpeg, &input, request.seconds));
            repaint.request_repaint();
        });
        self.pending = Some((request, rx));
    }

    /// The box above the slider, centered on the requested position and kept inside
    /// the window; below the slider if there is no room above.
    pub fn show(&self, ctx: &egui::Context) {
        let Some((x, texture)) = &self.shown else { return };
        let window = ctx.content_rect();
        let left = (x - BOX.x / 2.0).clamp(window.min.x, (window.max.x - BOX.x).max(window.min.x));
        let mut top = self.slider.min.y - GAP - BOX.y;
        if top < window.min.y {
            top = self.slider.max.y + GAP;
        }
        let rect = Rect::from_min_size(pos2(left, top), BOX);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("trim_thumbnail")));
        painter.rect_filled(rect, CornerRadius::ZERO, colors::SECONDARY_BG);
        match texture {
            // Fitted into the box, keeping the video's aspect ratio
            Some(texture) => {
                let size = texture.size_vec2();
                let scale = (BOX.x / size.x).min(BOX.y / size.y);
                let image = Rect::from_center_size(rect.center(), size * scale);
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                painter.image(texture.id(), image, uv, egui::Color32::WHITE);
            }
            None => {
                painter.text(rect.center(), Align2::CENTER_CENTER, "Preview\nunavailable", FontId::proportional(13.0), colors::LABEL_TEXT);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_drag_and_release() {
        let ctx = egui::Context::default();
        let slider = Rect::from_min_size(pos2(0.0, 300.0), vec2(672.0, 30.0));
        let mut t = Thumbnail::default();
        t.track(slider, true, false, 100.0, 5.0);
        let (request, due) = t.wanted.unwrap();
        assert_eq!(request, Request { x: 100.0, seconds: 5.0 });
        assert!(due <= Instant::now(), "a press asks at once");

        t.track(slider, true, true, 200.0, 10.0);
        assert!(t.wanted.unwrap().1 > Instant::now(), "a drag waits");
        t.poll(&ctx, "", "a.mp4");
        assert!(t.wanted.is_none() && t.pending.is_none(), "no ffmpeg, no preview");

        t.shown = Some((200.0, None));
        t.track(slider, false, false, 200.0, 10.0);
        assert!(t.shown.is_none() && t.wanted.is_none());

        // A frame that arrives after the release is dropped
        let (tx, rx) = channel();
        t.pending = Some((Request { x: 1.0, seconds: 1.0 }, rx));
        tx.send(None).unwrap();
        t.poll(&ctx, "ffmpeg", "a.mp4");
        assert!(t.pending.is_none() && t.shown.is_none());
    }
}
