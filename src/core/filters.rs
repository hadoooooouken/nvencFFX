//! Quick buttons of the Additional Options section.

use crate::config::settings::Settings;

/// Appends `filter` to a comma-separated chain unless it is already there.
fn add_to_chain(chain: &mut String, filter: &str) {
    let mut parts: Vec<&str> = chain.split(',').map(str::trim).filter(|f| !f.is_empty()).collect();
    if parts.contains(&filter) {
        return;
    }
    parts.push(filter);
    *chain = parts.join(",");
}

pub fn add_video_filter(s: &mut Settings, filter: &str) {
    add_to_chain(&mut s.additional_filter_options, filter);
}

/// Audio filters need re-encoding, so a copied / disabled track switches to AAC 160.
pub fn add_audio_filter(s: &mut Settings, filter: &str) {
    if matches!(s.audio_option.as_str(), "disable" | "copy") {
        s.audio_option = "aac_160k".to_owned();
    }
    add_to_chain(&mut s.additional_audio_filter_options, filter);
}

/// Sets `-name value`, replacing an existing value of the same option.
pub fn add_option(s: &mut Settings, option: &str) {
    let new: Vec<&str> = option.split_whitespace().collect();
    let mut words: Vec<&str> = s.additional_options.split_whitespace().collect();
    match words.iter().position(|w| Some(w) == new.first()) {
        Some(i) => {
            let end = (i + 2).min(words.len());
            words.splice(i..end, new);
        }
        None => words.extend(new),
    }
    s.additional_options = words.join(" ");
}

/// "Speed up X2" / "Slow down X2": replaces both filter fields.
pub fn set_speed(s: &mut Settings, speed: f64) {
    s.additional_filter_options = format!("setpts={}*PTS", py_float(1.0 / speed));
    if matches!(s.audio_option.as_str(), "disable" | "copy") {
        s.audio_option = "aac_96k".to_owned();
    }
    s.additional_audio_filter_options = format!("atempo={}", py_float(speed));
}

/// Always at least one decimal (`2.0`, `0.5`).
fn py_float(v: f64) -> String {
    if v.fract() == 0.0 { format!("{v:.1}") } else { v.to_string() }
}

pub fn set_stereo_out(s: &mut Settings) {
    add_option(s, "-ac 2");
    s.audio_option = "aac_160k".to_owned();
}

pub fn clear(s: &mut Settings) {
    s.additional_options.clear();
    s.additional_filter_options.clear();
    s.additional_audio_filter_options.clear();
}

pub enum Quick {
    Video(&'static str),
    Audio(&'static str),
    Speed(f64),
    StereoOut,
    Clear,
    Streams,
}

/// The three rows of quick buttons, left to right.
pub const QUICK_BUTTONS: [&[(&str, Quick)]; 3] = [
    &[
        ("Brightness", Quick::Video("eq=brightness=-0.15")),
        ("Sharpness", Quick::Video("unsharp=5:5:1.0:3:3:0.0")),
        ("H-Flip", Quick::Video("hflip")),
        ("Speed up X2", Quick::Speed(2.0)),
        ("Audio fix", Quick::Audio("loudnorm=I=-16:TP=-1.5:LRA=11")),
        ("Clear", Quick::Clear),
    ],
    &[
        ("Saturation", Quick::Video("eq=saturation=1.15")),
        ("Denoise", Quick::Video("hqdn3d=2:1.5:3:2.25")),
        ("V-Flip", Quick::Video("vflip")),
        ("Slow down X2", Quick::Speed(0.5)),
        ("Stereo out", Quick::StereoOut),
        ("Streams", Quick::Streams),
    ],
    &[
        ("Gamma RGB", Quick::Video("eq=gamma_r=1.0:gamma_g=1.0:gamma_b=1.0:gamma_weight=1.0")),
        ("Deshake", Quick::Video("deshake=rx=32:ry=32:edge=3:blocksize=32:contrast=200:search=0")),
        ("Rotate 90°", Quick::Video("transpose=1")),
        ("Crop to 16:9", Quick::Video("crop=iw:min(ih\\,iw*9/16):0:(ih-min(ih\\,iw*9/16))/2")),
        (
            "HDR to SDR",
            Quick::Video(
                "zscale=transfer=linear:npl=100,tonemap=tonemap=hable:desat=0,\
                 zscale=transfer=bt709:matrix=bt709:primaries=bt709",
            ),
        ),
    ],
];

/// Applies a quick button; `Streams` is handled by the caller.
pub fn apply(s: &mut Settings, quick: &Quick) {
    match quick {
        Quick::Video(f) => add_video_filter(s, f),
        Quick::Audio(f) => add_audio_filter(s, f),
        Quick::Speed(v) => set_speed(s, *v),
        Quick::StereoOut => set_stereo_out(s),
        Quick::Clear => clear(s),
        Quick::Streams => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_are_appended_once() {
        let mut s = Settings::default();
        add_video_filter(&mut s, "hflip");
        add_video_filter(&mut s, "vflip");
        add_video_filter(&mut s, "hflip");
        assert_eq!(s.additional_filter_options, "hflip,vflip");
        add_audio_filter(&mut s, "loudnorm");
        assert_eq!(s.audio_option, "aac_160k");
    }

    #[test]
    fn speed_and_options() {
        let mut s = Settings::default();
        set_speed(&mut s, 2.0);
        assert_eq!(s.additional_filter_options, "setpts=0.5*PTS");
        assert_eq!(s.additional_audio_filter_options, "atempo=2.0");
        assert_eq!(s.audio_option, "aac_96k");
        set_speed(&mut s, 0.5);
        assert_eq!(s.additional_filter_options, "setpts=2.0*PTS");

        s.additional_options = "-ac 6 -y".into();
        set_stereo_out(&mut s);
        assert_eq!(s.additional_options, "-ac 2 -y");
    }
}
