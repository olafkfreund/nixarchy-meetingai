//! No arguments runs the daemon. `ctl <cmd> [json]` sends one request to the
//! running daemon over its socket and prints the reply.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::Duration, };
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
    }}
fn usage() -> String { todo!() }
/// Socket of the running daemon under `runtime`.
pub fn socket_path_in(runtime: &Path) -> PathBuf {
    runtime.join("nixarchy-meetingai").join("daemon.sock") }
pub fn socket_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|r| socket_path_in(Path::new(&r))) }

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
            (reply.to_string(), if ok { 0 } else { 1 }) }
        Err(error) => (format!("nixarchy-meetingai: {error}"), 1), } }
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
#[cfg(test)] mod tests { use super::*;
    #[test]
    fn agent_flag_adds_the_origin() {
        let plain = super::request_line("state.get", serde_json::json!({}), None);
        assert!(plain.get("origin").is_none());
        let agent = super::request_line("state.get", serde_json::json!({}), Some("agent"));
        assert_eq!(agent["origin"], "agent");
    }
#[test] fn the_socket_sits_in_a_private_subdirectory() {
        let path = socket_path_in(Path::new("/run/user/1"));
        assert_eq!(
            path,
            Path::new("/run/user/1/nixarchy-meetingai/daemon.sock") ); }
#[test] fn unknown_arguments_are_rejected_and_no_arguments_run_the_daemon() {
        assert!(run_cli(&[]).is_none());
        assert_eq!(run_cli(&["--bogus".into()]).unwrap().1, 2);
        assert_eq!(run_cli(&["ctl".into()]).unwrap().1, 2);
        assert!(
            run_cli(&["--version".into()])
                .unwrap()
                .0
                .contains("nixarchy-meetingai")
        ); } }
