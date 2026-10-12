//! Help windows: the Help button's Manual / About / License tabs and the `?`
//! buttons' ffmpeg help output.

use std::sync::{Arc, mpsc::{Receiver, channel}};
use eframe::egui::{
    self, CornerRadius, FontId, Margin, Pos2, ScrollArea, Ui, UiBuilder, Vec2, ViewportBuilder,
    ViewportCommand, ViewportId, text::{LayoutJob, TextFormat}, vec2,
};
use crate::config::settings::VideoCodec;
use crate::core::probe::hidden_command;
use crate::ui::{
    dialogs,
    theme::{colors, dimensions},
    widgets::{ButtonKind, button},
};
use super::NvencFfxApp;

// Embedded, so the single exe needs no text files next to it
const MANUAL: &str = include_str!("../../nff-help.txt");
const ABOUT: &str = include_str!("../../nff-about.txt");
const LICENSE: &str = include_str!("../../nff-license.txt");

const MAIN_TITLE: &str = "nvencFFX Help";
/// `geometry("800x700")` / `geometry("800x600")` plus the self-drawn title bar.
const MAIN_SIZE: Vec2 = vec2(800.0, 700.0 + dialogs::TITLE_BAR_HEIGHT);
const TOPIC_SIZE: Vec2 = vec2(800.0, 600.0 + dialogs::TITLE_BAR_HEIGHT);
const MIN_SIZE: Vec2 = vec2(400.0, 300.0);
/// `font=("Consolas", 14)`
const TEXT_SIZE: f32 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Manual,
    About,
    License,
}

const TABS: [(&str, Tab); 3] = [("Manual", Tab::Manual), ("About", Tab::About), ("License", Tab::License)];

/// `load_tab_content`: the tab's text; in the manual, lines starting with `#` are
/// green headings (without the `#`).
fn tab_text(tab: Tab) -> LayoutJob {
    let font = FontId::monospace(TEXT_SIZE);
    let plain = TextFormat::simple(font.clone(), colors::LABEL_TEXT);
    let heading = TextFormat::simple(font, colors::ACCENT_GREEN);
    let mut job = LayoutJob::default();
    let text = match tab {
        Tab::Manual => MANUAL,
        Tab::About => ABOUT,
        Tab::License => LICENSE,
    };
    for line in text.lines() {
        match line.strip_prefix('#') {
            Some(title) if tab == Tab::Manual => job.append(title, 0.0, heading.clone()),
            _ => job.append(line, 0.0, plain.clone()),
        }
        job.append("\n", 0.0, plain.clone());
    }
    job
}

/// What a `?` button shows (`help_type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Topic {
    Encoder,
    Filters,
}

impl Topic {
    fn title(self) -> &'static str {
        match self {
            Self::Encoder => "NVENC Encoder Options",
            Self::Filters => "FFmpeg Video Filters",
        }
    }

    fn viewport(self) -> ViewportId {
        ViewportId::from_hash_of(("help", self.title()))
    }

    /// The ffmpeg arguments for this topic.
    fn args(self, codec: VideoCodec) -> Vec<String> {
        match self {
            Self::Encoder => vec!["-h".to_owned(), format!("encoder={}_nvenc", codec.suffix())],
            Self::Filters => vec!["-filters".to_owned()],
        }
    }
}

/// Runs ffmpeg and returns its stdout, or its stderr if that is empty.
fn fetch(ffmpeg: &str, args: &[String]) -> String {
    match hidden_command(ffmpeg).args(args).stdin(std::process::Stdio::null()).output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            if stdout.is_empty() { String::from_utf8_lossy(&out.stderr).into_owned() } else { stdout }
        }
        Err(e) => format!("Error retrieving help information:\n{e}"),
    }
}

struct MainWindow {
    tab: Tab,
    text: Arc<LayoutJob>,
    position: Option<Pos2>,
}

struct TopicWindow {
    text: String,
    /// The ffmpeg output, until it arrives.
    pending: Option<Receiver<String>>,
    position: Option<Pos2>,
}

/// The open help windows; each kind opens once (`open_help_windows`).
#[derive(Default)]
pub(super) struct Help {
    main: Option<MainWindow>,
    encoder: Option<TopicWindow>,
    filters: Option<TopicWindow>,
}

impl Help {
    fn topic(&mut self, topic: Topic) -> &mut Option<TopicWindow> {
        match topic {
            Topic::Encoder => &mut self.encoder,
            Topic::Filters => &mut self.filters,
        }
    }
}

fn main_viewport() -> ViewportId {
    ViewportId::from_hash_of("help_main")
}

