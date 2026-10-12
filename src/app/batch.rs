//! Batch Converter: a list of files converted one after another with the main
//! window's settings, in a window of its own.

use std::path::{Path, PathBuf};
use eframe::egui::{
    self, Align, Align2, CornerRadius, FontId, Pos2, Rect, ScrollArea, Ui, UiBuilder, Vec2,
    Layout, ViewportBuilder, ViewportId, pos2, vec2,
};
use crate::config::settings::VideoCodec;
use crate::core::{command, process::{Exit, Runner}};
use crate::ui::{
    dialogs::{self, Kind},
    theme::{colors, dimensions},
    widgets::{
        ButtonKind, button, cell, checkbox, cross_button, colored_label, file_drag_drop_overlay,
        handle_dropped_files, option_menu, place, row,
    },
};
use super::{Action, NvencFfxApp, Task, TaskKind, Taskbar, VIDEO_EXTENSIONS, has_extension};

const WINDOW_TITLE: &str = "Batch Converter";
/// `geometry("600x400")` / `minsize(600, 400)` plus the self-drawn title bar.
const WINDOW_SIZE: Vec2 = vec2(600.0, 400.0 + dialogs::TITLE_BAR_HEIGHT);
const CONTAINERS: &[&str] = &["mp4", "mkv", "mov"];

pub(super) fn viewport() -> ViewportId {
    ViewportId::from_hash_of("batch_converter")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Ready,
    Converting,
    Done,
    /// With the reason if ffmpeg did not even start.
    Failed(Option<String>),
    Cancelled,
}

impl Status {
    fn text(&self) -> String {
        match self {
            Self::Ready => "Ready".to_owned(),
            Self::Converting => "Converting".to_owned(),
            Self::Done => "Done".to_owned(),
            Self::Failed(None) => "Failed".to_owned(),
            Self::Failed(Some(e)) => format!("Failed: {e}"),
            Self::Cancelled => "Cancelled".to_owned(),
        }
    }
}

pub(super) struct BatchFile {
    pub path: String,
    pub status: Status,
}

/// A batch in progress (`is_converting`).
struct Run {
    /// The file being converted; the ones before it are finished (`current_file_index`).
    index: usize,
    /// The file being converted was removed from the list, so `index` already
    /// points at the next one.
    removed: bool,
    /// A file failed: the taskbar button ends up red.
    had_error: bool,
    /// The main window's input / output, restored afterwards.
    saved_input: String,
    saved_output: String,
}

#[derive(Default)]
pub(super) struct Batch {
    /// `batch_files`; kept while the window is closed, until the app exits.
    pub files: Vec<BatchFile>,
    /// The window is shown. Closing it during a batch only hides it.
    pub open: bool,
    /// Where the window opens: centered on the main window.
    position: Option<Pos2>,
    run: Option<Run>,
}

impl Batch {
    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    /// A file already listed is ignored.
    fn add(&mut self, path: String) {
        if !self.files.iter().any(|f| f.path == path) {
            self.files.push(BatchFile { path, status: Status::Ready });
        }
    }

    /// A running batch carries on with the file after the one it
    /// is converting, wherever that ends up in the list.
    fn remove(&mut self, index: usize) {
        if index >= self.files.len() {
            return;
        }
        self.files.remove(index);
        if let Some(run) = &mut self.run {
            if index < run.index {
                run.index -= 1;
            } else if index == run.index {
                run.removed = true;
            }
        }
    }

    fn clear(&mut self) {
        self.files.clear();
        if let Some(run) = &mut self.run {
            run.index = 0;
            run.removed = true;
        }
    }

    /// During a batch the taskbar shows the progress of the
    /// whole list (finished files plus the one in flight) instead of restarting at 0%
    /// for every file.
    pub fn overall_progress(&self, file_progress: f32) -> f32 {
        match &self.run {
            Some(run) if !self.files.is_empty() => {
                ((run.index as f32 + file_progress) / self.files.len() as f32).clamp(0.0, 1.0)
            }
            _ => file_progress,
        }
    }
}

