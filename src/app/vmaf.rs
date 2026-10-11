//! VMAF analysis from a right click on Play Output File (`_on_vmaf_right_click`,
//! `_run_vmaf_analysis`, `_execute_vmaf`). Runs alongside a conversion, as in the
//! original.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use eframe::egui;
use crate::core::{
    probe,
    process::{Event, Exit, Runner},
    vmaf,
};
use crate::ui::dialogs::Kind;
use super::NvencFfxApp;

type Prepared = Result<(Vec<String>, &'static str), String>;

pub(super) enum Vmaf {
    /// ffprobe reads both videos' sizes.
    Probing(Receiver<Prepared>),
    Running {
        runner: Runner,
        label: &'static str,
        /// The last score line's number.
        score: Option<String>,
    },
}

/// Both sizes, then the command (the part of `_run_vmaf_analysis` off the UI thread).
fn prepare(ffmpeg: &str, ffprobe: Option<&Path>, input: &str, output: &str, scale_flags: &str) -> Prepared {
    let size = |file| ffprobe.and_then(|p| probe::resolution(p, file));
    let (Some(input_size), Some(output_size)) = (size(input), size(output)) else {
        return Err("Could not determine video resolution".to_owned());
    };
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    Ok(vmaf::command(ffmpeg, input, output, input_size, output_size, scale_flags, threads))
}

/// The status line for a finished run (`_execute_vmaf`).
fn result_status(label: &str, score: Option<&str>) -> String {
    match score.map(str::parse::<f64>) {
        Some(Ok(score)) => format!("{label}: {score:.2}"),
        Some(Err(_)) => "Error parsing VMAF score".to_owned(),
        None => "VMAF score not found in output".to_owned(),
    }
}

impl NvencFfxApp {
    /// Right click on Play Output File: starts the analysis, or cancels a running one.
    pub(super) fn toggle_vmaf(&mut self, ctx: &egui::Context) {
        if self.vmaf.is_some() {
            return self.cancel_vmaf();
        }
        if self.input_file.is_empty() || self.output_file.is_empty() {
            return self.message(Kind::Warning, "Warning", "Please specify both Input and Output files.");
        }
        if !Path::new(&self.input_file).exists() || !Path::new(&self.output_file).exists() {
            return self.message(Kind::Warning, "Warning", "Input or Output file does not exist.");
        }
        if self.s.ffmpeg_path.is_empty() {
            return self.message(Kind::Error, "Error", "FFmpeg path is not specified");
        }
        let (tx, rx) = channel();
        let ffmpeg = self.s.ffmpeg_path.clone();
        let ffprobe: Option<PathBuf> = self.ffprobe.clone();
        let (input, output) = (self.input_file.clone(), self.output_file.clone());
        let (flags, repaint) = (self.s.interpolation_algo.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(prepare(&ffmpeg, ffprobe.as_deref(), &input, &output, &flags));
            repaint.request_repaint();
        });
        self.vmaf = Some(Vmaf::Probing(rx));
    }

    /// `_cancel_vmaf`
    fn cancel_vmaf(&mut self) {
        if let Some(Vmaf::Running { runner, .. }) = self.vmaf.take() {
            runner.cancel();
        }
        self.status_message = "VMAF analysis cancelled".to_owned();
        self.ffmpeg_output.clear();
    }

    pub(super) fn poll_vmaf(&mut self, ctx: &egui::Context) {
        match &mut self.vmaf {
            None => {}
            Some(Vmaf::Probing(rx)) => match rx.try_recv() {
                Ok(Ok((args, label))) => {
                    self.status_message = "VMAF Analysis in progress...".to_owned();
                    self.ffmpeg_output = "Calculating VMAF score...".to_owned();
                    self.vmaf = Some(Vmaf::Running { runner: Runner::start(vec![args], ctx), label, score: None });
                }
                Ok(Err(e)) => {
                    self.vmaf = None;
                    self.message(Kind::Error, "Error", e);
                }
                Err(_) => {}
            },
            Some(Vmaf::Running { runner, label, score }) => {
                let label = *label;
                for event in runner.poll() {
                    match event {
                        Event::Line { text, .. } => {
                            if let Some(found) = vmaf::score(&text) {
                                *score = Some(found.to_owned());
                            }
                            self.ffmpeg_output = text;
                        }
                        Event::Finished(outcome) => {
                            match outcome.exit {
                                Exit::Cancelled => {}
                                Exit::SpawnFailed(e) => self.status_message = format!("Error executing VMAF: {e}"),
                                Exit::Success | Exit::Failed(_) => {
                                    self.status_message = result_status(label, score.as_deref());
                                    #[cfg(windows)]
                                    if score.is_some() {
                                        crate::win32::beep();
                                    }
                                }
                            }
                            self.ffmpeg_output.clear();
                            self.vmaf = None;
                            return;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_texts() {
        assert_eq!(result_status("VMAF score", Some("95.126")), "VMAF score: 95.13");
        assert_eq!(result_status("VMAF score (with scaling)", Some("80")), "VMAF score (with scaling): 80.00");
        assert_eq!(result_status("VMAF score", Some("1.2.3")), "Error parsing VMAF score");
        assert_eq!(result_status("VMAF score", None), "VMAF score not found in output");
        assert_eq!(prepare("ffmpeg", None, "a", "b", "bicubic"), Err("Could not determine video resolution".to_owned()));
    }

    #[test]
    fn checks_before_starting_and_right_click_cancels() {
        let ctx = egui::Context::default();
        let mut app = NvencFfxApp::default();
        app.toggle_vmaf(&ctx);
        assert_eq!(app.dialog.take().unwrap().1.message, "Please specify both Input and Output files.");
        app.input_file = r"Z:\in.mp4".into();
        app.output_file = r"Z:\out.mp4".into();
        app.toggle_vmaf(&ctx);
        assert_eq!(app.dialog.take().unwrap().1.message, "Input or Output file does not exist.");
        let exe = std::env::current_exe().unwrap().to_string_lossy().into_owned();
        (app.input_file, app.output_file) = (exe.clone(), exe);
        app.toggle_vmaf(&ctx);
        assert_eq!(app.dialog.take().unwrap().1.message, "FFmpeg path is not specified");

        app.s.ffmpeg_path = "ffmpeg.exe".into();
        app.toggle_vmaf(&ctx);
        assert!(app.vmaf.is_some());
        app.toggle_vmaf(&ctx);
        assert!(app.vmaf.is_none());
        assert_eq!(app.status_message, "VMAF analysis cancelled");
    }

    /// The real analysis of a clip against itself scaled down. Skipped without
    /// ffmpeg / ffprobe on `PATH` or an ffmpeg without libvmaf.
    #[test]
    fn real_analysis_reports_a_score() {
        let (Some(ffmpeg), Some(ffprobe)) = (probe::find_executable("ffmpeg.exe"), probe::find_executable("ffprobe.exe"))
        else {
            return;
        };
        let filters = probe::hidden_command(&ffmpeg).args(["-hide_banner", "-filters"]).output().unwrap();
        if !String::from_utf8_lossy(&filters.stdout).contains("libvmaf") {
            return;
        }
        let dir = std::env::temp_dir();
        let pid = std::process::id();
        let (input, output) = (dir.join(format!("nvencffx-vmaf-in-{pid}.mp4")), dir.join(format!("nvencffx-vmaf-out-{pid}.mp4")));
        for (file, size) in [(&input, "320x240"), (&output, "160x120")] {
            let source = format!("testsrc=duration=1:size={size}:rate=10");
            let made = probe::hidden_command(&ffmpeg)
                .args(["-v", "error", "-y", "-f", "lavfi", "-i", &source, "-c:v", "mpeg4", "-q:v", "2"])
                .arg(file)
                .status()
                .unwrap();
            assert!(made.success());
        }
        let ctx = egui::Context::default();
        let mut app = NvencFfxApp {
            input_file: input.to_string_lossy().into_owned(),
            output_file: output.to_string_lossy().into_owned(),
            ffprobe: Some(ffprobe),
            ..Default::default()
        };
        app.s.ffmpeg_path = ffmpeg.to_string_lossy().into_owned();
        app.toggle_vmaf(&ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while app.vmaf.is_some() && std::time::Instant::now() < deadline {
            app.poll_vmaf(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(app.status_message.starts_with("VMAF score (with scaling): "), "{}", app.status_message);
        assert!(app.ffmpeg_output.is_empty());
        let _ = std::fs::remove_file(input);
        let _ = std::fs::remove_file(output);
    }
}
