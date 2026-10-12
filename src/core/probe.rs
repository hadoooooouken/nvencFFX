//! Locating ffmpeg / ffprobe and probing media durations and stream info off the
//! UI thread.

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

/// Next to our executable first, then on `PATH`.
pub fn find_executable(name: &str) -> Option<PathBuf> {
    let beside_exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(name)));
    let on_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join(name));
    beside_exe.into_iter().chain(on_path).find(|p| p.is_file())
}

/// `ffprobe.exe` from the same folder as `ffmpeg`, else wherever it can be found.
pub fn ffprobe_for(ffmpeg: &str) -> Option<PathBuf> {
    let sibling = Path::new(ffmpeg).parent().map(|d| d.join("ffprobe.exe"));
    match sibling {
        Some(p) if !ffmpeg.is_empty() && p.is_file() => Some(p),
        _ => find_executable("ffprobe.exe"),
    }
}

/// A console-less child process (`CREATE_NO_WINDOW`).
pub fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Runs `cmd` with its output captured, killing it after `timeout` (`Ok(None)`).
pub fn output_with_timeout(mut cmd: Command, timeout: std::time::Duration) -> std::io::Result<Option<Output>> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    // Both pipes are drained while waiting, so a chatty child cannot block on them
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    Ok(Some(Output { status, stdout: stdout.join().unwrap_or_default(), stderr: stderr.join().unwrap_or_default() }))
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    })
}

/// Width and height of the first video stream.
pub fn resolution(ffprobe: &Path, file: &str) -> Option<(u32, u32)> {
    let mut cmd = hidden_command(ffprobe);
    cmd.args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0"])
        .arg(file);
    let out = output_with_timeout(cmd, std::time::Duration::from_secs(10)).ok()??;
    if !out.status.success() {
        return None;
    }
    parse_resolution(&String::from_utf8_lossy(&out.stdout))
}

