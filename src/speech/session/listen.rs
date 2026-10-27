//! A live listening session.
//!
//! Owns microphone and call capture, preserving separate channels for the
//! live network transcriber. The UI drives `start`, `pump`, and `stop`.
//!
//! Everything that can fail — a missing device, a refused stream, a provider
//! error — is reported through [`SessionEvent`] so the overlay can show the real
//! reason instead of appearing to listen when nothing is recording.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::ai::providers::ProviderConfig;
use crate::audio::microphone::{AudioError, Microphone};

use crate::speech::transcription::{self, TranscriptLine};

/// How much audio a single pump drains, in samples (about 50 ms at 16 kHz).
const FRAME_SAMPLES: usize = 800;

/// Device callbacks have independent packet sizes. Retain incomplete frames;
/// padding a live underflow would insert gaps into otherwise continuous speech.
#[derive(Default)]
struct CaptureFrames {
    microphone: VecDeque<f32>,
    remote: VecDeque<f32>,
    remote_enabled: bool,
}

impl CaptureFrames {
    fn push(&mut self, microphone: &[f32], remote: &[f32]) -> bool {
        const LIMIT: usize = crate::audio::SAMPLE_RATE as usize * 2;
        if self.microphone.len() + microphone.len() > LIMIT
            || self.remote.len() + remote.len() > LIMIT
        {
            return false;
        }
        self.microphone.extend(microphone);
        if self.remote_enabled {
            self.remote.extend(remote);
        }
        true
    }

    fn next(&mut self, finishing: bool, quiet: [bool; 2]) -> Option<Vec<f32>> {
        let available = if !self.remote_enabled {
            self.microphone.len()
        } else if finishing {
            self.microphone.len().max(self.remote.len())
        } else {
            self.microphone.len().min(self.remote.len())
        };
        let frames = if finishing {
            if available == 0 {
                return None;
            }
            available.min(FRAME_SAMPLES)
        } else {
            if self.microphone.len() < FRAME_SAMPLES && !quiet[0]
                || self.remote_enabled && self.remote.len() < FRAME_SAMPLES && !quiet[1]
            {
                return None;
            }
            FRAME_SAMPLES
        };
        Some(
            (0..frames)
                .flat_map(|_| {
                    [
                        self.microphone.pop_front().unwrap_or(0.0),
                        self.remote.pop_front().unwrap_or(0.0),
                    ]
                })
                .collect(),
        )
    }

    fn set_remote(&mut self, enabled: bool) {
        self.remote.clear();
        self.remote_enabled = enabled;
    }
}

/// Something the session wants the UI to know about.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// Capture is running on this device.
    Started { device: String },
    /// Interleaved 16 kHz samples: microphone left, call audio right.
    Audio(Vec<f32>),
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
    pub output_device: Option<String>,
    /// Speech provider used by the live network task.
    pub speech: Option<ProviderConfig>,
}

