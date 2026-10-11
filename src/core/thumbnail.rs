//! Frame previews above the trim slider (`_show_thumbnail_preview`).

use std::path::Path;
use std::time::Duration;
use super::probe::{hidden_command, output_with_timeout};

/// Width of the preview (`352x198` box); the height follows the video.
pub const WIDTH: usize = 352;
/// Each attempt is given up after this long.
const TIMEOUT: Duration = Duration::from_secs(5);

/// One decoded frame, RGBA.
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// The attempts in order: CUDA decode with an 8-bit download, then 10-bit, then
/// the CPU. ffmpeg converts the frame to raw RGBA, so no image decoder is needed
/// (the original piped an MJPEG to Pillow).
pub fn commands(ffmpeg: &str, input: &str, seconds: f64) -> [Vec<String>; 3] {
    let at = format!("{seconds:.3}");
    let cuda = |format: &str| {
        let vf = format!("scale_cuda={WIDTH}:-2:interp_algo=bilinear,hwdownload,format={format}");
        [
            ffmpeg, "-threads", "1", "-hwaccel", "cuda", "-hwaccel_output_format", "cuda", "-ss", &at, "-i", input,
            "-vframes", "1", "-vf", &vf, "-an", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1",
        ]
        .map(str::to_owned)
        .to_vec()
    };
    let scale = format!("scale={WIDTH}:-1");
    let cpu = [
        ffmpeg, "-ss", &at, "-i", input, "-vframes", "1", "-vf", &scale, "-an", "-f", "rawvideo", "-pix_fmt", "rgba",
        "pipe:1",
    ]
    .map(str::to_owned)
    .to_vec();
    [cuda("nv12"), cuda("p010le"), cpu]
}

/// The frame at `seconds`, from the first attempt that delivers one.
pub fn grab(ffmpeg: &str, input: &str, seconds: f64) -> Option<Frame> {
    commands(ffmpeg, input, seconds).into_iter().find_map(|args| {
        let mut cmd = hidden_command(Path::new(&args[0]));
        cmd.args(&args[1..]);
        let out = output_with_timeout(cmd, TIMEOUT).ok()??;
        if out.status.success() { frame(out.stdout) } else { None }
    })
}

/// Whole RGBA rows of [`WIDTH`] pixels.
fn frame(rgba: Vec<u8>) -> Option<Frame> {
    let row = WIDTH * 4;
    (!rgba.is_empty() && rgba.len().is_multiple_of(row)).then(|| Frame { width: WIDTH, height: rgba.len() / row, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempts_and_frame_size() {
        let [nv12, p010, cpu] = commands("ffmpeg", "in.mkv", 12.0 + 1.0 / 3.0);
        assert_eq!(
            nv12.join(" "),
            "ffmpeg -threads 1 -hwaccel cuda -hwaccel_output_format cuda -ss 12.333 -i in.mkv -vframes 1 \
             -vf scale_cuda=352:-2:interp_algo=bilinear,hwdownload,format=nv12 -an -f rawvideo -pix_fmt rgba pipe:1"
        );
        assert!(p010.iter().any(|a| a.ends_with("format=p010le")));
        assert_eq!(cpu.join(" "), "ffmpeg -ss 12.333 -i in.mkv -vframes 1 -vf scale=352:-1 -an -f rawvideo -pix_fmt rgba pipe:1");

        let f = frame(vec![0; WIDTH * 4 * 198]).unwrap();
        assert_eq!((f.width, f.height), (352, 198));
        assert!(frame(vec![0; WIDTH * 4 + 3]).is_none());
        assert!(frame(Vec::new()).is_none());
    }

    /// A 4:3 test clip: a frame comes back (from CUDA or the CPU), and ffprobe reads
    /// the clip's size. Skipped without ffmpeg / ffprobe on `PATH`.
    #[test]
    fn frames_and_sizes_from_a_real_clip() {
        use crate::core::probe;
        let (Some(ffmpeg), Some(ffprobe)) = (probe::find_executable("ffmpeg.exe"), probe::find_executable("ffprobe.exe"))
        else {
            return;
        };
        let clip = std::env::temp_dir().join(format!("nvencffx-thumb-{}.mp4", std::process::id()));
        let made = hidden_command(&ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=duration=3:size=640x480:rate=25", "-c:v", "mpeg4"])
            .arg(&clip)
            .status()
            .unwrap();
        assert!(made.success());
        let clip_str = clip.to_string_lossy();
        assert_eq!(probe::resolution(&ffprobe, &clip_str), Some((640, 480)));
        let f = grab(&ffmpeg.to_string_lossy(), &clip_str, 1.5).expect("a frame");
        assert_eq!((f.width, f.height), (352, 264));
        assert_eq!(f.rgba.len(), 352 * 264 * 4);
        assert!(grab(&ffmpeg.to_string_lossy(), r"Z:\missing.mp4", 1.0).is_none());
        let _ = std::fs::remove_file(clip);
    }
}
