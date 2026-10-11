//! CustomTkinter look-alike widgets. Every widget takes its exact size (or derives it
//! from its label) so screens can be laid out on the same pixel grid as `nvencFFX.py`.

use std::path::PathBuf;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Galley, Layout, Popup, PopupCloseBehavior,
    Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind, TextEdit, Ui, UiBuilder, Vec2,
    pos2, vec2,
};
use std::sync::Arc;
use super::theme::{colors, dimensions};

// ---------------------------------------------------------------------------
// Layout helpers
// ---------------------------------------------------------------------------

/// Reserves a full-width strip of `height` in the current vertical flow.
pub fn row(ui: &mut Ui, height: f32) -> Rect {
    ui.allocate_space(vec2(ui.available_width(), height)).1
}

/// Rect at `(x, y)` inside `parent`, sized `w` x `h`.
pub fn sub_rect(parent: Rect, x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(parent.min + vec2(x, y), vec2(w, h))
}

/// Full-height cell of a [`row`] starting at `x` with width `w`.
pub fn cell(row: Rect, x: f32, w: f32) -> Rect {
    sub_rect(row, x, 0.0, w, row.height())
}

/// Runs `add` in a child ui confined to `rect`, laid out left-to-right and
/// vertically centered. Does not move the parent's cursor.
pub fn place<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    add(&mut child)
}

/// Reserves a row and paints a CTkFrame (`SECONDARY_BG`, rounded) at `x` inside it.
/// Returns the frame rect for placing contents.
pub fn section_frame(ui: &mut Ui, x: f32, width: f32, height: f32) -> Rect {
    let frame = cell(row(ui, height), x, width);
    ui.painter()
        .rect_filled(frame, CornerRadius::same(dimensions::CORNER_RADIUS), colors::SECONDARY_BG);
    frame
}

fn body_font() -> FontId {
    FontId::proportional(dimensions::FONT_SIZE)
}

fn layout_text(ui: &Ui, text: &str, font: FontId, color: Color32) -> Arc<Galley> {
    ui.painter().layout_no_wrap(text.to_owned(), font, color)
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// Plain CTkLabel text.
pub fn label(ui: &mut Ui, text: &str) -> Response {
    ui.label(text)
}

/// Label in a custom color (placeholder grey, status green, ...).
pub fn colored_label(ui: &mut Ui, text: &str, color: Color32) -> Response {
    ui.label(RichText::new(text).color(color))
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    Green,
    Grey,
    Red,
    Orange,
    /// `state="disabled"` with a `SECONDARY_BG` fill; ignores clicks.
    Disabled,
}

impl ButtonKind {
    fn fill(self, hovered: bool) -> Color32 {
        match (self, hovered) {
            (Self::Disabled, _) => colors::SECONDARY_BG,
            (Self::Green, false) => colors::ACCENT_GREEN,
            (Self::Green, true) => colors::HOVER_GREEN,
            (Self::Grey, false) => colors::ACCENT_GREY,
            (Self::Grey, true) => colors::HOVER_GREY,
            (Self::Red, false) => colors::ACCENT_RED,
            (Self::Red, true) => colors::HOVER_RED,
            (Self::Orange, false) => colors::ACCENT_ORANGE,
            (Self::Orange, true) => colors::HOVER_ORANGE,
        }
    }

    fn text_color(self) -> Color32 {
        if self == Self::Disabled { colors::TEXT_DISABLED } else { colors::TEXT_COLOR_B }
    }
}

fn paint_button(ui: &mut Ui, size: Vec2, kind: ButtonKind) -> (Rect, Response) {
    let sense = if kind == ButtonKind::Disabled { Sense::hover() } else { Sense::click() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(dimensions::CORNER_RADIUS),
            kind.fill(response.hovered()),
        );
    }
    (rect, response)
}

/// CTkButton: flat rounded fill, black 13px text.
pub fn button(ui: &mut Ui, size: Vec2, text: &str, kind: ButtonKind) -> Response {
    let (rect, response) = paint_button(ui, size, kind);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, body_font(), kind.text_color());
    response
}

