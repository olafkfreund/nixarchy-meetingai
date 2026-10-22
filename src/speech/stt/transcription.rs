//! Speech-to-text.
//!
//! Two paths, both bring-your-own-key:
//!
//! * [`transcribe`] sends a finished WAV chunk to a batch provider.
//! * [`LiveTranscriber`] streams raw PCM to Deepgram over a WebSocket and emits
//!   interim and final transcript lines.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::Message;

use crate::ai::providers::{ProviderConfig, SttProvider, SttWire};
use crate::audio::{SAMPLE_RATE, encode_wav};

/// Errors from speech transcription.
#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("no speech provider is configured")]
    NotConfigured,
    #[error("this speech provider needs an API key")]
    MissingKey,
    #[error("the request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("audio could not be encoded: {0}")]
    Encode(String),
    #[error("the provider returned {status}: {message}")]
    Status { status: u16, message: String },
    #[error("the provider returned an unexpected response: {0}")]
    Protocol(String),
    #[error("the live connection failed: {0}")]
    Socket(String),
}

fn client() -> reqwest::Client {
    crate::core::install_tls_provider();
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("a default HTTP client always builds")
}

/// Pull the transcript out of a provider response.
pub fn extract_transcript(provider: &SttProvider, value: &Value) -> Option<String> {
    let text = match provider.wire {
        SttWire::RawAudio => value
            .pointer("/results/channels/0/alternatives/0/transcript")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }?;
    let trimmed = text.trim().to_owned();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Transcribe one WAV buffer.
pub async fn transcribe(
    config: &ProviderConfig,
    wav: Vec<u8>,
    language: Option<&str>,
) -> Result<String, SttError> {
    let provider = SttProvider::by_id(&config.id).ok_or(SttError::NotConfigured)?;
    if config.api_key.trim().is_empty() {
        return Err(SttError::MissingKey);
    }
    let model = if config.model.trim().is_empty() {
        provider.default_model
    } else {
        config.model.trim()
    };
    let http = client();

    let endpoint = if config.base_url.is_empty() {
        provider.endpoint
    } else {
        &config.base_url
    };
    let mut url = reqwest::Url::parse(endpoint)
        .map_err(|_| SttError::Protocol("invalid speech endpoint".into()))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("model", model)
            .append_pair("diarize", "true")
            .append_pair("smart_format", "true")
            .append_pair("punctuate", "true");
        if let Some(language) = language.filter(|v| !v.is_empty()) {
            query.append_pair("language", language);
        }
    }
    let request = http
        .post(url)
        .header("Authorization", format!("Token {}", config.api_key))
        .header("Content-Type", "audio/wav")
        .body(wav);

    let response = request.send().await?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(SttError::Status {
            status: status.as_u16(),
            message: text,
        });
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| SttError::Protocol("response was not JSON".into()))?;
    extract_transcript(provider, &value)
        .ok_or_else(|| SttError::Protocol("no transcript in the response".into()))
}

/// One transcript line from the live stream.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptLine {
    /// Which capture channel produced it: `0` microphone, `1` system audio.
    pub channel: u8,
    pub speaker: Option<i64>,
    pub text: String,
    pub is_final: bool,
    pub speech_final: bool,
    pub start: f64,
    pub end: f64,
}

/// Deepgram streaming endpoint for raw 16-bit two-channel PCM.
pub fn deepgram_url(model: &str, language: Option<&str>) -> String {
    let mut url = format!(
        "wss://api.deepgram.com/v1/listen?model={model}&encoding=linear16&sample_rate={SAMPLE_RATE}&channels=2&multichannel=true&diarize_model=latest&interim_results=true&smart_format=true&punctuate=true&vad_events=true&endpointing=300&utterance_end_ms=1000"
    );
    if let Some(language) = language.filter(|value| !value.is_empty() && *value != "en") {
        url.push_str(&format!("&language={language}"));
    }
    url
}

/// Validate a model name before it is interpolated into a URL.
pub fn validate_model(model: &str) -> Result<&str, SttError> {
    let model = if model.trim().is_empty() {
        "nova-3"
    } else {
        model.trim()
    };
    let valid = model.len() <= 60
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(model)
    } else {
        Err(SttError::Protocol("invalid speech model".into()))
    }
}

#[derive(Deserialize)]
struct DeepgramMessage {
    #[serde(default)]
    channel_index: Vec<i64>,
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    speech_final: bool,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    channel: Option<DeepgramChannel>,
}

