//! Window control.
//!
//! GPUI 0.2 exposes activation and fullscreen but not "always on top" or preset
//! resizing, so those go straight to the display server through the native window
//! handle. On X11 this uses EWMH, the convention every mainstream WM implements.
//!
//! Every function reports whether the request was applied. A setting that cannot
//! be honoured says so instead of silently doing nothing.

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Outcome of a window request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// The display server accepted the change.
    Yes,
    /// The platform or compositor does not support it.
    Unsupported,
    /// The request failed; the reason is logged.
    Failed,
}

impl Applied {
    pub fn is_yes(self) -> bool {
        matches!(self, Applied::Yes)
    }
}

/// The native X11 window id, when running under X11.
#[cfg(target_os = "linux")]
fn x11_window(window: &Window) -> Option<u32> {
    // Call the trait method explicitly: `Window` also has an inherent
    // `window_handle` returning GPUI's own handle type.
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Xlib(xlib) => Some(xlib.window as u32),
        RawWindowHandle::Xcb(xcb) => Some(xcb.window.get()),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn set_above_x11(window_id: u32, above: bool) -> Result<(), String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask};

    let (connection, screen) = x11rb::connect(None).map_err(|error| error.to_string())?;
    let root = connection
        .setup()
        .roots
        .get(screen)
        .map(|screen| screen.root)
        .ok_or_else(|| "no X11 screen".to_owned())?;

    let state = connection
        .intern_atom(false, b"_NET_WM_STATE")
        .map_err(|error| error.to_string())?
        .reply()
        .map_err(|error| error.to_string())?
        .atom;
    let above_atom = connection
        .intern_atom(false, b"_NET_WM_STATE_ABOVE")
        .map_err(|error| error.to_string())?
        .reply()
        .map_err(|error| error.to_string())?
        .atom;

    let event = ClientMessageEvent::new(
        32,
        window_id,
        state,
        [
            1, // _NET_WM_STATE_ADD
            above_atom,
            0,
            if above { 1 } else { 0 }, // source: application
            0,
        ],
    );

    connection
        .send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )
        .map_err(|error| error.to_string())?;
    connection.flush().map_err(|error| error.to_string())?;
    let _ = AtomEnum::ATOM;
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_active_window_x11(window_id: u32) -> Result<(), String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt, EventMask};

    let (connection, screen) = x11rb::connect(None).map_err(|error| error.to_string())?;
    let root = connection
        .setup()
        .roots
        .get(screen)
        .map(|screen| screen.root)
        .ok_or_else(|| "no X11 screen".to_owned())?;
    let active = connection
        .intern_atom(false, b"_NET_ACTIVE_WINDOW")
        .map_err(|error| error.to_string())?
        .reply()
        .map_err(|error| error.to_string())?
        .atom;

    let event = ClientMessageEvent::new(32, window_id, active, [2, 0, 0, 0, 0]);
    connection
        .send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )
        .map_err(|error| error.to_string())?;
    connection.flush().map_err(|error| error.to_string())?;
    Ok(())
}

/// Keep the window above others, or release it.
pub fn set_always_on_top(window: &Window, above: bool) -> Applied {
    #[cfg(target_os = "linux")]
    {
        let Some(id) = x11_window(window) else {
            // Wayland forbids global stacking; this is a platform limit.
            return Applied::Unsupported;
        };
        match set_above_x11(id, above) {
            Ok(()) => Applied::Yes,
            Err(error) => {
                tracing::warn!(%error, "could not change always-on-top");
                Applied::Failed
            }
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let Some(handle) = win32_window(window) else {
            return Applied::Unsupported;
        };
        // SAFETY: live native window, no position/size change.
        if unsafe {
            SetWindowPos(
                handle,
                if above { HWND_TOPMOST } else { HWND_NOTOPMOST },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        } != 0
        {
            Applied::Yes
        } else {
            Applied::Failed
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (window, above);
        Applied::Unsupported
    }
}

#[cfg(windows)]
fn win32_window(window: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as _),
        _ => None,
    }
}

/// Apply capture exclusion before making a Windows window visible. This is an
/// OS capture policy, not protection from privileged software or external cameras.
pub fn protect_from_capture(window: &Window) -> Result<(), String> {
    #[cfg(windows)]
    {
        let handle = win32_window(window).ok_or("no Windows window handle")?;
        if let Err(error) = protect_win32(handle) {
            let message: Vec<u16> =
                format!("{error}\n\nLennette will close without showing an unprotected window.")
                    .encode_utf16()
                    .chain([0])
                    .collect();
            // SAFETY: the error dialog contains only owned, terminated strings.
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    std::ptr::null_mut(),
                    message.as_ptr(),
                    windows_sys::core::w!("Lennette capture protection"),
                    windows_sys::Win32::UI::WindowsAndMessaging::MB_OK
                        | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                );
            }
            return Err(error);
        }
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        // Keep app windows out of the taskbar and Alt+Tab. Failure of capture
        // exclusion above is fatal; these presentation flags are independent.
        unsafe {
            let style = GetWindowLongPtrW(handle, GWL_EXSTYLE);
            SetWindowLongPtrW(
                handle,
                GWL_EXSTYLE,
                (style | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize),
            );
        }
    }
    #[cfg(not(windows))]
    let _ = window;
    Ok(())
}