/// Centered on the main window.
fn centered(ctx: &egui::Context, size: Vec2) -> Option<Pos2> {
    ctx.input(|i| i.viewport().outer_rect).map(|r| r.center() - size / 2.0)
}

fn builder(title: &str, size: Vec2, position: Option<Pos2>, icon: Arc<egui::IconData>) -> ViewportBuilder {
    let builder = ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size(MIN_SIZE)
        .with_decorations(false)
        .with_resizable(true)
        .with_icon(icon);
    match position {
        Some(p) => builder.with_position(p),
        None => builder,
    }
}

/// The Close button centered at the bottom, 20 px up (its `pady` plus the frame's);
/// true when clicked.
fn close_button(ui: &mut Ui) -> bool {
    let size = vec2(dimensions::BUTTON_WIDTH, dimensions::WIDGET_HEIGHT);
    let rect = egui::Rect::from_center_size(ui.max_rect().center_bottom() - vec2(0.0, 20.0 + size.y / 2.0), size);
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| button(ui, size, "Close", ButtonKind::Grey)).inner.clicked()
}

/// A rounded `SECONDARY_BG` box filling `rect`, scrolled `both` ways or vertically.
fn text_box(ui: &mut Ui, rect: egui::Rect, id: &str, both: bool, add: impl FnOnce(&mut Ui)) {
    ui.painter().rect_filled(rect, CornerRadius::same(dimensions::CORNER_RADIUS), colors::SECONDARY_BG);
    ui.scope_builder(UiBuilder::new().max_rect(rect.shrink(8.0)), |ui| {
        let scroll = if both { ScrollArea::both() } else { ScrollArea::vertical() };
        scroll
            .id_salt(id)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, add);
    });
}

impl NvencFfxApp {
    /// The Help button.
    pub(super) fn open_main_help(&mut self, ctx: &egui::Context) {
        if self.help.main.is_some() {
            ctx.send_viewport_cmd_to(main_viewport(), ViewportCommand::Focus);
            return;
        }
        let tab = Tab::Manual;
        self.help.main = Some(MainWindow { tab, text: Arc::new(tab_text(tab)), position: centered(ctx, MAIN_SIZE) });
    }

