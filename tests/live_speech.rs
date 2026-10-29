//! The live speech client against a real WebSocket server: handshake, audio frames, events, shutdown.
use futures_util::{SinkExt, StreamExt};
use nixarchy_meetingai::ai::{catalog::custom::CustomProvider, providers::ProviderConfig};
use nixarchy_meetingai::speech::transcription::{LiveEvent, LiveTranscriber, TranscriptLine};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::{WebSocketStream, accept_hdr_async};

const WAIT: Duration = Duration::from_secs(8);

type Socket = WebSocketStream<TcpStream>;

/// What the client said while opening the connection.
struct Handshake {
    /// Header names are lowercased.
    headers: HashMap<String, String>,
    query: HashMap<String, String>,
}

/// Minimal percent-decoding for query values.
fn unescape(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[allow(clippy::result_large_err)]
fn note_handshake(
    request: &Request,
    response: Response,
    into: &mut Option<Handshake>,
) -> Result<Response, ErrorResponse> {
    let headers = request
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_lowercase(),
                value.to_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    let query = request
        .uri()
        .query()
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.to_owned(), unescape(value)))
        .collect();
    *into = Some(Handshake { headers, query });
    Ok(response)
}

async fn bind() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    (listener, format!("ws://{addr}"))
}

#[allow(clippy::result_large_err)]
async fn accept(listener: &TcpListener) -> (Socket, Handshake) {
    let (tcp, _) = listener.accept().await.unwrap();
    let mut seen = None;
    let socket = accept_hdr_async(tcp, |request: &Request, response: Response| {
        note_handshake(request, response, &mut seen)
    })
    .await
    .unwrap();
    (socket, seen.unwrap())
}

/// A Deepgram-shaped `Results` message; a speaker adds one tagged word.
fn results(channel: u8, text: &str, is_final: bool, speaker: Option<i64>) -> Message {
    let mut alternative = json!({ "transcript": text });
    if let Some(speaker) = speaker {
        alternative["words"] = json!([{
            "word": text.to_lowercase().trim_end_matches('.'),
            "punctuated_word": text,
            "speaker": speaker,
            "start": 0.5,
            "end": 1.25,
        }]);
    }
    let message = json!({
        "type": "Results",
        "channel_index": [channel, 2],
        "is_final": is_final,
        "speech_final": is_final,
        "start": 0.5,
        "duration": 0.75,
        "channel": { "alternatives": [alternative] },
    });
    Message::text(message.to_string())
}

fn started(channel: u8) -> Message {
    Message::text(json!({ "type": "SpeechStarted", "channel": [channel, 2] }).to_string())
}

/// Reads client frames until one satisfies `want`; keep-alives and the like are skipped.
async fn frame_where(socket: &mut Socket, want: impl Fn(&Message) -> bool) -> Message {
    timeout(WAIT, async {
        loop {
            let frame = socket
                .next()
                .await
                .expect("client hung up")
                .expect("bad frame");
            if want(&frame) {
                return frame;
            }
        }
    })
    .await
    .expect("the expected frame never came")
}

fn is_command(frame: &Message, name: &str) -> bool {
    matches!(frame, Message::Text(text) if text.as_str().contains(name))
}

async fn next_event(live: &mut LiveTranscriber) -> Option<LiveEvent> {
    timeout(WAIT, live.next_event())
        .await
        .expect("no event in time")
}

fn transcript(event: Option<LiveEvent>) -> TranscriptLine {
    match event {
        Some(LiveEvent::Transcript(line)) => line,
        other => panic!("expected a transcript, got {other:?}"),
    }
}

