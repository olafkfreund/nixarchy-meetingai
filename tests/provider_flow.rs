//! Provider traffic checked on the wire: chat, speech, recap and credentials against local fake servers.
use base64::Engine;
use nixarchy_meetingai::ai::catalog::custom::CustomProvider;
use nixarchy_meetingai::ai::catalog::presets::PRESETS;
use nixarchy_meetingai::ai::engine::{TurnContext, http_client, stream_chat};
use nixarchy_meetingai::ai::providers::ProviderConfig;
use nixarchy_meetingai::ai::recap;
use nixarchy_meetingai::ai::session::{ChatSession, chat_config};
use nixarchy_meetingai::audio::encode_wav;
use nixarchy_meetingai::core::credentials::{self, Credential, Kind};
use nixarchy_meetingai::core::domain::Message;
use nixarchy_meetingai::core::vault::Vault;
use nixarchy_meetingai::db::{Database, conversations, meetings};
use nixarchy_meetingai::speech::listen::transcribe_utterance;
use nixarchy_meetingai::speech::transcription::{LiveEvent, LiveTranscriber, transcribe};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;

/// Longest any single socket wait may take before the test is called hung.
const HANG: Duration = Duration::from_secs(10);
/// Replies leave the fake server this many bytes at a time.
const FRAGMENT: usize = 11;

/// One request as the fake server received it.
struct Seen {
    head: String,
    body: Vec<u8>,
}

impl Seen {
    fn parse(raw: &[u8]) -> Self {
        let end = find(raw, b"\r\n\r\n").expect("request has no header end");
        Self {
            head: String::from_utf8_lossy(&raw[..end]).into_owned(),
            body: raw[end + 4..].to_vec(),
        }
    }

    fn request_line(&self) -> &str {
        self.head.lines().next().unwrap_or_default()
    }

    fn header(&self, name: &str) -> Option<String> {
        self.head.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).expect("request body is not JSON")
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn declared_length(head: &[u8]) -> usize {
    String::from_utf8_lossy(head)
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())?
        })
        .unwrap_or(0)
}

/// Reads headers and exactly the declared body, or fails the test after `HANG`.
async fn read_request(socket: &mut TcpStream) -> Vec<u8> {
    timeout(HANG, async {
        let mut raw = Vec::new();
        let mut buf = [0u8; 3000];
        loop {
            if let Some(end) = find(&raw, b"\r\n\r\n")
                && raw.len() >= end + 4 + declared_length(&raw[..end])
            {
                return raw;
            }
            let n = socket.read(&mut buf).await.expect("read failed");
            assert!(n > 0, "client hung up before the request was complete");
            raw.extend_from_slice(&buf[..n]);
        }
    })
    .await
    .expect("request was never completed")
}

async fn write_slowly(socket: &mut TcpStream, bytes: &[u8]) {
    for part in bytes.chunks(FRAGMENT) {
        socket.write_all(part).await.unwrap();
        socket.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

fn head(status: u16, content_type: &str) -> String {
    format!("HTTP/1.1 {status} Reply\r\nContent-Type: {content_type}\r\nConnection: close\r\n\r\n")
}

/// Answers one connection with a fixed reply and returns what it was asked.
async fn serve(
    status: u16,
    content_type: &'static str,
    body: impl Into<Vec<u8>>,
) -> (String, JoinHandle<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = listener.local_addr().unwrap().to_string();
    let body = body.into();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.set_nodelay(true).unwrap();
        let raw = read_request(&mut socket).await;
        socket
            .write_all(head(status, content_type).as_bytes())
            .await
            .unwrap();
        write_slowly(&mut socket, &body).await;
        let _ = socket.shutdown().await;
        Seen::parse(&raw)
    });
    (host, task)
}

/// Receiver half of a cancel channel that lost its sender; that is not a stop request.
fn never_cancelled() -> Arc<watch::Receiver<bool>> {
    Arc::new(watch::channel(false).1)
}

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("nixarchy-flow-{tag}-{}", uuid::Uuid::new_v4()))
}

fn ollama(host: &str, key: &str, model: &str) -> ProviderConfig {
    let mut config = ProviderConfig::new("ollama", key, model);
    config.base_url = format!("http://{host}/v1/chat/completions");
    config
}

fn event_stream(events: &[&str]) -> String {
    events.iter().map(|e| format!("data: {e}\n\n")).collect()
}

fn question_turn(text: &str) -> Vec<Message> {
    vec![Message::user(text)]
}

