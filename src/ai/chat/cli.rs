//! Subscription providers that drive the `claude` and `codex` CLIs.
//!
//! The CLI runs in an empty temp dir and its own process group, without any
//! API-key variable, so it always uses the user's subscription login. Cancel
//! kills the whole group.

use std::future::Future;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

use super::engine::{ChatError, TurnContext, wait_for_cancellation};
use crate::ai::providers::ProviderConfig;
use crate::core::domain::{Message, Role};

const MAX_ANSWER_BYTES: usize = 2 * 1024 * 1024;
const STDERR_TAIL: usize = 2048;
/// Catalog model id meaning "let the CLI pick".
pub const CLI_DEFAULT_MODEL: &str = "default";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Claude,
    Codex,
    Agy,
}

/// One parsed stdout line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliEvent {
    Delta(String),
    /// End of the answer; claude carries the full text as a fallback.
    Done(Option<String>),
    Failed(String),
}

/// Parse one stdout line; unknown or non-JSON lines are ignored.
pub fn parse_line(kind: Kind, line: &str) -> Option<CliEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    let text = |pointer: &str| value.pointer(pointer).and_then(Value::as_str);
    if kind == Kind::Agy {
        // text_delta is incremental, including the last DONE step.
        return match value.get("event")?.as_str()? {
            "step_update" if text("/step_update/step_type") == Some("agent_response") => {
                text("/step_update/text_delta")
                    .filter(|t| !t.is_empty())
                    .map(|t| CliEvent::Delta(t.to_owned()))
            }
            "result" if text("/result/status") == Some("SUCCESS") => {
                Some(CliEvent::Done(text("/result/response").map(str::to_owned)))
            }
            "result" => Some(CliEvent::Failed(
                text("/result/error")
                    .or(text("/result/response"))
                    .unwrap_or("agy did not finish the turn")
                    .to_owned(),
            )),
            _ => None,
        };
    }
    match (kind, value.get("type")?.as_str()?) {
        (Kind::Claude, "stream_event") => {
            if text("/event/type") == Some("content_block_delta") {
                text("/event/delta/text").map(|t| CliEvent::Delta(t.to_owned()))
            } else {
                None
            }
        }
        (Kind::Claude, "result") => {
            if value.get("is_error").and_then(Value::as_bool) == Some(true) {
                Some(CliEvent::Failed(
                    text("/result")
                        .unwrap_or("claude reported an error")
                        .to_owned(),
                ))
            } else {
                Some(CliEvent::Done(text("/result").map(str::to_owned)))
            }
        }
        (Kind::Codex, "item.completed") if text("/item/type") == Some("agent_message") => {
            text("/item/text").map(|t| CliEvent::Delta(t.to_owned()))
        }
        (Kind::Codex, "turn.completed") => Some(CliEvent::Done(None)),
        (Kind::Codex, "turn.failed") => Some(CliEvent::Failed(
            text("/error/message")
                .unwrap_or("codex turn failed")
                .to_owned(),
        )),
        _ => None,
    }
}

fn quoted_transcript(history: &[Message]) -> String {
    history
        .iter()
        .map(|m| {
            let who = match m.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => "System",
            };
            let body = m.content.lines().collect::<Vec<_>>().join("\n> ");
            format!("{who}:\n> {body}")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Claude argv. The system prompt lives in `system_file`, never in argv (E2BIG, `/proc`).
pub fn claude_args(model: &str, system_file: &Path) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--include-partial-messages",
        "--verbose",
        "--tools",
        "",
        "--strict-mcp-config",
        "--no-session-persistence",
        "--settings",
        r#"{"disableAllHooks":true}"#,
        "--system-prompt-file",
    ]
    .map(String::from)
    .to_vec();
    args.push(system_file.display().to_string());
    if model != CLI_DEFAULT_MODEL && !model.is_empty() {
        args.extend(["--model".into(), model.to_owned()]);
    }
    args
}

/// Antigravity argv. agy takes the prompt only as an argument and has no system-prompt flag.
pub fn agy_args(model: &str, prompt: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "--mode",
        "plan",
        "--output-format",
        "stream-json",
        "--print-timeout",
        "120s",
    ]
    .map(String::from)
    .to_vec();
    if model != CLI_DEFAULT_MODEL && !model.is_empty() {
        args.extend(["--model".into(), model.to_owned()]);
    }
    args.extend(["-p".into(), prompt.to_owned()]);
    args
}