/// CTkButton with `font=("", 14, "bold")` (the Convert button).
pub fn bold_button(ui: &mut Ui, size: Vec2, text: &str, kind: ButtonKind) -> Response {
    let (rect, response) = paint_button(ui, size, kind);
    let font = FontId::proportional(14.0);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, kind.text_color());
    response
}

/// Square grey button with a painted "reset" (counter-clockwise arrow) icon.
pub fn reset_button(ui: &mut Ui, size: Vec2) -> Response {
    let (rect, response) = paint_button(ui, size, ButtonKind::Grey);
    let c = rect.center();
    let r = 5.5;
    let stroke = Stroke::new(1.4_f32, colors::TEXT_COLOR_B);
    let arc: Vec<Pos2> = (0..=24)
        .map(|i| {
            let a = (-60.0_f32 + 300.0 * i as f32 / 24.0).to_radians();
            pos2(c.x + r * a.cos(), c.y - r * a.sin())
        })
        .collect();
    let tip = *arc.last().unwrap();
    ui.painter().add(Shape::line(arc, stroke));
    ui.painter().add(Shape::convex_polygon(
        vec![tip + vec2(-3.0, -2.5), tip + vec2(3.0, -2.0), tip + vec2(0.5, 2.5)],
        colors::TEXT_COLOR_B,
        Stroke::NONE,
    ));
    response
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
pub struct EntryStyle<'a> {
    pub placeholder: &'a str,
    pub centered: bool,
    pub disabled: bool,
}

/// CTkEntry: `SECONDARY_BG` fill, 2px `#565B5E` border, white text.
/// Disabled entries turn grey like the original's custom FPS / width / bitrate fields.
pub fn entry(
    ui: &mut Ui,
    id_salt: &str,
    size: Vec2,
    value: &mut String,
    style: EntryStyle<'_>,
) -> Response {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let border = dimensions::ENTRY_BORDER;
    let fill = if style.disabled { colors::ACCENT_GREY } else { colors::SECONDARY_BG };
    ui.painter().rect(
        rect,
        CornerRadius::same(dimensions::CORNER_RADIUS),
        fill,
        Stroke::new(border, colors::ENTRY_BORDER),
        StrokeKind::Inside,
    );

    let text_rect = rect.shrink2(vec2(8.0, border + 1.0));
    let mut edit = TextEdit::singleline(value)
        .id_salt(id_salt)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .font(body_font())
        .vertical_align(Align::Center)
        .desired_width(text_rect.width())
        .interactive(!style.disabled)
        .text_color(if style.disabled { colors::PLACEHOLDER } else { colors::TEXT_COLOR_W });
    if !style.placeholder.is_empty() {
        edit = edit.hint_text(RichText::new(style.placeholder).color(colors::PLACEHOLDER));
    }
    if style.centered {
        edit = edit.horizontal_align(Align::Center);
    }
    ui.put(text_rect, edit)
}

// ---------------------------------------------------------------------------
// Check box / radio button
// ---------------------------------------------------------------------------

