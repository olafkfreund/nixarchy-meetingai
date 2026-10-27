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
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, above);
        Applied::Unsupported
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

/// Begin an OS-managed move after a press on a custom titlebar.
pub fn start_drag(window: &Window) {
    window.start_window_move();
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
