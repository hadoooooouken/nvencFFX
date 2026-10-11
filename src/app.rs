use eframe::egui::{self, Align2, Margin, Rect, ScrollArea, Ui, Vec2, vec2};
use crate::ui::{
    theme::{apply_theme, colors, dimensions},
    widgets::{
        ButtonKind, EntryStyle, bold_button, button, cell, checkbox, colored_label, entry,
        file_drag_drop_overlay, handle_dropped_files, label, option_menu, place, radio_value,
        reset_button, row, section_frame, sub_rect, text_checkbox, trim_slider,
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

    // Window chrome
    pub const OUTER_PAD: f32 = 10.0; // CTkScrollableFrame padx/pady
    pub const CONTENT_PAD: f32 = 21.0; // scrollable frame inset + main_frame padx
    pub const BOTTOM_PAD_X: f32 = 15.0;
    pub const BOTTOM_PAD_Y: f32 = 20.0;
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

const INPUT_PLACEHOLDER: &str = "Drag and drop a video file here or use the 'Browse' button.";
const OUTPUT_PLACEHOLDER: &str = "Also used as the destination file for screen recordings";
const FFMPEG_PLACEHOLDER: &str = "Path to ffmpeg.exe (required)";
const FF_OPTIONS_PLACEHOLDER: &str =
    "e.g. -aq-strength 8; -cq 0; -bf 0 or -bf 4; -intra-refresh 1; -forced-idr 1";
const VF_PLACEHOLDER: &str = "e.g.setpts=0.5*PTS - speed up video x2; crop=iw:min(ih\\,iw*9/16)";
const AF_PLACEHOLDER: &str = "e.g. atempo=2.0, volume=1.5";

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum VideoCodec {
    Hevc,
    H264,
    Av1,
}

impl VideoCodec {
    fn suffix(self) -> &'static str {
        match self {
            Self::Hevc => "hevc",
            Self::H264 => "h264",
            Self::Av1 => "av1",
        }
    }
}

pub struct NvencFfxApp {
    // File paths
    pub input_file: String,
    pub output_file: String,
    pub ffmpeg_path: String,

    // Codec & quality
    pub codec: VideoCodec,
    pub constant_qp_mode: bool,
    pub quality_level: String,
    pub bitrate: String,

    // Collapsible sections (TextCheckbox)
    pub expand_encoder_settings: bool,
    pub expand_fps_scale_settings: bool,
    pub expand_audio_settings: bool,
    pub expand_additional_options: bool,
    pub expand_presets: bool,

    // Advanced encoder settings
    pub threads: String,
    pub preset: String,
    pub tune: String,
    pub profile: String,
    pub level: String,
    pub tier: String,
    pub coder: String,
    pub hwaccel: String,
    pub multipass: String,
    pub rate_control: String,
    pub lookahead_level: String,
    pub split_encode_mode: String,
    pub cuda_output_format: bool,
    pub spatial_aq: bool,
    pub temporal_aq: bool,
    pub strict_gop: bool,
    pub no_scenecut: bool,
    pub weighted_pred: bool,

    // FPS & scaling
    pub fps_option: String,
    pub custom_fps: String,
    pub fps_mode: String,
    pub video_format: String,
    pub custom_video_width: String,
    pub interpolation_algo: String,

    // Audio
    pub audio_option: String,
    pub custom_abitrate: String,

    // Additional options & trimming
    pub trim_start: String,
    pub trim_end: String,
    pub trim_range: (f32, f32),
    pub trim_streamcopy: bool,
    pub precise_trim: bool,
    pub additional_options: String,
    pub additional_filter_options: String,
    pub additional_audio_filter_options: String,

    // Presets
    pub selected_preset: String,
    pub custom_preset_name: String,
    pub custom_presets: Vec<String>,
    pub preset_indicator: Option<String>,

    // Status display
    pub status_message: String,
}

impl Default for NvencFfxApp {
    fn default() -> Self {
        Self {
            input_file: String::new(),
            output_file: String::new(),
            ffmpeg_path: String::new(),

            codec: VideoCodec::Hevc,
            constant_qp_mode: true,
            quality_level: "30".to_owned(),
            bitrate: "6000".to_owned(),

            expand_encoder_settings: false,
            expand_fps_scale_settings: false,
            expand_audio_settings: false,
            expand_additional_options: false,
            expand_presets: false,

            threads: "4".to_owned(),
            preset: "p7".to_owned(),
            tune: "hq".to_owned(),
            profile: "main".to_owned(),
            level: "auto".to_owned(),
            tier: "1".to_owned(),
            coder: "cabac".to_owned(),
            hwaccel: "cuda".to_owned(),
            multipass: "qres".to_owned(),
            rate_control: "vbr".to_owned(),
            lookahead_level: "auto".to_owned(),
            split_encode_mode: "forced".to_owned(),
            cuda_output_format: false,
            spatial_aq: true,
            temporal_aq: true,
            strict_gop: false,
            no_scenecut: false,
            weighted_pred: false,

            fps_option: "source".to_owned(),
            custom_fps: "30".to_owned(),
            fps_mode: "auto".to_owned(),
            video_format: "source".to_owned(),
            custom_video_width: "1920".to_owned(),
            interpolation_algo: "bicubic".to_owned(),

            audio_option: "copy".to_owned(),
            custom_abitrate: "160".to_owned(),

            trim_start: "00:00:00".to_owned(),
            trim_end: "00:00:00".to_owned(),
            trim_range: (0.0, 1.0),
            trim_streamcopy: false,
            precise_trim: false,
            additional_options: String::new(),
            additional_filter_options: String::new(),
            additional_audio_filter_options: String::new(),

            selected_preset: "none".to_owned(),
            custom_preset_name: String::new(),
            custom_presets: Vec::new(),
            preset_indicator: None,

            status_message: "Ready for conversion".to_owned(),
        }
    }
}

/// Names of the `presets/*.json` files next to the executable (or the working directory).
fn scan_presets() -> Vec<String> {
    let dirs = [
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("presets"))),
        Some(std::path::PathBuf::from("presets")),
    ];
    let Some(dir) = dirs.into_iter().flatten().find(|d| d.is_dir()) else {
        return Vec::new();
    };
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
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

