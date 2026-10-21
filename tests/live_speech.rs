//! Real WebSocket protocol tests: channel identity, interim/VAD events and trailing finals.
use futures_util::{SinkExt, StreamExt};
use lennette::speech::transcription::{LiveEvent, LiveTranscriber};
use std::time::Duration;
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        Message,
        handshake::server::{Request, Response},
    },
};

#[tokio::test]
#[allow(clippy::result_large_err)] // tungstenite fixes the callback error type.
async fn streams_stereo_and_receives_final_words_after_stop() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_hdr_async(tcp, |request: &Request, response: Response| {
            assert_eq!(request.headers()["Authorization"], "Token test-key");
            Ok(response)
        })
        .await
        .unwrap();
        let Message::Binary(bytes) = ws.next().await.unwrap().unwrap() else {
            panic!("expected PCM");
        };
        let samples: Vec<i16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(samples, vec![8191, -16383, 0, 32767]);
        for event in [
            r#"{"type":"SpeechStarted","channel":[1,2]}"#,
            r#"{"type":"Results","channel_index":[1,2],"is_final":false,"channel":{"alternatives":[{"transcript":"What is"}]}}"#,
            r#"{"type":"Results","channel_index":[0,2],"is_final":true,"speech_final":true,"channel":{"alternatives":[{"transcript":"My side.","words":[{"speaker":0}]}]}}"#,
        ] {
            ws.send(Message::Text(event.into())).await.unwrap();
        }
        assert!(
            ws.next()
                .await
                .unwrap()
                .unwrap()
                .to_text()
                .unwrap()
                .contains("Finalize")
        );
        // Providers deliver the trailing final only after the client asks to finalize.
        ws.send(Message::Text(r#"{"type":"Results","channel_index":[1,2],"is_final":true,"speech_final":true,"channel":{"alternatives":[{"transcript":"What is Rust?","words":[{"speaker":2}]}]}}"#.into())).await.unwrap();
        assert!(
            ws.next()
                .await
                .unwrap()
                .unwrap()
                .to_text()
                .unwrap()
                .contains("CloseStream")
        );
        ws.close(None).await.unwrap();
    });
    let mut live =
        LiveTranscriber::connect_url("test-key".into(), format!("ws://{address}/listen"))
            .await
            .unwrap();
    live.push(&[0.25, -0.5, f32::NAN, 2.0]).await.unwrap();
    assert_eq!(
        live.next_event().await,
        Some(LiveEvent::SpeechStarted { channel: 1 })
    );
    assert!(
        matches!(live.next_event().await, Some(LiveEvent::Transcript(line)) if !line.is_final && line.channel == 1)
    );
    assert!(
        matches!(live.next_event().await, Some(LiveEvent::Transcript(line)) if line.is_final && line.channel == 0)
    );
    live.finish();
    let final_line = tokio::time::timeout(Duration::from_secs(2), live.next_event())
        .await
        .unwrap();
    assert!(
        matches!(final_line, Some(LiveEvent::Transcript(line)) if line.text == "What is Rust?" && line.channel == 1 && line.speaker == Some(2))
    );
    assert_eq!(live.next_event().await, None);
    server.await.unwrap();
}

#[tokio::test]
async fn unexpected_disconnect_is_reported() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        ws.close(None).await.unwrap();
    });
    let mut live = LiveTranscriber::connect_url("test-key".into(), format!("ws://{address}"))
        .await
        .unwrap();
    assert!(
        matches!(live.next_event().await, Some(LiveEvent::Failed(message)) if message.contains("disconnected"))
    );
    server.await.unwrap();
}
