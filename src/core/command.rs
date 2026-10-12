//! FFmpeg argument builder and the parsing / quoting used by the editable command
//! preview (custom command mode).

use std::path::{Path, PathBuf};
use crate::config::settings::{Settings, VideoCodec};

/// Inputs to the builder besides the persisted settings.
pub struct Job<'a> {
    pub ffmpeg: &'a str,
    pub input: &'a str,
    pub output: &'a str,
    /// The "Additional Options (Trimming)" section is expanded. In precise-trim mode
    /// its FF options only apply while it is.
    pub additional_enabled: bool,
}

/// Splits `-ss X` / `-to X` out of the additional options (fast seek before `-i`).
fn split_trim(options: &str) -> (Vec<String>, Vec<String>) {
    let parts: Vec<&str> = options.split_whitespace().collect();
    let (mut trim, mut other) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < parts.len() {
        if matches!(parts[i], "-ss" | "-to") && i + 1 < parts.len() {
            trim.extend([parts[i].to_owned(), parts[i + 1].to_owned()]);
            i += 2;
        } else {
            other.push(parts[i].to_owned());
            i += 1;
        }
    }
    (trim, other)
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn extend(cmd: &mut Vec<String>, args: &[&str]) {
    cmd.extend(args.iter().map(|a| (*a).to_owned()));
}

fn is_mkv(output: &str) -> bool {
    std::path::Path::new(output)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mkv"))
}

/// Stream mapping + forced `mov_text` subtitles for non-MKV outputs.
fn append_mapping(cmd: &mut Vec<String>, output: &str, other: &[String]) {
    let mkv = is_mkv(output);
    if !other.iter().any(|a| a == "-map") {
        if mkv {
            extend(cmd, &["-map", "0", "-ignore_unknown"]);
        } else {
            extend(cmd, &[
                "-map", "0:v?", "-map", "0:a?", "-map", "0:s?",
                "-map_metadata", "0", "-map_chapters", "0", "-ignore_unknown",
            ]);
        }
    }
    let has_sub_codec = |args: &[String]| args.iter().any(|a| a == "-c:s" || a == "-scodec");
    if !mkv && !has_sub_codec(other) && !has_sub_codec(cmd) {
        extend(cmd, &["-c:s", "mov_text"]);
    }
}

pub fn append_audio(cmd: &mut Vec<String>, s: &Settings) -> Result<(), String> {
    match s.audio_option.as_str() {
        "disable" => extend(cmd, &["-an"]),
        "copy" => extend(cmd, &["-c:a", "copy"]),
        "aac_96k" => extend(cmd, &["-c:a", "aac", "-b:a", "96k"]),
        "aac_160k" => extend(cmd, &["-c:a", "aac", "-b:a", "160k"]),
        "aac_256k" => extend(cmd, &["-c:a", "aac", "-b:a", "256k"]),
        "opus_96k" => extend(cmd, &["-c:a", "libopus", "-b:a", "96k"]),
        "opus_160k" => extend(cmd, &["-c:a", "libopus", "-b:a", "160k"]),
        "opus_256k" => extend(cmd, &["-c:a", "libopus", "-b:a", "256k"]),
        "custom" => {
            let kbps: i64 = s
                .custom_abitrate
                .trim()
                .parse()
                .map_err(|_| "Custom audio bitrate must be a number.".to_owned())?;
            cmd.extend(["-c:a".into(), "aac".into(), "-b:a".into(), format!("{kbps}k")]);
        }
        // Unknown values (hand-edited presets) add nothing
        _ => {}
    }
    Ok(())
}

/// `_build_ffmpeg_command(preview=False)` for the GUI settings.
pub fn build(s: &Settings, job: &Job<'_>) -> Result<Vec<String>, String> {
    if job.ffmpeg.is_empty() {
        return Err("FFmpeg path is not specified".into());
    }
    if job.output.is_empty() {
        return Err("Please set an output file".into());
    }
    if job.input.is_empty() {
        return Err("Please fill in all main fields.".into());
    }
    let (input, output) = (job.input, job.output);
    let mut cmd = vec![job.ffmpeg.to_owned(), "-hide_banner".to_owned()];
    let add_opts = s.additional_options.trim();

    if s.trim_streamcopy {
        let (trim, other) = if s.precise_trim { (Vec::new(), words(add_opts)) } else { split_trim(add_opts) };
        cmd.extend(trim);
        extend(&mut cmd, &["-y", "-i", input]);
        append_mapping(&mut cmd, output, &other);
        cmd.extend(other);
        extend(&mut cmd, &["-c:v", "copy"]);
        append_audio(&mut cmd, s)?;
        cmd.push(output.to_owned());
        return Ok(cmd);
    }

    let (trim, other) = if !s.precise_trim {
        split_trim(add_opts)
    } else if job.additional_enabled {
        (Vec::new(), words(add_opts))
    } else {
        (Vec::new(), Vec::new())
    };
    cmd.extend(trim);

    if s.encoder_hwaccel != "auto" {
        extend(&mut cmd, &["-hwaccel:v", &s.encoder_hwaccel]);
        if s.encoder_hwaccel == "cuda" && s.cuda_output_format {
            extend(&mut cmd, &["-hwaccel_output_format:v", "cuda"]);
        }
    }
    if s.encoder_threads != "auto" {
        extend(&mut cmd, &["-threads", &s.encoder_threads]);
    }
    extend(&mut cmd, &["-y", "-i", input]);
    append_mapping(&mut cmd, output, &other);

    let quality = s.quality_level.trim();
    let bitrate = s.bitrate.trim();
    let mut rate = Vec::new();
    if s.constant_qp_mode {
        match quality.parse::<i64>() {
            Ok(0..=51) => {}
            _ => return Err("Quality level must be a number between 0 and 51".into()),
        }
        rate.extend(["-rc:v".into(), "constqp".into(), "-qp:v".into(), quality.to_owned()]);
    } else {
        // A bitrate too large for i64 maths is rejected
        // with the same message as a non-number
        let (maxrate, bufsize) = bitrate
            .parse::<i64>()
            .ok()
            .and_then(|kbps| kbps.checked_mul(12)?.checked_add(9))
            .map(|v| v.div_euclid(10))
            .and_then(|maxrate| Some((maxrate, maxrate.checked_mul(2)?)))
            .ok_or_else(|| "Video bitrate must be a number.".to_owned())?;
        rate.extend([
            "-rc:v".into(), s.encoder_rc.clone(),
            "-b:v".into(), format!("{bitrate}k"),
            "-maxrate:v".into(), format!("{maxrate}k"),
            "-bufsize:v".into(), format!("{bufsize}k"),
        ]);
    }

    let mut vf = Vec::new();
    let fps = if s.fps_option == "custom" {
        if s.custom_fps.is_empty() {
            return Err("Please specify custom FPS.".into());
        }
        s.custom_fps.as_str()
    } else {
        s.fps_option.as_str()
    };
    if fps != "source" {
        vf.push(format!("fps={fps}"));
    }
    let width = if s.video_format_option == "custom" {
        if s.custom_video_width.is_empty() {
            return Err("Please specify custom video width.".into());
        }
        s.custom_video_width.as_str()
    } else {
        s.video_format_option.as_str()
    };
    if width != "source" {
        vf.push(format!("scale={width}:-2:flags={}", s.interpolation_algo));
    }
    let extra_vf = s.additional_filter_options.trim();
    if !extra_vf.is_empty() {
        vf.push(extra_vf.to_owned());
    }
    if !vf.is_empty() && !other.iter().any(|a| a == "-filter_complex") {
        extend(&mut cmd, &["-vf", &vf.join(",")]);
    }
    if s.fps_mode != "auto" {
        extend(&mut cmd, &["-fps_mode", &s.fps_mode]);
    }

    let codec = s.codec;
    extend(&mut cmd, &["-c:v", "copy", "-c:v:0", codec.encoder()]);
    let mut opt = |flag: &str, value: &str| {
        if value != "auto" {
            extend(&mut cmd, &[flag, value]);
        }
    };
    opt("-preset:v", &s.encoder_preset);
    opt("-tune:v", &s.encoder_tune);
    if codec != VideoCodec::Av1 {
        opt("-profile:v", &s.encoder_profile);
    }
    opt("-level:v", &s.encoder_level);
    if codec == VideoCodec::H264 {
        opt("-coder:v", &s.encoder_coder);
    } else {
        opt("-tier:v", &s.encoder_tier);
    }
    opt("-multipass:v", &s.encoder_multipass);
    opt("-lookahead_level:v", &s.encoder_lookahead_level);
    if codec != VideoCodec::H264 {
        opt("-split_encode_mode:v", &s.encoder_split_encode_mode);
    }
    for (on, flag) in [
        (s.encoder_spatial_aq, "-spatial-aq:v"),
        (s.encoder_temporal_aq, "-temporal-aq:v"),
        (s.encoder_strict_gop, "-strict_gop:v"),
        (s.encoder_no_scenecut, "-no-scenecut:v"),
    ] {
        if on {
            extend(&mut cmd, &[flag, "1"]);
        }
    }
    if s.encoder_weighted_pred {
        extend(&mut cmd, &["-weighted_pred:v", "1", "-bf", "0"]);
    }
    cmd.extend(rate);
    cmd.extend(other);

    let extra_af = s.additional_audio_filter_options.trim();
    if !extra_af.is_empty() {
        extend(&mut cmd, &["-af", extra_af]);
    }
    append_audio(&mut cmd, s)?;
    cmd.push(output.to_owned());

    // Aim encoder options at the first video stream only, so copied streams such
    // as cover art are left alone.
    const V0_WITH_V: &[&str] = &[
        "-preset:v", "-tune:v", "-profile:v", "-level:v", "-tier:v", "-coder:v",
        "-multipass:v", "-lookahead_level:v", "-split_encode_mode:v", "-spatial-aq:v",
        "-temporal-aq:v", "-strict_gop:v", "-no-scenecut:v", "-weighted_pred:v",
        "-rc:v", "-qp:v", "-b:v", "-maxrate:v", "-bufsize:v",
    ];
    for arg in &mut cmd {
        if V0_WITH_V.contains(&arg.as_str()) {
            arg.push_str(":0");
        } else if arg == "-bf" {
            arg.push_str(":v:0");
        }
    }
    Ok(cmd)
}

/// The command shown in the preview: a custom command gets the current input and
/// output swapped in (`_build_ffmpeg_command(preview=True)`).
pub fn preview(custom: Option<&[String]>, s: &Settings, job: &Job<'_>) -> Result<Vec<String>, String> {
    let Some(custom) = custom else { return build(s, job) };
    let mut cmd = custom.to_vec();
    if let Some(i) = cmd.iter().position(|a| a == "-i")
        && i + 1 < cmd.len()
    {
        cmd[i + 1] = job.input.to_owned();
    }
    if let Some(last) = cmd.last_mut() {
        *last = job.output.to_owned();
    }
    Ok(cmd)
}

/// The commands behind "Play 10s Preview": 10 seconds from the middle of the input are stream-copied to `clip`, which is then encoded to
/// `encoded` with the current settings (or custom command) minus any trimming.
pub struct PreviewClip {
    pub copy: Vec<String>,
    pub encode: Vec<String>,
    pub clip: PathBuf,
    pub encoded: PathBuf,
}

pub const PREVIEW_SECONDS: f64 = 10.0;

pub fn preview_clip(
    custom: Option<&[String]>,
    s: &Settings,
    job: &Job<'_>,
    duration: f64,
    temp_dir: &Path,
) -> Result<PreviewClip, String> {
    let input = Path::new(job.input);
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let ext = |p: &Path| p.extension().map(|e| format!(".{}", e.to_string_lossy()));
    let input_ext = ext(input).unwrap_or_else(|| ".mp4".to_owned());
    let output_ext = ext(Path::new(job.output)).unwrap_or_else(|| input_ext.clone());
    let clip = temp_dir.join(format!("{stem}_streamcopy10s{input_ext}"));
    let encoded = temp_dir.join(format!("{stem}_encoded10s{output_ext}"));
    let (clip_str, encoded_str) = (clip.to_string_lossy(), encoded.to_string_lossy());

    let mid = ((duration - PREVIEW_SECONDS) / 2.0).max(0.0).to_string();
    let copy = [
        job.ffmpeg, "-ss", &mid, "-i", job.input, "-map", "0", "-ignore_unknown",
        "-t", "10", "-c", "copy", "-y", &clip_str,
    ]
    .map(str::to_owned)
    .to_vec();

    let clip_job = Job { input: &clip_str, output: &encoded_str, ..*job };
    let mut encode = without_trim(preview(custom, s, &clip_job)?);
    // Keep every stream of the clip unless the user maps streams themselves
    if !s.additional_options.contains("-map")
        && let Some(i) = encode.iter().position(|a| a == "-i")
        && encode.get(i + 2).is_some_and(|a| a != "-map")
    {
        encode.splice(i + 2..i + 2, ["-map", "0", "-ignore_unknown"].map(str::to_owned));
    }
    Ok(PreviewClip { copy, encode, clip, encoded })
}

/// Drops `-ss X` / `-to X` pairs.
fn without_trim(args: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "-ss" || arg == "-to" {
            args.next();
        } else {
            out.push(arg);
        }
    }
    out
}

