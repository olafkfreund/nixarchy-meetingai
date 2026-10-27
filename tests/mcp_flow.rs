//! The MCP server (`nixarchy-meetingai mcp`) end to end: the real binary speaks
//! JSON-RPC on stdio to this test and talks to an in-process daemon over its
//! socket, with a fake chat provider. No microphone, network or API key.
use nixarchy_meetingai::{
    cli,
    core::{credentials, vault::Vault},
    daemon::{Daemon, Options, server},
    speech::transcription::{LiveEvent, TranscriptLine},
};
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, UnixStream},
    process::{Child, ChildStdin, ChildStdout, Command},
};

/// Answers every request with one streamed completion.
async fn fake_provider() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                loop {
                    let mut chunk = [0u8; 8192];
                    let n = socket.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                    let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") else {
                        continue;
                    };
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .map_or(0, |v| v.trim().parse().unwrap());
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
                let text = if String::from_utf8_lossy(&bytes).contains("three distinct") {
                    "What is next?\\nWho owns it?\\nWhen is it due?"
                } else {
                    "Mock answer."
                };
                let body = format!(
                    "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{text}\"}}}}]}}\n\ndata: [DONE]\n"
                );
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
            });
        }
    });
    url
}

/// A running `nixarchy-meetingai mcp` child spoken to in newline JSON-RPC.
struct Mcp {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Mcp {
    async fn start(runtime: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_nixarchy-meetingai"))
            .arg("mcp")
            .env("XDG_RUNTIME_DIR", runtime)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut mcp = Self {
            stdin: child.stdin.take().unwrap(),
            stdout: BufReader::new(child.stdout.take().unwrap()),
            _child: child,
            next: 0,
        };
        let init = mcp
            .rpc(
                "initialize",
                json!({"protocolVersion": "2025-06-18", "capabilities": {},
                       "clientInfo": {"name": "test", "version": "0"}}),
            )
            .await;
        assert_eq!(init["serverInfo"]["name"], "nixarchy-meetingai");
        let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        mcp.stdin
            .write_all(format!("{note}\n").as_bytes())
            .await
            .unwrap();
        mcp
    }

    async fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.stdin
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        loop {
            let mut text = String::new();
            let n = tokio::time::timeout(Duration::from_secs(30), self.stdout.read_line(&mut text))
                .await
                .expect("timed out waiting for the MCP server")
                .unwrap();
            assert!(n > 0, "the MCP server exited");
            let value: Value = serde_json::from_str(&text).unwrap();
            if value["id"] == id {
                assert!(value.get("error").is_none(), "{value}");
                return value["result"].clone();
            }
        }
    }

