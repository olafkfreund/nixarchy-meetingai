//! Command line entry point.
//!
//! Besides launching normally, lennette accepts one-shot control flags. Hyprland
//! binds call these, so a shortcut reaches the running instance instead of
//! starting a second copy.

/// A one-shot command requested by a shortcut.
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
        "--version" | "-V" => return Some(format!("lennette {}", env!("CARGO_PKG_VERSION"))),
        "--help" | "-h" => {
            return Some(usage());
        }
        _ => return None,
    };

    // Forward to the running instance over the shortcut socket, so bindings never
    // spawn a duplicate window.
    match send(command) {
        Ok(()) => Some(String::new()),
        Err(error) => Some(format!("lennette: {error}")),
    }
}

fn usage() -> String {
    [
        "lennette — desktop assistant",
        "",
        "Usage: lennette [option]",
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
        "LENNETTE_PRIVATE_OVERLAY=1 disables the desktop tray for protected launches.",
    ]
    .join("\n")
}

/// If an instance is already running, activate its window and return true.
pub fn activate_existing_instance() -> bool {
    send("focus-input").is_ok()
}

/// Path of the control socket shared with the running instance.
pub fn control_socket() -> Option<std::path::PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)?;
    Some(runtime.join("lennette-shortcut.sock"))
}

#[cfg(unix)]
fn send(command: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    let path = control_socket().ok_or("XDG_RUNTIME_DIR is not set")?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|_| "no running lennette instance was found".to_owned())?;
    let payload = if command.ends_with('\n') {
        command.to_owned()
    } else {
        format!("{command}\n")
    };
    stream
        .write_all(payload.as_bytes())
        .map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn send(_command: &str) -> Result<(), String> {
    Err("shortcut forwarding is not implemented on this platform yet".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_socket_lives_in_the_runtime_dir() {
        // The function reads the environment, so only assert its shape when set.
        if control_socket().is_some() {
            let path = control_socket().unwrap();
            assert!(path.ends_with("lennette-shortcut.sock"));
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
