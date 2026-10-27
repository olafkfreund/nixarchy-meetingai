//! OpenCode models (`provider/model`) through a private `opencode serve`.
//!
//! Tested against opencode 1.18.x. API used: the server prints
//! `opencode server listening on http://127.0.0.1:<port>`; `POST /session` (with a deny-all
//! permission ruleset), `GET /event` (SSE), `POST /session/{id}/prompt_async` (204),
//! `POST /session/{id}/abort`, `DELETE /session/{id}`, `GET /config/providers`.
//! The server runs in a 0700 temp dir with a private config, so none of the user's MCP servers
//! or plugins start (they made each call slow); the login travels in `OPENCODE_AUTH_CONTENT`, read
//! fresh from the real `auth.json` at each server start, so a refreshed token never lands in the
//! temp dir (a refresh inside the server is not saved; run `opencode auth login` to renew). It is protected
//! by a random basic-auth password, started on first use and stopped after 10 minutes idle or
//! when the daemon exits.

use std::collections::HashSet;
use std::future::Future;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::watch;

use super::cli::{TempDir, kill_group, quoted_transcript};
use super::engine::{ChatError, TurnContext, wait_for_cancellation};
use crate::ai::providers::ProviderConfig;
use crate::core::domain::Message;

const IDLE_LIMIT: Duration = Duration::from_secs(10 * 60);
const MAX_ANSWER_BYTES: usize = 2 * 1024 * 1024;
const MAX_EVENT_BYTES: usize = 1024 * 1024;
const TURN_TIMEOUT: Duration = Duration::from_secs(300);

struct Server {
    child: Child,
    base: String,
    password: String,
    _dir: TempDir,
}

static SERVER: tokio::sync::Mutex<Option<Server>> = tokio::sync::Mutex::const_new(None);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static LAST_USE: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
static WATCHING: AtomicBool = AtomicBool::new(false);

fn touch() {
    *LAST_USE.lock().expect("last use") = Some(Instant::now());
}

/// Counts a running turn so the idle stop never kills a server mid-answer.
struct Active;

impl Active {
    fn new() -> Self {
        ACTIVE.fetch_add(1, Ordering::SeqCst);
        touch();
        Self
    }
}

impl Drop for Active {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
        touch();
    }
}

fn idle_expired(active: usize, last: Option<Instant>, now: Instant, limit: Duration) -> bool {
    active == 0 && last.is_none_or(|l| now.saturating_duration_since(l) >= limit)
}

/// The port from `opencode server listening on http://127.0.0.1:4096`.
fn parse_base(line: &str) -> Option<String> {
    let rest = line.split("http://127.0.0.1:").nth(1)?;
    let port: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (!port.is_empty()).then(|| format!("http://127.0.0.1:{port}"))
}

/// The user's OpenCode login, read fresh from `auth.json`.
fn read_auth() -> Option<String> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".local/share"))
        })?;
    std::fs::read_to_string(data.join("opencode/auth.json"))
        .ok()
        .filter(|a| !a.trim().is_empty())
}

async fn start(program: &str, auth: Option<String>) -> Result<Server, ChatError> {
    let dir = TempDir::new()?;
    let io = |e: std::io::Error| ChatError::Cli(format!("could not prepare opencode: {e}"));
    let (data, conf, work) = (dir.0.join("data"), dir.0.join("conf"), dir.0.join("work"));
    std::fs::create_dir_all(&data).map_err(io)?;
    std::fs::create_dir_all(&conf).map_err(io)?;
    std::fs::create_dir_all(&work).map_err(io)?;
    let password = uuid::Uuid::new_v4().simple().to_string();
    let mut command = Command::new(program);
    command
        .args(["serve", "--hostname", "127.0.0.1", "--port", "0"])
        .current_dir(&work)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CONFIG_HOME", &conf)
        .env("OPENCODE_SERVER_PASSWORD", &password)
        .env("OPENCODE_DISABLE_AUTOUPDATE", "1")
        .env_remove("OPENCODE_SERVER_USERNAME")
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    super::cli::scrub_env(&mut command);
    match auth {
        Some(auth) => command.env("OPENCODE_AUTH_CONTENT", auth),
        None => command.env_remove("OPENCODE_AUTH_CONTENT"),
    };
    // SAFETY: setsid is async-signal-safe; it gives the server its own process group for shutdown.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ChatError::Cli(
            "`opencode` was not found in PATH; install it and run `opencode auth login`".into(),
        ),
        _ => ChatError::Cli(format!("could not start `opencode`: {e}")),
    })?;
    let mut lines = BufReader::new(child.stdout.take().expect("stdout piped")).lines();
    let base = tokio::time::timeout(Duration::from_secs(30), async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(base) = parse_base(&line) {
                return Some(base);
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(|| ChatError::Cli("`opencode serve` did not report a port".into()))?;
    // Keep reading so the server never blocks on a full pipe.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    Ok(Server {
        child,
        base,
        password,
        _dir: dir,
    })
}