/// A running capture session.
pub struct ListenSession {
    microphone: Option<Microphone>,
    system: Option<crate::audio::system_audio::SystemAudio>,
    started: std::time::Instant,
    sent_samples: usize,
    frames: CaptureFrames,
    last_audio: [std::time::Instant; 2],
    elapsed: Duration,
    output_device: Option<String>,
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
        let system = if config.system_audio && microphone.is_some() {
            match crate::audio::system_audio::SystemAudio::start(config.output_device.as_deref()) {
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

        if let Some(mic) = &microphone {
            mic.take(32_000);
        }
        let mut system = system;
        if let Some(sys) = &mut system {
            sys.take(32_000);
        }
        let remote_enabled = system.is_some();
        let session = Self {
            microphone,
            system,
            started: std::time::Instant::now(),
            sent_samples: 0,
            frames: CaptureFrames {
                remote_enabled,
                ..Default::default()
            },
            last_audio: [std::time::Instant::now(); 2],
            elapsed: Duration::ZERO,
            output_device: config.output_device.clone(),
        };
        (session, events)
    }

    /// Switch call capture without reconnecting or interrupting the microphone.
    pub fn set_system_audio(&mut self, enabled: bool) -> Vec<SessionEvent> {
        if !enabled {
            self.frames.set_remote(false);
            self.system.take();
            return vec![SessionEvent::Started {
                device: self
                    .microphone
                    .as_ref()
                    .map(|m| m.device_name().to_owned())
                    .unwrap_or_default(),
            }];
        }
        if self.system.is_some() {
            return Vec::new();
        }
        match crate::audio::system_audio::SystemAudio::start(self.output_device.as_deref()) {
            Ok(system) => {
                let device = format!("microphone + {}", system.source());
                self.frames.set_remote(true);
                self.last_audio[1] = std::time::Instant::now();
                self.system = Some(system);
                vec![SessionEvent::Started { device }]
            }
            Err(error) => vec![SessionEvent::Notice(format!(
                "Recording the microphone only: {error}"
            ))],
        }
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
    /// Returns raw interleaved PCM blocks and a level reading. This is cheap enough to
    /// call every frame: it only moves samples that are already buffered.
    pub fn pump(&mut self) -> Vec<SessionEvent> {
        let Some(microphone) = self.microphone.as_ref() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        let mut level = 0.0_f32;
        let due =
            (self.started.elapsed().as_secs_f64() * f64::from(crate::audio::SAMPLE_RATE)) as usize;
        if due.saturating_sub(self.sent_samples) > 32_000
            || microphone.dropped() > 0
            || self
                .system
                .as_ref()
                .is_some_and(|s| s.dropped() > 0 || !s.is_running())
        {
            self.microphone.take();
            self.system.take();
            events.push(SessionEvent::Failed(
                "Audio capture was interrupted or fell behind. Start again to reconnect.".into(),
            ));
            return events;
        }
        let frame = microphone.take(32_000);
        let remote = self
            .system
            .as_mut()
            .map(|s| s.take(32_000))
            .unwrap_or_default();
        let now = std::time::Instant::now();
        for (channel, samples) in [&frame, &remote].into_iter().enumerate() {
            if !samples.is_empty() {
                self.last_audio[channel] = now;
            }
        }
        // Wait through ordinary callback jitter. A loopback device may stop
        // callbacks during genuine silence; only then pad its absent packets.
        let quiet = self
            .last_audio
            .map(|at| now.duration_since(at) >= Duration::from_millis(250));
        level = level
            .max(crate::audio::rms(&frame))
            .max(crate::audio::rms(&remote));
        if !self.frames.push(&frame, &remote) {
            self.microphone.take();
            self.system.take();
            events.push(SessionEvent::Failed(
                "Audio capture was interrupted or fell behind. Start again to reconnect.".into(),
            ));
            return events;
        }
        while self.sent_samples + FRAME_SAMPLES <= due {
            let Some(samples) = self.frames.next(false, quiet) else {
                break;
            };
            self.sent_samples += samples.len() / 2;
            events.push(SessionEvent::Audio(samples));
        }
        self.elapsed = self.started.elapsed();
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
        let mut events = self.pump();
        if let Some(microphone) = self.microphone.take() {
            let frame = microphone.take(32_000);
            let remote = self
                .system
                .as_mut()
                .map(|s| s.take(32_000))
                .unwrap_or_default();
            if !self.frames.push(&frame, &remote) {
                events.push(SessionEvent::Failed(
                    "The final audio exceeded the capture buffer.".into(),
                ));
            }
            while let Some(samples) = self.frames.next(true, [false; 2]) {
                events.push(SessionEvent::Audio(samples));
            }
            microphone.stop();
        }
        self.system.take();
        events.push(SessionEvent::Stopped {
            elapsed: self.elapsed,
        });
        events
    }
}

/// Fixed channel order is the source of You/Them identity, independent of diarization.
pub fn interleave(microphone: &[f32], remote: &[f32], frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            [
                microphone.get(i).copied().unwrap_or(0.0),
                remote.get(i).copied().unwrap_or(0.0),
            ]
        })
        .collect()
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
    use crate::audio::encode_wav;

