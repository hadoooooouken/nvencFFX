//! Notification-area icon with its menu, balloons and the global recording hotkeys.
//!
//! A hidden window on its own thread receives the tray and `WM_HOTKEY` messages, and the chosen commands reach
//! the UI over a channel, like ffmpeg output does.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{MOD_ALT, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_F8, VK_F9},
        Shell::{
            NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY,
            NOTIFYICONDATAW, Shell_NotifyIconW,
        },
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
            DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics, HICON,
            IDI_APPLICATION, IMAGE_ICON, LoadIconW, LoadImageW, MF_SEPARATOR, MF_STRING, MSG,
            PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SM_CXSMICON,
            SM_CYSMICON, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
            TranslateMessage, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY, WM_HOTKEY,
            WM_LBUTTONDBLCLK, WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
};

/// What the tray menu or a hotkey asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Alt+F8 / "Start Recording"
    StartRecording,
    /// Alt+F9 / "Stop Recording"
    StopRecording,
    /// Double click / "Open nvencFFX"
    Open,
    /// "Exit"
    Exit,
}

const WM_TRAY: u32 = WM_APP; // WM_USER_TRAY
const ICON_ID: u32 = 1;
const IDM_START_RECORDING: usize = 1001;
const IDM_STOP_RECORDING: usize = 1002;
const IDM_OPEN_APP: usize = 1003;
const IDM_EXIT: usize = 1004;
const HOTKEY_START_ID: i32 = 2001;
const HOTKEY_STOP_ID: i32 = 2002;
const TIP: &str = crate::WINDOW_TITLE;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Copies `s` into a fixed `WCHAR` field, truncated to keep its terminating zero.
fn copy_wide(dst: &mut [u16], s: &str) {
    let n = dst.len() - 1;
    let mut len = 0;
    for (d, c) in dst[..n].iter_mut().zip(s.encode_utf16()) {
        *d = c;
        len += 1;
    }
    dst[len] = 0;
}

/// Window handle and icon, shared by the tray thread and [`Tray::balloon`].
#[derive(Clone, Copy)]
struct Icon {
    hwnd: usize,
    hicon: usize,
}

impl Icon {
    fn data(self) -> NOTIFYICONDATAW {
        let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd as HWND;
        nid.uID = ICON_ID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = self.hicon as HICON;
        copy_wide(&mut nid.szTip, TIP);
        nid
    }

    fn add(self) {
        unsafe { Shell_NotifyIconW(NIM_ADD, &self.data()) };
    }
}

/// State of the tray thread's window procedure.
struct State {
    icon: Icon,
    /// "TaskbarCreated": Explorer restarted and the icon must be added again
    taskbar_created: u32,
    send: Box<dyn Fn(Command)>,
}

thread_local! {
    static STATE: RefCell<Option<Rc<State>>> = const { RefCell::new(None) };
}

/// The tray icon, shown until this is dropped.
pub struct Tray {
    icon: Icon,
    rx: Receiver<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Tray {
    /// Adds the icon and registers Alt+F8 / Alt+F9. `wake` runs after each command
    /// (to repaint the UI). `None` if the window could not be created.
    pub fn start(wake: impl Fn() + Send + 'static) -> Option<Self> {
        let (tx, rx) = channel();
        let (ready_tx, ready_rx) = channel();
        let thread = std::thread::Builder::new()
            .name("tray".to_owned())
            .spawn(move || run(tx, wake, ready_tx))
            .ok()?;
        match ready_rx.recv() {
            Ok(icon) => Some(Self { icon, rx, thread: Some(thread) }),
            Err(_) => {
                let _ = thread.join();
                None
            }
        }
    }

    /// Commands since the last call.
    pub fn poll(&self) -> Vec<Command> {
        self.rx.try_iter().collect()
    }

    /// `show_balloon`: a notification from the tray icon.
    pub fn balloon(&self, title: &str, message: &str) {
        let mut nid = self.icon.data();
        nid.uFlags |= NIF_INFO;
        copy_wide(&mut nid.szInfo, message);
        copy_wide(&mut nid.szInfoTitle, title);
        nid.dwInfoFlags = NIIF_INFO;
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
    }
}

impl Drop for Tray {
    /// `destroy`: hotkeys unregistered, icon removed.
    fn drop(&mut self) {
        unsafe { PostMessageW(self.icon.hwnd as HWND, WM_CLOSE, 0, 0) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(tx: Sender<Command>, wake: impl Fn() + Send + 'static, ready: Sender<Icon>) {
    let class = wide("nvencFFX_tray");
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };
    // Fails harmlessly if a previous tray already registered the class
    unsafe { RegisterClassW(&wc) };
    // A hidden top-level window rather than a message-only one, which would miss
    // the "TaskbarCreated" broadcast
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return;
    }

    // The exe's icon (resource 1, see build.rs), else the stock application icon
    let (cx, cy) = unsafe { (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON)) };
    let mut hicon = unsafe { LoadImageW(instance, ICON_ID as usize as *const u16, IMAGE_ICON, cx, cy, 0) };
    let owned = !hicon.is_null();
    if !owned {
        hicon = unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) };
    }

    let icon = Icon { hwnd: hwnd as usize, hicon: hicon as usize };
    let state = State {
        icon,
        taskbar_created: unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) },
        send: Box::new(move |command| {
            if tx.send(command).is_ok() {
                wake();
            }
        }),
    };
    STATE.with(|s| *s.borrow_mut() = Some(Rc::new(state)));
    unsafe {
        // Another instance may hold them; that is ignored
        RegisterHotKey(hwnd, HOTKEY_START_ID, MOD_ALT | MOD_NOREPEAT, VK_F8 as u32);
        RegisterHotKey(hwnd, HOTKEY_STOP_ID, MOD_ALT | MOD_NOREPEAT, VK_F9 as u32);
    }
    icon.add();
    let _ = ready.send(icon);

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    STATE.with(|s| s.borrow_mut().take());
    if owned {
        unsafe { DestroyIcon(hicon) };
    }
}

