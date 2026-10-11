#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

use app::NvencFfxApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 750.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("nvencFFX"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "nvencFFX",
        native_options,
        Box::new(|cc| Ok(Box::new(NvencFfxApp::new(cc)))),
    )
}
