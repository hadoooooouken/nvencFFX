#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod core;
mod recording;
#[cfg(windows)]
mod win32;
pub mod ui;

use app::NvencFfxApp;
use eframe::egui;

/// Main window title, restored after a screen recording.
pub const WINDOW_TITLE: &str = concat!("nvencFFX ", env!("CARGO_PKG_VERSION"));

/// Title bar / taskbar icon: the 32x32 entry of `nff.ico` (the same file `build.rs`
/// embeds into the exe). That entry is an uncompressed 32bpp BMP: BGRA rows, bottom-up.
fn window_icon() -> egui::IconData {
    const ICO: &[u8] = include_bytes!("../nff.ico");
    const SIZE: usize = 32;
    let u16_at = |o: usize| u16::from_le_bytes([ICO[o], ICO[o + 1]]) as usize;
    let u32_at = |o: usize| u32::from_le_bytes([ICO[o], ICO[o + 1], ICO[o + 2], ICO[o + 3]]) as usize;

    let entry = (0..u16_at(4)).map(|i| 6 + 16 * i).find(|&e| {
        ICO[e] as usize == SIZE && ICO[e + 1] as usize == SIZE && u16_at(e + 6) == 32
    });
    let Some(entry) = entry else {
        return egui::IconData::default();
    };
    // Image data starts with a BITMAPINFOHEADER whose first field is its own size
    let bmp = u32_at(entry + 12);
    let pixels = bmp + u32_at(bmp);

    let mut rgba = Vec::with_capacity(SIZE * SIZE * 4);
    for row in (0..SIZE).rev() {
        for &[b, g, r, a] in ICO[pixels + row * SIZE * 4..][..SIZE * 4].as_chunks::<4>().0 {
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }
    egui::IconData { rgba, width: SIZE as u32, height: SIZE as u32 }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            // With vsync, every repaint blocks in present() for up to a frame. eframe
            // repaints on each WM_MOVE, so inside Windows' modal move loop that makes
            // window dragging stutter. Falls back Immediate -> Mailbox -> Fifo.
            surface: eframe::egui_wgpu::SurfaceConfig {
                present_mode: eframe::wgpu::PresentMode::AutoNoVsync,
                ..eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY
            },
            ..Default::default()
        },
        viewport: egui::ViewportBuilder::default()
            // Same footprint as the CustomTkinter original: fixed width, resizable height
            .with_inner_size([820.0, 700.0])
            .with_min_inner_size([820.0, 600.0])
            .with_max_inner_size([820.0, 4096.0])
            .with_title(WINDOW_TITLE)
            .with_icon(window_icon()),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "nvencFFX",
        native_options,
        Box::new(|cc| Ok(Box::new(NvencFfxApp::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn window_icon_is_decoded_from_nff_ico() {
        let icon = super::window_icon();
        assert_eq!((icon.width, icon.height), (32, 32));
        assert_eq!(icon.rgba.len(), 32 * 32 * 4);
        assert!(icon.rgba.as_chunks::<4>().0.iter().any(|p| p[3] == 255), "icon has opaque pixels");
    }
}
