//! Terminates the ffmpeg / ffprobe processes we started, so none outlive the app.

use windows_sys::Win32::{
    Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
    },
};

/// PIDs of this process's children whose executable name is one of `names`
/// (ignoring case), so instances other programs launched are left alone.
pub fn children_named(names: &[&str]) -> Vec<u32> {
    let me = std::process::id();
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut pids = Vec::new();
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        if entry.th32ParentProcessID == me {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let exe = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if names.iter().any(|n| n.eq_ignore_ascii_case(&exe)) {
                pids.push(entry.th32ProcessID);
            }
        }
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    pids
}

/// Terminates every child process named in `names`.
pub fn kill_children(names: &[&str]) {
    for pid in children_named(names) {
        terminate(pid);
    }
}

fn terminate(pid: u32) {
    let process = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if !process.is_null() {
        unsafe {
            TerminateProcess(process, 1);
            CloseHandle(process);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn kills_only_named_children() {
        let mut child = Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        assert!(children_named(&["PING.EXE"]).contains(&child.id()), "names match case-insensitively");
        // A name nothing runs under: other tests run ffmpeg and ping in parallel
        kill_children(&["nvencffx-test-none.exe"]);
        assert!(child.try_wait().unwrap().is_none(), "other children are left alone");
        terminate(child.id());
        assert!(!child.wait().unwrap().success());
    }
}
