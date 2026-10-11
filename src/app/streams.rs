//! Streams window (`_open_map_window`, `_create_stream_map_window`): checkboxes for
//! the input's streams, written to Add FF Options as `-map` options.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use eframe::egui::{
    self, Align2, CornerRadius, FontId, Pos2, Rect, ScrollArea, Ui, Vec2, ViewportBuilder, ViewportCommand,
    ViewportId, pos2, vec2,
};
use crate::core::{probe, streams::{self, Streams}};
use crate::ui::{
    dialogs::{self, Kind},
    theme::{colors, dimensions},
    widgets::{ButtonKind, button, checkbox, place, row},
};
use super::{Action, NvencFfxApp};

/// `geometry("600x600")` / `minsize(600, 550)` plus the self-drawn title bar.
const WINDOW_SIZE: Vec2 = vec2(600.0, 600.0 + dialogs::TITLE_BAR_HEIGHT);
const MIN_SIZE: Vec2 = vec2(600.0, 550.0 + dialogs::TITLE_BAR_HEIGHT);

pub(super) fn viewport() -> ViewportId {
    ViewportId::from_hash_of("stream_selection")
}

enum Load {
    Pending(Receiver<Result<Streams, String>>),
    Done(Streams),
    Failed(String),
}

pub(super) struct StreamsWindow {
    input: String,
    title: String,
    load: Load,
    /// Check state per stream, in the order of [`Streams::all`] (`stream_vars`).
    checked: Vec<bool>,
    position: Option<Pos2>,
}

/// Stream selections per input file, kept for the session (`map_selection_cache`).
pub(super) type Selections = HashMap<String, HashMap<usize, bool>>;

impl StreamsWindow {
    fn streams(&self) -> Option<&Streams> {
        match &self.load {
            Load::Done(streams) => Some(streams),
            _ => None,
        }
    }

    /// `save_selection`
    fn save(&self, selections: &mut Selections) {
        if let Some(streams) = self.streams() {
            let selection = streams.all().zip(&self.checked).map(|(s, c)| (s.index, *c)).collect();
            selections.insert(self.input.clone(), selection);
        }
    }
}