/// `<stem>_<codec>_custom<ext>` in the batch output folder,
/// else in the main window's last output folder if it still exists, else next to
/// the input. `<ext>` is the chosen container, or the input's own.
fn output_path(input: &Path, codec: VideoCodec, folder: &str, last_output_dir: &str, container: Option<&str>) -> PathBuf {
    let dir = if !folder.is_empty() {
        PathBuf::from(folder)
    } else if !last_output_dir.is_empty() && Path::new(last_output_dir).exists() {
        PathBuf::from(last_output_dir)
    } else {
        input.parent().map(Path::to_path_buf).unwrap_or_default()
    };
    let ext = match container {
        Some(c) => format!(".{c}"),
        None => input.extension().map_or_else(|| ".mp4".to_owned(), |e| format!(".{}", e.to_string_lossy())),
    };
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    dir.join(format!("{stem}_{}_custom{ext}", codec.suffix()))
}

/// Where the files will go.
fn output_label(folder: &str, last_output_dir: &str) -> String {
    let text = if !folder.is_empty() {
        folder
    } else if !last_output_dir.is_empty() && Path::new(last_output_dir).exists() {
        last_output_dir
    } else {
        "Original folder(s)"
    };
    truncate(text, 85)
}

/// `text` cut to `max` characters, the last three replaced by "...".
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        format!("{}...", text.chars().take(max - 3).collect::<String>())
    } else {
        text.to_owned()
    }
}