fn control_with_label(ui: &mut Ui, box_size: f32, text: &str) -> (Rect, Rect, Response, Arc<Galley>) {
    let galley = layout_text(ui, text, body_font(), colors::LABEL_TEXT);
    let size = vec2(
        box_size + dimensions::CONTROL_TEXT_GAP + galley.size().x,
        box_size.max(galley.size().y),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let box_rect = Rect::from_center_size(
        pos2(rect.min.x + box_size / 2.0, rect.center().y),
        Vec2::splat(box_size),
    );
    (rect, box_rect, response, galley)
}

fn paint_control_label(ui: &Ui, box_rect: Rect, galley: Arc<Galley>) {
    let pos = pos2(
        box_rect.max.x + dimensions::CONTROL_TEXT_GAP,
        box_rect.center().y - galley.size().y / 2.0,
    );
    ui.painter().galley(pos, galley, colors::LABEL_TEXT);
}

/// CTkCheckBox: 24px rounded box, 3px grey border; filled green with a checkmark
/// when on. Hovering fills the box with the hover green, like CTk does.
pub fn checkbox(ui: &mut Ui, value: &mut bool, text: &str) -> Response {
    checkbox_enabled(ui, value, text, true)
}

/// [`checkbox`] that can be disabled: grey border and text, not clickable.
pub fn checkbox_enabled(ui: &mut Ui, value: &mut bool, text: &str, enabled: bool) -> Response {
    let (_, box_rect, mut response, mut galley) =
        control_with_label(ui, dimensions::CHECKBOX_SIZE, text);
    if enabled && response.clicked() {
        *value = !*value;
        response.mark_changed();
    }

    let painter = ui.painter();
    let rounding = CornerRadius::same(dimensions::CORNER_RADIUS);
    if !enabled {
        painter.rect_stroke(
            box_rect,
            rounding,
            Stroke::new(dimensions::CHECKBOX_BORDER, colors::ENTRY_BORDER),
            StrokeKind::Inside,
        );
        galley = layout_text(ui, text, body_font(), colors::TEXT_DISABLED);
    } else if response.hovered() {
        painter.rect_filled(box_rect, rounding, colors::HOVER_GREEN);
    } else if *value {
        painter.rect_filled(box_rect, rounding, colors::ACCENT_GREEN);
    } else {
        painter.rect_stroke(
            box_rect,
            rounding,
            Stroke::new(dimensions::CHECKBOX_BORDER, colors::CONTROL_BORDER),
            StrokeKind::Inside,
        );
    }
    if *value {
        let o = box_rect.min;
        painter.add(Shape::line(
            vec![o + vec2(6.0, 12.5), o + vec2(10.0, 16.5), o + vec2(18.0, 7.5)],
            Stroke::new(2.6_f32, colors::CHECKMARK),
        ));
    }
    paint_control_label(ui, box_rect, galley);
    response
}

/// CTkRadioButton: 22px circle, 3px grey ring when off, 6px green ring when on.
pub fn radio(ui: &mut Ui, selected: bool, text: &str) -> Response {
    let (_, box_rect, response, galley) = control_with_label(ui, dimensions::RADIO_SIZE, text);
    let (width, color) = match (selected, response.hovered()) {
        (true, false) => (dimensions::RADIO_BORDER_CHECKED, colors::ACCENT_GREEN),
        (true, true) => (dimensions::RADIO_BORDER_CHECKED, colors::HOVER_GREEN),
        (false, false) => (dimensions::RADIO_BORDER_UNCHECKED, colors::CONTROL_BORDER),
        (false, true) => (dimensions::RADIO_BORDER_UNCHECKED, colors::HOVER_GREEN),
    };
    let radius = dimensions::RADIO_SIZE / 2.0 - width / 2.0;
    ui.painter().circle_stroke(box_rect.center(), radius, Stroke::new(width, color));
    paint_control_label(ui, box_rect, galley);
    response
}

/// Radio button bound to a string option; selecting it writes `value` into `current`.
pub fn radio_value(ui: &mut Ui, current: &mut String, value: &str, text: &str) -> Response {
    let mut response = radio(ui, current == value, text);
    if response.clicked() && current != value {
        *current = value.to_owned();
        response.mark_changed();
    }
    response
}

// ---------------------------------------------------------------------------
// Option menu
// ---------------------------------------------------------------------------

/// CTkOptionMenu: dark field + green arrow button, dropdown list with green hover.
pub fn option_menu(
    ui: &mut Ui,
    id_salt: &str,
    width: f32,
    value: &mut String,
    values: &[&str],
    enabled: bool,
) -> Response {
    let size = vec2(width, dimensions::WIDGET_HEIGHT);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, mut response) = ui.allocate_exact_size(size, sense);

    let r = dimensions::CORNER_RADIUS;
    let hovered = enabled && response.hovered();
    let (field, arrow_bg, text_color) = if enabled {
        let arrow = if hovered { colors::HOVER_GREEN } else { colors::ACCENT_GREEN };
        (colors::PRIMARY_BG, arrow, colors::LABEL_TEXT)
    } else {
        (colors::ACCENT_GREY, colors::ACCENT_GREY, colors::TEXT_DISABLED)
    };

    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(r), field);
    let arrow_rect = Rect::from_min_max(pos2(rect.max.x - rect.height(), rect.min.y), rect.max);
    painter.rect_filled(
        arrow_rect,
        CornerRadius { nw: 0, sw: 0, ne: r, se: r },
        arrow_bg,
    );
    let c = arrow_rect.center();
    painter.add(Shape::line(
        vec![c + vec2(-4.5, -2.0), c + vec2(0.0, 2.5), c + vec2(4.5, -2.0)],
        Stroke::new(2.0_f32, text_color),
    ));
    painter.with_clip_rect(rect.with_max_x(arrow_rect.min.x - 2.0)).text(
        pos2(rect.min.x + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        value.as_str(),
        body_font(),
        text_color,
    );

    let mut picked = None;
    Popup::menu(&response)
        .id(ui.make_persistent_id(("option_menu", id_salt)))
        .width(width)
        .close_behavior(PopupCloseBehavior::CloseOnClick)
        .show(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for &v in values {
                let (item, item_resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
                if item_resp.hovered() {
                    ui.painter()
                        .rect_filled(item, CornerRadius::same(4), colors::ACCENT_GREEN);
                }
                ui.painter().text(
                    pos2(item.min.x + 8.0, item.center().y),
                    Align2::LEFT_CENTER,
                    v,
                    body_font(),
                    colors::LABEL_TEXT,
                );
                if item_resp.clicked() {
                    picked = Some(v);
                }
            }
        });
        });

    if let Some(v) = picked
        && value != v
    {
        *value = v.to_owned();
        response.mark_changed();
    }
    response
}

