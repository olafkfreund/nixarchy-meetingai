//! Slow, stalled and broken providers: requests overlap, Stop cuts waits short, errors stay safe.
use nixarchy_meetingai::ai::engine::{
    ChatError, TurnContext, http_client, stream_chat, stream_chat_bounded,
};
use nixarchy_meetingai::ai::providers::ProviderConfig;
use nixarchy_meetingai::core::domain::Message;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};

const WAIT: Duration = Duration::from_secs(5);
const DELTA: &str = r#"data: {"choices":[{"delta":{"content":"pong"}}]}"#;

fn headers_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| at + 4)
}

/// Header fields of a raw request head, names lowercased.
fn header_map(head: &[u8]) -> HashMap<String, String> {
    String::from_utf8_lossy(head)
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_owned()))
        .collect()
}

/// Reads one request (headers plus the declared body) off `socket`; a stall fails the test.
async fn read_request(socket: &mut TcpStream) -> Vec<u8> {
    tokio::time::timeout(WAIT, async {
        let mut raw = Vec::new();
        let mut buf = [0u8; 2048];
        loop {
            if let Some(body_at) = headers_end(&raw) {
                let declared = header_map(&raw[..body_at])
                    .get("content-length")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                if raw.len() - body_at >= declared {
                    return raw;
                }
            }
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0, "client left before sending a whole request");
            raw.extend_from_slice(&buf[..n]);
        }
    })
    .await
    .expect("request never completed")
}

/// Binds a listener, serves its first connection with `handler`, returns the provider config.
async fn provider_with<F, Fut>(handler: F) -> ProviderConfig
where
    F: FnOnce(TcpStream) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = ProviderConfig::new("ollama", "", "stub");
    config.base_url = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        handler(socket).await;
    });
    config
}

/// One whole canned response; the server side may be cut off early, so write errors are ignored.
async fn respond(socket: &mut TcpStream, status: &str, kind: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = socket.write_all(head.as_bytes()).await;
    let _ = socket.write_all(body).await;
    let _ = socket.shutdown().await;
}

