#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
pub mod ui;

use app::NvencFfxApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            // With vsync, every repaint blocks in present() for up to a frame. eframe
            // repaints on each WM_MOVE, so inside Windows' modal move loop that makes
            // window dragging stutter. Falls back Immediate -> Mailbox -> Fifo.
            present_mode: eframe::wgpu::PresentMode::AutoNoVsync,
            ..Default::default()
        },
        viewport: egui::ViewportBuilder::default()
            // Same footprint as the CustomTkinter original: fixed width, resizable height
            .with_inner_size([820.0, 700.0])
            .with_min_inner_size([820.0, 600.0])
            .with_max_inner_size([820.0, 4096.0])
            .with_title(concat!("nvencFFX ", env!("CARGO_PKG_VERSION"))),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "nvencFFX",
        native_options,
        Box::new(|cc| Ok(Box::new(NvencFfxApp::new(cc)))),
    )
}
