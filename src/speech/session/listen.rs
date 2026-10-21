//! A live listening session.
//!
//! Owns the microphone, the speech segmenter and (when configured) the streaming
//! transcriber. The UI drives it: `start`, poll `pump`, and `stop`.
//!
//! Everything that can fail — a missing device, a refused stream, a provider
//! error — is reported through [`SessionEvent`] so the overlay can show the real
//! reason instead of appearing to listen when nothing is recording.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::ai::providers::ProviderConfig;
use crate::audio::microphone::{AudioError, Microphone};
use crate::audio::{FlushReason, Segmenter, VadConfig, encode_wav};
use crate::speech::transcription::{self, TranscriptLine};

/// How much audio a single pump drains, in samples (about 100 ms at 16 kHz).
const FRAME_SAMPLES: usize = 1_600;

/// Something the session wants the UI to know about.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// Capture is running on this device.
    Started { device: String },
    /// A finished utterance, already encoded as WAV and ready to transcribe.
    Utterance {
        wav_base64: String,
        duration: f64,
        reason: FlushReason,
    },
    /// A transcript line from the streaming provider.
    Transcript(TranscriptLine),
    /// Measured microphone level, for a meter.
    Level(f32),
    /// Capture stopped.
    Stopped { elapsed: Duration },
    /// Informational message, e.g. a fallback to microphone only.
    Notice(String),
    /// Something went wrong; the UI shows this verbatim.
    Failed(String),
}

/// Configuration for a session.
#[derive(Debug, Clone, Default)]
pub struct SessionConfig {
    /// Device id (`cpal:<name>`), or `None` for the system default.
    pub device: Option<String>,
    /// Capture the far side of the call through a monitor source.
    pub system_audio: bool,
    /// Batch speech provider, used to transcribe finished utterances.
    pub speech: Option<ProviderConfig>,
}

/// A running capture session.
pub struct ListenSession {
    microphone: Option<Microphone>,
    system: Option<crate::audio::system_audio::SystemAudio>,
    segmenter: Segmenter,
    elapsed: Duration,
}

impl ListenSession {
    /// Open the microphone and begin capturing.
    pub fn start(config: &SessionConfig) -> (Self, Vec<SessionEvent>) {
        let device = config.device.as_deref();
        let mut events = Vec::new();

        let microphone = match Microphone::start(device) {
            Ok(microphone) => {
                events.push(SessionEvent::Started {
                    device: microphone.device_name().to_owned(),
                });
                Some(microphone)
            }
            Err(error) => {
                events.push(SessionEvent::Failed(describe(&error)));
                None
            }
        };

        // The far side of the call, when requested. A failure here is reported
        // but must not stop microphone capture.
        let system = if config.system_audio {
            match crate::audio::system_audio::SystemAudio::start(None) {
                Ok(system) => {
                    events.push(SessionEvent::Started {
                        device: format!("microphone + {}", system.source()),
                    });
                    Some(system)
                }
                Err(error) => {
                    events.push(SessionEvent::Notice(format!(
                        "Recording the microphone only: {error}"
                    )));
                    None
                }
            }
        } else {
            None
        };

        let session = Self {
            microphone,
            system,
            segmenter: Segmenter::new(VadConfig::default()),
            elapsed: Duration::ZERO,
        };
        (session, events)
    }

    /// Whether capture is actually running.
    pub fn is_capturing(&self) -> bool {
        self.microphone.as_ref().is_some_and(Microphone::is_running)
    }

    /// Time since the session began.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Drain pending audio and report what happened.
    ///
    /// Returns finished utterances and a level reading. This is cheap enough to
    /// call every frame: it only moves samples that are already buffered.
    pub fn pump(&mut self) -> Vec<SessionEvent> {
        let Some(microphone) = self.microphone.as_ref() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        let mut earned = Duration::ZERO;
        let mut level = 0.0_f32;

        loop {
            let frame = microphone.take(FRAME_SAMPLES);
            let remote = self
                .system
                .as_mut()
                .map(|system| system.take(FRAME_SAMPLES))
                .unwrap_or_default();
            if frame.is_empty() && remote.is_empty() {
                break;
            }
            // Both sides of the call feed one stream, so the transcriber hears
            // the whole conversation rather than only the user.
            let mixed = if remote.is_empty() {
                frame
            } else if frame.is_empty() {
                remote
            } else {
                crate::audio::system_audio::mix(&frame, &remote)
            };
            level = level.max(crate::audio::rms(&mixed));
            earned +=
                Duration::from_secs_f64(mixed.len() as f64 / f64::from(crate::audio::SAMPLE_RATE));
            if let Some(segment) = self.segmenter.push(&mixed) {
                events.push(utterance_event(&segment));
            }
        }

        self.elapsed += earned;
        events.push(SessionEvent::Level(level));

        if !microphone.is_running() {
            events.push(SessionEvent::Failed(
                microphone
                    .failure()
                    .map(|error| format!("The microphone stopped: {error}"))
                    .unwrap_or_else(|| {
                        "The microphone stopped unexpectedly. Check your audio device.".to_owned()
                    }),
            ));
            self.microphone = None;
        }
        events
    }

