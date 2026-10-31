//! Speech recognition over complete WAV files and live stereo audio.

use crate::ai::providers::{ProviderConfig, SttProvider, SttWire};
use crate::audio::{SAMPLE_RATE, encode_wav};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio::sync::mpsc::{self, error::SendTimeoutError};
use tokio::sync::watch;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{HeaderName, HeaderValue},
    protocol::WebSocketConfig,
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

/// The push error once the speech worker is gone; its own `Failed` event carries the reason.
pub const STREAM_ENDED: &str = "Speech stream ended; start recording again";

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

impl SttError {
    /// Drops the request URL and masks `key` in the messages that may echo it.
    fn scrubbed(self, key: &str) -> Self {
        match self {
            Self::Request(error) => Self::Request(error.without_url()),
            mut other => {
                if !key.is_empty()
                    && let Self::Status { message, .. } | Self::Protocol(message) = &mut other
                {
                    *message = message.replace(key, "[redacted]");
                }
                other
            }
        }
    }
}

/// Events produced by a live or batch recognition session.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveEvent {
    Transcript(TranscriptLine),
    SpeechStarted { channel: u8 },
    UtteranceEnd { channel: u8, last_word_end: f64 },
    Notice(String),
    Failed(String),
}

/// Pending text of one channel is closed as a turn once it reaches this many bytes.
const TURN_LIMIT: usize = 32_000;
/// A boundary this much (seconds) before the pending end is treated as stale.
const STALE: f64 = 0.010;

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
        let index = usize::from(line.channel != 0);
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
            if turn.text.len() >= TURN_LIMIT
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
        let index = usize::from(channel != 0);
        if self.channels[index].end - last_word_end > STALE {
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
    let language = match (language, config.language.as_str()) {
        (Some(chosen), _) => Some(chosen),
        (None, "") => None,
        (None, configured) => Some(configured),
    };
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
    result.map_err(|error| error.scrubbed(&config.api_key))
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
    let model = Some(config.model.trim())
        .filter(|m| !m.is_empty())
        .unwrap_or(provider.default_model);
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
/// JSON body of the `json-wav` request format.
#[derive(serde::Serialize)]
struct WavJson<'a> {
    audio: String,
    model: &'a str,
    language: Option<&'a str>,
}

/// How a custom provider wants the recording delivered.
enum CustomBody {
    Json,
    Multipart,
    Raw,
}
impl CustomBody {
    fn from_format(format: &str) -> Self {
        match format {
            "json-wav" => Self::Json,
            "multipart-wav" => Self::Multipart,
            _ => Self::Raw,
        }
    }
}

