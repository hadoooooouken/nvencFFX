//! `presets/*.json` files and the built-in presets of the Presets section.

use std::path::{Path, PathBuf};
use serde_json::{Map, Value};
use super::settings::{Settings, VideoCodec};

/// The `presets` folder next to the executable. Falls back to `./presets` when only
/// that one exists (`cargo run`), and creates the exe-side folder otherwise.
pub fn presets_dir() -> PathBuf {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("presets")));
    let cwd = PathBuf::from("presets");
    match beside_exe {
        Some(dir) if dir.is_dir() => dir,
        _ if cwd.is_dir() => cwd,
        Some(dir) => {
            let _ = std::fs::create_dir_all(&dir);
            dir
        }
        None => cwd,
    }
}

/// Preset names (file stems), sorted case-insensitively.
pub fn list(dir: &Path) -> Vec<String> {
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

pub fn path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.json"))
}

pub fn load(dir: &Path, name: &str) -> Result<Map<String, Value>, String> {
    let file = path(dir, name);
    let text = std::fs::read_to_string(&file)
        .map_err(|_| format!("Preset file not found: {}", file.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("Failed to load preset: {e}"))
}

pub fn save(dir: &Path, name: &str, settings: &Settings) -> Result<(), String> {
    settings.save(&path(dir, name)).map_err(|e| format!("Failed to save preset: {e}"))
}

pub fn delete(dir: &Path, name: &str) -> Result<(), String> {
    std::fs::remove_file(path(dir, name)).map_err(|e| format!("Failed to delete preset: {e}"))
}

/// Display name of a built-in preset radio value.
pub fn builtin_name(value: &str) -> &str {
    match value {
        "fhdf" => "FHD Fast",
        "fhdq" => "FHD Quality",
        "hdf" => "HD Fast",
        "hdq" => "HD Quality",
        other => other,
    }
}

/// `_apply_preset`: "none" restores the defaults, the others apply the FHD/HD
/// Fast/Quality profiles. Both first clear the filters and trimming; the caller
/// moves `trim_end` to the probed duration afterwards.
pub fn apply_builtin(s: &mut Settings, value: &str) {
    s.additional_options.clear();
    s.additional_filter_options.clear();
    s.additional_audio_filter_options.clear();
    s.trim_start = "00:00:00".to_owned();
    s.trim_end = "00:00:00".to_owned();
    s.trim_streamcopy = false;
    s.precise_trim = false;

    let set = |field: &mut String, v: &str| *field = v.to_owned();
    if value == "none" {
        let d = Settings::default();
        s.bitrate = d.bitrate;
        s.constant_qp_mode = d.constant_qp_mode;
        s.quality_level = d.quality_level;
        s.audio_option = d.audio_option;
        s.custom_abitrate = d.custom_abitrate;
        s.encoder_threads = d.encoder_threads;
        s.encoder_hwaccel = d.encoder_hwaccel;
        s.cuda_output_format = d.cuda_output_format;
        s.encoder_preset = d.encoder_preset;
        s.encoder_tune = d.encoder_tune;
        s.encoder_profile = d.encoder_profile;
        s.encoder_level = d.encoder_level;
        s.encoder_tier = d.encoder_tier;
        s.encoder_multipass = d.encoder_multipass;
        s.encoder_rc = d.encoder_rc;
        s.encoder_lookahead_level = d.encoder_lookahead_level;
        s.encoder_spatial_aq = d.encoder_spatial_aq;
        s.encoder_temporal_aq = d.encoder_temporal_aq;
        s.encoder_no_scenecut = d.encoder_no_scenecut;
        s.encoder_weighted_pred = d.encoder_weighted_pred;
        s.encoder_strict_gop = d.encoder_strict_gop;
        s.encoder_split_encode_mode = d.encoder_split_encode_mode;
        s.encoder_coder = d.encoder_coder;
        s.fps_option = d.fps_option;
        s.custom_fps = d.custom_fps;
        s.video_format_option = d.video_format_option;
        s.custom_video_width = d.custom_video_width;
        s.interpolation_algo = d.interpolation_algo;
        set(&mut s.selected_preset, "none");
        return;
    }

    // (bitrate, quality, width, interpolation, encoder preset, multipass, quality profile)
    let (bitrate, quality, width, algo, preset, multipass, hq) = match value {
        "fhdf" => ("6000", "30", "1920", "bicubic", "p3", "disabled", false),
        "fhdq" => ("8000", "27", "1920", "spline", "p7", "fullres", true),
        "hdf" => ("5000", "30", "1280", "bicubic", "p3", "disabled", false),
        "hdq" => ("6000", "27", "1280", "spline", "p7", "fullres", true),
        _ => return,
    };
    set(&mut s.bitrate, bitrate);
    s.constant_qp_mode = true;
    set(&mut s.quality_level, quality);
    set(&mut s.video_format_option, width);
    set(&mut s.interpolation_algo, algo);

    s.cuda_output_format = false;
    set(&mut s.encoder_preset, preset);
    set(&mut s.encoder_tune, "hq");
    set(&mut s.encoder_profile, if s.codec == VideoCodec::Hevc { "main" } else { "high" });
    set(&mut s.encoder_tier, "1");
    set(&mut s.encoder_multipass, multipass);
    set(&mut s.encoder_rc, "vbr");
    if !hq {
        set(&mut s.encoder_lookahead_level, "0");
    } else if s.codec != VideoCodec::H264 {
        set(&mut s.encoder_lookahead_level, "auto");
    }
    set(&mut s.encoder_split_encode_mode, "auto");

    s.encoder_spatial_aq = hq;
    s.encoder_temporal_aq = hq;
    s.encoder_no_scenecut = false;
    s.encoder_weighted_pred = false;
    s.encoder_strict_gop = false;

    set(&mut s.audio_option, "aac_160k");
    set(&mut s.selected_preset, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_presets_match_the_original() {
        let mut s = Settings { additional_options: "-ss 1".into(), ..Default::default() };
        apply_builtin(&mut s, "hdq");
        assert_eq!(s.video_format_option, "1280");
        assert_eq!(s.interpolation_algo, "spline");
        assert_eq!(s.encoder_multipass, "fullres");
        assert_eq!(s.audio_option, "aac_160k");
        assert_eq!(s.selected_preset, "hdq");
        assert!(s.additional_options.is_empty());

        let mut s = Settings { codec: VideoCodec::H264, encoder_lookahead_level: "5".into(), ..Default::default() };
        apply_builtin(&mut s, "fhdq");
        assert_eq!(s.encoder_profile, "high");
        assert_eq!(s.encoder_lookahead_level, "5", "H.264 keeps lookahead on quality presets");

        apply_builtin(&mut s, "none");
        assert_eq!(s.encoder_preset, "p7");
        assert_eq!(s.video_format_option, "source");
        assert_eq!(s.selected_preset, "none");
    }

    #[test]
    fn save_load_delete_round_trip() {
        let dir = std::env::temp_dir().join(format!("nvencffx-presets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let s = Settings { bitrate: "1234".into(), ..Default::default() };
        save(&dir, "test", &s).unwrap();
        let text = std::fs::read_to_string(path(&dir, "test")).unwrap();
        assert!(text.starts_with("{\n    \"ffmpeg_path\""));
        assert_eq!(list(&dir), ["test"]);
        assert_eq!(load(&dir, "test").unwrap()["bitrate"], "1234");
        delete(&dir, "test").unwrap();
        assert!(list(&dir).is_empty());
        let _ = std::fs::remove_dir(&dir);
    }
}