fn show_menu(hwnd: HWND) -> Option<Command> {
    let items: [(usize, &str); 4] = [
        (IDM_START_RECORDING, "Start Recording (Alt + F8)"),
        (IDM_STOP_RECORDING, "Stop Recording (Alt + F9)"),
        (IDM_OPEN_APP, "Open nvencFFX"),
        (IDM_EXIT, "Exit"),
    ];
    let chosen = unsafe {
        let menu = CreatePopupMenu();
        for (i, (id, text)) in items.iter().enumerate() {
            if i == 2 {
                AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            }
            AppendMenuW(menu, MF_STRING, *id, wide(text).as_ptr());
        }
        // Required so the menu closes when clicking elsewhere
        SetForegroundWindow(hwnd);
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let chosen = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, pt.x, pt.y, 0, hwnd, std::ptr::null());
        // Lets the next right click open the menu again (KB135788)
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        chosen as usize
    };
    match chosen {
        IDM_START_RECORDING => Some(Command::StartRecording),
        IDM_STOP_RECORDING => Some(Command::StopRecording),
        IDM_OPEN_APP => Some(Command::Open),
        IDM_EXIT => Some(Command::Exit),
        _ => None,
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Cloned out so no borrow is held while the menu's modal loop re-enters here
    let Some(state) = STATE.with(|s| s.borrow().clone()) else {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    };
    match msg {
        WM_HOTKEY => {
            match wparam as i32 {
                HOTKEY_START_ID => (state.send)(Command::StartRecording),
                HOTKEY_STOP_ID => (state.send)(Command::StopRecording),
                _ => {}
            }
            0
        }
        WM_TRAY => {
            match (lparam & 0xffff) as u32 {
                WM_RBUTTONUP | WM_CONTEXTMENU => {
                    if let Some(command) = show_menu(hwnd) {
                        (state.send)(command);
                    }
                }
                WM_LBUTTONDBLCLK => (state.send)(Command::Open),
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                UnregisterHotKey(hwnd, HOTKEY_START_ID);
                UnregisterHotKey(hwnd, HOTKEY_STOP_ID);
                Shell_NotifyIconW(NIM_DELETE, &state.icon.data());
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        m if m == state.taskbar_created && m != 0 => {
            state.icon.add();
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn copy_wide_truncates_and_terminates() {
        let mut buf = [0xffffu16; 4];
        copy_wide(&mut buf, "abcdef");
        assert_eq!(buf, [97, 98, 99, 0]);
        copy_wide(&mut buf, "x");
        assert_eq!(&buf[..2], &[120, 0]);
    }

    #[test]
    fn hotkeys_and_tray_clicks_become_commands() {
        let (woken_tx, woken) = channel();
        let tray = Tray::start(move || woken_tx.send(()).unwrap()).expect("tray window");
        let hwnd = tray.icon.hwnd as HWND;
        unsafe {
            PostMessageW(hwnd, WM_HOTKEY, HOTKEY_START_ID as usize, 0);
            PostMessageW(hwnd, WM_HOTKEY, HOTKEY_STOP_ID as usize, 0);
            PostMessageW(hwnd, WM_TRAY, 0, WM_LBUTTONDBLCLK as isize);
        }
        for _ in 0..3 {
            woken.recv_timeout(Duration::from_secs(5)).expect("wake callback");
        }
        assert_eq!(tray.poll(), [Command::StartRecording, Command::StopRecording, Command::Open]);
        // Dropping removes the icon and ends the thread (the join would hang otherwise)
        drop(tray);
    }
}
