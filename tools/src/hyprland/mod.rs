//! Isolated native compositor QA. All process groups belong to this invocation.
mod overview;
mod smoke;
pub use overview::run as overview;
pub use smoke::run as smoke;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

type Environment = BTreeMap<OsString, OsString>;
pub(super) struct Arguments {
    values: BTreeMap<String, String>,
    flags: BTreeSet<String>,
}
impl Arguments {
    fn parse(args: &[String], switches: &[&str]) -> Result<Self> {
        let mut result = Self {
            values: BTreeMap::new(),
            flags: BTreeSet::new(),
        };
        let mut iter = args.iter();
        while let Some(key) = iter.next() {
            ensure!(key.starts_with("--"), "Expected an option, got {key}");
            if switches.contains(&key.as_str()) {
                ensure!(result.flags.insert(key.clone()), "Duplicate option: {key}");
            } else {
                let value = iter
                    .next()
                    .filter(|v| !v.starts_with("--"))
                    .with_context(|| format!("Missing value for {key}"))?;
                ensure!(
                    result.values.insert(key.clone(), value.clone()).is_none(),
                    "Duplicate option: {key}"
                );
            }
        }
        Ok(result)
    }
    fn take(&mut self, name: &str) -> Option<String> {
        self.values.remove(name)
    }
    fn path(&mut self, name: &str, default: PathBuf) -> PathBuf {
        self.take(name).map(PathBuf::from).unwrap_or(default)
    }
    fn required(&mut self, name: &str) -> Result<PathBuf> {
        self.take(name)
            .map(PathBuf::from)
            .with_context(|| format!("Required option: {name}"))
    }
    fn flag(&mut self, name: &str) -> bool {
        self.flags.remove(name)
    }
    fn finish(self) -> Result<()> {
        ensure!(
            self.values.is_empty() && self.flags.is_empty(),
            "Unknown options: {:?} {:?}",
            self.values.keys(),
            self.flags
        );
        Ok(())
    }
}

struct Managed(Child, bool);
impl Managed {
    fn spawn(command: &mut Command) -> Result<Self> {
        Ok(Self(command.process_group(0).spawn()?, false))
    }
    fn wait(&mut self, timeout: Duration) -> Result<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.0.try_wait()? {
                return Ok(status);
            }
            ensure!(Instant::now() < deadline, "Child {} timed out", self.0.id());
            thread::sleep(Duration::from_millis(20));
        }
    }
    fn stop(&mut self) {
        if self.1 {
            return;
        }
        self.1 = true;
        // SAFETY: process_group(0) created this private group for our own child.
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGTERM);
        }
        let _ = self.wait(Duration::from_secs(3));
        // Also collect any watchdog descendants after the main child exits.
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}
impl Drop for Managed {
    fn drop(&mut self) {
        self.stop();
    }
}

fn execute(command: &mut Command) -> Result<String> {
    let mut output = tempfile::tempfile()?;
    let mut error = tempfile::tempfile()?;
    let mut child = Managed::spawn(
        command
            .stdin(Stdio::null())
            .stdout(output.try_clone()?)
            .stderr(error.try_clone()?),
    )?;
    let status = child.wait(Duration::from_secs(20))?;
    use std::io::{Seek, SeekFrom};
    output.seek(SeekFrom::Start(0))?;
    error.seek(SeekFrom::Start(0))?;
    let mut stdout = String::new();
    let mut stderr = String::new();
    output.take(16 * 1024 * 1024).read_to_string(&mut stdout)?;
    error.take(1024 * 1024).read_to_string(&mut stderr)?;
    ensure!(
        status.success(),
        "Command {:?} failed ({status}): {stdout}\n{stderr}",
        command.get_program()
    );
    Ok(stdout.trim().into())
}
fn command(env: &Environment, executable: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(executable);
    command.env_clear().envs(env);
    command
}
fn quote(value: impl AsRef<str>) -> String {
    serde_json::to_string(value.as_ref()).unwrap()
}
fn path_quote(path: &Path) -> String {
    quote(path.to_string_lossy())
}
fn pause(milliseconds: u64) {
    thread::sleep(Duration::from_millis(milliseconds));
}

