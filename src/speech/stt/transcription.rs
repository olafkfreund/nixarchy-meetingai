use crate::speech::stt::filter::LineFilter;
fn deepgram_batch_url(endpoint: &str, model: &str, language: Option<&str>) -> Result<reqwest::Url, SttError> {
    let mut url = reqwest::Url::parse(endpoint) .map_err(|_| SttError::Protocol("invalid speech endpoint".into()))?;
    let eu = url.host_str() == Some("api.eu.deepgram.com");
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("model", model)
            .append_pair("diarize", "true")
            .append_pair("smart_format", "true")
            .append_pair("punctuate", "true");
        if let Some(language) = language.filter(|v| !v.is_empty()) {
            query.append_pair("language", language); }
        if eu {
            query.append_pair("mip_opt_out", "true"); } }
    Ok(url) }
/// Optional hints for batch providers; the daemon fills `glossary` from the profile.
#[derive(Debug, Clone, Default)]
pub struct SttHints {
    /// Names and terms (profile and meeting context) that the engine should spell right (OpenAI-compatible prompt).
    pub glossary: String, }
/// Channel, start, end (seconds) and samples of one utterance.
pub type BatchChunk = (u8, f64, f64, Vec<f32>);
/// A sample above this level counts as sound (about -42 dBFS).
const SOUND_LEVEL: f32 = 0.008;
/// 0.2 s at 16 kHz.
const MIN_SOUND_SAMPLES: usize = 3_200;
/// Transcribe one chunk of samples (language from the config).
pub async fn transcribe_chunk(config: &ProviderConfig, samples: &[f32], prompt: &str) -> Result<String, SttError> {
    transcribe_with_prompt(config, to_wav(samples)?, None, prompt).await }
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
        .map_err(SttError::Protocol) }
async fn transcribe_openai(config: &ProviderConfig, provider: &SttProvider, wav: Vec<u8>, language: Option<&str>, prompt: &str) -> Result<String, SttError> {
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
    if let Some(language) =
        language.filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
    {
        form = form.text("language", language.to_owned());
    }
    let mut request = crate::core::network::speech_client().post(endpoint).multipart(form);
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
    Ok(extract_transcript(provider, &value).unwrap_or_default()) }
#[cfg(test)] mod tests { use super::*;
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
    } }
