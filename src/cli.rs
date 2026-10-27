//! Command line entry point.
//!
//! Besides launching normally, nixarchy-meetingai accepts one-shot control flags. Hyprland
//! binds call these, so a shortcut reaches the running instance instead of
//! starting a second copy.

/// A one-shot command requested by a shortcut.
///
/// Returns `None` when this invocation should launch the app instead: no control
/// flag, or a toggle requested while nothing is listening on the socket. That
/// second case is what makes a toggle key launch the app when it is not running;
/// forwarding to nobody and exiting would leave the key doing nothing.
pub fn handle_args() -> Option<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = args.first()?;
    let command = match flag.as_str() {
        "--toggle-dashboard" => "toggle-dashboard",
        "--toggle-window" => "toggle-window",
        "--focus-input" => "focus-input",
        "--toggle-system-audio" => "toggle-system-audio",
        "--toggle-voice" => "toggle-voice",
        "--screenshot" => "screenshot",
        "--privacy" => "privacy-mode",
        "--normal" => "normal-mode",
        "--version" | "-V" => return Some(format!("nixarchy-meetingai {}", env!("CARGO_PKG_VERSION"))),
        "--help" | "-h" => {
            return Some(usage());
        }
        _ => return None,
    };

    // Forward to the running instance over the shortcut socket, so bindings never
    // spawn a duplicate window.
    match send(command) {
        Ok(()) => Some(String::new()),
        Err(error) if launches_when_absent(command) => {
            tracing::debug!(%error, %command, "no instance accepted the command; launching");
            None
        }
        Err(error) => Some(format!("nixarchy-meetingai: {error}")),
    }
}

/// Toggles are safe to treat as "start the app" when no instance is listening.
fn launches_when_absent(command: &str) -> bool {
    matches!(
        command,
        "toggle-window" | "toggle-dashboard" | "focus-input" | "privacy-mode" | "normal-mode"
    )
}

fn usage() -> String {
    [
        "nixarchy-meetingai — desktop assistant",
        "",
        "Usage: nixarchy-meetingai [option]",
        "",
        "  --toggle-dashboard      show or hide the dashboard",
        "  --toggle-window         show or hide the overlay",
        "  --focus-input           focus the composer",
        "  --toggle-system-audio   capture the far side of the call",
        "  --toggle-voice          start or stop listening",
        "  --screenshot            capture the screen",
        "  --version               print the version",
        "  --help                  print this help",
        "",
        "Run without a flag to launch the app.",
        "NIXARCHY_MEETINGAI_PRIVATE_OVERLAY=1 disables the desktop tray for protected launches.",
        "  --privacy               launch without a tray or taskbar icon",
        "  --normal                launch with a tray icon (default)",
    ]
    .join("\n")
}

/// Explorer owns tray pixels, so app-window capture exclusion cannot hide them.
/// Privacy launches omit the tray; normal launches include it.
pub fn private_overlay_enabled() -> bool {
    MODE.get_or_init(|| std::sync::atomic::AtomicBool::new(launch_private()))
        .load(std::sync::atomic::Ordering::Relaxed)
}

static MODE: std::sync::OnceLock<std::sync::atomic::AtomicBool> = std::sync::OnceLock::new();
pub fn set_private_mode(private: bool) {
    MODE.get_or_init(|| std::sync::atomic::AtomicBool::new(launch_private()))
        .store(private, std::sync::atomic::Ordering::Relaxed);
}

fn launch_private() -> bool {
    if std::env::args().any(|arg| arg == "--privacy") {
        return true;
    }
    if std::env::args().any(|arg| arg == "--normal") {
        return false;
    }
    private_overlay_for(
        cfg!(windows),
        std::env::var("NIXARCHY_MEETINGAI_PRIVATE_OVERLAY").ok().as_deref(),
    )
}

fn private_overlay_for(_windows: bool, value: Option<&str>) -> bool {
    match value {
        Some("1") => true,
        Some("0") => false,
        _ => false,
    }
}

/// If an instance is already running, activate its window and return true.
pub fn activate_existing_instance() -> bool {
    send("focus-input").is_ok()
}

/// Wait for the instance holding the lock to become reachable, then focus it.
///
/// A launch that loses the race may face an instance that has claimed the lock
/// but not yet bound its socket. Retrying turns that into a normal "bring it up"
/// instead of a silent no-op.
pub fn wait_for_running_instance(timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if send("focus-input").is_ok() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Path of the control socket shared with the running instance.
pub fn control_socket() -> Option<std::path::PathBuf> {
    runtime_path("nixarchy-meetingai-shortcut.sock")
}

/// Path of the exclusive launch lock held by the running instance.
pub fn instance_lock_path() -> Option<std::path::PathBuf> {
    runtime_path("nixarchy-meetingai-instance.lock")
}

fn runtime_path(name: &str) -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)?;
    #[cfg(windows)]
    let runtime = directories::ProjectDirs::from("dev", "nixarchy", "nixarchy-meetingai")?
        .data_local_dir()
        .join("runtime");
    std::fs::create_dir_all(&runtime).ok()?;
    Some(runtime.join(name))
}

