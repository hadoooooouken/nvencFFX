//! Thin Win32 helpers.

use std::ffi::c_void;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: *mut c_void, attribute: u32, value: *const c_void, size: u32) -> i32;
}

/// Dark title bar, like CustomTkinter gives the original.
///
/// Must run before the window is first shown (eframe keeps it hidden until the first
/// frame): switching an already visible window on Windows 10 only repaints the caption
/// text, leaving it on a dark patch over the light title bar.
pub fn use_dark_title_bar(window: &impl HasWindowHandle) {
    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else { return };
    let enabled: i32 = 1;
    // DWMWA_USE_IMMERSIVE_DARK_MODE: 20 since Windows 10 20H1, 19 on builds 1809-1909
    for attribute in [20, 19] {
        let hr = unsafe {
            DwmSetWindowAttribute(
                win32.hwnd.get() as *mut c_void,
                attribute,
                (&raw const enabled).cast(),
                size_of::<i32>() as u32,
            )
        };
        if hr >= 0 {
            break;
        }
    }
}
