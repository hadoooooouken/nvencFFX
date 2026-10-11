use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use eframe::egui::{
    self, Align, Align2, FontId, Layout, Margin, Rect, ScrollArea, TextEdit, Ui, Vec2,
    ViewportBuilder, ViewportCommand, ViewportId, vec2,
};
use crate::config::{
    presets,
    settings::{self, Settings, VideoCodec},
};
use crate::core::{
    command, estimate,
    filters::{self, QUICK_BUTTONS, Quick},
    probe::{self, Duration, DurationCache, InfoCache},
    process::{self, Event, Exit, Runner},
    progress, timecode,
};
use crate::ui::{
    dialogs::{self, Dialog, Kind, Outcome},
    theme::{apply_theme, colors, dimensions},
    widgets::{
        ButtonKind, EntryStyle, bold_button, button, cell, checkbox, checkbox_enabled,
        colored_label, entry, file_drag_drop_overlay, handle_dropped_files, label, option_menu,
        place, progress_bar, radio_value, reset_button, row, section_frame, sub_rect,
        text_checkbox, trim_slider,
    },
};

/// Pixel grid of the original CustomTkinter layout (`nvencFFX.py`, 100% scaling).
/// X offsets are relative to the content origin, which sits 31px from the window edge.
mod grid {
    pub const LABEL_X: f32 = 10.0;
    pub const ENTRY_X: f32 = 226.0;
    pub const ENTRY_W: f32 = 350.0;
    pub const BUTTON_X: f32 = 586.0;
    pub const ROW_H: f32 = 38.0; // 28px widget + 2 * 5px grid pady
    pub const SECTION_X: f32 = 10.0;
    pub const SECTION_W: f32 = 736.0;
    pub const SMALL_BUTTON_W: f32 = 80.0;
    /// Center of grid columns 0..=2, where the status line is centered.
    pub const STATUS_CENTER_X: f32 = 366.0;

    // Window chrome (egui margins are whole pixels)
    pub const OUTER_PAD: i8 = 10; // CTkScrollableFrame padx/pady
    pub const CONTENT_PAD: i8 = 21; // scrollable frame inset + main_frame padx
    pub const BOTTOM_PAD_X: i8 = 15;
    pub const BOTTOM_PAD_Y: i8 = 20;
}

const THREADS: &[&str] = &[
    "auto", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
];
const PRESETS: &[&str] = &["auto", "p1", "p2", "p3", "p4", "p5", "p6", "p7"];
const TUNES: &[&str] = &["auto", "hq", "uhq", "ll", "ull", "lossless"];
const TUNES_H264: &[&str] = &["auto", "hq", "ll", "ull", "lossless"];
const PROFILES_HEVC: &[&str] = &["auto", "main", "main10", "rext"];
const PROFILES_H264: &[&str] = &["auto", "baseline", "main", "high", "high10", "high422", "high444p"];
const LEVELS_HEVC: &[&str] = &[
    "auto", "1.0", "2.0", "2.1", "3.0", "3.1", "4.0", "4.1", "5.0", "5.1", "5.2", "6.0", "6.1", "6.2",
];
const LEVELS_H264: &[&str] = &[
    "auto", "1b", "1.0b", "1.1", "1.2", "1.3", "2.0", "2.1", "2.2", "3.0", "3.1", "3.2", "4.0",
    "4.1", "4.2", "5.0", "5.1", "5.2", "6.0",
];
const LEVELS_AV1: &[&str] = &[
    "auto", "2.0", "2.1", "2.2", "2.3", "3.0", "3.1", "3.2", "3.3", "4.0", "4.1", "4.2", "4.3",
    "5.0", "5.1", "5.2", "5.3", "6.0", "6.1", "6.2", "6.3", "7.0", "7.1", "7.2", "7.3",
];
const TIERS: &[&str] = &["auto", "0", "1"];
const CODERS: &[&str] = &["auto", "cabac", "cavlc", "ac", "vlc"];
const HWACCELS: &[&str] = &["auto", "cuda", "d3d11va", "d3d12va", "opencl", "vulkan"];
const MULTIPASS: &[&str] = &["auto", "disabled", "qres", "fullres"];
const RATE_CONTROLS: &[&str] = &["vbr", "cbr"];
const LOOKAHEAD: &[&str] = &[
    "auto", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15",
];
const SPLIT_ENCODE: &[&str] = &["auto", "disabled", "forced", "2", "3"];
const INTERPOLATIONS: &[&str] = &["bilinear", "bicubic", "neighbor", "area", "lanczos", "spline"];

/// Accepted by drag & drop and listed in the Browse filter.
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "flv", "wmv", "webm", "ts", "m4v", "mpg", "mpeg", "m2ts", "mts",
    "3gp", "ogv", "ogm", "vob", "f4v", "asf", "divx",
];
/// Containers an auto-named output keeps; anything else becomes `.mp4`.
const PRESERVE_EXTENSIONS: &[&str] = &["mp4", "mkv", "mov", "ts", "mts", "m2ts", "webm"];

const INPUT_PLACEHOLDER: &str = "Drag and drop a video file here or use the 'Browse' button.";
const OUTPUT_PLACEHOLDER: &str = "Also used as the destination file for screen recordings";
const FFMPEG_PLACEHOLDER: &str = "Path to ffmpeg.exe (required)";
const FF_OPTIONS_PLACEHOLDER: &str =
    "e.g. -aq-strength 8; -cq 0; -bf 0 or -bf 4; -intra-refresh 1; -forced-idr 1";
const VF_PLACEHOLDER: &str = "e.g.setpts=0.5*PTS - speed up video x2; crop=iw:min(ih\\,iw*9/16)";
const AF_PLACEHOLDER: &str = "e.g. atempo=2.0, volume=1.5";

const COMMAND_WINDOW_TITLE: &str = "FFmpeg Command Preview - Editable";

fn command_viewport() -> ViewportId {
    ViewportId::from_hash_of("command_preview")
}

/// `nff_settings.json` is written once the settings stay unchanged this long
/// (`_on_setting_changed`), and again on exit.
const SAVE_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

/// What to do with a dialog's answer.
enum Action {
    None,
    /// "Save Preset" name prompt
    SavePresetAs,
    OverwritePreset(String),
    DeletePreset(String),
    /// Warning before falling back to Save As
    ThenSavePresetAs,
}

/// The editable command preview (Output button).
struct CommandWindow {
    text: String,
}

/// What the running ffmpeg is doing.
enum TaskKind {
    /// Convert; the command and its input give the length `time=` counts towards.
    Convert { input: String, args: Vec<String> },
    /// Play 10s Preview: stream copy (step 0), then encoding to `encoded`.
    Preview { encoded: PathBuf },
}

struct Task {
    kind: TaskKind,
    runner: Runner,
}

/// How long closing the app waits for a cancelled ffmpeg to exit.
const STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub struct NvencFfxApp {
    /// Everything a preset / `nff_settings.json` holds.
    pub s: Settings,
    /// Where `s` is auto-saved; `None` (tests) never writes.
    settings_file: Option<PathBuf>,
    /// `s` as of the last change seen, and when it is due to be saved.
    observed_settings: Settings,
    save_due: Option<Instant>,
    pub input_file: String,
    pub output_file: String,

    // Collapsible sections (TextCheckbox)
    pub expand_encoder_settings: bool,
    pub expand_fps_scale_settings: bool,
    pub expand_audio_settings: bool,
    pub expand_additional_options: bool,
    pub expand_presets: bool,

    // Presets
    presets_dir: PathBuf,
    pub custom_presets: Vec<String>,
    preset_indicator: String,
    preset_indicator_active: bool,

    // ffprobe
    ffprobe: Option<PathBuf>,
    durations: DurationCache,
    /// Input tooltip texts
    media_info: InfoCache,
    /// Input whose `trim_end` should be set to its duration once probed.
    trim_end_follows: Option<String>,

    // Custom command mode
    custom_command: Option<Vec<String>>,
    command_window: Option<CommandWindow>,

    /// The running conversion or preview.
    task: Option<Task>,
    /// Temp clips of the last 10 s preview, deleted before the next one and on exit.
    preview_files: Vec<PathBuf>,

    dialog: Option<(ViewportId, Dialog<Action>)>,
    icon: Arc<egui::IconData>,

    // Status display
    pub status_message: String,
    /// Last FFmpeg / action message under the status line.
    pub ffmpeg_output: String,
    /// Conversion progress (0..=1) while a conversion runs.
    pub progress: Option<f32>,
}

impl Default for NvencFfxApp {
    fn default() -> Self {
        Self {
            s: Settings::default(),
            settings_file: None,
            observed_settings: Settings::default(),
            save_due: None,
            input_file: String::new(),
            output_file: String::new(),

            expand_encoder_settings: false,
            expand_fps_scale_settings: false,
            expand_audio_settings: false,
            expand_additional_options: false,
            expand_presets: false,

            presets_dir: PathBuf::from("presets"),
            custom_presets: Vec::new(),
            preset_indicator: "No preset selected".to_owned(),
            preset_indicator_active: false,

            ffprobe: None,
            durations: DurationCache::default(),
            media_info: InfoCache::default(),
            trim_end_follows: None,

            custom_command: None,
            command_window: None,

            task: None,
            preview_files: Vec::new(),

            dialog: None,
            icon: Arc::new(egui::IconData::default()),

            status_message: "Ready for conversion".to_owned(),
            ffmpeg_output: String::new(),
            progress: None,
        }
    }
}

fn interpolation_description(algo: &str) -> &'static str {
    match algo {
        "bilinear" => "Fast, low-quality scaling.",
        "bicubic" => "Smooth and balanced quality.",
        "neighbor" => "Blocky, pixelated output.",
        "area" => "Soft result for downscaling.",
        "lanczos" => "Sharp and high-quality.",
        "spline" => "Visually best in theory.",
        _ => "",
    }
}

/// Label + widget placed on a sub-rect of a section frame.
fn place_label(ui: &mut Ui, rect: Rect, text: &str) {
    place(ui, rect, |ui| label(ui, text));
}

fn has_extension(path: &Path, list: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| list.contains(&e.to_ascii_lowercase().as_str()))
}

/// `.ext` of `path` if it is a preserved container, `.mp4` otherwise.
fn output_extension(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some(e) if has_extension(path, PRESERVE_EXTENSIONS) => format!(".{e}"),
        _ => ".mp4".to_owned(),
    }
}

