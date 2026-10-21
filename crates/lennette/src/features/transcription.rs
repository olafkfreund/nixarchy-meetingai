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

use super::audio::{SAMPLE_RATE, encode_wav};
use super::providers::{ProviderConfig, SttProvider, SttWire};

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
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("a default HTTP client always builds")
}

/// Pull the transcript out of a provider response.
pub fn extract_transcript(provider: &SttProvider, value: &Value) -> Option<String> {
    let text = match provider.wire {
        SttWire::MultipartText => value.get("text").and_then(Value::as_str).map(str::to_owned),
        SttWire::RawAudio => value
            .pointer("/results/channels/0/alternatives/0/transcript")
            .and_then(Value::as_str)
            .map(str::to_owned),
        SttWire::GoogleJson => value
            .pointer("/results/0/alternatives/0/transcript")
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

    let request = match provider.wire {
        SttWire::MultipartText => {
            let part = reqwest::multipart::Part::bytes(wav)
                .file_name("audio.wav")
                .mime_str("audio/wav")
                .map_err(|error| SttError::Encode(error.to_string()))?;
            let mut form = reqwest::multipart::Form::new()
                .part("file", part)
                .text("model", model.to_owned());
            if let Some(language) = language.filter(|value| !value.is_empty()) {
                form = form.text("language", language.to_owned());
            }
            http.post(provider.endpoint)
                .bearer_auth(&config.api_key)
                .multipart(form)
        }
        SttWire::RawAudio => {
            let mut url = format!("{}?model={}", provider.endpoint, model);
            if let Some(language) = language.filter(|value| !value.is_empty()) {
                url.push_str(&format!("&language={language}"));
            }
            url.push_str("&diarize=true&smart_format=true");
            http.post(url)
                .header("Authorization", format!("Token {}", config.api_key))
                .header("Content-Type", "audio/wav")
                .body(wav)
        }
        SttWire::GoogleJson => {
            let body = json!({
                "config": {
                    "encoding": "LINEAR16",
                    "sampleRateHertz": SAMPLE_RATE,
                    "languageCode": language.unwrap_or("en-US"),
                },
                "audio": { "content": STANDARD.encode(&wav) },
            });
            http.post(provider.endpoint)
                .bearer_auth(&config.api_key)
                .json(&body)
        }
    };

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
    // Some providers answer in plain text rather than JSON.
    extract_transcript(provider, &value)
        .or_else(|| {
            let trimmed = text.trim();
            (!trimmed.is_empty() && !trimmed.starts_with('{')).then(|| trimmed.to_owned())
        })
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

/// Deepgram streaming endpoint for raw 16-bit mono PCM.
pub fn deepgram_url(model: &str, language: Option<&str>) -> String {
    let mut url = format!(
        "wss://api.deepgram.com/v1/listen?model={model}&encoding=linear16&sample_rate={SAMPLE_RATE}&channels=1&diarize=true&interim_results=true&smart_format=true&punctuate=true&vad_events=true&endpointing=300&utterance_end_ms=1000"
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

/// A running live transcription session.
pub struct LiveTranscriber {
    audio: mpsc::Sender<Vec<i16>>,
    stop: watch::Sender<bool>,
    lines: mpsc::Receiver<TranscriptLine>,
    task: tokio::task::JoinHandle<()>,
}

impl LiveTranscriber {
    /// Connect and start streaming.
    pub async fn connect(
        api_key: String,
        model: String,
        language: Option<String>,
    ) -> Result<Self, SttError> {
        let model = validate_model(&model)?.to_owned();
        let url = deepgram_url(&model, language.as_deref());

        let mut request = url
            .into_client_request()
            .map_err(|error| SttError::Socket(error.to_string()))?;
        request.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str(&format!("Token {api_key}")).map_err(|_| SttError::MissingKey)?,
        );

        let (socket, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|error| SttError::Socket(error.to_string()))?;
        let (mut sink, mut source) = socket.split();

        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<i16>>(64);
        let (stop_tx, mut stop_rx) = watch::channel(false);
        let (line_tx, line_rx) = mpsc::channel::<TranscriptLine>(64);

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_rx.changed() => break,
                    audio = audio_rx.recv() => {
                        let Some(audio) = audio else { break };
                        let mut bytes = Vec::with_capacity(audio.len() * 2);
                        for sample in audio {
                            bytes.extend_from_slice(&sample.to_le_bytes());
                        }
                        if sink.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    message = source.next() => {
                        match message {
                            Some(Ok(Message::Text(text))) => {
                                if let Some(line) = parse_live_message(text.as_bytes()) {
                                    let _ = line_tx.send(line).await;
                                }
                            }
                            Some(Ok(Message::Binary(bytes))) => {
                                if let Some(line) = parse_live_message(&bytes) {
                                    let _ = line_tx.send(line).await;
                                }
                            }
                            Some(Ok(_)) => {}
                            Some(Err(_)) | None => break,
                        }
                    }
                }
            }
            // Close the stream cleanly so the provider finalises its last words.
            let _ = sink
                .send(Message::Text(
                    json!({"type": "CloseStream"}).to_string().into(),
                ))
                .await;
            let _ = sink.close().await;
        });

        Ok(Self {
            audio: audio_tx,
            stop: stop_tx,
            lines: line_rx,
            task,
        })
    }

    /// Send a block of mono samples.
    pub async fn push(&self, samples: &[f32]) -> Result<(), SttError> {
        let pcm: Vec<i16> = samples
            .iter()
            .map(|sample| {
                let clamped = if sample.is_finite() {
                    sample.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                (clamped * f32::from(i16::MAX)) as i16
            })
            .collect();
        self.audio
            .send(pcm)
            .await
            .map_err(|_| SttError::Socket("the live stream has ended".into()))
    }

    /// Next transcript line, or `None` once the stream closes.
    pub async fn next_line(&mut self) -> Option<TranscriptLine> {
        self.lines.recv().await
    }

    /// Stop streaming and wait for the connection to close.
    pub async fn stop(self) {
        let _ = self.stop.send(true);
        let _ = self.task.await;
    }

    /// Keep the control handle across an owned stop.
    pub fn stop_handle(&self) -> watch::Sender<bool> {
        self.stop.clone()
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
    fn extracts_transcripts_per_wire_format() {
        let openai = SttProvider::by_id("openai-whisper").unwrap();
        assert_eq!(
            extract_transcript(openai, &json!({"text": "  hello  "})),
            Some("hello".into())
        );
        let deepgram = SttProvider::by_id("deepgram-stt").unwrap();
        assert_eq!(
            extract_transcript(
                deepgram,
                &json!({"results":{"channels":[{"alternatives":[{"transcript":"hi there"}]}]}})
            ),
            Some("hi there".into())
        );
        let google = SttProvider::by_id("google-stt").unwrap();
        assert_eq!(
            extract_transcript(
                google,
                &json!({"results":[{"alternatives":[{"transcript":"bonjour"}]}]})
            ),
            Some("bonjour".into())
        );
        assert_eq!(extract_transcript(openai, &json!({"text": "   "})), None);
    }

    #[test]
    fn live_url_carries_native_parameters() {
        let url = deepgram_url("nova-3", None);
        assert!(url.starts_with("wss://api.deepgram.com/v1/listen?"));
        assert!(url.contains("encoding=linear16"));
        assert!(url.contains("sample_rate=16000"));
        assert!(url.contains("diarize=true"));
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
