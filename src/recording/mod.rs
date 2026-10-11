//! Screen recording session (`_start_recording` … `_finalize_recording`).
//!
//! One worker thread runs a whole recording: after a short delay ffmpeg captures the
//! desktop (ddagrab) into numbered segments, and a new segment starts whenever ffmpeg
//! ends on its own (a display mode change stops ddagrab). The desktop audio is
//! recorded once for the whole session. On stop, ffmpeg is asked to finish with `q`,
//! the segments are joined and the audio is muxed in. Progress reaches the UI as
//! [`Event`]s over a channel, like ffmpeg output does.

pub mod audio;
pub mod capture;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use crate::core::{probe::hidden_command, process::for_each_line};

/// Segments started after the first before the recording is given up
/// (`max_restart_attempts`).
pub const MAX_RESTART_ATTEMPTS: u32 = 3;

/// How long the audio thread may take to close its file (`audio_thread.join(timeout=2)`).
const AUDIO_FINISH_TIMEOUT: Duration = Duration::from_secs(2);

/// How often the worker looks at ffmpeg and at a stop request.
const POLL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// Before the first segment (the original's 2 s `Timer`).
    pub start_delay: Duration,
    /// Before a restarted segment: lets DXGI settle after a display change.
    pub restart_delay: Duration,
    /// How long ffmpeg may take to finish after `q` before it is killed.
    pub stop_timeout: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            start_delay: Duration::from_secs(2),
            restart_delay: Duration::from_secs(1),
            stop_timeout: Duration::from_secs(5),
        }
    }
}

/// Everything a session needs, taken from the settings when it starts.
pub struct Config {
    /// The capture command without its segment file ([`capture::command`]).
    pub command: Vec<String>,
    /// For joining and muxing.
    pub ffmpeg: String,
    pub files: capture::Files,
    /// Of the segments, from the output file.
    pub extension: String,
    pub output: PathBuf,
    /// Audio codec options for the mux; `None` records no audio (Audio Settings
    /// "Disable").
    pub audio: Option<Vec<String>>,
    pub timing: Timing,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// ffmpeg started segment `part`; part 1 is the start of the recording.
    Started { part: u32 },
    /// An output line of the running ffmpeg.
    Line(String),
    /// ffmpeg ended on its own; segment `part` starts next, as restart `attempt`
    /// of [`MAX_RESTART_ATTEMPTS`].
    Restarting { attempt: u32, part: u32 },
    /// The restart limit was reached; what was captured is being saved.
    GaveUp,
    Joining,
    Muxing,
    /// Stopped before ffmpeg started. Last event.
    Cancelled,
    /// The first segment's ffmpeg could not be started. Last event.
    StartFailed(String),
    /// Saved; `name` is the output's file name, `failure` why the recording ended
    /// early. Last event.
    Saved { name: String, failure: Option<String> },
    /// The recording could not be saved. Last event.
    SaveFailed(String),
}

impl Event {
    pub fn is_last(&self) -> bool {
        matches!(self, Self::Cancelled | Self::StartFailed(_) | Self::Saved { .. } | Self::SaveFailed(_))
    }
}

#[derive(Default)]
struct Shared {
    stopped: Mutex<bool>,
    wake: Condvar,
}

impl Shared {
    fn stopped(&self) -> bool {
        *self.stopped.lock().unwrap()
    }

    /// Sleeps `delay` unless a stop comes first; `true` if stopped.
    fn wait_stopped(&self, delay: Duration) -> bool {
        let guard = self.stopped.lock().unwrap();
        *self.wake.wait_timeout_while(guard, delay, |stopped| !*stopped).unwrap().0
    }
}

/// A recording in progress, until its last [`Event`].
pub struct Session {
    rx: Receiver<Event>,
    shared: Arc<Shared>,
}

impl Session {
    pub fn start(config: Config, ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let shared = Arc::new(Shared::default());
        let (worker, ctx) = (shared.clone(), ctx.clone());
        std::thread::Builder::new()
            .name("recording".to_owned())
            .spawn(move || {
                let send = |event| {
                    let _ = tx.send(event);
                    ctx.request_repaint();
                };
                run(&config, &worker, &send);
            })
            .expect("spawn recording thread");
        Self { rx, shared }
    }

    /// Events since the last call.
    pub fn poll(&self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }

    /// `_stop_recording`: ffmpeg finishes its segment, then the recording is saved.
    pub fn stop(&self) {
        *self.shared.stopped.lock().unwrap() = true;
        self.shared.wake.notify_all();
    }

    /// Waits up to `timeout` for the last event (on exit, so the recording is saved
    /// before the app ends).
    pub fn wait(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while let Ok(event) = self.rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            if event.is_last() {
                return;
            }
        }
    }
}

fn run(config: &Config, shared: &Shared, send: &dyn Fn(Event)) {
    if shared.wait_stopped(config.timing.start_delay) {
        return send(Event::Cancelled);
    }
    let mut parts = Vec::new();
    let mut audio = None;
    let (mut part, mut attempts) = (1, 0);
    let failure = loop {
        // `_launch_recording_segment`
        let segment = config.files.part(part, &config.extension);
        let code = match spawn(&config.command, &segment) {
            Ok((child, lines)) => {
                parts.push(segment);
                send(Event::Started { part });
                // Audio is recorded once for the entire session
                if part == 1 && config.audio.is_some() {
                    audio = Some(audio::Capture::start(config.files.audio()));
                }
                watch(child, &lines, shared, send, audio.as_ref(), config.timing.stop_timeout)
            }
            Err(e) if part == 1 => return send(Event::StartFailed(e.to_string())),
            Err(e) => format!("restart error: {e}"),
        };
        // `_restart_recording`; a manual stop never triggers it
        if shared.stopped() {
            break None;
        }
        if attempts >= MAX_RESTART_ATTEMPTS {
            break Some(format!("FFmpeg exited with code {code}; restart limit reached."));
        }
        attempts += 1;
        part += 1;
        send(Event::Restarting { attempt: attempts, part });
        if shared.wait_stopped(config.timing.restart_delay) {
            break None;
        }
    };
    if failure.is_some() {
        send(Event::GaveUp);
    }
    if let Some(audio) = audio {
        audio.finish(AUDIO_FINISH_TIMEOUT);
    }
    finalize(config, &parts, failure, send);
}

/// Starts ffmpeg writing `segment`, with a pipe for `q` on its stdin; its output
/// lines arrive on the returned channel.
fn spawn(args: &[String], segment: &Path) -> std::io::Result<(Child, Receiver<String>)> {
    let (program, rest) = args.split_first().ok_or(std::io::ErrorKind::InvalidInput)?;
    let (reader, writer) = std::io::pipe()?;
    let stdout = writer.try_clone()?;
    // The command, and with it our copies of the write end, is dropped right away,
    // so reading ends with the process
    let child = hidden_command(program)
        .args(rest)
        .arg(segment)
        .stdin(Stdio::piped())
        .stdout(stdout)
        .stderr(writer)
        .spawn()?;
    let (tx, rx) = channel();
    std::thread::spawn(move || for_each_line(reader, |line| { let _ = tx.send(line); }));
    Ok((child, rx))
}

/// `_monitor_recording`: forwards ffmpeg's output until it exits; on a stop request
/// sends `q` and kills it if it does not finish in time. Returns the exit code.
fn watch(
    mut child: Child,
    lines: &Receiver<String>,
    shared: &Shared,
    send: &dyn Fn(Event),
    audio: Option<&audio::Capture>,
    stop_timeout: Duration,
) -> String {
    let mut stopping: Option<Instant> = None;
    loop {
        match lines.recv_timeout(POLL) {
            Ok(line) if stopping.is_none() => send(Event::Line(line)),
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(POLL),
            _ => {}
        }
        if stopping.is_none() && shared.stopped() {
            if let Some(audio) = audio {
                audio.stop();
            }
            // 'q' lets ffmpeg finish the segment file properly
            let sent = child.stdin.as_mut().is_some_and(|stdin| stdin.write_all(b"q").and_then(|()| stdin.flush()).is_ok());
            if !sent {
                let _ = child.kill();
            }
            stopping = Some(Instant::now());
        }
        let status = match child.try_wait() {
            Ok(None) if stopping.is_some_and(|t| t.elapsed() >= stop_timeout) => {
                let _ = child.kill();
                child.wait().ok()
            }
            Ok(None) => continue,
            Ok(Some(status)) => Some(status),
            Err(_) => None,
        };
        // Windows exit codes unsigned, as Python reported them
        return status.and_then(|s| s.code()).map_or_else(|| "unknown".to_owned(), |c| (c as u32).to_string());
    }
}