/// `<input stem>_<codec>_custom.<ext>` in `last_output_dir` (if it exists) or next to the input.
fn default_output(input: &Path, codec: VideoCodec, last_output_dir: &str) -> PathBuf {
    let dir = if !last_output_dir.is_empty() && Path::new(last_output_dir).exists() {
        PathBuf::from(last_output_dir)
    } else {
        input.parent().map(Path::to_path_buf).unwrap_or_default()
    };
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    dir.join(format!("{stem}_{}_custom{}", codec.suffix(), output_extension(input)))
}

/// `_update_output_filename`: an auto-named output follows a codec change. Outputs
/// that do not carry the `_<codec>_custom.` marker were chosen by hand and are kept.
fn renamed_output(output: &str, input: &str, codec: VideoCodec) -> Option<String> {
    if input.is_empty() {
        return None;
    }
    let cut = ["hevc", "h264", "av1"]
        .iter()
        .filter_map(|c| output.find(&format!("_{c}_custom.")))
        .min()?;
    let out = Path::new(output);
    let base = Path::new(&output[..cut]).file_name()?.to_string_lossy().into_owned();
    let dir = match out.parent() {
        Some(d) if d.is_dir() => d.to_path_buf(),
        _ => Path::new(input).parent().map(Path::to_path_buf).unwrap_or_default(),
    };
    let name = format!("{base}_{}_custom{}", codec.suffix(), output_extension(out));
    Some(dir.join(name).to_string_lossy().into_owned())
}

/// "FFmpeg exited with error code N" with the last output line; Windows codes are
/// shown unsigned, as Python reported them.
fn exit_message(code: Option<i32>, last_line: &str) -> String {
    let code = code.map_or_else(|| "unknown".to_owned(), |c| (c as u32).to_string());
    format!("FFmpeg exited with error code {code}.\nLast output: {last_line}")
}