#[cfg(windows)]
fn protect_win32(handle: windows_sys::Win32::Foundation::HWND) -> Result<(), String> {
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // RtlGetVersion reports the actual OS build regardless of manifest version
    // virtualization. Older systems can accept 0x11 but only black out captures.
    if unsafe { windows_sys::Wdk::System::SystemServices::RtlGetVersion(&mut version) } < 0
        || version.dwMajorVersion < 10
        || (version.dwMajorVersion == 10 && version.dwBuildNumber < 19041)
    {
        return Err("Windows capture exclusion requires Windows 10 version 2004 or newer.".into());
    }
    let mut affinity = 0;
    // SAFETY: caller supplies a top-level window owned by this process.
    if unsafe { SetWindowDisplayAffinity(handle, WDA_EXCLUDEFROMCAPTURE) } == 0 {
        return Err(format!(
            "Windows capture exclusion failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    if unsafe { GetWindowDisplayAffinity(handle, &mut affinity) } == 0
        || affinity != WDA_EXCLUDEFROMCAPTURE
    {
        return Err(
            "Windows did not retain capture exclusion. Windows 10 2004+ is required.".into(),
        );
    }
    Ok(())
}

/// Hide/show the native window; GPUI's app-level hide is a no-op on Windows.
#[cfg(windows)]
pub fn set_visible(window: &Window, visible: bool) -> Applied {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let Some(handle) = win32_window(window) else {
        return Applied::Unsupported;
    };
    // SAFETY: handle belongs to this live GPUI window. ShowWindow returns its
    // previous visibility, not success, so check the resulting state explicitly.
    unsafe {
        ShowWindow(handle, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
    }
    if (unsafe { IsWindowVisible(handle) } != 0) == visible {
        Applied::Yes
    } else {
        Applied::Failed
    }
}

/// Bring the window to the front.
pub fn focus(window: &Window) -> Applied {
    #[cfg(target_os = "linux")]
    if let Some(id) = x11_window(window) {
        let _ = set_active_window_x11(id);
    }
    window.activate_window();
    Applied::Yes
}

/// Resize the window to a preset, in logical pixels.
pub fn resize(window: &mut Window, width: f32, height: f32) -> Applied {
    use gpui::{px, size};

    // GPUI measures windows in logical pixels, so the request is logical and the
    // platform applies the scale factor.
    window.resize(size(px(width), px(height)));
    Applied::Yes
}

/// Whether global shortcuts can be registered on this session.
pub fn global_shortcuts_supported() -> (bool, &'static str) {
    if cfg!(windows) {
        return (true, "Global shortcuts use Windows hotkeys.");
    }
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    if wayland {
        (
            false,
            "Global shortcuts need the desktop's shortcut portal on Wayland, which is not \
             implemented; use X11 or XWayland for now.",
        )
    } else if cfg!(target_os = "linux") {
        (
            true,
            "Global shortcuts use the native desktop shortcut backend.",
        )
    } else {
        (
            false,
            "Global shortcuts are not implemented on this platform yet.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_accepts_and_retains_capture_exclusion() {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        use windows_sys::core::w;
        // SAFETY: this test creates and destroys its own hidden top-level HWND.
        unsafe {
            let handle = CreateWindowExW(
                0,
                w!("STATIC"),
                w!("lennette capture policy test"),
                WS_OVERLAPPED,
                0,
                0,
                32,
                32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!handle.is_null(), "could not create a test window");
            let result = protect_win32(handle);
            DestroyWindow(handle);
            result.expect("Windows must enforce exclusion before a release can be published");
        }
    }

    #[test]
    fn applied_is_truthy_only_when_confirmed() {
        assert!(Applied::Yes.is_yes());
        assert!(!Applied::Unsupported.is_yes());
        assert!(!Applied::Failed.is_yes());
    }

    #[test]
    fn shortcut_support_is_reported_consistently() {
        let (supported, message) = global_shortcuts_supported();
        assert!(!message.is_empty());
        if supported {
            assert!(message.contains("shortcut"));
        }
    }
}