fn capture_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}
fn plugins(path: Option<PathBuf>) -> Result<Vec<String>> {
    path.map(|p| -> Result<_> { Ok(serde_json::from_str(&fs::read_to_string(p)?)?) })
        .transpose()
        .map(Option::unwrap_or_default)
}
fn save(path: &Path, data: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(data)?))?;
    Ok(())
}
fn absolute(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).with_context(|| format!("Resolve {}", path.display()))
}

struct Harness {
    artifacts: PathBuf,
    runtime: tempfile::TempDir,
    env: Environment,
    parent: Environment,
    processes: Vec<Managed>,
    checks: Vec<Value>,
    host_pid: u32,
}
impl Harness {
    fn new(artifacts: &Path, parent_socket: Option<String>, overview: bool) -> Result<Self> {
        fs::create_dir_all(artifacts)?;
        let artifacts = absolute(artifacts)?;
        save(
            &artifacts.join("results.json"),
            &json!({"passed":false,"status":"running"}),
        )?;
        let runtime = tempfile::Builder::new()
            .prefix("ln-qa-")
            .tempdir_in("/tmp")?;
        let mut parent: Environment = std::env::vars_os().collect();
        // SAFETY: geteuid has no preconditions and does not modify process state.
        let runtime_default = format!("/run/user/{}", unsafe { libc::geteuid() });
        parent
            .entry("XDG_RUNTIME_DIR".into())
            .or_insert_with(|| runtime_default.into());
        let socket = parent_socket
            .or_else(|| std::env::var("WAYLAND_DISPLAY").ok())
            .context("Set WAYLAND_DISPLAY or pass --parent-socket")?;
        let socket = if Path::new(&socket).is_absolute() {
            PathBuf::from(socket)
        } else {
            PathBuf::from(parent.get(&OsString::from("XDG_RUNTIME_DIR")).unwrap()).join(socket)
        };
        ensure!(
            socket.exists(),
            "Parent Wayland socket is missing: {}",
            socket.display()
        );
        parent.insert("WAYLAND_DISPLAY".into(), socket.as_os_str().into());
        let mut env = parent.clone();
        env.insert("XDG_RUNTIME_DIR".into(), runtime.path().into());
        let config = if overview { "config" } else { "user-config" };
        let data = if overview { "data" } else { "user-data" };
        let home = if overview { "home" } else { "user-home" };
        for (key, directory) in [
            ("XDG_CONFIG_HOME", config),
            ("XDG_DATA_HOME", data),
            ("XDG_CACHE_HOME", "cache"),
            ("HOME", home),
        ] {
            fs::create_dir_all(artifacts.join(directory))?;
            env.insert(key.into(), artifacts.join(directory).into());
        }
        fs::create_dir_all(artifacts.join(config).join("hypr"))?;
        fs::create_dir_all(artifacts.join(home).join(".cache"))?;
        for key in [
            "HYPRLAND_INSTANCE_SIGNATURE",
            "DISPLAY",
            "AQ_BACKEND",
            "AQ_DRM_DEVICES",
            "LIBGL_ALWAYS_SOFTWARE",
            "LENNETTE_PRIVATE_OVERLAY",
        ] {
            env.remove(&OsString::from(key));
        }
        Ok(Self {
            artifacts,
            runtime,
            env,
            parent,
            processes: vec![],
            checks: vec![],
            host_pid: 0,
        })
    }
    fn start(&mut self, executable: &Path, config: &str) -> Result<()> {
        let executable = absolute(executable)?;
        let config_path = self.artifacts.join("config.lua");
        fs::write(&config_path, config)?;
        let watchdog = executable.parent().unwrap().join("start-hyprland");
        let mut cmd = command(
            &self.env,
            if watchdog.is_file() {
                &watchdog
            } else {
                &executable
            },
        );
        if watchdog.is_file() {
            cmd.args(["--no-nixgl", "--path"])
                .arg(&executable)
                .arg("--");
        }
        cmd.arg("--config").arg(&config_path);
        let log = fs::File::create(self.artifacts.join("compositor.log"))?;
        self.processes.push(Managed::spawn(
            cmd.stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log),
        )?);
        self.host_pid = self.processes[0].0.id();
        self.wait("compositor IPC", 60, |h| Ok(h.instance_socket()?.is_some()))?;
        let socket = self.instance_socket()?.unwrap();
        let signature = socket
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        self.env.insert(
            "HYPRLAND_INSTANCE_SIGNATURE".into(),
            signature.clone().into(),
        );
        self.wait("compositor instance metadata", 60, |h| {
            let Ok(value) = h.json(&["instances", "-j"]) else {
                return Ok(false);
            };
            if let Some(instance) = value
                .as_array()
                .and_then(|v| v.iter().find(|v| v["instance"] == signature))
            {
                h.host_pid = instance["pid"].as_u64().context("Instance PID missing")? as u32;
                return Ok(true);
            }
            Ok(false)
        })?;
        self.wait("Wayland socket", 15, |h| Ok(h.wayland_socket()?.is_some()))?;
        self.env
            .insert("WAYLAND_DISPLAY".into(), self.wayland_socket()?.unwrap());
        ensure!(
            self.ctl(&["configerrors"])?.is_empty(),
            "Nested compositor has configuration errors"
        );
        Ok(())
    }
    fn instance_socket(&self) -> Result<Option<PathBuf>> {
        let Ok(entries) = fs::read_dir(self.runtime.path().join("hypr")) else {
            return Ok(None);
        };
        Ok(entries
            .filter_map(|e| e.ok())
            .map(|e| e.path().join(".socket.sock"))
            .find(|p| p.exists()))
    }
    fn wayland_socket(&self) -> Result<Option<OsString>> {
        Ok(fs::read_dir(self.runtime.path())?
            .filter_map(|e| e.ok())
            .find(|e| {
                e.file_name().to_string_lossy().starts_with("wayland-")
                    && !e.file_name().to_string_lossy().ends_with(".lock")
            })
            .map(|e| e.file_name()))
    }
    fn wait(
        &mut self,
        description: &str,
        seconds: u64,
        mut predicate: impl FnMut(&mut Self) -> Result<bool>,
    ) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            ensure!(
                self.processes[0].0.try_wait()?.is_none(),
                "Compositor exited during {description}; see {}/compositor.log",
                self.artifacts.display()
            );
            if predicate(self)? {
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "Timed out: {description}");
            pause(100);
        }
    }
    fn call(&self, executable: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Result<String> {
        execute(command(&self.env, executable).args(args))
    }
    fn ctl(&self, args: &[&str]) -> Result<String> {
        self.call("hyprctl", args)
    }
    fn json(&self, args: &[&str]) -> Result<Value> {
        Ok(serde_json::from_str(&self.ctl(args)?)?)
    }
    fn parent_json(&self, args: &[&str]) -> Result<Value> {
        Ok(serde_json::from_str(&execute(
            command(&self.parent, "hyprctl").args(args),
        )?)?)
    }
    fn lua(&self, code: &str) -> Result<()> {
        let output = self.ctl(&["eval", code])?;
        ensure!(output == "ok", "Lua failed: {code}\n{output}");
        Ok(())
    }
    fn dispatch(&self, code: &str) -> Result<()> {
        self.lua(&format!("hl.dispatch({code})"))
    }
    fn clients(&self) -> Result<Vec<Value>> {
        self.json(&["-j", "clients"])?
            .as_array()
            .cloned()
            .context("Client list is not an array")
    }
    fn window(&self, class: &str) -> Result<Value> {
        self.clients()?
            .into_iter()
            .find(|v| v["class"] == class)
            .with_context(|| format!("Missing window: {class}"))
    }
    fn app_window(&self, pid: u32) -> Result<Value> {
        self.clients()?
            .into_iter()
            .find(|v| v["pid"] == pid && v["title"] == "lennette")
            .context("Missing app window")
    }
    fn spawn(&mut self, executable: &Path, args: &[&str], name: &str) -> Result<usize> {
        let log = fs::File::create(self.artifacts.join(format!("{name}.log")))?;
        let mut cmd = command(&self.env, executable);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        self.processes.push(Managed::spawn(&mut cmd)?);
        Ok(self.processes.len() - 1)
    }
    fn fixture(
        &mut self,
        client_dir: &Path,
        class: &str,
        color: &str,
        popup: bool,
    ) -> Result<usize> {
        let mut args = vec![class, color];
        if popup {
            args.push("popup");
        }
        let index = self.spawn(&client_dir.join("window"), &args, class)?;
        self.wait(&format!("map {class}"), 15, |h| {
            Ok(h.clients()?.iter().any(|w| w["class"] == class))
        })?;
        Ok(index)
    }
    fn capture(
        &self,
        client_dir: &Path,
        name: &str,
        protocol: &str,
        storage: &str,
        output: &str,
    ) -> Result<Frame> {
        let path = self
            .artifacts
            .join(format!("{}-{protocol}-{storage}.ppm", capture_name(name)));
        self.call(
            client_dir.join("capture"),
            &[
                protocol,
                output,
                storage,
                path.to_str().context("Non UTF-8 capture path")?,
                "0",
            ],
        )?;
        Frame::read(&path)
    }
    fn clean(&mut self, frame: Frame, name: &str) -> Result<()> {
        let bad = frame.unexpected();
        ensure!(
            bad == 0,
            "{name}: {bad} unexpected pixels, colors={:?}",
            frame.colors()
        );
        self.checks
            .push(json!({"test":name,"pixels":frame.width*frame.height,"unexpected_pixels":bad}));
        println!(
            "PASS {name}: {}x{}, no overlay or replacement pixels",
            frame.width, frame.height
        );
        Ok(())
    }
    fn record(&self, result: &Result<()>, extra: Value) -> Result<()> {
        let mut evidence = extra;
        evidence["passed"] = json!(result.is_ok());
        evidence["checks"] = json!(self.checks);
        if let Err(error) = result {
            evidence["error"] = json!(format!("{error:#}"));
        }
        save(&self.artifacts.join("results.json"), &evidence)
    }
    fn host_window(&self) -> Result<Value> {
        let clients = self.parent_json(&["-i", "0", "-j", "clients"])?;
        let matches: Vec<_> = clients
            .as_array()
            .context("Invalid host client list")?
            .iter()
            .filter(|w| w["pid"] == self.host_pid)
            .cloned()
            .collect();
        ensure!(
            matches.len() == 1,
            "Expected one nested host window for PID {}",
            self.host_pid
        );
        Ok(matches[0].clone())
    }
    fn bound_host(&mut self) -> Result<()> {
        self.wait("nested host window", 15, |h| Ok(h.host_window().is_ok()))?;
        let window = self.host_window()?;
        let selector = quote(format!(
            "address:{}",
            window["address"].as_str().context("Host address missing")?
        ));
        let workspace = self.parent_json(&["-i", "0", "-j", "activeworkspace"])?["id"].to_string();
        for code in [
            format!(
                "hl.dsp.window.move({{window={selector},workspace={},follow=false}})",
                quote(workspace)
            ),
            format!("hl.dsp.window.float({{window={selector},action=\"set\"}})"),
            format!("hl.dsp.window.resize({{window={selector},x=960,y=640}})"),
            format!("hl.dsp.window.center({{window={selector}}})"),
        ] {
            ensure!(
                execute(command(&self.parent, "hyprctl").args([
                    "-i",
                    "0",
                    "eval",
                    &format!("hl.dispatch({code})")
                ]))? == "ok",
                "Cannot bound nested host window"
            );
        }
        ensure!(
            self.host_window()?["floating"] == true,
            "Nested host window did not float"
        );
        Ok(())
    }
    fn local_display(&mut self, name: &str) -> Result<()> {
        let window = self.host_window()?;
        let selector = quote(format!(
            "address:{}",
            window["address"].as_str().context("Host address missing")?
        ));
        execute(command(&self.parent, "hyprctl").args([
            "-i",
            "0",
            "eval",
            &format!("hl.dispatch(hl.dsp.focus({{window={selector}}}))"),
        ]))?;
        pause(500);
        let geometry = format!(
            "{},{} {}x{}",
            window["at"][0], window["at"][1], window["size"][0], window["size"][1]
        );
        let path = self.artifacts.join(format!("{name}-local-display.ppm"));
        execute(
            command(&self.parent, "grim")
                .args(["-t", "ppm", "-g", &geometry])
                .arg(&path),
        )?;
        let visible = Frame::read(&path)?.local_marker_pixels();
        if visible <= 1000 {
            save(
                &self.artifacts.join("local-visibility-state.json"),
                &json!({"host":window,"nested_clients":self.clients()?,"nested_monitors":self.json(&["-j","monitors"])?}),
            )?;
        }
        ensure!(
            visible > 1000,
            "{name}: only {visible} local overlay pixels"
        );
        self.checks
            .push(json!({"test":name,"local_overlay_pixels":visible}));
        println!("PASS {name}: {visible} overlay pixels visible locally");
        Ok(())
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        // Kill only the groups created by this harness, in reverse startup order.
        while let Some(process) = self.processes.pop() {
            drop(process);
        }
        if let Ok(entries) = fs::read_dir(self.runtime.path().join("hypr")) {
            for entry in entries.flatten() {
                if let Ok(logs) = fs::read_dir(entry.path()) {
                    for log in logs.flatten() {
                        if log.file_name().to_string_lossy().starts_with("hyprland")
                            && log.path().extension().is_some_and(|s| s == "log")
                        {
                            let _ = fs::copy(log.path(), self.artifacts.join("hyprland.log"));
                        }
                    }
                }
            }
        }
    }
}