/// `_explore_path`: selects a file in Explorer, or opens a folder.
fn explore(path: &str) {
    let path = path.trim_matches(|c| matches!(c, '"' | '\'' | ' '));
    if path.is_empty() {
        return;
    }
    let mut target = Path::new(path);
    if !target.exists() {
        match target.parent() {
            Some(p) if !p.as_os_str().is_empty() && p.exists() => target = p,
            _ => return,
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("explorer");
        if target.is_file() {
            cmd.raw_arg(format!("/select,\"{}\"", target.display()));
        } else {
            cmd.arg(target);
        }
        let _ = cmd.spawn();
    }
}

impl NvencFfxApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_theme(&cc.egui_ctx);
        // egui would re-theme the visible window via winit, which Windows 10 renders as
        // dark caption text on the light title bar. Set it once, before showing, instead.
        cc.egui_ctx.options_mut(|o| o.sync_window_theme = false);
        #[cfg(windows)]
        crate::win32::use_dark_title_bar(cc);

        let presets_dir = presets::presets_dir();
        let mut app = Self {
            custom_presets: presets::list(&presets_dir),
            presets_dir,
            icon: Arc::new(crate::window_icon()),
            ..Self::default()
        };
        if let Some(ffmpeg) = probe::find_executable("ffmpeg.exe") {
            app.s.ffmpeg_path = ffmpeg.to_string_lossy().into_owned();
        }
        // A saved FFmpeg path wins over the detected one if it still exists
        let file = settings::settings_file();
        if let Err(e) = app.s.load(&file) {
            eprintln!("Error loading settings: {e}");
        }
        app.settings_file = Some(file);
        app.ffprobe = probe::ffprobe_for(&app.s.ffmpeg_path);
        app.fix_codec_dependent();
        app.restore_preset_selection();
        app.observed_settings = app.s.clone();
        // "Send to nvencFFX" passes the file as the first argument
        if let Some(arg) = std::env::args_os().nth(1) {
            app.select_input(PathBuf::from(arg), true);
        }
        app
    }

    // -- Dialogs -------------------------------------------------------------

    fn show_dialog_in(&mut self, viewport: ViewportId, kind: Kind, title: &str, message: impl Into<String>, action: Action) {
        self.dialog = Some((viewport, Dialog::new(kind, title, message, action)));
    }

    fn message(&mut self, kind: Kind, title: &str, message: impl Into<String>) {
        self.show_dialog_in(ViewportId::ROOT, kind, title, message, Action::None);
    }

    /// Draws the pending dialog if it belongs to `viewport` and handles its answer.
    fn dialog_ui(&mut self, ui: &Ui, viewport: ViewportId) {
        let Some((vp, dialog)) = &mut self.dialog else { return };
        if *vp != viewport {
            return;
        }
        if let Some(outcome) = dialog.show(ui)
            && let Some((_, dialog)) = self.dialog.take()
        {
            self.on_dialog(dialog.action, outcome);
        }
    }

    fn on_dialog(&mut self, action: Action, outcome: Outcome) {
        match (action, outcome) {
            (Action::SavePresetAs, Outcome::Accepted(name)) => {
                let name = name.trim().to_owned();
                if name.is_empty() {
                    return;
                }
                if presets::path(&self.presets_dir, &name).exists() {
                    let text = format!("Preset '{name}' already exists. Overwrite?");
                    self.show_dialog_in(ViewportId::ROOT, Kind::Question, "Overwrite", text,
                        Action::OverwritePreset(name));
                } else {
                    self.finish_save_as(name);
                }
            }
            (Action::OverwritePreset(name), Outcome::Accepted(_)) => self.finish_save_as(name),
            (Action::DeletePreset(name), Outcome::Accepted(_)) => self.finish_delete(name),
            (Action::ThenSavePresetAs, _) => self.save_preset_as(),
            _ => {}
        }
    }

    // -- Files ---------------------------------------------------------------

    /// A new input from Browse, drag & drop or the command line.
    fn select_input(&mut self, path: PathBuf, dropped: bool) {
        if dropped && !has_extension(&path, VIDEO_EXTENSIONS) {
            self.message(Kind::Warning, "Unsupported File", "Please drop a video file (.mp4, .mkv, .avi, etc.)");
            return;
        }
        self.input_file = path.to_string_lossy().into_owned();
        if let Some(dir) = path.parent() {
            self.s.last_input_dir = dir.to_string_lossy().into_owned();
        }
        self.output_file = default_output(&path, self.s.codec, &self.s.last_output_dir)
            .to_string_lossy()
            .into_owned();
        self.s.trim_start = "00:00:00".to_owned();
        self.trim_end_follows = Some(self.input_file.clone());
        self.status_message = "File selected. Ready for conversion.".to_owned();
        self.custom_command = None;
    }

    fn handle_drag_and_drop(&mut self, ctx: &egui::Context) {
        let Some(files) = handle_dropped_files(ctx) else { return };
        if let Some(first) = files.into_iter().next() {
            self.select_input(first, true);
        }
    }

    fn browse_input(&mut self, parent: &eframe::Frame) {
        // The saved folder may be gone since the last session
        let dir = Some(PathBuf::from(&self.s.last_input_dir))
            .filter(|d| !d.as_os_str().is_empty() && d.is_dir())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let file = rfd::FileDialog::new()
            .set_title("Select Video File")
            .set_directory(dir)
            .add_filter("Video Files", VIDEO_EXTENSIONS)
            .add_filter("All Files", &["*"])
            .set_parent(parent)
            .pick_file();
        if let Some(file) = file {
            self.select_input(file, false);
        }
    }

    fn browse_output(&mut self, parent: &eframe::Frame) {
        let output = Path::new(&self.output_file);
        let ext = match output.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_owned(),
            None => Path::new(&self.input_file)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("mp4")
                .to_owned(),
        };
        let name = match output.file_stem() {
            Some(stem) => format!("{}.{ext}", stem.to_string_lossy()),
            None => format!("output_{}_custom.{ext}", self.s.codec.suffix()),
        };
        let dir = [Path::new(&self.s.last_output_dir), output.parent().unwrap_or(Path::new(""))]
            .into_iter()
            .find(|d| !d.as_os_str().is_empty() && d.exists())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        // The current container's filter first
        let mut filters = vec![("MP4 Files", "mp4"), ("MKV Files", "mkv"), ("MOV Files", "mov")];
        if let Some(i) = filters.iter().position(|(_, e)| e.eq_ignore_ascii_case(&ext)) {
            let f = filters.remove(i);
            filters.insert(0, f);
        }
        let mut dialog = rfd::FileDialog::new().set_title("Save As...").set_directory(dir).set_file_name(name);
        for (title, e) in filters {
            dialog = dialog.add_filter(title, &[e]);
        }
        if let Some(file) = dialog.add_filter("All Files", &["*"]).set_parent(parent).save_file() {
            self.output_file = file.to_string_lossy().into_owned();
            if let Some(dir) = file.parent() {
                self.s.last_output_dir = dir.to_string_lossy().into_owned();
            }
            self.custom_command = None;
        }
    }

    fn browse_ffmpeg(&mut self, parent: &eframe::Frame) {
        let file = rfd::FileDialog::new()
            .set_title("Select FFmpeg Executable")
            .add_filter("Executable Files", &["exe"])
            .add_filter("All Files", &["*"])
            .set_parent(parent)
            .pick_file();
        if let Some(file) = file {
            self.s.ffmpeg_path = file.to_string_lossy().into_owned();
            let sibling = file.with_file_name("ffprobe.exe");
            self.ffprobe = sibling.is_file().then_some(sibling);
        }
    }

    /// FFmpeg Path typed in by hand (`_on_ffmpeg_path_focus_out`).
    fn ffmpeg_path_edited(&mut self) {
        let path = Path::new(&self.s.ffmpeg_path);
        if !path.is_file() {
            return;
        }
        let sibling = path.with_file_name("ffprobe.exe");
        self.status_message = if sibling.is_file() {
            "FFmpeg and FFprobe found successfully"
        } else {
            "FFmpeg found but FFprobe not found in the same directory"
        }
        .to_owned();
        self.ffprobe = sibling.is_file().then_some(sibling);
    }

    // -- Settings logic ------------------------------------------------------

    fn set_codec(&mut self, codec: VideoCodec) {
        if self.s.codec == codec {
            return;
        }
        self.s.codec = codec;
        match codec {
            VideoCodec::Hevc => self.s.encoder_profile = "main".to_owned(),
            VideoCodec::H264 => {
                self.s.encoder_profile = "main".to_owned();
                self.s.encoder_lookahead_level = "auto".to_owned();
            }
            VideoCodec::Av1 => self.s.encoder_profile.clear(),
        }
        self.fix_codec_dependent();
        if let Some(out) = renamed_output(&self.output_file, &self.input_file, codec) {
            self.output_file = out;
        }
        self.custom_command = None;
    }

    /// Brings codec-specific options back into the current codec's value lists.
    fn fix_codec_dependent(&mut self) {
        let s = &mut self.s;
        if s.codec == VideoCodec::H264 && s.encoder_tune == "uhq" {
            s.encoder_tune = "hq".to_owned();
        }
        if !self.levels().contains(&self.s.encoder_level.as_str()) {
            self.s.encoder_level = "auto".to_owned();
        }
        let s = &mut self.s;
        let profiles = match s.codec {
            VideoCodec::Hevc => PROFILES_HEVC,
            VideoCodec::H264 => PROFILES_H264,
            VideoCodec::Av1 => &[],
        };
        if s.codec == VideoCodec::Av1 {
            s.encoder_profile.clear();
        } else if !profiles.contains(&s.encoder_profile.as_str()) {
            s.encoder_profile = "main".to_owned();
        }
    }

    fn levels(&self) -> &'static [&'static str] {
        match self.s.codec {
            VideoCodec::Hevc => LEVELS_HEVC,
            VideoCodec::H264 => LEVELS_H264,
            VideoCodec::Av1 => LEVELS_AV1,
        }
    }

    /// `_on_tune_changed`
    fn tune_changed(&mut self) {
        if self.s.encoder_tune == "lossless" {
            self.s.encoder_spatial_aq = false;
            self.s.encoder_temporal_aq = false;
            self.status_message = "Note: AQ not supported for lossless encoding".to_owned();
        }
    }

    /// The Auto button: every encoder menu that offers "auto" goes to it, every flag off.
    fn apply_auto_encoder_settings(&mut self) {
        let s = &mut self.s;
        for v in [
            &mut s.encoder_threads,
            &mut s.encoder_preset,
            &mut s.encoder_tune,
            &mut s.encoder_level,
            &mut s.encoder_tier,
            &mut s.encoder_coder,
            &mut s.encoder_hwaccel,
            &mut s.encoder_multipass,
            &mut s.encoder_lookahead_level,
            &mut s.encoder_split_encode_mode,
        ] {
            *v = "auto".to_owned();
        }
        if s.codec != VideoCodec::Av1 {
            s.encoder_profile = "auto".to_owned();
        }
        for f in [
            &mut s.cuda_output_format,
            &mut s.encoder_spatial_aq,
            &mut s.encoder_temporal_aq,
            &mut s.encoder_strict_gop,
            &mut s.encoder_no_scenecut,
            &mut s.encoder_weighted_pred,
        ] {
            *f = false;
        }
    }

    fn set_preset_indicator(&mut self, text: String) {
        self.preset_indicator = text;
        self.preset_indicator_active = true;
    }

    /// Built-in preset radios and the Default button (`_apply_preset`).
    fn apply_builtin_preset(&mut self, value: &str) {
        presets::apply_builtin(&mut self.s, value);
        self.reset_trim();
        self.tune_changed();
        let text = if value == "none" {
            "Default settings applied".to_owned()
        } else {
            format!("{} preset applied", presets::builtin_name(value))
        };
        self.set_preset_indicator(text);
    }

    /// After loading `nff_settings.json`: drops a custom preset whose file is gone and
    /// shows the selection like `_update_preset_indicator`.
    fn restore_preset_selection(&mut self) {
        let name = self.s.custom_preset_selected.clone();
        if self.s.selected_preset == "custom"
            && !name.is_empty()
            && !presets::path(&self.presets_dir, &name).exists()
        {
            self.s.selected_preset = "none".to_owned();
            self.s.custom_preset_selected.clear();
        }
        let text = match self.s.selected_preset.as_str() {
            "" | "none" => return,
            "custom" if name.is_empty() => "Custom preset (unnamed)".to_owned(),
            "custom" => format!("Loaded preset: {name}"),
            value => format!("{} preset applied", presets::builtin_name(value)),
        };
        self.set_preset_indicator(text);
    }

    fn load_custom_preset(&mut self, name: &str) {
        match presets::load(&self.presets_dir, name) {
            Ok(map) => {
                let ffmpeg = self.s.ffmpeg_path.clone();
                self.s.apply_json(&map);
                if self.s.ffmpeg_path != ffmpeg {
                    self.ffprobe = probe::ffprobe_for(&self.s.ffmpeg_path);
                }
                self.fix_codec_dependent();
                self.s.custom_preset_selected = name.to_owned();
                self.s.selected_preset = "custom".to_owned();
                self.set_preset_indicator(format!("Loaded preset: {name}"));
            }
            Err(e) => self.message(Kind::Error, "Error", e),
        }
    }

    fn save_preset_as(&mut self) {
        self.show_dialog_in(ViewportId::ROOT, Kind::Prompt, "Save Preset", "Enter preset name:", Action::SavePresetAs);
    }

    fn finish_save_as(&mut self, name: String) {
        if let Err(e) = presets::save(&self.presets_dir, &name, &self.s) {
            self.message(Kind::Error, "Error", e);
            return;
        }
        self.custom_presets = presets::list(&self.presets_dir);
        self.s.custom_preset_selected = name.clone();
        self.s.selected_preset = "custom".to_owned();
        self.set_preset_indicator(format!("Preset saved: {name}"));
    }

    fn save_preset(&mut self) {
        let name = self.s.custom_preset_selected.clone();
        if name.is_empty() {
            self.save_preset_as();
        } else if !presets::path(&self.presets_dir, &name).exists() {
            let text = format!("Preset '{name}' does not exist. Saving as new preset.");
            self.show_dialog_in(ViewportId::ROOT, Kind::Warning, "Warning", text, Action::ThenSavePresetAs);
        } else if let Err(e) = presets::save(&self.presets_dir, &name, &self.s) {
            self.message(Kind::Error, "Error", e);
        } else {
            self.s.selected_preset = "custom".to_owned();
            self.set_preset_indicator(format!("Preset saved: {name}"));
        }
    }

    fn delete_preset(&mut self) {
        let name = self.s.custom_preset_selected.clone();
        let file = presets::path(&self.presets_dir, &name);
        if name.is_empty() {
            self.message(Kind::Info, "Info", "No preset selected to delete.");
        } else if !file.exists() {
            self.message(Kind::Error, "Error", format!("Preset file not found: {}", file.display()));
        } else {
            let text = format!("Are you sure you want to delete preset '{name}'?");
            self.show_dialog_in(ViewportId::ROOT, Kind::Question, "Confirm Delete", text, Action::DeletePreset(name));
        }
    }

    fn finish_delete(&mut self, name: String) {
        if let Err(e) = presets::delete(&self.presets_dir, &name) {
            self.message(Kind::Error, "Error", e);
            return;
        }
        self.custom_presets = presets::list(&self.presets_dir);
        self.s.custom_preset_selected.clear();
        self.s.selected_preset = "none".to_owned();
        self.set_preset_indicator(format!("Preset deleted: {name}"));
    }

    // -- Trimming ------------------------------------------------------------

    fn duration(&self) -> Option<f64> {
        self.durations.known(&self.input_file)
    }

    /// The slider's reset button: full range of the current input.
    fn reset_trim(&mut self) {
        self.s.trim_start = "00:00:00".to_owned();
        if self.input_file.is_empty() {
            self.s.trim_end = "00:00:00".to_owned();
        } else {
            self.trim_end_follows = Some(self.input_file.clone());
        }
    }

    fn validate_trim(&mut self, is_start: bool) -> bool {
        let (value, other) = if is_start {
            (&self.s.trim_start, &self.s.trim_end)
        } else {
            (&self.s.trim_end, &self.s.trim_start)
        };
        match timecode::validate(value, other, is_start, self.duration()) {
            Ok(()) => true,
            Err(e) => {
                self.message(Kind::Error, "Error", e);
                false
            }
        }
    }

    /// The Trim button: `-ss <from> -to <to>` replaces any trim options.
    fn add_trim_options(&mut self) {
        if !self.validate_trim(true) || !self.validate_trim(false) {
            return;
        }
        let trim = format!("-ss {} -to {}", self.s.trim_start, self.s.trim_end);
        let rest = timecode::strip_trim_options(&self.s.additional_options);
        self.s.additional_options = if rest.is_empty() { trim } else { format!("{rest} {trim}") };
    }

    /// Applies a handle drag; handles keep at least one second apart, and the end
    /// handle stops short of the last frame.
    fn trim_handle_moved(&mut self, is_start: bool, fraction: f32, duration: f64) {
        let mut secs = fraction as f64 * duration;
        if secs >= duration {
            secs = (duration - 0.1).max(0.0);
        }
        if is_start {
            let end = timecode::to_seconds(&self.s.trim_end) as f64;
            self.s.trim_start = timecode::format(secs.min(end - 1.0));
        } else {
            let start = timecode::to_seconds(&self.s.trim_start) as f64;
            self.s.trim_end = timecode::format(secs.max(start + 1.0));
        }
    }

    // -- Custom command ------------------------------------------------------

    fn job(&self) -> command::Job<'_> {
        command::Job {
            ffmpeg: &self.s.ffmpeg_path,
            input: &self.input_file,
            output: &self.output_file,
            additional_enabled: self.expand_additional_options,
        }
    }

    fn preview_command(&self) -> Result<Vec<String>, String> {
        command::preview(self.custom_command.as_deref(), &self.s, &self.job())
    }

    fn open_command_window(&mut self, ctx: &egui::Context) {
        if self.command_window.is_some() {
            ctx.send_viewport_cmd_to(command_viewport(), ViewportCommand::Focus);
            return;
        }
        if self.input_file.is_empty() {
            self.message(Kind::Error, "Error", "Please select an input file first.");
            return;
        }
        match self.preview_command() {
            Ok(cmd) => self.command_window = Some(CommandWindow { text: command::join(&cmd) }),
            Err(e) => self.message(Kind::Error, "Error", format!("Could not generate command: {e}")),
        }
    }

    /// Right-click on Output, or the preview's Copy button: the edited text if the
    /// preview is open, else the current command, with spaced paths quoted.
    fn copy_command(&mut self, ctx: &egui::Context) {
        let args = match &self.command_window {
            Some(w) => command::split(w.text.trim()),
            None => match self.preview_command() {
                Ok(cmd) => cmd,
                Err(_) => return,
            },
        };
        ctx.copy_text(command::join(&args));
        self.ffmpeg_output = "Command copied to clipboard!".to_owned();
    }

    fn apply_custom_command(&mut self) {
        let Some(window) = &self.command_window else { return };
        let text = window.text.trim().to_owned();
        let vp = command_viewport();
        if text.is_empty() {
            self.show_dialog_in(vp, Kind::Warning, "Warning", "Command is empty!", Action::None);
            return;
        }
        match command::parse_custom(&text) {
            Ok(args) => {
                self.ffmpeg_output = format!("Custom command applied: {}", command::join(&args));
                self.custom_command = Some(args);
                self.command_window = None;
            }
            Err(e) => {
                let message = format!(
                    "Invalid command: {e}\n\nPlease check:\n1. Input file (-i option)\n\
                     2. Output file path\n3. Valid FFmpeg arguments\n\nCommand: {text}"
                );
                self.show_dialog_in(vp, Kind::Error, "Error", message, Action::None);
            }
        }
    }

    fn reset_custom_command(&mut self) {
        self.custom_command = None;
        self.ffmpeg_output = "Custom command cleared – GUI settings are active again".to_owned();
        self.show_dialog_in(command_viewport(), Kind::Info, "Reset",
            "Custom command has been cleared.\nGUI settings are now active.", Action::None);
    }

    fn command_window_ui(&mut self, ui: &mut Ui) {
        enum Clicked {
            Copy,
            Apply,
            Reset,
            Close,
        }
        let mut clicked = None;
        if ui.input(|i| i.viewport().close_requested()) {
            clicked = Some(Clicked::Close);
        }
        let panel = egui::Frame::new().fill(colors::PRIMARY_BG);
        egui::CentralPanel::no_frame().frame(panel).show(ui, |ui| {
            if dialogs::title_bar(ui, COMMAND_WINDOW_TITLE) {
                clicked = Some(Clicked::Close);
            }
            let Some(window) = &mut self.command_window else { return };
            egui::Frame::new().inner_margin(Margin::same(15)).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 5.0);
                ui.vertical_centered(|ui| label(ui, "You can edit the command below:"));
                let text_h = ui.available_height() - dimensions::WIDGET_HEIGHT - 25.0;
                egui::Frame::new()
                    .fill(colors::SECONDARY_BG)
                    .corner_radius(egui::CornerRadius::same(dimensions::CORNER_RADIUS))
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ScrollArea::vertical().auto_shrink([false, false]).max_height(text_h - 16.0).show(ui, |ui| {
                            ui.add_sized(
                                vec2(ui.available_width(), text_h - 16.0),
                                TextEdit::multiline(&mut window.text)
                                    .id_salt("command_text")
                                    .font(FontId::monospace(14.0))
                                    .text_color(colors::TEXT_COLOR_W)
                                    .frame(egui::Frame::NONE),
                            );
                        });
                    });
                ui.add_space(15.0);
                let h = dimensions::WIDGET_HEIGHT;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                    let buttons = [
                        ("Copy to Clipboard", ButtonKind::Green, Clicked::Copy),
                        ("Apply Changes", ButtonKind::Green, Clicked::Apply),
                        ("Reset to Default", ButtonKind::Red, Clicked::Reset),
                    ];
                    for (text, kind, action) in buttons {
                        if button(ui, vec2(150.0, h), text, kind).clicked() {
                            clicked = Some(action);
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, vec2(106.0, h), "Close", ButtonKind::Grey).clicked() {
                            clicked = Some(Clicked::Close);
                        }
                    });
                });
            });
        });
        self.dialog_ui(ui, command_viewport());

        let ctx = ui.ctx().clone();
        match clicked {
            Some(Clicked::Copy) => self.copy_command(&ctx),
            Some(Clicked::Apply) => self.apply_custom_command(),
            Some(Clicked::Reset) => self.reset_custom_command(),
            Some(Clicked::Close) => self.command_window = None,
            None => {}
        }
        if self.command_window.is_none()
            && let Some((vp, _)) = &mut self.dialog
            && *vp == command_viewport()
        {
            *vp = ViewportId::ROOT;
        }
    }

    // -- Running ffmpeg ------------------------------------------------------

    /// Convert / Cancel (`_toggle_conversion`).
    fn toggle_conversion(&mut self, ctx: &egui::Context) {
        match &self.task {
            Some(Task { kind: TaskKind::Convert { .. }, runner }) => runner.cancel(),
            Some(_) => self.message(Kind::Error, "Error", "Please wait until preview creation completes"),
            None => self.start_conversion(ctx),
        }
    }

    /// `_start_conversion`: an applied custom command runs as it is.
    fn start_conversion(&mut self, ctx: &egui::Context) {
        let args = match &self.custom_command {
            Some(custom) => custom.clone(),
            None => match command::build(&self.s, &self.job()) {
                Ok(args) => args,
                Err(e) => return self.message(Kind::Error, "Error", e),
            },
        };
        let input = args.iter().position(|a| a == "-i").and_then(|i| args.get(i + 1)).cloned().unwrap_or_default();
        self.progress = Some(0.0);
        self.status_message = "Conversion in progress...".to_owned();
        self.ffmpeg_output = "Starting conversion...".to_owned();
        let runner = Runner::start(vec![args.clone()], ctx);
        self.task = Some(Task { kind: TaskKind::Convert { input, args }, runner });
    }

    /// Play 10s Preview / Cancel Preview (`_create_10s_preview`).
    fn toggle_preview(&mut self, ctx: &egui::Context) {
        match &self.task {
            Some(Task { kind: TaskKind::Preview { .. }, runner }) => return runner.cancel(),
            Some(_) => return self.message(Kind::Error, "Error", "Please wait until current process completes"),
            None => {}
        }
        if self.s.trim_streamcopy {
            return self.message(Kind::Error, "Error", "Preview is not available in Streamcopy mode");
        }
        if self.input_file.is_empty() {
            return self.message(Kind::Error, "Error", "Please select input file first");
        }
        let duration = match self.durations.state(&self.input_file) {
            Some(Duration::Known(d)) => Some(d),
            Some(Duration::Failed) => None,
            // Still probing: ask directly, as the original did
            _ => self.ffprobe.as_deref().and_then(|p| probe::duration(p, &self.input_file)),
        };
        let Some(duration) = duration.filter(|d| *d > 0.0) else {
            return self.message(Kind::Error, "Error", "Could not get video duration");
        };
        self.cleanup_preview_files();
        let temp = std::env::temp_dir();
        let clip = match command::preview_clip(self.custom_command.as_deref(), &self.s, &self.job(), duration, &temp) {
            Ok(clip) => clip,
            Err(e) => return self.message(Kind::Error, "Error", format!("Preview encoding failed: {e}")),
        };
        self.preview_files = vec![clip.clip, clip.encoded.clone()];
        self.status_message = "Creating 10-second preview...".to_owned();
        self.ffmpeg_output = "Starting preview encoding...".to_owned();
        self.progress = Some(0.0);
        let runner = Runner::start(vec![clip.copy, clip.encode], ctx);
        self.task = Some(Task { kind: TaskKind::Preview { encoded: clip.encoded }, runner });
    }

    fn poll_task(&mut self, ctx: &egui::Context) {
        let Some(task) = &self.task else { return };
        for event in task.runner.poll() {
            match event {
                Event::Line { step, text } => self.task_line(step, text, ctx),
                Event::Finished(outcome) => {
                    if let Some(task) = self.task.take() {
                        self.task_finished(task.kind, outcome);
                    }
                }
            }
        }
    }

    /// Shows an output line and the progress from its `time=` (`_update_progress`).
    fn task_line(&mut self, step: usize, text: String, ctx: &egui::Context) {
        let Some(task) = &self.task else { return };
        let total = match &task.kind {
            TaskKind::Convert { input, args } => {
                let duration = match self.durations.get(self.ffprobe.as_deref(), input, ctx) {
                    Some(Duration::Known(d)) => Some(d),
                    _ => None,
                };
                progress::expected_duration(args, duration)
            }
            // The preview's stream copy is not part of its progress
            TaskKind::Preview { .. } if step == 0 => return,
            TaskKind::Preview { .. } => Some(command::PREVIEW_SECONDS),
        };
        if let (Some(time), Some(total)) = (progress::time(&text), total) {
            self.progress = Some(progress::fraction(time, total));
        }
        self.ffmpeg_output = text;
    }

    fn task_finished(&mut self, kind: TaskKind, outcome: process::Outcome) {
        self.progress = None;
        self.ffmpeg_output.clear();
        match kind {
            TaskKind::Convert { .. } => self.conversion_finished(outcome),
            TaskKind::Preview { encoded } => self.preview_finished(outcome, &encoded),
        }
    }

    fn conversion_finished(&mut self, outcome: process::Outcome) {
        match outcome.exit {
            Exit::Success => {
                self.status_message = "Conversion complete!".to_owned();
                #[cfg(windows)]
                crate::win32::beep();
            }
            Exit::Cancelled => {
                self.status_message = "Conversion cancelled by user".to_owned();
                self.message(Kind::Info, "Cancelled", "Conversion was cancelled");
            }
            Exit::Failed(code) => {
                self.status_message = "Conversion error!".to_owned();
                self.message(Kind::Error, "Error", exit_message(code, &outcome.last_line));
            }
            Exit::SpawnFailed(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.status_message = "Error: ffmpeg.exe not found.".to_owned();
                self.message(Kind::Error, "Error", "ffmpeg.exe not found. Ensure it's in the program folder or system PATH.");
            }
            Exit::SpawnFailed(e) => {
                self.status_message = "An unexpected error occurred during conversion".to_owned();
                let text = format!("An unexpected error occurred during conversion:\n{e}");
                self.message(Kind::Error, "Unexpected Error", text);
            }
        }
    }

    fn preview_finished(&mut self, outcome: process::Outcome, encoded: &Path) {
        self.status_message = match (outcome.step, outcome.exit) {
            (_, Exit::Cancelled) => "Preview creation cancelled".to_owned(),
            (_, Exit::Success) => {
                if encoded.exists() {
                    self.open_file(encoded);
                }
                "Preview created successfully!".to_owned()
            }
            // The stream copy: the original showed this error and left the status alone
            (0, failure) => {
                let reason = match failure {
                    Exit::Failed(code) => exit_message(code, &outcome.last_line),
                    Exit::SpawnFailed(e) => e.to_string(),
                    _ => unreachable!(),
                };
                self.message(Kind::Error, "Error", format!("Streamcopy failed: {reason}"));
                "Preview creation failed!".to_owned()
            }
            (_, Exit::SpawnFailed(e)) => format!("Preview error: {e}"),
            (_, Exit::Failed(_)) => "Preview creation failed!".to_owned(),
        };
    }

    /// Play Input File / Play Output File (`_play_input_file`, `_play_output_file`).
    fn play(&mut self, output: bool) {
        let (path, unset, missing) = if output {
            (&self.output_file, "Output file is not specified", "Output file does not exist")
        } else {
            (&self.input_file, "Please select input file using Browse button", "Input file does not exist")
        };
        let path = PathBuf::from(path.trim());
        if path.as_os_str().is_empty() {
            self.message(Kind::Error, "Error", unset);
        } else if !path.exists() {
            self.message(Kind::Error, "Error", missing);
        } else {
            self.open_file(&path);
        }
    }

    /// Opens `path` in its default application (`os.startfile`).
    fn open_file(&mut self, path: &Path) {
        #[cfg(windows)]
        if let Err(e) = crate::win32::open_file(path) {
            self.message(Kind::Error, "Error", format!("Could not play file: {e}"));
        }
    }

    /// `_cleanup_preview_files`
    fn cleanup_preview_files(&mut self) {
        for file in self.preview_files.drain(..) {
            if file.exists()
                && let Err(e) = std::fs::remove_file(&file)
            {
                eprintln!("Error deleting temp file {}: {e}", file.display());
            }
        }
    }

    /// On exit: stops the running ffmpeg and any other ffmpeg / ffprobe we started.
    fn stop_processes(&mut self) {
        if let Some(task) = self.task.take() {
            task.runner.stop(STOP_TIMEOUT);
        }
        #[cfg(windows)]
        {
            let ffmpeg = Path::new(&self.s.ffmpeg_path).file_name().map(|n| n.to_string_lossy().into_owned());
            let mut names = vec!["ffmpeg.exe", "ffprobe.exe"];
            names.extend(ffmpeg.as_deref());
            crate::win32::process_tree::kill_children(&names);
        }
        self.cleanup_preview_files();
    }

    // -- Top rows ------------------------------------------------------------

    /// Label, entry and button of a path row; returns (entry, button) responses.
    fn path_row(
        ui: &mut Ui,
        text: &str,
        value: &mut String,
        id: &str,
        placeholder: &str,
        button_text: &str,
        kind: ButtonKind,
    ) -> (egui::Response, egui::Response) {
        let r = row(ui, grid::ROW_H);
        place_label(ui, cell(r, grid::LABEL_X, 200.0), text);
        let edit = place(ui, cell(r, grid::ENTRY_X, grid::ENTRY_W), |ui| {
            entry(
                ui,
                id,
                vec2(grid::ENTRY_W, dimensions::WIDGET_HEIGHT),
                value,
                EntryStyle { placeholder, ..Default::default() },
            )
        });
        let button = place(ui, cell(r, grid::BUTTON_X, dimensions::BUTTON_WIDTH), |ui| {
            button(ui, vec2(dimensions::BUTTON_WIDTH, dimensions::WIDGET_HEIGHT), button_text, kind)
        })
        .on_hover_text("Right-click to open containing folder");
        (edit, button)
    }

    fn path_rows(&mut self, ui: &mut Ui, frame: &eframe::Frame) {
        let (input, browse) = Self::path_row(ui, "Input File:", &mut self.input_file, "input",
            INPUT_PLACEHOLDER, "Browse", ButtonKind::Green);
        if !self.input_file.is_empty() {
            input.on_hover_ui(|ui| {
                let info = self.media_info.get(self.ffprobe.as_deref(), &self.input_file, ui.ctx());
                ui.set_max_width(600.0);
                ui.label(egui::RichText::new(info).font(FontId::monospace(14.0)));
            });
        }
        if browse.clicked() {
            self.browse_input(frame);
        } else if browse.secondary_clicked() {
            explore(&self.input_file);
        }
        let (_, save_as) = Self::path_row(ui, "Output File:", &mut self.output_file, "output",
            OUTPUT_PLACEHOLDER, "Save As", ButtonKind::Green);
        if save_as.clicked() {
            self.browse_output(frame);
        } else if save_as.secondary_clicked() {
            explore(&self.output_file);
        }
        let (edit, ffmpeg) = Self::path_row(ui, "FFmpeg Path:", &mut self.s.ffmpeg_path, "ffmpeg",
            FFMPEG_PLACEHOLDER, "FFmpeg", ButtonKind::Grey);
        if edit.lost_focus() {
            self.ffmpeg_path_edited();
        }
        if ffmpeg.clicked() {
            self.browse_ffmpeg(frame);
        } else if ffmpeg.secondary_clicked() {
            explore(&self.s.ffmpeg_path);
        }
    }

    fn quality_row(&mut self, ui: &mut Ui) {
        let r = row(ui, grid::ROW_H);
        let cqp = self.s.constant_qp_mode;
        let text = if cqp { "Quality Level:" } else { "Video Bitrate (k):" };
        place_label(ui, cell(r, grid::LABEL_X, 200.0), text);
        let value = if cqp { &mut self.s.quality_level } else { &mut self.s.bitrate };
        place(ui, cell(r, grid::ENTRY_X, 80.0), |ui| {
            entry(
                ui,
                "quality",
                vec2(80.0, dimensions::WIDGET_HEIGHT),
                value,
                EntryStyle { centered: true, ..Default::default() },
            )
        });
        place(ui, cell(r, grid::ENTRY_X + 90.0, 200.0), |ui| {
            checkbox(ui, &mut self.s.constant_qp_mode, "Constant QP mode")
        });
        let output = place(ui, cell(r, grid::BUTTON_X, dimensions::BUTTON_WIDTH), |ui| {
            button(ui, vec2(dimensions::BUTTON_WIDTH, dimensions::WIDGET_HEIGHT), "Output", ButtonKind::Grey)
                .on_hover_text("Right-click to copy command to clipboard")
        });
        if output.clicked() {
            self.open_command_window(ui.ctx());
        } else if output.secondary_clicked() {
            self.copy_command(ui.ctx());
        }
    }

    fn codec_row(&mut self, ui: &mut Ui) {
        let r = row(ui, grid::ROW_H);
        place_label(ui, cell(r, grid::LABEL_X, 200.0), "Video Codec:");
        for (i, (codec, text)) in [
            (VideoCodec::Hevc, "HEVC"),
            (VideoCodec::H264, "H.264"),
            (VideoCodec::Av1, "AV1"),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = cell(r, grid::ENTRY_X + 5.0 + 110.0 * i as f32, 100.0);
            if place(ui, rect, |ui| crate::ui::widgets::radio(ui, self.s.codec == codec, text)).clicked() {
                self.set_codec(codec);
            }
        }
        place(ui, cell(r, grid::BUTTON_X, dimensions::BUTTON_WIDTH), |ui| {
            button(ui, vec2(dimensions::BUTTON_WIDTH, dimensions::WIDGET_HEIGHT), "Help", ButtonKind::Grey)
        });
    }

    /// TextCheckbox header row; returns the row rect.
    fn section_header(ui: &mut Ui, expanded: &mut bool, title: &str) -> Rect {
        let r = row(ui, grid::ROW_H);
        place(ui, cell(r, grid::LABEL_X, 300.0), |ui| text_checkbox(ui, expanded, title));
        r
    }

    // -- Sections ------------------------------------------------------------

    fn encoder_section(&mut self, ui: &mut Ui) {
        let header = Self::section_header(ui, &mut self.expand_encoder_settings, "Advanced Encoder Settings");
        if !self.expand_encoder_settings {
            return;
        }
        let small = vec2(grid::SMALL_BUTTON_W, dimensions::WIDGET_HEIGHT);
        if place(ui, cell(header, grid::ENTRY_X, small.x), |ui| button(ui, small, "Auto", ButtonKind::Green)).clicked() {
            self.apply_auto_encoder_settings();
        }
        if place(ui, cell(header, grid::ENTRY_X + 90.0, small.x), |ui| {
            button(ui, small, "Default", ButtonKind::Green)
        })
        .clicked()
        {
            self.apply_builtin_preset("none");
        }

        ui.add_space(5.0);
        let f = section_frame(ui, grid::SECTION_X, grid::SECTION_W, 202.0);
        ui.add_space(5.0);

        let menu_w = dimensions::BUTTON_WIDTH;
        let row_rect = |i: usize, x: f32, w: f32| sub_rect(f, x, 7.0 + 32.0 * i as f32, w, dimensions::WIDGET_HEIGHT);
        let codec = self.s.codec;
        let levels = self.levels();
        let s = &mut self.s;

        // Left column
        let (tunes, profiles): (&[&str], &[&str]) = match codec {
            VideoCodec::Hevc => (TUNES, PROFILES_HEVC),
            VideoCodec::H264 => (TUNES_H264, PROFILES_H264),
            VideoCodec::Av1 => (TUNES, &[]),
        };
        let mut left: Vec<(&str, &str, &mut String, &[&str])> = vec![
            ("FF Threads:", "threads", &mut s.encoder_threads, THREADS),
            ("Preset:", "preset", &mut s.encoder_preset, PRESETS),
            ("Tune:", "tune", &mut s.encoder_tune, tunes),
        ];
        if codec != VideoCodec::Av1 {
            left.push(("Profile:", "profile", &mut s.encoder_profile, profiles));
        }
        left.push(("Level:", "level", &mut s.encoder_level, levels));
        if codec == VideoCodec::H264 {
            left.push(("Coder:", "coder", &mut s.encoder_coder, CODERS));
        } else {
            left.push(("Tier:", "tier", &mut s.encoder_tier, TIERS));
        }
        let mut tune_changed = false;
        for (i, (text, id, value, values)) in left.into_iter().enumerate() {
            place_label(ui, row_rect(i, 10.0, 75.0), text);
            let r = place(ui, row_rect(i, 87.0, menu_w), |ui| option_menu(ui, id, menu_w, value, values, true));
            tune_changed |= id == "tune" && r.changed();
        }

        // Right column; H.264 has neither lookahead level nor split encode
        let rc_enabled = !s.constant_qp_mode;
        let mut right: Vec<(&str, &str, &mut String, &[&str], bool)> = vec![
            ("HW Accel:", "hwaccel", &mut s.encoder_hwaccel, HWACCELS, true),
            ("Multipass:", "multipass", &mut s.encoder_multipass, MULTIPASS, true),
            ("Rate-Control:", "rc", &mut s.encoder_rc, RATE_CONTROLS, rc_enabled),
        ];
        if codec != VideoCodec::H264 {
            right.push(("Lookahead Level:", "lookahead", &mut s.encoder_lookahead_level, LOOKAHEAD, true));
            right.push(("Split Encode:", "split", &mut s.encoder_split_encode_mode, SPLIT_ENCODE, true));
        }
        for (i, (text, id, value, values, enabled)) in right.into_iter().enumerate() {
            place_label(ui, row_rect(i, 247.0, 105.0), text);
            place(ui, row_rect(i, 356.0, menu_w), |ui| option_menu(ui, id, menu_w, value, values, enabled));
        }

        let cuda = s.encoder_hwaccel == "cuda";
        let checks: [(&mut bool, &str, bool); 6] = [
            (&mut s.cuda_output_format, "CUDA Output Format", cuda),
            (&mut s.encoder_spatial_aq, "Spatial AQ", true),
            (&mut s.encoder_temporal_aq, "Temporal AQ", true),
            (&mut s.encoder_strict_gop, "Strict GOP", true),
            (&mut s.encoder_no_scenecut, "No-Scenecut", true),
            (&mut s.encoder_weighted_pred, "Weighted Prediction", true),
        ];
        for (i, (value, text, enabled)) in checks.into_iter().enumerate() {
            place(ui, row_rect(i, 516.0, 210.0), |ui| checkbox_enabled(ui, value, text, enabled));
        }
        if tune_changed {
            self.tune_changed();
        }
    }

    fn fps_section(&mut self, ui: &mut Ui) {
        Self::section_header(ui, &mut self.expand_fps_scale_settings, "FPS and Scaling Settings");
        if !self.expand_fps_scale_settings {
            return;
        }
        ui.add_space(10.0);
        let f = section_frame(ui, grid::SECTION_X, grid::SECTION_W, 240.0);
        ui.add_space(10.0);
        let s = &mut self.s;

        const COLS: [f32; 5] = [124.0, 274.0, 380.0, 484.0, 588.0];
        let line = |y: f32, h: f32| sub_rect(f, 0.0, y, f.width(), h);
        let radios = |ui: &mut Ui, r: Rect, current: &mut String, options: &[(&str, &str)]| {
            for (x, (text, value)) in COLS.iter().zip(options) {
                place(ui, cell(r, *x, 100.0), |ui| radio_value(ui, current, value, text));
            }
        };

        let r = line(0.0, 38.0);
        place_label(ui, cell(r, 10.0, 100.0), "FPS:");
        radios(ui, r, &mut s.fps_option, &[
            ("Source", "source"),
            ("60", "60"),
            ("50", "50"),
            ("30", "30"),
            ("23.976", "24000/1001"),
        ]);

        let r = line(38.0, 42.0);
        let disabled = s.fps_option != "custom";
        place(ui, cell(r, 20.0, 80.0), |ui| {
            entry(ui, "custom_fps", vec2(80.0, dimensions::WIDGET_HEIGHT), &mut s.custom_fps,
                EntryStyle { centered: true, disabled, ..Default::default() })
        });
        radios(ui, r, &mut s.fps_option, &[("Custom", "custom")]);

        let r = line(80.0, 38.0);
        place_label(ui, cell(r, 10.0, 110.0), "FPS Mode:");
        radios(ui, r, &mut s.fps_mode, &[
            ("Auto", "auto"),
            ("Constant", "cfr"),
            ("Variable", "vfr"),
            ("Passthrough", "passthrough"),
        ]);

        let r = line(118.0, 32.0);
        place_label(ui, cell(r, 10.0, 110.0), "Video Format:");
        radios(ui, r, &mut s.video_format_option, &[
            ("Source", "source"),
            ("HD", "1280"),
            ("FHD", "1920"),
            ("QHD", "2560"),
            ("4K", "3840"),
        ]);

        let r = line(150.0, 42.0);
        let disabled = s.video_format_option != "custom";
        place(ui, cell(r, 20.0, 80.0), |ui| {
            entry(ui, "custom_width", vec2(80.0, dimensions::WIDGET_HEIGHT), &mut s.custom_video_width,
                EntryStyle { centered: true, disabled, ..Default::default() })
        });
        radios(ui, r, &mut s.video_format_option, &[("Custom", "custom")]);

        let r = line(192.0, 48.0);
        place_label(ui, cell(r, 10.0, 115.0), "Interpolation Algo:");
        place(ui, cell(r, 127.0, dimensions::BUTTON_WIDTH), |ui| {
            option_menu(ui, "interp", dimensions::BUTTON_WIDTH, &mut s.interpolation_algo, INTERPOLATIONS, true)
        });
        let description = interpolation_description(&s.interpolation_algo);
        place(ui, cell(r, 278.0, 100.0), |ui| {
            ui.add(egui::Label::new(egui::RichText::new(description).color(colors::PLACEHOLDER)).wrap())
        });
    }

    fn audio_section(&mut self, ui: &mut Ui) {
        Self::section_header(ui, &mut self.expand_audio_settings, "Audio Settings");
        if !self.expand_audio_settings {
            return;
        }
        ui.add_space(10.0);
        let f = section_frame(ui, grid::SECTION_X, grid::SECTION_W, 94.0);
        ui.add_space(10.0);
        let s = &mut self.s;

        const COLS: [f32; 5] = [25.0, 155.0, 285.0, 415.0, 545.0];
        let first = sub_rect(f, 0.0, 5.0, f.width(), 42.0);
        for (x, (text, value)) in COLS.iter().zip([
            ("Disable", "disable"),
            ("Source", "copy"),
            ("AAC 96", "aac_96k"),
            ("AAC 160", "aac_160k"),
            ("AAC 256", "aac_256k"),
        ]) {
            place(ui, cell(first, *x, 100.0), |ui| radio_value(ui, &mut s.audio_option, value, text));
        }

        let second = sub_rect(f, 0.0, 47.0, f.width(), 42.0);
        let disabled = s.audio_option != "custom";
        place(ui, cell(second, 30.0, 80.0), |ui| {
            entry(ui, "custom_abitrate", vec2(80.0, dimensions::WIDGET_HEIGHT), &mut s.custom_abitrate,
                EntryStyle { centered: true, disabled, ..Default::default() })
        });
        for (x, (text, value)) in COLS[1..].iter().zip([
            ("Custom", "custom"),
            ("Opus 96", "opus_96k"),
            ("Opus 160", "opus_160k"),
            ("Opus 256", "opus_256k"),
        ]) {
            place(ui, cell(second, *x, 100.0), |ui| radio_value(ui, &mut s.audio_option, value, text));
        }
    }

    fn additional_section(&mut self, ui: &mut Ui) {
        Self::section_header(ui, &mut self.expand_additional_options, "Additional Options (Trimming)");
        if !self.expand_additional_options {
            return;
        }
        ui.add_space(10.0);
        let f = section_frame(ui, grid::SECTION_X, grid::SECTION_W, 346.0);
        ui.add_space(10.0);
        let h = dimensions::WIDGET_HEIGHT;

        // Trim slider + reset: handles sit at the trim times relative to the duration
        let duration = self.duration();
        let frac = |t: &str, d: f64| (timecode::to_seconds(t) as f64 / d) as f32;
        let (start0, end0) = match duration {
            Some(d) => (frac(&self.s.trim_start, d), frac(&self.s.trim_end, d)),
            None => (0.0, 1.0),
        };
        let (mut start, mut end) = (start0, end0);
        let slider = place(ui, sub_rect(f, 10.0, 10.0, 672.0, 30.0), |ui| {
            trim_slider(ui, vec2(672.0, 30.0), &mut start, &mut end)
        });
        if slider.changed()
            && let Some(d) = duration
        {
            if start != start0 {
                self.trim_handle_moved(true, start, d);
            } else if end != end0 {
                self.trim_handle_moved(false, end, d);
            }
        }
        if place(ui, sub_rect(f, 692.0, 10.0, 30.0, 30.0), |ui| reset_button(ui, vec2(30.0, 30.0))).clicked() {
            self.reset_trim();
        }

        // Trimming row
        let r = sub_rect(f, 0.0, 40.0, f.width(), 48.0);
        place_label(ui, cell(r, 10.0, 80.0), "Trimming:");
        place_label(ui, cell(r, 160.0, 36.0), "From:");
        let from = place(ui, cell(r, 198.0, 65.0), |ui| {
            entry(ui, "trim_start", vec2(65.0, h), &mut self.s.trim_start, EntryStyle { centered: true, ..Default::default() })
        });
        place_label(ui, cell(r, 274.0, 22.0), "To:");
        let to = place(ui, cell(r, 297.0, 65.0), |ui| {
            entry(ui, "trim_end", vec2(65.0, h), &mut self.s.trim_end, EntryStyle { centered: true, ..Default::default() })
        });
        if from.lost_focus() {
            self.validate_trim(true);
        }
        if to.lost_focus() {
            self.validate_trim(false);
        }
        if place(ui, cell(r, 372.0, 80.0), |ui| button(ui, vec2(80.0, h), "Trim", ButtonKind::Green)).clicked() {
            self.add_trim_options();
        }
        let s = &mut self.s;
        place(ui, cell(r, 462.0, 70.0), |ui| {
            if checkbox(ui, &mut s.trim_streamcopy, "Copy").clicked() && s.trim_streamcopy {
                s.audio_option = "copy".to_owned();
            }
        });
        place(ui, cell(r, 532.0, 100.0), |ui| checkbox(ui, &mut s.precise_trim, "Precise"));

        // Option / filter entries
        let entry_w = 537.0;
        let fields: [(&str, &str, &mut String, &str, bool); 3] = [
            ("Add FF Options:", "ff_options", &mut s.additional_options, FF_OPTIONS_PLACEHOLDER, true),
            ("Add Video Filters (-vf):", "vf", &mut s.additional_filter_options, VF_PLACEHOLDER, true),
            ("Add Audio Filters (-af):", "af", &mut s.additional_audio_filter_options, AF_PLACEHOLDER, false),
        ];
        for (i, (text, id, value, placeholder, help)) in fields.into_iter().enumerate() {
            let r = sub_rect(f, 0.0, 88.0 + 48.0 * i as f32, f.width(), 48.0);
            place_label(ui, cell(r, 10.0, 140.0), text);
            place(ui, cell(r, 152.0, entry_w), |ui| {
                entry(ui, id, vec2(entry_w, h), value, EntryStyle { placeholder, ..Default::default() })
            });
            if help {
                place(ui, cell(r, 696.0, 30.0), |ui| button(ui, vec2(30.0, h), "?", ButtonKind::Grey));
            }
        }

        // Quick filter buttons
        for (ri, buttons) in QUICK_BUTTONS.iter().enumerate() {
            for (bi, (text, quick)) in buttons.iter().enumerate() {
                let kind = match quick {
                    Quick::Clear => ButtonKind::Red,
                    Quick::Streams => ButtonKind::Green,
                    _ => ButtonKind::Grey,
                };
                let rect = sub_rect(f, 10.0 + 116.0 * bi as f32, 232.0 + 38.0 * ri as f32, 106.0, h);
                if place(ui, rect, |ui| button(ui, rect.size(), text, kind)).clicked() {
                    filters::apply(&mut self.s, quick);
                }
            }
        }
    }

    fn presets_section(&mut self, ui: &mut Ui) {
        Self::section_header(ui, &mut self.expand_presets, "Presets");
        if !self.expand_presets {
            return;
        }
        ui.add_space(10.0);
        let f = section_frame(ui, grid::SECTION_X, grid::SECTION_W, 108.0);
        ui.add_space(10.0);
        let h = dimensions::WIDGET_HEIGHT;

        let r = sub_rect(f, 0.0, 0.0, f.width(), 32.0);
        for (x, (text, value)) in [10.0, 145.0, 265.0, 385.0, 505.0].into_iter().zip([
            ("Default (Reset)", "none"),
            ("FHD Fast", "fhdf"),
            ("FHD Quality", "fhdq"),
            ("HD Fast", "hdf"),
            ("HD Quality", "hdq"),
        ]) {
            if place(ui, cell(r, x, 130.0), |ui| radio_value(ui, &mut self.s.selected_preset, value, text)).clicked() {
                self.apply_builtin_preset(value);
            }
        }

        let r = sub_rect(f, 0.0, 32.0, f.width(), 38.0);
        if place(ui, cell(r, 10.0, 100.0), |ui| radio_value(ui, &mut self.s.selected_preset, "custom", "Custom"))
            .clicked()
            && !self.s.custom_preset_selected.is_empty()
        {
            let name = self.s.custom_preset_selected.clone();
            self.load_custom_preset(&name);
        }
        place_label(ui, cell(r, 145.0, 50.0), "Preset:");
        let names: Vec<&str> = self.custom_presets.iter().map(String::as_str).collect();
        let mut picked = self.s.custom_preset_selected.clone();
        let menu = place(ui, cell(r, 196.0, 170.0), |ui| {
            option_menu(ui, "custom_preset", 170.0, &mut picked, &names, true)
        });
        if menu.changed() {
            self.load_custom_preset(&picked);
        }
        if place(ui, cell(r, 385.0, 70.0), |ui| button(ui, vec2(70.0, h), "Save As", ButtonKind::Green)).clicked() {
            self.save_preset_as();
        }
        if place(ui, cell(r, 465.0, 60.0), |ui| button(ui, vec2(60.0, h), "Save", ButtonKind::Green)).clicked() {
            self.save_preset();
        }
        if place(ui, cell(r, 535.0, 60.0), |ui| button(ui, vec2(60.0, h), "Delete", ButtonKind::Red)).clicked() {
            self.delete_preset();
        }

        let r = sub_rect(f, 0.0, 70.0, f.width(), 38.0);
        let color = if self.preset_indicator_active { colors::ACCENT_GREEN } else { colors::PLACEHOLDER };
        place(ui, cell(r, 10.0, 600.0), |ui| colored_label(ui, &self.preset_indicator, color));
    }

    /// "Estimated size: ..." from the bitrate and the probed duration.
    fn estimated_size(&self) -> String {
        if self.s.constant_qp_mode {
            return "Estimated size: Not available for CQP".to_owned();
        }
        let Ok(video) = self.s.bitrate.trim().parse::<i64>() else { return String::new() };
        let audio = estimate::audio_kbps(&self.s.audio_option, &self.s.custom_abitrate);
        match self.durations.state(&self.input_file) {
            Some(Duration::Known(d)) => {
                format!("Estimated size: {:.2} MB", estimate::size_mb(video as f64, audio, d))
            }
            Some(Duration::Failed) => "Estimated size: Could not get duration".to_owned(),
            _ => String::new(),
        }
    }

    fn status_rows(&self, ui: &mut Ui) {
        let r = row(ui, grid::ROW_H);
        place_label(ui, cell(r, 5.0, 300.0), &self.estimated_size());
        ui.painter().text(
            egui::pos2(r.min.x + grid::STATUS_CENTER_X, r.center().y),
            Align2::CENTER_CENTER,
            &self.status_message,
            egui::FontId::proportional(dimensions::FONT_SIZE),
            colors::LABEL_TEXT,
        );

        if let Some(progress) = self.progress {
            ui.add_space(10.0);
            let r = row(ui, 8.0);
            place(ui, cell(r, grid::SECTION_X, grid::SECTION_W), |ui| progress_bar(ui, grid::SECTION_W, progress));
            let r = row(ui, dimensions::WIDGET_HEIGHT);
            ui.painter().text(
                egui::pos2(r.min.x + grid::SECTION_X + grid::SECTION_W / 2.0, r.center().y),
                Align2::CENTER_CENTER,
                if progress == 0.0 { "0%".to_owned() } else { format!("{:.1}%", progress * 100.0) },
                egui::FontId::proportional(dimensions::FONT_SIZE),
                colors::LABEL_TEXT,
            );
        }

        if !self.ffmpeg_output.is_empty() {
            ui.add_space(10.0);
            let galley = ui.painter().layout(
                self.ffmpeg_output.clone(),
                egui::FontId::proportional(dimensions::FONT_SIZE),
                colors::LABEL_TEXT,
                600.0,
            );
            let r = row(ui, galley.size().y.max(dimensions::WIDGET_HEIGHT));
            let pos = egui::pos2(
                r.min.x + grid::STATUS_CENTER_X - galley.size().x / 2.0,
                r.center().y - galley.size().y / 2.0,
            );
            ui.painter().galley(pos, galley, colors::LABEL_TEXT);
        }
    }

    // -- Bottom bar ----------------------------------------------------------

    fn bottom_bar(&mut self, ui: &mut Ui) {
        let h = dimensions::BIG_BUTTON_HEIGHT;
        let gap = 4.0;
        let r = row(ui, h);
        let w = (r.width() - gap * 4.0) / 5.0;
        // The preview stays cancellable even if Copy mode is switched on meanwhile
        let preview = match &self.task {
            Some(Task { kind: TaskKind::Preview { .. }, .. }) => ("Cancel Preview", ButtonKind::Red),
            _ if self.s.trim_streamcopy => ("Play 10s Preview", ButtonKind::Disabled),
            _ => ("Play 10s Preview", ButtonKind::Grey),
        };
        let buttons = [
            ("Play Input File", ButtonKind::Grey),
            ("Play Output File", ButtonKind::Grey),
            preview,
            ("Batch Convert", ButtonKind::Grey),
            ("Screen Record", ButtonKind::Grey),
        ];
        let mut clicked = None;
        for (i, (text, kind)) in buttons.into_iter().enumerate() {
            let rect = cell(r, (w + gap) * i as f32, w);
            if place(ui, rect, |ui| button(ui, rect.size(), text, kind)).clicked() {
                clicked = Some(i);
            }
        }
        let ctx = ui.ctx().clone();
        match clicked {
            Some(0) => self.play(false),
            Some(1) => self.play(true),
            Some(2) => self.toggle_preview(&ctx),
            _ => {}
        }

        ui.add_space(10.0);
        let r = row(ui, h);
        let (text, kind) = match &self.task {
            Some(Task { kind: TaskKind::Convert { .. }, .. }) => ("Cancel", ButtonKind::Red),
            _ => ("Convert", ButtonKind::Green),
        };
        if place(ui, r, |ui| bold_button(ui, r.size(), text, kind)).clicked() {
            self.toggle_conversion(&ctx);
        }
    }

    /// Per-frame bookkeeping: ffmpeg output, finished probes and state that follows
    /// other settings.
    fn update_state(&mut self, ctx: &egui::Context) {
        self.poll_task(ctx);
        self.durations.poll();
        // Probe the current input (size estimate, trim slider)
        self.durations.get(self.ffprobe.as_deref(), &self.input_file, ctx);
        if let Some(file) = &self.trim_end_follows {
            match self.durations.get(self.ffprobe.as_deref(), file, ctx) {
                Some(Duration::Pending) => {}
                Some(Duration::Known(d)) if *file == self.input_file && d > 0.0 => {
                    self.s.trim_end = timecode::format(d);
                    self.trim_end_follows = None;
                }
                _ => self.trim_end_follows = None,
            }
        }
        // `_update_cuda_output_format_state`
        if self.s.encoder_hwaccel != "cuda" {
            self.s.cuda_output_format = false;
        }
        self.autosave(ctx);
    }

    /// Restarts the save delay on every settings change and saves once it runs out.
    fn autosave(&mut self, ctx: &egui::Context) {
        if self.settings_file.is_none() {
            return;
        }
        let now = Instant::now();
        if self.s != self.observed_settings {
            self.observed_settings = self.s.clone();
            self.save_due = Some(now + SAVE_DELAY);
        }
        match self.save_due {
            Some(due) if now >= due => self.save_settings(),
            Some(due) => ctx.request_repaint_after(due - now),
            None => {}
        }
    }

    /// `_save_settings`
    fn save_settings(&mut self) {
        self.save_due = None;
        if let Some(file) = &self.settings_file
            && let Err(e) = self.s.save(file)
        {
            eprintln!("Error saving settings: {e}");
        }
    }
}

