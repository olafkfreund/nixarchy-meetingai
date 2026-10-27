//! Protocol-level end-to-end checks without external credentials or devices.
use base64::{Engine, engine::general_purpose::STANDARD};
use nixarchy_meetingai::{
    ai::{
        engine::{self, TurnContext},
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

#[tokio::test]
async fn anthropic_preset_sends_native_messages_and_streams_only_visible_text() {
    use nixarchy_meetingai::ai::catalog::presets::PRESETS;
    for streaming in [false, true] {
        let response = if streaming {
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"private reasoning\"}}\n\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Claude answer\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n"
        } else {
            r#"{"content":[{"type":"thinking","thinking":"private reasoning"},{"type":"text","text":"Claude answer"}]}"#
        };
        let (url, request) = server(
            200,
            if streaming {
                "text/event-stream"
            } else {
                "application/json"
            },
            response.into(),
        );
        let mut config = PRESETS
            .iter()
            .find(|preset| preset.wire == "anthropic")
            .unwrap()
            .draft()
            .to_config();
        config.api_key = "local-test-key".into();
        config.base_url = url;
        config.custom.as_mut().unwrap().streaming = streaming;
        let answer = engine::stream_chat(
            &engine::http_client(),
            &config,
            &[Message::user("Look")],
            &TurnContext {
                system: Some("Be concise".into()),
                images: vec!["QUJD".into()],
            },
            Arc::new(tokio::sync::watch::channel(false).1),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(answer, "Claude answer");
        let bytes = request.join().unwrap();
        let raw = String::from_utf8_lossy(&bytes);
        let headers = raw.split_once("\r\n\r\n").unwrap().0.to_lowercase();
        assert!(headers.contains("authorization: bearer local-test-key"));
        assert!(headers.contains("anthropic-version: 2026-10-21"));
        let body: serde_json::Value =
            serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["system"], "Be concise");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"][1]["source"]["data"], "QUJD");
        assert_eq!(body["max_tokens"], 4096);
    }
}

#[tokio::test]
async fn custom_chat_uses_its_method_auth_model_and_response_pointer() {
    use nixarchy_meetingai::ai::catalog::custom::CustomProvider;
    let (url, request) = server(
        200,
        "application/json",
        r#"{"answer":{"text":"Custom answer"}}"#.into(),
    );
    let mut config = ProviderConfig::new("custom-chat-test", "local-test-key", "my-model");
    config.base_url = url;
    config.custom = Some(CustomProvider {
        method: "PUT".into(),
        auth: "header".into(),
        auth_header: "x-api-key".into(),
        response_path: "/answer/text".into(),
        ..Default::default()
    });
    let answer = engine::stream_chat(
        &engine::http_client(),
        &config,
        &[Message::user("Hello")],
        &TurnContext::default(),
        Arc::new(tokio::sync::watch::channel(false).1),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(answer, "Custom answer");
    let bytes = request.join().unwrap();
    let raw = String::from_utf8_lossy(&bytes);
    assert!(raw.starts_with("PUT /test"));
    assert!(raw.to_lowercase().contains("x-api-key: local-test-key"));
    assert!(!raw.to_lowercase().contains("authorization:"));
    let body: serde_json::Value =
        serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["model"], "my-model");
    assert_eq!(body["stream"], false);
}

#[tokio::test]
async fn custom_speech_uploads_each_format_and_extracts_configured_text() {
    use nixarchy_meetingai::{ai::catalog::custom::CustomProvider, speech::transcription};
    for format in ["raw-wav", "multipart-wav", "json-wav"] {
        let (url, request) = server(
            200,
            "application/json",
            r#"{"text":"Words from custom speech"}"#.into(),
        );
        let mut config = ProviderConfig::new("custom-speech-test", "", "my-stt-model");
        config.base_url = url;
        config.custom = Some(CustomProvider {
            auth: "none".into(),
            request_format: format.into(),
            response_path: "/text".into(),
            ..Default::default()
        });
        let wav = encode_wav(&[0.25; 1600]).unwrap();
        assert_eq!(
            transcription::transcribe(&config, wav.clone(), Some("de"))
                .await
                .unwrap(),
            "Words from custom speech"
        );
        let bytes = request.join().unwrap();
        let offset = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let headers = String::from_utf8_lossy(&bytes[..offset]).to_lowercase();
        assert!(!headers.contains("authorization:"));
        let body = &bytes[offset..];
        match format {
            "raw-wav" => {
                assert_eq!(body, wav);
                assert!(headers.contains("audio/wav"));
            }
            "multipart-wav" => {
                assert!(headers.contains("multipart/form-data; boundary=nixarchy-meetingai-"));
                assert!(body.windows(wav.len()).any(|part| part == wav));
                assert!(String::from_utf8_lossy(body).contains("my-stt-model"));
            }
            _ => {
                let body: serde_json::Value = serde_json::from_slice(body).unwrap();
                assert_eq!(
                    STANDARD.decode(body["audio"].as_str().unwrap()).unwrap(),
                    wav
                );
                assert_eq!(body["model"], "my-stt-model");
                assert_eq!(body["language"], "de");
            }
        }
    }
}

#[tokio::test]
async fn batch_listen_flushes_the_remote_channel_on_stop() {
    use nixarchy_meetingai::{
        ai::catalog::custom::CustomProvider,
        speech::transcription::{LiveEvent, LiveTranscriber},
    };
    let (url, request) = server(200, "text/plain", "Remote question?".into());
    let mut config = ProviderConfig::new("custom-batch-listen", "", "speech-model");
    config.base_url = url;
    config.custom = Some(CustomProvider {
        auth: "none".into(),
        request_format: "raw-wav".into(),
        response_format: "text".into(),
        ..Default::default()
    });
    let mut live = LiveTranscriber::connect_config(&config).await.unwrap();
    live.push(&[0.0, 0.25].repeat(3200)).await.unwrap();
    live.finish();
    let mut transcript = None;
    while let Some(event) = tokio::time::timeout(Duration::from_secs(5), live.next_event())
        .await
        .unwrap()
    {
        match event {
            LiveEvent::Transcript(line) => transcript = Some(line),
            LiveEvent::Failed(error) => panic!("{error}"),
            _ => {}
        }
    }
    let line = transcript.unwrap();
    assert_eq!(line.channel, 1);
    assert_eq!(line.text, "Remote question?");
    assert!(line.speech_final);
    let bytes = request.join().unwrap();
    let offset = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let mut wav = hound::WavReader::new(std::io::Cursor::new(&bytes[offset..])).unwrap();
    assert_eq!(wav.spec().channels, 1);
    assert!(wav.samples::<i16>().all(|sample| sample.unwrap() > 0));
}

#[test]
fn custom_providers_are_encrypted_managed_and_do_not_reactivate_retired_ids() {
    use nixarchy_meetingai::ai::catalog::custom::CustomProvider;
    let directory = std::env::temp_dir().join(format!(
        "nixarchy-meetingai-custom-{}",
        uuid::Uuid::new_v4()
    ));
    let vault = Vault::open(&directory).unwrap();
    let mut credential = Credential::chat("custom-my-provider", "custom-secret-key");
    credential.base_url = "http://localhost:12345/chat/completions".into();
    credential.model = "local-model".into();
    credential.custom = Some(CustomProvider::default());
    credentials::save(&vault, &credential).unwrap();
    assert_eq!(
        credentials::list(&vault, Kind::Chat).unwrap(),
        vec![credential.clone()]
    );
    assert!(credential.to_config().is_usable());
    let bytes = std::fs::read(directory.join("vault.bin")).unwrap();
    assert!(
        !bytes
            .windows(17)
            .any(|window| window == b"custom-secret-key")
    );
    credential.provider = "ollama".into();
    assert!(credential.validate().is_err());
    credentials::remove(&vault, Kind::Chat, "custom-my-provider").unwrap();
    assert!(credentials::list(&vault, Kind::Chat).unwrap().is_empty());
}

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
async fn meeting_recap_uses_final_persisted_words_and_survives_reopen() {
    use nixarchy_meetingai::ai::catalog::custom::CustomProvider;
    let directory =
        std::env::temp_dir().join(format!("nixarchy-meetingai-recap-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("test.db");
    let db = Database::open(&path).unwrap();
    let id = meetings::start(db.connection(), "Review", "", "en").unwrap();
    meetings::add_line(
        db.connection(),
        &id,
        "me",
        None,
        "Let's decide tomorrow.",
        0.0,
        1.0,
    )
    .unwrap();
    meetings::finish(db.connection(), &id, 2).unwrap();
    meetings::add_line(
        db.connection(),
        &id,
        "them",
        None,
        "Final words: Sam owns the task by Friday.",
        1.0,
        2.0,
    )
    .unwrap();
    let (url, request) = server(
        200,
        "text/plain",
        "## Summary\nReviewed the task.\n## Decisions\nProceed.\n## Action items\nSam — Friday."
            .into(),
    );
    let mut config = ProviderConfig::new("custom-recap", "", "model");
    config.base_url = url;
    config.custom = Some(CustomProvider {
        auth: "none".into(),
        response_format: "text".into(),
        ..Default::default()
    });
    nixarchy_meetingai::ai::recap::generate(&path, &id, Some(config), "French")
        .await
        .unwrap();
    let bytes = request.join().unwrap();
    let raw = String::from_utf8_lossy(&bytes);
    assert!(raw.contains("Final words: Sam owns the task by Friday."));
    assert!(raw.contains("French"));
    assert!(raw.contains("Decisions"));
    assert!(raw.contains("Action items"));
    let db = Database::open(&path).unwrap();
    let recap = meetings::recap(db.connection(), &id).unwrap().unwrap();
    assert_eq!(recap.status, "ready");
    assert!(recap.content.contains("Sam"));
    meetings::delete(db.connection(), &id).unwrap();
    meetings::finish_recap(db.connection(), &id, Ok("late reply")).unwrap();
    assert!(meetings::recap(db.connection(), &id).unwrap().is_none());
}

#[tokio::test]
async fn speech_to_streamed_answer_and_finished_meeting_for_both_ai_services() {
    for provider in ["gemini", "ollama"] {
        let dir =
            std::env::temp_dir().join(format!("nixarchy-meetingai-flow-{}", uuid::Uuid::new_v4()));
        let vault = Vault::open(&dir).unwrap();
        let mut speech = Credential::speech("deepgram-stt", "test-speech-key");
        // Keep the request observer for a separate server so we can assert auth and WAV.
        let (stt_url, stt_request) = server(200, "application/json", r#"{"results":{"channels":[{"alternatives":[{"transcript":"How does Rust prevent data races?"}]}]}}"#.into());
        speech.base_url = stt_url;
        let (chat_url, chat_request) = server(200, "text/event-stream", "data: {\"choices\":[{\"delta\":{\"content\":\"Ownership \"}}]}\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"prevents races. ✓\"}}]}\n\ndata: [DONE]".into());
        let mut chat = Credential::chat(provider, "test-chat-key");
        chat.model = "chat-test-model".into();
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
        // Windows cannot remove SQLite's database/WAL files until the connection
        // closes. Linux permits unlinking open files and hid this lifecycle bug.
        drop(database);
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
        let mut config = ProviderConfig::new("gemini", "test-key", "chat-test-model");
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
    let mut config = ProviderConfig::new("gemini", "test-key", "chat-test-model");
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

#[tokio::test]
async fn selecting_a_live_model_sends_that_model_without_pinned_model_options() {
    let (url, request) = server(200, "text/event-stream", "data: {\"choices\":[{\"delta\":{\"content\":\"Selected model works\"}}]}\n\ndata: [DONE]\n\n".into());
    let mut config = ProviderConfig::new("gemini", "test-key", "vendor/live-model");
    config.base_url = url;
    let answer = engine::stream_chat(
        &engine::http_client(),
        &config,
        &[Message::user("Please help")],
        &TurnContext::default(),
        Arc::new(tokio::sync::watch::channel(false).1),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(answer, "Selected model works");
    let raw = request.join().unwrap();
    let offset = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let body: serde_json::Value = serde_json::from_slice(&raw[offset..]).unwrap();
    assert_eq!(body["model"], "vendor/live-model");
    assert!(body.get("reasoning_effort").is_none());
}