/// `1920,1080` → (1920, 1080).
fn parse_resolution(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.trim().split_once(',')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// `ffprobe -show_entries format=duration`, in seconds.
pub fn duration(ffprobe: &Path, file: &str) -> Option<f64> {
    let out = hidden_command(ffprobe)
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(file)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok().filter(|d: &f64| d.is_finite())
}

/// `ffprobe -hide_banner <file>` for the input tooltip.
pub fn media_info(ffprobe: &Path, file: &str) -> String {
    match overview(ffprobe, file, Some(std::time::Duration::from_secs(10))) {
        Ok(Some(text)) => compact_info(&text),
        Ok(None) => "Timeout: ffprobe took too long".to_owned(),
        Err(e) => format!("Error reading metadata:\n{e}"),
    }
}

/// The stream overview of `ffprobe -hide_banner <file>`; `None` if it took longer
/// than `timeout`.
pub fn overview(ffprobe: &Path, file: &str, timeout: Option<std::time::Duration>) -> std::io::Result<Option<String>> {
    let mut cmd = hidden_command(ffprobe);
    cmd.arg("-hide_banner").arg(file);
    let out = match timeout {
        Some(timeout) => output_with_timeout(cmd, timeout)?,
        None => Some(cmd.stdin(Stdio::null()).output()?),
    };
    // ffprobe prints it to stderr
    Ok(out.map(|out| match String::from_utf8_lossy(&out.stderr).trim() {
        "" => String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        err => err.to_owned(),
    }))
}

/// Drops the `Metadata:` and `Chapters:` blocks to keep the tooltip compact.
fn compact_info(text: &str) -> String {
    let mut out = Vec::new();
    let mut skip_deeper_than: Option<usize> = None;
    for line in text.lines() {
        let stripped = line.trim();
        let indent = if stripped.is_empty() { 0 } else { line.len() - line.trim_start().len() };
        if let Some(block) = skip_deeper_than {
            if !stripped.is_empty() && indent > block {
                continue;
            }
            skip_deeper_than = None;
        }
        if stripped == "Metadata:" || stripped == "Chapters:" {
            skip_deeper_than = Some(indent);
            continue;
        }
        out.push(line);
    }
    out.join("
").trim().to_owned()
}

const INFO_CACHE_SIZE: usize = 50;

/// Tooltip texts of recently probed files, least recently used first.
pub struct InfoCache {
    entries: VecDeque<(String, Option<String>)>,
    tx: Sender<(String, String)>,
    rx: Receiver<(String, String)>,
}

impl Default for InfoCache {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { entries: VecDeque::new(), tx, rx }
    }
}

impl InfoCache {
    /// The tooltip text for `file`, starting a probe when it is not cached yet.
    pub fn get(&mut self, ffprobe: Option<&Path>, file: &str, ctx: &egui::Context) -> String {
        while let Ok((done, info)) = self.rx.try_recv() {
            match self.entries.iter_mut().find(|(f, _)| *f == done) {
                Some(entry) => entry.1 = Some(info),
                None => self.entries.push_back((done, Some(info))),
            }
        }
        self.trim(INFO_CACHE_SIZE);
        if let Some(i) = self.entries.iter().position(|(f, _)| f == file) {
            let entry = self.entries.remove(i).unwrap();
            let text = entry.1.clone().unwrap_or_else(|| "Loading metadata...".to_owned());
            self.entries.push_back(entry);
            return text;
        }
        let Some(ffprobe) = ffprobe else { return "ffprobe not found".to_owned() };
        self.trim(INFO_CACHE_SIZE - 1);
        self.entries.push_back((file.to_owned(), None));
        let (tx, ffprobe, file, ctx) = (self.tx.clone(), ffprobe.to_path_buf(), file.to_owned(), ctx.clone());
        std::thread::spawn(move || {
            let info = media_info(&ffprobe, &file);
            let _ = tx.send((file, info));
            ctx.request_repaint();
        });
        "Loading metadata...".to_owned()
    }

    fn trim(&mut self, len: usize) {
        while self.entries.len() > len {
            self.entries.pop_front();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Duration {
    Pending,
    Known(f64),
    Failed,
}

/// Per-file duration cache filled by background ffprobe runs.
pub struct DurationCache {
    entries: HashMap<String, Duration>,
    tx: Sender<(String, Option<f64>)>,
    rx: Receiver<(String, Option<f64>)>,
}

impl Default for DurationCache {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { entries: HashMap::new(), tx, rx }
    }
}

impl DurationCache {
    /// Collects finished probes; returns the files that just resolved.
    pub fn poll(&mut self) -> Vec<String> {
        let mut done = Vec::new();
        while let Ok((file, d)) = self.rx.try_recv() {
            self.entries.insert(file.clone(), d.map_or(Duration::Failed, Duration::Known));
            done.push(file);
        }
        done
    }

    /// The cached state of `file`, starting a probe when there is none yet.
    /// Returns `None` when no probe can run (no ffprobe, or the file is missing).
    pub fn get(&mut self, ffprobe: Option<&Path>, file: &str, ctx: &egui::Context) -> Option<Duration> {
        if let Some(d) = self.entries.get(file) {
            return Some(*d);
        }
        let ffprobe = ffprobe?.to_path_buf();
        if file.is_empty() || !Path::new(file).is_file() {
            return None;
        }
        self.entries.insert(file.to_owned(), Duration::Pending);
        let (tx, file, ctx) = (self.tx.clone(), file.to_owned(), ctx.clone());
        std::thread::spawn(move || {
            let d = duration(&ffprobe, &file);
            let _ = tx.send((file, d));
            ctx.request_repaint();
        });
        Some(Duration::Pending)
    }

    /// The cached state of `file` without starting a probe.
    pub fn state(&self, file: &str) -> Option<Duration> {
        self.entries.get(file).copied()
    }

    /// The known duration of `file`, if already probed.
    pub fn known(&self, file: &str) -> Option<f64> {
        match self.entries.get(file) {
            Some(Duration::Known(d)) if *d > 0.0 => Some(*d),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_drops_metadata_and_chapters() {
        let text = "Input #0, matroska,webm, from 'a.mkv':
  Metadata:
    title           : Clip
    ENCODER         : Lavf
  Duration: 00:01:00.00, start: 0.000000, bitrate: 5000 kb/s
  Chapters:
    Chapter #0:0: start 0.000000, end 60.000000
      Metadata:
        title           : One
  Stream #0:0: Video: hevc (Main), yuv420p10le, 1920x1080, 60 fps
    Metadata:
      DURATION        : 00:01:00.000000000
  Stream #0:1: Audio: opus, 48000 Hz, stereo";
        assert_eq!(
            compact_info(text),
            "Input #0, matroska,webm, from 'a.mkv':
  Duration: 00:01:00.00, start: 0.000000, bitrate: 5000 kb/s
  Stream #0:0: Video: hevc (Main), yuv420p10le, 1920x1080, 60 fps
  Stream #0:1: Audio: opus, 48000 Hz, stereo"
        );
    }

    #[test]
    fn resolution_is_read_from_csv() {
        assert_eq!(parse_resolution("1920,1080\r\n"), Some((1920, 1080)));
        assert_eq!(parse_resolution("1920,1080,"), None, "exactly two values");
        assert_eq!(parse_resolution(""), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_slow_command_is_killed() {
        let mut cmd = hidden_command("ping");
        cmd.args(["-n", "30", "127.0.0.1"]);
        let started = std::time::Instant::now();
        assert!(output_with_timeout(cmd, std::time::Duration::from_millis(300)).unwrap().is_none());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        let mut cmd = hidden_command("cmd");
        cmd.args(["/c", "echo out & echo err 1>&2"]);
        let out = output_with_timeout(cmd, std::time::Duration::from_secs(10)).unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "out");
        assert_eq!(String::from_utf8_lossy(&out.stderr).trim(), "err");
    }

    #[test]
    fn info_cache_keeps_the_most_recent_entries() {
        let ctx = egui::Context::default();
        let mut cache = InfoCache::default();
        assert_eq!(cache.get(None, "a.mp4", &ctx), "ffprobe not found");
        for i in 0..INFO_CACHE_SIZE {
            cache.entries.push_back((format!("{i}.mp4"), Some(format!("info {i}"))));
        }
        // A hit moves the entry to the back, so "0.mp4" survives the next insert
        assert_eq!(cache.get(None, "0.mp4", &ctx), "info 0");
        cache.tx.send(("new.mp4".into(), "fresh".into())).unwrap();
        assert_eq!(cache.get(None, "new.mp4", &ctx), "fresh");
        assert_eq!(cache.entries.len(), INFO_CACHE_SIZE);
        assert!(cache.entries.iter().any(|(f, _)| f == "0.mp4"));
        assert!(!cache.entries.iter().any(|(f, _)| f == "1.mp4"));
    }
}
