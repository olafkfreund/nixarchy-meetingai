//! Protocol-level end-to-end checks without external credentials or devices.
use base64::{Engine, engine::general_purpose::STANDARD};
use lennette::{
    ai::{
        engine::{self, TurnContext},
        models::MERGE_MODEL,
        providers::ProviderConfig,
        session::{self, ChatSession},
    },
    audio::encode_wav,
    core::{
        credentials::{self, Credential, Kind},
        domain::Message,
        vault::Vault,
    },
    db::{Database, conversations, meetings},
    speech::listen::transcribe_utterance,
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::Duration,
};

fn server(
    status: u16,
    content_type: &str,
    body: String,
) -> (String, std::thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/test", listener.local_addr().unwrap());
    let content_type = content_type.to_owned();
    let thread = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            let n = socket.read(&mut chunk).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&chunk[..n]);
            if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        // Fragment SSE payloads, including inside UTF-8 and JSON tokens.
        for bytes in body.as_bytes().chunks(7) {
            socket.write_all(bytes).unwrap();
        }
        request
    });
    (url, thread)
}

#[tokio::test]
async fn speech_to_streamed_answer_and_finished_meeting_for_both_ai_services() {
    for provider in ["gemini", "merge-gateway"] {
        let dir = std::env::temp_dir().join(format!("lennette-flow-{}", uuid::Uuid::new_v4()));
        let vault = Vault::open(&dir).unwrap();
        let mut speech = Credential::speech("deepgram-stt", "test-speech-key");
        // Keep the request observer for a separate server so we can assert auth and WAV.
        let (stt_url, stt_request) = server(200, "application/json", r#"{"results":{"channels":[{"alternatives":[{"transcript":"How does Rust prevent data races?"}]}]}}"#.into());
        speech.base_url = stt_url;
        let (chat_url, chat_request) = server(200, "text/event-stream", "data: {\"choices\":[{\"delta\":{\"content\":\"Ownership \"}}]}\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"prevents races. ✓\"}}]}\n\ndata: [DONE]".into());
        let mut chat = Credential::chat(provider, "test-chat-key");
        chat.model = if provider == "merge-gateway" {
            MERGE_MODEL.into()
        } else {
            "gemini-test-model".into()
        };
        chat.base_url = chat_url;
        credentials::save(&vault, &speech).unwrap();
        credentials::save(&vault, &chat).unwrap();
        let speech = credentials::load(&vault, Kind::Speech, "deepgram-stt")
            .unwrap()
            .unwrap()
            .to_config();
        assert_eq!(speech.model, "nova-3");
        let wav = encode_wav(&vec![0.1; 32000]).unwrap();
        let question = transcribe_utterance(&speech, &STANDARD.encode(&wav), Some("en-US"))
            .await
            .unwrap();
        let request = stt_request.join().unwrap();
        let headers = String::from_utf8_lossy(&request).to_lowercase();
        assert!(headers.contains("authorization: token test-speech-key"));
        assert!(headers.contains("model=nova-3"));
        assert!(headers.contains("language=en-us"));
        assert!(request.windows(4).any(|b| b == b"RIFF"));
        let database = Database::open(dir.join("test.db")).unwrap();
        let meeting =
            meetings::start(database.connection(), "Test meeting", "meeting", "en").unwrap();
        meetings::add_line(
            database.connection(),
            &meeting,
            "them",
            None,
            &question,
            0.0,
            2.0,
        )
        .unwrap();
        let mut session = ChatSession::new();
        let history = session.begin(Some(&database), &question).unwrap();
        // Dropping an unused cancel sender must not cancel the request.
        let cancel = Arc::new(tokio::sync::watch::channel(false).1);
        let answer = engine::stream_chat(
            &engine::http_client(),
            &session::chat_config(Some(&vault)).unwrap(),
            &history,
            &TurnContext {
                system: Some("Answer briefly".into()),
                images: vec!["QUJD".into()],
            },
            cancel,
            |delta| session.push_delta(Some(&database), delta),
        )
        .await
        .unwrap();
        session.finish();
        meetings::finish(database.connection(), &meeting, 2).unwrap();
        assert_eq!(answer, "Ownership prevents races. ✓");
        assert!(!session.is_streaming());
        let saved =
            conversations::messages(database.connection(), session.conversation_id().unwrap())
                .unwrap();
        assert_eq!(saved.last().unwrap().content, answer);
        assert!(
            meetings::list(database.connection(), 1).unwrap()[0]
                .ended_at
                .is_some()
        );
        let raw = chat_request.join().unwrap();
        let headers = String::from_utf8_lossy(&raw).to_lowercase();
        assert!(headers.contains("authorization: bearer test-chat-key"));
        let end = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let body: serde_json::Value = serde_json::from_slice(&raw[end..]).unwrap();
        assert_eq!(body["model"], chat.model);
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][1]["content"][0]["text"], question);
        assert_eq!(
            body["messages"][1]["content"][1]["image_url"]["url"],
            "data:image/png;base64,QUJD"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[tokio::test]
async fn auth_errors_empty_streams_and_trailing_errors_are_visible() {
    for (status, payload, expected) in [
        (401, "{\"error\":\"invalid key\"}", "401"),
        (200, "data: [DONE]\n", "without an answer"),
        (
            200,
            "data: {\"error\":\"quota exceeded\"}",
            "quota exceeded",
        ),
    ] {
        let (url, request) = server(status, "text/event-stream", payload.into());
        let mut config = ProviderConfig::new("merge-gateway", "test-key", MERGE_MODEL);
        config.base_url = url;
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let error = engine::stream_chat(
            &engine::http_client(),
            &config,
            &[Message::user("hello")],
            &TurnContext::default(),
            Arc::new(rx),
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
        request.join().unwrap();
    }
}

#[tokio::test]
async fn cancelling_a_silent_provider_does_not_wait_for_another_delta() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = ProviderConfig::new("gemini", "test-key", "m");
    config.base_url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::watch::channel(false);
    let cancel = async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(true).unwrap();
    };
    let client = engine::http_client();
    let context = TurnContext::default();
    let request = engine::stream_chat(&client, &config, &[], &context, Arc::new(rx), |_| {});
    // Keep the client and context alive for the in-flight future.
    let (_, answer) = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(cancel, request)
    })
    .await
    .unwrap();
    assert_eq!(answer.unwrap(), "");
}