#[derive(Deserialize)]
struct DeepgramChannel {
    #[serde(default)]
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Deserialize)]
struct DeepgramAlternative {
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    words: Vec<DeepgramWord>,
}

#[derive(Deserialize)]
struct DeepgramWord {
    #[serde(default)]
    speaker: Option<i64>,
    #[serde(default)]
    word: String,
    #[serde(default)]
    punctuated_word: Option<String>,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    end: f64,
}

/// Parse one Deepgram `Results` message into a transcript line.
pub fn parse_live_message(payload: &[u8]) -> Option<TranscriptLine> {
    let message: DeepgramMessage = serde_json::from_slice(payload).ok()?;
    let channel = message
        .channel_index
        .first()
        .copied()
        .unwrap_or(0)
        .clamp(0, 1) as u8;
    let alternative = message.channel?.alternatives.into_iter().next()?;
    let transcript = alternative.transcript.trim().to_owned();
    // Interim results with no words carry nothing useful.
    if transcript.is_empty() && !message.speech_final {
        return None;
    }
    let speaker = alternative
        .words
        .iter()
        .filter_map(|word| word.speaker)
        .next()
        .or_else(|| alternative.words.first().and_then(|word| word.speaker));
    Some(TranscriptLine {
        channel,
        speaker,
        text: transcript,
        is_final: message.is_final,
        speech_final: message.speech_final,
        start: message.start,
        end: message.start + message.duration,
    })
}

/// Provider events, including silence boundaries and errors that must reach the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveEvent {
    Transcript(TranscriptLine),
    SpeechStarted { channel: u8 },
    UtteranceEnd { channel: u8, last_word_end: f64 },
    Failed(String),
}

pub fn parse_live_event(payload: &[u8]) -> Option<LiveEvent> {
    let value: Value = serde_json::from_slice(payload).ok()?;
    let channel = value
        .get("channel")
        .and_then(|v| v.as_array().and_then(|a| a.first()).or(Some(v)))
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(1) as u8;
    match value.get("type").and_then(Value::as_str) {
        Some("SpeechStarted") => Some(LiveEvent::SpeechStarted { channel }),
        Some("UtteranceEnd") => Some(LiveEvent::UtteranceEnd {
            channel,
            last_word_end: value
                .get("last_word_end")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        }),
        Some("Error") => Some(LiveEvent::Failed(
            value
                .get("description")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("Deepgram rejected the live stream")
                .to_owned(),
        )),
        _ => parse_live_message(payload).map(LiveEvent::Transcript),
    }
}

/// Preserve speaker changes within a single final result, not just its first speaker.
pub fn parse_live_events(payload: &[u8]) -> Vec<LiveEvent> {
    let Some(event) = parse_live_event(payload) else {
        return Vec::new();
    };
    let LiveEvent::Transcript(line) = &event else {
        return vec![event];
    };
    if !line.is_final {
        return vec![event];
    }
    let Some(alternative) = serde_json::from_slice::<DeepgramMessage>(payload)
        .ok()
        .and_then(|m| m.channel)
        .and_then(|c| c.alternatives.into_iter().next())
    else {
        return vec![event];
    };
    if alternative.words.is_empty() || alternative.words.iter().any(|w| w.word.is_empty()) {
        return vec![event];
    }
    let mut lines: Vec<TranscriptLine> = Vec::new();
    for word in alternative.words {
        let text = word.punctuated_word.unwrap_or(word.word);
        if let Some(last) = lines.last_mut().filter(|last| last.speaker == word.speaker) {
            last.text.push(' ');
            last.text.push_str(&text);
            last.end = word.end;
        } else {
            lines.push(TranscriptLine {
                channel: line.channel,
                speaker: word.speaker,
                text,
                is_final: true,
                speech_final: false,
                start: word.start,
                end: word.end,
            });
        }
    }
    if let Some(last) = lines.last_mut() {
        last.speech_final = line.speech_final;
    }
    lines.into_iter().map(LiveEvent::Transcript).collect()
}

