//! Runs ffmpeg off the UI thread (`_run_ffmpeg`): output lines stream back over a
//! channel, and a cancel terminates the running process.

use std::io::Read;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use super::probe::hidden_command;

pub enum Event {
    /// A non-empty output line of command `step`.
    Line { step: usize, text: String },
    /// The run is over; nothing follows.
    Finished(Outcome),
}

pub struct Outcome {
    /// The command that ended the run (the last one on success).
    pub step: usize,
    pub exit: Exit,
    /// Last non-empty output line, for error messages.
    pub last_line: String,
}

#[derive(Debug)]
pub enum Exit {
    Success,
    Cancelled,
    /// Non-zero exit code
    Failed(Option<i32>),
    /// The process could not be started.
    SpawnFailed(std::io::Error),
}

#[derive(Default)]
struct Shared {
    child: Mutex<Option<Child>>,
    cancelled: AtomicBool,
}

/// A background run of one or more ffmpeg commands; each starts once the previous
/// one succeeded.
pub struct Runner {
    rx: Receiver<Event>,
    shared: Arc<Shared>,
}

impl Runner {
    pub fn start(steps: Vec<Vec<String>>, ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let shared = Arc::new(Shared::default());
        let (worker, ctx) = (shared.clone(), ctx.clone());
        std::thread::spawn(move || run(&steps, &worker, &tx, &ctx));
        Self { rx, shared }
    }

    /// Events since the last call.
    pub fn poll(&self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }

    /// Terminates the running command; the run finishes as [`Exit::Cancelled`].
    pub fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::SeqCst);
        if let Some(child) = self.shared.child.lock().unwrap().as_mut() {
            let _ = child.kill();
        }
    }

    /// Suspends the running command (`_pause_conversion`); `false` if none runs or
    /// Windows refused. A cancel still terminates a suspended process.
    #[cfg(windows)]
    pub fn pause(&self) -> bool {
        self.shared.child.lock().unwrap().as_ref().is_some_and(crate::win32::suspend::suspend)
    }

    /// Resumes after [`Self::pause`] (`_resume_conversion`).
    #[cfg(windows)]
    pub fn resume(&self) -> bool {
        self.shared.child.lock().unwrap().as_ref().is_some_and(crate::win32::suspend::resume)
    }

    /// Cancels and waits (up to `timeout`) until the process is gone, so its files
    /// can be deleted.
    pub fn stop(&self, timeout: Duration) {
        self.cancel();
        let deadline = Instant::now() + timeout;
        loop {
            match self.rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(Event::Finished(_)) | Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return,
                Ok(Event::Line { .. }) => {}
            }
        }
    }
}

fn run(steps: &[Vec<String>], shared: &Shared, tx: &Sender<Event>, ctx: &egui::Context) {
    let mut last_line = String::new();
    for (step, args) in steps.iter().enumerate() {
        let exit = run_step(step, args, shared, tx, ctx, &mut last_line);
        if !matches!(exit, Exit::Success) || step + 1 == steps.len() {
            let _ = tx.send(Event::Finished(Outcome { step, exit, last_line }));
            ctx.request_repaint();
            return;
        }
    }
}

fn run_step(
    step: usize,
    args: &[String],
    shared: &Shared,
    tx: &Sender<Event>,
    ctx: &egui::Context,
    last_line: &mut String,
) -> Exit {
    if shared.cancelled.load(Ordering::SeqCst) {
        return Exit::Cancelled;
    }
    let Some((program, rest)) = args.split_first() else {
        return Exit::SpawnFailed(std::io::ErrorKind::InvalidInput.into());
    };
    // stdout and stderr share one pipe (`stderr=STDOUT`), so lines arrive in order
    let mut reader = {
        let (reader, writer) = match std::io::pipe() {
            Ok(pipe) => pipe,
            Err(e) => return Exit::SpawnFailed(e),
        };
        let stdout = match writer.try_clone() {
            Ok(w) => w,
            Err(e) => return Exit::SpawnFailed(e),
        };
        let mut cmd = hidden_command(program);
        cmd.args(rest).stdin(Stdio::null()).stdout(stdout).stderr(writer);
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => return Exit::SpawnFailed(e),
        };
        let mut slot = shared.child.lock().unwrap();
        // A cancel between the check above and the spawn found no child to kill
        if shared.cancelled.load(Ordering::SeqCst) {
            let _ = child.kill();
        }
        *slot = Some(child);
        // Dropping `cmd` closes our copies of the write end, so reads end with the child
        reader
    };

    for_each_line(&mut reader, |text| {
        last_line.clone_from(&text);
        let _ = tx.send(Event::Line { step, text });
        ctx.request_repaint();
    });

    // Poll instead of a blocking wait so `cancel` never waits for the lock
    let status = loop {
        let mut slot = shared.child.lock().unwrap();
        let Some(child) = slot.as_mut() else { break None };
        match child.try_wait() {
            Ok(Some(status)) => {
                *slot = None;
                break Some(status);
            }
            Ok(None) => {}
            Err(_) => {
                *slot = None;
                break None;
            }
        }
        drop(slot);
        std::thread::sleep(Duration::from_millis(20));
    };
    if shared.cancelled.load(Ordering::SeqCst) {
        Exit::Cancelled
    } else {
        match status {
            Some(s) if s.success() => Exit::Success,
            Some(s) => Exit::Failed(s.code()),
            None => Exit::Failed(None),
        }
    }
}

