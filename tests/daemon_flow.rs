//! The daemon end to end over its socket, with a fake chat provider and fake
//! transcript lines. No microphone, network or API key is involved.
use nixarchy_meetingai::{
    cli,
    core::{credentials, vault::Vault},
    daemon::{Daemon, Options, server},
    speech::transcription::{LiveEvent, TranscriptLine},
};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, UnixStream, unix::OwnedWriteHalf},
};

const SECRET: &str = "sk-very-secret-key-123";

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
                let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Mock answer.\"}}]}\n\ndata: [DONE]\n";
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

struct Client {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: OwnedWriteHalf,
    events: Vec<Value>,
    next: u64,
}

impl Client {
    async fn connect(path: &PathBuf) -> Self {
        let (reader, writer) = UnixStream::connect(path).await.unwrap().into_split();
        Self {
            reader: BufReader::new(reader),
            writer,
            events: Vec::new(),
            next: 0,
        }
    }

    async fn line(&mut self) -> Option<Value> {
        let mut text = String::new();
        let n = tokio::time::timeout(Duration::from_secs(20), self.reader.read_line(&mut text))
            .await
            .expect("timed out waiting for the daemon")
            .unwrap_or(0);
        (n > 0).then(|| serde_json::from_str(&text).unwrap())
    }

    async fn call(&mut self, cmd: &str, args: Value) -> Value {
        self.next += 1;
        let id = self.next;
        let request = format!("{}\n", json!({"id": id, "cmd": cmd, "args": args}));
        self.writer.write_all(request.as_bytes()).await.unwrap();
        loop {
            let value = self.line().await.expect("socket closed");
            if value["id"] == id {
                return value;
            }
            self.events.push(value);
        }
    }

    async fn event(&mut self, name: &str) -> Value {
        loop {
            if let Some(i) = self.events.iter().position(|e| e["event"] == name) {
                return self.events.remove(i);
            }
            let value = self.line().await.expect("socket closed");
            self.events.push(value);
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
async fn daemon_flow_scripted_session() {
    let root = std::env::temp_dir().join(format!("nixarchy-daemon-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let vault_dir = root.join("data");
    let provider = fake_provider().await;
    {
        let vault = Vault::open(&vault_dir).unwrap();
        let mut credential = credentials::Credential::chat("gemini", SECRET);
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
    let server = {
        let (socket, daemon) = (socket.clone(), daemon.clone());
        tokio::spawn(async move { server::serve(&socket, daemon).await })
    };
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mode = std::fs::metadata(socket.parent().unwrap()).unwrap();
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&mode.permissions()) & 0o777,
        0o700
    );

    let mut client = Client::connect(&socket).await;
    assert_eq!(client.call("bogus.command", json!({})).await["ok"], false);
    for (key, value) in [
        ("chat_provider", json!("gemini")),
        ("chat_model", json!("")),
        ("auto_response", json!("off")),
    ] {
        let reply = client
            .call("settings.set", json!({"key": key, "value": value}))
            .await;
        assert_eq!(reply["ok"], true, "{reply}");
    }
    assert_eq!(client.call("session.new", json!({})).await["ok"], true);
    client.event("session.reset").await;
    assert_eq!(client.call("record.start", json!({})).await["ok"], true);
    let state = client.call("state.get", json!({})).await;
    assert_eq!(state["data"]["recording"], true);

    daemon.inject_live(final_line(1, "We should ship on Friday.", 0.0));
    daemon.inject_live(final_line(0, "Agreed, Friday works.", 2.0));
    let line = client.event("transcript.final").await;
    assert_eq!(line["who"], "system");
    assert_eq!(client.event("transcript.final").await["who"], "mic");

    let sent = client
        .call("chat.send", json!({"text": "Summarise the plan."}))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    let started = client.event("answer.started").await;
    assert_eq!(started["question"], "Summarise the plan.");
    assert_eq!(started["automatic"], false);
    assert_eq!(started["provider"], "gemini");
    let delta = client.event("answer.delta").await;
    assert_eq!(delta["text"], "Mock answer.");
    assert_eq!(delta["provider"], "gemini");
    assert_eq!(delta["model"], "mock");
    let found = client
        .call("settings.search", json!({"query": "provider"}))
        .await;
    assert!(!found.to_string().contains("chat_provider"), "{found}");
    assert!(!found.to_string().contains("live_provider"), "{found}");
    assert_eq!(client.event("answer.done").await["turn"], delta["turn"]);

    // Round 5 protocol over the socket: profiles, prompt reset, overlay geometry.
    let saved = client
        .call(
            "profiles.save",
            json!({"name": "Role", "personal_context": "Rust"}),
        )
        .await;
    assert_eq!(saved["ok"], true, "{saved}");
    let listed = client.call("profiles.list", json!({})).await;
    assert_eq!(listed["data"][0]["documents"], json!([]), "{listed}");
    assert_eq!(listed["data"][0]["personal_context"], "Rust");
    let reset = client
        .call(
            "prompts.reset",
            json!({"mode": "interview", "kind": "live_answer"}),
        )
        .await;
    assert!(
        reset["data"]["text"].as_str().unwrap().contains("bold"),
        "{reset}"
    );
    let geometry = client
        .call("settings.set", json!({"key": "overlay_h", "value": 480}))
        .await;
    assert_eq!(geometry["ok"], true, "{geometry}");
    assert_eq!(
        client.call("settings.get", json!({})).await["data"]["overlay_h"],
        480
    );

    // A second client sees events too, and the secret never crosses the socket.
    let mut other = Client::connect(&socket).await;
    let settings = other.call("settings.get", json!({})).await;
    assert_eq!(settings["data"]["auto_response"], "off");
    assert!(!settings.to_string().contains(SECRET));
    let providers = other.call("providers.list", json!({})).await;
    assert!(providers.to_string().contains("\"has_key\":true"));
    assert!(!providers.to_string().contains(SECRET));
    // A settings change reaches the other clients too.
    let set = client
        .call(
            "settings.set",
            json!({"key": "auto_response", "value": "off"}),
        )
        .await;
    assert_eq!(set["ok"], true, "{set}");
    assert_eq!(
        other.event("settings").await["settings"]["auto_response"],
        "off"
    );
    // A key is removed only on null, never on an empty string.
    let empty = client
        .call(
            "settings.set",
            json!({"key": "api_key", "provider": "gemini", "value": ""}),
        )
        .await;
    assert_eq!(empty["ok"], false, "{empty}");

    // An oversize line closes the client.
    let mut raw = UnixStream::connect(&socket).await.unwrap();
    let _ = raw.write_all(&vec![b'a'; 1024 * 1024 + 2]).await;
    let mut rest = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(10), raw.read_to_end(&mut rest)).await;
    assert!(closed.is_ok(), "the daemon kept an oversize client open");

    assert_eq!(client.call("record.stop", json!({})).await["ok"], true);
    // Finalization is under way: switching sessions is refused, and quit waits for it.
    assert_eq!(client.call("session.new", json!({})).await["ok"], false);
    assert_eq!(client.call("quit", json!({})).await["ok"], true);
    let notes = client.event("notes.updated").await;
    let meeting = notes["meeting"].as_str().unwrap().to_owned();
    let text = client.call("notes.get", json!({"meeting": meeting})).await;
    assert!(
        text["data"]["text"]
            .as_str()
            .unwrap()
            .contains("Mock answer"),
        "{text}"
    );
    let list = client.call("session.list", json!({})).await;
    assert!(
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == meeting.as_str())
    );

    tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!socket.exists());
}