/// Stable fragments accumulate independently until endpointing or UtteranceEnd.
#[derive(Default)]
pub struct LiveTurns {
    text: [String; 2],
    last_end: [f64; 2],
    speaker: [Option<i64>; 2],
}
impl LiveTurns {
    pub fn push(&mut self, line: &TranscriptLine) -> Vec<String> {
        let mut completed = Vec::new();
        if !line.is_final {
            return completed;
        }
        let channel = usize::from(line.channel.min(1));
        if let (Some(previous), Some(next)) = (self.speaker[channel], line.speaker)
            && previous != next
            && !self.text[channel].is_empty()
        {
            completed.push(std::mem::take(&mut self.text[channel]));
        }
        if !line.text.is_empty() {
            self.speaker[channel] = line.speaker.or(self.speaker[channel]);
            if !self.text[channel].is_empty() {
                self.text[channel].push(' ');
            }
            self.text[channel].push_str(&line.text);
            self.last_end[channel] = line.end;
        }
        if line.speech_final {
            completed.extend(self.finish(line.channel, line.end));
        }
        completed
    }
    pub fn finish(&mut self, channel: u8, last_word_end: f64) -> Option<String> {
        let channel = usize::from(channel.min(1));
        // A delayed boundary for an earlier utterance cannot flush the next turn.
        if last_word_end + 0.01 < self.last_end[channel] {
            return None;
        }
        self.speaker[channel] = None;
        let text = std::mem::take(&mut self.text[channel]);
        (!text.is_empty()).then_some(text)
    }
}

/// A running live transcription session. Dropping it cancels the socket task.
pub struct LiveTranscriber {
    audio: mpsc::Sender<Vec<i16>>,
    stop: watch::Sender<bool>,
    events: mpsc::Receiver<LiveEvent>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl LiveTranscriber {
    pub async fn connect(
        api_key: String,
        model: String,
        language: Option<String>,
    ) -> Result<Self, SttError> {
        let model = validate_model(&model)?;
        Self::connect_url(api_key, deepgram_url(model, language.as_deref())).await
    }

    /// Endpoint injection is also used by protocol integration tests.
    pub async fn connect_url(api_key: String, url: String) -> Result<Self, SttError> {
        if api_key.trim().is_empty() {
            return Err(SttError::MissingKey);
        }
        crate::core::install_tls_provider();
        let mut request = url
            .into_client_request()
            .map_err(|e| SttError::Socket(e.to_string()))?;
        request.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str(&format!("Token {api_key}")).map_err(|_| SttError::MissingKey)?,
        );
        let (socket, _) = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio_tungstenite::connect_async(request),
        )
        .await
        .map_err(|_| SttError::Socket("Deepgram connection timed out".into()))?
        .map_err(|e| SttError::Socket(e.to_string()))?;
        let (mut sink, mut source) = socket.split();
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<i16>>(64);
        let (stop_tx, mut stop_rx) = watch::channel(false);
        let (event_tx, event_rx) = mpsc::channel::<LiveEvent>(128);
        let task = tokio::spawn(async move {
            let mut keepalive = tokio::time::interval(std::time::Duration::from_secs(3));
            keepalive.tick().await;
            let mut closing = false;
            let mut deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(86400);
            let result: Result<(), String> = async {
                loop {
                    tokio::select! {
                        _ = stop_rx.changed(), if !closing => {
                            // Every previously accepted audio block precedes finalization.
                            while let Ok(audio) = audio_rx.try_recv() {
                                let bytes: Vec<u8> = audio.into_iter().flat_map(i16::to_le_bytes).collect();
                                sink.send(Message::Binary(bytes.into())).await.map_err(|e| e.to_string())?;
                            }
                            sink.send(Message::Text(json!({"type":"Finalize"}).to_string().into())).await.map_err(|e| e.to_string())?;
                            sink.send(Message::Text(json!({"type":"CloseStream"}).to_string().into())).await.map_err(|e| e.to_string())?;
                            closing = true;
                            deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(4);
                        }
                        audio = audio_rx.recv(), if !closing => {
                            let Some(audio) = audio else { break };
                            let bytes: Vec<u8> = audio.into_iter().flat_map(i16::to_le_bytes).collect();
                            sink.send(Message::Binary(bytes.into())).await.map_err(|e| e.to_string())?;
                        }
                        _ = keepalive.tick(), if !closing => {
                            sink.send(Message::Text(json!({"type":"KeepAlive"}).to_string().into())).await.map_err(|e| e.to_string())?;
                        }
                        _ = tokio::time::sleep_until(deadline), if closing => {
                            return Err("Deepgram did not finish the last words before the close timeout".into());
                        }
                        message = source.next() => {
                            match message {
                                Some(Ok(Message::Text(text))) => {
                                    for event in parse_live_events(text.as_bytes()) {
                                        if event_tx.send(event).await.is_err() { return Ok(()); }
                                    }
                                }
                                Some(Ok(Message::Close(_))) | None => {
                                    if !closing { return Err("Deepgram disconnected; recording stopped. Start again to reconnect.".into()); }
                                    break;
                                }
                                Some(Err(error)) => return Err(error.to_string()),
                                Some(Ok(_)) => {}
                            }
                        }
                    }
                }
                Ok(())
            }.await;
            if let Err(error) = result {
                let _ = event_tx.send(LiveEvent::Failed(error)).await;
            }
            let _ = sink.close().await;
        });
        Ok(Self {
            audio: audio_tx,
            stop: stop_tx,
            events: event_rx,
            task: Some(task),
        })
    }

    /// Send interleaved microphone/call samples with bounded backpressure.
    pub async fn push(&self, samples: &[f32]) -> Result<(), SttError> {
        let pcm = samples
            .iter()
            .map(|s| {
                if s.is_finite() {
                    (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
                } else {
                    0
                }
            })
            .collect();
        tokio::time::timeout(std::time::Duration::from_secs(1), self.audio.send(pcm))
            .await
            .map_err(|_| SttError::Socket("the live audio connection fell behind".into()))?
            .map_err(|_| SttError::Socket("the live stream has ended".into()))
    }

    pub async fn next_event(&mut self) -> Option<LiveEvent> {
        self.events.recv().await
    }
    pub fn try_event(&mut self) -> Result<LiveEvent, mpsc::error::TryRecvError> {
        self.events.try_recv()
    }
    /// Request finalization; continue receiving events until the stream closes.
    pub fn finish(&self) {
        let _ = self.stop.send(true);
    }
}

