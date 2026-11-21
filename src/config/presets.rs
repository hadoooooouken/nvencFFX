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

/// A custom preset item found on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetItem {
    /// Relative identifier used for loading/saving, e.g. `"vfx/8bit_pixelation"`.
    pub id: String,
    /// Category / folder name, e.g. `"vfx"` (or empty for root presets).
    pub group: String,
    /// Preset name without folder prefix, e.g. `"8bit_pixelation"`.
    pub name: String,
}

impl PresetItem {
    pub fn display_label(&self) -> String {
        if self.group.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", self.group, self.name)
        }
    }
}

/// Validates and normalizes a preset identifier (`\` -> `/`).
/// Allows either `"name"` or a 1-level subfolder `"group/name"`.
pub fn normalize_name(name: &str) -> Result<String, String> {
    let normalized = name.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Err("Preset name cannot be empty.".to_owned());
    }
    if normalized.contains([':', '*', '?', '"', '<', '>', '|']) {
        return Err(format!("Invalid preset name: '{name}'"));
    }
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.len() > 2 {
        return Err("Only one subfolder level is supported (e.g. 'group/name').".to_owned());
    }
    for part in &parts {
        if part.trim().is_empty() || *part == "." || *part == ".." {
            return Err(format!("Invalid preset name: '{name}'"));
        }
    }
    if parts.len() == 2 {
        let folder = parts[0];
        if folder.eq_ignore_ascii_case("shaders") || folder.starts_with('.') {
            return Err(format!("Reserved folder name: '{folder}'"));
        }
    }
    Ok(parts.join("/"))
}

fn parse_root_stem(stem: &str) -> (String, String) {
    if let Some(rest) = stem.strip_prefix('[')
        && let Some((raw_group, raw_name)) = rest.split_once(']')
    {
        let group = raw_group.trim();
        let name = raw_name.trim_start();
        if !group.is_empty() && !name.is_empty() {
            return (group.to_owned(), name.to_owned());
        }
    }
    (String::new(), stem.to_owned())
}

/// Preset items found in `dir` (both root files and 1-level subfolders),
/// sorted by group, then by name. Auxiliary folders like `shaders` are excluded.
pub fn list(dir: &Path) -> Vec<PresetItem> {
    let mut items = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return items };

    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let Some(folder_name) = p.file_name().and_then(|n| n.to_str()) else { continue };
            if folder_name.eq_ignore_ascii_case("shaders") || folder_name.starts_with('.') {
                continue;
            }
            if let Ok(sub_entries) = std::fs::read_dir(&p) {
                for sub in sub_entries.flatten() {
                    let sp = sub.path();
                    if sp.is_file()
                        && sp.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"))
                        && let Some(stem) = sp.file_stem().and_then(|s| s.to_str())
                    {
                        items.push(PresetItem {
                            id: format!("{folder_name}/{stem}"),
                            group: folder_name.to_owned(),
                            name: stem.to_owned(),
                        });
                    }
                }
            }
        } else if p.is_file()
            && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"))
            && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
        {
            let (group, name) = parse_root_stem(stem);
            items.push(PresetItem {
                id: stem.to_owned(),
                group,
                name,
            });
        }
    }

    // Grouped items come first alphabetically by group; root items (empty group) come at the end
    items.sort_by(|a, b| {
        a.group
            .is_empty()
            .cmp(&b.group.is_empty())
            .then_with(|| a.group.to_lowercase().cmp(&b.group.to_lowercase()))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.to_lowercase().cmp(&b.id.to_lowercase()))
    });
    items
}

pub fn path(dir: &Path, name: &str) -> PathBuf {
    let normalized = name.replace('\\', "/");
    let mut out = dir.to_path_buf();
    let mut parts = normalized.split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            out.push(format!("{part}.json"));
        } else {
            out.push(part);
        }
    }
    out
}

pub fn load(dir: &Path, name: &str) -> Result<Map<String, Value>, String> {
    let file = path(dir, name);
    let text = std::fs::read_to_string(&file)
        .map_err(|_| format!("Preset file not found: {}", file.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("Failed to load preset: {e}"))
}

pub fn save(dir: &Path, name: &str, settings: &Settings) -> Result<(), String> {
    let clean = normalize_name(name)?;
    let file = path(dir, &clean);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create preset folder: {e}"))?;
    }
    settings.save(&file).map_err(|e| format!("Failed to save preset: {e}"))
}