impl NvencFfxApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_theme(&cc.egui_ctx);
        Self {
            custom_presets: scan_presets(),
            ..Self::default()
        }
    }

    fn handle_drag_and_drop(&mut self, ctx: &egui::Context) {
        let Some(files) = handle_dropped_files(ctx) else { return };
        let Some(first_file) = files.first() else { return };
        self.input_file = first_file.to_string_lossy().to_string();

        // Auto-generate default output name
        if let Some(stem) = first_file.file_stem() {
            let ext = first_file.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
            let parent = first_file.parent().unwrap_or_else(|| std::path::Path::new(""));
            let out_name = format!("{}_{}_nvenc.{}", stem.to_string_lossy(), self.codec.suffix(), ext);
            self.output_file = parent.join(out_name).to_string_lossy().to_string();
        }

        self.status_message = "File selected. Ready for conversion.".to_owned();
    }

    // -- Top rows ------------------------------------------------------------

    fn path_row(
        ui: &mut Ui,
        text: &str,
        value: &mut String,
        id: &str,
        placeholder: &str,
        button_text: &str,
        kind: ButtonKind,
    ) -> egui::Response {
        let r = row(ui, grid::ROW_H);
        place_label(ui, cell(r, grid::LABEL_X, 200.0), text);
        place(ui, cell(r, grid::ENTRY_X, grid::ENTRY_W), |ui| {
            entry(
                ui,
                id,
                vec2(grid::ENTRY_W, dimensions::WIDGET_HEIGHT),
                value,
                EntryStyle { placeholder, ..Default::default() },
            )
        });
        place(ui, cell(r, grid::BUTTON_X, dimensions::BUTTON_WIDTH), |ui| {
            button(ui, vec2(dimensions::BUTTON_WIDTH, dimensions::WIDGET_HEIGHT), button_text, kind)
        })
    }

    fn quality_row(&mut self, ui: &mut Ui) {
        let r = row(ui, grid::ROW_H);
        let text = if self.constant_qp_mode { "Quality Level:" } else { "Video Bitrate (k):" };
        place_label(ui, cell(r, grid::LABEL_X, 200.0), text);
        let value = if self.constant_qp_mode { &mut self.quality_level } else { &mut self.bitrate };
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
            checkbox(ui, &mut self.constant_qp_mode, "Constant QP mode")
        });
        place(ui, cell(r, grid::BUTTON_X, dimensions::BUTTON_WIDTH), |ui| {
            button(ui, vec2(dimensions::BUTTON_WIDTH, dimensions::WIDGET_HEIGHT), "Output", ButtonKind::Grey)
                .on_hover_text("Right-click to copy command to clipboard")
        });
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
            if place(ui, rect, |ui| crate::ui::widgets::radio(ui, self.codec == codec, text)).clicked() {
                self.set_codec(codec);
            }
        }
        place(ui, cell(r, grid::BUTTON_X, dimensions::BUTTON_WIDTH), |ui| {
            button(ui, vec2(dimensions::BUTTON_WIDTH, dimensions::WIDGET_HEIGHT), "Help", ButtonKind::Grey)
        });
    }

    fn set_codec(&mut self, codec: VideoCodec) {
        if self.codec == codec {
            return;
        }
        self.codec = codec;
        match codec {
            VideoCodec::Hevc => self.profile = "main".to_owned(),
            VideoCodec::H264 => {
                self.profile = "main".to_owned();
                if self.tune == "uhq" {
                    self.tune = "hq".to_owned();
                }
            }
            VideoCodec::Av1 => self.profile.clear(),
        }
        if !self.levels().contains(&self.level.as_str()) {
            self.level = "auto".to_owned();
        }
    }

    fn levels(&self) -> &'static [&'static str] {
        match self.codec {
            VideoCodec::Hevc => LEVELS_HEVC,
            VideoCodec::H264 => LEVELS_H264,
            VideoCodec::Av1 => LEVELS_AV1,
        }
    }

    /// TextCheckbox header row; returns true while the section is expanded.
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
        place(ui, cell(header, grid::ENTRY_X, small.x), |ui| button(ui, small, "Auto", ButtonKind::Green));
        if place(ui, cell(header, grid::ENTRY_X + 90.0, small.x), |ui| {
            button(ui, small, "Default", ButtonKind::Green)
        })
        .clicked()
        {
            self.reset_encoder_settings();
        }

        ui.add_space(5.0);
        let f = section_frame(ui, grid::SECTION_X, grid::SECTION_W, 202.0);
        ui.add_space(5.0);

        let menu_w = dimensions::BUTTON_WIDTH;
        let row_rect = |i: usize, x: f32, w: f32| sub_rect(f, x, 7.0 + 32.0 * i as f32, w, dimensions::WIDGET_HEIGHT);
        let codec = self.codec;

        // Left column
        let (tunes, profiles): (&[&str], &[&str]) = match codec {
            VideoCodec::Hevc => (TUNES, PROFILES_HEVC),
            VideoCodec::H264 => (TUNES_H264, PROFILES_H264),
            VideoCodec::Av1 => (TUNES, &[]),
        };
        let levels = self.levels();
        let mut left: Vec<(&str, &str, &mut String, &[&str])> = vec![
            ("FF Threads:", "threads", &mut self.threads, THREADS),
            ("Preset:", "preset", &mut self.preset, PRESETS),
            ("Tune:", "tune", &mut self.tune, tunes),
        ];
        if codec != VideoCodec::Av1 {
            left.push(("Profile:", "profile", &mut self.profile, profiles));
        }
        left.push(("Level:", "level", &mut self.level, levels));
        if codec == VideoCodec::H264 {
            left.push(("Coder:", "coder", &mut self.coder, CODERS));
        } else {
            left.push(("Tier:", "tier", &mut self.tier, TIERS));
        }
        for (i, (text, id, value, values)) in left.into_iter().enumerate() {
            place_label(ui, row_rect(i, 10.0, 75.0), text);
            place(ui, row_rect(i, 87.0, menu_w), |ui| option_menu(ui, id, menu_w, value, values, true));
        }

        // Right column
        let rc_enabled = !self.constant_qp_mode;
        let mut right: Vec<(&str, &str, &mut String, &[&str], bool)> = vec![
            ("HW Accel:", "hwaccel", &mut self.hwaccel, HWACCELS, true),
            ("Multipass:", "multipass", &mut self.multipass, MULTIPASS, true),
            ("Rate-Control:", "rc", &mut self.rate_control, RATE_CONTROLS, rc_enabled),
            ("Lookahead Level:", "lookahead", &mut self.lookahead_level, LOOKAHEAD, true),
        ];
        if codec != VideoCodec::H264 {
            right.push(("Split Encode:", "split", &mut self.split_encode_mode, SPLIT_ENCODE, true));
        }
        for (i, (text, id, value, values, enabled)) in right.into_iter().enumerate() {
            place_label(ui, row_rect(i, 247.0, 105.0), text);
            place(ui, row_rect(i, 356.0, menu_w), |ui| option_menu(ui, id, menu_w, value, values, enabled));
        }

        let checks: [(&mut bool, &str); 6] = [
            (&mut self.cuda_output_format, "CUDA Output Format"),
            (&mut self.spatial_aq, "Spatial AQ"),
            (&mut self.temporal_aq, "Temporal AQ"),
            (&mut self.strict_gop, "Strict GOP"),
            (&mut self.no_scenecut, "No-Scenecut"),
            (&mut self.weighted_pred, "Weighted Prediction"),
        ];
        for (i, (value, text)) in checks.into_iter().enumerate() {
            place(ui, row_rect(i, 516.0, 210.0), |ui| checkbox(ui, value, text));
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

        const COLS: [f32; 5] = [124.0, 274.0, 380.0, 484.0, 588.0];
        let line = |y: f32, h: f32| sub_rect(f, 0.0, y, f.width(), h);
        let radios = |ui: &mut Ui, r: Rect, current: &mut String, options: &[(&str, &str)]| {
            for (x, (text, value)) in COLS.iter().zip(options) {
                place(ui, cell(r, *x, 100.0), |ui| radio_value(ui, current, value, text));
            }
        };

        let r = line(0.0, 38.0);
        place_label(ui, cell(r, 10.0, 100.0), "FPS:");
        radios(ui, r, &mut self.fps_option, &[
            ("Source", "source"),
            ("60", "60"),
            ("50", "50"),
            ("30", "30"),
            ("23.976", "24000/1001"),
        ]);

        let r = line(38.0, 42.0);
        let disabled = self.fps_option != "custom";
        place(ui, cell(r, 20.0, 80.0), |ui| {
            entry(ui, "custom_fps", vec2(80.0, dimensions::WIDGET_HEIGHT), &mut self.custom_fps,
                EntryStyle { centered: true, disabled, ..Default::default() })
        });
        radios(ui, r, &mut self.fps_option, &[("Custom", "custom")]);

        let r = line(80.0, 38.0);
        place_label(ui, cell(r, 10.0, 110.0), "FPS Mode:");
        radios(ui, r, &mut self.fps_mode, &[
            ("Auto", "auto"),
            ("Constant", "cfr"),
            ("Variable", "vfr"),
            ("Passthrough", "passthrough"),
        ]);

        let r = line(118.0, 32.0);
        place_label(ui, cell(r, 10.0, 110.0), "Video Format:");
        radios(ui, r, &mut self.video_format, &[
            ("Source", "source"),
            ("HD", "1280"),
            ("FHD", "1920"),
            ("QHD", "2560"),
            ("4K", "3840"),
        ]);

        let r = line(150.0, 42.0);
        let disabled = self.video_format != "custom";
        place(ui, cell(r, 20.0, 80.0), |ui| {
            entry(ui, "custom_width", vec2(80.0, dimensions::WIDGET_HEIGHT), &mut self.custom_video_width,
                EntryStyle { centered: true, disabled, ..Default::default() })
        });
        radios(ui, r, &mut self.video_format, &[("Custom", "custom")]);

        let r = line(192.0, 48.0);
        place_label(ui, cell(r, 10.0, 115.0), "Interpolation Algo:");
        place(ui, cell(r, 127.0, dimensions::BUTTON_WIDTH), |ui| {
            option_menu(ui, "interp", dimensions::BUTTON_WIDTH, &mut self.interpolation_algo, INTERPOLATIONS, true)
        });
        let description = interpolation_description(&self.interpolation_algo);
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

        const COLS: [f32; 5] = [25.0, 155.0, 285.0, 415.0, 545.0];
        let first = sub_rect(f, 0.0, 5.0, f.width(), 42.0);
        for (x, (text, value)) in COLS.iter().zip([
            ("Disable", "disable"),
            ("Source", "copy"),
            ("AAC 96", "aac_96k"),
            ("AAC 160", "aac_160k"),
            ("AAC 256", "aac_256k"),
        ]) {
            place(ui, cell(first, *x, 100.0), |ui| radio_value(ui, &mut self.audio_option, value, text));
        }

        let second = sub_rect(f, 0.0, 47.0, f.width(), 42.0);
        let disabled = self.audio_option != "custom";
        place(ui, cell(second, 30.0, 80.0), |ui| {
            entry(ui, "custom_abitrate", vec2(80.0, dimensions::WIDGET_HEIGHT), &mut self.custom_abitrate,
                EntryStyle { centered: true, disabled, ..Default::default() })
        });
        for (x, (text, value)) in COLS[1..].iter().zip([
            ("Custom", "custom"),
            ("Opus 96", "opus_96k"),
            ("Opus 160", "opus_160k"),
            ("Opus 256", "opus_256k"),
        ]) {
            place(ui, cell(second, *x, 100.0), |ui| radio_value(ui, &mut self.audio_option, value, text));
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

        // Trim slider + reset
        let (start, end) = &mut self.trim_range;
        place(ui, sub_rect(f, 10.0, 10.0, 672.0, 30.0), |ui| trim_slider(ui, vec2(672.0, 30.0), start, end));
        if place(ui, sub_rect(f, 692.0, 10.0, 30.0, 30.0), |ui| reset_button(ui, vec2(30.0, 30.0))).clicked() {
            self.trim_range = (0.0, 1.0);
        }

        // Trimming row
        let r = sub_rect(f, 0.0, 40.0, f.width(), 48.0);
        place_label(ui, cell(r, 10.0, 80.0), "Trimming:");
        place_label(ui, cell(r, 160.0, 36.0), "From:");
        place(ui, cell(r, 198.0, 65.0), |ui| {
            entry(ui, "trim_start", vec2(65.0, h), &mut self.trim_start, EntryStyle { centered: true, ..Default::default() })
        });
        place_label(ui, cell(r, 274.0, 22.0), "To:");
        place(ui, cell(r, 297.0, 65.0), |ui| {
            entry(ui, "trim_end", vec2(65.0, h), &mut self.trim_end, EntryStyle { centered: true, ..Default::default() })
        });
        place(ui, cell(r, 372.0, 80.0), |ui| button(ui, vec2(80.0, h), "Trim", ButtonKind::Green));
        place(ui, cell(r, 462.0, 70.0), |ui| {
            if checkbox(ui, &mut self.trim_streamcopy, "Copy").clicked() && self.trim_streamcopy {
                self.audio_option = "copy".to_owned();
            }
        });
        place(ui, cell(r, 532.0, 100.0), |ui| checkbox(ui, &mut self.precise_trim, "Precise"));

        // Option / filter entries
        let entry_w = 537.0;
        let fields: [(&str, &str, &mut String, &str, bool); 3] = [
            ("Add FF Options:", "ff_options", &mut self.additional_options, FF_OPTIONS_PLACEHOLDER, true),
            ("Add Video Filters (-vf):", "vf", &mut self.additional_filter_options, VF_PLACEHOLDER, true),
            ("Add Audio Filters (-af):", "af", &mut self.additional_audio_filter_options, AF_PLACEHOLDER, false),
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
        let rows: [&[(&str, ButtonKind)]; 3] = [
            &[
                ("Brightness", ButtonKind::Grey),
                ("Sharpness", ButtonKind::Grey),
                ("H-Flip", ButtonKind::Grey),
                ("Speed up X2", ButtonKind::Grey),
                ("Audio fix", ButtonKind::Grey),
                ("Clear", ButtonKind::Red),
            ],
            &[
                ("Saturation", ButtonKind::Grey),
                ("Denoise", ButtonKind::Grey),
                ("V-Flip", ButtonKind::Grey),
                ("Slow down X2", ButtonKind::Grey),
                ("Stereo out", ButtonKind::Grey),
                ("Streams", ButtonKind::Green),
            ],
            &[
                ("Gamma RGB", ButtonKind::Grey),
                ("Deshake", ButtonKind::Grey),
                ("Rotate 90°", ButtonKind::Grey),
                ("Crop to 16:9", ButtonKind::Grey),
                ("HDR to SDR", ButtonKind::Grey),
            ],
        ];
        for (ri, buttons) in rows.iter().enumerate() {
            for (bi, (text, kind)) in buttons.iter().enumerate() {
                let rect = sub_rect(f, 10.0 + 116.0 * bi as f32, 232.0 + 38.0 * ri as f32, 106.0, h);
                place(ui, rect, |ui| button(ui, rect.size(), text, *kind));
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
            if place(ui, cell(r, x, 130.0), |ui| radio_value(ui, &mut self.selected_preset, value, text)).changed() {
                self.preset_indicator = None;
            }
        }

        let r = sub_rect(f, 0.0, 32.0, f.width(), 38.0);
        place(ui, cell(r, 10.0, 100.0), |ui| radio_value(ui, &mut self.selected_preset, "custom", "Custom"));
        place_label(ui, cell(r, 145.0, 50.0), "Preset:");
        let presets: Vec<&str> = self.custom_presets.iter().map(String::as_str).collect();
        let picked = place(ui, cell(r, 196.0, 170.0), |ui| {
            option_menu(ui, "custom_preset", 170.0, &mut self.custom_preset_name, &presets, true)
        });
        if picked.changed() {
            self.selected_preset = "custom".to_owned();
            self.preset_indicator = Some(self.custom_preset_name.clone());
        }
        place(ui, cell(r, 385.0, 70.0), |ui| button(ui, vec2(70.0, h), "Save As", ButtonKind::Green));
        place(ui, cell(r, 465.0, 60.0), |ui| button(ui, vec2(60.0, h), "Save", ButtonKind::Green));
        place(ui, cell(r, 535.0, 60.0), |ui| button(ui, vec2(60.0, h), "Delete", ButtonKind::Red));

        let r = sub_rect(f, 0.0, 70.0, f.width(), 38.0);
        place(ui, cell(r, 10.0, 600.0), |ui| match &self.preset_indicator {
            Some(name) => colored_label(ui, &format!("Loaded preset: {name}"), colors::ACCENT_GREEN),
            None => colored_label(ui, "No preset selected", colors::PLACEHOLDER),
        });
    }

    fn status_row(&self, ui: &mut Ui) {
        let r = row(ui, grid::ROW_H);
        if self.constant_qp_mode {
            place_label(ui, cell(r, 5.0, 300.0), "Estimated size: Not available for CQP");
        }
        ui.painter().text(
            egui::pos2(r.min.x + grid::STATUS_CENTER_X, r.center().y),
            Align2::CENTER_CENTER,
            &self.status_message,
            egui::FontId::proportional(dimensions::FONT_SIZE),
            colors::LABEL_TEXT,
        );
    }

    fn reset_encoder_settings(&mut self) {
        let d = Self::default();
        self.threads = d.threads;
        self.preset = d.preset;
        self.tune = d.tune;
        self.level = d.level;
        self.tier = d.tier;
        self.coder = d.coder;
        self.hwaccel = d.hwaccel;
        self.multipass = d.multipass;
        self.rate_control = d.rate_control;
        self.lookahead_level = d.lookahead_level;
        self.split_encode_mode = d.split_encode_mode;
        self.cuda_output_format = d.cuda_output_format;
        self.spatial_aq = d.spatial_aq;
        self.temporal_aq = d.temporal_aq;
        self.strict_gop = d.strict_gop;
        self.no_scenecut = d.no_scenecut;
        self.weighted_pred = d.weighted_pred;
        if self.codec != VideoCodec::Av1 {
            self.profile = d.profile;
        }
    }

    // -- Bottom bar ----------------------------------------------------------

    fn bottom_bar(&mut self, ui: &mut Ui) {
        let h = dimensions::BIG_BUTTON_HEIGHT;
        let gap = 4.0;
        let r = row(ui, h);
        let w = (r.width() - gap * 4.0) / 5.0;
        for (i, text) in ["Play Input File", "Play Output File", "Play 10s Preview", "Batch Convert", "Screen Record"]
            .into_iter()
            .enumerate()
        {
            let rect = cell(r, (w + gap) * i as f32, w);
            place(ui, rect, |ui| button(ui, rect.size(), text, ButtonKind::Grey));
        }
        ui.add_space(10.0);
        let r = row(ui, h);
        if place(ui, r, |ui| bold_button(ui, r.size(), "Convert", ButtonKind::Green)).clicked() {
            self.status_message = "Starting conversion...".to_owned();
        }
    }
}

impl eframe::App for NvencFfxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_drag_and_drop(ctx);

        egui::TopBottomPanel::bottom("action_bar")
            .show_separator_line(false)
            .frame(egui::Frame::none().fill(colors::PRIMARY_BG).inner_margin(Margin {
                left: grid::BOTTOM_PAD_X,
                right: grid::BOTTOM_PAD_X,
                top: 0.0,
                bottom: grid::BOTTOM_PAD_Y,
            }))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                self.bottom_bar(ui);
            });

        let panel_frame = egui::Frame::none()
            .fill(colors::PRIMARY_BG)
            .inner_margin(Margin::same(grid::OUTER_PAD));

        egui::CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    egui::Frame::none()
                        .inner_margin(Margin {
                            left: grid::CONTENT_PAD,
                            right: 0.0,
                            top: grid::CONTENT_PAD,
                            bottom: 15.0,
                        })
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;

                            Self::path_row(ui, "Input File:", &mut self.input_file, "input",
                                INPUT_PLACEHOLDER, "Browse", ButtonKind::Green)
                                .on_hover_text("Right-click to open containing folder");
                            Self::path_row(ui, "Output File:", &mut self.output_file, "output",
                                OUTPUT_PLACEHOLDER, "Save As", ButtonKind::Green)
                                .on_hover_text("Right-click to open containing folder");
                            Self::path_row(ui, "FFmpeg Path:", &mut self.ffmpeg_path, "ffmpeg",
                                FFMPEG_PLACEHOLDER, "FFmpeg", ButtonKind::Grey)
                                .on_hover_text("Right-click to open containing folder");
                            self.quality_row(ui);
                            self.codec_row(ui);

                            self.encoder_section(ui);
                            self.fps_section(ui);
                            self.audio_section(ui);
                            self.additional_section(ui);
                            self.presets_section(ui);

                            self.status_row(ui);
                        });
                });
        });

        file_drag_drop_overlay(ctx);
    }
}
