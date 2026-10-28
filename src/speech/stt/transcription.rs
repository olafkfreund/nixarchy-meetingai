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
use crate::speech::stt::filter::LineFilter;

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
    crate::core::network::speech_client()
}

/// Pull the transcript out of a provider response.
pub fn extract_transcript(provider: &SttProvider, value: &Value) -> Option<String> {
    let text = match provider.wire {
        SttWire::InProcess => None,
        SttWire::WhisperServer => value.get("text").and_then(Value::as_str).map(str::to_owned),
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
    transcribe_with_prompt(config, wav, language, "").await
}

/// Like [`transcribe`], with a whisper `prompt` (recent text and glossary).
pub async fn transcribe_with_prompt(
    config: &ProviderConfig,
    wav: Vec<u8>,
    language: Option<&str>,
    prompt: &str,
) -> Result<String, SttError> {
    transcribe_inner(config, wav, language, prompt)
        .await
        .map_err(|error| {
            let redact = |text: String| {
                if config.api_key.is_empty() {
                    text
                } else {
                    text.replace(&config.api_key, "[redacted]")
                }
            };
            match error {
                SttError::Request(e) => SttError::Request(e.without_url()),
                SttError::Status { status, message } => SttError::Status {
                    status,
                    message: redact(message),
                },
                SttError::Protocol(message) => SttError::Protocol(redact(message)),
                error => error,
            }
        })
}

async fn transcribe_inner(
    config: &ProviderConfig,
    wav: Vec<u8>,
    language: Option<&str>,
    prompt: &str,
) -> Result<String, SttError> {
    let language =
        language.or_else(|| (!config.language.is_empty()).then_some(config.language.as_str()));
    if let Some(custom) = &config.custom {
        custom.validate(true).map_err(SttError::Protocol)?;
        let method = reqwest::Method::from_bytes(custom.method.as_bytes())
            .map_err(|_| SttError::Protocol("invalid request method".into()))?;
        let mut endpoint = reqwest::Url::parse(&config.base_url)
            .map_err(|_| SttError::Protocol("invalid speech endpoint".into()))?;
        if custom.request_format == "raw-wav" {
            if !config.model.is_empty() {
                endpoint
                    .query_pairs_mut()
                    .append_pair("model", &config.model);
            }
            if let Some(language) = language {
                endpoint.query_pairs_mut().append_pair("language", language);
            }
        }
        let mut request = client().request(method, endpoint);
        if let Some((header, value)) = custom.header(&config.api_key) {
            request = request.header(header, value);
        }
        request = match custom.request_format.as_str() {
            "json-wav" => request.json(&json!({"audio": STANDARD.encode(wav), "model": config.model, "language": language})),
            "multipart-wav" => {
                // Generated boundary never appears in audio; no intermediate file.
                let boundary = format!("nixarchy-meetingai-{}", uuid::Uuid::new_v4());
                let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n", config.model).into_bytes();
                body.extend(wav);
                body.extend(format!("\r\n--{boundary}--\r\n").as_bytes());
                request.header("Content-Type", format!("multipart/form-data; boundary={boundary}")).body(body)
            }
            _ => request.header("Content-Type", "audio/wav").body(wav),
        };
        let response = request.send().await?;
        let status = response.status();
        let bytes = crate::core::network::read_body(
            response,
            if status.is_success() {
                4 * 1024 * 1024
            } else {
                16 * 1024
            },
        )
        .await
        .map_err(|e| SttError::Protocol(e.to_string()))?;
        let text = String::from_utf8(bytes)
            .map_err(|_| SttError::Protocol("response contained invalid UTF-8".into()))?;
        if !status.is_success() {
            return Err(SttError::Status {
                status: status.as_u16(),
                message: text,
            });
        }
        return custom.text(&text).map_err(SttError::Protocol);
    }
    let provider = SttProvider::by_id(&config.id).ok_or(SttError::NotConfigured)?;
    if provider.wire == SttWire::InProcess {
        // Parakeet takes no prompt or language; it is multilingual and resamples itself.
        let reader = hound::WavReader::new(std::io::Cursor::new(wav))
            .map_err(|e| SttError::Encode(e.to_string()))?;
        let spec = reader.spec();
        if spec.channels != 1
            || spec.bits_per_sample != 16
            || spec.sample_format != hound::SampleFormat::Int
        {
            return Err(SttError::Encode("Parakeet needs mono 16-bit audio".into()));
        }
        let samples = reader
            .into_samples::<i16>()
            .map(|s| s.map(|v| f32::from(v) / 32768.0))
            .collect::<Result<Vec<f32>, _>>()
            .map_err(|e| SttError::Encode(e.to_string()))?;
        return crate::speech::stt::parakeet::transcribe(samples, spec.sample_rate as i32)
            .await
            .map(|t| t.trim().to_owned())
            .map_err(SttError::Protocol);
    }
    if provider.wire == SttWire::WhisperServer {
        let endpoint = if config.base_url.is_empty() {
            provider.endpoint
        } else {
            &config.base_url
        };
        let file = reqwest::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;
        let mut form = reqwest::multipart::Form::new()
            .part("file", file)
            .text("response_format", "json")
            .text("temperature", "0");
        if !prompt.is_empty() {
            form = form.text("prompt", prompt.to_owned());
        }
        if let Some(language) = language.filter(|v| !v.is_empty()) {
            form = form.text("language", language.to_owned());
        }
        let response = client().post(endpoint).multipart(form).send().await?;
        let status = response.status();
        let bytes = crate::core::network::read_body(response, 4 * 1024 * 1024)
            .await
            .map_err(|e| SttError::Protocol(e.to_string()))?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if !status.is_success() {
            return Err(SttError::Status {
                status: status.as_u16(),
                message: text.chars().take(512).collect(),
            });
        }
        let value: Value = serde_json::from_str(&text)
            .map_err(|_| SttError::Protocol("response was not JSON".into()))?;
        // Empty text is silence (a cough), not a failure.
        return Ok(extract_transcript(provider, &value).unwrap_or_default());
    }
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
    let bytes = crate::core::network::read_body(
        response,
        if status.is_success() {
            4 * 1024 * 1024
        } else {
            16 * 1024
        },
    )
    .await
    .map_err(|e| SttError::Protocol(e.to_string()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| SttError::Protocol("response contained invalid UTF-8".into()))?;
    if !status.is_success() {
        return Err(SttError::Status {
            status: status.as_u16(),
            message: text,
        });
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| SttError::Protocol("response was not JSON".into()))?;
    Ok(extract_transcript(provider, &value).unwrap_or_default())
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
        let mut parsed = reqwest::Url::parse(&url).expect("bundled Deepgram endpoint");
        parsed.query_pairs_mut().append_pair("language", language);
        url = parsed.into();
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
        // A provider that misses silence boundaries must not accumulate a whole
        // meeting in its current turn. Split at a final fragment, preserving words.
        if self.text[channel].len() >= 64_000 {
            completed.push(std::mem::take(&mut self.text[channel]));
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

/// Optional hints for batch providers; the daemon fills `glossary` from the profile.
#[derive(Debug, Clone, Default)]
pub struct SttHints {
    /// Names and terms (profile and meeting context) that whisper should spell right.
    pub glossary: String,
}

/// A running live transcription session. Dropping it cancels the socket task.
pub struct LiveTranscriber {
    audio: mpsc::Sender<Vec<i16>>,
    stop: watch::Sender<bool>,
    events: mpsc::Receiver<LiveEvent>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl LiveTranscriber {
    pub async fn connect_config(config: &ProviderConfig) -> Result<Self, SttError> {
        Self::connect_config_with(config, SttHints::default()).await
    }

    /// Like [`connect_config`](Self::connect_config); hints help batch (whisper) providers.
    pub async fn connect_config_with(
        config: &ProviderConfig,
        hints: SttHints,
    ) -> Result<Self, SttError> {
        if !config.speech_usable() {
            return Err(SttError::NotConfigured);
        }
        if !config.speech_streaming() {
            return Ok(Self::connect_batch(config.clone(), hints));
        }
        let Some(custom) = &config.custom else {
            let mut url = deepgram_url(validate_model(&config.model)?, Some(&config.language));
            if !config.base_url.is_empty() {
                let endpoint = config
                    .base_url
                    .replace("https://", "wss://")
                    .replace("http://", "ws://");
                let query = url.split_once('?').map(|(_, q)| q).unwrap_or_default();
                url = format!(
                    "{endpoint}{}{query}",
                    if endpoint.contains('?') { "&" } else { "?" }
                );
            }
            return Self::connect_url(config.api_key.clone(), url).await;
        };
        custom.validate(true).map_err(SttError::Protocol)?;
        let mut url = reqwest::Url::parse(&custom.stream_url)
            .map_err(|_| SttError::Protocol("invalid stream endpoint".into()))?;
        url.query_pairs_mut().extend_pairs([
            ("model", config.model.as_str()),
            ("encoding", "linear16"),
            ("sample_rate", "16000"),
            ("channels", "2"),
            ("multichannel", "true"),
            ("interim_results", "true"),
            ("punctuate", "true"),
            ("endpointing", "300"),
            ("utterance_end_ms", "1000"),
        ]);
        if !config.language.is_empty() {
            url.query_pairs_mut()
                .append_pair("language", &config.language);
        }
        Self::connect_authenticated(url.into(), custom.header(&config.api_key)).await
    }

    /// A bounded batch adapter keeps microphone and remote speakers separate.
    /// Silence closes an utterance; long speech is split at fifteen seconds.
    fn connect_batch(config: ProviderConfig, hints: SttHints) -> Self {
        let (audio, mut incoming) = mpsc::channel::<Vec<i16>>(64);
        let (stop, mut stopping) = watch::channel(false);
        let (sender, events) = mpsc::channel(128);
        let task = tokio::spawn(async move {
            let mut chunks = BatchChunks::default();
            let mut filter = LineFilter::default();
            let mut requests = futures_util::stream::FuturesOrdered::new();
            loop {
                let finished;
                let ready = tokio::select! {
                    _ = stopping.changed() => {
                        let mut ready = Vec::new();
                        while let Ok(pcm) = incoming.try_recv() { ready.extend(chunks.push(&pcm)); }
                        ready.extend(chunks.finish());
                        finished = true; ready
                    }
                    pcm = incoming.recv() => {
                        match pcm { Some(pcm) => { finished = false; chunks.push(&pcm) }, None => { finished = true; chunks.finish() } }
                    }
                    result = requests.next(), if !requests.is_empty() => {
                        match result {
                            Some(Ok(Some(line))) => { if filter.accept(&line) { let _ = sender.send(LiveEvent::Transcript(line)).await; } }
                            Some(Ok(None)) => {}
                            Some(Err(error)) => { let _ = sender.send(LiveEvent::Failed(error)).await; break; }
                            None => {}
                        }
                        continue;
                    }
                };
                for channel in chunks.started.drain(..) {
                    let _ = sender.send(LiveEvent::SpeechStarted { channel }).await;
                }
                for (channel, start, end, samples) in ready {
                    if requests.len() >= 4 {
                        let _ = sender.send(LiveEvent::Failed("The speech service fell behind. Recording stopped; start again to reconnect.".into())).await;
                        return;
                    }
                    let config = config.clone();
                    let prompt = filter.prompt(channel, &hints.glossary);
                    requests.push_back(async move {
                        let text = transcribe_with_retry(&config, &samples, &prompt)
                            .await
                            .map_err(|e| format!("{e} (lost audio {start:.1}s..{end:.1}s)"))?;
                        if text.is_empty() {
                            return Ok(None);
                        }
                        Ok::<_, String>(Some(TranscriptLine {
                            channel,
                            speaker: None,
                            text,
                            is_final: true,
                            speech_final: true,
                            start,
                            end,
                        }))
                    });
                }
                if finished {
                    while let Some(result) = requests.next().await {
                        match result {
                            Ok(Some(line)) => {
                                if filter.accept(&line) {
                                    let _ = sender.send(LiveEvent::Transcript(line)).await;
                                }
                            }
                            Ok(None) => {}
                            Err(error) => {
                                let _ = sender.send(LiveEvent::Failed(error)).await;
                                break;
                            }
                        }
                    }
                    break;
                }
            }
        });
        Self {
            audio,
            stop,
            events,
            task: Some(task),
        }
    }
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
        Self::connect_authenticated(
            url,
            Some(("Authorization".into(), format!("Token {api_key}"))),
        )
        .await
    }

    async fn connect_authenticated(
        url: String,
        auth: Option<(String, String)>,
    ) -> Result<Self, SttError> {
        crate::core::install_tls_provider();
        let mut request = url
            .into_client_request()
            .map_err(|e| SttError::Socket(e.to_string()))?;
        if let Some((name, value)) = auth {
            request.headers_mut().insert(
                tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| SttError::Protocol("invalid authentication header".into()))?,
                HeaderValue::from_str(&value).map_err(|_| SttError::MissingKey)?,
            );
        }
        let (socket, _) = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio_tungstenite::connect_async_with_config(
                request,
                Some(
                    tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
                        .max_message_size(Some(1024 * 1024))
                        .max_frame_size(Some(1024 * 1024)),
                ),
                true,
            ),
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
                            deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(4);
                            tokio::time::timeout_at(deadline, async {
                                while let Ok(audio) = audio_rx.try_recv() {
                                    let bytes: Vec<u8> = audio.into_iter().flat_map(i16::to_le_bytes).collect();
                                    send_live(&mut sink, Message::Binary(bytes.into())).await?;
                                }
                                send_live(&mut sink, Message::Text(json!({"type":"Finalize"}).to_string().into())).await?;
                                send_live(&mut sink, Message::Text(json!({"type":"CloseStream"}).to_string().into())).await
                            }).await.map_err(|_| "The speech connection timed out while finishing the last audio.".to_owned())??;
                            closing = true;
                        }
                        audio = audio_rx.recv(), if !closing => {
                            let Some(audio) = audio else { break };
                            let bytes: Vec<u8> = audio.into_iter().flat_map(i16::to_le_bytes).collect();
                            send_live(&mut sink, Message::Binary(bytes.into())).await?;
                        }
                        _ = keepalive.tick(), if !closing => {
                            send_live(&mut sink, Message::Text(json!({"type":"KeepAlive"}).to_string().into())).await?;
                        }
                        _ = tokio::time::sleep_until(deadline), if closing => {
                            return Err("Deepgram did not finish the last words before the close timeout".into());
                        }
                        message = source.next() => {
                            match message {
                                Some(Ok(Message::Text(text))) => {
                                    for event in parse_live_events(text.as_bytes()) {
                                        let delivery_deadline = if closing { deadline } else { tokio::time::Instant::now() + std::time::Duration::from_secs(1) };
                                        match tokio::time::timeout_at(delivery_deadline, event_tx.send(event)).await {
                                            Ok(Ok(())) => {},
                                            Ok(Err(_)) => return Ok(()),
                                            Err(_) => return Err("The transcript consumer fell behind; recording stopped.".into()),
                                        }
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
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(1),
                    event_tx.send(LiveEvent::Failed(error)),
                )
                .await;
            }
            let _ = tokio::time::timeout(std::time::Duration::from_secs(1), sink.close()).await;
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

async fn send_live<S>(sink: &mut S, message: Message) -> Result<(), String>
where
    S: futures_util::Sink<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    tokio::time::timeout(std::time::Duration::from_secs(1), sink.send(message)).await
        .map_err(|_| "The speech connection stopped accepting audio. Recording stopped; start again to reconnect.".to_owned())?
        .map_err(|e| e.to_string())
}

type BatchChunk = (u8, f64, f64, Vec<f32>);
/// A sample above this level counts as sound (about -42 dBFS).
const SOUND_LEVEL: f32 = 0.008;
/// 0.2 s at 16 kHz.
const MIN_SOUND_SAMPLES: usize = 3_200;
#[derive(Default)]
struct BatchChunks {
    started: Vec<u8>,
    samples: [Vec<f32>; 2],
    silence: [usize; 2],
    position: usize,
    start: [usize; 2],
}
impl BatchChunks {
    fn push(&mut self, pcm: &[i16]) -> Vec<BatchChunk> {
        let mut ready = Vec::new();
        for frame in pcm.as_chunks::<2>().0 {
            for (channel, value) in frame.iter().enumerate() {
                let sample = f32::from(*value) / f32::from(i16::MAX);
                let speech = sample.abs() > SOUND_LEVEL;
                if self.samples[channel].is_empty() {
                    if !speech {
                        continue;
                    }
                    self.start[channel] = self.position;
                    self.started.push(channel as u8);
                }
                self.samples[channel].push(sample);
                self.silence[channel] = if speech { 0 } else { self.silence[channel] + 1 };
                if (self.silence[channel] >= 9600 || self.samples[channel].len() >= 240_000)
                    && let Some(chunk) = self.take(channel)
                {
                    ready.push(chunk);
                }
            }
            self.position += 1;
        }
        ready
    }
    fn take(&mut self, channel: usize) -> Option<BatchChunk> {
        let samples = std::mem::take(&mut self.samples[channel]);
        self.silence[channel] = 0;
        // A click or breath opens a chunk too; whisper turns such near-silence into invented
        // words ("Amen."), so send only chunks with at least 0.2 s of sound in them.
        let sound = samples.iter().filter(|s| s.abs() > SOUND_LEVEL).count();
        (samples.len() >= 1600 && sound >= MIN_SOUND_SAMPLES).then_some((
            channel as u8,
            self.start[channel] as f64 / 16000.0,
            self.position as f64 / 16000.0,
            samples,
        ))
    }
    fn finish(&mut self) -> Vec<BatchChunk> {
        (0..2).filter_map(|c| self.take(c)).collect()
    }
}

impl Drop for LiveTranscriber {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Loopback providers (whisper-server) may be restarting: retry transient failures.
async fn transcribe_with_retry(
    config: &ProviderConfig,
    samples: &[f32],
    prompt: &str,
) -> Result<String, SttError> {
    let loopback = SttProvider::by_id(&config.id).is_some_and(|p| p.wire == SttWire::WhisperServer)
        && reqwest::Url::parse(if config.base_url.is_empty() {
            "http://127.0.0.1"
        } else {
            &config.base_url
        })
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| matches!(h, "localhost" | "127.0.0.1" | "[::1]"))
        })
        .unwrap_or(false);
    let mut delays = [250u64, 1000, 3000].into_iter();
    loop {
        let result = transcribe_with_prompt(config, to_wav(samples)?, None, prompt).await;
        let transient = matches!(
            result,
            Err(SttError::Request(_) | SttError::Status { status: 500.., .. })
        );
        match delays.next() {
            Some(ms) if loopback && transient => {
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await
            }
            _ => return result,
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
    #[test]
    fn a_click_in_silence_is_not_sent_but_speech_is() {
        let pcm = |s: &[f32]| -> Vec<i16> {
            s.iter()
                .flat_map(|v| {
                    let x = (v * f32::from(i16::MAX)) as i16;
                    [x, 0]
                })
                .collect()
        };
        let mut chunks = super::BatchChunks::default();
        // A 0.05 s click, then a second of silence: nothing goes to whisper.
        let mut click = vec![0.3_f32; 800];
        click.extend(vec![0.0; 16_000]);
        assert!(chunks.push(&pcm(&click)).is_empty());
        assert!(chunks.finish().is_empty());
        // Half a second of a loud tone, then silence: one mic chunk.
        let mut tone: Vec<f32> = (0..8_000).map(|i| 0.2 * (i as f32 * 0.1).sin()).collect();
        tone.extend(vec![0.0; 16_000]);
        let ready = chunks.push(&pcm(&tone));
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, 0);
    }

    use super::*;
    use crate::ai::providers::STT_PROVIDERS;

    /// Two channels through the batch path with a fake recogniser: channel and times survive,
    /// and `finish` flushes chunks that never saw trailing silence.
    #[tokio::test]
    async fn parakeet_batch_keeps_channel_and_times_and_flushes_on_stop() {
        crate::speech::stt::parakeet::set_decoder(Box::new(|_, samples| {
            let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            if peak < 0.3 {
                "hello from the microphone"
            } else {
                "remote voice on the call"
            }
            .to_owned()
        }));
        let config = ProviderConfig::new("parakeet-local", "", "");
        let mut live = LiveTranscriber::connect_batch(config, SttHints::default());
        // Mic speaks from 0.5 s to 1.5 s, the call from 0 s to 1 s; the audio ends mid-speech.
        let audio: Vec<f32> = (0..24_000)
            .flat_map(|i| {
                [
                    if i >= 8_000 { 0.2 } else { 0.0 },
                    if i < 16_000 { 0.4 } else { 0.0 },
                ]
            })
            .collect();
        live.push(&audio).await.unwrap();
        live.finish();
        let mut lines = Vec::new();
        while let Some(event) = live.next_event().await {
            if let LiveEvent::Transcript(line) = event {
                lines.push(line);
            }
        }
        lines.sort_by_key(|l| l.channel);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(
            (lines[0].channel, lines[0].text.as_str()),
            (0, "hello from the microphone")
        );
        assert_eq!(
            (lines[1].channel, lines[1].text.as_str()),
            (1, "remote voice on the call")
        );
        assert!((lines[0].start - 0.5).abs() < 0.01 && (lines[0].end - 1.5).abs() < 0.01);
        assert!(lines[1].start.abs() < 0.01 && lines[1].end > 1.0);
    }

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
    fn missing_endpoint_events_do_not_grow_a_turn_for_the_whole_meeting() {
        let mut turns = LiveTurns::default();
        let mut completed = Vec::new();
        for i in 0..200 {
            completed.extend(turns.push(&TranscriptLine {
                channel: 1,
                speaker: Some(0),
                text: "é".repeat(1000),
                is_final: true,
                speech_final: false,
                start: i as f64,
                end: i as f64 + 1.0,
            }));
            assert!(turns.text[1].len() < 64_000);
        }
        completed.extend(turns.finish(1, 201.0));
        assert_eq!(
            completed
                .iter()
                .map(|s| s.matches('é').count())
                .sum::<usize>(),
            200_000
        );
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
            assert!(provider.wire == SttWire::InProcess || provider.endpoint.starts_with("http"));
        }
    }
}
