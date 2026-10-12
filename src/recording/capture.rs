//! Screen recording command lines and file names.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use crate::config::settings::Settings;
use crate::core::command;

/// ddagrab frame rate: the FPS setting, 60 for "source".
pub fn frame_rate(s: &Settings) -> &str {
    match s.fps_option.as_str() {
        "" | "source" => "60",
        "custom" => &s.custom_fps,
        fps => fps,
    }
}

/// `recording_command`: desktop capture with the NVENC settings and no audio. The
/// segment file is appended when each segment starts.
pub fn command(s: &Settings, ffmpeg: &str, additional_enabled: bool) -> Vec<String> {
    let mut cmd: Vec<String> = [
        ffmpeg, "-y", "-thread_queue_size", "4096", "-use_wallclock_as_timestamps", "1", "-f",
        "lavfi", "-i",
    ]
    .map(str::to_owned)
    .to_vec();
    cmd.push(format!("ddagrab=framerate={}", frame_rate(s)));
    cmd.extend(["-vf", "setparams=range=limited", "-c:v", s.codec.encoder()].map(str::to_owned));
    if s.encoder_preset != "auto" {
        cmd.extend(["-preset:v".to_owned(), s.encoder_preset.clone()]);
    }
    if s.constant_qp_mode {
        cmd.extend(["-rc:v", "constqp", "-qp:v", &s.quality_level].map(str::to_owned));
    } else {
        cmd.extend(["-rc:v".to_owned(), s.encoder_rc.clone(), "-b:v".to_owned(), format!("{}k", s.bitrate)]);
    }
    if additional_enabled {
        cmd.extend(command::split(s.additional_options.trim()));
    }
    cmd.extend(["-fps_mode", &s.fps_mode, "-an"].map(str::to_owned));
    cmd
}

/// The output's extension with its dot, `.mp4` if it has none (`recording_extension`).
pub fn extension(output: &Path) -> String {
    match output.extension() {
        Some(e) if !e.is_empty() => format!(".{}", e.to_string_lossy()),
        _ => ".mp4".to_owned(),
    }
}

/// `screen_record-<date>.mp4` on the desktop, used when no output file is set.
pub fn default_output(desktop: &Path, date: &str) -> PathBuf {
    desktop.join(format!("screen_record-{date}.mp4"))
}

/// Temp files of one recording, all named after `recording_base_filename`.
#[derive(Clone, Debug)]
pub struct Files {
    base: PathBuf,
}

impl Files {
    /// `<temp>\nvencffx_record_<date>_<pid>_<ns>`
    pub fn new(temp: &Path, date: &str) -> Self {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        Self { base: temp.join(format!("nvencffx_record_{date}_{}_{ns}", std::process::id())) }
    }

    fn with_suffix(&self, suffix: &str) -> PathBuf {
        let mut name = OsString::from(self.base.as_os_str());
        name.push(suffix);
        name.into()
    }

    /// Folder holding the files (`tempfile.gettempdir()`).
    pub fn dir(&self) -> &Path {
        self.base.parent().unwrap_or(Path::new(""))
    }

    /// Video segment `index` (from 1).
    pub fn part(&self, index: u32, extension: &str) -> PathBuf {
        self.with_suffix(&format!("_part{index}{extension}"))
    }

    pub fn audio(&self) -> PathBuf {
        self.with_suffix("_audio.wav")
    }

    pub fn concat_list(&self) -> PathBuf {
        self.with_suffix("_concat.txt")
    }

    pub fn joined(&self, extension: &str) -> PathBuf {
        self.with_suffix(&format!("_joined{extension}"))
    }
}

/// The concat demuxer's list: one `file '<path>'` line per segment, with forward
/// slashes and quotes escaped as `'\''`.
pub fn concat_list(parts: &[&Path]) -> String {
    parts
        .iter()
        .map(|part| {
            let path = std::path::absolute(part).unwrap_or_else(|_| part.to_path_buf());
            let path = path.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
            format!("file '{path}'\n")
        })
        .collect()
}

/// Joins the segments without re-encoding.
pub fn concat_command(ffmpeg: &str, list: &Path, output: &Path) -> Vec<String> {
    let [list, output] = [list, output].map(|p| p.to_string_lossy().into_owned());
    [ffmpeg, "-y", "-f", "concat", "-safe", "0", "-i", &list, "-c", "copy", &output]
        .map(str::to_owned)
        .to_vec()
}

