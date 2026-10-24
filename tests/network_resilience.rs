//! Deterministic delayed-provider tests; no external keys or paid requests.
use lennette::{
    ai::{
        engine::{self, TurnContext},
        providers::ProviderConfig,
    },
    core::domain::Message,
};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, watch},
};

async fn read_request(socket: &mut TcpStream) {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0u8; 4096];
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
            let length: usize = headers
                .lines()
                .find_map(|s| s.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return;
            }
        }
    }
}

async fn response(status: u16, body: String) -> (ProviderConfig, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = ProviderConfig::new("gemini", "private-test-key", "model");
    config.base_url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        let headers = format!(
            "HTTP/1.1 {status} Test\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(headers.as_bytes()).await.unwrap();
        let _ = socket.write_all(body.as_bytes()).await;
    });
    (config, server)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_workers_allow_twelve_requests_to_wait_concurrently() {
    const REQUESTS: usize = 12;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (arrivals, mut received) = mpsc::channel(REQUESTS);
    let (release, gate) = watch::channel(false);
    let server = tokio::spawn(async move {
        let mut tasks = Vec::new();
        for _ in 0..REQUESTS {
            let (mut socket, _) = listener.accept().await.unwrap();
            let arrivals = arrivals.clone();
            let mut gate = gate.clone();
            tasks.push(tokio::spawn(async move {
                read_request(&mut socket).await;
                arrivals.send(()).await.unwrap();
                while !*gate.borrow() { gate.changed().await.unwrap(); }
                let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Answer\"}}]}\n\ndata: [DONE]\n";
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
    });
    let tasks: Vec<_> = (0..REQUESTS)
        .map(|_| {
            let mut config = ProviderConfig::new("gemini", "test-key", "model");
            config.base_url = url.clone();
            tokio::spawn(async move {
                engine::stream_chat(
                    &engine::http_client(),
                    &config,
                    &[Message::user("Hi")],
                    &TurnContext::default(),
                    Arc::new(watch::channel(false).1),
                    |_| {},
                )
                .await
            })
        })
        .collect();
    tokio::time::timeout(Duration::from_secs(3), async {
        for _ in 0..REQUESTS {
            received.recv().await.unwrap();
        }
    })
    .await
    .expect("All twelve requests must reach the provider before any response is released");
    release.send(true).unwrap();
    for task in tasks {
        assert_eq!(task.await.unwrap().unwrap(), "Answer");
    }
    server.await.unwrap();
}

#[tokio::test]
async fn stop_cancels_a_stalled_error_body() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = ProviderConfig::new("gemini", "test-key", "model");
    config.base_url = format!("http://{}", listener.local_addr().unwrap());
    let (headers, ready) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        socket
            .write_all(b"HTTP/1.1 429 Slow\r\nContent-Length: 1000\r\n\r\n")
            .await
            .unwrap();
        headers.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    let (cancel, receiver) = watch::channel(false);
    let request = tokio::spawn(async move {
        engine::stream_chat(
            &engine::http_client(),
            &config,
            &[],
            &TurnContext::default(),
            Arc::new(receiver),
            |_| {},
        )
        .await
    });
    ready.await.unwrap();
    let start = std::time::Instant::now();
    cancel.send(true).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(500), request)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        ""
    );
    assert!(start.elapsed() < Duration::from_millis(500));
    server.abort();
}

#[tokio::test]
async fn stop_interrupts_backpressure_without_waiting_for_the_consumer() {
    let (config, server) = response(
        200,
        "data: {\"choices\":[{\"delta\":{\"content\":\"Partial\"}}]}\n\ndata: [DONE]\n".into(),
    )
    .await;
    let (cancel, receiver) = watch::channel(false);
    let (delivery, ready) = tokio::sync::oneshot::channel();
    let mut delivery = Some(delivery);
    let request = tokio::spawn(async move {
        engine::stream_chat_bounded(
            &engine::http_client(),
            &config,
            &[],
            &TurnContext::default(),
            Arc::new(receiver),
            |_| {
                delivery.take().unwrap().send(()).unwrap();
                std::future::pending::<()>()
            },
        )
        .await
    });
    ready.await.unwrap();
    cancel.send(true).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(500), request)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        "Partial"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn errors_redact_credentials_and_abrupt_eof_does_not_claim_completion() {
    for (status, body, expected) in [
        (401, "invalid private-test-key".to_owned(), "[redacted]"),
        (
            200,
            "data: {\"error\":\"invalid private-test-key\"}\n".to_owned(),
            "[redacted]",
        ),
        (
            200,
            "data: {\"choices\":[{\"delta\":{\"content\":\"Incomplete\"}}]}\n".to_owned(),
            "before confirming completion",
        ),
        (200, "x".repeat(256 * 1024 + 1), "size limit"),
    ] {
        let (config, server) = response(status, body).await;
        let error = engine::stream_chat(
            &engine::http_client(),
            &config,
            &[],
            &TurnContext::default(),
            Arc::new(watch::channel(false).1),
            |_| {},
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("private-test-key"));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn inactive_response_body_times_out() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = ProviderConfig::new("gemini", "test-key", "model");
    config.base_url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n")
            .await
            .unwrap();
        std::future::pending::<()>().await;
    });
    let _ = engine::http_client(); // Install the app's TLS provider before overriding the timeout.
    let client = reqwest::Client::builder()
        .read_timeout(Duration::from_millis(50))
        .build()
        .unwrap();
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        engine::stream_chat(
            &client,
            &config,
            &[],
            &TurnContext::default(),
            Arc::new(watch::channel(false).1),
            |_| {},
        ),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(matches!(error, engine::ChatError::Request(e) if e.is_timeout()));
    server.abort();
}
