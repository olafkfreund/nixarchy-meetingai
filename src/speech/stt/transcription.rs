//! Speech recognition over complete WAV files and live stereo audio.

use crate::ai::providers::{ProviderConfig, SttProvider, SttWire};
use crate::audio::{SAMPLE_RATE, encode_wav};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::{
    Message, client::IntoClientRequest, protocol::WebSocketConfig,
};

/// A provider result placed on the session audio clock.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptLine {
    pub channel: u8,
    pub speaker: Option<i64>,
    pub text: String,
    pub is_final: bool,
    pub speech_final: bool,
    pub start: f64,
    pub end: f64,
}

/// An error returned while preparing or using speech recognition.
#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("Speech recognition is not configured")]
    NotConfigured,
    #[error("Speech recognition needs an API key")]
    MissingKey,
    #[error("Speech request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Could not encode speech audio: {0}")]
    Encode(String),
    #[error("Speech service returned {status}: {message}")]
    Status { status: u16, message: String },
    #[error("Speech response could not be used: {0}")]
    Protocol(String),
    #[error("Speech stream stopped: {0}")]
    Socket(String),
}

/// Events produced by a live or batch recognition session.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveEvent {
    Transcript(TranscriptLine),
    SpeechStarted { channel: u8 },
    UtteranceEnd { channel: u8, last_word_end: f64 },
    Failed(String),
}

#[derive(Default)]
struct Turn {
    text: String,
    end: f64,
    speaker: Option<i64>,
}

/// Collects final fragments into completed turns for each audio channel.
#[derive(Default)]
pub struct LiveTurns {
    channels: [Turn; 2],
}
impl LiveTurns {
    pub fn push(&mut self, line: &TranscriptLine) -> Vec<String> {
        if !line.is_final {
            return Vec::new();
        }
        let index = usize::from(line.channel.min(1));
        let mut ready = Vec::new();
        if line.speaker.is_some()
            && self.channels[index].speaker.is_some()
            && line.speaker != self.channels[index].speaker
            && let Some(text) = self.take(index)
        {
            ready.push(text);
        }
        let turn = &mut self.channels[index];
        let fragment = line.text.trim();
        if !fragment.is_empty() {
            if !turn.text.is_empty() {
                turn.text.push(' ');
            }
            turn.text.push_str(fragment);
            turn.end = line.end;
            if line.speaker.is_some() {
                turn.speaker = line.speaker;
            }
            if turn.text.len() >= 32_000
                && let Some(text) = self.take(index)
            {
                ready.push(text);
            }
        }
        if line.speech_final
            && let Some(text) = self.finish(line.channel, line.end)
        {
            ready.push(text);
        }
        ready
    }
    pub fn finish(&mut self, channel: u8, last_word_end: f64) -> Option<String> {
        let index = usize::from(channel.min(1));
        if last_word_end + 0.01 < self.channels[index].end {
            return None;
        }
        self.take(index)
    }
    fn take(&mut self, index: usize) -> Option<String> {
        let turn = &mut self.channels[index];
        turn.speaker = None;
        (!turn.text.is_empty()).then(|| std::mem::take(&mut turn.text))
    }
}

/// Transcribes a complete WAV recording.
pub async fn transcribe(
    config: &ProviderConfig,
    wav: Vec<u8>,
    language: Option<&str>,
) -> Result<String, SttError> {
    transcribe_with_prompt(config, wav, language, "").await
}