fn stop(server: Server) {
    if let Some(pid) = server.child.id() {
        kill_group(pid);
    }
}

/// Stop the server now (daemon exit).
pub async fn shutdown() {
    if let Some(server) = SERVER.lock().await.take() {
        stop(server);
    }
}

fn watch_idle() {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let mut slot = SERVER.lock().await;
            let last = *LAST_USE.lock().expect("last use");
            // Cleared while the lock is held, so connect() cannot see a watcher that is exiting.
            if slot.is_none() {
                WATCHING.store(false, Ordering::SeqCst);
                break;
            }
            if idle_expired(
                ACTIVE.load(Ordering::SeqCst),
                last,
                Instant::now(),
                IDLE_LIMIT,
            ) {
                if let Some(server) = slot.take() {
                    stop(server);
                }
                WATCHING.store(false, Ordering::SeqCst);
                break;
            }
        }
    });
}

/// Base URL and password of the running server, starting it when needed.
async fn connect() -> Result<(String, String, Active), ChatError> {
    let mut slot = SERVER.lock().await;
    if let Some(server) = slot.as_mut()
        && !matches!(server.child.try_wait(), Ok(None))
    {
        *slot = None;
    }
    if slot.is_none() {
        *slot = Some(start("opencode", read_auth()).await?);
    }
    let server = slot.as_ref().expect("just started");
    let out = (server.base.clone(), server.password.clone(), Active::new());
    drop(slot);
    watch_idle();
    Ok(out)
}

/// `provider/model` ids from the server's configured providers.
pub async fn models() -> Result<Vec<String>, String> {
    let (base, password, _active) = connect().await.map_err(|e| e.to_string())?;
    let response = super::engine::http_client()
        .get(format!("{base}/config/providers"))
        .basic_auth("opencode", Some(&password))
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    let value: Value = response.json().await.map_err(|e| e.to_string())?;
    let models = parse_models(&value);
    if models.is_empty() {
        return Err("OpenCode lists no models; sign in with `opencode auth login`".into());
    }
    Ok(models)
}

/// Zen's zero-cost models are rate limited and run out, so they sort last (the dropdown's first
/// entry becomes the default); zero cost under another provider is a subscription, not a free tier.
fn parse_models(value: &Value) -> Vec<String> {
    let mut models: Vec<(bool, String)> = value["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|p| {
            let id = p["id"].as_str().unwrap_or_default();
            p["models"]
                .as_object()
                .into_iter()
                .flatten()
                .map(move |(name, m)| {
                    let free = id == "opencode"
                        && m["cost"]["input"] == json!(0)
                        && m["cost"]["output"] == json!(0);
                    (free, format!("{id}/{name}"))
                })
        })
        .filter(|(_, m)| !m.starts_with('/') && !m.contains(char::is_control))
        .collect();
    models.sort();
    models.dedup();
    models.into_iter().map(|(_, m)| m).collect()
}

#[derive(Debug, PartialEq, Eq)]
enum Step {
    Skip,
    Delta(String),
    Done,
    Fail(String),
}

