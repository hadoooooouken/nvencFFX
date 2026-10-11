//! Screen recording in the main window (`_screen_record`, `_start_recording`,
//! `_stop_recording`): the Screen Record button, the window title, tray balloons and
//! the status lines of a [`recording::Session`].

use std::path::PathBuf;
use std::time::{Duration, Instant};
use eframe::egui::{self, ViewportCommand};
use crate::core::command;
use crate::recording::{self, Event, MAX_RESTART_ATTEMPTS, Session, capture};
use crate::ui::{dialogs::Kind, widgets::ButtonKind};
use super::NvencFfxApp;

const RECORDING_TITLE: &str = "Screen is recording now - nvencFFX";
/// The window is minimized this long after a recording starts.
const MINIMIZE_DELAY: Duration = Duration::from_secs(1);
/// On exit, how long saving a running recording may take.
const EXIT_SAVE_TIMEOUT: Duration = Duration::from_secs(300);

pub(super) struct Recording {
    session: Session,
    /// Stopped, or given up after repeated ffmpeg errors: the recording is being
    /// saved and the button is disabled (`recording_finalizing`).
    stopping: bool,
}

impl NvencFfxApp {
    /// Text and look of the Screen Record button.
    pub(super) fn record_button(&self) -> (&'static str, ButtonKind) {
        match &self.recording {
            Some(r) if r.stopping => ("Stop Recording", ButtonKind::Disabled),
            Some(_) => ("Stop Recording", ButtonKind::Red),
            None => ("Screen Record", ButtonKind::Grey),
        }
    }

    /// The Screen Record / Stop Recording button (`_screen_record`).
    pub(super) fn toggle_recording(&mut self, ctx: &egui::Context) {
        if self.recording.is_some() {
            self.stop_recording(ctx);
        } else {
            self.start_recording(ctx);
        }
    }

    /// `_start_recording`. Alt+F8 and the tray's Start do nothing while recording.
    pub(super) fn start_recording(&mut self, ctx: &egui::Context) {
        match &self.recording {
            Some(r) if r.stopping => {
                return self.message(Kind::Info, "Screen recording", "Please wait until the previous recording is saved.");
            }
            Some(_) => return,
            None => {}
        }
        let ffmpeg = self.s.ffmpeg_path.trim().to_owned();
        if ffmpeg.is_empty() {
            return self.message(Kind::Error, "Error", "FFmpeg path is not specified.");
        }
        let audio = if self.s.audio_option == "disable" {
            None
        } else {
            let mut args = Vec::new();
            if let Err(e) = command::append_audio(&mut args, &self.s) {
                return self.message(Kind::Error, "Error", format!("Failed to start screen recording: {e}"));
            }
            Some(args)
        };

        #[cfg(windows)]
        let (date, desktop) = (crate::win32::local_timestamp(), crate::win32::desktop_dir());
        #[cfg(not(windows))]
        let (date, desktop) = (String::new(), PathBuf::new());
        let output = match self.output_file.trim() {
            "" => capture::default_output(&desktop, &date),
            out => PathBuf::from(out),
        };
        let config = recording::Config {
            command: capture::command(&self.s, &ffmpeg, self.expand_additional_options),
            ffmpeg,
            files: capture::Files::new(&std::env::temp_dir(), &date),
            extension: capture::extension(&output),
            output,
            audio,
            timing: recording::Timing::default(),
        };

        ctx.send_viewport_cmd(ViewportCommand::Title(RECORDING_TITLE.to_owned()));
        self.status_message = "Screen is recording now...".to_owned();
        self.ffmpeg_output = "Screen recording starting...".to_owned();
        self.balloon("Recording Started", "Screen is being captured...");
        self.minimize_due = Some(Instant::now() + MINIMIZE_DELAY);
        self.recording = Some(Recording { session: Session::start(config, ctx), stopping: false });
    }

    /// `_stop_recording`. Alt+F9 and the tray's Stop do nothing without a recording.
    pub(super) fn stop_recording(&mut self, ctx: &egui::Context) {
        let Some(recording) = &mut self.recording else { return };
        if recording.stopping {
            return;
        }
        recording.stopping = true;
        recording.session.stop();
        ctx.send_viewport_cmd(ViewportCommand::Title(crate::WINDOW_TITLE.to_owned()));
        self.minimize_due = None;
    }

    /// The pending minimize, 1 s after the start (`after(1000, iconify)`).
    pub(super) fn minimize_when_due(&mut self, ctx: &egui::Context) {
        let Some(due) = self.minimize_due else { return };
        let now = Instant::now();
        if now >= due {
            self.minimize_due = None;
            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        } else {
            ctx.request_repaint_after(due - now);
        }
    }

