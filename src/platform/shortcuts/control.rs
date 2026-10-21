//! Control socket for one-shot shortcut commands.
//!
//! A Hyprland binding cannot reach a running app directly, so it runs
//! `lennette --toggle-window` and that forwards the command here. The listener
//! turns each line into an action the overlay applies on its next frame.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::sync::{Arc, Mutex};

use crate::platform::shortcuts::Action;

/// Commands received and not yet handled.
pub type Queue = Arc<Mutex<Vec<Action>>>;

/// A running control socket.
pub struct ControlListener {
    queue: Queue,
    path: std::path::PathBuf,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl ControlListener {
    /// Bind the socket and start accepting commands.
    pub fn start() -> Result<Self, String> {
        let path =
            crate::cli::control_socket().ok_or_else(|| "XDG_RUNTIME_DIR is not set".to_owned())?;
        // A stale socket from a previous run would block binding.
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).map_err(|error| error.to_string())?;

        let queue: Queue = Arc::new(Mutex::new(Vec::new()));
        let sink = queue.clone();
        let worker = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let reader = BufReader::new(stream);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if let Some(action) = parse_command(line.trim())
                        && let Ok(mut queue) = sink.lock()
                    {
                        queue.push(action);
                    }
                }
            }
        });

        Ok(Self {
            queue,
            path,
            worker: Some(worker),
        })
    }

    /// Take pending actions.
    pub fn drain(&self) -> Vec<Action> {
        self.queue
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

impl Drop for ControlListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        // The accept loop exits when the process ends; the handle is detached
        // rather than joined so shutdown never blocks on a blocked accept.
        let _ = self.worker.take();
    }
}

/// Map a forwarded command to an action.
pub fn parse_command(command: &str) -> Option<Action> {
    match command {
        "toggle-dashboard" => Some(Action::ToggleDashboard),
        "toggle-window" => Some(Action::ToggleWindow),
        "focus-input" => Some(Action::FocusInput),
        "toggle-system-audio" => Some(Action::SystemAudio),
        "toggle-voice" => Some(Action::VoiceInput),
        "screenshot" => Some(Action::Screenshot),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flag_round_trips() {
        for action in Action::ALL {
            let flag = match action {
                Action::ToggleDashboard => "toggle-dashboard",
                Action::ToggleWindow => "toggle-window",
                Action::FocusInput => "focus-input",
                Action::SystemAudio => "toggle-system-audio",
                Action::VoiceInput => "toggle-voice",
                Action::Screenshot => "screenshot",
            };
            assert_eq!(parse_command(flag), Some(action));
        }
    }

    #[test]
    fn unknown_commands_are_ignored() {
        assert_eq!(parse_command("rm -rf /"), None);
        assert_eq!(parse_command(""), None);
    }

    #[test]
    fn the_listener_binds_and_cleans_up() {
        // Only meaningful when a runtime dir exists.
        if crate::cli::control_socket().is_none() {
            return;
        }
        let path = crate::cli::control_socket().unwrap();
        let listener = match ControlListener::start() {
            Ok(listener) => listener,
            Err(_) => return,
        };
        assert!(path.exists());
        drop(listener);
        assert!(!path.exists(), "the socket should be removed on drop");
    }
}