    #[test]
    fn uneven_device_packets_preserve_both_audio_tracks_without_silence_insertion() {
        let microphone: Vec<f32> = (1..=16_000).map(|i| i as f32 / 20_000.0).collect();
        let remote: Vec<f32> = microphone.iter().map(|s| -s).collect();
        let mut frames = CaptureFrames {
            remote_enabled: true,
            ..Default::default()
        };
        let mut actual = Vec::new();
        let mut remote_cursor = 0;
        // Microphone callbacks and parec pipe reads rarely share packet boundaries.
        for (tick, chunk) in microphone.chunks(640).enumerate() {
            let remote_end = (((tick + 1) * 640 / 1024) * 1024).min(remote.len());
            assert!(frames.push(chunk, &remote[remote_cursor..remote_end]));
            remote_cursor = remote_end;
            while let Some(frame) = frames.next(false, [false; 2]) {
                actual.extend(frame);
            }
        }
        assert!(frames.push(&[], &remote[remote_cursor..]));
        while let Some(frame) = frames.next(true, [false; 2]) {
            actual.extend(frame);
        }
        assert_eq!(actual, interleave(&microphone, &remote, microphone.len()));
    }

    #[test]
    fn live_underflow_waits_and_stop_pads_only_the_final_tail() {
        let mut frames = CaptureFrames {
            remote_enabled: true,
            ..Default::default()
        };
        assert!(frames.push(&vec![0.2; 800], &[0.7; 300]));
        assert!(frames.next(false, [false; 2]).is_none());
        assert!(frames.push(&[], &[0.8; 500]));
        assert_eq!(
            frames.next(false, [false; 2]).unwrap(),
            interleave(
                &vec![0.2; 800],
                &[vec![0.7; 300], vec![0.8; 500]].concat(),
                800
            )
        );
        assert!(frames.push(&[0.3; 10], &[0.9; 5]));
        assert!(frames.next(false, [false; 2]).is_none());
        assert_eq!(
            frames.next(true, [false; 2]).unwrap(),
            interleave(&[0.3; 10], &[0.9; 5], 10)
        );
        assert!(frames.next(true, [false; 2]).is_none());
    }

    #[test]
    fn silent_loopback_can_pause_callbacks_without_stalling_the_microphone() {
        let mut frames = CaptureFrames {
            remote_enabled: true,
            ..Default::default()
        };
        assert!(frames.push(&[0.2; 800], &[0.7; 300]));
        assert!(
            frames.next(false, [false; 2]).is_none(),
            "packet jitter must wait"
        );
        assert_eq!(
            frames.next(false, [false, true]).unwrap(),
            interleave(&[0.2; 800], &[0.7; 300], 800)
        );
        assert!(frames.push(&[0.4; 800], &[]));
        assert_eq!(
            frames.next(false, [false, true]).unwrap(),
            interleave(&[0.4; 800], &[], 800)
        );
        // Fresh audio resets the quiet grace, so the resumed words stay intact.
        assert!(frames.push(&[0.6; 800], &[0.9; 800]));
        assert_eq!(
            frames.next(false, [false; 2]).unwrap(),
            interleave(&[0.6; 800], &[0.9; 800], 800)
        );
    }

    #[test]
    fn stalled_capture_is_bounded_and_disabling_remote_releases_microphone() {
        let mut frames = CaptureFrames {
            remote_enabled: true,
            ..Default::default()
        };
        assert!(frames.push(&vec![0.4; 32_000], &[]));
        assert!(!frames.push(&[0.4], &[]));
        assert!(frames.next(false, [false; 2]).is_none());
        frames.set_remote(false);
        let frame = frames.next(false, [false; 2]).unwrap();
        assert_eq!(frame, interleave(&vec![0.4; 800], &[], 800));
        assert!(frames.remote.is_empty());
    }

    #[test]
    fn stereo_channels_never_mix_and_missing_audio_is_silent() {
        assert_eq!(
            interleave(&[0.1, 0.2], &[0.8], 3),
            vec![0.1, 0.8, 0.2, 0.0, 0.0, 0.0]
        );
        assert_eq!(interleave(&[], &[0.3], 1), vec![0.0, 0.3]);
    }

    #[test]
    fn missing_device_fails_with_a_readable_message() {
        let config = SessionConfig {
            device: Some("cpal:nixarchy-meetingai-no-such-device".into()),
            system_audio: false,
            output_device: None,
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
            device: Some("cpal:nixarchy-meetingai-no-such-device".into()),
            system_audio: false,
            output_device: None,
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
            device: Some("cpal:nixarchy-meetingai-no-such-device".into()),
            system_audio: false,
            output_device: None,
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