async fn transcribe_with_prompt(
    config: &ProviderConfig,
    wav: Vec<u8>,
    language: Option<&str>,
    prompt: &str,
) -> Result<String, SttError> {
    let language =
        language.or_else(|| (!config.language.is_empty()).then_some(config.language.as_str()));
    let result = if let Some(custom) = &config.custom {
        transcribe_custom(config, custom, wav, language).await
    } else if let Some(provider) = SttProvider::by_id(&config.id) {
        match provider.wire {
            SttWire::InProcess => transcribe_parakeet(wav).await,
            SttWire::OpenAiMultipart => {
                transcribe_openai(config, provider, wav, language, prompt).await
            }
            SttWire::RawAudio => transcribe_deepgram(config, provider, wav, language).await,
        }
    } else {
        Err(SttError::NotConfigured)
    };
    result.map_err(|error| match error {
        SttError::Request(error) => SttError::Request(error.without_url()),
        SttError::Status { status, message } => SttError::Status {
            status,
            message: hide_key(message, &config.api_key),
        },
        SttError::Protocol(message) => SttError::Protocol(hide_key(message, &config.api_key)),
        other => other,
    })
}
fn hide_key(message: String, key: &str) -> String {
    if key.is_empty() {
        message
    } else {
        message.replace(key, "[redacted]")
    }
}
async fn response_text(response: reqwest::Response) -> Result<String, SttError> {
    let status = response.status();
    let limit = if status.is_success() {
        4 * 1024 * 1024
    } else {
        16 * 1024
    };
    let body = crate::core::network::read_body(response, limit)
        .await
        .map_err(|e| SttError::Protocol(format!("Could not read speech reply: {e}")))?;
    let text = String::from_utf8(body)
        .map_err(|_| SttError::Protocol("Speech reply was not UTF-8".into()))?;
    if !status.is_success() {
        return Err(SttError::Status {
            status: status.as_u16(),
            message: text,
        });
    }
    Ok(text)
}
async fn transcribe_deepgram(
    config: &ProviderConfig,
    provider: &SttProvider,
    wav: Vec<u8>,
    language: Option<&str>,
) -> Result<String, SttError> {
    if config.api_key.trim().is_empty() {
        return Err(SttError::MissingKey);
    }
    let model = if config.model.trim().is_empty() {
        provider.default_model
    } else {
        config.model.trim()
    };
    let endpoint = if config.base_url.is_empty() {
        provider.endpoint
    } else {
        &config.base_url
    };
    let url = deepgram_batch_url(endpoint, model, language)?;
    let response = crate::core::network::speech_client()
        .post(url)
        .header("Authorization", format!("Token {}", config.api_key))
        .header("Content-Type", "audio/wav")
        .body(wav)
        .send()
        .await?;
    let text = response_text(response).await?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| SttError::Protocol("Speech service sent malformed JSON".into()))?;
    Ok(extract_transcript(provider, &value).unwrap_or_default())
}
async fn transcribe_custom(
    config: &ProviderConfig,
    custom: &crate::ai::catalog::custom::CustomProvider,
    wav: Vec<u8>,
    language: Option<&str>,
) -> Result<String, SttError> {
    custom.validate(true).map_err(SttError::Protocol)?;
    let method = reqwest::Method::from_bytes(custom.method.as_bytes())
        .map_err(|_| SttError::Protocol("Unsupported speech request method".into()))?;
    let mut url = reqwest::Url::parse(&config.base_url)
        .map_err(|_| SttError::Protocol("Speech endpoint URL is invalid".into()))?;
    if custom.request_format == "raw-wav" {
        let mut query = url.query_pairs_mut();
        if !config.model.is_empty() {
            query.append_pair("model", &config.model);
        }
        if let Some(language) = language {
            query.append_pair("language", language);
        }
    }
    let mut request = crate::core::network::speech_client().request(method, url);
    if let Some((name, value)) = custom.header(&config.api_key) {
        request = request.header(name, value);
    }
    request = match custom.request_format.as_str() {
        "json-wav" => request.json(&json!({"audio": base64::engine::general_purpose::STANDARD.encode(wav), "model": config.model, "language": language})),
        "multipart-wav" => {
            let boundary = format!("nixarchy-meetingai-{}", uuid::Uuid::new_v4());
            let mut body = Vec::new();
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n", config.model).as_bytes());
            body.extend_from_slice(&wav);
            body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
            request.header("Content-Type", format!("multipart/form-data; boundary={boundary}")).body(body)
        }
        _ => request.header("Content-Type", "audio/wav").body(wav),
    };
    let text = response_text(request.send().await?).await?;
    custom.text(&text).map_err(SttError::Protocol)
}
fn extract_transcript(provider: &SttProvider, value: &Value) -> Option<String> {
    let text = match provider.wire {
        SttWire::RawAudio => value.pointer("/results/channels/0/alternatives/0/transcript"),
        SttWire::OpenAiMultipart => value.get("text"),
        SttWire::InProcess => None,
    }?
    .as_str()?
    .trim();
    (!text.is_empty()).then(|| text.to_owned())
}
fn to_wav(samples: &[f32]) -> Result<Vec<u8>, SttError> {
    encode_wav(samples).map_err(|e| SttError::Encode(e.to_string()))
}
use crate::speech::stt::filter::LineFilter;
fn deepgram_batch_url(
    endpoint: &str,
    model: &str,
    language: Option<&str>,
) -> Result<reqwest::Url, SttError> {
    let mut url = reqwest::Url::parse(endpoint)
        .map_err(|_| SttError::Protocol("invalid speech endpoint".into()))?;
    let eu = url.host_str() == Some("api.eu.deepgram.com");
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
        if eu {
            query.append_pair("mip_opt_out", "true");
        }
    }
    Ok(url)
}
/// Optional hints for batch providers; the daemon fills `glossary` from the profile.
#[derive(Debug, Clone, Default)]
pub struct SttHints {
    /// Names and terms (profile and meeting context) that the engine should spell right (OpenAI-compatible prompt).
    pub glossary: String,
}
/// Channel, start, end (seconds) and samples of one utterance.
pub type BatchChunk = (u8, f64, f64, Vec<f32>);
/// A sample above this level counts as sound (about -42 dBFS).
const SOUND_LEVEL: f32 = 0.008;
/// 0.2 s at 16 kHz.
const MIN_SOUND_SAMPLES: usize = 3_200;
/// Transcribe one chunk of samples (language from the config).
pub async fn transcribe_chunk(
    config: &ProviderConfig,
    samples: &[f32],
    prompt: &str,
) -> Result<String, SttError> {
    transcribe_with_prompt(config, to_wav(samples)?, None, prompt).await
}
async fn transcribe_parakeet(wav: Vec<u8>) -> Result<String, SttError> {
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
    crate::speech::stt::parakeet::transcribe(samples, spec.sample_rate as i32)
        .await
        .map(|t| t.trim().to_owned())
        .map_err(SttError::Protocol)
}
async fn transcribe_openai(
    config: &ProviderConfig,
    provider: &SttProvider,
    wav: Vec<u8>,
    language: Option<&str>,
    prompt: &str,
) -> Result<String, SttError> {
    if !config.speech_usable() {
        return Err(SttError::NotConfigured);
    }
    let endpoint = if config.base_url.is_empty() {
        provider.endpoint
    } else {
        &config.base_url
    };
    let url = reqwest::Url::parse(endpoint)
        .map_err(|e| SttError::Protocol(format!("invalid speech base URL: {e}")))?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(SttError::Protocol(
            "speech base URL must not contain a query string or fragment".into(),
        ));
    }
    let base = endpoint.trim_end_matches('/');
    let base = base.strip_suffix("/v1").unwrap_or(base);
    let endpoint = format!("{base}/v1/audio/transcriptions");
    let file = reqwest::multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let mut form = reqwest::multipart::Form::new()
        .part("file", file)
        .text("model", config.model.clone());
    if !prompt.is_empty() {
        form = form.text("prompt", prompt.to_owned());
    }
    if let Some(language) = language.filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto")) {
        form = form.text("language", language.to_owned());
    }
    let mut request = crate::core::network::speech_client()
        .post(endpoint)
        .multipart(form);
    if !config.api_key.is_empty() {
        request = request.bearer_auth(&config.api_key);
    }
    let response = request.send().await?;
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
    Ok(extract_transcript(provider, &value).unwrap_or_default())
}