impl Drop for LiveTranscriber {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Encode samples as WAV, shared by batch transcription and local chunks.
pub fn to_wav(samples: &[f32]) -> Result<Vec<u8>, SttError> {
    encode_wav(samples).map_err(|error| SttError::Encode(error.to_string()))
}

/// Convenience for callers that already hold f32 samples.
pub async fn transcribe_samples(
    config: &ProviderConfig,
    samples: &[f32],
    language: Option<&str>,
) -> Result<String, SttError> {
    transcribe(config, to_wav(samples)?, language).await
}

/// Message shape expected by the frontend for a local chunk.
pub fn local_chunk_payload(samples: &[f32], start: f64) -> Result<Value, SttError> {
    Ok(json!({
        "wavBase64": STANDARD.encode(to_wav(samples)?),
        "start": start,
        "duration": samples.len() as f64 / f64::from(SAMPLE_RATE),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::providers::STT_PROVIDERS;

    #[test]
    fn speaker_changes_preserve_word_identity_and_one_endpoint() {
        let events = parse_live_events(br#"{"type":"Results","channel_index":[1,2],"is_final":true,"speech_final":true,"channel":{"alternatives":[{"transcript":"Hello. How are you?","words":[{"word":"hello","punctuated_word":"Hello.","speaker":0,"start":0.1,"end":0.4},{"word":"how","punctuated_word":"How","speaker":1,"start":0.6,"end":0.8},{"word":"are","speaker":1,"start":0.9,"end":1.0},{"word":"you","punctuated_word":"you?","speaker":1,"start":1.1,"end":1.3}]}]}}"#);
        assert_eq!(events.len(), 2);
        assert!(
            matches!(&events[0], LiveEvent::Transcript(line) if line.text == "Hello." && line.speaker == Some(0) && !line.speech_final)
        );
        assert!(
            matches!(&events[1], LiveEvent::Transcript(line) if line.text == "How are you?" && line.speaker == Some(1) && line.speech_final)
        );
    }

    #[test]
    fn speaker_changes_complete_separate_turns_without_a_silence_boundary() {
        let mut turns = LiveTurns::default();
        let line = |speaker, text: &str, speech_final, start, end| TranscriptLine {
            channel: 1,
            speaker: Some(speaker),
            text: text.into(),
            is_final: true,
            speech_final,
            start,
            end,
        };
        assert!(
            turns
                .push(&line(0, "How do we invalidate the cache?", false, 0.0, 1.0))
                .is_empty()
        );
        assert_eq!(
            turns.push(&line(1, "We need to consider outages.", false, 1.1, 2.0)),
            ["How do we invalidate the cache?"]
        );
        assert_eq!(
            turns.push(&line(2, "What if Redis fails?", true, 2.1, 3.0)),
            ["We need to consider outages.", "What if Redis fails?"]
        );
        assert!(
            turns.finish(1, 3.0).is_none(),
            "endpoint must not emit duplicate turns"
        );
    }

    #[test]
    fn stale_utterance_end_does_not_flush_the_next_turn() {
        let mut turns = LiveTurns::default();
        let line = TranscriptLine {
            channel: 1,
            text: "What comes next".into(),
            is_final: true,
            speech_final: false,
            start: 5.0,
            end: 7.0,
            speaker: None,
        };
        assert!(turns.push(&line).is_empty());
        assert_eq!(turns.finish(1, 3.0), None);
        assert_eq!(turns.finish(1, 7.0).as_deref(), Some("What comes next"));
        assert_eq!(turns.finish(1, 7.0), None);
    }

    #[test]
    fn extracts_transcripts_per_wire_format() {
        let deepgram = SttProvider::by_id("deepgram-stt").unwrap();
        assert_eq!(
            extract_transcript(
                deepgram,
                &json!({"results":{"channels":[{"alternatives":[{"transcript":"hi there"}]}]}})
            ),
            Some("hi there".into())
        );
        assert_eq!(
            extract_transcript(
                deepgram,
                &json!({"results":{"channels":[{"alternatives":[{"transcript":" "}]}]}})
            ),
            None
        );
    }

    #[test]
    fn live_url_carries_native_parameters() {
        let url = deepgram_url("nova-3", None);
        assert!(url.starts_with("wss://api.deepgram.com/v1/listen?"));
        assert!(url.contains("encoding=linear16"));
        assert!(url.contains("sample_rate=16000"));
        assert!(url.contains("diarize_model=latest"));
        assert!(url.contains("channels=2&multichannel=true"));
        assert!(url.contains("interim_results=true"));
        assert!(!url.contains("language="), "default language is omitted");
        assert!(deepgram_url("nova-3", Some("de")).contains("language=de"));
    }

    #[test]
    fn model_names_are_validated() {
        assert_eq!(validate_model("").unwrap(), "nova-3");
        assert_eq!(validate_model("nova-3").unwrap(), "nova-3");
        assert!(validate_model("bad model").is_err());
        assert!(validate_model("evil&inject=1").is_err());
        assert!(validate_model(&"x".repeat(61)).is_err());
    }

    #[test]
    fn parses_final_and_interim_lines() {
        let payload = br#"{
            "channel_index": [1, 2],
            "is_final": true,
            "speech_final": true,
            "start": 1.5,
            "duration": 2.0,
            "channel": {"alternatives": [{"transcript": "hello team", "words": [{"speaker": 2, "word": "hello"}]}]}
        }"#;
        let line = parse_live_message(payload).expect("line");
        assert_eq!(line.channel, 1);
        assert_eq!(line.speaker, Some(2));
        assert_eq!(line.text, "hello team");
        assert!(line.is_final && line.speech_final);
        assert_eq!(line.start, 1.5);
        assert_eq!(line.end, 3.5);
    }

    #[test]
    fn empty_interim_is_dropped_but_empty_final_is_kept() {
        let interim = br#"{"channel_index":[0],"is_final":false,"channel":{"alternatives":[{"transcript":""}]}}"#;
        assert!(parse_live_message(interim).is_none());
        let final_boundary = br#"{"channel_index":[0],"is_final":true,"speech_final":true,"channel":{"alternatives":[{"transcript":""}]}}"#;
        let line = parse_live_message(final_boundary).expect("final boundary");
        assert!(line.is_final && line.speech_final);
    }

    #[test]
    fn malformed_messages_are_ignored() {
        assert!(parse_live_message(b"not json").is_none());
    }

    #[test]
    fn local_chunk_payload_is_complete() {
        let payload = local_chunk_payload(&vec![0.0; 16_000], 2.0).unwrap();
        assert_eq!(payload["start"], 2.0);
        assert_eq!(payload["duration"], 1.0);
        let wav = STANDARD
            .decode(payload["wavBase64"].as_str().unwrap())
            .unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
    }

    #[test]
    fn every_stt_provider_has_defaults() {
        for provider in STT_PROVIDERS {
            assert!(!provider.name.is_empty());
            assert!(provider.endpoint.starts_with("https://"));
        }
    }
}
