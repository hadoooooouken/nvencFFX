//! In-window replacements for Tk's `messagebox` / `simpledialog`, and the
//! self-drawn title bar of secondary windows.

use eframe::egui::{
    self, Align, Align2, CornerRadius, FontId, Id, Key, Layout, Margin, Modal, Rect, Response,
    RichText, Sense, Stroke, Ui, ViewportCommand, pos2, vec2,
};
use super::{
    theme::{colors, dimensions},
    widgets::{ButtonKind, EntryStyle, button, entry},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Info,
    Warning,
    Error,
    /// Yes / No
    Question,
    /// Text entry with OK / Cancel
    Prompt,
}

pub enum Outcome {
    /// OK / Yes; carries the entered text for a prompt.
    Accepted(String),
    Rejected,
}

/// A modal message box. `A` tags what to do with the answer.
pub struct Dialog<A> {
    pub kind: Kind,
    pub title: String,
    pub message: String,
    pub input: String,
    pub action: A,
}

impl<A> Dialog<A> {
    pub fn new(kind: Kind, title: impl Into<String>, message: impl Into<String>, action: A) -> Self {
        Self { kind, title: title.into(), message: message.into(), input: String::new(), action }
    }

    /// Draws the dialog over `ui`'s viewport; returns the answer once given.
    pub fn show(&mut self, ui: &Ui) -> Option<Outcome> {
        let accent = match self.kind {
            Kind::Error => colors::ACCENT_RED,
            Kind::Warning => colors::ACCENT_ORANGE,
            _ => colors::ACCENT_GREEN,
        };
        let frame = egui::Frame::new()
            .fill(colors::SECONDARY_BG)
            .stroke(Stroke::new(1.0_f32, colors::ENTRY_BORDER))
            .corner_radius(CornerRadius::same(dimensions::CORNER_RADIUS))
            .inner_margin(Margin::same(16));
        let mut outcome = None;
        let response = Modal::new(Id::new("nvencffx_dialog"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(140))
            .show(ui.ctx(), |ui| {
                ui.set_width(380.0);
                ui.spacing_mut().item_spacing = vec2(0.0, 10.0);
                ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    ui.painter().circle_filled(dot.center(), 5.0, accent);
                    ui.add_space(6.0);
                    ui.label(RichText::new(&self.title).size(15.0).color(colors::TEXT_COLOR_W));
                });
                ui.add(egui::Label::new(&self.message).wrap());

                if self.kind == Kind::Prompt {
                    let r = entry(ui, "dialog_input", vec2(380.0, dimensions::WIDGET_HEIGHT),
                        &mut self.input, EntryStyle::default());
                    if !r.has_focus() && !r.lost_focus() {
                        r.request_focus();
                    }
                }

                let size = vec2(80.0, dimensions::WIDGET_HEIGHT);
                let (ok, cancel) = match self.kind {
                    Kind::Question => ("Yes", Some("No")),
                    Kind::Prompt => ("OK", Some("Cancel")),
                    _ => ("OK", None),
                };
                ui.add_space(4.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                    if let Some(cancel) = cancel
                        && button(ui, size, cancel, ButtonKind::Grey).clicked()
                    {
                        outcome = Some(Outcome::Rejected);
                    }
                    if button(ui, size, ok, ButtonKind::Green).clicked() {
                        outcome = Some(Outcome::Accepted(self.input.clone()));
                    }
                });
            });
        if outcome.is_none() {
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                outcome = Some(Outcome::Accepted(self.input.clone()));
            } else if response.should_close() {
                outcome = Some(Outcome::Rejected);
            }
        }
        outcome
    }
}

pub const TITLE_BAR_HEIGHT: f32 = 32.0;

/// Title bar for an undecorated window: drag to move, painted close button, 1px
/// outline around the window. Returns true when close was clicked.
pub fn title_bar(ui: &mut Ui, title: &str) -> bool {
    let window = ui.max_rect();
    let bar = Rect::from_min_size(window.min, vec2(window.width(), TITLE_BAR_HEIGHT));
    let close_rect = Rect::from_min_max(pos2(bar.max.x - 46.0, bar.min.y), bar.max);

    let drag: Response = ui.interact(bar.with_max_x(close_rect.min.x), ui.id().with("title_bar"), Sense::click_and_drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    let close = ui.interact(close_rect, ui.id().with("title_close"), Sense::click());

    let painter = ui.painter();
    painter.rect_filled(bar, CornerRadius::ZERO, colors::SECONDARY_BG);
    painter.text(
        pos2(bar.min.x + 12.0, bar.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(dimensions::FONT_SIZE),
        colors::LABEL_TEXT,
    );
    if close.hovered() {
        painter.rect_filled(close_rect, CornerRadius::ZERO, colors::HOVER_RED);
    }
    let c = close_rect.center();
    let x = Stroke::new(1.2_f32, colors::LABEL_TEXT);
    painter.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], x);
    painter.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], x);
    painter.rect_stroke(window, CornerRadius::ZERO, Stroke::new(1.0_f32, colors::ENTRY_BORDER), egui::StrokeKind::Inside);

    ui.advance_cursor_after_rect(bar);
    close.clicked()
}
