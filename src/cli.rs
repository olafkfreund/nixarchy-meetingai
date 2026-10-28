//! Command line entry point.
//!
//! No arguments runs the daemon. `ctl <cmd> [json]` sends one request to the
//! running daemon over its socket and prints the reply.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::Duration,
};

/// Handle a one-shot invocation. Returns the text to print and the exit code, or
/// `None` when this invocation should run the daemon.
pub fn run_cli(args: &[String]) -> Option<(String, i32)> {
    match args.first().map(String::as_str)? {
        "ctl" => Some(ctl(&args[1..])),
        "mcp" => Some(match crate::mcp::run() {
            Ok(()) => (String::new(), 0),
            Err(error) => (format!("nixarchy-meetingai mcp: {error}"), 1),
        }),
        "--version" | "-V" => Some((
            format!("nixarchy-meetingai {}", env!("CARGO_PKG_VERSION")),
            0,
        )),
        "--help" | "-h" => Some((usage(), 0)),
        other => Some((
            format!(
                "nixarchy-meetingai: unknown argument '{other}'\n\n{}",
                usage()
            ),
            2,
        )),
    }
}

fn usage() -> String {
    [
        "nixarchy-meetingai: meeting copilot daemon",
        "",
        "Usage:",
        "  nixarchy-meetingai                  run the daemon",
        "  nixarchy-meetingai ctl CMD [JSON]   send one command, e.g. ctl state.get",
        "                                      or ctl chat.send '{\"text\":\"hi\"}'",
        "  nixarchy-meetingai ctl --agent CMD [JSON]",
        "                                      read-only, for AI agents: local-only interviews",
        "                                      stay hidden and anything but reads is refused",
        "  nixarchy-meetingai mcp              MCP server over stdio, for AI agents",
        "  nixarchy-meetingai --version | --help",
    ]
    .join("\n")
}

/// Socket of the running daemon under `runtime`.
pub fn socket_path_in(runtime: &Path) -> PathBuf {
    runtime.join("nixarchy-meetingai").join("daemon.sock")
}

pub fn socket_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|r| socket_path_in(Path::new(&r)))
}

/// Send one request and return the printed reply line plus an exit code.
fn ctl(args: &[String]) -> (String, i32) {
    // `--agent`: the daemon applies its read-only allowlist and local-only protection.
    let (origin, args) = match args.first().map(String::as_str) {
        Some("--agent") => (Some("agent"), &args[1..]),
        _ => (None, args),
    };
    let Some(cmd) = args.first() else {
        return (usage(), 2);
    };
    let args_json = match args.get(1) {
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => value,
            Err(error) => return (format!("nixarchy-meetingai: bad JSON: {error}"), 2),
        },
        None => serde_json::json!({}),
    };
    match exchange(cmd, args_json, origin) {
        Ok(reply) => {
            let ok = reply["ok"] == true;
            (reply.to_string(), if ok { 0 } else { 1 })
        }
        Err(error) => (format!("nixarchy-meetingai: {error}"), 1),
    }
}

fn exchange(
    cmd: &str,
    args: serde_json::Value,
    origin: Option<&str>,
) -> Result<serde_json::Value, String> {
    let path = socket_path().ok_or("XDG_RUNTIME_DIR is not set")?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|_| "no running nixarchy-meetingai daemon was found".to_owned())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .map_err(|e| e.to_string())?;
    let request = request_line(cmd, args, origin);
    writeln!(stream, "{request}").map_err(|e| e.to_string())?;
    // Events share the socket; skip them until our reply arrives.
    for line in BufReader::new(stream).lines() {
        let value: serde_json::Value =
            serde_json::from_str(&line.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if value["id"] == 1 {
            return Ok(value);
        }
    }
    Err("the daemon closed the connection".into())
}

fn request_line(cmd: &str, args: serde_json::Value, origin: Option<&str>) -> serde_json::Value {
    let mut request = serde_json::json!({"id": 1, "cmd": cmd, "args": args});
    if let Some(origin) = origin {
        request["origin"] = origin.into();
    }
    request
}

/// Path of the exclusive launch lock held by the running instance.
pub fn instance_lock_path() -> Option<std::path::PathBuf> {
    runtime_path("nixarchy-meetingai-instance.lock")
}

fn runtime_path(name: &str) -> Option<std::path::PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)?;
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

#[cfg(test)]
mod tests {
    #[test]
    fn agent_flag_adds_the_origin() {
        let plain = super::request_line("state.get", serde_json::json!({}), None);
        assert!(plain.get("origin").is_none());
        let agent = super::request_line("state.get", serde_json::json!({}), Some("agent"));
        assert_eq!(agent["origin"], "agent");
    }

    use super::*;

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
        // Other tests fork children in parallel; a child briefly holds a copy of
        // the lock fd until its exec closes it, so allow that window to pass.
        let reacquired = (0..100).any(|_| {
            InstanceLock::acquire_at(&path).is_some() || {
                std::thread::sleep(std::time::Duration::from_millis(20));
                false
            }
        });
        assert!(reacquired, "the slot should be reusable");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_socket_sits_in_a_private_subdirectory() {
        let path = socket_path_in(Path::new("/run/user/1"));
        assert_eq!(
            path,
            Path::new("/run/user/1/nixarchy-meetingai/daemon.sock")
        );
    }

    #[test]
    fn unknown_arguments_are_rejected_and_no_arguments_run_the_daemon() {
        assert!(run_cli(&[]).is_none());
        assert_eq!(run_cli(&["--bogus".into()]).unwrap().1, 2);
        assert_eq!(run_cli(&["ctl".into()]).unwrap().1, 2);
        assert!(
            run_cli(&["--version".into()])
                .unwrap()
                .0
                .contains("nixarchy-meetingai")
        );
    }
}
