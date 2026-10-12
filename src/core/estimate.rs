//! Output size estimate shown under the sections.

/// Audio bitrate (kbit/s) counted for an audio option; 160 when unknown.
pub fn audio_kbps(option: &str, custom: &str) -> f64 {
    match option {
        "disable" => 0.0,
        "aac_96k" | "opus_96k" => 96.0,
        "aac_256k" | "opus_256k" => 256.0,
        "custom" => custom.trim().parse::<i64>().map_or(160.0, |v| v as f64),
        _ => 160.0,
    }
}

/// `(video + audio) kbit/s * seconds / 8 / 1024` in MB.
pub fn size_mb(video_kbps: f64, audio_kbps: f64, seconds: f64) -> f64 {
    (video_kbps + audio_kbps) * seconds / 8.0 / 1024.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_formula() {
        assert_eq!(audio_kbps("copy", ""), 160.0);
        assert_eq!(audio_kbps("opus_256k", ""), 256.0);
        assert_eq!(audio_kbps("custom", "320"), 320.0);
        assert_eq!(audio_kbps("custom", "x"), 160.0);
        assert_eq!(format!("{:.2}", size_mb(6000.0, 160.0, 60.0)), "45.12");
    }
}
