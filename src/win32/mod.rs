//! Thin Win32 helpers.

pub mod process_tree;
pub mod suspend;
pub mod taskbar;
pub mod tray;

use std::ffi::{OsString, c_void};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawWindowHandle,
    Win32WindowHandle, WindowHandle,
};
use windows_sys::Win32::{
    Foundation::{HWND, SYSTEMTIME},
    System::{Com::CoTaskMemFree, Diagnostics::Debug::MessageBeep, SystemInformation::GetLocalTime},
    UI::{
        HiDpi::{GetDpiForSystem, GetDpiForWindow},
        Shell::{FOLDERID_Desktop, SHGetKnownFolderPath, ShellExecuteW},
        WindowsAndMessaging::{
            GetClientRect, GetForegroundWindow, GetSystemMetrics, MB_ICONASTERISK, SM_CXSCREEN,
            SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
        },
    },
};
use windows_sys::core::PWSTR;

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: *mut c_void, attribute: u32, value: *const c_void, size: u32) -> i32;
}

/// The Win32 handle of an eframe window.
pub fn hwnd(window: &impl HasWindowHandle) -> Option<HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(win32) => Some(win32.hwnd.get() as HWND),
        _ => None,
    }
}

/// A window known only by its handle, e.g. as the parent of a file dialog opened
/// from a secondary viewport (eframe exposes no handle for those).
pub struct Window(HWND);

impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = std::num::NonZeroIsize::new(self.0 as isize).ok_or(HandleError::Unavailable)?;
        // The handle came from Windows and the dialog using it is modal, so it
        // outlives the borrow
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(Win32WindowHandle::new(hwnd))) })
    }
}

impl HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::windows())
    }
}

/// The window the user is working in, i.e. the one whose button was just clicked.
pub fn foreground_window() -> Option<Window> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_null()).then_some(Window(hwnd))
}

/// `get_real_dpi`: display scaling in percent (`int(round(dpi / 96 * 100))`), from the
/// DPI of the foreground window's monitor or else the system DPI. Valid before eframe
/// creates a window because the manifest declares per-monitor DPI awareness.
pub fn display_scaling() -> f32 {
    let hwnd = unsafe { GetForegroundWindow() };
    let dpi = match unsafe { GetDpiForWindow(hwnd) } {
        0 => unsafe { GetDpiForSystem() },
        dpi => dpi,
    };
    (dpi as f32 / 96.0 * 100.0).round()
}

/// Dark title bar.
///
/// Must run before the window is first shown (eframe keeps it hidden until the first
/// frame): switching an already visible window on Windows 10 only repaints the caption
/// text, leaving it on a dark patch over the light title bar.
pub fn use_dark_title_bar(window: &impl HasWindowHandle) {
    let Some(hwnd) = hwnd(window) else { return };
    let enabled: i32 = 1;
    // DWMWA_USE_IMMERSIVE_DARK_MODE: 20 since Windows 10 20H1, 19 on builds 1809-1909
    for attribute in [20, 19] {
        let hr = unsafe {
            DwmSetWindowAttribute(
                hwnd,
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

/// Centred horizontally on the primary screen, 50 px from the top. These are
/// physical pixels: the client width is centred, the frame's top-left corner is placed.
pub fn center_window(window: &impl HasWindowHandle) {
    let Some(hwnd) = hwnd(window) else { return };
    let mut rect = unsafe { std::mem::zeroed() };
    if unsafe { GetClientRect(hwnd, &mut rect) } == 0 {
        return;
    }
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let x = screen_width / 2 - (rect.right - rect.left) / 2;
    unsafe {
        SetWindowPos(hwnd, std::ptr::null_mut(), x, 50, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
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

/// Local time as `datetime.now().strftime("%d_%m_%y-%H_%M_%S")`.
pub fn local_timestamp() -> String {
    let mut t: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut t) };
    format!(
        "{:02}_{:02}_{:02}-{:02}_{:02}_{:02}",
        t.wDay,
        t.wMonth,
        t.wYear % 100,
        t.wHour,
        t.wMinute,
        t.wSecond
    )
}

/// The user's Desktop folder, also when it was moved (e.g. into OneDrive);
/// `%USERPROFILE%\Desktop` if the shell does not know it.
pub fn desktop_dir() -> PathBuf {
    let mut path: PWSTR = std::ptr::null_mut();
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_Desktop, 0, std::ptr::null_mut(), &mut path) };
    let known = (hr >= 0 && !path.is_null()).then(|| {
        let len = (0..).take_while(|&i| unsafe { *path.add(i) } != 0).count();
        PathBuf::from(OsString::from_wide(unsafe { std::slice::from_raw_parts(path, len) }))
    });
    // Freed even on failure, as the API requires
    unsafe { CoTaskMemFree(path as *const c_void) };
    known.unwrap_or_else(|| PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default()).join("Desktop"))
}