/// Interpret one SSE event of session `sid`. Only deltas of text parts count (reasoning is skipped).
fn step(event: &Value, sid: &str, text_parts: &mut HashSet<String>) -> Step {
    let p = &event["properties"];
    let kind = event["type"].as_str().unwrap_or_default();
    // A session.error without a sessionID is a server-wide failure: it ends our turn too.
    let ours = p["sessionID"].as_str() == Some(sid)
        || (kind == "session.error" && p["sessionID"].is_null());
    if !ours {
        return Step::Skip;
    }
    let message = |v: &Value| {
        v["data"]["message"]
            .as_str()
            .or(v["name"].as_str())
            .unwrap_or("OpenCode reported an error")
            .to_owned()
    };
    match kind {
        "message.part.updated" if p["part"]["type"] == "text" => {
            if let Some(id) = p["part"]["id"].as_str() {
                text_parts.insert(id.to_owned());
            }
            Step::Skip
        }
        "message.part.delta" if p["field"] == "text" => {
            match (p["partID"].as_str(), p["delta"].as_str()) {
                (Some(id), Some(delta)) if text_parts.contains(id) && !delta.is_empty() => {
                    Step::Delta(delta.to_owned())
                }
                _ => Step::Skip,
            }
        }
        "session.status" if p["status"]["type"] == "retry" => Step::Fail(
            p["status"]["message"]
                .as_str()
                .unwrap_or("OpenCode could not reach the model")
                .to_owned(),
        ),
        "message.updated" if !p["info"]["error"].is_null() => {
            Step::Fail(message(&p["info"]["error"]))
        }
        "session.error" => Step::Fail(message(&p["error"])),
        "session.idle" => Step::Done,
        _ => Step::Skip,
    }
}

/// Stream one turn through the OpenCode server named by `config.model` (`provider/model`).
pub async fn stream<F: Future<Output = ()>>(
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
    cancel: Arc<watch::Receiver<bool>>,
    on_text: impl FnMut(String) -> F,
) -> Result<String, ChatError> {
    let (provider, model) = config
        .model
        .split_once('/')
        .filter(|(p, m)| !p.is_empty() && !m.is_empty())
        .ok_or_else(|| {
            ChatError::Cli(format!(
                "OpenCode models look like provider/model, not `{}`",
                config.model
            ))
        })?;
    let (last, earlier) = history
        .split_last()
        .ok_or_else(|| ChatError::Cli("there is nothing to send".into()))?;
    let mut text = String::new();
    if !earlier.is_empty() {
        text.push_str("Conversation so far (quoted):\n");
        text.push_str(&quoted_transcript(earlier));
        text.push_str("\n\n");
    }
    text.push_str(&last.content);
    let (base, password, _active) = connect().await?;
    turn(
        &base,
        &password,
        (provider, model),
        context.system.as_deref().unwrap_or_default(),
        &text,
        cancel,
        TURN_TIMEOUT,
        on_text,
    )
    .await
}