/// Video copied, the desktop audio encoded with the Audio Settings (`audio`, from
/// [`crate::core::command::append_audio`]).
pub fn mux_command(ffmpeg: &str, video: &Path, wav: &Path, audio: &[String], output: &Path) -> Vec<String> {
    let [video, wav, output] = [video, wav, output].map(|p| p.to_string_lossy().into_owned());
    let mut cmd: Vec<String> = [ffmpeg, "-y", "-i", &video, "-i", &wav, "-c:v", "copy"].map(str::to_owned).to_vec();
    cmd.extend_from_slice(audio);
    cmd.extend(["-map", "0:v:0", "-map", "1:a:0", &output].map(str::to_owned));
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::VideoCodec;

    #[test]
    fn capture_command() {
        let s = Settings::default();
        let cmd = command(&s, "ffmpeg.exe", false).join(" ");
        assert_eq!(
            cmd,
            "ffmpeg.exe -y -thread_queue_size 4096 -use_wallclock_as_timestamps 1 -f lavfi \
             -i ddagrab=framerate=60 -vf setparams=range=limited -c:v hevc_nvenc -preset:v p7 \
             -rc:v constqp -qp:v 30 -fps_mode auto -an"
        );

        let s = Settings {
            codec: VideoCodec::Av1,
            encoder_preset: "auto".into(),
            constant_qp_mode: false,
            encoder_rc: "cbr".into(),
            bitrate: "8000".into(),
            fps_option: "custom".into(),
            custom_fps: "120".into(),
            fps_mode: "cfr".into(),
            additional_options: " -bf 0 -metadata title=\"a b\" ".into(),
            ..Default::default()
        };
        let cmd = command(&s, "ffmpeg", true);
        assert_eq!(cmd[9], "ddagrab=framerate=120");
        let tail = cmd[12..].join("|");
        assert_eq!(tail, "-c:v|av1_nvenc|-rc:v|cbr|-b:v|8000k|-bf|0|-metadata|title=a b|-fps_mode|cfr|-an");
        assert!(!command(&s, "ffmpeg", false).contains(&"-bf".to_owned()), "options only while the section is open");

        let s = Settings { fps_option: "24000/1001".into(), ..Default::default() };
        assert_eq!(frame_rate(&s), "24000/1001");
    }

    #[test]
    fn file_names_share_the_session_base() {
        let files = Files::new(Path::new(r"C:\Temp"), "05_03_26-14_07_09");
        let part = files.part(2, ".mkv").to_string_lossy().into_owned();
        let prefix = format!(r"C:\Temp\nvencffx_record_05_03_26-14_07_09_{}_", std::process::id());
        assert!(part.starts_with(&prefix) && part.ends_with("_part2.mkv"), "{part}");
        assert!(files.audio().to_string_lossy().ends_with("_audio.wav"));
        assert!(files.joined(".mp4").to_string_lossy().ends_with("_joined.mp4"));
        assert_eq!(files.dir(), Path::new(r"C:\Temp"));

        assert_eq!(extension(Path::new(r"C:\v\out.MKV")), ".MKV");
        assert_eq!(extension(Path::new(r"C:\v\out")), ".mp4");
        assert_eq!(default_output(Path::new(r"C:\Desk"), "01_02_26-03_04_05"), Path::new(r"C:\Desk\screen_record-01_02_26-03_04_05.mp4"));
    }

    #[test]
    fn concat_list_and_mux_command() {
        let parts = [Path::new(r"C:\Temp\a_part1.mp4"), Path::new(r"C:\Temp\it's_part2.mp4")];
        assert_eq!(concat_list(&parts), "file 'C:/Temp/a_part1.mp4'\nfile 'C:/Temp/it'\\''s_part2.mp4'\n");

        let audio = ["-c:a", "aac", "-b:a", "96k"].map(str::to_owned);
        let cmd = mux_command("ffmpeg", Path::new("v.mp4"), Path::new("a.wav"), &audio, Path::new("out.mp4"));
        assert_eq!(cmd.join(" "), "ffmpeg -y -i v.mp4 -i a.wav -c:v copy -c:a aac -b:a 96k -map 0:v:0 -map 1:a:0 out.mp4");
        let cmd = concat_command("ffmpeg", Path::new("l.txt"), Path::new("j.mp4"));
        assert_eq!(cmd.join(" "), "ffmpeg -y -f concat -safe 0 -i l.txt -c copy j.mp4");
    }
}