#[derive(Default)]
struct OpenChunk {
    start: f64,
    samples: Vec<f32>,
    sound: usize,
    quiet: usize,
}
/// Splits stereo PCM into recognisable utterances on each channel.
#[derive(Default)]
pub struct BatchChunks {
    frames: usize,
    open: [Option<OpenChunk>; 2],
    started: Vec<u8>,
}
impl BatchChunks {
    pub fn push(&mut self, pcm: &[i16]) -> Vec<BatchChunk> {
        let mut ready = Vec::new();
        for frame in pcm.as_chunks::<2>().0 {
            for (channel, sample) in frame.iter().enumerate() {
                let value = f32::from(*sample) / 32767.0;
                let sounding = value.abs() > SOUND_LEVEL;
                if self.open[channel].is_none() && sounding {
                    self.open[channel] = Some(OpenChunk {
                        start: self.now(),
                        ..OpenChunk::default()
                    });
                    self.started.push(channel as u8);
                }
                if let Some(chunk) = &mut self.open[channel] {
                    chunk.samples.push(value);
                    if sounding {
                        chunk.sound += 1;
                        chunk.quiet = 0;
                    } else {
                        chunk.quiet += 1;
                    }
                    if (chunk.quiet >= 9_600 || chunk.samples.len() >= 240_000)
                        && let Some(chunk) =
                            self.close(channel, (self.frames + 1) as f64 / f64::from(SAMPLE_RATE))
                    {
                        ready.push(chunk);
                    }
                }
            }
            self.frames += 1;
        }
        ready
    }
    pub fn finish(&mut self) -> Vec<BatchChunk> {
        (0..2)
            .filter_map(|channel| self.close(channel, self.now()))
            .collect()
    }
    fn close(&mut self, channel: usize, end: f64) -> Option<BatchChunk> {
        let chunk = self.open[channel].take()?;
        (chunk.samples.len() >= 1_600 && chunk.sound >= MIN_SOUND_SAMPLES).then_some((
            channel as u8,
            chunk.start,
            end,
            chunk.samples,
        ))
    }
    fn now(&self) -> f64 {
        self.frames as f64 / f64::from(SAMPLE_RATE)
    }
    fn pending_system(&self) -> Option<f64> {
        self.open[1].as_ref().map(|c| c.start)
    }
    fn started(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.started)
    }
}