/// The folder a file dialog starts in: the first of `dirs` that exists, else the
/// working directory.
fn start_dir(dirs: &[&str]) -> PathBuf {
    dirs.iter()
        .map(PathBuf::from)
        .find(|d| !d.as_os_str().is_empty() && d.is_dir())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// A file dialog owned by the batch window (the one just clicked in).
fn file_dialog(title: &str, dir: PathBuf) -> rfd::FileDialog {
    let dialog = rfd::FileDialog::new().set_title(title).set_directory(dir);
    #[cfg(windows)]
    if let Some(parent) = crate::win32::foreground_window() {
        return dialog.set_parent(&parent);
    }
    dialog
}

/// Number, file name, status and the remove button; true when
/// the button was clicked.
fn file_row(ui: &mut Ui, index: usize, file: &BatchFile) -> bool {
    let r = row(ui, dimensions::WIDGET_HEIGHT);
    let font = FontId::proportional(dimensions::FONT_SIZE);
    let color = colors::LABEL_TEXT;
    let remove = Rect::from_center_size(pos2(r.max.x - 15.0, r.center().y), vec2(20.0, 20.0));

    let painter = ui.painter();
    painter.text(pos2(r.min.x + 20.0, r.center().y), Align2::CENTER_CENTER, format!("{}.", index + 1), font.clone(), color);
    // At least 80px wide; a longer status pushes the name aside
    let status = painter.layout_no_wrap(file.status.text(), font.clone(), color);
    let status_w = status.size().x.max(80.0);
    let status_rect = Rect::from_min_max(pos2(remove.min.x - 5.0 - status_w, r.min.y), pos2(remove.min.x - 5.0, r.max.y));
    painter.galley(status_rect.center() - status.size() / 2.0, status, color);
    let name = Path::new(&file.path).file_name().unwrap_or_default().to_string_lossy();
    let name_rect = Rect::from_min_max(pos2(r.min.x + 40.0, r.min.y), pos2(status_rect.min.x - 10.0, r.max.y));
    painter.with_clip_rect(name_rect).text(
        pos2(name_rect.min.x, r.center().y),
        Align2::LEFT_CENTER,
        truncate(&name, 40),
        font,
        color,
    );

    place(ui, remove, |ui| cross_button(ui, remove.size(), ButtonKind::Red)).clicked()
}

impl NvencFfxApp {
    /// The Batch Convert button.
    pub(super) fn open_batch_converter(&mut self, ctx: &egui::Context) {
        if self.batch.open {
            ctx.send_viewport_cmd_to(viewport(), egui::ViewportCommand::Focus);
            return;
        }
        self.batch.open = true;
        self.batch.position = ctx.input(|i| i.viewport().outer_rect).map(|r| r.center() - WINDOW_SIZE / 2.0);
    }

    /// `start_batch_conversion`
    pub(super) fn start_batch(&mut self, ctx: &egui::Context) {
        if self.batch.files.is_empty() || self.batch.running() {
            return;
        }
        self.batch.run = Some(Run {
            index: 0,
            removed: false,
            had_error: false,
            saved_input: self.input_file.clone(),
            saved_output: self.output_file.clone(),
        });
        self.progress = Some(0.0);
        self.status_message = "Conversion in progress...".to_owned();
        self.taskbar(Taskbar::Start);
        self.convert_next_file(ctx);
    }

    /// Starts the file the batch is at (as the main window's
    /// input, with an output named after it), or ends the batch after the last one.
    fn convert_next_file(&mut self, ctx: &egui::Context) {
        loop {
            let Some(run) = &self.batch.run else { return };
            let Some(file) = self.batch.files.get_mut(run.index) else {
                return self.batch_completed();
            };
            file.status = Status::Converting;
            let input = file.path.clone();
            let container = self.s.batch_change_container.then_some(self.s.batch_output_container.as_str());
            let output = output_path(
                Path::new(&input),
                self.s.codec,
                &self.s.batch_output_folder,
                &self.s.last_output_dir,
                container,
            );
            self.input_file.clone_from(&input);
            self.output_file = output.to_string_lossy().into_owned();
            // An applied custom command gets this file's input and output
            match command::preview(self.custom_command.as_deref(), &self.s, &self.job()) {
                Ok(args) => {
                    let runner = Runner::start(vec![args.clone()], ctx);
                    self.task = Some(Task { kind: TaskKind::Convert { input, args }, runner, paused: false });
                    return;
                }
                Err(e) => self.batch_file_done(Status::Failed(Some(e))),
            }
        }
    }

    /// Records the result of the file being converted and moves past it.
    fn batch_file_done(&mut self, status: Status) {
        let Some(run) = &mut self.batch.run else { return };
        if matches!(status, Status::Failed(_)) {
            run.had_error = true;
        }
        if run.removed {
            run.removed = false;
            return;
        }
        if let Some(file) = self.batch.files.get_mut(run.index) {
            file.status = status;
        }
        run.index += 1;
    }

    /// The current file's ffmpeg is done; on to the next.
    pub(super) fn batch_file_finished(&mut self, exit: Exit, ctx: &egui::Context) {
        let status = match exit {
            Exit::Success => Status::Done,
            Exit::Failed(_) => Status::Failed(None),
            Exit::SpawnFailed(e) => Status::Failed(Some(e.to_string())),
            Exit::Cancelled => return self.cancel_batch(),
        };
        self.batch_file_done(status);
        self.convert_next_file(ctx);
    }

    /// The end of a batch. No completion sound.
    fn batch_completed(&mut self) {
        let Some(run) = self.batch.run.take() else { return };
        self.progress = None;
        self.ffmpeg_output.clear();
        self.status_message = "Batch conversion completed!".to_owned();
        self.taskbar(if run.had_error { Taskbar::Error } else { Taskbar::Clear });
        self.restore_input_output(run);
    }

    /// `cancel_batch_conversion`: the batch ends at once; the terminated ffmpeg's
    /// thread finishes on its own.
    pub(super) fn cancel_batch(&mut self) {
        if let Some(task) = self.task.take() {
            task.runner.cancel();
        }
        let Some(run) = self.batch.run.take() else { return };
        if !run.removed
            && let Some(file) = self.batch.files.get_mut(run.index)
        {
            file.status = Status::Cancelled;
        }
        self.ffmpeg_output.clear();
        self.status_message = "Batch conversion cancelled".to_owned();
        self.progress = None;
        self.taskbar(Taskbar::Clear);
        self.restore_input_output(run);
    }

    /// The main window's own files, if it had any.
    fn restore_input_output(&mut self, run: Run) {
        if !run.saved_input.is_empty() {
            self.input_file = run.saved_input;
        }
        if !run.saved_output.is_empty() {
            self.output_file = run.saved_output;
        }
    }

    fn batch_add_files(&mut self) {
        let files = file_dialog("Select Video Files", start_dir(&[&self.s.last_input_dir]))
            .add_filter("Video Files", VIDEO_EXTENSIONS)
            .add_filter("All Files", &["*"])
            .pick_files();
        for file in files.unwrap_or_default() {
            self.batch.add(file.to_string_lossy().into_owned());
        }
    }

    fn batch_select_output_folder(&mut self) {
        let dir = start_dir(&[&self.s.batch_output_folder, &self.s.last_output_dir]);
        if let Some(folder) = file_dialog("Select Output Folder", dir).pick_folder() {
            self.s.batch_output_folder = folder.to_string_lossy().into_owned();
        }
    }

    /// Video files join the list, anything else gets a warning.
    fn batch_files_dropped(&mut self, files: Vec<PathBuf>) {
        let mut unsupported = false;
        for file in files {
            if has_extension(&file, VIDEO_EXTENSIONS) {
                self.batch.add(file.to_string_lossy().into_owned());
            } else {
                unsupported = true;
            }
        }
        if unsupported {
            self.show_dialog_in(viewport(), Kind::Warning, "Unsupported File",
                "Please drop a video file (.mp4, .mkv, .avi, etc.)", Action::None);
        }
    }

    /// Shows the window while it is open.
    pub(super) fn show_batch_window(&mut self, ctx: &egui::Context) {
        if !self.batch.open {
            return;
        }
        let mut builder = ViewportBuilder::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size(WINDOW_SIZE)
            .with_min_inner_size(WINDOW_SIZE)
            .with_decorations(false)
            .with_resizable(true)
            .with_icon(self.icon.clone());
        if let Some(position) = self.batch.position {
            builder = builder.with_position(position);
        }
        ctx.show_viewport_immediate(viewport(), builder, |ui, _| self.batch_window_ui(ui));
    }

    fn batch_window_ui(&mut self, ui: &mut Ui) {
        enum Clicked {
            OutputFolder,
            AddFiles,
            Close,
            RemoveAll,
            Remove(usize),
        }
        let mut clicked = None;
        if ui.input(|i| i.viewport().close_requested()) {
            clicked = Some(Clicked::Close);
        }
        let ctx = ui.ctx().clone();
        if let Some(files) = handle_dropped_files(&ctx) {
            self.batch_files_dropped(files);
        }

        let panel = egui::Frame::new().fill(colors::PRIMARY_BG);
        egui::CentralPanel::no_frame().frame(panel).show(ui, |ui| {
            if dialogs::title_bar(ui, WINDOW_TITLE) {
                clicked = Some(Clicked::Close);
            }
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let area = ui.available_rect_before_wrap().shrink(10.0);
            let h = dimensions::WIDGET_HEIGHT;
            let line = |y: f32| Rect::from_min_size(pos2(area.min.x, y), vec2(area.width(), h));
            // The list takes what the rows below it (and their padding) leave
            let list = area.with_max_y(area.max.y - (5.0 + h + 5.0 + h + 10.0 + 5.0 + h + 5.0));

            let mut y = list.max.y + 5.0;
            let folder = output_label(&self.s.batch_output_folder, &self.s.last_output_dir);
            place(ui, line(y), |ui| {
                colored_label(ui, "Output: ", colors::ACCENT_GREEN);
                colored_label(ui, &folder, colors::TEXT_COLOR_W);
            });

            y += h + 5.0;
            let (r, col) = (line(y), area.width() / 3.0);
            let folder_button = cell(r, 0.0, col - 5.0);
            if place(ui, folder_button, |ui| button(ui, folder_button.size(), "Output Folder", ButtonKind::Green)).clicked() {
                clicked = Some(Clicked::OutputFolder);
            }
            // Kept against the container menu so the two stay together when the window widens
            place(ui, cell(r, col, col - 10.0), |ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    checkbox(ui, &mut self.s.batch_change_container, "Change output container")
                })
            });
            let enabled = self.s.batch_change_container;
            place(ui, cell(r, 2.0 * col, col), |ui| {
                option_menu(ui, "batch_container", col, &mut self.s.batch_output_container, CONTAINERS, enabled)
            });

            y += h + 10.0 + 5.0;
            let (r, w) = (line(y), (area.width() - 10.0) / 3.0);
            let buttons = [
                ("Add Files", ButtonKind::Green, Clicked::AddFiles),
                ("Close", ButtonKind::Grey, Clicked::Close),
                ("Remove All", ButtonKind::Red, Clicked::RemoveAll),
            ];
            for (i, (text, kind, action)) in buttons.into_iter().enumerate() {
                let rect = cell(r, (w + 5.0) * i as f32, w);
                if place(ui, rect, |ui| button(ui, rect.size(), text, kind)).clicked() {
                    clicked = Some(action);
                }
            }

            ui.painter().rect_filled(list, CornerRadius::same(dimensions::CORNER_RADIUS), colors::SECONDARY_BG);
            ui.scope_builder(UiBuilder::new().max_rect(list.shrink(5.0)), |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                    .show(ui, |ui| {
                        for (i, file) in self.batch.files.iter().enumerate() {
                            ui.add_space(2.0);
                            if file_row(ui, i, file) {
                                clicked = Some(Clicked::Remove(i));
                            }
                            ui.add_space(2.0);
                        }
                    });
            });
        });
        dialogs::resize_edges(ui);
        file_drag_drop_overlay(&ctx, "Drop video files to add them to the batch");
        self.dialog_ui(ui, viewport());

        match clicked {
            Some(Clicked::OutputFolder) => self.batch_select_output_folder(),
            Some(Clicked::AddFiles) => self.batch_add_files(),
            Some(Clicked::Close) => self.batch.open = false,
            Some(Clicked::RemoveAll) => self.batch.clear(),
            Some(Clicked::Remove(i)) => self.batch.remove(i),
            None => {}
        }
        // A message still open moves to the main window
        if !self.batch.open
            && let Some((vp, _)) = &mut self.dialog
            && *vp == viewport()
        {
            *vp = ViewportId::ROOT;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_names() {
        let input = Path::new(r"C:\v\clip.MKV");
        let out = |folder, last, container| output_path(input, VideoCodec::Hevc, folder, last, container);
        assert_eq!(out("", "", None), Path::new(r"C:\v\clip_hevc_custom.MKV"));
        assert_eq!(out(r"D:\out", "", Some("mov")), Path::new(r"D:\out\clip_hevc_custom.mov"));
        assert_eq!(out("", r"Z:\missing", None), Path::new(r"C:\v\clip_hevc_custom.MKV"), "a vanished folder is skipped");
        let temp = std::env::temp_dir();
        let temp = temp.to_str().unwrap();
        assert_eq!(out("", temp, None), Path::new(temp).join("clip_hevc_custom.MKV"));
        assert_eq!(
            output_path(Path::new(r"C:\v\noext"), VideoCodec::Av1, "", "", None),
            Path::new(r"C:\v\noext_av1_custom.mp4")
        );

        assert_eq!(output_label("", ""), "Original folder(s)");
        assert_eq!(output_label(r"D:\out", temp), r"D:\out");
        let long = format!(r"D:\{}", "é".repeat(90));
        assert_eq!(output_label(&long, "").chars().count(), 85);
        assert!(output_label(&long, "").ends_with("..."));
        assert_eq!(truncate("short.mp4", 40), "short.mp4");
    }

    fn files(batch: &Batch) -> Vec<&str> {
        batch.files.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn removing_files_keeps_the_running_batch_on_track() {
        let mut batch = Batch::default();
        for f in ["a", "b", "a", "c", "d"] {
            batch.add(f.to_owned());
        }
        assert_eq!(files(&batch), ["a", "b", "c", "d"], "no duplicates");
        let run = |index| Run { index, removed: false, had_error: false, saved_input: String::new(), saved_output: String::new() };

        batch.run = Some(run(2)); // converting "c"
        batch.remove(0);
        assert_eq!(batch.run.as_ref().unwrap().index, 1, "still on c");
        batch.remove(1); // "c" itself
        let r = batch.run.as_ref().unwrap();
        assert!(r.removed && r.index == 1 && batch.files[1].path == "d", "d is next");
        batch.remove(1); // "d", while c's ffmpeg still runs
        assert!(batch.run.as_ref().unwrap().removed);
        assert_eq!(files(&batch), ["b"]);

        batch.add("e".to_owned());
        batch.clear();
        let r = batch.run.as_ref().unwrap();
        assert!(r.removed && r.index == 0);

        batch.files = ["a", "b", "c", "d"].map(|p| BatchFile { path: p.into(), status: Status::Ready }).into();
        batch.run = Some(run(1));
        assert_eq!(batch.overall_progress(0.5), 0.375, "(1 + 0.5) / 4");
        batch.run = None;
        assert_eq!(batch.overall_progress(0.5), 0.5);
    }

    /// Runs the batch until it ends.
    fn finish_batch(app: &mut NvencFfxApp, ctx: &egui::Context) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while app.batch.running() && std::time::Instant::now() < deadline {
            app.poll_task(ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!app.batch.running(), "batch finished");
    }

    fn statuses(app: &NvencFfxApp) -> Vec<String> {
        app.batch.files.iter().map(|f| f.status.text()).collect()
    }

    /// A custom command standing in for ffmpeg. It gets each file as its input (`%2`),
    /// prints a line, runs for a while if the file ends in `.wait`, and exits with the
    /// file's name as its code.
    #[cfg(windows)]
    fn fake_ffmpeg(name: &str) -> Vec<String> {
        let bat = std::env::temp_dir().join(format!("nvencffx-{name}-{}.bat", std::process::id()));
        let script = "@echo off\r\necho started %2\r\n\
                      if \"%~x2\"==\".wait\" ping -n 30 127.0.0.1 >nul\r\nexit /b %~n2\r\n";
        std::fs::write(&bat, script).unwrap();
        vec![bat.to_string_lossy().into_owned(), "-i".into(), "x".into(), "out".into()]
    }

    #[cfg(windows)]
    #[test]
    fn batch_converts_each_file_and_restores_the_main_files() {
        let ctx = egui::Context::default();
        let mut app = NvencFfxApp { input_file: r"C:\v\mine.mp4".into(), ..Default::default() };
        app.custom_command = Some(fake_ffmpeg("batch-codes"));
        for file in [r"C:\v\0.mp4", r"C:\v\1.mp4", r"C:\w\0.mp4"] {
            app.batch.add(file.into());
        }
        app.s.batch_change_container = true;
        app.s.batch_output_container = "mkv".into();
        app.s.batch_output_folder = r"D:\out".into();
        app.toggle_conversion(&ctx);
        assert!(app.batch.running());
        assert_eq!(app.status_message, "Conversion in progress...");
        assert_eq!(app.input_file, r"C:\v\0.mp4");
        assert_eq!(app.output_file, r"D:\out\0_hevc_custom.mkv");

        finish_batch(&mut app, &ctx);
        assert_eq!(statuses(&app), ["Done", "Failed", "Done"]);
        assert_eq!(app.status_message, "Batch conversion completed!");
        assert_eq!(app.progress, None);
        assert_eq!(app.input_file, r"C:\v\mine.mp4", "the main input is back");
        assert_eq!(app.output_file, r"D:\out\0_hevc_custom.mkv", "an empty output was not restored");
        assert!(app.dialog.is_none(), "no message boxes in a batch");
    }

    #[cfg(windows)]
    #[test]
    fn batch_cancel_pause_and_build_errors() {
        let ctx = egui::Context::default();
        let mut app = NvencFfxApp::default();
        app.batch.add(r"C:\v\a.mp4".into());
        // No FFmpeg path: the file fails without stopping the batch
        app.toggle_conversion(&ctx);
        assert!(!app.batch.running());
        assert_eq!(statuses(&app), ["Failed: FFmpeg path is not specified"]);

        app.custom_command = Some(fake_ffmpeg("batch-cancel"));
        app.batch.clear();
        app.batch.add(r"C:\v\b.wait".into());
        app.batch.add(r"C:\v\0.mp4".into());
        app.toggle_conversion(&ctx);
        assert_eq!(statuses(&app), ["Converting", "Ready"]);
        // Wait until the script runs
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.ffmpeg_output.is_empty() && std::time::Instant::now() < deadline {
            app.poll_task(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        app.toggle_pause();
        assert_eq!(app.status_message, "Batch conversion paused");
        app.toggle_conversion(&ctx);
        assert_eq!(app.status_message, "Conversion in progress...", "left click on Resume resumes");
        app.toggle_preview(&ctx);
        assert_eq!(app.dialog.take().unwrap().1.message, "Please wait until current process completes");

        app.toggle_conversion(&ctx);
        assert!(!app.batch.running() && app.task.is_none());
        assert_eq!(statuses(&app), ["Cancelled", "Ready"]);
        assert_eq!(app.status_message, "Batch conversion cancelled");
        assert!(app.dialog.is_none());
    }

    #[test]
    fn window_lists_files_and_its_buttons_work() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply_theme(&ctx);
        let mut app = NvencFfxApp::default();
        for f in ["a.mp4", "b.mkv"] {
            app.batch.add(format!(r"C:\v\{f}"));
        }
        app.batch.files[1].status = Status::Failed(Some("a long reason that pushes the name aside".into()));
        let click = |app: &mut NvencFfxApp, pos: Pos2| {
            for pressed in [None, Some(true), Some(false)] {
                let events = match pressed {
                    None => vec![egui::Event::PointerMoved(pos)],
                    Some(pressed) => vec![egui::Event::PointerButton {
                        pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default(),
                    }],
                };
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, WINDOW_SIZE)),
                    events,
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| app.batch_window_ui(ui));
                out.textures_delta.clear();
            }
        };
        let top = dialogs::TITLE_BAR_HEIGHT + 10.0;
        // The checkbox enables the container menu
        let container_row = WINDOW_SIZE.y - 10.0 - 5.0 - 28.0 - 5.0 - 10.0 - 28.0 + 14.0;
        click(&mut app, pos2(10.0 + 580.0 / 3.0 + 12.0, container_row));
        assert!(app.s.batch_change_container);
        // × of the first file (list padding 5, row padding 2)
        click(&mut app, pos2(10.0 + 580.0 - 5.0 - 15.0, top + 5.0 + 2.0 + 14.0));
        assert_eq!(files(&app.batch), [r"C:\v\b.mkv"]);
        // Close hides the window
        app.batch.open = true;
        click(&mut app, pos2(300.0, WINDOW_SIZE.y - 10.0 - 5.0 - 14.0));
        assert!(!app.batch.open);
    }
}
