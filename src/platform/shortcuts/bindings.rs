//! Global shortcuts.
//!
//! GPUI has no system-wide hotkey support, so on X11 this grabs keys directly
//! through `XGrabKey` and listens for presses on a dedicated connection. That is
//! what makes a shortcut work while another application is focused.
//!
//! Wayland forbids this by design. Rather than pretend, [`support`] reports it and
//! the settings page says so.

use std::sync::mpsc as std_mpsc;
use std::thread;

/// A shortcut the user can trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    ToggleDashboard,
    ToggleWindow,
    FocusInput,
    SystemAudio,
    VoiceInput,
    Screenshot,
    PrivacyMode,
    NormalMode,
}

impl Action {
    pub const ALL: [Action; 6] = [
        Action::ToggleDashboard,
        Action::ToggleWindow,
        Action::FocusInput,
        Action::SystemAudio,
        Action::VoiceInput,
        Action::Screenshot,
    ];

    /// Stable identifier, matching the reference's binding ids.
    pub const fn id(self) -> &'static str {
        match self {
            Action::ToggleDashboard => "toggle_dashboard",
            Action::ToggleWindow => "toggle_window",
            Action::FocusInput => "focus_input",
            Action::SystemAudio => "system_audio",
            Action::VoiceInput => "audio_recording",
            Action::Screenshot => "screenshot",
            Action::PrivacyMode => "privacy_mode",
            Action::NormalMode => "normal_mode",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Action::ToggleDashboard => "Toggle Dashboard",
            Action::ToggleWindow => "Toggle Window",
            Action::FocusInput => "Refocus Input",
            Action::SystemAudio => "System Audio",
            Action::VoiceInput => "Voice Input",
            Action::Screenshot => "Screenshot",
            Action::PrivacyMode => "Privacy mode",
            Action::NormalMode => "Normal mode",
        }
    }

    pub fn from_id(id: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|action| action.id() == id)
    }
}

/// A parsed key combination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub action: Action,
    pub display: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// Key name in the form the X11 layer maps, e.g. `d`, `backslash`, `f1`.
    pub key: String,
}

impl Binding {
    /// Default bindings, matching the reference build.
    pub fn defaults() -> Vec<Binding> {
        [
            (Action::ToggleDashboard, "d"),
            (Action::ToggleWindow, "backslash"),
            (Action::FocusInput, "i"),
            (Action::SystemAudio, "m"),
            (Action::VoiceInput, "a"),
            (Action::Screenshot, "s"),
        ]
        .into_iter()
        .filter_map(|(action, key)| Binding::parse(action, &format!("ctrl+shift+{key}")).ok())
        .collect()
    }

    /// Parse `ctrl+shift+d`. `cmd`/`super` are treated as ctrl on Linux.
    pub fn parse(action: Action, spec: &str) -> Result<Binding, String> {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut key: Option<String> = None;

        for part in spec.split('+').map(|part| part.trim().to_lowercase()) {
            if part.is_empty() {
                continue;
            }
            match part.as_str() {
                "ctrl" | "control" | "cmd" | "command" | "meta" | "super" => ctrl = true,
                "shift" => shift = true,
                "alt" | "option" => alt = true,
                other => {
                    if key.replace(other.to_owned()).is_some() {
                        return Err(format!("'{spec}' names more than one key"));
                    }
                }
            }
        }

        let key = key.ok_or_else(|| format!("'{spec}' has no main key"))?;
        if !ctrl && !shift && !alt {
            return Err(format!("'{spec}' needs at least one modifier"));
        }
        let valid = key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric())
            || matches!(key.as_str(), "backslash" | "space" | "enter")
            || (key.starts_with('f')
                && key[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)));
        if !valid {
            return Err(format!("'{key}' is not a supported key"));
        }

        Ok(Binding {
            action,
            display: format_display(ctrl, shift, alt, &key),
            ctrl,
            shift,
            alt,
            key,
        })
    }

    /// Validate a user-entered combination.
    pub fn validate(spec: &str) -> Result<(), String> {
        Binding::parse(Action::ToggleWindow, spec).map(|_| ())
    }
}

fn format_display(ctrl: bool, shift: bool, alt: bool, key: &str) -> String {
    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if shift {
        parts.push("Shift");
    }
    if alt {
        parts.push("Alt");
    }
    let key = match key {
        "backslash" => "\\".to_owned(),
        "space" => "Space".to_owned(),
        "enter" => "Enter".to_owned(),
        other => other.to_uppercase(),
    };
    parts.push(&key);
    parts.join("+")
}