fn channel_number(value: Option<&Value>) -> u8 {
    let value = value.and_then(|v| v.as_array().and_then(|a| a.first()).or(Some(v)));
    value.and_then(Value::as_u64).unwrap_or(0).min(1) as u8
}
fn event_lines(text: &str) -> Vec<LiveEvent> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    match value.get("type").and_then(Value::as_str) {
        Some("SpeechStarted") => {
            return vec![LiveEvent::SpeechStarted {
                channel: channel_number(value.get("channel")),
            }];
        }
        Some("UtteranceEnd") => {
            return vec![LiveEvent::UtteranceEnd {
                channel: channel_number(value.get("channel")),
                last_word_end: value
                    .get("last_word_end")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
            }];
        }
        Some("Error") => {
            return vec![LiveEvent::Failed(
                value
                    .get("description")
                    .or_else(|| value.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("Speech service reported an error; start recording again")
                    .to_owned(),
            )];
        }
        _ => {}
    }
    let Some(choice) = value.pointer("/channel/alternatives/0") else {
        return Vec::new();
    };
    let channel = channel_number(value.get("channel_index"));
    let is_final = value
        .get("is_final")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let speech_final = value
        .get("speech_final")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let start = value.get("start").and_then(Value::as_f64).unwrap_or(0.0);
    let end = start + value.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
    let words = choice.get("words").and_then(Value::as_array);
    let speaker = words.and_then(|w| {
        w.iter()
            .find_map(|v| v.get("speaker").and_then(Value::as_i64))
    });
    if is_final
        && let Some(words) = words
        && !words.is_empty()
        && words.iter().all(|w| {
            w.get("word")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
        })
    {
        let mut out = Vec::new();
        let mut run = Vec::new();
        let mut current = words[0].get("speaker").and_then(Value::as_i64);
        let mut run_start = words[0]
            .get("start")
            .and_then(Value::as_f64)
            .unwrap_or(start);
        for (i, word) in words.iter().enumerate() {
            let next = word.get("speaker").and_then(Value::as_i64);
            if i > 0 && next != current {
                out.push(LiveEvent::Transcript(TranscriptLine {
                    channel,
                    speaker: current,
                    text: run.join(" "),
                    is_final: true,
                    speech_final: false,
                    start: run_start,
                    end: words[i - 1]
                        .get("end")
                        .and_then(Value::as_f64)
                        .unwrap_or(end),
                }));
                run.clear();
                current = next;
                run_start = word.get("start").and_then(Value::as_f64).unwrap_or(start);
            }
            run.push(
                word.get("punctuated_word")
                    .and_then(Value::as_str)
                    .or_else(|| word.get("word").and_then(Value::as_str))
                    .unwrap_or(""),
            );
        }
        out.push(LiveEvent::Transcript(TranscriptLine {
            channel,
            speaker: current,
            text: run.join(" "),
            is_final: true,
            speech_final,
            start: run_start,
            end: words
                .last()
                .and_then(|w| w.get("end"))
                .and_then(Value::as_f64)
                .unwrap_or(end),
        }));
        return out;
    }
    let text = choice
        .get("transcript")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if text.is_empty() && !speech_final {
        return Vec::new();
    }
    vec![LiveEvent::Transcript(TranscriptLine {
        channel,
        speaker,
        text: text.to_owned(),
        is_final,
        speech_final,
        start,
        end,
    })]
}