/// Calls `on_line` with each trimmed, non-empty line until `reader` ends. ffmpeg ends
/// its progress lines with '\r', so that ends a line too.
pub fn for_each_line(mut reader: impl Read, mut on_line: impl FnMut(String)) {
    let mut emit = |line: &mut Vec<u8>| {
        let text = String::from_utf8_lossy(line).trim().to_owned();
        line.clear();
        if !text.is_empty() {
            on_line(text);
        }
    };
    let (mut buf, mut line) = ([0u8; 4096], Vec::new());
    while let Ok(n @ 1..) = reader.read(&mut buf) {
        for &b in &buf[..n] {
            if b == b'\r' || b == b'\n' {
                emit(&mut line);
            } else {
                line.push(b);
            }
        }
    }
    emit(&mut line);
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn cmd(line: &str) -> Vec<String> {
        ["cmd", "/c"].into_iter().map(str::to_owned).chain([line.to_owned()]).collect()
    }

    fn wait(runner: &Runner) -> (Vec<(usize, String)>, Outcome) {
        let mut lines = Vec::new();
        loop {
            match runner.rx.recv_timeout(Duration::from_secs(20)).expect("runner finishes") {
                Event::Line { step, text } => lines.push((step, text)),
                Event::Finished(outcome) => return (lines, outcome),
            }
        }
    }

    #[test]
    fn streams_both_outputs_and_runs_steps_in_order() {
        let ctx = egui::Context::default();
        let runner = Runner::start(vec![cmd("echo one&& echo two 1>&2"), cmd("echo three")], &ctx);
        let (lines, outcome) = wait(&runner);
        assert_eq!(lines, [(0, "one".into()), (0, "two".into()), (1, "three".into())]);
        assert!(matches!(outcome.exit, Exit::Success));
        assert_eq!((outcome.step, outcome.last_line.as_str()), (1, "three"));
    }

    #[test]
    fn a_failing_step_stops_the_run() {
        let ctx = egui::Context::default();
        let runner = Runner::start(vec![cmd("echo bad&& exit 3"), cmd("echo never")], &ctx);
        let (lines, outcome) = wait(&runner);
        assert_eq!(lines, [(0, "bad".into())]);
        assert!(matches!(outcome.exit, Exit::Failed(Some(3))));
        assert_eq!(outcome.step, 0);

        let runner = Runner::start(vec![vec!["Z:\\no\\such\\ffmpeg.exe".into()]], &ctx);
        let (_, outcome) = wait(&runner);
        assert!(matches!(outcome.exit, Exit::SpawnFailed(ref e) if e.kind() == std::io::ErrorKind::NotFound));
    }

    #[test]
    fn pause_stops_output_until_resumed_and_cancel_still_works() {
        let ctx = egui::Context::default();
        let ping = ["ping", "-n", "30", "127.0.0.1"].map(str::to_owned).to_vec();
        let runner = Runner::start(vec![ping], &ctx);
        assert!(matches!(runner.rx.recv_timeout(Duration::from_secs(10)), Ok(Event::Line { .. })));
        assert!(runner.pause());
        // Lines already in the pipe may still arrive; after that ping stays silent
        std::thread::sleep(Duration::from_millis(300));
        runner.poll();
        std::thread::sleep(Duration::from_millis(2500));
        assert!(runner.poll().is_empty(), "no output while suspended");
        assert!(runner.resume());
        assert!(matches!(runner.rx.recv_timeout(Duration::from_secs(5)), Ok(Event::Line { .. })));

        assert!(runner.pause());
        runner.cancel();
        let (_, outcome) = wait(&runner);
        assert!(matches!(outcome.exit, Exit::Cancelled), "a suspended process can be cancelled");
        assert!(!runner.pause(), "nothing left to pause");
    }

    #[test]
    fn cancel_terminates_the_process() {
        let ctx = egui::Context::default();
        let ping = ["ping", "-n", "30", "127.0.0.1"].map(str::to_owned).to_vec();
        let runner = Runner::start(vec![ping, cmd("echo never")], &ctx);
        let started = Instant::now();
        // Wait for the first output line so the process is running
        assert!(matches!(runner.rx.recv_timeout(Duration::from_secs(10)), Ok(Event::Line { .. })));
        runner.cancel();
        let (_, outcome) = wait(&runner);
        assert!(matches!(outcome.exit, Exit::Cancelled));
        assert_eq!(outcome.step, 0);
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