/// Joins arguments for display, quoting the ones with spaces so the text can be
/// split back losslessly by [`split`].
pub fn join(args: &[String]) -> String {
    args.iter()
        .map(|a| if a.is_empty() || a.contains(char::is_whitespace) { format!("\"{a}\"") } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_like_path(arg: &str) -> bool {
    (arg.contains(':') && arg.contains('\\')) || arg.starts_with("\\\\")
}

/// Splits an edited command line. Double quotes group words and are removed. An
/// unquoted Windows path that was split at its spaces is glued back together up to
/// the next `-option`.
pub fn split(text: &str) -> Vec<String> {
    let mut args: Vec<(String, bool)> = Vec::new();
    let (mut cur, mut quoted, mut in_quotes, mut started) = (String::new(), false, false, false);
    for ch in text.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                quoted = true;
                started = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if started {
                    args.push((std::mem::take(&mut cur), quoted));
                }
                (quoted, started) = (false, false);
            }
            c => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push((cur, quoted));
    }

    let mut out = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        let (arg, quoted) = &args[i];
        let mut j = i + 1;
        if !quoted && looks_like_path(arg) {
            while j < args.len() && !args[j].1 && !args[j].0.starts_with('-') {
                j += 1;
            }
        }
        out.push(args[i..j].iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>().join(" "));
        i = j;
    }
    out
}

