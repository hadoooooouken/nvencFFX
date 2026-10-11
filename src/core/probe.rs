//! Locating ffmpeg / ffprobe and probing media durations off the UI thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, Sender, channel};

/// `_find_executable`: next to our executable first, then on `PATH`.
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

/// A console-less child process, like the original's `CREATE_NO_WINDOW` calls.
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