#[allow(clippy::too_many_arguments)] // one call site; a struct would only rename them
async fn turn<F: Future<Output = ()>>(
    base: &str,
    password: &str,
    (provider, model): (&str, &str),
    system: &str,
    text: &str,
    cancel: Arc<watch::Receiver<bool>>,
    limit: Duration,
    mut on_text: impl FnMut(String) -> F,
) -> Result<String, ChatError> {
    if *cancel.borrow() {
        return Ok(String::new());
    }
    let mut cancellation = (*cancel).clone();
    let client = super::engine::loading_http_client();
    let http = |e: reqwest::Error| ChatError::Cli(e.without_url().to_string());
    let get = |path: &str| {
        client
            .get(format!("{base}{path}"))
            .basic_auth("opencode", Some(password))
    };
    let post = |path: &str| {
        client
            .post(format!("{base}{path}"))
            .basic_auth("opencode", Some(password))
    };
    let created: Value = post("/session")
        // Every tool is denied for this session: it can only answer.
        .json(&json!({"permission": [{"permission": "*", "pattern": "*", "action": "deny"}]}))
        .send()
        .await
        .map_err(http)?
        .error_for_status()
        .map_err(http)?
        .json()
        .await
        .map_err(http)?;
    let sid = created["id"]
        .as_str()
        .ok_or_else(|| ChatError::Cli("OpenCode created no session".into()))?
        .to_owned();
    let result = async {
        // Subscribe before asking so no event is missed.
        let events = get("/event")
            .send()
            .await
            .map_err(http)?
            .error_for_status()
            .map_err(http)?;
        let sent = post(&format!("/session/{sid}/prompt_async"))
            .json(&json!({
                "model": {"providerID": provider, "modelID": model},
                "system": system,
                "tools": {},
                "parts": [{"type": "text", "text": text}],
            }))
            .send()
            .await
            .map_err(http)?;
        if !sent.status().is_success() {
            return Err(ChatError::Cli(format!(
                "OpenCode refused the message (HTTP {})",
                sent.status().as_u16()
            )));
        }
        let mut stream = events.bytes_stream();
        let (mut buffer, mut answer) = (Vec::new(), String::new());
        let mut text_parts = HashSet::new();
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let chunk = tokio::select! {
                chunk = stream.next() => match chunk { Some(c) => c.map_err(http)?, None => {
                    return Err(ChatError::Cli("OpenCode closed the event stream".into()));
                } },
                _ = wait_for_cancellation(&mut cancellation) => return Ok(answer),
                () = tokio::time::sleep_until(deadline) => {
                    return Err(ChatError::Cli(format!(
                        "OpenCode did not finish within {} s", limit.as_secs()
                    )));
                }
            };
            buffer.extend_from_slice(&chunk);
            while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buffer.drain(..=end).collect();
                let Some(json) = std::str::from_utf8(&line)
                    .ok()
                    .and_then(|l| l.trim().strip_prefix("data:"))
                else {
                    continue;
                };
                let Ok(event) = serde_json::from_str::<Value>(json.trim()) else {
                    continue;
                };
                match step(&event, &sid, &mut text_parts) {
                    Step::Skip => {}
                    Step::Delta(delta) => {
                        if delta.len() > MAX_ANSWER_BYTES.saturating_sub(answer.len()) {
                            return Err(ChatError::Cli("answer exceeded the size limit".into()));
                        }
                        answer.push_str(&delta);
                        tokio::select! {
                            _ = on_text(delta) => {},
                            _ = wait_for_cancellation(&mut cancellation) => return Ok(answer),
                        }
                    }
                    Step::Done if answer.is_empty() => {
                        return Err(ChatError::Cli("OpenCode returned no answer".into()));
                    }
                    Step::Done => return Ok(answer),
                    Step::Fail(message) => return Err(ChatError::Cli(message)),
                }
            }
            // Only an unfinished line is left in the buffer: that is what the limit bounds.
            if buffer.len() > MAX_EVENT_BYTES {
                return Err(ChatError::Cli(
                    "OpenCode event exceeded the size limit".into(),
                ));
            }
        }
    }
    .await;
    // Best effort: stop a still-running turn and drop the session.
    let _ = post(&format!("/session/{sid}/abort")).send().await;
    let _ = client
        .delete(format!("{base}/session/{sid}"))
        .basic_auth("opencode", Some(password))
        .send()
        .await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn port_comes_from_the_listening_line() {
        assert_eq!(
            parse_base("opencode server listening on http://127.0.0.1:4096"),
            Some("http://127.0.0.1:4096".into())
        );
        assert_eq!(parse_base("Warning: not secured"), None);
    }

    #[test]
    fn models_are_provider_slash_model() {
        let value = json!({"providers": [
            {"id": "openai", "models": {"gpt-a": {}, "gpt-b": {}}},
            {"id": "opencode", "models": {"big-pickle": {"cost": {"input": 0, "output": 0}}, "paid": {"cost": {"input": 1, "output": 2}}}},
            {"id": "ollama-cloud", "models": {"gpt-oss:120b": {}}}]});
        // The free Zen model comes last, so the first entry is never one that ran out.
        assert_eq!(
            parse_models(&value),
            [
                "ollama-cloud/gpt-oss:120b",
                "openai/gpt-a",
                "openai/gpt-b",
                "opencode/paid",
                "opencode/big-pickle"
            ]
        );
        assert!(parse_models(&json!({})).is_empty());
    }

    #[test]
    fn idle_stop_waits_for_the_limit_and_for_running_turns() {
        let now = Instant::now();
        let limit = Duration::from_secs(600);
        assert!(!idle_expired(0, Some(now), now, limit));
        assert!(idle_expired(0, Some(now), now + limit, limit));
        assert!(!idle_expired(1, Some(now), now + limit, limit));
    }

    #[test]
    fn only_text_deltas_of_this_session_become_answer_text() {
        let mut parts = HashSet::new();
        let ev = |t: &str, p: Value| json!({"type": t, "properties": p});
        let sid = "s1";
        let reasoning = ev(
            "message.part.updated",
            json!({"sessionID": sid, "part": {"id": "r", "type": "reasoning"}}),
        );
        let text = ev(
            "message.part.updated",
            json!({"sessionID": sid, "part": {"id": "t", "type": "text"}}),
        );
        assert_eq!(step(&reasoning, sid, &mut parts), Step::Skip);
        assert_eq!(step(&text, sid, &mut parts), Step::Skip);
        let delta = |part: &str, session: &str| {
            ev(
                "message.part.delta",
                json!({"sessionID": session, "partID": part, "field": "text", "delta": "hi"}),
            )
        };
        assert_eq!(step(&delta("r", sid), sid, &mut parts), Step::Skip);
        assert_eq!(step(&delta("t", "other"), sid, &mut parts), Step::Skip);
        assert_eq!(
            step(&delta("t", sid), sid, &mut parts),
            Step::Delta("hi".into())
        );
        assert_eq!(
            step(
                &ev("session.idle", json!({"sessionID": sid})),
                sid,
                &mut parts
            ),
            Step::Done
        );
        let retry = ev(
            "session.status",
            json!({"sessionID": sid, "status": {"type": "retry", "message": "Free usage exceeded"}}),
        );
        assert_eq!(
            step(&retry, sid, &mut parts),
            Step::Fail("Free usage exceeded".into())
        );
        let failed = ev(
            "message.updated",
            json!({"sessionID": sid, "info": {"error": {"name": "X", "data": {"message": "bad key"}}}}),
        );
        assert_eq!(step(&failed, sid, &mut parts), Step::Fail("bad key".into()));
    }

    /// A tiny OpenCode: records requests, answers the four endpoints the client uses.
    async fn fake_server(events: String) -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let (events, log) = (events.clone(), log.clone());
                tokio::spawn(async move {
                    let mut raw = Vec::new();
                    let mut buf = [0u8; 4096];
                    // Read headers, then the body by Content-Length.
                    let (head_end, length) = loop {
                        let n = socket.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        raw.extend_from_slice(&buf[..n]);
                        if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&raw[..i]).to_lowercase();
                            let length = head
                                .lines()
                                .find_map(|l| {
                                    l.strip_prefix("content-length: ")?.trim().parse().ok()
                                })
                                .unwrap_or(0usize);
                            break (i + 4, length);
                        }
                    };
                    while raw.len() < head_end + length {
                        let n = socket.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        raw.extend_from_slice(&buf[..n]);
                    }
                    let text = String::from_utf8_lossy(&raw).into_owned();
                    log.lock().unwrap().push(text.clone());
                    let first = text.lines().next().unwrap_or_default().to_owned();
                    let (status, content_type, body) = if first.starts_with("POST /session ") {
                        ("200 OK", "application/json", r#"{"id":"s1"}"#.to_owned())
                    } else if first.starts_with("GET /event") {
                        ("200 OK", "text/event-stream", events)
                    } else if first.contains("prompt_async") {
                        ("204 No Content", "text/plain", String::new())
                    } else {
                        ("200 OK", "application/json", "true".to_owned())
                    };
                    if body == "STALL" {
                        let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n";
                        let _ = socket.write_all(head.as_bytes()).await;
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        return;
                    }
                    let reply = format!(
                        "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(reply.as_bytes()).await;
                });
            }
        });
        (base, seen)
    }

    fn sse(events: &[Value]) -> String {
        events.iter().map(|e| format!("data: {e}\n\n")).collect()
    }

    #[tokio::test]
    async fn a_turn_creates_a_denied_session_streams_text_and_cleans_up() {
        let part = |id: &str, kind: &str| json!({"type": "message.part.updated", "properties": {"sessionID": "s1", "part": {"id": id, "type": kind}}});
        let delta = |id: &str, d: &str| json!({"type": "message.part.delta", "properties": {"sessionID": "s1", "partID": id, "field": "text", "delta": d}});
        let (base, seen) = fake_server(sse(&[
            part("r", "reasoning"),
            delta("r", "thinking"),
            part("t", "text"),
            delta("t", "Hello "),
            delta("t", "world"),
            json!({"type": "session.idle", "properties": {"sessionID": "s1"}}),
        ]))
        .await;
        let (_tx, rx) = watch::channel(false);
        let mut deltas = Vec::new();
        let answer = turn(
            &base,
            "pw",
            ("openai", "gpt-a"),
            "be brief",
            "hi",
            Arc::new(rx),
            Duration::from_secs(30),
            |d| {
                deltas.push(d);
                std::future::ready(())
            },
        )
        .await
        .unwrap();
        assert_eq!(answer, "Hello world");
        assert_eq!(deltas, ["Hello ", "world"]);
        let log = seen.lock().unwrap().join("\n---\n");
        assert!(log.contains(r#""action":"deny""#), "{log}");
        assert!(log.contains(r#""providerID":"openai""#) && log.contains(r#""system":"be brief""#));
        assert!(log.to_lowercase().contains("authorization: basic"));
        assert!(
            log.contains("/abort") && log.contains("DELETE /session/s1"),
            "{log}"
        );
    }

    #[tokio::test]
    async fn a_provider_error_reaches_the_caller() {
        let (base, _) = fake_server(sse(&[json!({"type": "session.status", "properties": {"sessionID": "s1", "status": {"type": "retry", "message": "Free usage exceeded"}}})])).await;
        let (_tx, rx) = watch::channel(false);
        let error = turn(
            &base,
            "pw",
            ("opencode", "big-pickle"),
            "",
            "hi",
            Arc::new(rx),
            Duration::from_secs(30),
            |_| std::future::ready(()),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("Free usage exceeded"), "{error}");
    }

    #[tokio::test]
    async fn the_server_start_reads_the_port_and_stop_kills_its_group() {
        let dir = std::env::temp_dir().join(format!("oc-fake-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("opencode");
        let env_dump = dir.join("env");
        std::fs::write(&script, format!("#!/bin/sh\nenv > {}\necho 'Warning: unsecured'\necho 'opencode server listening on http://127.0.0.1:4242'\nexec sleep 60\n", env_dump.display())).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let server = start(script.to_str().unwrap(), Some(r#"{"x":"tok"}"#.into()))
            .await
            .unwrap();
        assert_eq!(server.base, "http://127.0.0.1:4242");
        // The login is passed in the environment; no auth.json is written or linked.
        let env = std::fs::read_to_string(&env_dump).unwrap();
        assert!(
            env.contains(r#"OPENCODE_AUTH_CONTENT={"x":"tok"}"#),
            "no auth env"
        );
        assert!(!env.contains("OPENCODE_SERVER_USERNAME"));
        assert!(!server._dir.0.join("data/opencode/auth.json").exists());
        let pid = server.child.id().unwrap() as i32;
        stop(server);
        tokio::time::sleep(Duration::from_millis(200)).await;
        // SAFETY: signal 0 only probes for existence.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "server survived");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_silent_server_times_out_with_a_failover_error() {
        let (base, _) = fake_server("STALL".into()).await;
        let (_tx, rx) = watch::channel(false);
        let error = turn(
            &base,
            "pw",
            ("openai", "gpt-a"),
            "",
            "hi",
            Arc::new(rx),
            Duration::from_millis(300),
            |_| std::future::ready(()),
        )
        .await
        .unwrap_err();
        // Cli, so the chain moves on to the next provider.
        assert!(matches!(error, ChatError::Cli(_)), "{error}");
        assert!(error.to_string().contains("did not finish"), "{error}");
    }

    #[test]
    fn a_session_error_without_a_session_id_ends_the_turn() {
        let mut parts = HashSet::new();
        let event = json!({"type": "session.error", "properties": {"error": {"name": "ProviderAuthError", "data": {"message": "expired login"}}}});
        assert_eq!(
            step(&event, "s1", &mut parts),
            Step::Fail("expired login".into())
        );
        let other =
            json!({"type": "session.error", "properties": {"sessionID": "s2", "error": {}}});
        assert_eq!(step(&other, "s1", &mut parts), Step::Skip);
    }

    #[tokio::test]
    async fn many_small_events_in_one_burst_do_not_trip_the_line_limit() {
        let noise = json!({"type": "server.heartbeat", "properties": {"pad": "x".repeat(80)}});
        let mut events = vec![noise; 20_000];
        events.push(json!({"type": "message.part.updated", "properties": {"sessionID": "s1", "part": {"id": "t", "type": "text"}}}));
        events.push(json!({"type": "message.part.delta", "properties": {"sessionID": "s1", "partID": "t", "field": "text", "delta": "ok"}}));
        events.push(json!({"type": "session.idle", "properties": {"sessionID": "s1"}}));
        let (base, _) = fake_server(sse(&events)).await;
        assert!(sse(&events).len() > 2 * MAX_EVENT_BYTES);
        let (_tx, rx) = watch::channel(false);
        let answer = turn(
            &base,
            "pw",
            ("openai", "gpt-a"),
            "",
            "hi",
            Arc::new(rx),
            Duration::from_secs(30),
            |_| std::future::ready(()),
        )
        .await
        .unwrap();
        assert_eq!(answer, "ok");
    }
}
