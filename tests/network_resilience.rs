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
async fn openai_compatible_speech_gives_up_with_the_lost_range() {
    use nixarchy_meetingai::speech::transcription::LiveEvent;
    let (config, server) = speech_mock(vec![None]).await;
    let events = run_speech(&config).await;
    let failure = events.iter().find_map(|e| match e {
        LiveEvent::Failed(m) => Some(m.clone()),
        _ => None,
    });
    assert!(
        failure.is_some_and(|m| m.contains("lost audio") && m.contains("s..")),
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