struct Frame {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}
impl Frame {
    // The parent desktop can apply opacity/color management to its ordinary
    // nested window (e.g. 255 -> 235). Detect the distinctive local marker by
    // chroma; protocol exports continue to use exact background comparisons.
    fn local_marker_pixels(&self) -> usize {
        self.pixels
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|[red, green, blue]| {
                *red >= 180 && *blue >= 180 && *green <= 40 && red.abs_diff(*blue) <= 20
            })
            .count()
    }
    fn read(path: &Path) -> Result<Self> {
        Self::parse(&fs::read(path)?)
    }
    fn parse(data: &[u8]) -> Result<Self> {
        let mut sections = data.splitn(4, |b| *b == b'\n');
        ensure!(sections.next() == Some(b"P6"), "Expected P6 image");
        let dimensions = std::str::from_utf8(sections.next().context("Missing dimensions")?)?
            .split_whitespace()
            .map(str::parse::<usize>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure!(
            dimensions.len() == 2 && dimensions.iter().all(|v| *v > 0),
            "Invalid dimensions"
        );
        ensure!(sections.next() == Some(b"255"), "Expected 8-bit pixels");
        let pixels = sections.next().context("Missing pixels")?;
        let size = dimensions[0]
            .checked_mul(dimensions[1])
            .and_then(|v| v.checked_mul(3))
            .context("Image size overflow")?;
        ensure!(pixels.len() == size, "Truncated or oversized frame");
        Ok(Self {
            width: dimensions[0],
            height: dimensions[1],
            pixels: pixels.into(),
        })
    }
    fn count(&self, color: [u8; 3]) -> usize {
        self.pixels
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|rgb| **rgb == color)
            .count()
    }
    fn unexpected(&self) -> usize {
        self.pixels
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|rgb| {
                rgb.iter()
                    .zip([32, 96, 128])
                    .any(|(a, b)| a.abs_diff(b) > 3)
            })
            .count()
    }
    fn colors(&self) -> Vec<([u8; 3], usize)> {
        let mut counts = BTreeMap::new();
        for rgb in self.pixels.as_chunks::<3>().0.iter() {
            *counts.entry([rgb[0], rgb[1], rgb[2]]).or_insert(0) += 1;
        }
        let mut counts: Vec<_> = counts.into_iter().collect();
        counts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        counts.truncate(8);
        counts
    }
}
fn read_ack(stream: &mut std::process::ChildStdout) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut line = Vec::new();
    loop {
        ensure!(
            Instant::now() < deadline,
            "Virtual keyboard acknowledgement timed out"
        );
        let mut fd = libc::pollfd {
            fd: stream.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: valid, uniquely borrowed pollfd and child stdout descriptor.
        let ready = unsafe { libc::poll(&mut fd, 1, 100) };
        ensure!(ready >= 0, "Keyboard poll failed");
        if ready == 0 {
            continue;
        }
        let mut byte = [0];
        ensure!(stream.read(&mut byte)? == 1, "Virtual keyboard exited");
        if byte[0] == b'\n' {
            return Ok(String::from_utf8(line)?.trim().into());
        }
        ensure!(line.len() < 1024, "Invalid keyboard acknowledgement");
        line.push(byte[0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_labels_cannot_create_subdirectories() {
        assert_eq!(
            capture_name("wlr/shm overlay + popup"),
            "wlr-shm-overlay---popup"
        );
        assert!(!capture_name("../outside").contains('/'));
    }
    #[test]
    fn pixels_detect_one_marker_black_fallback_and_truncation() {
        let mut bytes = b"P6\n2 1\n255\n".to_vec();
        bytes.extend([32, 96, 128, 35, 93, 131]);
        assert_eq!(Frame::parse(&bytes).unwrap().unexpected(), 0);
        bytes[11..14].copy_from_slice(&[255, 0, 255]);
        let frame = Frame::parse(&bytes).unwrap();
        assert_eq!(frame.count([255, 0, 255]), 1);
        assert_eq!(frame.unexpected(), 1);
        bytes[11..14].copy_from_slice(&[0, 0, 0]);
        assert_eq!(Frame::parse(&bytes).unwrap().unexpected(), 1);
        assert!(Frame::parse(&bytes[..bytes.len() - 1]).is_err());
        assert!(Frame::parse(b"P6\n18446744073709551615 2\n255\n").is_err());
    }

    #[test]
    fn local_visibility_handles_parent_opacity_without_accepting_cyan_or_black() {
        let frame = Frame {
            width: 4,
            height: 1,
            pixels: vec![235, 0, 235, 255, 0, 255, 0, 235, 235, 0, 0, 0],
        };
        assert_eq!(frame.local_marker_pixels(), 2);
        assert_eq!(frame.unexpected(), 4);
    }
    #[test]
    fn arguments_reject_missing_duplicate_unknown_options() {
        for input in [
            vec!["--x"],
            vec!["--x", "1", "--x", "2"],
            vec!["--flag", "--flag"],
        ] {
            assert!(
                Arguments::parse(
                    &input.into_iter().map(String::from).collect::<Vec<_>>(),
                    &["--flag"]
                )
                .is_err()
            );
        }
        assert!(
            Arguments::parse(&["--typo".into(), "x".into()], &[])
                .unwrap()
                .finish()
                .is_err()
        );
    }
    #[test]
    fn bounded_commands_collect_output_and_errors() {
        let mut success = Command::new("sh");
        success.args(["-c", "printf ok"]);
        assert_eq!(execute(&mut success).unwrap(), "ok");
        let mut failure = Command::new("sh");
        failure.args(["-c", "echo failed >&2; exit 2"]);
        assert!(
            execute(&mut failure)
                .unwrap_err()
                .to_string()
                .contains("failed")
        );
    }
}