fn ask(text: &str) -> Vec<Message> {
    vec![Message::user(text)]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn many_requests_are_in_flight_together_on_few_workers() {
    const CALLS: usize = 9;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let (gate_tx, gate_rx) = watch::channel(false);
    let (arrived_tx, mut arrived_rx) = mpsc::unbounded_channel::<()>();
    tokio::spawn(async move {
        for _ in 0..CALLS {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (mut gate, arrived) = (gate_rx.clone(), arrived_tx.clone());
            tokio::spawn(async move {
                read_request(&mut socket).await;
                arrived.send(()).unwrap();
                gate.wait_for(|open| *open).await.unwrap();
                let body = format!("{DELTA}\n\ndata: [DONE]\n\n");
                respond(&mut socket, "200 OK", "text/event-stream", body.as_bytes()).await;
            });
        }
    });

    let (_keep, cancel) = watch::channel(false);
    let cancel = Arc::new(cancel);
    let calls: Vec<_> = (0..CALLS)
        .map(|_| {
            let (url, cancel) = (url.clone(), cancel.clone());
            tokio::spawn(async move {
                let mut config = ProviderConfig::new("ollama", "", "stub");
                config.base_url = url;
                stream_chat(
                    &http_client(),
                    &config,
                    &ask("hi"),
                    &TurnContext::default(),
                    cancel,
                    |_| {},
                )
                .await
            })
        })
        .collect();

    tokio::time::timeout(WAIT, async {
        for _ in 0..CALLS {
            arrived_rx.recv().await.unwrap();
        }
    })
    .await
    .expect("not every request reached the server while all were held");
    gate_tx.send(true).unwrap();
    for call in calls {
        assert_eq!(call.await.unwrap().unwrap(), "pong");
    }
}

#[tokio::test]
async fn stop_ends_a_call_stuck_on_an_error_body() {
    let (sent_tx, sent_rx) = oneshot::channel();
    let config = provider_with(|mut socket| async move {
        let head = "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\nContent-Length: 4000\r\n\r\nupstream";
        socket.write_all(head.as_bytes()).await.unwrap();
        sent_tx.send(()).unwrap();
        std::future::pending::<()>().await;
    })
    .await;
    let (stop, cancel) = watch::channel(false);
    let call = tokio::spawn(async move {
        stream_chat(
            &http_client(),
            &config,
            &ask("hi"),
            &TurnContext::default(),
            Arc::new(cancel),
            |_| {},
        )
        .await
    });
    sent_rx.await.unwrap();
    stop.send(true).unwrap();
    let outcome = tokio::time::timeout(Duration::from_millis(500), call)
        .await
        .expect("Stop did not end the call")
        .unwrap();
    assert_eq!(outcome.unwrap(), "");
}

#[tokio::test]
async fn stop_ends_a_call_blocked_on_a_slow_consumer() {
    let config = provider_with(|mut socket| async move {
        let head =
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
        let event = r#"data: {"choices":[{"delta":{"content":"first words"}}]}"#;
        socket
            .write_all(format!("{head}{event}\n\n").as_bytes())
            .await
            .unwrap();
        std::future::pending::<()>().await;
    })
    .await;
    let (stop, cancel) = watch::channel(false);
    let (seen_tx, mut seen_rx) = mpsc::unbounded_channel::<String>();
    let call = tokio::spawn(async move {
        stream_chat_bounded(
            &http_client(),
            &config,
            &ask("hi"),
            &TurnContext::default(),
            Arc::new(cancel),
            move |delta| {
                let seen = seen_tx.clone();
                async move {
                    seen.send(delta).unwrap();
                    std::future::pending::<()>().await;
                }
            },
        )
        .await
    });
    assert_eq!(seen_rx.recv().await.unwrap(), "first words");
    stop.send(true).unwrap();
    let outcome = tokio::time::timeout(Duration::from_millis(500), call)
        .await
        .expect("Stop did not interrupt the blocked consumer")
        .unwrap();
    assert_eq!(outcome.unwrap(), "first words");
}

#[tokio::test]
async fn failures_never_leak_the_key_and_a_cut_stream_is_not_success() {
    const KEY: &str = "zq-secret-7731";
    let oversized = vec![b'x'; 256 * 1024 + 1];
    let cases: Vec<(&str, String, Vec<u8>, &str)> = vec![
        (
            "401 Unauthorized",
            "application/json".into(),
            format!(r#"{{"error":"rejected {KEY}"}}"#).into_bytes(),
            "[redacted]",
        ),
        (
            "200 OK",
            "text/event-stream".into(),
            format!("data: {{\"error\":\"bad credential {KEY}\"}}\n\n").into_bytes(),
            "[redacted]",
        ),
        (
            "200 OK",
            "text/event-stream".into(),
            format!("{DELTA}\n\n").into_bytes(),
            "before confirming completion",
        ),
        (
            "200 OK",
            "text/event-stream".into(),
            oversized,
            "size limit",
        ),
    ];
    for (status, kind, body, expected) in cases {
        let mut config = provider_with(move |mut socket| async move {
            respond(&mut socket, status, &kind, &body).await;
        })
        .await;
        config.api_key = KEY.into();
        let (_keep, cancel) = watch::channel(false);
        let message = stream_chat(
            &http_client(),
            &config,
            &ask("hi"),
            &TurnContext::default(),
            Arc::new(cancel),
            |_| {},
        )
        .await
        .expect_err("this reply must be an error")
        .to_string();
        assert!(message.contains(expected), "{message}");
        assert!(!message.contains(KEY), "{message}");
    }
}

#[tokio::test]
async fn a_body_that_goes_quiet_times_out() {
    let config = provider_with(|mut socket| async move {
        let head =
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 90000\r\n\r\n";
        socket.write_all(head.as_bytes()).await.unwrap();
        std::future::pending::<()>().await;
    })
    .await;
    // Building a client directly needs the TLS provider the app's own client installs.
    let _ = http_client();
    let client = reqwest::Client::builder()
        .read_timeout(Duration::from_millis(250))
        .build()
        .unwrap();
    let (_keep, cancel) = watch::channel(false);
    let started = Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(1),
        stream_chat(
            &client,
            &config,
            &ask("hi"),
            &TurnContext::default(),
            Arc::new(cancel),
            |_| {},
        ),
    )
    .await
    .expect("the idle body never timed out");
    match outcome {
        Err(ChatError::Request(error)) => assert!(error.is_timeout(), "{error}"),
        other => panic!("expected a request timeout, got {other:?}"),
    }
    assert!(started.elapsed() < Duration::from_secs(1));
}

/// Serves one multipart request per entry: `None` drops the connection, `Some(text)` answers.
async fn speech_mock(
    replies: Vec<Option<&'static str>>,
) -> (ProviderConfig, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = ProviderConfig::new("openai-compatible-stt", "", "m");
    config.base_url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        for reply in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_request(&mut socket).await;
            if let Some(text) = reply {
                let body = format!("{{\"text\":\"{text}\"}}");
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(head.as_bytes()).await.unwrap();
                socket.write_all(body.as_bytes()).await.unwrap();
            }
        }
    });
    (config, server)
}

async fn run_speech(
    config: &ProviderConfig,
) -> Vec<nixarchy_meetingai::speech::transcription::LiveEvent> {
    use nixarchy_meetingai::speech::transcription::LiveTranscriber;
    let mut live = LiveTranscriber::connect_config(config).await.unwrap();
    // 0.5 s of speech on the microphone channel only (interleaved L/R), then flush the chunk.
    let pcm: Vec<f32> = (0..16_000)
        .map(|i| if i % 2 == 0 { 0.25 } else { 0.0 })
        .collect();
    live.push(&pcm).await.unwrap();
    live.finish();
    let mut events = Vec::new();
    while let Some(event) = live.next_event().await {
        events.push(event);
    }
    events
}

#[tokio::test]
async fn openai_compatible_speech_reports_the_lost_range_and_carries_on() {
    use nixarchy_meetingai::speech::transcription::LiveEvent;
    let (config, server) = speech_mock(vec![None]).await;
    let events = run_speech(&config).await;
    assert!(
        !events.iter().any(|e| matches!(e, LiveEvent::Failed(_))),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, LiveEvent::Notice(m) if m.contains("lost speech for"))),
        "{events:?}"
    );
    server.abort();
}

#[tokio::test]
async fn empty_openai_compatible_text_is_skipped_without_failing() {
    use nixarchy_meetingai::speech::transcription::LiveEvent;
    let (config, server) = speech_mock(vec![Some("")]).await;
    let events = run_speech(&config).await;
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, LiveEvent::Failed(_) | LiveEvent::Transcript(_))),
        "{events:?}"
    );
    server.abort();
}
