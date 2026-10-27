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
    fake_provider_with(|_| "Mock answer.".into()).await
}

/// Like [`fake_provider`], but the reply is computed from the request text.
async fn fake_provider_with(reply: fn(&str) -> String) -> String {
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
                let content = reply(&String::from_utf8_lossy(&bytes));
                let body = format!(
                    "data: {}\n\ndata: [DONE]\n",
                    json!({"choices": [{"delta": {"content": content}}]})
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
        let mut credential = credentials::Credential::chat("ollama", SECRET);
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
        ("chat_provider", json!("ollama")),
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
    assert_eq!(started["provider"], "ollama");
    let delta = client.event("answer.delta").await;
    assert_eq!(delta["text"], "Mock answer.");
    assert_eq!(delta["provider"], "ollama");
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
            json!({"key": "api_key", "provider": "ollama", "value": ""}),
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

/// Chunk calls list one action; when earlier open items are offered, it continues the first.
fn team_reply(request: &str) -> String {
    if !request.contains("Summarise this slice") {
        return "Team recap.".into();
    }
    let continues = request
        .split("id=")
        .nth(1)
        .map(|rest| rest[..36].to_owned());
    let items = json!([{"kind": "action", "text": "Send the budget to finance", "owner": null,
        "due": null, "source_line": 1, "continues_item": continues}]);
    format!("Notes.\n```json\n{items}\n```")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn team_series_carries_an_action_into_the_next_meeting() {
    let root = std::env::temp_dir().join(format!("nixarchy-team-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let vault_dir = root.join("data");
    {
        let vault = Vault::open(&vault_dir).unwrap();
        let mut credential = credentials::Credential::chat("ollama", SECRET);
        credential.model = "mock".into();
        credential.base_url = fake_provider_with(team_reply).await;
        credentials::save(&vault, &credential).unwrap();
    }
    let daemon = Daemon::new(Options {
        db_path: vault_dir.join("test.db"),
        vault_dir,
        capture: false,
    })
    .unwrap();
    let socket = cli::socket_path_in(&root.join("run"));
    {
        let (socket, daemon) = (socket.clone(), daemon.clone());
        tokio::spawn(async move { server::serve(&socket, daemon).await });
    }
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut client = Client::connect(&socket).await;
    for (key, value) in [
        ("chat_provider", json!("ollama")),
        ("chat_model", json!("")),
        ("auto_response", json!("off")),
        ("meeting_mode", json!("team_meeting")),
    ] {
        let set = client
            .call("settings.set", json!({"key": key, "value": value}))
            .await;
        assert_eq!(set["ok"], true, "{set}");
    }
    let series = client
        .call("series.create", json!({"name": "Weekly"}))
        .await["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Meeting 1: a chunk is due after 5 minutes of speech and yields one action.
    let new = client.call("session.new", json!({"series": series})).await;
    assert_eq!(new["ok"], true, "{new}");
    assert_eq!(
        client.call("state.get", json!({})).await["data"]["series"],
        series
    );
    assert_eq!(client.call("record.start", json!({})).await["ok"], true);
    daemon.inject_live(final_line(
        1,
        "Someone should send the budget to finance.",
        0.0,
    ));
    tokio::time::sleep(Duration::from_millis(300)).await;
    daemon.inject_live(final_line(1, "Yes, by Friday.", 400.0));
    let first = client.event("items.updated").await["meeting"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(client.call("record.stop", json!({})).await["ok"], true);
    client.event("notes.updated").await;
    let listed = client.call("items.list", json!({"meeting": first})).await["data"].clone();
    let item = &listed[0];
    assert_eq!(
        (listed.as_array().unwrap().len(), &item["kind"]),
        (1, &json!("action"))
    );
    assert_eq!(
        (&item["owner"], &item["status"], &item["series_id"]),
        (&Value::Null, &json!("open"), &json!(series))
    );
    assert_eq!(item["source_line"], 1);
    let id = item["id"].as_str().unwrap().to_owned();

    // Meeting 2 sees it as last time's open item and is offered it as an earlier task.
    let mut started = false;
    for _ in 0..100 {
        if client.call("session.new", json!({"series": series})).await["ok"] == true {
            started = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(started, "the first meeting never finished");
    let last = client
        .call("last_time.get", json!({"series": series}))
        .await["data"]
        .clone();
    assert_eq!(
        (&last["meeting"], &last["items"][0]["id"]),
        (&json!(first), &json!(id))
    );
    assert_eq!(client.call("record.start", json!({})).await["ok"], true);
    daemon.inject_live(final_line(1, "About that budget for finance.", 0.0));
    tokio::time::sleep(Duration::from_millis(300)).await;
    daemon.inject_live(final_line(1, "Still pending.", 400.0));
    let earlier = client.event("earlier_item").await;
    assert_eq!(earlier["item"]["id"], id);
    assert_eq!(earlier["quote"], "About that budget for finance.");
    assert!(earlier["meeting"].is_string());
    let done = client
        .call("items.update", json!({"id": id, "status": "done"}))
        .await;
    assert_eq!(done["ok"], true, "{done}");
    let done = client
        .call("items.list", json!({"series": series, "status": "done"}))
        .await;
    assert_eq!(done["data"][0]["id"], id, "{done}");
    assert_eq!(client.call("record.stop", json!({})).await["ok"], true);
    assert_eq!(client.call("quit", json!({})).await["ok"], true);
}

/// Interviewer replies: questions with one real and one invented quote, probes, and a recap
/// that first passes judgement and then, after the reminder, reports evidence.
fn interviewer_reply(request: &str) -> String {
    if request.contains("Rate the candidate") {
        "Score 7/10.".into()
    } else if request.contains("previous answer contained an assessment") {
        "Evidence recap.".into()
    } else if request.contains("Question bank") {
        "Strong candidate, 7/10.".into()
    } else if request.contains("interview questions") {
        json!([
            {"text": "What was your part in the Rust migration?", "source": "Led the Rust migration", "competency": "ownership"},
            {"text": "Tell me about your Go years.", "source": "Ten years of Go", "competency": "depth"}
        ])
        .to_string()
    } else {
        "Probe: what was your part in the migration?".into()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn interviewer_session_then_delete_leaves_nothing() {
    let root = std::env::temp_dir().join(format!("nixarchy-iv-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let vault_dir = root.join("data");
    {
        let vault = Vault::open(&vault_dir).unwrap();
        let mut credential = credentials::Credential::chat("ollama", SECRET);
        credential.model = "mock".into();
        credential.base_url = fake_provider_with(interviewer_reply).await;
        credentials::save(&vault, &credential).unwrap();
    }
    let db_path = vault_dir.join("test.db");
    let daemon = Daemon::new(Options {
        db_path: db_path.clone(),
        vault_dir,
        capture: false,
    })
    .unwrap();
    let socket = cli::socket_path_in(&root.join("run"));
    {
        let (socket, daemon) = (socket.clone(), daemon.clone());
        tokio::spawn(async move { server::serve(&socket, daemon).await });
    }
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut client = Client::connect(&socket).await;
    for (key, value) in [
        ("chat_provider", json!("ollama")),
        ("chat_model", json!("")),
        ("live_provider", json!("ollama")),
        ("auto_response", json!("off")),
        ("response_pace", json!("Fast")),
        ("meeting_mode", json!("interviewer")),
    ] {
        let set = client
            .call("settings.set", json!({"key": key, "value": value}))
            .await;
        assert_eq!(set["ok"], true, "{set}");
    }

    // Prepare: a candidate, two documents and a question bank with the invented quote dropped.
    let id = client
        .call("candidate.create", json!({"name": "Ada"}))
        .await["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (kind, name, text) in [
        ("cv", "cv.txt", "Ada. Led the Rust migration of billing."),
        ("job", "job.txt", "Senior engineer."),
    ] {
        let path = root.join(name);
        std::fs::write(&path, text).unwrap();
        let attached = client
            .call(
                "candidate.attach",
                json!({"id": id, "kind": kind, "path": path}),
            )
            .await;
        assert_eq!(attached["ok"], true, "{attached}");
    }
    let generated = client
        .call("questions.generate", json!({"candidate": id}))
        .await;
    assert_eq!(generated["ok"], true, "{generated}");
    assert_eq!(generated["data"]["dropped"], 1);
    assert_eq!(generated["data"]["questions"].as_array().unwrap().len(), 1);
    client.event("questions.updated").await;
    let got = client.call("candidate.get", json!({"id": id})).await["data"].clone();
    assert_eq!(got["documents"].as_array().unwrap().len(), 2);
    let question = got["questions"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(got["questions"][0]["status"], "planned");

    // Consent is required once; then a mic line that matches the question marks it asked.
    assert_eq!(
        client.call("session.new", json!({"candidate": id})).await["ok"],
        true
    );
    let refused = client.call("record.start", json!({})).await;
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .starts_with("consent_required: "),
        "{refused}"
    );
    let started = client.call("record.start", json!({"consent": true})).await;
    assert_eq!(started["ok"], true, "{started}");
    let state = client.call("state.get", json!({})).await["data"].clone();
    assert_eq!(
        (&state["candidate"], &state["consent"]),
        (&json!(id), &json!(true))
    );
    daemon.inject_live(final_line(
        0,
        "What was your part in the Rust migration?",
        0.0,
    ));
    assert_eq!(client.event("questions.updated").await["candidate"], id);
    let got = client.call("candidate.get", json!({"id": id})).await["data"].clone();
    assert_eq!(got["questions"][0]["status"], "asked");
    let noted = client
        .call(
            "questions.update",
            json!({"id": question, "note": "Owned the cutover"}),
        )
        .await;
    assert_eq!(noted["data"]["note"], "Owned the cutover", "{noted}");

    // A candidate turn gets a probe even with automatic answers off.
    daemon.inject_live(final_line(
        1,
        "I led the migration of the billing system last year.",
        3.0,
    ));
    let probe = client.event("answer.started").await;
    assert_eq!(probe["automatic"], true);
    assert!(
        client.event("answer.delta").await["text"]
            .as_str()
            .unwrap()
            .starts_with("Probe:")
    );
    client.event("answer.done").await;

    // A verdict never reaches the overlay.
    let sent = client
        .call("chat.send", json!({"text": "Rate the candidate."}))
        .await;
    assert_eq!(sent["ok"], true, "{sent}");
    assert_eq!(
        client.event("answer.error").await["message"],
        "The answer contained an assessment and was withheld."
    );

    // The recap retries once with the reminder and reports evidence only.
    assert_eq!(client.call("record.stop", json!({})).await["ok"], true);
    let meeting = client.event("notes.updated").await["meeting"]
        .as_str()
        .unwrap()
        .to_owned();
    let notes = client.call("notes.get", json!({"meeting": meeting})).await;
    assert_eq!(notes["data"]["text"], "Evidence recap.", "{notes}");

    // Deleting the candidate removes everything.
    let mut deleted = json!({});
    for _ in 0..100 {
        deleted = client.call("candidates.delete", json!({"id": id})).await;
        if deleted["ok"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(deleted["ok"], true, "{deleted}");
    let db = nixarchy_meetingai::db::Database::open(&db_path).unwrap();
    for table in [
        "candidates",
        "candidate_documents",
        "interview_questions",
        "meetings",
        "meeting_lines",
        "meeting_note_chunks",
        "meeting_recaps",
        "meeting_items",
        "notes_fts",
        "conversations",
    ] {
        let rows: i64 = db
            .connection()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "{table} still has rows");
    }
    assert_eq!(client.call("quit", json!({})).await["ok"], true);
}

static CLOUD_HITS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Counts every request and answers with questions: a soft-hyphenated real quote, a 3-word quote
/// (too short), and 11 more good ones so the bank overflows 12.
fn counting_reply(_request: &str) -> String {
    CLOUD_HITS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let mut questions = vec![
        json!({"text": "Q hyphen?", "source": "led the  infrastructure team", "competency": null}),
        json!({"text": "Q short?", "source": "Ten years", "competency": null}),
    ];
    questions.extend((0..11).map(|i| json!({"text": format!("Q{i}?"), "source": "Led the infrastructure team", "competency": null})));
    json!(questions).to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_only_data_never_reaches_a_cloud_model_even_without_the_session_flag() {
    use std::sync::atomic::Ordering::SeqCst;
    let root = std::env::temp_dir().join(format!("nixarchy-lo-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let vault_dir = root.join("data");
    {
        let vault = Vault::open(&vault_dir).unwrap();
        let mut credential = credentials::Credential::chat("ollama", SECRET);
        credential.model = "mock-cloud".into();
        credential.base_url = fake_provider_with(counting_reply).await;
        credentials::save(&vault, &credential).unwrap();
    }
    let db_path = vault_dir.join("test.db");
    let daemon = Daemon::new(Options {
        db_path: db_path.clone(),
        vault_dir,
        capture: false,
    })
    .unwrap();
    let socket = cli::socket_path_in(&root.join("run"));
    {
        let (socket, daemon) = (socket.clone(), daemon.clone());
        tokio::spawn(async move { server::serve(&socket, daemon).await });
    }
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut client = Client::connect(&socket).await;
    for (key, value) in [
        ("chat_provider", json!("ollama")),
        ("chat_model", json!("")),
    ] {
        client
            .call("settings.set", json!({"key": key, "value": value}))
            .await;
    }
    let candidate = async |client: &mut Client, local: bool| -> String {
        let id = client
            .call("candidate.create", json!({"name": "Ada"}))
            .await["data"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let path = root.join(format!("cv-{id}.txt"));
        std::fs::write(&path, "Ada. Led the infra\u{ad}\nstructure team at Acme.").unwrap();
        client
            .call(
                "candidate.attach",
                json!({"id": id, "kind": "cv", "path": path}),
            )
            .await;
        client
            .call("candidate.local_only", json!({"id": id, "on": local}))
            .await;
        id
    };

    // C3: the session has no candidate, but the target is local-only: nothing goes to a cloud model (a -cloud ollama model).
    let local = candidate(&mut client, true).await;
    let refused = client
        .call("questions.generate", json!({"candidate": local}))
        .await;
    assert_ne!(refused["ok"], true, "{refused}");
    assert_eq!(
        CLOUD_HITS.load(SeqCst),
        0,
        "a local-only CV reached the cloud"
    );

    // C1: regenerating the notes of a local-only interview does not call the cloud either.
    {
        let db = nixarchy_meetingai::db::Database::open(&db_path).unwrap();
        let c = db.connection();
        let m = nixarchy_meetingai::db::meetings::start(c, "T", "", "en", None, Default::default())
            .unwrap();
        nixarchy_meetingai::db::meetings::add_line(c, &m, "them", None, "I led it", 0.0, 1.0)
            .unwrap();
        nixarchy_meetingai::db::repos::candidates::link_meeting(c, &local, &m).unwrap();
        client.call("notes.regenerate", json!({"meeting": m})).await;
        // Ollama is not running here, so the local attempt fails; that is the point.
        client.event("error").await;
    }
    assert_eq!(
        CLOUD_HITS.load(SeqCst),
        0,
        "local-only notes reached the cloud"
    );

    // M2: a quote broken by a soft hyphen and extra spaces is found; a 3-word quote is not;
    // 13 questions keep 12 at most and count the overflow as dropped.
    let open = candidate(&mut client, false).await;
    let generated = client
        .call("questions.generate", json!({"candidate": open}))
        .await;
    assert_eq!(generated["ok"], true, "{generated}");
    assert_eq!(generated["data"]["questions"].as_array().unwrap().len(), 11);
    // 1 short quote + 1 beyond the first 12.
    assert_eq!(generated["data"]["dropped"], 2);
    assert_eq!(CLOUD_HITS.load(SeqCst), 1);
    assert_eq!(client.call("quit", json!({})).await["ok"], true);
}