/// Validates an edited command: an input after `-i` and an output file
/// (the last argument that is not an option value).
pub fn parse_custom(text: &str) -> Result<Vec<String>, String> {
    let args = split(text);
    if args.len() < 3 {
        return Err("Command too short - not a valid FFmpeg command".into());
    }
    let input = args.iter().position(|a| a == "-i").and_then(|i| args.get(i + 1));
    let mut output = None;
    let mut skip_next = false;
    for arg in &args[1..] {
        if std::mem::take(&mut skip_next) {
            continue;
        }
        if arg.starts_with('-') {
            skip_next = true;
        } else {
            output = Some(arg);
        }
    }
    if input.is_none() {
        return Err("Missing input file (-i option)".into());
    }
    if output.is_none() {
        return Err("Missing output file".into());
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job<'a>(output: &'a str) -> Job<'a> {
        Job { ffmpeg: "ffmpeg.exe", input: "C:\\in put.mp4", output, additional_enabled: false }
    }

    #[test]
    fn default_settings_build_the_default_command() {
        let cmd = build(&Settings::default(), &job("out.mp4")).unwrap().join(" ");
        assert_eq!(
            cmd,
            "ffmpeg.exe -hide_banner -hwaccel:v cuda -threads 4 -y -i C:\\in put.mp4 \
             -map 0:v? -map 0:a? -map 0:s? -map_metadata 0 -map_chapters 0 -ignore_unknown \
             -c:s mov_text -c:v copy -c:v:0 hevc_nvenc -preset:v:0 p7 -tune:v:0 hq \
             -profile:v:0 main -tier:v:0 1 -multipass:v:0 qres -split_encode_mode:v:0 forced \
             -spatial-aq:v:0 1 -temporal-aq:v:0 1 -rc:v:0 constqp -qp:v:0 30 -c:a copy out.mp4"
        );
    }

    #[test]
    fn bitrate_mode_trim_and_filters() {
        let s = Settings {
            codec: VideoCodec::H264,
            constant_qp_mode: false,
            bitrate: "5000".into(),
            encoder_weighted_pred: true,
            video_format_option: "1280".into(),
            additional_options: "-ss 00:00:05 -to 00:00:10 -ac 2".into(),
            additional_filter_options: "hflip".into(),
            audio_option: "opus_96k".into(),
            ..Default::default()
        };
        let cmd = build(&s, &job("out.mkv")).unwrap().join(" ");
        assert!(cmd.starts_with("ffmpeg.exe -hide_banner -ss 00:00:05 -to 00:00:10 -hwaccel:v cuda"));
        assert!(cmd.contains("-map 0 -ignore_unknown -vf scale=1280:-2:flags=bicubic,hflip"));
        assert!(!cmd.contains("mov_text"));
        assert!(cmd.contains("-coder:v:0 cabac"));
        assert!(!cmd.contains("split_encode"));
        assert!(cmd.contains("-weighted_pred:v:0 1 -bf:v:0 0"));
        assert!(cmd.contains("-rc:v:0 vbr -b:v:0 5000k -maxrate:v:0 6000k -bufsize:v:0 12000k -ac 2"));
        assert!(cmd.ends_with("-c:a libopus -b:a 96k out.mkv"));
    }

    #[test]
    fn streamcopy_and_errors() {
        let s = Settings {
            trim_streamcopy: true,
            additional_options: "-ss 1 -to 2".into(),
            ..Default::default()
        };
        let cmd = build(&s, &job("o.mp4")).unwrap().join(" ");
        assert!(cmd.starts_with("ffmpeg.exe -hide_banner -ss 1 -to 2 -y -i"));
        assert!(cmd.ends_with("-c:s mov_text -c:v copy -c:a copy o.mp4"));

        let s = Settings { quality_level: "60".into(), ..Default::default() };
        assert!(build(&s, &job("o.mp4")).is_err());
        assert!(build(&Settings::default(), &job("")).is_err());
    }

    #[test]
    fn huge_bitrate_is_rejected_not_overflowed() {
        let rate = |bitrate: &str| {
            let s = Settings { constant_qp_mode: false, bitrate: bitrate.into(), ..Default::default() };
            build(&s, &job("o.mp4")).map(|cmd| cmd.join(" "))
        };
        let max_ok = (i64::MAX - 9) / 12;
        let cmd = rate(&max_ok.to_string()).unwrap();
        let maxrate = (max_ok * 12 + 9) / 10;
        assert!(cmd.contains(&format!("-maxrate:v:0 {maxrate}k -bufsize:v:0 {}k", maxrate * 2)), "{cmd}");
        for bad in [(max_ok + 1).to_string(), "999999999999999999".into(), i64::MAX.to_string(), "1".repeat(30)] {
            assert_eq!(rate(&bad).unwrap_err(), "Video bitrate must be a number.", "{bad}");
        }
    }

    #[test]
    fn audio_options_known_values_only() {
        let audio = |opt: &str| {
            let s = Settings { audio_option: opt.into(), custom_abitrate: "128".into(), ..Default::default() };
            let mut cmd = Vec::new();
            append_audio(&mut cmd, &s).map(|()| cmd.join(" "))
        };
        assert_eq!(audio("disable").unwrap(), "-an");
        assert_eq!(audio("copy").unwrap(), "-c:a copy");
        assert_eq!(audio("aac_96k").unwrap(), "-c:a aac -b:a 96k");
        assert_eq!(audio("aac_160k").unwrap(), "-c:a aac -b:a 160k");
        assert_eq!(audio("aac_256k").unwrap(), "-c:a aac -b:a 256k");
        assert_eq!(audio("opus_96k").unwrap(), "-c:a libopus -b:a 96k");
        assert_eq!(audio("opus_160k").unwrap(), "-c:a libopus -b:a 160k");
        assert_eq!(audio("opus_256k").unwrap(), "-c:a libopus -b:a 256k");
        assert_eq!(audio("custom").unwrap(), "-c:a aac -b:a 128k");
        for unknown in ["mp3_320k", "x_y", "aac_128k", "", "AAC_96K"] {
            assert_eq!(audio(unknown).unwrap(), "", "{unknown}");
        }
    }

    #[test]
    fn split_handles_quotes_and_bare_paths() {
        let args = split(r#"ffmpeg -i C:\My Videos\a b.mp4 -vf "scale=1280:-2" "D:\out dir\o.mp4""#);
        assert_eq!(args, ["ffmpeg", "-i", r"C:\My Videos\a b.mp4", "-vf", "scale=1280:-2", r"D:\out dir\o.mp4"]);
        assert_eq!(split(&join(&args)), args);
        assert_eq!(split(r#"x -metadata title="a b" o"#), ["x", "-metadata", "title=a b", "o"]);
    }

    #[test]
    fn custom_command_validation() {
        assert!(parse_custom("ffmpeg -i in.mp4 out.mp4").is_ok());
        assert_eq!(parse_custom("ffmpeg out.mp4 x").unwrap_err(), "Missing input file (-i option)");
        assert_eq!(parse_custom("ffmpeg -i").unwrap_err(), "Command too short - not a valid FFmpeg command");
        assert_eq!(parse_custom("ffmpeg -i in.mp4 -y").unwrap_err(), "Missing output file");
    }

    #[test]
    fn preview_clip_cuts_the_middle_and_drops_trimming() {
        let s = Settings { additional_options: "-ss 00:00:05 -to 00:00:20".into(), ..Default::default() };
        let temp = Path::new(r"T:\tmp");
        let clip = preview_clip(None, &s, &job("out.mkv"), 70.0, temp).unwrap();
        assert_eq!(clip.clip, temp.join("in put_streamcopy10s.mp4"));
        assert_eq!(clip.encoded, temp.join("in put_encoded10s.mkv"));
        assert_eq!(
            join(&clip.copy),
            r#"ffmpeg.exe -ss 30 -i "C:\in put.mp4" -map 0 -ignore_unknown -t 10 -c copy -y "T:\tmp\in put_streamcopy10s.mp4""#
        );
        let encode = join(&clip.encode);
        assert!(encode.starts_with(r#"ffmpeg.exe -hide_banner -hwaccel:v cuda -threads 4 -y -i "T:\tmp\in put_streamcopy10s.mp4" -map 0 -ignore_unknown -c:v copy"#), "{encode}");
        assert!(!encode.contains("-ss") && !encode.contains("-to"));
        assert!(encode.ends_with(r#""T:\tmp\in put_encoded10s.mkv""#));

        // A custom command gets the clip swapped in and `-map 0` after its input
        let custom = split("ffmpeg -ss 1 -i a.mp4 -c:v hevc_nvenc b.mp4");
        let clip = preview_clip(Some(&custom), &Settings::default(), &job(""), 4.0, temp).unwrap();
        assert_eq!(clip.copy[2], "0", "a clip shorter than 10 s starts at 0");
        assert_eq!(
            join(&clip.encode),
            r#"ffmpeg -i "T:\tmp\in put_streamcopy10s.mp4" -map 0 -ignore_unknown -c:v hevc_nvenc "T:\tmp\in put_encoded10s.mp4""#
        );
    }

    #[test]
    fn preview_swaps_input_and_output_of_a_custom_command() {
        let custom = split("ffmpeg -i a.mp4 -c copy b.mp4");
        let cmd = preview(Some(&custom), &Settings::default(), &job("new.mp4")).unwrap();
        assert_eq!(join(&cmd), r#"ffmpeg -i "C:\in put.mp4" -c copy new.mp4"#);
    }
}
