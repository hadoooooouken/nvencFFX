//! Progress overlay on the taskbar button (`TaskbarProgress`, `ITaskbarList3`).
//!
//! Cosmetic: if the COM object cannot be created (e.g. explorer.exe is not running)
//! every call does nothing.

use std::ffi::c_void;
use windows_sys::Win32::{
    Foundation::HWND,
    System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx},
    UI::Shell::{
        TBPF_ERROR, TBPF_NOPROGRESS, TBPF_NORMAL, TBPF_PAUSED, TBPFLAG, TaskbarList,
    },
};
use windows_sys::core::{GUID, HRESULT};

const IID_ITASKBARLIST3: GUID = GUID::from_u128(0xea1afb91_9e28_4b86_90e9_9e9f8a5eefaf);

/// `ITaskbarList3` vtable (IUnknown -> ITaskbarList -> ITaskbarList2 -> ITaskbarList3),
/// up to the two methods used here.
#[repr(C)]
struct Vtbl {
    _query_interface: usize,
    _add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    hr_init: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    _add_tab: usize,
    _delete_tab: usize,
    _activate_tab: usize,
    _set_active_alt: usize,
    _mark_fullscreen_window: usize,
    set_progress_value: unsafe extern "system" fn(*mut c_void, HWND, u64, u64) -> HRESULT,
    set_progress_state: unsafe extern "system" fn(*mut c_void, HWND, TBPFLAG) -> HRESULT,
}

/// Colour of the taskbar bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// No bar (`TBPF_NOPROGRESS`)
    Off,
    /// Green (`TBPF_NORMAL`)
    Normal,
    /// Yellow (`TBPF_PAUSED`)
    Paused,
    /// Red (`TBPF_ERROR`)
    Error,
}

pub struct TaskbarProgress {
    hwnd: HWND,
    /// `ITaskbarList3*`
    list: *mut c_void,
}

impl TaskbarProgress {
    /// For the top-level window `hwnd`; `None` if the taskbar API is unavailable.
    /// Must be used on the thread that created it (the UI thread).
    pub fn new(hwnd: HWND) -> Option<Self> {
        let mut list: *mut c_void = std::ptr::null_mut();
        unsafe {
            // winit already initialised COM on this thread (OLE drag & drop), so
            // this only adds a reference; the result does not matter
            CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
            let hr = CoCreateInstance(&TaskbarList, std::ptr::null_mut(), CLSCTX_INPROC_SERVER, &IID_ITASKBARLIST3, &mut list);
            if hr < 0 || list.is_null() {
                return None;
            }
        }
        let this = Self { hwnd, list };
        if unsafe { (this.vtbl().hr_init)(this.list) } < 0 {
            return None;
        }
        Some(this)
    }

    fn vtbl(&self) -> &Vtbl {
        unsafe { &**(self.list as *const *const Vtbl) }
    }

    /// `set_progress`: `fraction` (0..=1) of the bar, in steps of 1%.
    pub fn set_progress(&self, fraction: f32) {
        let completed = (fraction * 100.0).round().clamp(0.0, 100.0) as u64;
        unsafe { (self.vtbl().set_progress_value)(self.list, self.hwnd, completed, 100) };
    }

    pub fn set_state(&self, state: State) {
        let flag = match state {
            State::Off => TBPF_NOPROGRESS,
            State::Normal => TBPF_NORMAL,
            State::Paused => TBPF_PAUSED,
            State::Error => TBPF_ERROR,
        };
        unsafe { (self.vtbl().set_progress_state)(self.list, self.hwnd, flag) };
    }
}

impl Drop for TaskbarProgress {
    /// `release`
    fn drop(&mut self) {
        unsafe { (self.vtbl().release)(self.list) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_are_harmless_without_a_taskbar_button() {
        // No window: the calls fail inside the shell, which must not matter
        if let Some(taskbar) = TaskbarProgress::new(std::ptr::null_mut()) {
            taskbar.set_state(State::Normal);
            taskbar.set_progress(0.5);
            taskbar.set_progress(7.0);
            taskbar.set_state(State::Off);
        }
    }
}
