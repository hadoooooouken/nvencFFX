//! The persisted settings model: the same keys, value types and key order as
//! `_get_current_settings` in `nvencFFX.py`, so preset files stay interchangeable.

use std::path::{Path, PathBuf};
use serde::Serialize;
use serde_json::{Map, Value};

/// `nff_settings.json` next to the executable, where the original keeps it.
pub fn settings_file() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("nff_settings.json")))
        .unwrap_or_else(|| PathBuf::from("nff_settings.json"))
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    Hevc,
    H264,
    Av1,
}

impl VideoCodec {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Hevc => "hevc",
            Self::H264 => "h264",
            Self::Av1 => "av1",
        }
    }

    fn from_suffix(s: &str) -> Option<Self> {
        [Self::Hevc, Self::H264, Self::Av1].into_iter().find(|c| c.suffix() == s)
    }

    pub fn encoder(self) -> &'static str {
        match self {
            Self::Hevc => "hevc_nvenc",
            Self::H264 => "h264_nvenc",
            Self::Av1 => "av1_nvenc",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Settings {
    pub ffmpeg_path: String,
    pub last_input_dir: String,
    pub last_output_dir: String,
    pub codec: VideoCodec,
    pub bitrate: String,
    pub constant_qp_mode: bool,
    pub quality_level: String,
    pub audio_option: String,
    pub custom_abitrate: String,

    pub encoder_threads: String,
    pub encoder_preset: String,
    pub encoder_tune: String,
    pub encoder_profile: String,
    pub encoder_level: String,
    pub encoder_tier: String,
    pub encoder_coder: String,
    pub encoder_hwaccel: String,
    pub encoder_multipass: String,
    pub encoder_rc: String,
    pub encoder_lookahead_level: String,
    pub encoder_split_encode_mode: String,

    pub fps_option: String,
    pub custom_fps: String,
    pub fps_mode: String,
    pub video_format_option: String,
    pub custom_video_width: String,
    pub interpolation_algo: String,

    pub cuda_output_format: bool,
    pub encoder_spatial_aq: bool,
    pub encoder_temporal_aq: bool,
    pub encoder_strict_gop: bool,
    pub encoder_no_scenecut: bool,
    pub encoder_weighted_pred: bool,

    pub trim_start: String,
    pub trim_end: String,
    pub trim_streamcopy: bool,
    pub precise_trim: bool,

    pub additional_options: String,
    pub additional_filter_options: String,
    pub additional_audio_filter_options: String,

    pub selected_preset: String,
    /// The preset dropdown's value. Written out only while `selected_preset` is
    /// "custom", like the original.
    pub custom_preset_selected: String,

    pub batch_output_folder: String,
    pub batch_change_container: bool,
    pub batch_output_container: String,

    /// Always this build's version: `apply_json` ignores the stored one, so saving
    /// stamps the writer's version like the original. The schema is unchanged since
    /// 1.x; a migration step would go into `load`, keyed on the file's version.
    pub version: String,
}

impl Default for Settings {
    fn default() -> Self {
        let s = |v: &str| v.to_owned();
        Self {
            ffmpeg_path: String::new(),
            last_input_dir: String::new(),
            last_output_dir: String::new(),
            codec: VideoCodec::Hevc,
            bitrate: s("6000"),
            constant_qp_mode: true,
            quality_level: s("30"),
            audio_option: s("copy"),
            custom_abitrate: s("160"),

            encoder_threads: s("4"),
            encoder_preset: s("p7"),
            encoder_tune: s("hq"),
            encoder_profile: s("main"),
            encoder_level: s("auto"),
            encoder_tier: s("1"),
            encoder_coder: s("cabac"),
            encoder_hwaccel: s("cuda"),
            encoder_multipass: s("qres"),
            encoder_rc: s("vbr"),
            encoder_lookahead_level: s("auto"),
            encoder_split_encode_mode: s("forced"),

            fps_option: s("source"),
            custom_fps: s("30"),
            fps_mode: s("auto"),
            video_format_option: s("source"),
            custom_video_width: s("1920"),
            interpolation_algo: s("bicubic"),

            cuda_output_format: false,
            encoder_spatial_aq: true,
            encoder_temporal_aq: true,
            encoder_strict_gop: false,
            encoder_no_scenecut: false,
            encoder_weighted_pred: false,

            trim_start: s("00:00:00"),
            trim_end: s("00:00:00"),
            trim_streamcopy: false,
            precise_trim: false,

            additional_options: String::new(),
            additional_filter_options: String::new(),
            additional_audio_filter_options: String::new(),

            selected_preset: s("none"),
            custom_preset_selected: String::new(),

            batch_output_folder: String::new(),
            batch_change_container: false,
            batch_output_container: s("mp4"),

            version: s(env!("CARGO_PKG_VERSION")),
        }
    }
}

impl Settings {
    /// JSON object in the original's key order (`_get_current_settings`).
    pub fn to_json(&self) -> Map<String, Value> {
        let Ok(Value::Object(mut map)) = serde_json::to_value(self) else {
            unreachable!("Settings serializes to an object");
        };
        if self.selected_preset != "custom" {
            map.insert("custom_preset_selected".to_owned(), Value::String(String::new()));
        }
        map
    }

    /// `_load_settings`: merges the file into `self`. A missing file is not an error.
    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        // Editors such as Notepad may have saved it with a BOM
        let text = text.trim_start_matches('\u{feff}');
        let map: Map<String, Value> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        self.apply_json(&map);
        Ok(())
    }

    /// Writes `nff_settings.json` / a preset file with 4-space indentation and raw
    /// UTF-8, like `json.dump(indent=4, ensure_ascii=False)`.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut out = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
        let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
        self.to_json().serialize(&mut ser)?;
        std::fs::write(path, out)
    }

    /// Merges a settings / preset object into `self` like `_apply_settings_dict`:
    /// missing or empty keys keep the current value, except the three
    /// additional-option fields, which a preset without them clears.
    pub fn apply_json(&mut self, map: &Map<String, Value>) {
        let text = |key: &str| match map.get(key) {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            Some(Value::Number(n)) => Some(n.to_string()),
            _ => None,
        };
        let flag = |key: &str| map.get(key).and_then(Value::as_bool);

        macro_rules! texts {
            ($($field:ident),* $(,)?) => {$(
                if let Some(v) = text(stringify!($field)) {
                    self.$field = v;
                }
            )*};
        }
        macro_rules! flags {
            ($($field:ident),* $(,)?) => {$(
                if let Some(v) = flag(stringify!($field)) {
                    self.$field = v;
                }
            )*};
        }

        if let Some(path) = text("ffmpeg_path")
            && std::path::Path::new(&path).exists()
        {
            self.ffmpeg_path = path;
        }
        texts!(last_input_dir, last_output_dir);
        if let Some(codec) = text("codec").as_deref().and_then(VideoCodec::from_suffix) {
            self.codec = codec;
        }
        flags!(constant_qp_mode);
        texts!(quality_level, bitrate, audio_option, custom_abitrate);
        texts!(
            encoder_threads,
            encoder_preset,
            encoder_tune,
            encoder_profile,
            encoder_level,
            encoder_tier,
            encoder_coder,
            encoder_hwaccel,
            encoder_multipass,
            encoder_rc,
            encoder_lookahead_level,
            encoder_split_encode_mode,
        );
        flags!(
            cuda_output_format,
            encoder_spatial_aq,
            encoder_temporal_aq,
            encoder_strict_gop,
            encoder_no_scenecut,
            encoder_weighted_pred,
        );
        texts!(fps_option, custom_fps, fps_mode, video_format_option, custom_video_width, interpolation_algo);
        texts!(trim_start, trim_end);
        flags!(trim_streamcopy, precise_trim);

        self.additional_options = text("additional_options").unwrap_or_default();
        self.additional_filter_options = text("additional_filter_options").unwrap_or_default();
        self.additional_audio_filter_options =
            text("additional_audio_filter_options").unwrap_or_default();

        texts!(selected_preset, custom_preset_selected);

        if let Some(folder) = text("batch_output_folder")
            && std::path::Path::new(&folder).exists()
        {
            self.batch_output_folder = folder;
        }
        flags!(batch_change_container);
        texts!(batch_output_container);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_keys_follow_the_original_order() {
        let keys: Vec<String> = Settings::default().to_json().keys().cloned().collect();
        assert_eq!(keys.first().map(String::as_str), Some("ffmpeg_path"));
        assert_eq!(keys.last().map(String::as_str), Some("version"));
        assert_eq!(keys.len(), 46);
        let json = serde_json::to_string(&Settings::default().to_json()).unwrap();
        assert!(json.contains(r#""codec":"hevc""#));
    }

    #[test]
    fn custom_preset_name_is_only_kept_for_custom_selection() {
        let mut s = Settings { custom_preset_selected: "mine".into(), ..Default::default() };
        assert_eq!(s.to_json()["custom_preset_selected"], "");
        s.selected_preset = "custom".into();
        assert_eq!(s.to_json()["custom_preset_selected"], "mine");
    }

    #[test]
    fn apply_merges_like_the_original() {
        let mut s = Settings {
            additional_filter_options: "hflip".into(),
            encoder_preset: "p3".into(),
            ..Default::default()
        };
        let preset: Map<String, Value> = serde_json::from_str(
            r#"{"encoder_hwaccel": "auto", "encoder_tune": "", "codec": "av1",
                "additional_options": "-ac 2", "encoder_spatial_aq": false}"#,
        )
        .unwrap();
        s.apply_json(&preset);
        assert_eq!(s.encoder_hwaccel, "auto");
        assert_eq!(s.encoder_tune, "hq", "empty value keeps the current one");
        assert_eq!(s.encoder_preset, "p3", "missing key keeps the current one");
        assert_eq!(s.codec, VideoCodec::Av1);
        assert!(!s.encoder_spatial_aq);
        assert_eq!(s.additional_options, "-ac 2");
        assert_eq!(s.additional_filter_options, "", "missing filter field is cleared");
    }

    #[test]
    fn settings_file_round_trip() {
        let file = std::env::temp_dir().join(format!("nvencffx-settings-{}.json", std::process::id()));
        let mut fresh = Settings::default();
        fresh.load(&file).expect("a missing file keeps the defaults");
        assert_eq!(fresh, Settings::default());

        // A 1.8.4 file: older version, keys in any order, some missing
        std::fs::write(
            &file,
            r#"{"version": "1.8.4", "codec": "h264", "last_input_dir": "D:\\clips",
                "trim_streamcopy": true, "custom_abitrate": "192"}"#,
        )
        .unwrap();
        let mut s = Settings::default();
        s.load(&file).unwrap();
        assert_eq!(s.codec, VideoCodec::H264);
        assert_eq!(s.last_input_dir, r"D:\clips");
        assert!(s.trim_streamcopy);
        assert_eq!(s.custom_abitrate, "192");
        assert_eq!(s.bitrate, "6000", "missing key keeps the default");
        assert_eq!(s.version, env!("CARGO_PKG_VERSION"), "saving stamps this build's version");

        s.save(&file).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("{\n    \"ffmpeg_path\": \"\",\n"));
        let mut back = Settings::default();
        back.load(&file).unwrap();
        assert_eq!(back, s);

        std::fs::write(&file, "\u{feff}{\"bitrate\": \"7000\"}").unwrap();
        s.load(&file).unwrap();
        assert_eq!(s.bitrate, "7000", "a BOM is skipped");

        std::fs::write(&file, "not json").unwrap();
        assert!(Settings::default().load(&file).is_err());
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn round_trips_through_json() {
        let mut s = Settings { codec: VideoCodec::H264, bitrate: "8000".into(), ..Default::default() };
        s.selected_preset = "custom".into();
        s.custom_preset_selected = "x".into();
        let mut back = Settings::default();
        back.apply_json(&s.to_json());
        assert_eq!(back, s);
    }
}
