//! Thin Win32 helpers.

pub mod process_tree;

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    System::Diagnostics::Debug::MessageBeep,
    UI::{Shell::ShellExecuteW, WindowsAndMessaging::{MB_ICONASTERISK, SW_SHOWNORMAL}},
};

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

/// `os.startfile`: opens `path` with its default application.
pub fn open_file(path: &Path) -> Result<(), String> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values up to 32 are error codes; apart from SE_ERR_NOASSOC they match Win32 errors
    match result as usize {
        31 => Err("No application is associated with this file type".to_owned()),
        code @ 0..=32 => Err(std::io::Error::from_raw_os_error(code as i32).to_string()),
        _ => Ok(()),
    }
}

/// The "conversion complete" sound (`MessageBeep(MB_ICONASTERISK)`).
pub fn beep() {
    unsafe { MessageBeep(MB_ICONASTERISK) };
}
