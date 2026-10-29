//! Daemon command-line dispatch, socket path resolution, and process lock.
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

/// Exclusive process lock ensuring at most one daemon runs per user session.
pub struct InstanceLock {
    _lock_handle: std::fs::File,
}

fn lock_file_location() -> Option<PathBuf> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")?;
    Some(Path::new(&runtime_dir).join("nixarchy-meetingai-instance.lock"))
}

impl InstanceLock {
    /// Acquire the session-wide lock file under `XDG_RUNTIME_DIR`.
    pub fn acquire() -> Option<Self> {
        let target = lock_file_location()?;
        Self::open_and_lock(&target)
    }

    fn open_and_lock(target: &Path) -> Option<Self> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).ok()?;
        }
        let mut handle = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(target)
        {
            Ok(f) => f,
            Err(err) => {
                tracing::warn!(
                    path = %target.display(),
                    %err,
                    "failed to open single-instance lock file"
                );
                return None;
            }
        };
        handle.try_lock().ok()?;
        let _ = handle.set_len(0);
        let _ = std::io::Seek::seek(&mut handle, std::io::SeekFrom::Start(0));
        let _ = writeln!(handle, "{}", std::process::id());
        let _ = handle.flush();
        Some(Self {
            _lock_handle: handle,
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_flag_adds_the_origin() {
        let plain = super::request_line("state.get", serde_json::json!({}), None);
        assert!(plain.get("origin").is_none());
        let agent = super::request_line("state.get", serde_json::json!({}), Some("agent"));
        assert_eq!(agent["origin"], "agent");
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

    #[test]
    fn runtime_lock_file_has_expected_suffix() {
        if std::env::var_os("XDG_RUNTIME_DIR").is_some() {
            let resolved = lock_file_location().expect("runtime dir is present");
            assert!(resolved.ends_with("nixarchy-meetingai-instance.lock"));
        }
    }

    #[test]
    fn exclusive_lock_blocks_second_holder_until_released() {
        let scratch_path = std::env::temp_dir().join(format!(
            "nixarchy-meetingai-lock-test-{}.lock",
            uuid::Uuid::new_v4()
        ));
        let primary = InstanceLock::open_and_lock(&scratch_path)
            .expect("initial lock acquisition must succeed");
        assert!(InstanceLock::open_and_lock(&scratch_path).is_none());
        drop(primary);

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut reacquired = None;
        while std::time::Instant::now() < deadline {
            reacquired = InstanceLock::open_and_lock(&scratch_path);
            if reacquired.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(reacquired.is_some(), "lock was not released in time");
        drop(reacquired);
        let _ = std::fs::remove_file(&scratch_path);
    }
}