    /// The `tools/call` result: `(is_error, text, structured)`.
    async fn tool(&mut self, name: &str, arguments: Value) -> (bool, String, Value) {
        let r = self
            .rpc("tools/call", json!({"name": name, "arguments": arguments}))
            .await;
        (
            r["isError"] == true,
            r["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            r["structuredContent"].clone(),
        )
    }
}

/// One command straight on the daemon socket, as the shell would send it.
async fn socket_call(socket: &Path, cmd: &str, args: Value) -> Value {
    let (read, mut write) = UnixStream::connect(socket).await.unwrap().into_split();
    let line = format!("{}\n", json!({"id": 1, "cmd": cmd, "args": args}));
    write.write_all(line.as_bytes()).await.unwrap();
    let mut read = BufReader::new(read);
    loop {
        let mut text = String::new();
        read.read_line(&mut text).await.unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        if value["id"] == 1 {
            return value;
        }
    }
}

fn final_line(channel: u8, text: &str, start: f64) -> LiveEvent {
    LiveEvent::Transcript(TranscriptLine {
        channel,
        speaker: None,
        text: text.into(),
        is_final: true,
        speech_final: true,
        start,
        end: start + 2.0,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcp_flow() {
    // Unix socket paths are limited to 108 bytes: keep the runtime dir short.
    let short = |tag: &str| {
        let dir = std::env::temp_dir().join(format!(
            "nm.{tag}{}",
            &uuid::Uuid::new_v4().simple().to_string()[..6]
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    };

    // No daemon: the handshake works and every tool returns a clean error.
    let mut lonely = Mcp::start(&short("a")).await;
    let (is_error, text, _) = lonely.tool("meeting_status", json!({})).await;
    assert!(is_error && text.contains("daemon is not running"), "{text}");

    let root = short("d");
    let vault_dir = root.join("data");
    let provider = fake_provider().await;
    {
        let vault = Vault::open(&vault_dir).unwrap();
        let mut credential = credentials::Credential::chat("gemini", "sk-test");
        credential.model = "mock".into();
        credential.base_url = provider;
        credentials::save(&vault, &credential).unwrap();
    }
    let daemon = Daemon::new(Options {
        db_path: vault_dir.join("test.db"),
        vault_dir,
        capture: false,
    })
    .unwrap();
    let socket = cli::socket_path_in(&root.join("run"));
    let serving = {
        let (socket, daemon) = (socket.clone(), daemon.clone());
        tokio::spawn(async move { server::serve(&socket, daemon).await })
    };
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    for (key, value) in [
        ("chat_provider", json!("gemini")),
        ("chat_model", json!("")),
        ("auto_response", json!("off")),
    ] {
        let set = socket_call(&socket, "settings.set", json!({"key": key, "value": value})).await;
        assert_eq!(set["ok"], true, "{set}");
    }

    let mut mcp = Mcp::start(&root.join("run")).await;
    let tools = mcp.rpc("tools/list", json!({})).await;
    assert_eq!(tools["tools"].as_array().unwrap().len(), 14);

    let (is_error, _, data) = mcp.tool("meeting_status", json!({})).await;
    assert!(!is_error);
    assert_eq!(data["state"]["recording"], false);
    assert_eq!(data["now"]["status"], "idle");

    let (is_error, answer, data) = mcp
        .tool("ask", json!({"question": "Summarise the plan."}))
        .await;
    assert!(!is_error, "{answer}");
    assert_eq!(data["answer"], "Mock answer.");
    assert_eq!(
        (data["provider"].clone(), data["partial"].clone()),
        (json!("gemini"), json!(false))
    );
    assert!(
        !data["suggestions"].as_array().unwrap().is_empty(),
        "{data}"
    );
    let (is_error, text, _) = mcp.tool("ask", json!({"question": "x".repeat(8001)})).await;
    assert!(is_error && text.contains("question"), "{text}");

    // Recording by an agent is refused until the owner allows it.
    let (is_error, text, _) = mcp.tool("record_start", json!({})).await;
    assert!(is_error && text.contains("mcp_allow_record"), "{text}");
    let allow = socket_call(
        &socket,
        "settings.set",
        json!({"key": "mcp_allow_record", "value": true}),
    )
    .await;
    assert_eq!(allow["ok"], true, "{allow}");
    let (is_error, text, _) = mcp.tool("record_start", json!({})).await;
    assert!(!is_error, "{text}");

    daemon.inject_live(final_line(1, "We should ship on Friday.", 0.0));
    daemon.inject_live(final_line(0, "Agreed, Friday works.", 2.0));
    daemon.inject_live(final_line(1, "Then we are done.", 4.0));
    let (_, _, first) = mcp.tool("transcript_get", json!({"limit": 2})).await;
    let rows = first["lines"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let after = rows[1]["line"].as_i64().unwrap();
    let (_, _, rest) = mcp
        .tool("transcript_get", json!({"after_line": after}))
        .await;
    let rest = rest["lines"].as_array().unwrap();
    assert_eq!(
        (rest.len(), rest[0]["text"].clone()),
        (1, json!("Then we are done."))
    );
    let (is_error, _, _) = mcp.tool("transcript_get", json!({"limit": 501})).await;
    assert!(is_error);

    let current = mcp
        .rpc(
            "resources/read",
            json!({"uri": "meeting://current/transcript"}),
        )
        .await;
    let text = current["contents"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("ship on Friday") && text.contains("Agreed"),
        "{text}"
    );
    let listed = mcp.rpc("resources/list", json!({})).await;
    assert_eq!(
        listed["resources"][0]["uri"],
        "meeting://current/transcript"
    );

    // The allowlist is checked before any socket call.
    for key in ["api_key", "mcp_allow_record"] {
        let (is_error, text, _) = mcp
            .tool("settings_set", json!({"key": key, "value": "x"}))
            .await;
        assert!(is_error && text.contains("may not change"), "{text}");
    }
    let (is_error, text, _) = mcp
        .tool("settings_set", json!({"key": "text_scale", "value": 120}))
        .await;
    assert!(!is_error, "{text}");
    let settings = socket_call(&socket, "settings.get", json!({})).await;
    assert_eq!(settings["data"]["text_scale"], 120);
    assert_eq!(settings["data"]["mcp_allow_record"], true);

    let (is_error, text, data) = mcp.tool("series_list", json!({})).await;
    assert!(!is_error, "{text}");
    assert!(data["series"].is_array());
    let (is_error, text, _) = mcp.tool("meeting_items", json!({})).await;
    assert!(is_error, "{text}");
    let (is_error, text, data) = mcp.tool("meeting_items", json!({"series": "none"})).await;
    assert!(!is_error, "{text}");
    assert_eq!(data["items"].as_array().unwrap().len(), 0);

    let (is_error, text, _) = mcp.tool("record_stop", json!({})).await;
    assert!(!is_error, "{text}");
    assert_eq!(socket_call(&socket, "quit", json!({})).await["ok"], true);
    tokio::time::timeout(Duration::from_secs(10), serving)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