// ---------------------------------------------------------------------------
// Section toggle (TextCheckbox)
// ---------------------------------------------------------------------------

/// The original's `TextCheckbox`: 24px green badge with a white ▲/▼ and a title.
pub fn text_checkbox(ui: &mut Ui, is_expanded: &mut bool, title: &str) -> Response {
    let badge = dimensions::BADGE_SIZE;
    let gap = 8.0;
    let galley = layout_text(ui, title, body_font(), colors::LABEL_TEXT);
    let size = vec2(badge + gap + galley.size().x, dimensions::WIDGET_HEIGHT);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    let response_hovered = response.hovered();
    if response.clicked() {
        *is_expanded = !*is_expanded;
        response.mark_changed();
    }

    let badge_rect = Rect::from_min_size(
        pos2(rect.min.x, rect.center().y - badge / 2.0),
        Vec2::splat(badge),
    );
    let fill = if response_hovered { colors::HOVER_GREEN } else { colors::ACCENT_GREEN };
    let painter = ui.painter();
    painter.rect_filled(badge_rect, CornerRadius::same(dimensions::CORNER_RADIUS), fill);

    let c = badge_rect.center();
    let (hw, hh) = (3.5, 3.0);
    let points = if *is_expanded {
        vec![pos2(c.x, c.y - hh), pos2(c.x + hw, c.y + hh), pos2(c.x - hw, c.y + hh)]
    } else {
        vec![pos2(c.x - hw, c.y - hh), pos2(c.x + hw, c.y - hh), pos2(c.x, c.y + hh)]
    };
    painter.add(Shape::convex_polygon(points, colors::TEXT_COLOR_W, Stroke::NONE));

    let text_pos = pos2(badge_rect.max.x + gap, rect.center().y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, colors::LABEL_TEXT);

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Trim slider
// ---------------------------------------------------------------------------

/// Two-handle range slider from the original's trim canvas: grey 3px track,
/// green selection bar and 16px green handles. `start`/`end` are fractions 0..=1.
pub fn trim_slider(ui: &mut Ui, size: Vec2, start: &mut f32, end: &mut f32) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let pad = 10.0;
    let x0 = rect.min.x + pad;
    let span = (rect.width() - 2.0 * pad).max(1.0);
    let y = rect.center().y;
    let to_x = |f: f32| x0 + f.clamp(0.0, 1.0) * span;
    let to_frac = |x: f32| ((x - x0) / span).clamp(0.0, 1.0);

    // Which handle is being dragged: 0 = start, 1 = end
    let drag_id = response.id.with("handle");
    if response.drag_started() {
        if let Some(p) = response.interact_pointer_pos() {
            let handle = if (p.x - to_x(*start)).abs() <= (p.x - to_x(*end)).abs() { 0u8 } else { 1u8 };
            ui.data_mut(|d| d.insert_temp(drag_id, handle));
        }
    }
    if response.dragged() {
        if let (Some(p), Some(handle)) =
            (response.interact_pointer_pos(), ui.data(|d| d.get_temp::<u8>(drag_id)))
        {
            let f = to_frac(p.x);
            if handle == 0 {
                *start = f.min(*end);
            } else {
                *end = f.max(*start);
            }
            response.mark_changed();
        }
    }

    let painter = ui.painter();
    painter.line_segment(
        [pos2(x0, y), pos2(x0 + span, y)],
        Stroke::new(3.0_f32, colors::TRIM_TRACK),
    );
    let (sx, ex) = (to_x(*start), to_x(*end));
    if sx < ex {
        painter.rect_filled(
            Rect::from_min_max(pos2(sx, y - 3.0), pos2(ex, y + 3.0)),
            CornerRadius::ZERO,
            colors::ACCENT_GREEN,
        );
    }
    for x in [sx, ex] {
        painter.circle_filled(pos2(x, y), 8.0, colors::ACCENT_GREEN);
    }
    response
}