#[tokio::test]
async fn markdown_code_deltas_arrive_before_the_provider_finishes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = ProviderConfig::new("merge-gateway", "test-key", MERGE_MODEL);
    config.base_url = format!("http://{}", listener.local_addr().unwrap());
    let (ack_tx, ack_rx) = std::sync::mpsc::channel();
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_finished = finished.clone();
    let first = "## Streaming\n\n```rust\nfn main() {\n";
    let second = "    println!(\"héllo 世界\");\n}\n```\n\n- **Ready**";
    let packet = |text: &str| {
        format!(
            "data: {}\n\n",
            serde_json::json!({"choices":[{"delta":{"content":text}}]})
        )
    };
    let payload1 = packet(first);
    let payload2 = packet(second) + "data: [DONE]\n\n";
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0; 8192];
        assert!(socket.read(&mut request).unwrap() > 0);
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload1.len()+payload2.len()).unwrap();
        socket.write_all(payload1.as_bytes()).unwrap();
        socket.flush().unwrap();
        // The server cannot complete until the consumer has rendered a partial block.
        ack_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        server_finished.store(true, std::sync::atomic::Ordering::SeqCst);
        socket.write_all(payload2.as_bytes()).unwrap();
    });
    let mut partial = String::new();
    let mut snapshots = 0;
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let answer = engine::stream_chat(
        &engine::http_client(),
        &config,
        &[Message::user("show code")],
        &TurnContext::default(),
        Arc::new(rx),
        |delta| {
            partial.push_str(delta);
            snapshots += 1;
            let blocks = lennette::ui::markdown::parse(&partial);
            let code = blocks
                .iter()
                .find_map(|block| match block {
                    lennette::ui::markdown::Block::Code { language, text } => {
                        Some((language, text))
                    }
                    _ => None,
                })
                .unwrap();
            let tokens = lennette::ui::widgets::syntax::highlight(code.0.as_deref(), code.1, false);
            assert_eq!(tokens.last().unwrap().range.end, code.1.len());
            if snapshots == 1 {
                assert!(!finished.load(std::sync::atomic::Ordering::SeqCst));
                ack_tx.send(()).unwrap();
            }
        },
    )
    .await
    .unwrap();
    server.join().unwrap();
    assert_eq!(snapshots, 2);
    assert_eq!(answer, format!("{first}{second}"));
}