#[tokio::test]
async fn custom_streaming_provider_sends_its_header_and_query() {
    let (listener, base) = bind().await;
    let mut config = ProviderConfig::new("custom-rt-12", "live-key-6c1", "vendor/stt-v5");
    config.base_url = "http://127.0.0.1:9/batch".into();
    config.language = "nl-NL".into();
    config.custom = Some(CustomProvider {
        auth: "header".into(),
        auth_header: "x-live-auth".into(),
        request_format: "raw-wav".into(),
        response_path: "/text".into(),
        streaming: true,
        stream_url: format!("{base}/rt/listen?region=eu-west-4"),
        ..CustomProvider::default()
    });

    let server: JoinHandle<Handshake> = tokio::spawn(async move {
        let (mut socket, handshake) = accept(&listener).await;
        socket
            .send(results(1, "Welcome everyone", true, None))
            .await
            .unwrap();
        frame_where(&mut socket, |f| is_command(f, "CloseStream")).await;
        socket.close(None).await.unwrap();
        handshake
    });

    let mut live = LiveTranscriber::connect_config(&config).await.unwrap();
    let first = transcript(next_event(&mut live).await);
    assert_eq!(
        (first.channel, first.text.as_str()),
        (1, "Welcome everyone")
    );
    live.finish();
    while next_event(&mut live).await.is_some() {}

    let handshake = server.await.unwrap();
    assert_eq!(handshake.headers["x-live-auth"], "live-key-6c1");
    assert!(!handshake.headers.contains_key("authorization"));
    let query = &handshake.query;
    assert_eq!(query["model"], "vendor/stt-v5");
    assert_eq!(query["channels"], "2");
    assert_eq!(query["sample_rate"], "16000");
    assert_eq!(query["region"], "eu-west-4");
    assert_eq!(query["language"], "nl-NL");
}

#[tokio::test]
async fn stereo_audio_goes_out_and_the_last_final_still_comes_back() {
    let (listener, base) = bind().await;
    let server = tokio::spawn(async move {
        let (mut socket, handshake) = accept(&listener).await;
        let audio = frame_where(&mut socket, |f| matches!(f, Message::Binary(_))).await;
        let Message::Binary(bytes) = audio else {
            unreachable!()
        };
        socket.send(started(1)).await.unwrap();
        socket
            .send(results(1, "so the", false, None))
            .await
            .unwrap();
        socket
            .send(results(0, "Hello team.", true, None))
            .await
            .unwrap();
        frame_where(&mut socket, |f| is_command(f, "Finalize")).await;
        socket
            .send(results(1, "Agreed.", true, Some(3)))
            .await
            .unwrap();
        frame_where(&mut socket, |f| is_command(f, "CloseStream")).await;
        socket.close(None).await.unwrap();
        (handshake, bytes.to_vec())
    });

    let mut live = LiveTranscriber::connect_url("tk-live-880".into(), format!("{base}/live/v9"))
        .await
        .unwrap();
    live.push(&[0.5, -0.25, f32::INFINITY, -3.0]).await.unwrap();

    assert_eq!(
        next_event(&mut live).await,
        Some(LiveEvent::SpeechStarted { channel: 1 })
    );
    let interim = transcript(next_event(&mut live).await);
    assert_eq!((interim.channel, interim.is_final), (1, false));
    assert_eq!(interim.text, "so the");
    let done = transcript(next_event(&mut live).await);
    assert_eq!((done.channel, done.text.as_str()), (0, "Hello team."));
    assert!(done.is_final && done.speech_final);

    live.finish();
    let tail = transcript(next_event(&mut live).await);
    assert_eq!(tail.text, "Agreed.");
    assert_eq!((tail.channel, tail.speaker), (1, Some(3)));
    assert!(tail.is_final && tail.speech_final);
    assert_eq!(next_event(&mut live).await, None);

    let (handshake, pcm) = server.await.unwrap();
    assert_eq!(handshake.headers["authorization"], "Token tk-live-880");
    let mut samples = Vec::new();
    let mut at = 0;
    while at + 1 < pcm.len() {
        samples.push(i16::from(pcm[at]) | (i16::from(pcm[at + 1]) << 8));
        at += 2;
    }
    assert_eq!(samples, [16383, -8191, 0, -32767]);
}

#[tokio::test]
async fn a_server_that_hangs_up_is_reported() {
    let (listener, base) = bind().await;
    let server = tokio::spawn(async move {
        let (mut socket, _) = accept(&listener).await;
        socket.close(None).await.unwrap();
    });
    let mut live = LiveTranscriber::connect_url("tk-live-881".into(), format!("{base}/gone"))
        .await
        .unwrap();
    let failure = loop {
        match next_event(&mut live).await {
            Some(LiveEvent::Failed(message)) => break message,
            Some(_) => {}
            None => panic!("the stream ended without reporting the disconnect"),
        }
    };
    assert!(failure.contains("disconnected"), "{failure}");
    server.await.unwrap();
}
