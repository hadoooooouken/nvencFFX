//! Custom-drawn widgets. Every widget takes its exact size (or derives it from its
//! label) so screens can be laid out on a fixed pixel grid.

use std::path::PathBuf;
use eframe::egui::{
    self, Align, Align2, Color32, Context, CornerRadius, FontId, Galley, Id, Layout, Popup,
    PopupCloseBehavior, Pos2, PointerButton, Rect, Response, RichText, Sense, Shape, Stroke,
    StrokeKind, TextEdit, Ui, UiBuilder, Vec2, pos2,
    text::{CCursor, CCursorRange},
    text_edit::TextEditState,
    vec2,
};
use std::sync::Arc;
use crate::config::presets::PresetItem;
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

/// Reserves a row and paints a frame (`SECONDARY_BG`, rounded) at `x` inside it.
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

/// Plain label text.
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

/// Button: flat rounded fill, black 13px text.
pub fn button(ui: &mut Ui, size: Vec2, text: &str, kind: ButtonKind) -> Response {
    let (rect, response) = paint_button(ui, size, kind);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, body_font(), kind.text_color());
    response
}

/// Button with 14px bold text (the Convert button).
pub fn bold_button(ui: &mut Ui, size: Vec2, text: &str, kind: ButtonKind) -> Response {
    let (rect, response) = paint_button(ui, size, kind);
    let font = FontId::proportional(14.0);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, kind.text_color());
    response
}

/// Button with a painted "×" (the batch list's remove button). Painted rather than
/// a text glyph, whose side bearings would put it off center.
pub fn cross_button(ui: &mut Ui, size: Vec2, kind: ButtonKind) -> Response {
    let (rect, response) = paint_button(ui, size, kind);
    let painter = ui.painter();
    let c = pos2(painter.round_to_pixel_center(rect.center().x), painter.round_to_pixel_center(rect.center().y));
    let (d, stroke) = (3.0, Stroke::new(1.5_f32, kind.text_color()));
    painter.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
    painter.line_segment([c + vec2(-d, d), c + vec2(d, -d)], stroke);
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

/// Entry: `SECONDARY_BG` fill, 2px `#565B5E` border, white text.
/// Disabled entries turn grey (custom FPS / width / bitrate fields).
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
    let id = ui.make_persistent_id(id_salt);
    let mut edit = TextEdit::singleline(value)
        .id(id)
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
    let selection = selection_before_right_click(ui.ctx(), id);
    let response = ui.put(text_rect, edit);
    if !style.disabled {
        text_context_menu(ui, &response, selection, value, false);
    }
    response
}

// ---------------------------------------------------------------------------
// Text context menu
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAction {
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
}

/// egui moves the text cursor on any pointer press, which would drop the selection
/// the menu acts on. Call before showing the `TextEdit` `id`; hand the result to
/// [`text_context_menu`].
pub fn selection_before_right_click(ctx: &Context, id: Id) -> Option<TextEditState> {
    let pressed = ctx.input(|i| i.pointer.button_pressed(PointerButton::Secondary));
    if pressed && ctx.read_response(id).is_some_and(|r| r.contains_pointer()) {
        TextEditState::load(ctx, id)
    } else {
        None
    }
}

/// Cut / Copy / Paste / Delete / Select All on a right click
/// into a text field. `selection` comes from [`selection_before_right_click`].
pub fn text_context_menu(
    ui: &Ui,
    response: &Response,
    selection: Option<TextEditState>,
    text: &mut String,
    multiline: bool,
) {
    if let Some(state) = selection {
        state.store(ui.ctx(), response.id);
    }
    const ITEMS: [(&str, TextAction); 5] = [
        ("Cut", TextAction::Cut),
        ("Copy", TextAction::Copy),
        ("Paste", TextAction::Paste),
        ("Delete", TextAction::Delete),
        ("Select All", TextAction::SelectAll),
    ];
    let mut chosen = None;
    Popup::context_menu(response)
        .frame(egui::Frame::new().fill(colors::SECONDARY_BG).corner_radius(10).inner_margin(3))
        .width(120.0)
        .show(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 6.0);
            for (label, action) in ITEMS {
                let (rect, item) = ui.allocate_exact_size(vec2(120.0, 28.0), Sense::click());
                if item.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), colors::HOVER_GREEN);
                }
                ui.painter().text(
                    pos2(rect.min.x + 8.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    body_font(),
                    colors::TEXT_COLOR_W,
                );
                if item.clicked() {
                    chosen = Some(action);
                }
            }
        });
    if let Some(action) = chosen {
        let clipboard = (action == TextAction::Paste)
            .then(|| arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok())
            .flatten();
        edit_text(ui.ctx(), response.id, text, action, clipboard.as_deref(), multiline);
    }
}