impl NvencFfxApp {
    /// The Streams button (`_open_map_window`).
    pub(super) fn open_streams(&mut self, ctx: &egui::Context) {
        if self.streams.is_some() {
            ctx.send_viewport_cmd_to(viewport(), ViewportCommand::Focus);
            return;
        }
        if self.input_file.is_empty() {
            return self.message(Kind::Warning, "Warning", "Please select an input file first.");
        }
        if !Path::new(&self.input_file).exists() {
            return self.message(Kind::Warning, "Error", "Input file does not exist.");
        }
        let Some(ffprobe) = self.ffprobe.clone() else {
            return self.message(Kind::Error, "Error", "ffprobe.exe not found.");
        };
        let (tx, rx) = channel();
        let (input, repaint) = (self.input_file.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = match probe::overview(&ffprobe, &input, None) {
                Ok(text) => Ok(streams::parse(&text.unwrap_or_default())),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        let name = Path::new(&self.input_file).file_name().unwrap_or_default().to_string_lossy();
        self.streams = Some(StreamsWindow {
            title: format!("Stream Selection - {name}"),
            input: self.input_file.clone(),
            load: Load::Pending(rx),
            checked: Vec::new(),
            position: ctx.input(|i| i.viewport().outer_rect).map(|r| r.center() - WINDOW_SIZE / 2.0),
        });
    }

    /// `apply_map`: writes the selection to Add FF Options and closes the window.
    fn apply_streams(&mut self) {
        let Some(window) = &self.streams else { return };
        let Some(streams) = window.streams() else { return };
        if !window.checked.iter().any(|c| *c) {
            self.show_dialog_in(viewport(), Kind::Warning, "Warning", "At least one stream must be selected.", Action::None);
            return;
        }
        let all = window.checked.iter().all(|c| *c);
        let selected: Vec<&str> =
            streams.all().zip(&window.checked).filter(|(_, c)| **c).map(|(s, _)| s.map_code.as_str()).collect();
        self.s.additional_options = streams::apply_map(&self.s.additional_options, &selected, all);
        window.save(&mut self.map_selections);
        self.streams = None;
    }

    pub(super) fn show_streams_window(&mut self, ctx: &egui::Context) {
        let Some(window) = &self.streams else { return };
        let mut builder = ViewportBuilder::default()
            .with_title(&window.title)
            .with_inner_size(WINDOW_SIZE)
            .with_min_inner_size(MIN_SIZE)
            .with_decorations(false)
            .with_resizable(true)
            .with_icon(self.icon.clone());
        if let Some(position) = window.position {
            builder = builder.with_position(position);
        }
        ctx.show_viewport_immediate(viewport(), builder, |ui, _| self.streams_window_ui(ui));
    }

    fn streams_window_ui(&mut self, ui: &mut Ui) {
        enum Clicked {
            Apply,
            CheckAll,
            UncheckAll,
            Close,
        }
        let Some(window) = &mut self.streams else { return };
        // `fetch_and_populate` finished: restore this file's previous selection
        if let Load::Pending(rx) = &window.load
            && let Ok(result) = rx.try_recv()
        {
            window.load = match result {
                Ok(streams) => {
                    let saved = self.map_selections.get(&window.input);
                    window.checked =
                        streams.all().map(|s| saved.and_then(|m| m.get(&s.index)).copied().unwrap_or(true)).collect();
                    Load::Done(streams)
                }
                Err(e) => Load::Failed(format!("Error: {e}")),
            };
        }

        let mut clicked = None;
        if ui.input(|i| i.viewport().close_requested()) {
            clicked = Some(Clicked::Close);
        }
        let panel = egui::Frame::new().fill(colors::PRIMARY_BG);
        egui::CentralPanel::no_frame().frame(panel).show(ui, |ui| {
            if dialogs::title_bar(ui, &window.title) {
                clicked = Some(Clicked::Close);
            }
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let area = ui.available_rect_before_wrap().shrink(10.0);
            let h = dimensions::WIDGET_HEIGHT;
            ui.painter().text(
                pos2(area.center().x, area.min.y + h / 2.0),
                Align2::CENTER_CENTER,
                "Select streams to include in the output file:",
                FontId::proportional(14.0),
                colors::TEXT_COLOR_W,
            );

            // Apply / Check All / Uncheck All need the streams; Close always works
            let buttons_y = area.max.y - 5.0 - h;
            let w = (area.width() - 15.0) / 4.0;
            let loaded = window.streams().is_some();
            let buttons = [
                ("Apply", ButtonKind::Green, Clicked::Apply),
                ("Check All", ButtonKind::Grey, Clicked::CheckAll),
                ("Uncheck All", ButtonKind::Grey, Clicked::UncheckAll),
                ("Close", ButtonKind::Grey, Clicked::Close),
            ];
            for (i, (text, kind, action)) in buttons.into_iter().enumerate() {
                let kind = if loaded || matches!(action, Clicked::Close) { kind } else { ButtonKind::Disabled };
                let rect = Rect::from_min_size(pos2(area.min.x + (w + 5.0) * i as f32, buttons_y), vec2(w, h));
                if place(ui, rect, |ui| button(ui, rect.size(), text, kind)).clicked() {
                    clicked = Some(action);
                }
            }

            let list = Rect::from_min_max(pos2(area.min.x, area.min.y + h + 10.0), pos2(area.max.x, buttons_y - 10.0));
            ui.painter().rect_filled(list, CornerRadius::same(dimensions::CORNER_RADIUS), colors::SECONDARY_BG);
            ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(5.0)), |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                    .show(ui, |ui| stream_list(ui, &window.load, &mut window.checked));
            });
        });
        dialogs::resize_edges(ui);
        self.dialog_ui(ui, viewport());

        match clicked {
            Some(Clicked::Apply) => self.apply_streams(),
            Some(Clicked::CheckAll | Clicked::UncheckAll) => {
                let value = matches!(clicked, Some(Clicked::CheckAll));
                if let Some(window) = &mut self.streams {
                    window.checked.iter_mut().for_each(|c| *c = value);
                }
            }
            // `on_close`
            Some(Clicked::Close) => {
                if let Some(window) = self.streams.take() {
                    window.save(&mut self.map_selections);
                }
            }
            None => {}
        }
        // A message still open moves to the main window
        if self.streams.is_none()
            && let Some((vp, _)) = &mut self.dialog
            && *vp == viewport()
        {
            *vp = ViewportId::ROOT;
        }
    }
}