impl eframe::App for NvencFfxApp {
    fn on_exit(&mut self) {
        self.stop_processes();
        self.save_settings();
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_drag_and_drop(&ctx);
        self.update_state(&ctx);

        egui::Panel::bottom("action_bar")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(colors::PRIMARY_BG).inner_margin(Margin {
                left: grid::BOTTOM_PAD_X,
                right: grid::BOTTOM_PAD_X,
                top: 0,
                bottom: grid::BOTTOM_PAD_Y,
            }))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                self.bottom_bar(ui);
            });

        let panel_frame = egui::Frame::new()
            .fill(colors::PRIMARY_BG)
            .inner_margin(Margin::same(grid::OUTER_PAD));

        egui::CentralPanel::default_margins().frame(panel_frame).show(ui, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(Margin {
                            left: grid::CONTENT_PAD,
                            right: 0,
                            top: grid::CONTENT_PAD,
                            bottom: 15,
                        })
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;

                            self.path_rows(ui, frame);
                            self.quality_row(ui);
                            self.codec_row(ui);

                            self.encoder_section(ui);
                            self.fps_section(ui);
                            self.audio_section(ui);
                            self.additional_section(ui);
                            self.presets_section(ui);

                            self.status_rows(ui);
                        });
                });
        });

        file_drag_drop_overlay(&ctx);
        self.dialog_ui(ui, ViewportId::ROOT);

        if self.command_window.is_some() {
            let builder = ViewportBuilder::default()
                .with_title(COMMAND_WINDOW_TITLE)
                .with_inner_size([850.0, 550.0])
                .with_decorations(false)
                .with_resizable(false)
                .with_icon(self.icon.clone());
            ctx.show_viewport_immediate(command_viewport(), builder, |ui, _| self.command_window_ui(ui));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sections_render_for_every_codec() {
        let ctx = egui::Context::default();
        apply_theme(&ctx);
        let mut app = NvencFfxApp {
            expand_encoder_settings: true,
            expand_fps_scale_settings: true,
            expand_audio_settings: true,
            expand_additional_options: true,
            expand_presets: true,
            progress: Some(0.4),
            ffmpeg_output: "Command copied to clipboard!".into(),
            ..Default::default()
        };
        for codec in [VideoCodec::H264, VideoCodec::Av1, VideoCodec::Hevc] {
            app.set_codec(codec);
            let mut out = ctx.run_ui(Default::default(), |ui| {
                app.update_state(ui.ctx());
                app.quality_row(ui);
                app.codec_row(ui);
                app.encoder_section(ui);
                app.fps_section(ui);
                app.audio_section(ui);
                app.additional_section(ui);
                app.presets_section(ui);
                app.status_rows(ui);
            });
            out.textures_delta.clear();
        }
        assert_eq!(app.s.encoder_profile, "main");
    }

    /// Clicks at `pos` (primary or secondary button) on the top rows + additional
    /// section laid out as in the window (zero item spacing, origin at 0,0).
    fn click(app: &mut NvencFfxApp, pos: egui::Pos2, secondary: bool) {
        let ctx = egui::Context::default();
        apply_theme(&ctx);
        let button = if secondary { egui::PointerButton::Secondary } else { egui::PointerButton::Primary };
        let events = [
            vec![egui::Event::PointerMoved(pos)],
            vec![egui::Event::PointerButton { pos, button, pressed: true, modifiers: Default::default() }],
            vec![egui::Event::PointerButton { pos, button, pressed: false, modifiers: Default::default() }],
        ];
        for events in events {
            let input = egui::RawInput { events, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                app.quality_row(ui);
                app.codec_row(ui);
                app.additional_section(ui);
            });
            out.textures_delta.clear();
        }
    }

    #[test]
    fn buttons_are_wired() {
        let mut app = NvencFfxApp {
            input_file: r"C:\v\clip.mkv".into(),
            output_file: r"C:\v\clip_hevc_custom.mkv".into(),
            expand_additional_options: true,
            ..Default::default()
        };
        app.s.ffmpeg_path = "ffmpeg.exe".into();
        let y = |row: f32| row * grid::ROW_H + grid::ROW_H / 2.0;

        // AV1 radio (third codec, circle center) renames the auto-named output
        click(&mut app, egui::pos2(grid::ENTRY_X + 5.0 + 220.0 + 11.0, y(1.0)), false);
        assert_eq!(app.s.codec, VideoCodec::Av1);
        assert!(app.output_file.ends_with("clip_av1_custom.mkv"), "{}", app.output_file);

        // Output opens the editable command preview
        let output = egui::pos2(grid::BUTTON_X + 70.0, y(0.0));
        click(&mut app, output, false);
        let text = &app.command_window.as_ref().expect("command window open").text;
        assert!(text.contains("av1_nvenc"), "{text}");

        // Additional section starts after the header row; Trim button and H-Flip
        let section_y = 3.0 * grid::ROW_H + 10.0;
        app.s.trim_end = "00:00:30".into();
        click(&mut app, egui::pos2(grid::SECTION_X + 372.0 + 40.0, section_y + 40.0 + 24.0), false);
        assert_eq!(app.s.additional_options, "-ss 00:00:00 -to 00:00:30");
        click(&mut app, egui::pos2(grid::SECTION_X + 10.0 + 232.0 + 50.0, section_y + 232.0 + 14.0), false);
        assert_eq!(app.s.additional_filter_options, "hflip");
    }

    #[test]
    fn auto_and_codec_switch_keep_values_valid() {
        let mut app = NvencFfxApp::default();
        app.s.encoder_tune = "uhq".into();
        app.s.encoder_level = "6.2".into();
        app.set_codec(VideoCodec::H264);
        assert_eq!(app.s.encoder_tune, "hq");
        assert_eq!(app.s.encoder_level, "auto");
        app.apply_auto_encoder_settings();
        assert_eq!(app.s.encoder_profile, "auto");
        assert_eq!(app.s.encoder_rc, "vbr", "rate control has no auto");
        assert!(!app.s.encoder_spatial_aq);
    }

    #[test]
    fn trim_button_and_handles() {
        let mut app = NvencFfxApp::default();
        app.s.trim_start = "00:00:05".into();
        app.s.trim_end = "00:00:20".into();
        app.s.additional_options = "-ac 2 -ss 00:00:01 -to 00:00:02".into();
        app.add_trim_options();
        assert_eq!(app.s.additional_options, "-ac 2 -ss 00:00:05 -to 00:00:20");

        app.trim_handle_moved(true, 0.9, 100.0);
        assert_eq!(app.s.trim_start, "00:00:19", "start stays a second before the end");
        app.trim_handle_moved(false, 1.0, 100.0);
        assert_eq!(app.s.trim_end, "00:01:39");
    }

    #[test]
    fn output_names_follow_the_original() {
        let input = Path::new(r"C:\v\clip.MKV");
        assert_eq!(default_output(input, VideoCodec::H264, ""), Path::new(r"C:\v\clip_h264_custom.MKV"));
        assert_eq!(
            default_output(Path::new(r"C:\v\clip.avi"), VideoCodec::Av1, r"Z:\missing"),
            Path::new(r"C:\v\clip_av1_custom.mp4")
        );

        let out = renamed_output(r"C:\v\clip_hevc_custom.mkv", r"C:\v\clip.mkv", VideoCodec::Av1).unwrap();
        assert!(out.ends_with("clip_av1_custom.mkv"));
        assert_eq!(renamed_output(r"C:\v\mine.mkv", r"C:\v\clip.mkv", VideoCodec::Av1), None);
        assert_eq!(renamed_output(r"C:\v\a_hevc_custom.mp4", "", VideoCodec::Av1), None);
    }

    #[test]
    fn settings_are_saved_after_the_delay() {
        let file = std::env::temp_dir().join(format!("nvencffx-autosave-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let ctx = egui::Context::default();
        let mut app = NvencFfxApp { settings_file: Some(file.clone()), ..Default::default() };

        app.s.bitrate = "9000".into();
        app.autosave(&ctx);
        assert!(app.save_due.is_some() && !file.exists(), "a change waits for the delay");
        app.s.bitrate = "9500".into();
        app.save_due = Some(Instant::now());
        app.autosave(&ctx);
        assert!(app.save_due.is_some_and(|due| due > Instant::now()), "a new change restarts it");

        app.save_due = Some(Instant::now());
        app.autosave(&ctx);
        assert!(app.save_due.is_none());
        let mut saved = Settings::default();
        saved.load(&file).unwrap();
        assert_eq!(saved.bitrate, "9500");
        let _ = std::fs::remove_file(&file);
    }

    /// Polls until the running task is done.
    fn finish_task(app: &mut NvencFfxApp, ctx: &egui::Context) {
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        while app.task.is_some() && Instant::now() < deadline {
            app.poll_task(ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(app.task.is_none(), "task finished");
    }

    #[cfg(windows)]
    #[test]
    fn conversion_reports_progress_and_errors() {
        let ctx = egui::Context::default();
        let mut app = NvencFfxApp::default();
        // A stand-in for ffmpeg: prints a status line, then fails
        let fake = "cmd /c echo frame=1 time=00:00:05.00 -t 10 & exit 2";
        app.custom_command = Some(fake.split(' ').map(str::to_owned).collect());
        app.toggle_conversion(&ctx);
        assert_eq!(app.status_message, "Conversion in progress...");
        app.task_line(0, "frame=1 time=00:00:05.00".into(), &ctx);
        assert_eq!(app.progress, Some(0.5), "-t 10 is the expected length");

        finish_task(&mut app, &ctx);
        assert_eq!(app.status_message, "Conversion error!");
        assert_eq!(app.progress, None);
        let (_, dialog) = app.dialog.as_ref().expect("error dialog");
        assert_eq!(dialog.message, "FFmpeg exited with error code 2.\nLast output: frame=1 time=00:00:05.00 -t 10");
    }

    #[cfg(windows)]
    #[test]
    fn cancel_and_missing_ffmpeg() {
        let ctx = egui::Context::default();
        let ping = "ping -n 30 127.0.0.1".split(' ').map(str::to_owned).collect();
        let mut app = NvencFfxApp { custom_command: Some(ping), ..Default::default() };
        app.toggle_conversion(&ctx);
        app.toggle_conversion(&ctx);
        finish_task(&mut app, &ctx);
        assert_eq!(app.status_message, "Conversion cancelled by user");

        app.custom_command = Some(vec![r"Z:\missing\ffmpeg.exe".into(), "-i".into(), "a".into(), "b".into()]);
        app.toggle_conversion(&ctx);
        finish_task(&mut app, &ctx);
        assert_eq!(app.status_message, "Error: ffmpeg.exe not found.");
    }

    #[test]
    fn preview_and_play_check_their_inputs() {
        let ctx = egui::Context::default();
        let mut app = NvencFfxApp { s: Settings { trim_streamcopy: true, ..Default::default() }, ..Default::default() };
        app.toggle_preview(&ctx);
        assert_eq!(app.dialog.take().unwrap().1.message, "Preview is not available in Streamcopy mode");
        app.s.trim_streamcopy = false;
        app.toggle_preview(&ctx);
        assert_eq!(app.dialog.take().unwrap().1.message, "Please select input file first");
        app.input_file = r"Z:\missing.mp4".into();
        app.toggle_preview(&ctx);
        assert_eq!(app.dialog.take().unwrap().1.message, "Could not get video duration");
        assert!(app.task.is_none());

        app.play(false);
        assert_eq!(app.dialog.take().unwrap().1.message, "Input file does not exist");
        app.play(true);
        assert_eq!(app.dialog.take().unwrap().1.message, "Output file is not specified");
    }

    #[test]
    fn startup_drops_a_missing_custom_preset() {
        let mut app = NvencFfxApp { presets_dir: std::env::temp_dir().join("nvencffx-no-presets"), ..Default::default() };
        app.s.selected_preset = "custom".into();
        app.s.custom_preset_selected = "gone".into();
        app.restore_preset_selection();
        assert_eq!(app.s.selected_preset, "none");
        assert!(app.s.custom_preset_selected.is_empty());
        assert_eq!(app.preset_indicator, "No preset selected");

        app.s.selected_preset = "hdq".into();
        app.restore_preset_selection();
        assert_eq!(app.preset_indicator, "HD Quality preset applied");
    }
}