/// Appends one multipart/form-data part (RFC 7578 framing) to `out`.
fn form_part(
    out: &mut Vec<u8>,
    boundary: &str,
    disposition: &str,
    content_type: Option<&str>,
    data: &[u8],
) {
    out.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: {disposition}\r\n").as_bytes(),
    );
    if let Some(kind) = content_type {
        out.extend_from_slice(format!("Content-Type: {kind}\r\n").as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(data);
    out.extend_from_slice(b"\r\n");
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
        let pairs = [
            ("model", config.model.as_str()),
            ("language", language.unwrap_or("")),
        ];
        let wanted: Vec<_> = pairs.into_iter().filter(|(_, v)| !v.is_empty()).collect();
        if !wanted.is_empty() {
            url.query_pairs_mut().extend_pairs(wanted);
        }
    }
    let (content_type, bytes) = match CustomBody::from_format(&custom.request_format) {
        CustomBody::Json => {
            let body = WavJson {
                audio: base64::engine::general_purpose::STANDARD.encode(&wav),
                model: &config.model,
                language,
            };
            let bytes = serde_json::to_vec(&body).map_err(|e| SttError::Encode(e.to_string()))?;
            ("application/json".to_owned(), bytes)
        }
        CustomBody::Multipart => {
            let boundary = format!("nixarchy-meetingai-{}", uuid::Uuid::new_v4());
            let mut body = Vec::with_capacity(wav.len() + 512);
            form_part(
                &mut body,
                &boundary,
                "form-data; name=\"model\"",
                None,
                config.model.as_bytes(),
            );
            form_part(
                &mut body,
                &boundary,
                "form-data; name=\"file\"; filename=\"audio.wav\"",
                Some("audio/wav"),
                &wav,
            );
            body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
            (format!("multipart/form-data; boundary={boundary}"), body)
        }
        CustomBody::Raw => ("audio/wav".to_owned(), wav),
    };
    let mut request = crate::core::network::speech_client().request(method, url);
    if let Some((name, value)) = custom.header(&config.api_key) {
        request = request.header(name, value);
    }
    let reply = request
        .header("Content-Type", content_type)
        .body(bytes)
        .send()
        .await?;
    let text = response_text(reply).await?;
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

/// Quiet samples (0.6 s) that end an utterance.
const QUIET_CLOSE: usize = SAMPLE_RATE as usize * 3 / 5;
/// Samples (15 s) after which an utterance is cut even without a pause.
const LONGEST: usize = SAMPLE_RATE as usize * 15;
/// Shortest utterance (0.1 s) worth recognising.
const SHORTEST: usize = SAMPLE_RATE as usize / 10;

/// Middle of the quietest 100 ms window in the last 2 s of `samples`, or `samples.len()`
/// when no earlier window is strictly quieter than the last one.
fn quiet_cut(samples: &[f32]) -> usize {
    let rate = SAMPLE_RATE as usize;
    let window = rate / 10;
    let end = samples.len();
    if end < window {
        return end;
    }
    let energy = |at: usize| -> f32 { samples[at..at + window].iter().map(|s| s * s).sum() };
    let mut best = (energy(end - window), end);
    let mut at = end.saturating_sub(rate * 2);
    while at + window <= end {
        let e = energy(at);
        if e < best.0 {
            best = (e, at + window / 2);
        }
        at += window;
    }
    best.1
}

#[derive(Default)]
struct OpenChunk {
    start: f64,
    samples: Vec<f32>,
    sound: usize,
    quiet: usize,
}
impl OpenChunk {
    fn from_samples(start: f64, samples: Vec<f32>) -> Self {
        let mut chunk = Self {
            start,
            samples: Vec::with_capacity(samples.len()),
            sound: 0,
            quiet: 0,
        };
        for sample in samples {
            chunk.add(sample, sample.abs() > SOUND_LEVEL);
        }
        chunk
    }
    /// Keeps one more sample; true when the utterance has to end now.
    fn add(&mut self, value: f32, sounding: bool) -> bool {
        self.samples.push(value);
        if sounding {
            self.sound += 1;
            self.quiet = 0;
        } else {
            self.quiet += 1;
        }
        self.quiet >= QUIET_CLOSE || self.samples.len() >= LONGEST
    }
    /// Ends the utterance; clicks and breaths are dropped.
    fn close(self, channel: u8, end: f64) -> Option<BatchChunk> {
        (self.samples.len() >= SHORTEST && self.sound >= MIN_SOUND_SAMPLES).then_some((
            channel,
            self.start,
            end,
            self.samples,
        ))
    }
}

#[derive(Default)]
enum ChannelState {
    #[default]
    Idle,
    Speaking(OpenChunk),
}

/// Splits stereo PCM into recognisable utterances on each channel.
#[derive(Default)]
pub struct BatchChunks {
    frames: usize,
    lanes: [ChannelState; 2],
    opened: [bool; 2],
}
impl BatchChunks {
    pub fn push(&mut self, pcm: &[i16]) -> Vec<BatchChunk> {
        let pairs = pcm.len() / 2;
        let rate = f64::from(SAMPLE_RATE);
        let mut done = Vec::new();
        for channel in 0..2 {
            for (offset, &raw) in pcm.iter().skip(channel).step_by(2).take(pairs).enumerate() {
                let frame = self.frames + offset;
                let value = f32::from(raw) / 32767.0;
                let sounding = value.abs() > SOUND_LEVEL;
                let mut chunk = match std::mem::take(&mut self.lanes[channel]) {
                    ChannelState::Speaking(open) => open,
                    ChannelState::Idle if !sounding => continue,
                    ChannelState::Idle => {
                        self.opened[channel] = true;
                        OpenChunk {
                            start: frame as f64 / rate,
                            ..OpenChunk::default()
                        }
                    }
                };
                if chunk.add(value, sounding) {
                    let cut = if chunk.quiet >= QUIET_CLOSE {
                        chunk.samples.len()
                    } else {
                        quiet_cut(&chunk.samples)
                    };
                    if cut < chunk.samples.len() {
                        let tail = chunk.samples.split_off(cut);
                        let cut_time = (frame + 1 - tail.len()) as f64 / rate;
                        let next = OpenChunk::from_samples(cut_time, tail);
                        chunk.sound -= next.sound;
                        done.extend(chunk.close(channel as u8, cut_time));
                        self.lanes[channel] = ChannelState::Speaking(next);
                    } else {
                        done.extend(chunk.close(channel as u8, (frame + 1) as f64 / rate));
                    }
                } else {
                    self.lanes[channel] = ChannelState::Speaking(chunk);
                }
            }
        }
        self.frames += pairs;
        // Stable: on equal end times the microphone comes first.
        done.sort_by(|a, b| a.2.total_cmp(&b.2));
        done
    }
    pub fn finish(&mut self) -> Vec<BatchChunk> {
        let end = self.now();
        (0u8..2)
            .filter_map(
                |channel| match std::mem::take(&mut self.lanes[usize::from(channel)]) {
                    ChannelState::Speaking(open) => open.close(channel, end),
                    ChannelState::Idle => None,
                },
            )
            .collect()
    }
    fn now(&self) -> f64 {
        self.frames as f64 / f64::from(SAMPLE_RATE)
    }
    fn pending_system(&self) -> Option<f64> {
        match &self.lanes[1] {
            ChannelState::Speaking(open) => Some(open.start),
            ChannelState::Idle => None,
        }
    }
    fn open_system_end(&self) -> Option<f64> {
        matches!(self.lanes[1], ChannelState::Speaking(_)).then(|| self.now())
    }
    /// Which channels opened an utterance since the last call.
    fn take_opened(&mut self) -> [bool; 2] {
        std::mem::take(&mut self.opened)
    }
}

fn channel_number(value: Option<&Value>) -> u8 {
    let number = match value {
        Some(Value::Array(items)) => items.first().and_then(Value::as_u64),
        Some(other) => other.as_u64(),
        None => None,
    };
    number.map_or(0, |n| u8::from(n >= 1))
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
            let last_word_end = match value.get("last_word_end") {
                Some(Value::Number(n)) => n.as_f64().unwrap_or_default(),
                _ => 0.0,
            };
            return vec![LiveEvent::UtteranceEnd {
                channel: channel_number(value.get("channel")),
                last_word_end,
            }];
        }
        Some("Error") => {
            let reason = ["description", "message"]
                .iter()
                .find_map(|key| value[*key].as_str())
                .unwrap_or("Speech service reported an error; start recording again");
            return vec![LiveEvent::Failed(reason.to_owned())];
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

fn model_byte_ok(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.')
}
fn validated_model(model: &str) -> Result<&str, SttError> {
    let model = model.trim();
    if model.is_empty() {
        return Ok("nova-3");
    }
    if model.len() > 60 {
        return Err(SttError::Protocol(
            "Speech model name is longer than 60 characters".into(),
        ));
    }
    if let Some(bad) = model.bytes().find(|b| !model_byte_ok(*b)) {
        return Err(SttError::Protocol(format!(
            "Speech model name has an unsupported character (byte {bad:#04x})"
        )));
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
/// Events a consumer may fall behind by before the worker waits.
const EVENT_BACKLOG: usize = 96;

pub struct LiveTranscriber {
    pcm_tx: mpsc::Sender<Vec<u8>>,
    updates: mpsc::Receiver<LiveEvent>,
    finish_tx: watch::Sender<bool>,
    worker: tokio::task::JoinHandle<()>,
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
            let mut target = reqwest::Url::parse(&config.base_url)
                .map_err(|_| SttError::Protocol("Speech service URL is invalid".into()))?;
            let scheme = match target.scheme() {
                "https" => "wss",
                "http" => "ws",
                other => other,
            }
            .to_owned();
            target
                .set_scheme(&scheme)
                .map_err(|()| SttError::Protocol("Speech service URL cannot be streamed".into()))?;
            let live_query = generated.split_once('?').map_or("", |parts| parts.1);
            let joined = match target.query() {
                Some(existing) if !existing.is_empty() => format!("{existing}&{live_query}"),
                _ => live_query.to_owned(),
            };
            target.set_query(Some(&joined));
            target.into()
        };
        Self::connect_url(config.api_key.clone(), url).await
    }
    pub async fn connect_url(api_key: String, url: String) -> Result<Self, SttError> {
        if api_key.trim().is_empty() {
            return Err(SttError::MissingKey);
        }
        let auth = ("Authorization".to_owned(), format!("Token {api_key}"));
        Self::open_socket(&url, Some(auth)).await
    }
    async fn open_socket(url: &str, auth: Option<(String, String)>) -> Result<Self, SttError> {
        crate::core::install_tls_provider();
        let mut request = url
            .into_client_request()
            .map_err(|e| SttError::Socket(format!("Cannot prepare speech connection: {e}")))?;
        if let Some((name, value)) = auth {
            let name = HeaderName::try_from(name.as_str())
                .map_err(|_| SttError::Protocol("Invalid speech authentication header".into()))?;
            let value = HeaderValue::try_from(value.as_str()).map_err(|_| SttError::MissingKey)?;
            request.headers_mut().insert(name, value);
        }
        let limits = WebSocketConfig::default()
            .max_message_size(Some(WS_LIMIT))
            .max_frame_size(Some(WS_LIMIT));
        // Nagle off: small audio and control frames must not wait to fill a segment.
        let connecting = tokio_tungstenite::connect_async_with_config(request, Some(limits), true);
        tokio::select! {
            joined = connecting => match joined {
                Ok((socket, _)) => Ok(Self::spawn_socket(socket)),
                Err(e) => Err(SttError::Socket(format!("Could not connect to speech service: {e}"))),
            },
            () = tokio::time::sleep(Duration::from_secs(10)) => Err(SttError::Socket(
                "Speech connection timed out; start recording again".into(),
            )),
        }
    }
    fn spawn_socket(socket: Socket) -> Self {
        let (pcm_tx, pcm_rx) = mpsc::channel(32);
        let (notify, updates) = mpsc::channel(EVENT_BACKLOG);
        let (finish_tx, finish_rx) = watch::channel(false);
        let worker = tokio::spawn(socket_task(socket, pcm_rx, notify, finish_rx));
        Self {
            pcm_tx,
            updates,
            finish_tx,
            worker,
        }
    }
    fn connect_batch(config: ProviderConfig, hints: SttHints) -> Self {
        let (pcm_tx, pcm_rx) = mpsc::channel(32);
        let (notify, updates) = mpsc::channel(EVENT_BACKLOG);
        let (finish_tx, finish_rx) = watch::channel(false);
        let worker = tokio::spawn(batch_task(config, hints, pcm_rx, notify, finish_rx));
        Self {
            pcm_tx,
            updates,
            finish_tx,
            worker,
        }
    }
    pub async fn push(&self, samples: &[f32]) -> Result<(), SttError> {
        let bytes: Vec<u8> = samples
            .iter()
            .flat_map(|x| {
                let level = if x.is_finite() {
                    x.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                ((level * 32767.0) as i16).to_le_bytes()
            })
            .collect();
        match self.pcm_tx.send_timeout(bytes, SEND_LIMIT).await {
            Ok(()) => Ok(()),
            Err(SendTimeoutError::Timeout(_)) => Err(SttError::Socket(
                "Speech audio queue fell behind; start recording again".into(),
            )),
            Err(SendTimeoutError::Closed(_)) => Err(SttError::Socket(STREAM_ENDED.into())),
        }
    }
    pub async fn next_event(&mut self) -> Option<LiveEvent> {
        self.updates.recv().await
    }
    pub fn try_event(&mut self) -> Result<LiveEvent, mpsc::error::TryRecvError> {
        self.updates.try_recv()
    }
    pub fn finish(&self) {
        let _ = self.finish_tx.send(true);
    }
}
impl Drop for LiveTranscriber {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

/// Largest incoming WebSocket message or frame.
const WS_LIMIT: usize = 1024 * 1024;
/// Longest wait for a single send or for room in a queue.
const SEND_LIMIT: Duration = Duration::from_secs(1);
/// Pause between keep-alive messages.
const KEEPALIVE_EVERY: Duration = Duration::from_secs(3);
/// Time from the first stop request until the server must have closed.
const FINISH_LIMIT: Duration = Duration::from_secs(4);

/// Where the socket task is in its life.
#[derive(Clone, Copy)]
enum Phase {
    Streaming,
    /// Draining queued audio and sending the closing commands.
    Finalizing {
        deadline: Instant,
    },
    /// Waiting for the server to hang up.
    AwaitingClose {
        deadline: Instant,
    },
}

/// How the socket task ended.
enum End {
    Quiet,
    Fail(String),
}

async fn send_socket(socket: &mut Socket, message: Message) -> Result<(), String> {
    match tokio::time::timeout(SEND_LIMIT, socket.send(message)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(format!("Speech connection failed: {e}")),
        Err(_) => Err("Speech service stopped accepting audio; start recording again".to_owned()),
    }
}

/// Hands the events of one server message to the consumer, waiting for room until
/// `until` (a fresh one-second window per event when `None`).
async fn deliver(
    tx: &mpsc::Sender<LiveEvent>,
    text: &str,
    until: Option<Instant>,
) -> Result<(), End> {
    for event in event_lines(text) {
        let limit = until.unwrap_or_else(|| Instant::now() + SEND_LIMIT);
        match tokio::time::timeout_at(limit, tx.send(event)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Err(End::Quiet),
            Err(_) => {
                return Err(End::Fail(
                    "Speech event consumer fell behind; start recording again".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Resolves once finishing was requested (or the requesting side is gone).
async fn stop_requested(finish_rx: &mut watch::Receiver<bool>) {
    let _ = finish_rx.wait_for(|stop| *stop).await;
}

/// Sends what is still queued, then asks the server to finalise and close.
async fn finalize(socket: &mut Socket, pcm_rx: &mut mpsc::Receiver<Vec<u8>>) -> Result<(), String> {
    while let Ok(block) = pcm_rx.try_recv() {
        send_socket(socket, Message::Binary(block.into())).await?;
    }
    for command in [r#"{"type":"Finalize"}"#, r#"{"type":"CloseStream"}"#] {
        send_socket(socket, Message::Text(command.into())).await?;
    }
    Ok(())
}

async fn socket_task(
    mut socket: Socket,
    mut pcm_rx: mpsc::Receiver<Vec<u8>>,
    tx: mpsc::Sender<LiveEvent>,
    mut finish_rx: watch::Receiver<bool>,
) {
    let mut keepalive = tokio::time::interval_at(Instant::now() + KEEPALIVE_EVERY, KEEPALIVE_EVERY);
    let mut phase = Phase::Streaming;
    let end = loop {
        match phase {
            Phase::Streaming => tokio::select! {
                biased;
                message = socket.next() => match message {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(end) = deliver(&tx, &text, None).await {
                            break end;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break End::Fail("Speech service disconnected; start recording again".into());
                    }
                    Some(Err(e)) => break End::Fail(format!("Speech connection failed: {e}")),
                    Some(Ok(_)) => {}
                },
                // A dropped sender counts as a stop request too.
                () = stop_requested(&mut finish_rx) => {
                    phase = Phase::Finalizing { deadline: Instant::now() + FINISH_LIMIT };
                }
                _ = keepalive.tick() => {
                    let ping = Message::Text(r#"{"type":"KeepAlive"}"#.into());
                    if let Err(e) = send_socket(&mut socket, ping).await {
                        break End::Fail(e);
                    }
                }
                block = pcm_rx.recv() => match block {
                    Some(block) => {
                        if let Err(e) = send_socket(&mut socket, Message::Binary(block.into())).await {
                            break End::Fail(e);
                        }
                    }
                    None => phase = Phase::Finalizing { deadline: Instant::now() + FINISH_LIMIT },
                },
            },
            Phase::Finalizing { deadline } => {
                match tokio::time::timeout_at(deadline, finalize(&mut socket, &mut pcm_rx)).await {
                    Ok(Ok(())) => phase = Phase::AwaitingClose { deadline },
                    Ok(Err(e)) => break End::Fail(e),
                    Err(_) => {
                        break End::Fail(
                            "Speech finalisation timed out; start recording again".into(),
                        );
                    }
                }
            }
            Phase::AwaitingClose { deadline } => tokio::select! {
                biased;
                message = socket.next() => match message {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(end) = deliver(&tx, &text, Some(deadline)).await {
                            break end;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break End::Quiet,
                    Some(Err(e)) => break End::Fail(format!("Speech connection failed: {e}")),
                    Some(Ok(_)) => {}
                },
                () = tokio::time::sleep_until(deadline) => {
                    break End::Fail(
                        "Speech service did not close after finalisation; start recording again".into(),
                    );
                }
            },
        }
    };
    if let End::Fail(message) = end {
        let _ = tx
            .send_timeout(LiveEvent::Failed(message), SEND_LIMIT)
            .await;
        let _ = tokio::time::timeout(SEND_LIMIT, socket.close(None)).await;
    }
}

type Pending = futures_util::stream::FuturesOrdered<
    std::pin::Pin<
        Box<dyn std::future::Future<Output = (u8, f64, f64, Result<String, SttError>)> + Send>,
    >,
>;
/// Batch requests allowed to be outstanding at once.
const MAX_IN_FLIGHT: usize = 4;
/// Waiting audio (seconds) kept before the oldest chunk is dropped.
const MAX_WAITING_SECONDS: f64 = 120.0;
/// Continuous chunk errors remain non-fatal only below this span.
const MAX_ERROR_STREAK: Duration = Duration::from_secs(60);

fn lost_speech_notice(start: f64, end: f64) -> LiveEvent {
    LiveEvent::Notice(format!("lost speech for {start:.1}–{end:.1} s"))
}

/// The open system chunk's end, but only when no closed system chunk is still being transcribed:
/// a mic line that overlaps one must wait for its text before the echo check can judge it.
fn open_only_system_end(
    in_flight: &std::collections::VecDeque<f64>,
    waiting: &std::collections::VecDeque<BatchChunk>,
    chunks: &BatchChunks,
) -> Option<f64> {
    (in_flight.is_empty() && !waiting.iter().any(|c| c.0 == 1))
        .then(|| chunks.open_system_end())
        .flatten()
}

fn earliest_system(
    in_flight: &std::collections::VecDeque<f64>,
    waiting: &std::collections::VecDeque<BatchChunk>,
    chunks: &BatchChunks,
) -> Option<f64> {
    [
        in_flight.front().copied(),
        waiting.iter().find(|c| c.0 == 1).map(|c| c.1),
        chunks.pending_system(),
    ]
    .into_iter()
    .flatten()
    .reduce(f64::min)
}

async fn queue_chunks(
    pending: &mut Pending,
    waiting: &mut std::collections::VecDeque<BatchChunk>,
    system_requests: &mut std::collections::VecDeque<f64>,
    config: &ProviderConfig,
    filter: &LineFilter,
    glossary: &str,
    tx: &mpsc::Sender<LiveEvent>,
) {
    while pending.len() < MAX_IN_FLIGHT
        && let Some((channel, start, end, samples)) = waiting.pop_front()
    {
        if channel == 1 {
            system_requests.push_back(start);
        }
        let config = config.clone();
        let prompt = filter.prompt(channel, glossary);
        pending.push_back(Box::pin(async move {
            let first = transcribe_chunk(&config, &samples, &prompt).await;
            let result = match first {
                Err(SttError::NotConfigured | SttError::MissingKey) => first,
                Err(_) => transcribe_chunk(&config, &samples, &prompt).await,
                Ok(_) => first,
            };
            (channel, start, end, result)
        }));
    }
    let mut queued: f64 = waiting.iter().map(|(_, s, e, _)| e - s).sum();
    while queued > MAX_WAITING_SECONDS
        && let Some((_, start, end, _)) = waiting.pop_front()
    {
        queued -= end - start;
        let _ = tx.send(lost_speech_notice(start, end)).await;
    }
}

async fn emit_lines(tx: &mpsc::Sender<LiveEvent>, lines: Vec<TranscriptLine>) {
    for line in lines {
        let _ = tx.send(LiveEvent::Transcript(line)).await;
    }
}

async fn handle_chunk_result(
    (channel, start, end, result): (u8, f64, f64, Result<String, SttError>),
    system_requests: &mut std::collections::VecDeque<f64>,
    failing_since: &mut Option<(f64, Instant)>,
    filter: &mut LineFilter,
    tx: &mpsc::Sender<LiveEvent>,
    error: &mut Option<String>,
) -> bool {
    if channel == 1 {
        system_requests.pop_front();
    }
    match result {
        Ok(text) => {
            *failing_since = None;
            if !text.is_empty() {
                let line = TranscriptLine {
                    channel,
                    speaker: None,
                    text,
                    is_final: true,
                    speech_final: true,
                    start,
                    end,
                };
                emit_lines(tx, filter.accept(&line)).await;
            }
            true
        }
        Err(e @ (SttError::NotConfigured | SttError::MissingKey)) => {
            *error = Some(format!("{e} (lost audio {start:.1}s..{end:.1}s)"));
            false
        }
        Err(e) => {
            let (first_start, first_at) = *failing_since.get_or_insert((start, Instant::now()));
            if end - first_start >= MAX_ERROR_STREAK.as_secs_f64()
                || first_at.elapsed() >= MAX_ERROR_STREAK
            {
                *error = Some(format!("{e} (lost audio {start:.1}s..{end:.1}s)"));
                false
            } else {
                let _ = tx.send(lost_speech_notice(start, end)).await;
                true
            }
        }
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
    let mut waiting: std::collections::VecDeque<BatchChunk> = std::collections::VecDeque::new();
    let mut system_requests: std::collections::VecDeque<f64> = std::collections::VecDeque::new();
    let mut failing_since: Option<(f64, Instant)> = None;
    let mut error = None;
    loop {
        if (*stop.borrow() || input.is_closed()) && input.is_empty() {
            break;
        }
        tokio::select! {
            biased;
            result = pending.next(), if !pending.is_empty() => {
                if let Some(done) = result {
                    if !handle_chunk_result(done, &mut system_requests, &mut failing_since, &mut filter, &tx, &mut error).await {
                        break;
                    }
                    queue_chunks(&mut pending, &mut waiting, &mut system_requests, &config, &filter, &hints.glossary, &tx).await;
                    let earliest = earliest_system(&system_requests, &waiting, &chunks);
                    emit_lines(&tx, filter.tick(chunks.now(), earliest, open_only_system_end(&system_requests, &waiting, &chunks))).await;
                }
            }
            _ = stop.changed() => {},
            block = input.recv() => { let Some(block) = block else { break; }; {
                let pcm: Vec<i16> = block.as_chunks::<2>().0.iter().map(|pair| i16::from_le_bytes(*pair)).collect();
                waiting.extend(chunks.push(&pcm));
                for channel in (0u8..).zip(chunks.take_opened()).filter_map(|(c, opened)| opened.then_some(c)) { let _ = tx.send(LiveEvent::SpeechStarted { channel }).await; }
                queue_chunks(&mut pending, &mut waiting, &mut system_requests, &config, &filter, &hints.glossary, &tx).await;
                let earliest = earliest_system(&system_requests, &waiting, &chunks);
                emit_lines(&tx, filter.tick(chunks.now(), earliest, open_only_system_end(&system_requests, &waiting, &chunks))).await;
            } }
        }
    }
    if error.is_none() {
        waiting.extend(chunks.finish());
        queue_chunks(
            &mut pending,
            &mut waiting,
            &mut system_requests,
            &config,
            &filter,
            &hints.glossary,
            &tx,
        )
        .await;
        while let Some(done) = pending.next().await {
            if !handle_chunk_result(
                done,
                &mut system_requests,
                &mut failing_since,
                &mut filter,
                &tx,
                &mut error,
            )
            .await
            {
                break;
            }
            queue_chunks(
                &mut pending,
                &mut waiting,
                &mut system_requests,
                &config,
                &filter,
                &hints.glossary,
                &tx,
            )
            .await;
            let earliest = earliest_system(&system_requests, &waiting, &chunks);
            emit_lines(
                &tx,
                filter.tick(
                    chunks.now(),
                    earliest,
                    open_only_system_end(&system_requests, &waiting, &chunks),
                ),
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
    use serde_json::json;
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
        let piece = "日本".repeat(40);
        let line = TranscriptLine {
            channel: 1,
            speaker: None,
            text: piece.clone(),
            is_final: true,
            speech_final: false,
            start: 0.0,
            end: 1.0,
        };
        let mut complete = String::new();
        for _ in 0..400 {
            for text in turns.push(&line) {
                complete.push_str(&text);
            }
            assert!(turns.channels[1].text.len() < TURN_LIMIT);
        }
        complete.push_str(&turns.finish(1, 1.0).unwrap_or_default());
        // Turns are joined with single spaces; without them nothing may be lost or repeated.
        assert_eq!(complete.replace(' ', ""), piece.repeat(400));
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
        assert!(validated_model(&"q".repeat(64)).is_err());
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
    /// The fake Parakeet decoder is process-global: tests that install one run one at a time.
    static DECODER_TESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test]
    async fn parakeet_batch_keeps_channel_and_times_and_flushes_on_stop() {
        let _one_at_a_time = DECODER_TESTS.lock().await;
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

    /// Stopping while the request slots are full still transcribes every utterance:
    /// the final chunks wait for a slot instead of failing as "fell behind".
    #[tokio::test]
    async fn stop_with_full_request_slots_keeps_every_line() {
        let _one_at_a_time = DECODER_TESTS.lock().await;
        let mut said = 0;
        crate::speech::stt::parakeet::set_decoder(Box::new(move |_, samples| {
            std::thread::sleep(Duration::from_millis(150));
            let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            said += 1;
            if peak > 0.3 {
                format!("far side remark {said}")
            } else {
                format!("near side remark {said}")
            }
        }));
        let config = ProviderConfig::new("parakeet-local", "", "");
        let mut live = LiveTranscriber::connect_batch(config, SttHints::default());
        // Four closed mic utterances (0.4 s speech, 0.8 s quiet each), then speech still open
        // on both channels when recording stops.
        let mut audio = Vec::new();
        for _ in 0..4 {
            audio.extend((0..6_400).flat_map(|_| [0.2_f32, 0.0]));
            audio.extend((0..12_800).flat_map(|_| [0.0_f32, 0.0]));
        }
        audio.extend((0..6_400).flat_map(|_| [0.2_f32, 0.45]));
        live.push(&audio).await.unwrap();
        live.finish();
        let (mut lines, mut failed) = (Vec::new(), None);
        while let Some(event) = live.next_event().await {
            match event {
                LiveEvent::Transcript(line) => lines.push(line),
                LiveEvent::Failed(message) => failed = Some(message),
                _ => {}
            }
        }
        assert_eq!(failed, None);
        assert_eq!(
            lines.iter().filter(|l| l.channel == 0).count(),
            5,
            "{lines:?}"
        );
        assert_eq!(
            lines.iter().filter(|l| l.channel == 1).count(),
            1,
            "{lines:?}"
        );
    }

    /// One utterance the engine keeps failing on becomes a "lost speech" notice; the
    /// recording goes on and the other utterances still give lines.
    #[tokio::test]
    async fn a_failing_chunk_becomes_a_notice_and_the_rest_still_come_out() {
        let _one_at_a_time = DECODER_TESTS.lock().await;
        crate::speech::stt::parakeet::set_decoder(Box::new(|_, samples| {
            let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(!(0.25..0.35).contains(&peak), "fake engine failure");
            format!("line level {peak:.1}")
        }));
        let config = ProviderConfig::new("parakeet-local", "", "");
        let mut live = LiveTranscriber::connect_batch(config, SttHints::default());
        let mut audio = Vec::new();
        for level in [0.1_f32, 0.3, 0.2] {
            audio.extend((0..6_400).flat_map(|_| [level, 0.0]));
            audio.extend((0..12_800).flat_map(|_| [0.0_f32, 0.0]));
        }
        live.push(&audio).await.unwrap();
        live.finish();
        let (mut lines, mut notices, mut failed) = (Vec::new(), Vec::new(), None);
        while let Some(event) = live.next_event().await {
            match event {
                LiveEvent::Transcript(line) => lines.push(line.text),
                LiveEvent::Notice(message) => notices.push(message),
                LiveEvent::Failed(message) => failed = Some(message),
                _ => {}
            }
        }
        assert_eq!(failed, None);
        assert_eq!(lines, ["line level 0.1", "line level 0.2"]);
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(notices[0].contains("lost speech for 1.2"), "{notices:?}");
    }

    /// An utterance that runs into the 15 s limit is cut at the pause just before it,
    /// not through the word at exactly 15 s.
    #[test]
    fn long_speech_is_cut_at_the_pause_before_the_limit() {
        let mut tone: Vec<f32> = (0..16_000 * 20)
            .map(|i| 0.2 * (i as f32 * 0.1).sin().signum())
            .collect();
        tone[14 * 16_000..14 * 16_000 + 2_400].fill(0.0);
        let pcm: Vec<i16> = tone
            .iter()
            .flat_map(|v| [(v * f32::from(i16::MAX)) as i16, 0])
            .collect();
        let mut chunks = BatchChunks::default();
        let ready = chunks.push(&pcm);
        assert_eq!(ready.len(), 1);
        assert!((ready[0].2 - 14.05).abs() < 0.06, "cut at {}", ready[0].2);
        let rest = chunks.finish();
        assert_eq!(rest.len(), 1);
        assert!(
            (rest[0].1 - ready[0].2).abs() < 0.001,
            "{} vs {}",
            rest[0].1,
            ready[0].2
        );
    }

    /// A held mic line may skip ahead of the open system chunk only while no closed system
    /// chunk is still waiting for its text: that text is what the echo check compares against.
    #[test]
    fn open_system_end_is_withheld_while_system_text_is_in_flight() {
        let pcm: Vec<i16> = (0..16_000)
            .flat_map(|i| [0, (6_000.0 * (i as f32 * 0.1).sin().signum()) as i16])
            .collect();
        let mut chunks = BatchChunks::default();
        assert!(chunks.push(&pcm).is_empty());
        let none = std::collections::VecDeque::new();
        assert!(open_only_system_end(&none, &none_waiting(), &chunks).is_some());
        let in_flight = std::collections::VecDeque::from([0.0]);
        assert_eq!(
            open_only_system_end(&in_flight, &none_waiting(), &chunks),
            None
        );
    }

    fn none_waiting() -> std::collections::VecDeque<BatchChunk> {
        std::collections::VecDeque::new()
    }

    /// Ten utterances arriving faster than a slow engine answers all come out, in order:
    /// they wait for a request slot instead of ending the recording.
    #[tokio::test]
    async fn a_slow_engine_with_ten_waiting_chunks_keeps_every_line_in_order() {
        let _one_at_a_time = DECODER_TESTS.lock().await;
        let mut said = 0;
        crate::speech::stt::parakeet::set_decoder(Box::new(move |_, _| {
            std::thread::sleep(Duration::from_millis(50));
            said += 1;
            format!("remark number {said}")
        }));
        let config = ProviderConfig::new("parakeet-local", "", "");
        let mut live = LiveTranscriber::connect_batch(config, SttHints::default());
        let mut audio = Vec::new();
        for _ in 0..10 {
            audio.extend((0..6_400).flat_map(|_| [0.2_f32, 0.0]));
            audio.extend((0..12_800).flat_map(|_| [0.0_f32, 0.0]));
        }
        live.push(&audio).await.unwrap();
        live.finish();
        let (mut lines, mut failed) = (Vec::new(), None);
        while let Some(event) = live.next_event().await {
            match event {
                LiveEvent::Transcript(line) => lines.push(line.text),
                LiveEvent::Failed(message) => failed = Some(message),
                _ => {}
            }
        }
        assert_eq!(failed, None);
        let expected: Vec<String> = (1..=10).map(|n| format!("remark number {n}")).collect();
        assert_eq!(lines, expected);
    }

    /// The four-second finish deadline also covers draining queued audio into a sink
    /// that accepts it slowly (each send alone stays under its one-second limit).
    #[tokio::test]
    async fn finish_deadline_covers_draining_a_slow_sink() {
        use tokio::net::TcpSocket;
        let listening = TcpSocket::new_v4().unwrap();
        listening.set_recv_buffer_size(4096).unwrap();
        listening.bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listening.local_addr().unwrap();
        let listener = listening.listen(1).unwrap();
        let sink = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            while let Some(Ok(_)) = ws.next().await {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        });
        let dialing = TcpSocket::new_v4().unwrap();
        dialing.set_send_buffer_size(4096).unwrap();
        let tcp = dialing.connect(address).await.unwrap();
        let (socket, _) = tokio_tungstenite::client_async(
            format!("ws://{address}/"),
            tokio_tungstenite::MaybeTlsStream::Plain(tcp),
        )
        .await
        .unwrap();
        let mut live = LiveTranscriber::spawn_socket(socket);
        for _ in 0..24 {
            live.push(&vec![0.1; 8_192]).await.unwrap();
        }
        let began = Instant::now();
        live.finish();
        let mut failure = None;
        while let Some(event) = live.next_event().await {
            if let LiveEvent::Failed(message) = event {
                failure = Some(message);
            }
        }
        assert!(
            failure.is_some(),
            "a sink this slow cannot be drained in time"
        );
        assert!(
            began.elapsed() < Duration::from_millis(6_500),
            "{:?}",
            began.elapsed()
        );
        sink.abort();
    }
}