/// Runs an ffmpeg step of the finalization with its output discarded.
fn run_ffmpeg(args: &[String]) -> bool {
    let Some((program, rest)) = args.split_first() else { return false };
    hidden_command(program)
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn non_empty(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.len() > 0)
}

/// `_concat_recording_parts`: one usable segment is used as it is.
fn join_parts(config: &Config, parts: &[PathBuf]) -> Result<PathBuf, String> {
    let usable: Vec<&Path> = parts.iter().map(PathBuf::as_path).filter(|p| non_empty(p)).collect();
    match usable[..] {
        [] => return Err("FFmpeg failed to generate a video segment. Check encoder settings.".to_owned()),
        [one] => return Ok(one.to_path_buf()),
        _ => {}
    }
    let list = config.files.concat_list();
    let joined = config.files.joined(&config.extension);
    let ok = std::fs::write(&list, capture::concat_list(&usable)).is_ok()
        && run_ffmpeg(&capture::concat_command(&config.ffmpeg, &list, &joined))
        && non_empty(&joined);
    let _ = std::fs::remove_file(&list);
    if ok { Ok(joined) } else { Err("Failed to join screen recording segments.".to_owned()) }
}

/// `shutil.move`: a rename, or copy and delete across drives.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

/// Joins the segments and writes the output; returns the joined file.
fn save(config: &Config, parts: &[PathBuf], send: &dyn Fn(Event)) -> Result<PathBuf, String> {
    let joined = join_parts(config, parts)?;
    let wav = config.files.audio();
    let has_audio = std::fs::metadata(&wav).is_ok_and(|m| m.len() > 100);
    match &config.audio {
        Some(audio) if has_audio => {
            send(Event::Muxing);
            let mux = capture::mux_command(&config.ffmpeg, &joined, &wav, audio, &config.output);
            if !run_ffmpeg(&mux) || !config.output.exists() {
                return Err("Failed to mux audio and video streams.".to_owned());
            }
        }
        _ => move_file(&joined, &config.output).map_err(|e| e.to_string())?,
    }
    Ok(joined)
}