    pub(super) fn poll_recording(&mut self, ctx: &egui::Context) {
        let Some(recording) = &self.recording else { return };
        let events = recording.session.poll();
        let mut stopping = recording.stopping;
        let mut ended = false;
        for event in events {
            ended |= event.is_last();
            match event {
                Event::Started { part } => {
                    self.status_message = "Screen is recording now...".to_owned();
                    self.ffmpeg_output = if part == 1 {
                        "Screen recording started...".to_owned()
                    } else {
                        format!("Screen recording resumed in segment {part}.")
                    };
                }
                Event::Line(text) if !stopping => self.ffmpeg_output = text,
                Event::Line(_) => {}
                Event::Restarting { attempt, part } => {
                    self.status_message = format!("Display changed. Restarting recording ({attempt}/{MAX_RESTART_ATTEMPTS})...");
                    self.ffmpeg_output = format!("FFmpeg stopped unexpectedly; starting segment {part} (attempt {attempt}).");
                }
                // `_recording_recovery_failed`
                Event::GaveUp => {
                    stopping = true;
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                }
                Event::Joining => {
                    self.status_message = "Joining recording segments...".to_owned();
                    self.ffmpeg_output = "Joining recording segments... please wait.".to_owned();
                }
                Event::Muxing => {
                    self.status_message = "Muxing audio and video...".to_owned();
                    self.ffmpeg_output = "Muxing audio and video... please wait.".to_owned();
                }
                Event::Cancelled => {
                    self.status_message = "Screen recording cancelled".to_owned();
                    self.ffmpeg_output.clear();
                }
                // `_handle_recording_error`
                Event::StartFailed(e) => {
                    self.status_message = "Screen recording failed".to_owned();
                    self.ffmpeg_output.clear();
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    self.message(Kind::Error, "Error", format!("Failed to start screen recording: {e}"));
                }
                Event::Saved { name, failure } => {
                    match failure {
                        Some(failure) => {
                            self.status_message = "Recording stopped after repeated FFmpeg errors".to_owned();
                            self.ffmpeg_output = format!("Saved partial recording to {name}. {failure}");
                        }
                        None => {
                            self.status_message = "Screen recording stopped".to_owned();
                            self.ffmpeg_output = format!("Saved to {name}");
                        }
                    }
                    #[cfg(windows)]
                    crate::win32::beep();
                    self.balloon("Recording Saved", &format!("File: {name}"));
                }
                Event::SaveFailed(message) => {
                    self.status_message = "Error finishing recording".to_owned();
                    self.ffmpeg_output = message;
                }
            }
        }
        if ended {
            self.recording = None;
            self.minimize_due = None;
            ctx.send_viewport_cmd(ViewportCommand::Title(crate::WINDOW_TITLE.to_owned()));
        } else if let Some(recording) = &mut self.recording {
            recording.stopping = stopping;
        }
    }

    /// On exit: a running recording is stopped and saved before the app ends.
    pub(super) fn finish_recording(&mut self) {
        if let Some(recording) = self.recording.take() {
            recording.session.stop();
            recording.session.wait(EXIT_SAVE_TIMEOUT);
        }
    }

    /// `show_balloon` on the tray icon.
    fn balloon(&self, title: &str, message: &str) {
        #[cfg(windows)]
        if let Some(tray) = &self.tray {
            tray.balloon(title, message);
        }
        #[cfg(not(windows))]
        let _ = (title, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::Settings;

    fn recorder_app() -> NvencFfxApp {
        NvencFfxApp { s: Settings { ffmpeg_path: r"Z:\missing\ffmpeg.exe".into(), ..Default::default() }, ..Default::default() }
    }

    fn poll_until_ended(app: &mut NvencFfxApp, ctx: &egui::Context) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while app.recording.is_some() && Instant::now() < deadline {
            app.poll_recording(ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(app.recording.is_none(), "recording ended");
    }

    #[test]
    fn start_checks_ffmpeg_and_audio_settings() {
        let ctx = egui::Context::default();
        let mut app = NvencFfxApp::default();
        app.toggle_recording(&ctx);
        assert_eq!(app.dialog.take().unwrap().1.message, "FFmpeg path is not specified.");

        let mut app = recorder_app();
        app.s.audio_option = "custom".into();
        app.s.custom_abitrate = "lots".into();
        app.toggle_recording(&ctx);
        assert_eq!(app.dialog.take().unwrap().1.message, "Failed to start screen recording: Custom audio bitrate must be a number.");
        assert!(app.recording.is_none());
    }

    #[test]
    fn stop_during_the_start_delay_cancels() {
        let ctx = egui::Context::default();
        let mut app = recorder_app();
        app.toggle_recording(&ctx);
        assert_eq!(app.record_button(), ("Stop Recording", ButtonKind::Red));
        assert_eq!(app.status_message, "Screen is recording now...");
        assert_eq!(app.ffmpeg_output, "Screen recording starting...");
        assert!(app.minimize_due.is_some());

        app.toggle_recording(&ctx);
        assert_eq!(app.record_button(), ("Stop Recording", ButtonKind::Disabled));
        assert!(app.minimize_due.is_none(), "no minimize after a stop");
        // Start while saving asks to wait; a second stop does nothing
        app.start_recording(&ctx);
        assert_eq!(app.dialog.take().unwrap().1.message, "Please wait until the previous recording is saved.");
        app.stop_recording(&ctx);

        poll_until_ended(&mut app, &ctx);
        assert_eq!(app.status_message, "Screen recording cancelled");
        assert_eq!(app.ffmpeg_output, "");
        assert_eq!(app.record_button(), ("Screen Record", ButtonKind::Grey));
    }

    #[cfg(windows)]
    #[test]
    fn missing_ffmpeg_reports_the_failed_start() {
        let ctx = egui::Context::default();
        let mut app = recorder_app();
        app.start_recording(&ctx);
        // A second start (Alt+F8) while recording is ignored
        app.start_recording(&ctx);
        assert!(app.dialog.is_none());
        poll_until_ended(&mut app, &ctx);
        assert_eq!(app.status_message, "Screen recording failed");
        let message = app.dialog.take().unwrap().1.message;
        assert!(message.starts_with("Failed to start screen recording: "), "{message}");
    }
}