fn validated_model(model: &str) -> Result<&str, SttError> {
    let model = model.trim();
    if model.is_empty() {
        return Ok("nova-3");
    }
    if model.len() > 60
        || !model
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
    {
        return Err(SttError::Protocol(
            "Speech model name contains unsupported characters or is too long".into(),
        ));
    }
    Ok(model)
}
fn live_url(model: &str, language: Option<&str>, eu: bool) -> Result<String, SttError> {
    let host = if eu {
        "api.eu.deepgram.com"
    } else {
        "api.deepgram.com"
    };
    let mut url = format!(
        "wss://{host}/v1/listen?model={}&encoding=linear16&sample_rate=16000&channels=2&multichannel=true&diarize_model=latest&interim_results=true&smart_format=true&punctuate=true&vad_events=true&endpointing=300&utterance_end_ms=1000",
        validated_model(model)?
    );
    if eu {
        url.push_str("&mip_opt_out=true");
    }
    if let Some(language) = language.filter(|l| !l.is_empty() && *l != "en") {
        let mut encoded_url = reqwest::Url::parse("https://speech.invalid/").unwrap();
        encoded_url
            .query_pairs_mut()
            .append_pair("language", language);
        let encoded = encoded_url.query().unwrap();
        url.push('&');
        url.push_str(encoded);
    }
    Ok(url)
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
/// An active stream of speech events.
pub struct LiveTranscriber {
    audio: mpsc::Sender<Vec<u8>>,
    events: mpsc::Receiver<LiveEvent>,
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}
impl LiveTranscriber {
    pub async fn connect_config(config: &ProviderConfig) -> Result<Self, SttError> {
        Self::connect_config_with(config, SttHints::default()).await
    }
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
        if let Some(custom) = &config.custom {
            custom.validate(true).map_err(SttError::Protocol)?;
            let mut url = reqwest::Url::parse(&custom.stream_url)
                .map_err(|_| SttError::Protocol("Speech stream URL is invalid".into()))?;
            {
                let mut query = url.query_pairs_mut();
                for (key, value) in [
                    ("model", config.model.as_str()),
                    ("encoding", "linear16"),
                    ("sample_rate", "16000"),
                    ("channels", "2"),
                    ("multichannel", "true"),
                    ("interim_results", "true"),
                    ("punctuate", "true"),
                    ("endpointing", "300"),
                    ("utterance_end_ms", "1000"),
                ] {
                    query.append_pair(key, value);
                }
                if !config.language.is_empty() {
                    query.append_pair("language", &config.language);
                }
            }
            return Self::open_socket(url.as_str(), custom.header(&config.api_key)).await;
        }
        let eu = config.base_url.starts_with("https://api.eu.deepgram.com/");
        let generated = live_url(&config.model, Some(&config.language), eu)?;
        let url = if config.base_url.is_empty() {
            generated
        } else {
            let base = config
                .base_url
                .replace("https://", "wss://")
                .replace("http://", "ws://");
            let query = generated.split_once('?').unwrap().1;
            format!(
                "{base}{}{query}",
                if base.contains('?') { "&" } else { "?" }
            )
        };
        Self::connect_url(config.api_key.clone(), url).await
    }
    pub async fn connect_url(api_key: String, url: String) -> Result<Self, SttError> {
        if api_key.trim().is_empty() {
            return Err(SttError::MissingKey);
        }
        Self::open_socket(
            &url,
            Some(("Authorization".into(), format!("Token {api_key}"))),
        )
        .await
    }
    async fn open_socket(url: &str, auth: Option<(String, String)>) -> Result<Self, SttError> {
        crate::core::install_tls_provider();
        let mut request = url
            .into_client_request()
            .map_err(|e| SttError::Socket(format!("Cannot prepare speech connection: {e}")))?;
        if let Some((name, value)) = auth {
            let name =
                tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| {
                        SttError::Protocol("Invalid speech authentication header".into())
                    })?;
            let value = tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&value)
                .map_err(|_| SttError::MissingKey)?;
            request.headers_mut().insert(name, value);
        }
        let config = WebSocketConfig::default()
            .max_message_size(Some(1024 * 1024))
            .max_frame_size(Some(1024 * 1024));
        let (socket, _) = tokio::time::timeout(
            Duration::from_secs(10),
            tokio_tungstenite::connect_async_with_config(request, Some(config), false),
        )
        .await
        .map_err(|_| SttError::Socket("Speech connection timed out; start recording again".into()))?
        .map_err(|e| SttError::Socket(format!("Could not connect to speech service: {e}")))?;
        Ok(Self::spawn_socket(socket))
    }
    fn spawn_socket(socket: Socket) -> Self {
        let (audio, input) = mpsc::channel(32);
        let (event_tx, events) = mpsc::channel(128);
        let (stop, stopping) = watch::channel(false);
        let task = tokio::spawn(async move {
            socket_task(socket, input, event_tx, stopping).await;
        });
        Self {
            audio,
            events,
            stop,
            task,
        }
    }
    fn connect_batch(config: ProviderConfig, hints: SttHints) -> Self {
        let (audio, input) = mpsc::channel(32);
        let (event_tx, events) = mpsc::channel(128);
        let (stop, stopping) = watch::channel(false);
        let task = tokio::spawn(async move {
            batch_task(config, hints, input, event_tx, stopping).await;
        });
        Self {
            audio,
            events,
            stop,
            task,
        }
    }
    pub async fn push(&self, samples: &[f32]) -> Result<(), SttError> {
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            let integer = if sample.is_finite() {
                (sample.clamp(-1.0, 1.0) * 32767.0) as i16
            } else {
                0
            };
            bytes.extend_from_slice(&integer.to_le_bytes());
        }
        tokio::time::timeout(Duration::from_secs(1), self.audio.send(bytes))
            .await
            .map_err(|_| {
                SttError::Socket("Speech audio queue fell behind; start recording again".into())
            })?
            .map_err(|_| SttError::Socket("Speech stream ended; start recording again".into()))
    }
    pub async fn next_event(&mut self) -> Option<LiveEvent> {
        self.events.recv().await
    }
    pub fn try_event(&mut self) -> Result<LiveEvent, mpsc::error::TryRecvError> {
        self.events.try_recv()
    }
    pub fn finish(&self) {
        let _ = self.stop.send(true);
    }
}
impl Drop for LiveTranscriber {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn send_socket(socket: &mut Socket, message: Message) -> Result<(), String> {
    tokio::time::timeout(Duration::from_secs(1), socket.send(message))
        .await
        .map_err(|_| "Speech service stopped accepting audio; start recording again".to_owned())?
        .map_err(|e| format!("Speech connection failed: {e}"))
}
async fn offer(
    tx: &mpsc::Sender<LiveEvent>,
    event: LiveEvent,
    deadline: Option<tokio::time::Instant>,
) -> Result<(), String> {
    let remaining = deadline.map_or(Duration::from_secs(1), |end| {
        end.saturating_duration_since(tokio::time::Instant::now())
            .min(Duration::from_secs(1))
    });
    tokio::time::timeout(remaining, tx.send(event))
        .await
        .map_err(|_| "Speech event consumer fell behind; start recording again".to_owned())?
        .map_err(|_| "receiver gone".to_owned())
}
async fn socket_task(
    mut socket: Socket,
    mut input: mpsc::Receiver<Vec<u8>>,
    tx: mpsc::Sender<LiveEvent>,
    mut stop: watch::Receiver<bool>,
) {
    let mut keepalive = tokio::time::interval(Duration::from_secs(3));
    keepalive.tick().await;
    let mut deadline = None;
    let mut finished = false;
    let result: Result<(), String> = 'work: loop {
        if *stop.borrow() && !finished && input.is_empty() {
            let end = tokio::time::Instant::now() + Duration::from_secs(4);
            deadline = Some(end);
            for command in ["{\"type\":\"Finalize\"}", "{\"type\":\"CloseStream\"}"] {
                let remaining = end.saturating_duration_since(tokio::time::Instant::now());
                match tokio::time::timeout(
                    remaining,
                    send_socket(&mut socket, Message::Text(command.into())),
                )
                .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => break 'work Err(e),
                    Err(_) => {
                        break 'work Err(
                            "Speech finalisation timed out; start recording again".into()
                        );
                    }
                }
            }
            finished = true;
        }
        tokio::select! {
            biased;
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    for event in event_lines(&text) {
                        if let Err(e) = offer(&tx, event, deadline).await { if e == "receiver gone" { return; } break 'work Err(e); }
                    }
                }
                Some(Ok(Message::Close(_))) | None => break if finished { Ok(()) } else { Err("Speech service disconnected; start recording again".into()) },
                Some(Err(e)) => break Err(format!("Speech connection failed: {e}")),
                _ => {}
            },
            _ = keepalive.tick(), if !finished => {
                if let Err(e) = send_socket(&mut socket, Message::Text("{\"type\":\"KeepAlive\"}".into())).await { break Err(e); }
            }
            _ = stop.changed(), if !finished => {},
            _ = tokio::time::sleep_until(deadline.unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(86_400))), if finished => break Err("Speech service did not close after finalisation; start recording again".into()),
            block = input.recv(), if !finished => {
                if let Some(block) = block && let Err(e) = send_socket(&mut socket, Message::Binary(block.into())).await { break Err(e); }
            }
        }
    };
    if let Err(message) = result {
        let _ = offer(&tx, LiveEvent::Failed(message), None).await;
        let _ = tokio::time::timeout(Duration::from_secs(1), socket.close(None)).await;
    }
}
type Pending = futures_util::stream::FuturesOrdered<
    std::pin::Pin<
        Box<dyn std::future::Future<Output = (u8, f64, f64, Result<String, SttError>)> + Send>,
    >,