#[tokio::test]
async fn anthropic_preset_keeps_only_visible_text_and_sends_its_wire_details() {
    let reply_json = json!({"content": [
        {"type": "thinking", "thinking": "weighing the release dates"},
        {"type": "text", "text": "Ship on Thursday."},
    ]})
    .to_string();
    let reply_sse = event_stream(&[
        r#"{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"hidden musing"}}"#,
        r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Ship on Thursday."}}"#,
        r#"{"type":"message_stop"}"#,
    ]);
    for (streaming, content_type, reply) in [
        (false, "application/json", reply_json),
        (true, "text/event-stream", reply_sse),
    ] {
        let (host, seen) = serve(200, content_type, reply).await;
        let preset = PRESETS.iter().find(|p| p.wire == "anthropic").unwrap();
        let mut credential = preset.draft();
        credential.api_key = "sk-quartz-5150".into();
        credential.base_url = format!("http://{host}/v1/messages");
        let mut config = credential.to_config();
        config.custom.as_mut().unwrap().streaming = streaming;

        let context = TurnContext {
            system: Some("Act as a release manager.".into()),
            images: vec!["cGl4ZWxzLWZvci10ZXN0".into()],
        };
        let answer = stream_chat(
            &http_client(),
            &config,
            &question_turn("When do we ship?"),
            &context,
            never_cancelled(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(answer, "Ship on Thursday.", "streaming={streaming}");

        let seen = seen.await.unwrap();
        assert_eq!(
            seen.header("authorization").unwrap().to_lowercase(),
            "bearer sk-quartz-5150"
        );
        assert_eq!(seen.header("anthropic-version").unwrap(), "2026-10-21");
        let body = seen.json();
        assert_eq!(body["system"], "Act as a release manager.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(
            body["messages"][0]["content"][1]["source"]["data"],
            "cGl4ZWxzLWZvci10ZXN0"
        );
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["stream"], streaming);
    }
}

#[tokio::test]
async fn custom_chat_provider_follows_its_method_header_and_reply_path() {
    let reply = json!({"data": {"reply": [{"body": "Staging is green."}]}}).to_string();
    let (host, seen) = serve(200, "application/json", reply).await;
    let mut config = ProviderConfig::new("custom-relay-7", "relay-secret-33", "relay/mini-9");
    config.base_url = format!("http://{host}/relay/ask");
    config.custom = Some(CustomProvider {
        method: "PATCH".into(),
        auth: "header".into(),
        auth_header: "x-relay-token".into(),
        response_path: "/data/reply/0/body".into(),
        ..CustomProvider::default()
    });

    let answer = stream_chat(
        &http_client(),
        &config,
        &question_turn("Is staging ready?"),
        &TurnContext::default(),
        never_cancelled(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(answer, "Staging is green.");

    let seen = seen.await.unwrap();
    assert!(seen.request_line().starts_with("PATCH /relay/ask "));
    assert_eq!(seen.header("x-relay-token").unwrap(), "relay-secret-33");
    assert!(seen.header("authorization").is_none());
    let body = seen.json();
    assert_eq!(body["model"], "relay/mini-9");
    assert_eq!(body["stream"], false);
}

fn sample_wav() -> Vec<u8> {
    let samples: Vec<f32> = (0..900).map(|i| (i as f32 * 0.07).sin() * 0.35).collect();
    encode_wav(&samples).unwrap()
}

/// Sends `sample_wav()` through a custom speech provider using `format`; returns what arrived.
async fn upload_as(format: &str) -> Seen {
    let reply = json!({"result": {"transcript": "Numbers look fine."}}).to_string();
    let (host, seen) = serve(200, "application/json", reply).await;
    let mut config = ProviderConfig::new("custom-stt-4", "", "stt-large-2");
    config.base_url = format!("http://{host}/transcribe");
    config.custom = Some(CustomProvider {
        auth: "none".into(),
        request_format: format.into(),
        response_path: "/result/transcript".into(),
        ..CustomProvider::default()
    });
    let text = transcribe(&config, sample_wav(), Some("pt-BR"))
        .await
        .unwrap();
    assert_eq!(text, "Numbers look fine.", "{format}");
    let seen = seen.await.unwrap();
    assert!(seen.header("authorization").is_none(), "{format}");
    seen
}

#[tokio::test]
async fn raw_speech_upload_is_the_wav_itself() {
    let seen = upload_as("raw-wav").await;
    assert!(seen.header("content-type").unwrap().contains("audio/wav"));
    assert_eq!(seen.body, sample_wav());
}

#[tokio::test]
async fn multipart_speech_upload_carries_wav_and_model() {
    let seen = upload_as("multipart-wav").await;
    let kind = seen.header("content-type").unwrap();
    assert!(kind.contains("multipart/form-data; boundary=nixarchy-meetingai-"));
    assert!(find(&seen.body, &sample_wav()).is_some());
    assert!(find(&seen.body, b"stt-large-2").is_some());
}

#[tokio::test]
async fn json_speech_upload_base64_encodes_the_wav() {
    let seen = upload_as("json-wav").await;
    let body = seen.json();
    let audio = base64::engine::general_purpose::STANDARD
        .decode(body["audio"].as_str().unwrap())
        .unwrap();
    assert_eq!(audio, sample_wav());
    assert_eq!(body["model"], "stt-large-2");
    assert_eq!(body["language"], "pt-BR");
}

#[tokio::test]
async fn batch_live_transcription_flushes_the_system_channel_on_stop() {
    let (host, seen) = serve(200, "text/plain", "Budget approved for the third quarter.").await;
    let mut config = ProviderConfig::new("custom-batch-2", "", "batch-model");
    config.base_url = format!("http://{host}/upload");
    config.custom = Some(CustomProvider {
        auth: "none".into(),
        request_format: "raw-wav".into(),
        response_format: "text".into(),
        ..CustomProvider::default()
    });

    let mut live = LiveTranscriber::connect_config(&config).await.unwrap();
    // Half a second of steady sound on the right (system) channel only, interleaved L/R.
    let pcm: Vec<f32> = (0..16_000)
        .map(|i| if i % 2 == 1 { 0.3 } else { 0.0 })
        .collect();
    live.push(&pcm).await.unwrap();
    live.finish();

    let mut lines = Vec::new();
    while let Some(event) = timeout(HANG, live.next_event()).await.unwrap() {
        match event {
            LiveEvent::Failed(message) => panic!("live transcription failed: {message}"),
            LiveEvent::Transcript(line) => lines.push(line),
            _ => {}
        }
    }
    let last = lines.last().expect("no transcript arrived");
    assert_eq!(last.channel, 1);
    assert_eq!(last.text, "Budget approved for the third quarter.");
    assert!(last.speech_final);

    let seen = seen.await.unwrap();
    let mut reader = hound::WavReader::new(std::io::Cursor::new(seen.body)).unwrap();
    assert_eq!(reader.spec().channels, 1);
    let uploaded: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
    assert!(!uploaded.is_empty());
    assert!(uploaded.iter().all(|&s| s > 0));
}

#[test]
fn custom_credentials_are_encrypted_at_rest_and_validated() {
    let dir = temp_dir("vault");
    let vault = Vault::open(&dir).unwrap();
    let mut credential = Credential::chat("custom-local-gw", "gw-secret-key-9d2f");
    credential.base_url = "http://[::1]:8088/chat".into();
    credential.model = "gateway-model".into();
    credential.custom = Some(CustomProvider::default());

    credentials::save(&vault, &credential).unwrap();
    assert_eq!(
        credentials::list(&vault, Kind::Chat).unwrap(),
        vec![credential.clone()]
    );
    assert!(credential.to_config().is_usable());
    let on_disk = std::fs::read(dir.join("vault.bin")).unwrap();
    assert!(find(&on_disk, b"gw-secret-key-9d2f").is_none());

    credential.provider = "ollama".into();
    assert!(credential.validate().is_err());

    credentials::remove(&vault, Kind::Chat, "custom-local-gw").unwrap();
    assert!(credentials::list(&vault, Kind::Chat).unwrap().is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn recap_includes_words_that_arrived_after_the_meeting_ended() {
    let dir = temp_dir("recap");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("meetings.db");
    let late_text = "Zanzibar launch moves to autumn";
    let (host, seen) = serve(
        200,
        "text/plain",
        "## Decisions\nLaunch moves to autumn.\n## Action items\nNone recorded.",
    )
    .await;
    let mut config = ProviderConfig::new("custom-notes-8", "", "recap-model");
    config.base_url = format!("http://{host}/recap");
    config.custom = Some(CustomProvider {
        auth: "none".into(),
        response_format: "text".into(),
        ..CustomProvider::default()
    });

    let id = {
        let db = Database::open(&path).unwrap();
        let c = db.connection();
        let id = meetings::start(c, "Sync", "", "en", None, Default::default()).unwrap();
        meetings::add_line(
            c,
            &id,
            "them",
            Some(0),
            "We reviewed the roadmap.",
            0.0,
            3.0,
        )
        .unwrap();
        meetings::finish(c, &id, 60).unwrap();
        meetings::add_line(c, &id, "them", Some(1), late_text, 3.0, 6.0).unwrap();
        id
    };

    recap::generate(&path, &id, Some(config), "Swahili")
        .await
        .unwrap();

    let request = String::from_utf8_lossy(&seen.await.unwrap().body).into_owned();
    for needle in [late_text, "Swahili", "Decisions", "Action items"] {
        assert!(request.contains(needle), "request lacks {needle:?}");
    }

    let db = Database::open(&path).unwrap();
    let stored = meetings::recap(db.connection(), &id).unwrap().unwrap();
    assert_eq!(stored.status, "ready");
    assert!(stored.content.contains("autumn"));

    meetings::delete(db.connection(), &id).unwrap();
    meetings::finish_recap(db.connection(), &id, Ok("too late")).unwrap();
    assert_eq!(meetings::recap(db.connection(), &id).unwrap(), None);
    drop(db);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn spoken_question_becomes_a_streamed_answer_in_a_finished_meeting() {
    let dir = temp_dir("pipeline");
    std::fs::create_dir_all(&dir).unwrap();
    let vault = Vault::open(dir.join("vault")).unwrap();

    let transcript = "When is the freeze?";
    let stt_reply = json!({"results": {"channels": [
        {"alternatives": [{"transcript": transcript}]}
    ]}})
    .to_string();
    let (stt_host, stt_seen) = serve(200, "application/json", stt_reply).await;
    let part_a = "Freeze starts ";
    let part_b = "Friday – café closes";
    let chat_reply = format!(
        "data: {}\r\n\r\ndata: {}\n\ndata: [DONE]",
        json!({"choices": [{"delta": {"content": part_a}}]}),
        json!({"choices": [{"delta": {"content": part_b}}]}),
    );
    let (chat_host, chat_seen) = serve(200, "text/event-stream", chat_reply).await;

    let mut speech = Credential::speech("deepgram-stt", "dg-key-0042");
    speech.base_url = format!("http://{stt_host}/v1/listen");
    credentials::save(&vault, &speech).unwrap();
    let mut chat = Credential::chat("ollama", "chat-key-0043");
    chat.model = "llama-local:7b".into();
    chat.base_url = format!("http://{chat_host}/v1/chat/completions");
    credentials::save(&vault, &chat).unwrap();

    let stt_config = credentials::load(&vault, Kind::Speech, "deepgram-stt")
        .unwrap()
        .unwrap()
        .to_config();
    assert_eq!(stt_config.model, "nova-3");
    let wav = encode_wav(&[0.1, -0.1, 0.2, -0.2, 0.0, 0.1]).unwrap();
    let heard = transcribe_utterance(
        &stt_config,
        &base64::engine::general_purpose::STANDARD.encode(&wav),
        Some("xx-YY"),
    )
    .await
    .unwrap();
    assert_eq!(heard, transcript);
    let stt_seen = stt_seen.await.unwrap();
    assert_eq!(
        stt_seen.header("authorization").unwrap().to_lowercase(),
        "token dg-key-0042"
    );
    let line = stt_seen.request_line().to_lowercase();
    assert!(line.contains("model=nova-3") && line.contains("language=xx-yy"));
    assert!(find(&stt_seen.body, b"RIFF").is_some());

    let db = Database::open(dir.join("meetings.db")).unwrap();
    let c = db.connection();
    let meeting = meetings::start(c, "Planning", "", "en", None, Default::default()).unwrap();
    meetings::add_line(c, &meeting, "them", None, &heard, 0.0, 2.0).unwrap();

    let mut session = ChatSession::new();
    let history = session.begin(Some(&db), &heard).unwrap();
    let config = chat_config(Some(&vault)).unwrap();
    let context = TurnContext {
        system: Some("Answer from the meeting.".into()),
        images: vec!["c2NyZWVuc2hvdA==".into()],
    };
    let answer = stream_chat(
        &http_client(),
        &config,
        &history,
        &context,
        never_cancelled(),
        |delta| session.push_delta(Some(&db), delta),
    )
    .await
    .unwrap();
    assert_eq!(answer, format!("{part_a}{part_b}"));
    session.finish();
    assert!(!session.is_streaming());

    let stored = conversations::messages(c, session.conversation_id().unwrap()).unwrap();
    assert_eq!(stored.last().unwrap().content, answer);
    meetings::finish(c, &meeting, 120).unwrap();
    assert!(meetings::list(c, 5).unwrap()[0].ended_at.is_some());

    let sent = chat_seen.await.unwrap();
    assert_eq!(
        sent.header("authorization").unwrap().to_lowercase(),
        "bearer chat-key-0043"
    );
    let body = sent.json();
    assert_eq!(body["model"], "llama-local:7b");
    assert_eq!(body["stream"], true);
    assert_eq!(body["messages"][0]["content"], "Answer from the meeting.");
    assert_eq!(body["messages"][1]["content"][0]["text"], heard);
    assert_eq!(
        body["messages"][1]["content"][1]["image_url"]["url"],
        "data:image/png;base64,c2NyZWVuc2hvdA=="
    );

    drop(db);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn provider_failures_reach_the_caller() {
    let cases: [(u16, &str, &str); 3] = [
        (422, r#"{"error":{"message":"bad shape"}}"#, "422"),
        (200, "data: [DONE]\n\n", "without an answer"),
        (
            200,
            "data: {\"error\":\"monthly allowance used up\"}\n\n",
            "monthly allowance used up",
        ),
    ];
    for (status, body, expected) in cases {
        let (host, _seen) = serve(status, "text/event-stream", body).await;
        let (_keep, cancel) = watch::channel(false);
        let error = stream_chat(
            &http_client(),
            &ollama(&host, "k", "tiny"),
            &question_turn("status please"),
            &TurnContext::default(),
            Arc::new(cancel),
            |_| {},
        )
        .await
        .expect_err("the call should fail");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[tokio::test]
async fn cancelling_a_silent_provider_returns_at_once() {
    // Bound but never accepted: the request just sits there.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = ollama(&listener.local_addr().unwrap().to_string(), "k", "tiny");
    let (stop, cancel) = watch::channel(false);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(80)).await;
        let _ = stop.send(true);
    });
    let client = http_client();
    let context = TurnContext::default();
    let outcome = timeout(
        Duration::from_millis(800),
        stream_chat(&client, &config, &[], &context, Arc::new(cancel), |_| {}),
    )
    .await
    .expect("cancel did not release the call");
    assert_eq!(outcome.unwrap(), "");
    drop(listener);
}

#[tokio::test]
async fn deltas_are_delivered_while_the_provider_is_still_talking() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = ollama(&listener.local_addr().unwrap().to_string(), "k", "tiny");
    let part_a = "Try this:\n```ru";
    let part_b = "st\nprintln!(\"señor\");\n```";
    let (go_tx, mut go_rx) = mpsc::unbounded_channel::<()>();
    let server_done = Arc::new(AtomicBool::new(false));
    let flag = server_done.clone();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        let chunk = |text: &str| {
            format!(
                "data: {}\n\n",
                json!({"choices": [{"delta": {"content": text}}]})
            )
        };
        socket
            .write_all(head(200, "text/event-stream").as_bytes())
            .await
            .unwrap();
        socket.write_all(chunk(part_a).as_bytes()).await.unwrap();
        socket.flush().await.unwrap();
        timeout(HANG, go_rx.recv())
            .await
            .expect("the first delta never reached the caller");
        flag.store(true, Ordering::SeqCst);
        socket.write_all(chunk(part_b).as_bytes()).await.unwrap();
        socket.write_all(b"data: [DONE]\n\n").await.unwrap();
        let _ = socket.shutdown().await;
    });

    let mut deltas: Vec<String> = Vec::new();
    let answer = stream_chat(
        &http_client(),
        &config,
        &question_turn("print a snippet"),
        &TurnContext::default(),
        never_cancelled(),
        |delta| {
            if deltas.is_empty() {
                assert!(
                    !server_done.load(Ordering::SeqCst),
                    "server already finished"
                );
                go_tx.send(()).unwrap();
            }
            deltas.push(delta.to_owned());
        },
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(deltas.len(), 2);
    assert_eq!(answer, format!("{part_a}{part_b}"));
}

#[tokio::test]
async fn a_live_model_id_is_sent_untouched() {
    let reply = event_stream(&[r#"{"choices":[{"delta":{"content":"ok"}}]}"#, "[DONE]"]);
    let (host, seen) = serve(200, "text/event-stream", reply).await;
    let model = "acme/zeta-7b:q4";
    stream_chat(
        &http_client(),
        &ollama(&host, "k", model),
        &question_turn("ping"),
        &TurnContext::default(),
        never_cancelled(),
        |_| {},
    )
    .await
    .unwrap();
    let body = seen.await.unwrap().json();
    assert_eq!(body["model"], model);
    assert!(body.get("reasoning_effort").is_none());
}