/// Whether global shortcuts can work in this session.
pub fn support() -> (bool, &'static str) {
    if cfg!(windows) {
        return (true, "Global shortcuts use Windows hotkeys.");
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("DISPLAY").is_none() {
        return (
            true,
            "Global shortcuts use the desktop's shortcut portal. Change the keys in your \
             desktop's own shortcuts settings.",
        );
    }
    if cfg!(target_os = "linux") {
        (true, "Global shortcuts use X11 key grabs.")
    } else {
        (
            false,
            "Global shortcuts are not implemented on this platform yet.",
        )
    }
}

/// A running global shortcut listener.
pub struct ShortcutListener {
    events: std_mpsc::Receiver<Action>,
    stop: Option<std_mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ShortcutListener {
    /// Grab every binding and start listening.
    ///
    /// Returns the bindings that could not be grabbed alongside the listener, so
    /// a conflict is reported rather than silently dropped.
    #[cfg(any(target_os = "linux", windows))]
    pub fn start(bindings: &[Binding]) -> Result<(Self, Vec<String>), String> {
        let (event_tx, event_rx) = std_mpsc::channel();
        let (stop_tx, stop_rx) = std_mpsc::channel();
        let (ready_tx, ready_rx) = std_mpsc::sync_channel::<Result<Vec<String>, String>>(1);
        let bindings = bindings.to_vec();

        let worker = thread::spawn(move || {
            #[cfg(target_os = "linux")]
            x11::run(bindings, event_tx, stop_rx, ready_tx);
            #[cfg(windows)]
            windows::run(bindings, event_tx, stop_rx, ready_tx);
        });

        match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(failed)) => Ok((
                Self {
                    events: event_rx,
                    stop: Some(stop_tx),
                    worker: Some(worker),
                },
                failed,
            )),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => {
                let _ = stop_tx.send(());
                Err("the shortcut listener did not start in time".to_owned())
            }
        }
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    pub fn start(_bindings: &[Binding]) -> Result<(Self, Vec<String>), String> {
        Err("global shortcuts are not implemented on this platform yet".to_owned())
    }

    /// Blocking receive with a timeout, for polling from a UI frame.
    pub fn next_action(&self, timeout: std::time::Duration) -> Option<Action> {
        self.events.recv_timeout(timeout).ok()
    }

    /// Non-blocking receive.
    pub fn try_action(&self) -> Option<Action> {
        self.events.try_recv().ok()
    }
}

#[cfg(windows)]
#[path = "windows.rs"]
mod windows;

impl Drop for ShortcutListener {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use super::{Action, Binding};
    use std::sync::mpsc as std_mpsc;
    use std::time::Duration;
    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{ConnectionExt, GrabMode, ModMask};

    /// X11 modifier mask bits.
    const SHIFT: u16 = 1;
    const LOCK: u16 = 2;
    const CONTROL: u16 = 4;
    const MOD1: u16 = 8;

    pub(super) fn run(
        bindings: Vec<Binding>,
        events: std_mpsc::Sender<Action>,
        stop: std_mpsc::Receiver<()>,
        ready: std_mpsc::SyncSender<Result<Vec<String>, String>>,
    ) {
        let connection = match x11rb::connect(None) {
            Ok((connection, screen)) => (connection, screen),
            Err(error) => {
                let _ = ready.send(Err(format!("cannot reach the display: {error}")));
                return;
            }
        };
        let (connection, screen) = connection;
        let root = connection
            .setup()
            .roots
            .get(screen)
            .map(|screen| screen.root)
            .unwrap_or_default();

        // Keycode lookup needs the keyboard mapping.
        let min_keycode = connection.setup().min_keycode;
        let max_keycode = connection.setup().max_keycode;
        let count = max_keycode - min_keycode + 1;
        let mapping = match connection.get_keyboard_mapping(min_keycode, count) {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => reply,
                Err(error) => {
                    let _ = ready.send(Err(format!("cannot read the keymap: {error}")));
                    return;
                }
            },
            Err(error) => {
                let _ = ready.send(Err(format!("cannot read the keymap: {error}")));
                return;
            }
        };
        let per_code = mapping.keysyms_per_keycode as usize;

        let mut grabbed: Vec<(u16, u16, Action)> = Vec::new();
        let mut failed = Vec::new();
        for binding in &bindings {
            let Some(sym) = keysym(&binding.key) else {
                failed.push(format!("{} (unknown key)", binding.display));
                continue;
            };
            let Some(keycode) = mapping
                .keysyms
                .chunks(per_code)
                .position(|row| row.contains(&sym))
                .map(|index| min_keycode + index as u8)
            else {
                failed.push(format!("{} (not on this keyboard)", binding.display));
                continue;
            };

            let mut mods = 0u16;
            if binding.ctrl {
                mods |= CONTROL;
            }
            if binding.shift {
                mods |= SHIFT;
            }
            if binding.alt {
                mods |= MOD1;
            }

            // Grab with and without Caps Lock / Num Lock, or the shortcut silently
            // stops working when a lock key is on.
            let mut ok = true;
            for extra in [0u16, LOCK, 16] {
                let mask = ModMask::from(mods | extra);
                // `.check()` reports a protocol error (key already grabbed by
                // another client) as a ReplyError, which is a different type from
                // a connection failure, so both are flattened to "failed".
                let result = connection
                    .grab_key(false, root, mask, keycode, GrabMode::ASYNC, GrabMode::ASYNC)
                    .map_err(|error| error.to_string())
                    .and_then(|cookie| cookie.check().map_err(|error| error.to_string()));
                if result.is_err() {
                    ok = false;
                    break;
                }
                grabbed.push((keycode as u16, mods | extra, binding.action));
            }
            if !ok {
                failed.push(format!(
                    "{} (already taken by another app)",
                    binding.display
                ));
            }
        }
        let _ = connection.flush();
        let _ = ready.send(Ok(failed));

        // Drain any queued events, then block for the next one.
        loop {
            if stop.try_recv().is_ok() {
                break;
            }
            match connection.poll_for_event() {
                Ok(Some(Event::KeyPress(event))) => {
                    let code = event.detail as u16;
                    let matched = grabbed.iter().find(|(keycode, mask, _)| {
                        *keycode == code && (u16::from(event.state) & !LOCK) == (*mask & !LOCK)
                    });
                    if let Some((_, _, action)) = matched
                        && events.send(*action).is_err()
                    {
                        break;
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    // Nothing pending: nap briefly instead of busy-looping, which
                    // keeps idle CPU near zero.
                    thread_sleep();
                }
                Err(_) => break,
            }
        }

        for (keycode, mask, _) in grabbed {
            let _ = connection.ungrab_key(keycode as u8, root, ModMask::from(mask));
        }
        let _ = connection.flush();
    }

    fn thread_sleep() {
        thread::sleep(Duration::from_millis(20));
    }

    use std::thread;

    /// X11 keysym for a supported key name.
    fn keysym(key: &str) -> Option<u32> {
        if key.len() == 1 {
            let c = key.chars().next()?;
            // Latin-1 printable keysyms equal their codepoint.
            return Some(c.to_ascii_lowercase() as u32);
        }
        if let Some(number) = key.strip_prefix('f') {
            let n: u32 = number.parse().ok()?;
            if (1..=24).contains(&n) {
                return Some(0xffbe + (n - 1));
            }
        }
        match key {
            "backslash" => Some(0x005c),
            "space" => Some(0x0020),
            "enter" => Some(0xff0d),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_bindings() {
        let defaults = Binding::defaults();
        assert_eq!(defaults.len(), 6);
        let dashboard = &defaults[0];
        assert_eq!(dashboard.action, Action::ToggleDashboard);
        assert!(dashboard.ctrl && dashboard.shift && !dashboard.alt);
        assert_eq!(dashboard.key, "d");
        assert_eq!(dashboard.display, "Ctrl+Shift+D");
    }

    #[test]
    fn backslash_is_readable() {
        let binding = Binding::parse(Action::ToggleWindow, "ctrl+backslash").unwrap();
        assert_eq!(binding.display, "Ctrl+\\");
        assert_eq!(binding.key, "backslash");
    }

    #[test]
    fn cmd_is_treated_as_ctrl() {
        let binding = Binding::parse(Action::Screenshot, "cmd+shift+s").unwrap();
        assert!(binding.ctrl);
        assert_eq!(binding.display, "Ctrl+Shift+S");
    }

    #[test]
    fn rejects_invalid_combinations() {
        assert!(Binding::parse(Action::ToggleWindow, "").is_err());
        assert!(
            Binding::parse(Action::ToggleWindow, "d").is_err(),
            "no modifier"
        );
        assert!(
            Binding::parse(Action::ToggleWindow, "ctrl+d+e").is_err(),
            "two keys"
        );
        assert!(Binding::parse(Action::ToggleWindow, "ctrl+nope").is_err());
        assert!(Binding::validate("ctrl+shift+k").is_ok());
    }

    #[test]
    fn function_keys_are_supported() {
        assert!(Binding::parse(Action::Screenshot, "ctrl+f12").is_ok());
        assert!(Binding::parse(Action::Screenshot, "ctrl+f25").is_err());
    }

    #[test]
    fn action_ids_round_trip() {
        for action in Action::ALL {
            assert_eq!(Action::from_id(action.id()), Some(action));
            assert!(!action.label().is_empty());
        }
        assert_eq!(Action::from_id("nope"), None);
    }

    #[test]
    fn support_report_is_informative() {
        let (supported, message) = support();
        assert!(!message.is_empty());
        assert!(supported || message.contains("not"));
    }
}