>;
fn queue_chunks(
    pending: &mut Pending,
    chunks: Vec<BatchChunk>,
    config: &ProviderConfig,
    filter: &LineFilter,
    glossary: &str,
) -> Result<(), String> {
    for (channel, start, end, samples) in chunks {
        if pending.len() >= 4 {
            return Err("Speech requests fell behind; start recording again".into());
        }
        let config = config.clone();
        let prompt = filter.prompt(channel, glossary);
        pending.push_back(Box::pin(async move {
            (
                channel,
                start,
                end,
                transcribe_chunk(&config, &samples, &prompt).await,
            )
        }));
    }
    Ok(())
}
async fn emit_lines(tx: &mpsc::Sender<LiveEvent>, lines: Vec<TranscriptLine>) {
    for line in lines {
        let _ = tx.send(LiveEvent::Transcript(line)).await;
    }
}
async fn batch_task(
    config: ProviderConfig,
    hints: SttHints,
    mut input: mpsc::Receiver<Vec<u8>>,
    tx: mpsc::Sender<LiveEvent>,
    mut stop: watch::Receiver<bool>,
) {
    let mut chunks = BatchChunks::default();
    let mut filter = LineFilter::default();
    let mut pending = Pending::new();
    let mut system_requests: std::collections::VecDeque<f64> = std::collections::VecDeque::new();
    let mut error = None;
    loop {
        if (*stop.borrow() || input.is_closed()) && input.is_empty() {
            break;
        }
        tokio::select! {
            biased;
            result = pending.next(), if !pending.is_empty() => {
                if let Some((channel, start, end, result)) = result {
                    if channel == 1 { system_requests.pop_front(); }
                    match result {
                        Ok(text) if !text.is_empty() => {
                            let line = TranscriptLine { channel, speaker: None, text, is_final: true, speech_final: true, start, end };
                            emit_lines(&tx, filter.accept(&line)).await;
                        }
                        Err(e) => { error = Some(format!("{e} (lost audio {start:.1}s..{end:.1}s)")); break; }
                        _ => {}
                    }
                    let earliest = [system_requests.front().copied(), chunks.pending_system()].into_iter().flatten().reduce(f64::min);
                    emit_lines(&tx, filter.tick(chunks.now(), earliest)).await;
                }
            }
            _ = stop.changed() => {},
            block = input.recv() => { let Some(block) = block else { break; }; {
                let pcm: Vec<i16> = block.as_chunks::<2>().0.iter().map(|pair| i16::from_le_bytes(*pair)).collect();
                let ready = chunks.push(&pcm);
                for channel in chunks.started() { let _ = tx.send(LiveEvent::SpeechStarted { channel }).await; }
                for chunk in &ready { if chunk.0 == 1 { system_requests.push_back(chunk.1); } }
                if let Err(e) = queue_chunks(&mut pending, ready, &config, &filter, &hints.glossary) { error = Some(e); break; }
                let earliest = [system_requests.front().copied(), chunks.pending_system()].into_iter().flatten().reduce(f64::min);
                emit_lines(&tx, filter.tick(chunks.now(), earliest)).await;
            } }
        }
    }
    if error.is_none() {
        let ready = chunks.finish();
        for chunk in &ready {
            if chunk.0 == 1 {
                system_requests.push_back(chunk.1);
            }
        }
        if let Err(e) = queue_chunks(&mut pending, ready, &config, &filter, &hints.glossary) {
            error = Some(e);
        }
    }
    if error.is_none() {
        while let Some((channel, start, end, result)) = pending.next().await {
            if channel == 1 {
                system_requests.pop_front();
            }
            match result {
                Ok(text) if !text.is_empty() => {
                    let line = TranscriptLine {
                        channel,
                        speaker: None,
                        text,
                        is_final: true,
                        speech_final: true,
                        start,
                        end,
                    };
                    emit_lines(&tx, filter.accept(&line)).await;
                }
                Err(e) => {
                    error = Some(format!("{e} (lost audio {start:.1}s..{end:.1}s)"));
                    break;
                }
                _ => {}
            }
            emit_lines(
                &tx,
                filter.tick(chunks.now(), system_requests.front().copied()),
            )
            .await;
        }
    }
    emit_lines(&tx, filter.flush()).await;
    if let Some(message) = error {
        let _ = tx.send(LiveEvent::Failed(message)).await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn speaker_runs_preserve_final_boundary() {
        let events = event_lines(
            r#"{"type":"Results","is_final":true,"speech_final":true,"channel_index":[1],"channel":{"alternatives":[{"transcript":"later","words":[{"word":"good","punctuated_word":"Good","speaker":4,"start":2.0,"end":2.2},{"word":"morning","punctuated_word":"morning.","speaker":4,"start":2.2,"end":2.6},{"word":"indeed","punctuated_word":"Indeed!","speaker":8,"start":2.8,"end":3.1}]}]}}"#,
        );
        assert_eq!(events.len(), 2);
        let LiveEvent::Transcript(first) = &events[0] else {
            panic!("first run")
        };
        let LiveEvent::Transcript(second) = &events[1] else {
            panic!("second run")
        };
        assert_eq!(
            (&first.text, first.speaker, first.speech_final),
            (&"Good morning.".to_owned(), Some(4), false)
        );
        assert_eq!(
            (&second.text, second.speaker, second.speech_final),
            (&"Indeed!".to_owned(), Some(8), true)
        );
    }

    #[test]
    fn turn_speaker_and_stale_boundaries() {
        let mut turns = LiveTurns::default();
        let mut line = TranscriptLine {
            channel: 0,
            speaker: Some(2),
            text: "opening".into(),
            is_final: true,
            speech_final: false,
            start: 0.0,
            end: 1.4,
        };
        assert!(turns.push(&line).is_empty());
        assert_eq!(turns.finish(0, 1.0), None);
        line.speaker = Some(7);
        line.text = "reply".into();
        line.end = 2.0;
        assert_eq!(turns.push(&line), vec!["opening"]);
        line.text = "continues".into();
        line.end = 2.5;
        line.speech_final = true;
        assert_eq!(turns.push(&line), vec!["reply continues"]);
        assert_eq!(turns.finish(0, 2.5), None);
    }

    #[test]
    fn turn_bound_flushes_unicode_once() {
        let mut turns = LiveTurns::default();
        let line = TranscriptLine {
            channel: 1,
            speaker: None,
            text: "é".repeat(90),
            is_final: true,
            speech_final: false,
            start: 0.0,
            end: 1.0,
        };
        let mut complete = String::new();
        for _ in 0..350 {
            for text in turns.push(&line) {
                complete.push_str(&text);
            }
            assert!(turns.channels[1].text.len() < 32_000);
        }
        complete.push_str(&turns.finish(1, 1.0).unwrap_or_default());
        assert_eq!(complete.chars().filter(|c| *c == 'é').count(), 350 * 90);
    }

    #[test]
    fn parsed_results_and_empty_boundaries() {
        assert!(event_lines("{bad").is_empty());
        assert!(event_lines(r#"{"channel":{"alternatives":[{"transcript":"  "}]}}"#).is_empty());
        let events = event_lines(
            r#"{"channel_index":[1],"start":4.0,"duration":0.75,"is_final":true,"speech_final":true,"channel":{"alternatives":[{"transcript":"  hello  ","words":[{"speaker":3}]}]}}"#,
        );
        let LiveEvent::Transcript(line) = &events[0] else {
            panic!("line")
        };
        assert_eq!(
            (
                line.channel,
                line.speaker,
                line.text.as_str(),
                line.start,
                line.end
            ),
            (1, Some(3), "hello", 4.0, 4.75)
        );
        assert!(line.is_final && line.speech_final);
        assert_eq!(
            event_lines(r#"{"speech_final":true,"channel":{"alternatives":[{"transcript":""}]}}"#)
                .len(),
            1
        );
        assert_eq!(
            event_lines(r#"{"type":"SpeechStarted","channel":1}"#),
            vec![LiveEvent::SpeechStarted { channel: 1 }]
        );
        assert_eq!(
            event_lines(r#"{"type":"SpeechStarted","channel":[9]}"#),
            vec![LiveEvent::SpeechStarted { channel: 1 }]
        );
        assert_eq!(
            event_lines(r#"{"type":"UtteranceEnd"}"#),
            vec![LiveEvent::UtteranceEnd {
                channel: 0,
                last_word_end: 0.0
            }]
        );
        assert_eq!(
            event_lines(r#"{"type":"Error","message":"oops"}"#),
            vec![LiveEvent::Failed("oops".into())]
        );
    }

    #[test]
    fn catalog_and_reply_paths() {
        for provider in crate::ai::providers::STT_PROVIDERS {
            assert!(!provider.name.is_empty());
            if provider.wire == SttWire::RawAudio {
                assert!(provider.endpoint.starts_with("http"));
            }
        }
        let deepgram = SttProvider::by_id("deepgram-stt").unwrap();
        let openai = SttProvider::by_id("openai-compatible-stt").unwrap();
        assert_eq!(
            extract_transcript(
                deepgram,
                &json!({"results":{"channels":[{"alternatives":[{"transcript":"  line  "}]}]}})
            ),
            Some("line".into())
        );
        assert_eq!(
            extract_transcript(
                deepgram,
                &json!({"results":{"channels":[{"alternatives":[{"transcript":"  "}]}]}})
            ),
            None
        );
        assert_eq!(
            extract_transcript(openai, &json!({"text":"answer"})),
            Some("answer".into())
        );
    }

    #[test]
    fn live_url_checks_model_and_region() {
        assert_eq!(validated_model("  ").unwrap(), "nova-3");
        assert_eq!(validated_model("  alpha_2.0 ").unwrap(), "alpha_2.0");
        assert!(validated_model("model/unsafe").is_err());
        assert!(validated_model(&"x".repeat(61)).is_err());
        let plain = live_url("", None, false).unwrap();
        assert!(
            plain.starts_with("wss://api.deepgram.com/v1/listen?model=nova-3&encoding=linear16")
        );
        assert!(!plain.contains("language="));
        assert!(
            !live_url("", Some("en"), false)
                .unwrap()
                .contains("language=")
        );
        assert!(
            live_url("", Some("fr-CA"), true)
                .unwrap()
                .contains("api.eu.deepgram.com")
        );
        assert!(
            live_url("", Some("fr-CA"), true)
                .unwrap()
                .contains("&mip_opt_out=true&language=fr-CA")
        );
    }

    #[tokio::test]
    #[allow(clippy::result_large_err)] // the closure's Err type is fixed by tungstenite's callback
    async fn configured_stream_keeps_the_supplied_host() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_hdr_async(
                tcp,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    assert_eq!(request.uri().path(), "/listen");
                    assert!(request.uri().query().unwrap_or("").contains("model=nova-3"));
                    Ok(response)
                },
            )
            .await
            .unwrap();
            drop(ws);
        });
        let mut config = ProviderConfig::new("deepgram-stt", "sample-key", "");
        config.base_url = format!("http://{address}/listen?tenant=two");
        let live = LiveTranscriber::connect_config_with(&config, SttHints::default())
            .await
            .unwrap();
        drop(live);
        server.await.unwrap();
    }

    #[test]
    fn batch_splits_long_speech_and_closes_on_quiet() {
        let mut chunks = BatchChunks::default();
        let mut pcm = vec![0_i16; 2 * 8_000];
        pcm.extend([12_000_i16, 0].repeat(243_200));
        let ready = chunks.push(&pcm);
        assert_eq!(ready.len(), 1);
        assert_eq!((ready[0].0, ready[0].1, ready[0].2), (0, 0.5, 15.5));
        let mut quiet = vec![0_i16; 2 * 9_600];
        quiet.extend([0_i16, 0].repeat(100));
        let closed = chunks.push(&quiet);
        assert_eq!(closed.len(), 1);
        assert!((closed[0].2 - 16.3).abs() < 0.001);
    }
    #[tokio::test]
    async fn openai_multipart_sends_model_wav_and_optional_bearer_key() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let count = socket.read(&mut buf).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buf[..count]);
                if let Some(split) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..split]);
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|s| s.parse().ok())
                        })
                        .unwrap();
                    if request.len() >= split + 4 + length {
                        break;
                    }
                }
            }
            let text = String::from_utf8_lossy(&request);
            assert!(text.starts_with("POST /v1/audio/transcriptions HTTP/1.1"));
            assert!(
                text.to_ascii_lowercase()
                    .contains("authorization: bearer test-key")
            );
            assert!(text.contains("name=\"model\"\r\n\r\nparakeet"));
            assert!(text.contains("name=\"language\"\r\n\r\nen"));
            assert!(text.contains("name=\"file\"; filename=\"audio.wav\""));
            assert!(text.contains("Content-Type: audio/wav"));
            assert!(text.contains("RIFFtest"));
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 16\r\nConnection: close\r\n\r\n{\"text\":\"hello\"}").unwrap();
        });
        let mut config = crate::ai::providers::ProviderConfig::new(
            "openai-compatible-stt",
            "test-key",
            "parakeet",
        );
        config.base_url = format!("http://127.0.0.1:{port}/v1///");
        config.language = "en".into();
        assert_eq!(
            super::transcribe(&config, b"RIFFtest".to_vec(), None)
                .await
                .unwrap(),
            "hello"
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn openai_multipart_rejects_query_and_fragment_in_base_url() {
        let mut config =
            crate::ai::providers::ProviderConfig::new("openai-compatible-stt", "", "parakeet");
        for suffix in ["?token=secret", "#fragment"] {
            config.base_url = format!("http://127.0.0.1:8000/v1{suffix}");
            let error = super::transcribe(&config, b"RIFFtest".to_vec(), None)
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("query string or fragment"),
                "{error}"
            );
        }
    }

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
}