    /// Stop capturing and flush any pending speech.
    pub fn stop(&mut self) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        if let Some(microphone) = self.microphone.take() {
            // Drain anything still buffered before closing the device.
            loop {
                let frame = microphone.take(FRAME_SAMPLES);
                if frame.is_empty() {
                    break;
                }
                if let Some(segment) = self.segmenter.push(&frame) {
                    events.push(utterance_event(&segment));
                }
            }
            microphone.stop();
        }
        if let Some(system) = self.system.take() {
            let _ = system;
        }
        if let Some(segment) = self.segmenter.finish() {
            events.push(utterance_event(&segment));
        }
        events.push(SessionEvent::Stopped {
            elapsed: self.elapsed,
        });
        events
    }
}

fn utterance_event(segment: &crate::audio::Segment) -> SessionEvent {
    let duration = segment.samples.len() as f64 / f64::from(crate::audio::SAMPLE_RATE);
    let wav = encode_wav(&segment.samples).unwrap_or_default();
    SessionEvent::Utterance {
        wav_base64: STANDARD.encode(wav),
        duration,
        reason: segment.reason,
    }
}

fn describe(error: &AudioError) -> String {
    match error {
        AudioError::NoDevice => {
            "No microphone was found. Connect one, or pick a device in Settings → Audio.".to_owned()
        }
        AudioError::DeviceMissing(name) => {
            format!("'{name}' is no longer connected. Pick another device in Settings → Audio.")
        }
        AudioError::Config(message) => format!("The microphone rejected its settings: {message}"),
        AudioError::Format(format) => {
            format!("The microphone uses an unsupported format: {format}")
        }
        AudioError::Startup(message) => format!("The microphone could not start: {message}"),
        AudioError::Stream(message) => format!("The microphone reported an error: {message}"),
        AudioError::NotRunning => "Capture is not running.".to_owned(),
    }
}

/// Transcribe a finished utterance with the configured batch provider.
pub async fn transcribe_utterance(
    config: &ProviderConfig,
    wav_base64: &str,
    language: Option<&str>,
) -> Result<String, String> {
    let wav = STANDARD
        .decode(wav_base64.as_bytes())
        .map_err(|error| format!("could not decode audio: {error}"))?;
    transcription::transcribe(config, wav, language)
        .await
        .map_err(|error| error.to_string())
}

/// Build a consistent payload for the overlay's local-chunk event.
pub fn local_chunk_payload(wav_base64: &str, start: f64, duration: f64) -> serde_json::Value {
    serde_json::json!({
        "wavBase64": wav_base64,
        "start": start,
        "duration": duration,
    })
}

/// Shared handle for a session that the UI can start and stop.
pub type SharedSession = Arc<parking_lot::Mutex<Option<ListenSession>>>;

/// Create an empty shared session slot.
pub fn shared_session() -> SharedSession {
    Arc::new(parking_lot::Mutex::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_device_fails_with_a_readable_message() {
        let config = SessionConfig {
            device: Some("cpal:lennette-no-such-device".into()),
            system_audio: false,
            speech: None,
        };
        let (session, events) = ListenSession::start(&config);
        assert!(!session.is_capturing());
        let failure = events
            .iter()
            .find_map(|event| match event {
                SessionEvent::Failed(message) => Some(message.clone()),
                _ => None,
            })
            .expect("a missing device must report failure");
        assert!(failure.contains("no longer connected"), "{failure}");
    }

    #[test]
    fn stop_is_safe_without_a_microphone() {
        let config = SessionConfig {
            device: Some("cpal:lennette-no-such-device".into()),
            system_audio: false,
            speech: None,
        };
        let (mut session, _) = ListenSession::start(&config);
        let events = session.stop();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, SessionEvent::Stopped { .. }))
        );
        // Stopping twice must not panic.
        let again = session.stop();
        assert!(
            again
                .iter()
                .any(|event| matches!(event, SessionEvent::Stopped { .. }))
        );
    }

    #[test]
    fn pump_without_capture_is_quiet() {
        let config = SessionConfig {
            device: Some("cpal:lennette-no-such-device".into()),
            system_audio: false,
            speech: None,
        };
        let (mut session, _) = ListenSession::start(&config);
        assert!(session.pump().is_empty());
        assert_eq!(session.elapsed(), Duration::ZERO);
    }

    #[test]
    fn utterance_events_carry_real_wav() {
        let samples = vec![0.0f32; 16_000];
        let wav = encode_wav(&samples).unwrap();
        let encoded = STANDARD.encode(wav);
        let decoded = STANDARD.decode(encoded.as_bytes()).unwrap();
        assert_eq!(&decoded[0..4], b"RIFF");
    }

    #[test]
    fn chunk_payload_matches_the_frontend_contract() {
        let payload = local_chunk_payload("AAAA", 2.0, 1.5);
        assert_eq!(payload["wavBase64"], "AAAA");
        assert_eq!(payload["start"], 2.0);
        assert_eq!(payload["duration"], 1.5);
    }

    #[test]
    fn transcribing_invalid_audio_fails_cleanly() {
        let config = ProviderConfig::new("deepgram-stt", "key", "nova-3");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = runtime
            .block_on(transcribe_utterance(&config, "not-base64!!", None))
            .unwrap_err();
        assert!(error.contains("could not decode audio"));
    }
}