    /// A `?` button: runs ffmpeg in the background and shows
    /// what it prints.
    pub(super) fn open_help(&mut self, topic: Topic, ctx: &egui::Context) {
        if self.help.topic(topic).is_some() {
            ctx.send_viewport_cmd_to(topic.viewport(), ViewportCommand::Focus);
            return;
        }
        let (tx, rx) = channel();
        let (ffmpeg, args, repaint) = (self.s.ffmpeg_path.clone(), topic.args(self.s.codec), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(fetch(&ffmpeg, &args));
            repaint.request_repaint();
        });
        *self.help.topic(topic) = Some(TopicWindow {
            text: "Loading help information...".to_owned(),
            pending: Some(rx),
            position: centered(ctx, TOPIC_SIZE),
        });
    }

    pub(super) fn show_help_windows(&mut self, ctx: &egui::Context) {
        if let Some(window) = &self.help.main {
            let builder = builder(MAIN_TITLE, MAIN_SIZE, window.position, self.icon.clone());
            ctx.show_viewport_immediate(main_viewport(), builder, |ui, _| self.main_help_ui(ui));
        }
        for topic in [Topic::Encoder, Topic::Filters] {
            if let Some(window) = self.help.topic(topic) {
                let builder = builder(topic.title(), TOPIC_SIZE, window.position, self.icon.clone());
                ctx.show_viewport_immediate(topic.viewport(), builder, |ui, _| self.topic_help_ui(ui, topic));
            }
        }
    }

    fn main_help_ui(&mut self, ui: &mut Ui) {
        let Some(window) = &mut self.help.main else { return };
        let mut close = ui.input(|i| i.viewport().close_requested());
        let panel = egui::Frame::new().fill(colors::PRIMARY_BG);
        egui::CentralPanel::no_frame().frame(panel).show(ui, |ui| {
            close |= dialogs::title_bar(ui, MAIN_TITLE);
            let area = ui.available_rect_before_wrap();
            let h = dimensions::WIDGET_HEIGHT;

            // Tab buttons, the open one green (`switch_tab`)
            let tabs_top = area.min + vec2(20.0, 20.0);
            for (i, (text, tab)) in TABS.into_iter().enumerate() {
                let rect = egui::Rect::from_min_size(tabs_top + vec2(111.0 * i as f32, 0.0), vec2(106.0, h));
                let kind = if window.tab == tab { ButtonKind::Green } else { ButtonKind::Grey };
                let clicked = ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| button(ui, rect.size(), text, kind)).inner.clicked();
                if clicked && window.tab != tab {
                    window.tab = tab;
                    window.text = Arc::new(tab_text(tab));
                }
            }

            // Text box between the tabs and the Close button
            let text = egui::Rect::from_min_max(
                egui::pos2(area.min.x + 20.0, tabs_top.y + h + 20.0),
                area.max - vec2(20.0, 20.0 + h + 20.0),
            );
            let job = window.text.clone();
            text_box(ui, text, "help_text", false, |ui| {
                ui.add(egui::Label::new(job).wrap());
            });
            close |= close_button(ui);
        });
        dialogs::resize_edges(ui);
        if close {
            self.help.main = None;
        }
    }

    fn topic_help_ui(&mut self, ui: &mut Ui, topic: Topic) {
        let Some(window) = self.help.topic(topic) else { return };
        if let Some(text) = window.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            window.text = text;
            window.pending = None;
        }
        let mut close = ui.input(|i| i.viewport().close_requested());
        let panel = egui::Frame::new().fill(colors::PRIMARY_BG);
        egui::CentralPanel::no_frame().frame(panel).show(ui, |ui| {
            close |= dialogs::title_bar(ui, topic.title());
            let area = ui.available_rect_before_wrap();
            let h = dimensions::WIDGET_HEIGHT;
            let text = egui::Rect::from_min_max(area.min + vec2(15.0, 15.0), area.max - vec2(15.0, 20.0 + h + 15.0));
            let content = &window.text;
            text_box(ui, text, "help_text", true, |ui| {
                egui::Frame::new().inner_margin(Margin::same(2)).show(ui, |ui| {
                    let text = egui::RichText::new(content).font(FontId::monospace(TEXT_SIZE)).color(colors::LABEL_TEXT);
                    ui.add(egui::Label::new(text).extend());
                });
            });
            close |= close_button(ui);
        });
        dialogs::resize_edges(ui);
        if close {
            *self.help.topic(topic) = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_headings_are_green_without_the_hash() {
        let job = tab_text(Tab::Manual);
        assert!(job.text.starts_with("ADVANCED ENCODER SETTINGS\n"));
        assert_eq!(job.sections[0].format.color, colors::ACCENT_GREEN);
        assert!(!job.text.contains('\r'));
        let about = tab_text(Tab::About);
        assert!(about.sections.iter().all(|s| s.format.color == colors::LABEL_TEXT));
        assert!(about.text.starts_with(&format!("{}\n", crate::WINDOW_TITLE)), "nff-about.txt names this version");
        assert!(tab_text(Tab::License).text.starts_with("MIT License"));
    }

    #[test]
    fn topics_ask_ffmpeg_for_the_current_encoder() {
        assert_eq!(Topic::Encoder.args(VideoCodec::Av1), ["-h", "encoder=av1_nvenc"]);
        assert_eq!(Topic::Filters.args(VideoCodec::H264), ["-filters"]);
        assert!(fetch(r"Z:\missing\ffmpeg.exe", &[]).starts_with("Error retrieving help information:\n"));
    }

    #[test]
    fn windows_open_once_and_close() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply_theme(&ctx);
        let mut app = NvencFfxApp::default();
        app.open_main_help(&ctx);
        app.open_help(Topic::Filters, &ctx);
        app.open_help(Topic::Filters, &ctx);
        assert!(app.help.main.is_some() && app.help.filters.is_some() && app.help.encoder.is_none());

        // The About tab, then Close (centered at the bottom)
        let size = MAIN_SIZE;
        let click = |app: &mut NvencFfxApp, pos: Pos2| {
            for pressed in [None, Some(true), Some(false)] {
                let events = match pressed {
                    None => vec![egui::Event::PointerMoved(pos)],
                    Some(pressed) => vec![egui::Event::PointerButton {
                        pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default(),
                    }],
                };
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| app.main_help_ui(ui));
                out.textures_delta.clear();
            }
        };
        let tabs_y = dialogs::TITLE_BAR_HEIGHT + 20.0 + 14.0;
        click(&mut app, egui::pos2(20.0 + 111.0 + 53.0, tabs_y));
        assert_eq!(app.help.main.as_ref().unwrap().tab, Tab::About);
        click(&mut app, egui::pos2(size.x / 2.0, size.y - 20.0 - 14.0));
        assert!(app.help.main.is_none());
    }
}