/// `_finalize_recording`: temp files are deleted only once the output is saved.
fn finalize(config: &Config, parts: &[PathBuf], failure: Option<String>, send: &dyn Fn(Event)) {
    send(Event::Joining);
    match save(config, parts, send) {
        Ok(joined) => {
            for file in parts.iter().chain([&config.files.audio(), &joined]) {
                if file.exists() {
                    let _ = std::fs::remove_file(file);
                }
            }
            let name = config.output.file_name().unwrap_or_default().to_string_lossy().into_owned();
            send(Event::Saved { name, failure });
        }
        Err(e) => {
            let hint = if parts.iter().any(|p| p.exists()) {
                format!(" Segments were kept in {}.", config.files.dir().display())
            } else {
                String::new()
            };
            send(Event::SaveFailed(format!("{e}{hint}")));
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn config(command: &str, ffmpeg: &str, name: &str) -> Config {
        let dir = std::env::temp_dir();
        let files = capture::Files::new(&dir, name);
        Config {
            command: command.split(' ').map(str::to_owned).collect(),
            ffmpeg: ffmpeg.to_owned(),
            files,
            extension: ".mp4".to_owned(),
            output: dir.join(format!("nvencffx-test-{name}-{}.mp4", std::process::id())),
            audio: None,
            timing: Timing {
                start_delay: Duration::from_millis(10),
                restart_delay: Duration::from_millis(10),
                stop_timeout: Duration::from_millis(500),
            },
        }
    }

    /// Events up to the last one.
    fn events(session: &Session) -> Vec<Event> {
        let mut events = Vec::new();
        while events.last().is_none_or(|e: &Event| !e.is_last()) {
            events.push(session.rx.recv_timeout(Duration::from_secs(30)).expect("session ends"));
        }
        events
    }

    #[test]
    fn stop_before_ffmpeg_starts_cancels() {
        let ctx = egui::Context::default();
        let mut config = config("cmd /c exit 1", "ffmpeg", "cancel");
        config.timing.start_delay = Duration::from_secs(30);
        let session = Session::start(config, &ctx);
        let started = Instant::now();
        session.stop();
        assert_eq!(events(&session), [Event::Cancelled]);
        assert!(started.elapsed() < Duration::from_secs(5), "the stop ends the delay");
    }

    #[test]
    fn missing_ffmpeg_fails_the_start() {
        let ctx = egui::Context::default();
        let session = Session::start(config(r"Z:\missing\ffmpeg.exe -y", "ffmpeg", "missing"), &ctx);
        assert!(matches!(&events(&session)[..], [Event::StartFailed(_)]));
    }

    /// A stand-in for ffmpeg that writes its segment and exits at once, as after a
    /// display change: three restarts, then the four segments are joined (which
    /// fails here, with no ffmpeg) and kept.
    #[test]
    fn segments_restart_up_to_the_limit() {
        let ctx = egui::Context::default();
        let config = config("cmd /c echo frame=1& echo x>", r"Z:\missing\ffmpeg.exe", "restart");
        let parts: Vec<PathBuf> = (1..=4).map(|i| config.files.part(i, ".mp4")).collect();
        let dir = config.files.dir().to_path_buf();
        let session = Session::start(config, &ctx);
        let events: Vec<Event> = events(&session).into_iter().filter(|e| !matches!(e, Event::Line(_))).collect();
        let mut expected = vec![Event::Started { part: 1 }];
        for attempt in 1..=3 {
            expected.push(Event::Restarting { attempt, part: attempt + 1 });
            expected.push(Event::Started { part: attempt + 1 });
        }
        expected.extend([
            Event::GaveUp,
            Event::Joining,
            Event::SaveFailed(format!("Failed to join screen recording segments. Segments were kept in {}.", dir.display())),
        ]);
        assert_eq!(events, expected);
        for part in parts {
            assert!(part.exists(), "{} kept", part.display());
            let _ = std::fs::remove_file(part);
        }
    }

    /// Stopped while waiting to restart: the one segment becomes the output and the
    /// temp files are gone.
    #[test]
    fn stop_saves_the_segment_as_the_output() {
        let ctx = egui::Context::default();
        let mut config = config("cmd /c echo frame=1& echo x>", "ffmpeg", "save");
        config.timing.restart_delay = Duration::from_secs(30);
        let (part, output) = (config.files.part(1, ".mp4"), config.output.clone());
        let session = Session::start(config, &ctx);
        let first: Vec<Event> = (0..3).map(|_| session.rx.recv_timeout(Duration::from_secs(10)).unwrap()).collect();
        assert_eq!(first, [Event::Started { part: 1 }, Event::Line("frame=1".into()), Event::Restarting { attempt: 1, part: 2 }]);
        session.stop();
        let name = output.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(events(&session), [Event::Joining, Event::Saved { name, failure: None }]);
        assert_eq!(std::fs::read_to_string(&output).unwrap().trim(), "x");
        assert!(!part.exists(), "segment moved");
        let _ = std::fs::remove_file(output);
    }

    /// The real thing with a test pattern instead of the desktop: `q` ends ffmpeg,
    /// and the desktop audio (silence at least, if there is a playback device) is
    /// muxed in. Skipped without ffmpeg on `PATH`.
    #[test]
    fn ffmpeg_finishes_on_q_and_audio_is_muxed() {
        let Some(ffmpeg) = crate::core::probe::find_executable("ffmpeg.exe") else { return };
        let ffmpeg = ffmpeg.to_string_lossy().into_owned();
        let ctx = egui::Context::default();
        let command = format!("{ffmpeg} -y -re -f lavfi -i testsrc=size=320x240:rate=25 -c:v mpeg4");
        let mut config = config(&command, &ffmpeg, "ffmpeg");
        config.audio = Some(["-c:a", "aac", "-b:a", "96k"].map(str::to_owned).to_vec());
        config.timing.stop_timeout = Duration::from_secs(5);
        let (files, output) = (config.files.clone(), config.output.clone());
        let session = Session::start(config, &ctx);
        assert_eq!(session.rx.recv_timeout(Duration::from_secs(10)), Ok(Event::Started { part: 1 }));
        std::thread::sleep(Duration::from_millis(1500));
        let stopped = Instant::now();
        session.stop();
        let events = events(&session);
        assert!(stopped.elapsed() < Duration::from_secs(4), "ffmpeg obeyed q");
        let name = output.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(events.last(), Some(&Event::Saved { name, failure: None }), "{events:?}");
        assert!(non_empty(&output));
        assert!(!files.part(1, ".mp4").exists() && !files.audio().exists(), "temp files deleted");
        let _ = std::fs::remove_file(output);
    }
}
