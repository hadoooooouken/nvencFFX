//! Output size estimate shown under the sections.

/// Constant bitrate of the MP3 320 option.
const MP3_320_KBPS: f64 = 320.0;
/// Typical FLAC / ALAC bitrate for 16-bit 48 kHz stereo (about half of raw PCM).
const LOSSLESS_KBPS: f64 = 800.0;
/// Raw 16-bit 48 kHz stereo PCM: 48000 Hz * 16 bit * 2 channels.
const PCM_S16_KBPS: f64 = 1536.0;

/// Audio bitrate (kbit/s) counted for an audio option; 160 when unknown.
pub fn audio_kbps(option: &str, custom: &str) -> f64 {
    match option {
        "disable" => 0.0,
        "aac_96k" | "opus_96k" => 96.0,
        "aac_256k" | "opus_256k" => 256.0,
        "mp3_320k" => MP3_320_KBPS,
        "flac" | "alac" => LOSSLESS_KBPS,
        "wav" => PCM_S16_KBPS,
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
        assert_eq!(audio_kbps("mp3_320k", ""), 320.0);
        assert_eq!(audio_kbps("flac", ""), 800.0);
        assert_eq!(audio_kbps("alac", ""), 800.0);
        assert_eq!(audio_kbps("wav", ""), 1536.0);
        assert_eq!(audio_kbps("custom", "320"), 320.0);
        assert_eq!(audio_kbps("custom", "x"), 160.0);
        assert_eq!(format!("{:.2}", size_mb(6000.0, 160.0, 60.0)), "45.12");
    }
}