/// Runs a menu action on the `TextEdit` `id` and focuses it. Copy takes the whole
/// text if nothing is selected; Cut and Delete need a selection.
pub fn edit_text(
    ctx: &Context,
    id: Id,
    text: &mut String,
    action: TextAction,
    clipboard: Option<&str>,
    multiline: bool,
) {
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    let chars = text.chars().count();
    let range = state.cursor.char_range().unwrap_or_else(|| CCursorRange::one(CCursor::new(chars)));
    let sel = range.as_sorted_char_range();
    let (first, last) = (sel.start.0.min(chars), sel.end.0.min(chars));
    let byte = |c: usize| text.char_indices().nth(c).map_or(text.len(), |(b, _)| b);
    let bytes = byte(first)..byte(last);
    let cursor = match action {
        TextAction::SelectAll => CCursorRange::two(CCursor::new(0), CCursor::new(chars)),
        TextAction::Copy => {
            let copied = if bytes.is_empty() { text.as_str() } else { &text[bytes] };
            if !copied.is_empty() {
                ctx.copy_text(copied.to_owned());
            }
            range
        }
        TextAction::Cut | TextAction::Delete if bytes.is_empty() => range,
        TextAction::Cut | TextAction::Delete => {
            if action == TextAction::Cut {
                ctx.copy_text(text[bytes.clone()].to_owned());
            }
            text.replace_range(bytes, "");
            CCursorRange::one(CCursor::new(first))
        }
        TextAction::Paste => {
            let Some(pasted) = clipboard.filter(|t| !t.is_empty()) else { return };
            // As egui pastes: one line in single-line fields
            let pasted = if multiline { pasted.replace("\r\n", "\n") } else { pasted.replace(['\r', '\n'], " ") };
            text.replace_range(bytes, &pasted);
            CCursorRange::one(CCursor::new(first + pasted.chars().count()))
        }
    };
    state.cursor.set_char_range(Some(cursor));
    state.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
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

/// Checkbox: 24px rounded box, 3px grey border; filled green with a checkmark
/// when on. Hovering fills the box with the hover green.
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

/// Radio button: 22px circle, 3px grey ring when off, 6px green ring when on.
pub fn radio(ui: &mut Ui, selected: bool, text: &str) -> Response {
    let (_, box_rect, response, galley) = control_with_label(ui, dimensions::RADIO_SIZE, text);
    let (width, color) = match (selected, response.hovered()) {
        (true, false) => (dimensions::RADIO_BORDER_CHECKED, colors::ACCENT_GREEN),
        (true, true) => (dimensions::RADIO_BORDER_CHECKED, colors::HOVER_GREEN),
        (false, false) => (dimensions::RADIO_BORDER_UNCHECKED, colors::CONTROL_BORDER),
        (false, true) => (dimensions::RADIO_BORDER_UNCHECKED, colors::HOVER_GREEN),
    };
    // epaint strokes circles outside the radius: keep the ring within the 22px circle
    let radius = dimensions::RADIO_SIZE / 2.0 - width;
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

/// Option menu: dark field + green arrow button, dropdown list with green hover.
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

    if let Some(v) = picked {
        *value = v.to_owned();
        response.mark_changed();
    }
    response
}

/// Preset option menu opening upward with a collapsible category accordion and bottom search bar.
pub fn preset_menu(
    ui: &mut Ui,
    id_salt: &str,
    width: f32,
    selected_id: &mut String,
    items: &[PresetItem],
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
        vec![c + vec2(-4.5, 2.0), c + vec2(0.0, -2.5), c + vec2(4.5, 2.0)],
        Stroke::new(2.0_f32, text_color),
    ));

    let display_text = if let Some(item) = items.iter().find(|i| i.id == *selected_id) {
        item.display_label()
    } else {
        selected_id.clone()
    };

    painter.with_clip_rect(rect.with_max_x(arrow_rect.min.x - 2.0)).text(
        pos2(rect.min.x + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        &display_text,
        body_font(),
        text_color,
    );

    let popup_id = ui.make_persistent_id(("preset_menu", id_salt));
    let search_id = popup_id.with("preset_search");
    let open_group_id = popup_id.with("preset_open_group");

    // Empty string "" represents the root ("other") section so it never collides with a real "other" folder.
    let active_group_key = items
        .iter()
        .find(|i| i.id == *selected_id)
        .map(|i| i.group.to_lowercase());

    if enabled && response.clicked() {
        ui.data_mut(|d| {
            d.insert_temp(search_id, String::new());
            if let Some(ref g) = active_group_key {
                d.insert_temp(open_group_id, g.clone());
            } else {
                d.remove_temp::<String>(open_group_id);
            }
        });
    }

    let mut picked = None;
    if enabled {
        Popup::menu(&response)
            .id(popup_id)
            .align(egui::RectAlign::TOP_START)
            .width(width.max(220.0))
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;

                let prev_query: String = ui.data_mut(|d| d.get_temp(search_id).unwrap_or_default());

                let paint_clipped_text =
                    |ui: &Ui, row: Rect, x_offset: f32, text: &str, color: Color32| {
                        let clip = ui.clip_rect().intersect(row.with_max_x(row.max.x - 6.0));
                        ui.painter().with_clip_rect(clip).text(
                            pos2(row.min.x + x_offset, row.center().y),
                            Align2::LEFT_CENTER,
                            text,
                            body_font(),
                            color,
                        );
                    };

                let content_rect = ui.ctx().content_rect();
                let space_above = (rect.min.y - content_rect.min.y - 48.0).max(80.0);
                let max_list_h = 380.0_f32.min(space_above);

                let query = prev_query.trim().to_lowercase();
                if !query.is_empty() {
                    let matches: Vec<&PresetItem> = items
                        .iter()
                        .filter(|i| {
                            i.display_label().to_lowercase().contains(&query)
                                || i.id.to_lowercase().contains(&query)
                        })
                        .collect();

                    let content_h = if matches.is_empty() {
                        26.0
                    } else {
                        matches.len() as f32 * 24.0
                    };
                    let scroll_h = content_h.min(max_list_h);

                    egui::ScrollArea::vertical()
                        .id_salt("search_scroll")
                        .min_scrolled_height(scroll_h)
                        .max_height(scroll_h)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;

                            if matches.is_empty() {
                                let (item, _) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), 26.0),
                                    Sense::hover(),
                                );
                                paint_clipped_text(
                                    ui,
                                    item,
                                    12.0,
                                    "No presets found",
                                    colors::PLACEHOLDER,
                                );
                            } else {
                                for item in matches {
                                    let (row, resp) = ui
                                        .push_id(&item.id, |ui| {
                                            ui.allocate_exact_size(
                                                vec2(ui.available_width(), 24.0),
                                                Sense::click(),
                                            )
                                        })
                                        .inner;
                                    let is_selected = item.id == *selected_id;
                                    if resp.hovered() {
                                        ui.painter().rect_filled(
                                            row,
                                            CornerRadius::same(4),
                                            colors::ACCENT_GREEN,
                                        );
                                    } else if is_selected {
                                        ui.painter().rect_filled(
                                            row,
                                            CornerRadius::same(4),
                                            colors::SECONDARY_BG,
                                        );
                                    }
                                    let text_color = if resp.hovered() {
                                        colors::TEXT_COLOR_B
                                    } else {
                                        colors::LABEL_TEXT
                                    };
                                    let label = item.display_label();
                                    paint_clipped_text(ui, row, 12.0, &label, text_color);
                                    if resp.clicked() {
                                        picked = Some(item.id.clone());
                                        ui.close();
                                    }
                                }
                            }
                        });
                } else {
                    // (canonical_key, display_title, items)
                    let mut groups: Vec<(String, String, Vec<&PresetItem>)> = Vec::new();
                    for item in items {
                        let key = item.group.to_lowercase();
                        let display = if item.group.is_empty() {
                            "other".to_owned()
                        } else {
                            item.group.clone()
                        };
                        if let Some((_, _, list)) = groups.iter_mut().find(|(k, _, _)| k == &key) {
                            list.push(item);
                        } else {
                            groups.push((key, display, vec![item]));
                        }
                    }

                    let mut open_group: Option<String> =
                        ui.data_mut(|d| d.get_temp::<String>(open_group_id));

                    let content_h = if items.is_empty() {
                        26.0
                    } else {
                        let open_items = open_group
                            .as_deref()
                            .and_then(|og| groups.iter().find(|(k, _, _)| k == og))
                            .map_or(0, |(_, _, list)| list.len());
                        groups.len() as f32 * 26.0 + open_items as f32 * 24.0
                    };
                    let scroll_h = content_h.min(max_list_h);

                    egui::ScrollArea::vertical()
                        .id_salt("accordion_scroll")
                        .min_scrolled_height(scroll_h)
                        .max_height(scroll_h)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;

                            if items.is_empty() {
                                let (item, _) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), 26.0),
                                    Sense::hover(),
                                );
                                paint_clipped_text(
                                    ui,
                                    item,
                                    12.0,
                                    "No presets found",
                                    colors::PLACEHOLDER,
                                );
                                return;
                            }

                            for (group_key, group_display, group_items) in &groups {
                                let (header_row, header_resp) = ui
                                    .push_id(("group", group_key), |ui| {
                                        ui.allocate_exact_size(
                                            vec2(ui.available_width(), 26.0),
                                            Sense::click(),
                                        )
                                    })
                                    .inner;

                                if header_resp.clicked() {
                                    let was_open =
                                        open_group.as_deref() == Some(group_key.as_str());
                                    let next = if was_open { None } else { Some(group_key.clone()) };
                                    ui.data_mut(|d| {
                                        if let Some(ref g) = next {
                                            d.insert_temp(open_group_id, g.clone());
                                        } else {
                                            d.remove_temp::<String>(open_group_id);
                                        }
                                    });
                                    open_group = next;
                                    ui.ctx().request_repaint();
                                }

                                let is_open = open_group.as_deref() == Some(group_key.as_str());

                                if header_resp.hovered() {
                                    ui.painter().rect_filled(
                                        header_row,
                                        CornerRadius::same(4),
                                        colors::SECONDARY_BG,
                                    );
                                }

                                let c = pos2(header_row.min.x + 12.0, header_row.center().y);
                                let arrow_stroke = Stroke::new(1.8_f32, colors::ACCENT_GREEN);
                                if is_open {
                                    ui.painter().add(Shape::line(
                                        vec![
                                            c + vec2(-3.5, -2.0),
                                            c + vec2(0.0, 2.0),
                                            c + vec2(3.5, -2.0),
                                        ],
                                        arrow_stroke,
                                    ));
                                } else {
                                    ui.painter().add(Shape::line(
                                        vec![
                                            c + vec2(-2.0, -3.5),
                                            c + vec2(2.0, 0.0),
                                            c + vec2(-2.0, 3.5),
                                        ],
                                        arrow_stroke,
                                    ));
                                }

                                let title = format!("{} ({})", group_display, group_items.len());
                                paint_clipped_text(
                                    ui,
                                    header_row,
                                    24.0,
                                    &title,
                                    colors::ACCENT_GREEN,
                                );

                                if is_open {
                                    for item in group_items {
                                        let (row, resp) = ui
                                            .push_id(&item.id, |ui| {
                                                ui.allocate_exact_size(
                                                    vec2(ui.available_width(), 24.0),
                                                    Sense::click(),
                                                )
                                            })
                                            .inner;
                                        let is_selected = item.id == *selected_id;
                                        if resp.hovered() {
                                            ui.painter().rect_filled(
                                                row,
                                                CornerRadius::same(4),
                                                colors::ACCENT_GREEN,
                                            );
                                        } else if is_selected {
                                            ui.painter().rect_filled(
                                                row,
                                                CornerRadius::same(4),
                                                colors::SECONDARY_BG,
                                            );
                                        }

                                        let text_color = if resp.hovered() {
                                            colors::TEXT_COLOR_B
                                        } else {
                                            colors::LABEL_TEXT
                                        };
                                        paint_clipped_text(ui, row, 28.0, &item.name, text_color);

                                        if resp.clicked() {
                                            picked = Some(item.id.clone());
                                            ui.close();
                                        }
                                    }
                                }
                            }
                        });
                }

                let (sep_rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
                let sep_y = sep_rect.min.y + 0.5;
                ui.painter().line_segment(
                    [pos2(sep_rect.min.x, sep_y), pos2(sep_rect.max.x, sep_y)],
                    Stroke::new(1.0, colors::ENTRY_BORDER),
                );

                let mut search_query = prev_query.clone();
                let search_height = 26.0;
                let (search_row, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), search_height), Sense::hover());
                ui.painter().rect_filled(search_row, CornerRadius::ZERO, colors::SECONDARY_BG);

                let search_rect = search_row.shrink2(vec2(8.0, 2.0));
                let has_query = !search_query.is_empty();
                let clear_w = if has_query { 20.0 } else { 0.0 };
                let edit_rect = search_rect.with_max_x(search_rect.max.x - clear_w);

                let edit = TextEdit::singleline(&mut search_query)
                    .hint_text(RichText::new("Search...").color(colors::PLACEHOLDER))
                    .font(body_font())
                    .vertical_align(Align::Center)
                    .desired_width(edit_rect.width())
                    .text_color(colors::TEXT_COLOR_W)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO);
                ui.put(edit_rect, edit);

                let mut query_changed = search_query != prev_query;

                if has_query {
                    let clear_rect = Rect::from_min_max(
                        pos2(search_rect.max.x - 18.0, search_rect.min.y),
                        pos2(search_rect.max.x, search_rect.max.y),
                    );
                    let clear_resp =
                        ui.interact(clear_rect, popup_id.with("clear_search"), Sense::click());
                    let clear_color = if clear_resp.hovered() {
                        colors::HOVER_RED
                    } else {
                        colors::PLACEHOLDER
                    };
                    ui.painter().text(
                        clear_rect.center(),
                        Align2::CENTER_CENTER,
                        "×",
                        body_font(),
                        clear_color,
                    );
                    if clear_resp.clicked() {
                        search_query.clear();
                        query_changed = true;
                    }
                }

                if query_changed {
                    ui.data_mut(|d| d.insert_temp(search_id, search_query.clone()));
                    ui.ctx().request_repaint();
                }
            });
    }

    if let Some(v) = picked {
        *selected_id = v;
        response.mark_changed();
    }
    response
}