pub fn delete(dir: &Path, name: &str) -> Result<(), String> {
    let file = path(dir, name);
    std::fs::remove_file(&file).map_err(|e| format!("Failed to delete preset: {e}"))?;
    if let Some(parent) = file.parent()
        && parent != dir
    {
        let _ = std::fs::remove_dir(parent);
    }
    Ok(())
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

/// "none" restores the defaults, the others apply the FHD/HD Fast/Quality profiles. Both first clear the filters and trimming; the caller
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
    fn builtin_presets_have_fixed_values() {
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
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let s = Settings { bitrate: "1234".into(), ..Default::default() };
        save(&dir, "test", &s).unwrap();
        let text = std::fs::read_to_string(path(&dir, "test")).unwrap();
        assert!(text.starts_with("{\r\n    \"ffmpeg_path\""));
        assert_eq!(list(&dir), [PresetItem { id: "test".into(), group: String::new(), name: "test".into() }]);
        assert_eq!(load(&dir, "test").unwrap()["bitrate"], "1234");
        delete(&dir, "test").unwrap();
        assert!(list(&dir).is_empty());

        // Subfolder round trip + empty subfolder cleanup
        save(&dir, r"vfx\test_vfx", &s).unwrap();
        assert_eq!(list(&dir), [PresetItem { id: "vfx/test_vfx".into(), group: "vfx".into(), name: "test_vfx".into() }]);
        assert_eq!(load(&dir, "vfx/test_vfx").unwrap()["bitrate"], "1234");
        delete(&dir, "vfx/test_vfx").unwrap();
        assert!(list(&dir).is_empty());
        assert!(!dir.join("vfx").exists(), "empty subfolder should be removed on delete");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_and_root_tag_edge_cases() {
        assert_eq!(normalize_name(r" vfx\custom ").unwrap(), "vfx/custom");
        assert!(normalize_name("").is_err());
        assert!(normalize_name("../escape").is_err());
        assert!(normalize_name("a/b/c").is_err());
        assert!(normalize_name("shaders/foo").is_err());
        assert!(normalize_name(".hidden/foo").is_err());
        assert!(normalize_name("vfx/").is_err());
        assert!(normalize_name("C:/evil").is_err());

        assert_eq!(parse_root_stem("[vfx] cool"), ("vfx".into(), "cool".into()));
        assert_eq!(parse_root_stem("[vfx]"), (String::new(), "[vfx]".into()));
        assert_eq!(parse_root_stem("[] cool"), (String::new(), "[] cool".into()));
    }

    /// The presets shipped in `presets/`, written by 1.7.x / 1.8.x with subsets of the
    /// keys: every value they hold reaches the settings (paths only if they exist, the
    /// version never, empty strings keep the current value).
    #[test]
    fn shipped_presets_apply_every_value() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("presets");
        let items = list(&dir);
        assert!(!items.is_empty(), "presets folder should not be empty");
        assert!(items.iter().all(|i| i.group != "shaders"), "shaders folder must be excluded from preset items");
        assert!(items.iter().any(|i| i.id == "vfx/superview_stretch"), "vfx/superview_stretch preset must be present");
        let shader_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("presets").join("shaders").join("superview.glsl");
        assert!(shader_path.is_file(), "superview.glsl must exist in presets/shaders");
        for item in items {
            let preset = load(&dir, &item.id).unwrap_or_else(|e| panic!("{}: {e}", item.id));
            let mut s = Settings::default();
            s.apply_json(&preset);
            let applied = s.to_json();
            for (key, value) in &preset {
                let expected = match value {
                    _ if matches!(key.as_str(), "version" | "ffmpeg_path" | "batch_output_folder" | "batch_same_folder") => continue,
                    Value::String(v) if v.is_empty() => continue,
                    Value::Number(n) => Value::String(n.to_string()),
                    v => v.clone(),
                };
                assert_eq!(applied.get(key), Some(&expected), "{}: {key}", item.id);
            }
        }
    }
}