/// The scrolled list: a status line while loading or on failure, else a titled
/// checkbox group per stream type.
fn stream_list(ui: &mut Ui, load: &Load, checked: &mut [bool]) {
    let centered = |ui: &mut Ui, text: &str, color, pad: f32| {
        ui.add_space(pad);
        let r = row(ui, dimensions::WIDGET_HEIGHT);
        ui.painter().text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(13.0), color);
        ui.add_space(pad);
    };
    let streams = match load {
        Load::Pending(_) => return centered(ui, "Analyzing file, please wait...", colors::ACCENT_GREY, 40.0),
        Load::Failed(e) => return centered(ui, e, colors::ACCENT_RED, 20.0),
        Load::Done(streams) if streams.is_empty() => {
            return centered(ui, "No streams found or error parsing output", colors::ACCENT_RED, 20.0);
        }
        Load::Done(streams) => streams,
    };
    let mut checks = checked.iter_mut();
    for (title, list) in streams.sections() {
        if list.is_empty() {
            continue;
        }
        ui.add_space(10.0);
        let r = row(ui, dimensions::WIDGET_HEIGHT);
        ui.painter().text(
            pos2(r.min.x + 10.0, r.center().y),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(13.0),
            colors::TEXT_COLOR_W,
        );
        ui.add_space(5.0);
        for (stream, value) in list.iter().zip(checks.by_ref()) {
            ui.add_space(2.0);
            let r = row(ui, dimensions::CHECKBOX_SIZE);
            place(ui, r.with_min_x(r.min.x + 25.0), |ui| checkbox(ui, value, &stream.text));
            ui.add_space(2.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(app: &mut NvencFfxApp, output: &str) {
        let (tx, rx) = channel();
        tx.send(Ok(streams::parse(output))).unwrap();
        app.streams = Some(StreamsWindow {
            input: "a.mkv".into(),
            title: "Stream Selection - a.mkv".into(),
            load: Load::Pending(rx),
            checked: Vec::new(),
            position: None,
        });
        draw(app, &[]);
    }

    fn draw(app: &mut NvencFfxApp, events: &[egui::Event]) {
        let ctx = egui::Context::default();
        crate::ui::theme::apply_theme(&ctx);
        for event in events.iter().map(|e| vec![e.clone()]).chain([Vec::new()]) {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, WINDOW_SIZE)),
                events: event,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| app.streams_window_ui(ui));
            out.textures_delta.clear();
        }
    }

    const OUTPUT: &str = "  Stream #0:0: Video: h264, yuv420p, 1920x1080
  Stream #0:1(eng): Audio: aac, 48000 Hz, stereo
  Stream #0:2(jpn): Audio: aac, 48000 Hz, stereo";

    #[test]
    fn apply_writes_maps_and_remembers_the_selection() {
        let mut app = NvencFfxApp::default();
        app.s.additional_options = "-ac 2 -map 0:a:1".into();
        loaded(&mut app, OUTPUT);
        assert_eq!(app.streams.as_ref().unwrap().checked, [true, true, true]);

        app.streams.as_mut().unwrap().checked[1] = false;
        app.apply_streams();
        assert!(app.streams.is_none());
        assert_eq!(app.s.additional_options, "-ac 2 -ignore_unknown -map 0:v:0 -map 0:a:1");

        // Reopened: the selection is back; all checked removes the maps
        loaded(&mut app, OUTPUT);
        assert_eq!(app.streams.as_ref().unwrap().checked, [true, false, true]);
        app.streams.as_mut().unwrap().checked[1] = true;
        app.apply_streams();
        assert_eq!(app.s.additional_options, "-ac 2");

        loaded(&mut app, OUTPUT);
        app.streams.as_mut().unwrap().checked.fill(false);
        app.apply_streams();
        assert!(app.streams.is_some(), "nothing selected keeps the window open");
        assert_eq!(app.dialog.as_ref().unwrap().1.message, "At least one stream must be selected.");
    }

    #[test]
    fn buttons_and_checkboxes() {
        let mut app = NvencFfxApp::default();
        loaded(&mut app, OUTPUT);
        let click = |pos: Pos2| {
            [
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
            ]
        };
        let w = (WINDOW_SIZE.x - 20.0 - 15.0) / 4.0;
        let buttons_y = WINDOW_SIZE.y - 10.0 - 5.0 - 14.0;
        // Uncheck All (third button), then the first audio checkbox: below the list
        // top, the Video title and its row, then the Audio title
        draw(&mut app, &click(pos2(10.0 + 2.0 * (w + 5.0) + w / 2.0, buttons_y)));
        assert_eq!(app.streams.as_ref().unwrap().checked, [false, false, false]);
        let list_top = dialogs::TITLE_BAR_HEIGHT + 10.0 + 28.0 + 10.0 + 5.0;
        let audio = list_top + (10.0 + 28.0 + 5.0 + 2.0 + 24.0 + 2.0) + (10.0 + 28.0 + 5.0 + 2.0) + 12.0;
        draw(&mut app, &click(pos2(10.0 + 5.0 + 25.0 + 12.0, audio)));
        assert_eq!(app.streams.as_ref().unwrap().checked, [false, true, false]);
        // Close keeps the selection for the next time
        draw(&mut app, &click(pos2(10.0 + 3.0 * (w + 5.0) + w / 2.0, buttons_y)));
        assert!(app.streams.is_none());
        assert_eq!(app.map_selections["a.mkv"], HashMap::from([(0, false), (1, true), (2, false)]));
    }

    #[test]
    fn opening_checks_the_input() {
        let ctx = egui::Context::default();
        let mut app = NvencFfxApp::default();
        app.open_streams(&ctx);
        assert_eq!(app.dialog.take().unwrap().1.message, "Please select an input file first.");
        app.input_file = r"Z:\missing.mkv".into();
        app.open_streams(&ctx);
        let (_, dialog) = app.dialog.take().unwrap();
        assert_eq!((dialog.title.as_str(), dialog.message.as_str()), ("Error", "Input file does not exist."));
        assert!(app.streams.is_none());
    }
}
