//! Native notification-area icon with a thread-owned Win32 message pump.
use super::{TrayAction, TrayBackend, icon_at};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use windows_sys::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Shell::*, WindowsAndMessaging::*},
};
use windows_sys::core::w;

unsafe extern "system" fn tray_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Shell callbacks may be sent synchronously. Forward them to queued messages
    // so the worker observes them whether Explorer sends or posts the callback.
    let queued = if message == WM_APP + 1 {
        Some(WM_APP + 2)
    } else if message == unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) } {
        Some(WM_APP + 3)
    } else {
        None
    };
    if let Some(message) = queued {
        unsafe {
            PostMessageW(hwnd, message, wparam, lparam);
        }
        0
    } else {
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }
}

pub struct Backend {
    queue: Arc<Mutex<Vec<TrayAction>>>,
    tooltip: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl TrayBackend for Backend {
    fn poll(&self) -> Vec<TrayAction> {
        self.queue
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }
    fn set_tooltip(&self, text: &str) {
        if let Ok(mut tip) = self.tooltip.lock() {
            *tip = text.into();
        }
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn start() -> Result<Backend, String> {
    let queue: Arc<Mutex<Vec<TrayAction>>> = Default::default();
    let tooltip = Arc::new(Mutex::new("lennette".to_owned()));
    let stop = Arc::new(AtomicBool::new(false));
    let (sink, tip, halt) = (queue.clone(), tooltip.clone(), stop.clone());
    let (ready, started) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        // SAFETY: all Win32 resources below are created, used and destroyed on
        // this thread; no handle or borrowed pointer escapes the worker.
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class = WNDCLASSW {
                lpfnWndProc: Some(tray_window_proc),
                hInstance: instance,
                lpszClassName: w!("lennette.native.tray"),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                let _ = ready.send(Err(std::io::Error::last_os_error().to_string()));
                return;
            }
            let hwnd = CreateWindowExW(
                0,
                w!("lennette.native.tray"),
                w!("lennette tray"),
                0,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                let _ = ready.send(Err(std::io::Error::last_os_error().to_string()));
                return;
            }
            let mut pixels = icon_at(32);
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            let mask = [0u8; 128];
            let icon = CreateIcon(
                std::ptr::null_mut(),
                32,
                32,
                1,
                32,
                mask.as_ptr(),
                pixels.as_ptr(),
            );
            if icon.is_null() {
                let _ = ready.send(Err(std::io::Error::last_os_error().to_string()));
                DestroyWindow(hwnd);
                return;
            }
            let mut data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                uCallbackMessage: WM_APP + 1,
                hIcon: icon,
                ..Default::default()
            };
            for (dst, ch) in data.szTip.iter_mut().zip("lennette".encode_utf16()) {
                *dst = ch;
            }
            if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
                let _ = ready.send(Err("Windows notification area is unavailable".into()));
                DestroyIcon(icon);
                DestroyWindow(hwnd);
                return;
            }
            let _ = ready.send(Ok(()));
            let mut last_tip = "lennette".to_owned();
            while !halt.load(Ordering::Relaxed) {
                if let Ok(current) = tip.lock()
                    && *current != last_tip
                {
                    last_tip.clone_from(&current);
                    data.szTip.fill(0);
                    for (dst, ch) in data.szTip[..127].iter_mut().zip(current.encode_utf16()) {
                        *dst = ch;
                    }
                    Shell_NotifyIconW(NIM_MODIFY, &data);
                }
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if msg.message == WM_APP + 3 {
                        Shell_NotifyIconW(NIM_ADD, &data);
                    } else if msg.message == WM_APP + 2 {
                        let action = match msg.lParam as u32 {
                            WM_LBUTTONUP | WM_LBUTTONDBLCLK => Some(TrayAction::ShowOverlay),
                            WM_RBUTTONUP => {
                                let menu = CreatePopupMenu();
                                if !menu.is_null() {
                                    AppendMenuW(menu, MF_STRING, 1, w!("Show overlay"));
                                    AppendMenuW(menu, MF_STRING, 2, w!("Open dashboard"));
                                    AppendMenuW(menu, MF_STRING, 3, w!("Start or stop listening"));
                                    AppendMenuW(
                                        menu,
                                        MF_STRING,
                                        5,
                                        w!("Go stealth (Privacy mode)"),
                                    );
                                    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                                    AppendMenuW(menu, MF_STRING, 4, w!("Quit lennette"));
                                    let mut point = POINT::default();
                                    GetCursorPos(&mut point);
                                    SetForegroundWindow(hwnd);
                                    let choice = TrackPopupMenu(
                                        menu,
                                        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                                        point.x,
                                        point.y,
                                        0,
                                        hwnd,
                                        std::ptr::null(),
                                    );
                                    DestroyMenu(menu);
                                    PostMessageW(hwnd, WM_NULL, 0, 0);
                                    match choice {
                                        1 => Some(TrayAction::ShowOverlay),
                                        2 => Some(TrayAction::OpenDashboard),
                                        3 => Some(TrayAction::ToggleListening),
                                        4 => Some(TrayAction::Quit),
                                        5 => Some(TrayAction::PrivacyMode),
                                        _ => None,
                                    }
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };
                        if let Some(action) = action
                            && let Ok(mut q) = sink.lock()
                        {
                            q.push(action);
                        }
                    } else {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Shell_NotifyIconW(NIM_DELETE, &data);
            DestroyIcon(icon);
            DestroyWindow(hwnd);
        }
    });
    match started.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(Backend {
            queue,
            tooltip,
            stop,
            worker: Some(worker),
        }),
        result => {
            stop.store(true, Ordering::Relaxed);
            Err(match result {
                Ok(Err(error)) => error,
                _ => "Windows tray startup timed out".into(),
            })
        }
    }
}