// ---------------------------------------------------------------------------
// Section toggle
// ---------------------------------------------------------------------------

/// Section header: 24px green badge with a white ▲/▼ and a title.
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

/// Two-handle range slider for trimming: grey 3px track,
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
    if response.drag_started()
        && let Some(p) = response.interact_pointer_pos()
    {
        let handle = if (p.x - to_x(*start)).abs() <= (p.x - to_x(*end)).abs() { 0u8 } else { 1u8 };
        ui.data_mut(|d| d.insert_temp(drag_id, handle));
    }
    if response.dragged()
        && let (Some(p), Some(handle)) =
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

/// Progress bar: 8px pill, `SECONDARY_BG` track, green progress (`fraction` 0..=1).
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
pub fn file_drag_drop_overlay(ctx: &egui::Context, title: &str) {
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
        title,
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
    fn text_actions() {
        let ctx = egui::Context::default();
        let id = Id::new("field");
        let select = |from: usize, to: usize| {
            let mut state = TextEditState::default();
            state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(from), CCursor::new(to))));
            state.store(&ctx, id);
        };
        let copied = |mut out: egui::FullOutput| {
            out.textures_delta.clear();
            out.platform_output.commands.into_iter().find_map(|c| match c {
                egui::OutputCommand::CopyText(t) => Some(t),
                _ => None,
            })
        };
        let mut text = "héllo world".to_owned();
        let run = |action, clipboard: Option<&str>, text: &mut String| {
            copied(ctx.run_ui(Default::default(), |ui| edit_text(ui.ctx(), id, text, action, clipboard, false)))
        };

        // Copy without a selection takes everything; Cut / Delete need one
        select(3, 3);
        assert_eq!(run(TextAction::Copy, None, &mut text).as_deref(), Some("héllo world"));
        assert_eq!(run(TextAction::Cut, None, &mut text), None);
        assert_eq!(text, "héllo world");

        select(6, 1); // backwards selection of "éllo "
        assert_eq!(run(TextAction::Cut, None, &mut text).as_deref(), Some("éllo "));
        assert_eq!(text, "hworld");

        select(1, 1);
        run(TextAction::Paste, Some("a\r\nb"), &mut text);
        assert_eq!(text, "ha  bworld", "one line in a single-line field");
        let state = TextEditState::load(&ctx, id).unwrap();
        assert_eq!(state.cursor.char_range().unwrap().primary.index.0, 5, "cursor after the paste");

        run(TextAction::SelectAll, None, &mut text);
        run(TextAction::Delete, None, &mut text);
        assert_eq!(text, "");
    }

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
    fn test_preset_menu_accordion() {
        let ctx = Context::default();
        let items = vec![
            PresetItem {
                id: "audio/mono".to_string(),
                group: "audio".to_string(),
                name: "mono".to_string(),
            },
            PresetItem {
                id: "vfx/filter1".to_string(),
                group: "vfx".to_string(),
                name: "filter1".to_string(),
            },
        ];
        let mut selected = "vfx/filter1".to_string();

        // Frame 1: Show menu
        let mut out = ctx.run_ui(Default::default(), |ui| {
            preset_menu(ui, "test", 200.0, &mut selected, &items, true);
        });
        out.textures_delta.clear();

        // Frame 2: Click the preset menu button (at 10, 10) to open popup
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::PointerButton {
            pos: pos2(10.0, 10.0),
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        raw.events.push(egui::Event::PointerButton {
            pos: pos2(10.0, 10.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        let mut out = ctx.run_ui(raw, |ui| {
            preset_menu(ui, "test", 200.0, &mut selected, &items, true);
        });
        out.textures_delta.clear();

        // Frame 3: Render popup open. Active group "vfx" should be open by default.
        let mut test_popup_id = None;
        let mut out = ctx.run_ui(Default::default(), |ui| {
            test_popup_id = Some(ui.make_persistent_id(("preset_menu", "test")));
            preset_menu(ui, "test", 200.0, &mut selected, &items, true);
        });
        out.textures_delta.clear();

        let pid = test_popup_id.unwrap();
        let open_gid = pid.with("preset_open_group");
        let open_group: Option<String> = ctx.data(|d| d.get_temp(open_gid));
        assert_eq!(open_group.as_deref(), Some("vfx"));

        // Frame 4: Click on the "audio" header to switch open group
        let popup_pos = egui::AreaState::load(&ctx, pid).unwrap().left_top_pos();
        let audio_header_pos = popup_pos + vec2(50.0, 17.0);

        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::PointerButton {
            pos: audio_header_pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        raw.events.push(egui::Event::PointerButton {
            pos: audio_header_pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        let mut out = ctx.run_ui(raw, |ui| {
            preset_menu(ui, "test", 200.0, &mut selected, &items, true);
        });
        out.textures_delta.clear();

        // Frame 5: Check state
        let mut out = ctx.run_ui(Default::default(), |ui| {
            preset_menu(ui, "test", 200.0, &mut selected, &items, true);
        });
        out.textures_delta.clear();

        let open_group: Option<String> = ctx.data(|d| d.get_temp(open_gid));
        assert_eq!(open_group.as_deref(), Some("audio"));

        // Frame 6: Click audio again to collapse it
        let popup_pos = egui::AreaState::load(&ctx, pid).unwrap().left_top_pos();
        let audio_header_pos = popup_pos + vec2(50.0, 17.0);
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::PointerButton {
            pos: audio_header_pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        raw.events.push(egui::Event::PointerButton {
            pos: audio_header_pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        let mut out = ctx.run_ui(raw, |ui| {
            preset_menu(ui, "test", 200.0, &mut selected, &items, true);
        });
        out.textures_delta.clear();

        // Frame 7: Check state - should now be None (all collapsed)
        let mut out = ctx.run_ui(Default::default(), |ui| {
            preset_menu(ui, "test", 200.0, &mut selected, &items, true);
        });
        out.textures_delta.clear();

        let open_group: Option<String> = ctx.data(|d| d.get_temp(open_gid));
        assert_eq!(open_group, None);
    }

    #[test]
    fn test_preset_menu_root_and_search() {
        let ctx = Context::default();
        let items = vec![
            PresetItem {
                id: "vfx/old_crt_tv".to_string(),
                group: "vfx".to_string(),
                name: "old_crt_tv".to_string(),
            },
            PresetItem {
                id: "my_root".to_string(),
                group: String::new(),
                name: "my_root".to_string(),
            },
        ];
        let mut selected = "my_root".to_string();

        // Frame 1: Render menu
        let mut out = ctx.run_ui(Default::default(), |ui| {
            preset_menu(ui, "search_test", 200.0, &mut selected, &items, true);
        });
        out.textures_delta.clear();

        // Frame 2: Click menu button to open popup
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::PointerButton {
            pos: pos2(10.0, 10.0),
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        raw.events.push(egui::Event::PointerButton {
            pos: pos2(10.0, 10.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        let mut pid = None;
        let mut out = ctx.run_ui(raw, |ui| {
            pid = Some(ui.make_persistent_id(("preset_menu", "search_test")));
            preset_menu(ui, "search_test", 200.0, &mut selected, &items, true);
        });
        out.textures_delta.clear();

        // Root preset opens the "" ("other") group
        let pid = pid.unwrap();
        let open_group: Option<String> = ctx.data(|d| d.get_temp(pid.with("preset_open_group")));
        assert_eq!(open_group.as_deref(), Some(""));

        // Frame 3: Inject search query "vfx/old" and render matching list
        ctx.data_mut(|d| d.insert_temp(pid.with("preset_search"), "vfx/old".to_string()));
        let mut out = ctx.run_ui(Default::default(), |ui| {
            preset_menu(ui, "search_test", 200.0, &mut selected, &items, true);
        });
        out.textures_delta.clear();

        // Frame 4: Click first search result
        let popup_pos = egui::AreaState::load(&ctx, pid).unwrap().left_top_pos();
        let first_match_pos = popup_pos + vec2(50.0, 17.0);
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::PointerButton {
            pos: first_match_pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        raw.events.push(egui::Event::PointerButton {
            pos: first_match_pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        let mut changed = false;
        let mut out = ctx.run_ui(raw, |ui| {
            changed = preset_menu(ui, "search_test", 200.0, &mut selected, &items, true).changed();
        });
        out.textures_delta.clear();

        assert!(changed);
        assert_eq!(selected, "vfx/old_crt_tv");
    }

    #[test]
    fn test_cell_geometry() {
        let r = Rect::from_min_size(pos2(31.0, 100.0), vec2(756.0, 38.0));
        let c = cell(r, 226.0, 350.0);
        assert_eq!(c.min, pos2(257.0, 100.0));
        assert_eq!(c.size(), vec2(350.0, 38.0));
    }

    /// A radio button is 22px across, checked or not; the checked ring leaves a 10px hole.
    #[test]
    fn radio_circle_is_22px() {
        let ctx = Context::default();
        for selected in [false, true] {
            let mut center = Pos2::ZERO;
            let mut out = ctx.run_ui(Default::default(), |ui| {
                ui.allocate_space(vec2(0.0, 50.0));
                let r = radio(ui, selected, "Source");
                center = pos2(r.rect.min.x + dimensions::RADIO_SIZE / 2.0, r.rect.center().y);
            });
            out.textures_delta.clear();
            let meshes = ctx.tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
            // Opaque ring vertices only (feathering adds a transparent fringe)
            let distances: Vec<f32> = meshes
                .iter()
                .filter_map(|m| match &m.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => Some(mesh),
                    _ => None,
                })
                .flat_map(|mesh| mesh.vertices.iter())
                .filter(|v| v.color.a() == 255 && v.color != colors::LABEL_TEXT && (v.pos - center).length() < 15.0)
                .map(|v| (v.pos - center).length())
                .collect();
            let outer = distances.iter().copied().fold(0.0, f32::max);
            assert!((outer - 11.0).abs() < 0.6, "outer radius {outer} (selected: {selected})");
            if selected {
                let inner = distances.iter().copied().fold(f32::MAX, f32::min);
                assert!((inner - 5.0).abs() < 0.6, "inner radius {inner}");
            }
        }
    }
}
