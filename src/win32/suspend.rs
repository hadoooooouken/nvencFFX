//! Pausing a running ffmpeg (`_suspend_process_by_pid`, `_resume_process_by_pid`).

use std::os::windows::io::AsRawHandle;
use windows_sys::Win32::Foundation::{HANDLE, NTSTATUS};

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtSuspendProcess(process: HANDLE) -> NTSTATUS;
    fn NtResumeProcess(process: HANDLE) -> NTSTATUS;
}

/// Suspends every thread of `process`. Uses the handle `std` got from
/// `CreateProcess` rather than reopening the PID, which could have been reused.
pub fn suspend(process: &impl AsRawHandle) -> bool {
    unsafe { NtSuspendProcess(process.as_raw_handle()) == 0 }
}

/// Resumes a process stopped by [`suspend`].
pub fn resume(process: &impl AsRawHandle) -> bool {
    unsafe { NtResumeProcess(process.as_raw_handle()) == 0 }
}