/// Private HOME for agy: the real `~/.gemini` (login, state) linked in, except its MCP servers
/// and settings, which are emptied. agy starts every configured MCP server on each call
/// (~40 s here); with none it answers in seconds, and no tool can reach the call.
fn agy_home(dir: &Path) -> Result<std::path::PathBuf, ChatError> {
    use std::os::unix::fs::symlink;
    let io = |e: std::io::Error| ChatError::Cli(format!("could not prepare agy: {e}"));
    let real = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| ChatError::Cli("HOME is not set".into()))?;
    let home = dir.join("home");
    let gemini = home.join(".gemini");
    std::fs::create_dir_all(gemini.join("config")).map_err(io)?;
    let link_all = |from: &Path, to: &Path, skip: &dyn Fn(&str) -> bool| -> Result<(), ChatError> {
        let Ok(entries) = std::fs::read_dir(from) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            if !skip(&entry.file_name().to_string_lossy()) {
                symlink(entry.path(), to.join(entry.file_name())).map_err(io)?;
            }
        }
        Ok(())
    };
    link_all(&real.join(".gemini"), &gemini, &|n| {
        matches!(
            n,
            "config" | "settings.json" | "settings.json.orig" | "settings.nix"
        )
    })?;
    link_all(&real.join(".gemini/config"), &gemini.join("config"), &|n| {
        n.starts_with("mcp_config")
    })?;
    std::fs::write(
        gemini.join("config/mcp_config.json"),
        r#"{"mcpServers":{}}"#,
    )
    .map_err(io)?;
    std::fs::write(gemini.join("settings.json"), "{}").map_err(io)?;
    for name in [".config", ".local", ".cache"] {
        let _ = symlink(real.join(name), home.join(name));
    }
    Ok(home)
}

/// Models agy offers: the first column of `agy models`.
pub async fn agy_models() -> Result<Vec<String>, String> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        Command::new("agy")
            .arg("models")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "`agy models` timed out".to_owned())?
    .map_err(|e| format!("could not run `agy models`: {e}"))?;
    let models: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.split_once('\t').map(|(id, _)| id.trim().to_owned()))
        .filter(|id| !id.is_empty() && !id.starts_with('-'))
        .collect();
    if models.is_empty() {
        return Err("`agy models` listed nothing; sign in with `agy`".into());
    }
    Ok(models)
}

/// Models codex offers: the listed entries of `codex debug models`, `default` first.
pub async fn codex_models() -> Result<Vec<String>, String> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        Command::new("codex")
            .args(["debug", "models"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "`codex debug models` timed out".to_owned())?
    .map_err(|e| format!("could not run `codex debug models`: {e}"))?;
    parse_codex_models(&output.stdout)
}

fn parse_codex_models(json: &[u8]) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let mut models = vec![CLI_DEFAULT_MODEL.to_owned()];
    for row in value["models"].as_array().into_iter().flatten() {
        if let Some(slug) = row["slug"].as_str().filter(|s| {
            row["visibility"] == "list" && !s.starts_with('-') && !models.iter().any(|m| m == s)
        }) {
            models.push(slug.to_owned());
        }
    }
    if models.len() == 1 {
        return Err("`codex debug models` listed no models".into());
    }
    Ok(models)
}

/// Codex argv; the prompt arrives on stdin (`-`).
pub fn codex_args(model: &str, dir: &Path, images: &[std::path::PathBuf]) -> Vec<String> {
    let mut args: Vec<String> = [
        "exec",
        "--json",
        "-c",
        "forced_login_method=chatgpt",
        "-s",
        "read-only",
        "--ephemeral",
        "--skip-git-repo-check",
        "-C",
    ]
    .map(String::from)
    .to_vec();
    args.push(dir.display().to_string());
    if model != CLI_DEFAULT_MODEL && !model.is_empty() {
        args.extend(["-m".into(), model.to_owned()]);
    }
    for image in images {
        args.extend(["-i".into(), image.display().to_string()]);
    }
    args.push("-".into());
    args
}