/// CTkProgressBar: 8px pill, `SECONDARY_BG` track, green progress (`fraction` 0..=1).
pub fn progress_bar(ui: &mut Ui, width: f32, fraction: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 8.0), Sense::hover());
    let pill = CornerRadius::same(4);
    ui.painter().rect_filled(rect, pill, colors::SECONDARY_BG);
    let done = rect.with_max_x(rect.min.x + rect.width() * fraction.clamp(0.0, 1.0));
    if done.width() > 0.0 {
        ui.painter().rect_filled(done, pill, colors::ACCENT_GREEN);
    }
    response
}

// ---------------------------------------------------------------------------
// Drag & drop
// ---------------------------------------------------------------------------

/// Checks native egui drag-and-drop events and extracts dropped file paths
pub fn handle_dropped_files(ctx: &egui::Context) -> Option<Vec<PathBuf>> {
    let paths: Vec<PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|p| !p.as_os_str().is_empty())
            .collect()
    });
    (!paths.is_empty()).then_some(paths)
}

/// Returns true if files are currently being dragged over the application window
pub fn is_drag_hovering(ctx: &egui::Context) -> bool {
    ctx.input(|i| !i.raw.hovered_files.is_empty())
}

/// Renders a full-window drag-and-drop overlay when the user hovers files over the window
pub fn file_drag_drop_overlay(ctx: &egui::Context) {
    if !is_drag_hovering(ctx) {
        return;
    }

    let screen_rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("drag_drop_overlay"),
    ));
    painter.rect_filled(
        screen_rect,
        CornerRadius::ZERO,
        Color32::from_rgba_unmultiplied(14, 17, 19, 210),
    );
    painter.rect_stroke(
        screen_rect.shrink(20.0),
        CornerRadius::same(8),
        Stroke::new(2.5_f32, colors::ACCENT_GREEN),
        StrokeKind::Middle,
    );
    painter.text(
        screen_rect.center() - vec2(0.0, 12.0),
        Align2::CENTER_CENTER,
        "Drop video file to load into nvencFFX",
        FontId::proportional(20.0),
        colors::ACCENT_GREEN,
    );
    painter.text(
        screen_rect.center() + vec2(0.0, 16.0),
        Align2::CENTER_CENTER,
        "MP4, MKV, MOV, TS, AVI, WebM, ...",
        body_font(),
        colors::PLACEHOLDER,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drag_hovering_empty() {
        let ctx = egui::Context::default();
        assert!(!is_drag_hovering(&ctx));
        assert!(handle_dropped_files(&ctx).is_none());
    }

    #[test]
    fn test_text_checkbox_toggle() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let mut expanded = false;
            let _resp = text_checkbox(ui, &mut expanded, "Test Section");
            assert!(!expanded);
        });
        // No renderer here to upload the font atlas to
        output.textures_delta.clear();
    }

    #[test]
    fn test_cell_geometry() {
        let r = Rect::from_min_size(pos2(31.0, 100.0), vec2(756.0, 38.0));
        let c = cell(r, 226.0, 350.0);
        assert_eq!(c.min, pos2(257.0, 100.0));
        assert_eq!(c.size(), vec2(350.0, 38.0));
    }
}