/// Exclusive claim on being the one running instance.
///
/// A socket probe alone cannot stop a double launch: two processes can both
/// find no listener and both start. `flock` is atomic, so exactly one launch
/// obtains the lock and every later launch sees it taken. The kernel releases
/// the lock when the process exits, so a crash never leaves it stuck.
pub struct InstanceLock {
    _file: std::fs::File,
}

impl InstanceLock {
    /// Claim the instance slot, or return `None` when another instance holds it.
    pub fn acquire() -> Option<Self> {
        Self::acquire_at(&instance_lock_path()?)
    }

    /// Claim an explicit lock path. Used by tests to avoid the live instance.
    fn acquire_at(path: &std::path::Path) -> Option<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|error| tracing::warn!(%error, path = %path.display(), "could not open instance lock"))
            .ok()?;

        // std uses an OS file lock on Unix and Windows, released on crash too.
        file.try_lock().ok()?;

        let _ = file.set_len(0);
        use std::io::Write;
        let mut writer = &file;
        let _ = writeln!(writer, "{}", std::process::id());
        Some(Self { _file: file })
    }
}

#[cfg(unix)]
fn send(command: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    let path = control_socket().ok_or("XDG_RUNTIME_DIR is not set")?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|_| "no running nixarchy-meetingai instance was found".to_owned())?;
    let payload = if command.ends_with('\n') {
        command.to_owned()
    } else {
        format!("{command}\n")
    };
    stream
        .write_all(payload.as_bytes())
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn send(command: &str) -> Result<(), String> {
    use std::io::Write;
    let name = crate::platform::control::pipe_name()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let mut pipe = loop {
        match std::fs::OpenOptions::new().write(true).open(&name) {
            Ok(pipe) => break pipe,
            Err(error)
                if error.raw_os_error()
                    == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32)
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(_) => return Err("no running nixarchy-meetingai instance was found".to_owned()),
        }
    };
    writeln!(pipe, "{command}").map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_launch_has_a_tray_unless_privacy_is_requested() {
        for value in [None, Some(""), Some("invalid")] {
            assert!(!private_overlay_for(true, value));
        }
        assert!(private_overlay_for(true, Some("1")));
        assert!(!private_overlay_for(true, Some("0")));
        assert!(!private_overlay_for(false, None));
        assert!(private_overlay_for(false, Some("1")));
        assert!(!private_overlay_for(false, Some("0")));
    }

    #[test]
    fn control_socket_lives_in_the_runtime_dir() {
        // The function reads the environment, so only assert its shape when set.
        if control_socket().is_some() {
            let path = control_socket().unwrap();
            assert!(path.ends_with("nixarchy-meetingai-shortcut.sock"));
        }
    }

    #[test]
    fn instance_lock_exists_in_the_runtime_dir() {
        if let Some(path) = instance_lock_path() {
            assert!(path.ends_with("nixarchy-meetingai-instance.lock"));
        }
    }

    #[test]
    fn only_one_instance_may_hold_the_lock() {
        // Use a private lock file: the developer's own running app legitimately
        // holds the real one, and the test must not depend on that.
        let path = std::env::temp_dir().join(format!(
            "nixarchy-meetingai-lock-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);

        let first = InstanceLock::acquire_at(&path).expect("the slot should be free");
        // While held, a second claim must fail; that is what stops a double launch.
        assert!(
            InstanceLock::acquire_at(&path).is_none(),
            "a second instance was allowed"
        );
        drop(first);
        // Releasing returns the slot, so the app can start again after quitting.
        assert!(
            InstanceLock::acquire_at(&path).is_some(),
            "the slot should be reusable"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn toggles_heal_a_missing_instance_by_launching() {
        // A toggle must start the app when nothing is listening, or the bound key
        // silently does nothing before the first launch.
        for command in ["toggle-window", "toggle-dashboard", "focus-input"] {
            assert!(launches_when_absent(command), "{command} should launch");
        }
        // A failed one-shot must not launch: the user asked for an action, not a start.
        for command in ["screenshot", "toggle-voice", "toggle-system-audio"] {
            assert!(
                !launches_when_absent(command),
                "{command} should not launch"
            );
        }
    }

    #[test]
    fn usage_lists_every_shortcut_flag() {
        let text = usage();
        for flag in [
            "--toggle-dashboard",
            "--toggle-window",
            "--focus-input",
            "--toggle-system-audio",
            "--toggle-voice",
            "--screenshot",
        ] {
            assert!(text.contains(flag), "usage is missing {flag}");
        }
    }
}