fn claude_message(text: &str, images: &[String]) -> String {
    let mut content = vec![json!({"type":"text","text":text})];
    content.extend(images.iter().map(|image| {
        json!({
            "type":"image","source":{"type":"base64","media_type":"image/png","data":image}
        })
    }));
    format!(
        "{}\n",
        json!({"type":"user","message":{"role":"user","content":content}})
    )
}

fn kill_group(pid: u32) {
    // SAFETY: killpg only signals the process group (= session) we created with setsid.
    unsafe { libc::killpg(pid as i32, libc::SIGKILL) };
}

/// Kills the child's process group when dropped, so a cancelled future leaves nothing running.
struct GroupGuard(Option<u32>);

impl GroupGuard {
    fn kill(&mut self) {
        if let Some(pid) = self.0.take() {
            kill_group(pid);
        }
    }
}

impl Drop for GroupGuard {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Private (0700) temp dir removed when dropped, also when the future is dropped.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> Result<Self, ChatError> {
        use std::os::unix::fs::DirBuilderExt;
        let path =
            std::env::temp_dir().join(format!("nixarchy-meetingai-cli-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|e| ChatError::Cli(e.to_string()))?;
        Ok(Self(path))
    }

    /// Write a 0600 file into the dir.
    fn write(&self, name: &str, bytes: &[u8]) -> Result<std::path::PathBuf, ChatError> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let path = self.0.join(name);
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .and_then(|mut f| f.write_all(bytes))
            .map_err(|e| ChatError::Cli(e.to_string()))?;
        Ok(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A model id is passed to the CLI as a value; one that looks like a flag is refused.
fn check_model(model: &str) -> Result<(), ChatError> {
    if model.starts_with('-') {
        return Err(ChatError::Protocol(format!("invalid model id `{model}`")));
    }
    Ok(())
}

/// Whether `program` is an executable file on `PATH`.
pub fn on_path(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            std::fs::metadata(dir.join(program))
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
    })
}

/// Stream one turn through the subscription CLI named by `config.id`.
pub async fn stream<F: Future<Output = ()>>(
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
    cancel: Arc<watch::Receiver<bool>>,
    on_text: impl FnMut(String) -> F,
) -> Result<String, ChatError> {
    let kind = match config.id.as_str() {
        "codex-sub" => Kind::Codex,
        "agy-sub" => Kind::Agy,
        _ => Kind::Claude,
    };
    let (last, earlier) = history
        .split_last()
        .ok_or_else(|| ChatError::Protocol("there is nothing to send".into()))?;
    check_model(&config.model)?;
    let system = context.system.clone().unwrap_or_default();
    // Earlier turns ride in the stdin user message, not in argv.
    let mut text = String::new();
    if !earlier.is_empty() {
        text.push_str("Conversation so far (quoted):\n");
        text.push_str(&quoted_transcript(earlier));
        text.push_str("\n\n");
    }
    text.push_str(&last.content);
    let dir = TempDir::new()?;
    let (program, args, payload) = match kind {
        Kind::Claude => {
            let file = dir.write("system.md", system.as_bytes())?;
            (
                "claude",
                claude_args(&config.model, &file),
                claude_message(&text, &context.images),
            )
        }
        Kind::Codex => {
            let mut files = Vec::new();
            for (i, image) in context.images.iter().enumerate() {
                let bytes = STANDARD
                    .decode(image)
                    .map_err(|_| ChatError::Protocol("attachment is not valid base64".into()))?;
                files.push(dir.write(&format!("image-{i}.png"), &bytes)?);
            }
            let prompt = format!("{system}\n\n{text}");
            ("codex", codex_args(&config.model, &dir.0, &files), prompt)
        }
        Kind::Agy => {
            let prompt = format!("{system}\n\n{text}");
            // One argv string is capped near 128 KiB; a Cli error lets the chain try the next provider.
            if prompt.len() > 100_000 {
                return Err(ChatError::Cli(
                    "the conversation is too long for the Antigravity CLI".into(),
                ));
            }
            agy_home(&dir.0)?;
            ("agy", agy_args(&config.model, &prompt), String::new())
        }
    };
    run(program, kind, args, payload, &dir.0, cancel, on_text).await
}

/// Spawn `program`, feed `payload`, and turn stdout into deltas.
pub(super) async fn run<F: Future<Output = ()>>(
    program: &str,
    kind: Kind,
    args: Vec<String>,
    payload: String,
    dir: &Path,
    cancel: Arc<watch::Receiver<bool>>,
    mut on_text: impl FnMut(String) -> F,
) -> Result<String, ChatError> {
    if *cancel.borrow() {
        return Ok(String::new());
    }
    let mut cancellation = (*cancel).clone();
    let mut command = Command::new(program);
    command
        .args(args)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("OPENAI_API_KEY_FILE")
        .env_remove("GEMINI_API_KEY")
        .env_remove("GEMINI_API_KEY_FILE")
        .env_remove("GOOGLE_API_KEY")
        .current_dir(dir);
    if kind == Kind::Agy {
        command.env("HOME", dir.join("home"));
    }
    // SAFETY: setsid is async-signal-safe and runs between fork and exec; it drops the
    // controlling terminal, so a wrapper (mise) that opens /dev/tty cannot stall. pgid == pid.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = command
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => ChatError::Cli(format!(
                "`{program}` was not found in PATH; install it and sign in with your subscription"
            )),
            _ => ChatError::Cli(format!("could not start `{program}`: {error}")),
        })?;
    let mut group = GroupGuard(child.id());
    let mut stdin = child.stdin.take().expect("stdin piped");
    // Written off-task so a full pipe cannot block cancellation; dropping closes stdin.
    tokio::spawn(async move {
        let _ = stdin.write_all(payload.as_bytes()).await;
    });
    let mut stderr = child.stderr.take().expect("stderr piped");
    let stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        let _ = (&mut stderr).take(1 << 20).read_to_end(&mut bytes).await;
        let start = bytes.len().saturating_sub(STDERR_TAIL);
        String::from_utf8_lossy(&bytes[start..]).trim().to_owned()
    });
    let mut lines = BufReader::new(child.stdout.take().expect("stdout piped")).lines();
    let mut answer = String::new();
    let mut done = false;
    let outcome: Result<(), ChatError> = loop {
        let line = tokio::select! {
            line = lines.next_line() => line,
            _ = wait_for_cancellation(&mut cancellation) => break Ok(()),
        };
        let line = match line {
            Ok(Some(line)) => line,
            Ok(None) => break Ok(()),
            Err(error) => break Err(ChatError::Protocol(error.to_string())),
        };
        match parse_line(kind, &line) {
            Some(CliEvent::Delta(text)) => {
                if text.len() > MAX_ANSWER_BYTES.saturating_sub(answer.len()) {
                    break Err(ChatError::Protocol("answer exceeded the size limit".into()));
                }
                answer.push_str(&text);
                tokio::select! {
                    _ = on_text(text) => {},
                    _ = wait_for_cancellation(&mut cancellation) => break Ok(()),
                }
            }
            Some(CliEvent::Done(full)) => {
                if answer.is_empty()
                    && let Some(full) = full.filter(|t| !t.is_empty())
                {
                    answer = full.clone();
                    on_text(full).await;
                }
                done = true;
                break Ok(());
            }
            Some(CliEvent::Failed(message)) => break Err(ChatError::Cli(message)),
            None => {}
        }
    };
    // Done, cancelled, failed or EOF: never leave the group running.
    group.kill();
    let status = child.wait().await;
    let tail = stderr_task.await.unwrap_or_default();
    outcome?;
    if *cancel.borrow() || (done && !answer.trim().is_empty()) {
        return Ok(answer);
    }
    let status = status.map(|s| s.to_string()).unwrap_or_default();
    Err(ChatError::Cli(format!(
        "`{program}` ended ({status}) without an answer: {tail}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn events(kind: Kind, name: &str) -> Vec<CliEvent> {
        fixture(name)
            .lines()
            .filter_map(|l| parse_line(kind, l))
            .collect()
    }

    #[test]
    fn claude_fixture_yields_delta_then_result() {
        assert_eq!(
            events(Kind::Claude, "claude-stream.ndjson"),
            [
                CliEvent::Delta("hi there".into()),
                CliEvent::Done(Some("hi there".into()))
            ]
        );
    }

    #[test]
    fn agy_fixture_yields_incremental_deltas_then_result() {
        let events = events(Kind::Agy, "agy-stream.ndjson");
        let deltas: String = events
            .iter()
            .filter_map(|e| match e {
                CliEvent::Delta(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 5);
        assert!(deltas.starts_with("Kubernetes is an open-source"));
        // The result carries the same text the deltas add up to.
        assert_eq!(events.last(), Some(&CliEvent::Done(Some(deltas))));
    }

    #[test]
    fn codex_catalog_lists_visible_models_after_default() {
        let json = br#"{"models":[{"slug":"a","visibility":"list"},{"slug":"hidden","visibility":"hide"},{"slug":"b","visibility":"list"},{"slug":"a","visibility":"list"}]}"#;
        assert_eq!(parse_codex_models(json).unwrap(), ["default", "a", "b"]);
        assert!(parse_codex_models(br#"{"models":[]}"#).is_err());
        assert!(parse_codex_models(b"nope").is_err());
    }

    #[test]
    fn agy_failure_status_is_an_error() {
        let line = r#"{"event":"result","result":{"status":"ERROR","error":"not signed in"}}"#;
        assert_eq!(
            parse_line(Kind::Agy, line),
            Some(CliEvent::Failed("not signed in".into()))
        );
        assert_eq!(
            parse_line(
                Kind::Agy,
                r#"{"event":"result","result":{"status":"TIMEOUT"}}"#
            ),
            Some(CliEvent::Failed("agy did not finish the turn".into()))
        );
    }

    #[test]
    fn agy_home_replaces_mcp_and_settings_but_keeps_the_login() {
        let real = std::env::temp_dir().join(format!("agy-real-{}", uuid::Uuid::new_v4()));
        let g = real.join(".gemini");
        std::fs::create_dir_all(g.join("config")).unwrap();
        for f in [
            "oauth_creds.json",
            "settings.json",
            "settings.nix",
            "config/config.json",
            "config/mcp_config.json",
            "config/mcp_config.sync-conflict.json",
        ] {
            std::fs::write(g.join(f), "real").unwrap();
        }
        let dir = real.join("run");
        std::fs::create_dir_all(&dir).unwrap();
        // SAFETY: only this test touches HOME, and it restores it before returning.
        let old = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", &real) };
        let home = agy_home(&dir).unwrap();
        unsafe { std::env::set_var("HOME", old.unwrap_or_default()) };
        let read = |p: &str| std::fs::read_to_string(home.join(p)).unwrap();
        assert_eq!(read(".gemini/oauth_creds.json"), "real");
        assert_eq!(read(".gemini/config/config.json"), "real");
        assert_eq!(
            read(".gemini/config/mcp_config.json"),
            r#"{"mcpServers":{}}"#
        );
        assert_eq!(read(".gemini/settings.json"), "{}");
        assert!(!home.join(".gemini/settings.nix").exists());
        assert!(
            !home
                .join(".gemini/config/mcp_config.sync-conflict.json")
                .exists()
        );
        let _ = std::fs::remove_dir_all(&real);
    }

    #[test]
    fn codex_fixture_yields_one_delta_then_done() {
        assert_eq!(
            events(Kind::Codex, "codex-exec.ndjson"),
            [CliEvent::Delta("hi there".into()), CliEvent::Done(None)]
        );
    }

    #[test]
    fn errors_and_noise_are_classified() {
        assert_eq!(
            parse_line(
                Kind::Claude,
                r#"{"type":"result","is_error":true,"result":"not logged in"}"#
            ),
            Some(CliEvent::Failed("not logged in".into()))
        );
        assert_eq!(
            parse_line(
                Kind::Codex,
                r#"{"type":"turn.failed","error":{"message":"nope"}}"#
            ),
            Some(CliEvent::Failed("nope".into()))
        );
        assert_eq!(parse_line(Kind::Codex, "not json"), None);
    }

    #[test]
    fn argv_follows_the_plan_and_never_passes_dangerous_flags() {
        let claude = claude_args("haiku", Path::new("/t/system.md"));
        assert!(claude.contains(&"/t/system.md".to_owned()));
        assert!(!claude.contains(&"--system-prompt".to_owned()));
        assert!(claude.windows(2).any(|w| w == ["--tools", ""]));
        assert!(claude.windows(2).any(|w| w == ["--model", "haiku"]));
        let codex = codex_args("default", Path::new("/t"), &["/t/a.png".into()]);
        assert!(codex.windows(2).any(|w| w == ["-s", "read-only"]));
        assert!(!codex.contains(&"-m".to_owned()));
        assert_eq!(codex.last().unwrap(), "-");
        assert!(
            ![&claude, &codex]
                .iter()
                .any(|a| a.iter().any(|s| s.contains("dangerously")))
        );
    }

    #[tokio::test]
    async fn cancel_kills_the_whole_group() {
        let dir = std::env::temp_dir().join(format!("cli-cancel-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake.sh");
        // The sleep is a grandchild: only a group kill ends it.
        std::fs::write(&script, "#!/bin/sh\nsleep 60 &\necho $! > pid\nwait\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (tx, rx) = watch::channel(false);
        let cancel = async {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            tx.send(true).unwrap();
        };
        let started = std::time::Instant::now();
        let run = run(
            script.to_str().unwrap(),
            Kind::Claude,
            vec![],
            String::new(),
            &dir,
            Arc::new(rx),
            |_| std::future::ready(()),
        );
        let (_, answer) = tokio::join!(cancel, run);
        assert_eq!(answer.unwrap(), "");
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        let pid: i32 = std::fs::read_to_string(dir.join("pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        // SAFETY: signal 0 only probes for existence.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "grandchild survived");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn child_touching_the_terminal_cannot_stall() {
        let dir = std::env::temp_dir().join(format!("cli-tty-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nstty -a </dev/tty >/dev/null 2>&1\necho '{\"type\":\"result\",\"result\":\"ok\"}'\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (_tx, rx) = watch::channel(false);
        let answer = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run(
                script.to_str().unwrap(),
                Kind::Claude,
                vec![],
                String::new(),
                &dir,
                Arc::new(rx),
                |_| std::future::ready(()),
            ),
        )
        .await
        .expect("child stalled on the terminal")
        .unwrap();
        assert_eq!(answer, "ok");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn exit_without_answer_reports_stderr() {
        let dir = std::env::temp_dir();
        let (_tx, rx) = watch::channel(false);
        let error = run(
            "sh",
            Kind::Codex,
            vec!["-c".into(), "echo boom >&2; exit 3".into()],
            String::new(),
            &dir,
            Arc::new(rx),
            |_| std::future::ready(()),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("boom"), "{error}");
    }

    #[test]
    fn temp_dir_is_private_and_removed_on_drop() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let path = dir.0.clone();
        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o700);
        let file = dir.write("system.md", b"secret").unwrap();
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        drop(dir);
        assert!(!path.exists());
    }

    #[test]
    fn on_path_finds_real_binaries_only() {
        assert!(on_path("sh"));
        assert!(!on_path("definitely-not-a-real-binary-xyz"));
    }

    #[test]
    fn flag_like_model_ids_are_rejected() {
        assert!(check_model("-x").is_err());
        assert!(check_model("haiku").is_ok());
    }

    #[tokio::test]
    async fn dropping_the_future_kills_the_group() {
        let dir = TempDir::new().unwrap();
        let script = dir
            .write(
                "fake.sh",
                b"#!/bin/sh\nsleep 60 &\necho $! > pid\necho $$ > self\nwait\n",
            )
            .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (_tx, rx) = watch::channel(false);
        let run = run(
            script.to_str().unwrap(),
            Kind::Claude,
            vec![],
            String::new(),
            &dir.0,
            Arc::new(rx),
            |_| std::future::ready(()),
        );
        // The timeout drops the future mid-run.
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(500), run)
                .await
                .is_err()
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        for name in ["pid", "self"] {
            let pid: i32 = std::fs::read_to_string(dir.0.join(name))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            // SAFETY: signal 0 only probes for existence.
            assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "{name} survived");
        }
    }
}
