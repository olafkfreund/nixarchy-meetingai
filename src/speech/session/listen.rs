//! Dual-channel capture coordinator for live meeting transcription.
//!
//! Pairs microphone audio (left channel) with optional system monitor audio
//! (right channel) into 50 ms stereo frames at 16 kHz and routes them through
//! the optional [`EchoGate`].

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::ai::providers::ProviderConfig;
use crate::audio::echo_gate::{EchoGate, FRAME};
use crate::audio::microphone::{AudioError, Microphone};
use crate::audio::system_audio::{SystemAudio, SystemAudioError};
use crate::audio::{SAMPLE_RATE, rms};
use crate::speech::transcription::{self, TranscriptLine};

/// Maximum samples buffered per channel (2 seconds at 16 kHz).
const MAX_CHANNEL_SAMPLES: usize = (SAMPLE_RATE as usize) * 2;

/// Silence duration after which a capture channel is treated as quiet.
const QUIET_AFTER: Duration = Duration::from_millis(250);

/// Notice emitted when the user picked a microphone that bypasses echo cancellation.
const ECHO_CANCEL_NOTICE: &str = "Echo cancelling is off for the chosen microphone. Pick \"Meeting AI echo-cancelled mic\" in Settings → Audio to turn it on.";

/// Events produced by a [`ListenSession`] lifecycle step.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// Capture is active on the named device(s).
    Started { device: String },
    /// Interleaved 16 kHz stereo samples: microphone on left, system on right.
    Audio(Vec<f32>),
    /// A transcript segment from the speech engine.
    Transcript(TranscriptLine),
    /// Peak RMS loudness observed during the latest pump cycle.
    Level(f32),
    /// Capture has ended with the session's final elapsed duration.
    Stopped { elapsed: Duration },
    /// Non-fatal warning message shown to the user while capture continues.
    Notice(String),
    /// Fatal capture error shown to the user; capture has stopped.
    Failed(String),
}

/// Configuration for opening a [`ListenSession`].
#[derive(Debug, Clone, Default)]
pub struct SessionConfig {
    /// Which input to open; `None` lets the capture layer pick (echo-cancelled source first).
    pub device: Option<String>,
    /// Also record what the speakers play (the remote side) on the right channel.
    pub system_audio: bool,
    /// Monitor source identifier for system audio, or `None` for the default output.
    pub output_device: Option<String>,
    /// Speech provider settings carried for the daemon's live stream.
    pub speech: Option<ProviderConfig>,
    /// Suppress system audio leaking into an un-cancelled microphone.
    pub echo_gate: bool,
}

/// Pairs mono 16 kHz queues from microphone and system audio into stereo frames.
struct StereoAssembler {
    mic_queue: VecDeque<f32>,
    sys_queue: VecDeque<f32>,
    sys_enabled: bool,
}

impl StereoAssembler {
    fn new(sys_enabled: bool) -> Self {
        Self {
            mic_queue: VecDeque::with_capacity(MAX_CHANNEL_SAMPLES),
            sys_queue: VecDeque::with_capacity(MAX_CHANNEL_SAMPLES),
            sys_enabled,
        }
    }

    fn configure_system(&mut self, enabled: bool) {
        self.sys_enabled = enabled;
        self.sys_queue.clear();
    }

    fn ingest(&mut self, mic_samples: &[f32], sys_samples: &[f32]) -> bool {
        if self.mic_queue.len() + mic_samples.len() > MAX_CHANNEL_SAMPLES {
            return false;
        }
        if self.sys_enabled && self.sys_queue.len() + sys_samples.len() > MAX_CHANNEL_SAMPLES {
            return false;
        }
        self.mic_queue.extend(mic_samples.iter().copied());
        if self.sys_enabled {
            self.sys_queue.extend(sys_samples.iter().copied());
        }
        true
    }

    fn pull_live(&mut self, mic_quiet: bool, sys_quiet: bool) -> Option<Vec<f32>> {
        let mic_ready = self.mic_queue.len() >= FRAME || mic_quiet;
        let sys_ready = !self.sys_enabled || self.sys_queue.len() >= FRAME || sys_quiet;
        if !mic_ready || !sys_ready {
            return None;
        }
        let mut stereo = Vec::with_capacity(FRAME * 2);
        for _ in 0..FRAME {
            let left = self.mic_queue.pop_front().unwrap_or(0.0);
            let right = if self.sys_enabled {
                self.sys_queue.pop_front().unwrap_or(0.0)
            } else {
                0.0
            };
            stereo.push(left);
            stereo.push(right);
        }
        Some(stereo)
    }

    fn pull_finishing(&mut self) -> Option<Vec<f32>> {
        let sys_len = if self.sys_enabled {
            self.sys_queue.len()
        } else {
            0
        };
        let pairs = self.mic_queue.len().max(sys_len).min(FRAME);
        if pairs == 0 {
            return None;
        }
        let mut stereo = Vec::with_capacity(pairs * 2);
        for _ in 0..pairs {
            let left = self.mic_queue.pop_front().unwrap_or(0.0);
            let right = if self.sys_enabled {
                self.sys_queue.pop_front().unwrap_or(0.0)
            } else {
                0.0
            };
            stereo.push(left);
            stereo.push(right);
        }
        Some(stereo)
    }
}

fn describe_mic_error(err: &AudioError) -> String {
    match err {
        AudioError::NoDevice => {
            "No audio input device was detected. Plug in a microphone or choose one under Settings → Audio."
                .to_owned()
        }
        AudioError::DeviceMissing(name) => format!(
            "Microphone '{name}' is not available. Select another input device in Settings → Audio."
        ),
        AudioError::Config(detail) => format!(
            "Could not configure the selected microphone ({detail}). Try a different input in Settings → Audio."
        ),
        AudioError::Format(detail) => format!(
            "This microphone delivers audio in a format that cannot be used ({detail}). Pick a different input under Settings → Audio."
        ),
        AudioError::Startup(detail) => format!(
            "Could not start the microphone ({detail}). Check your input choice in Settings → Audio."
        ),
        AudioError::Stream(detail) => {
            format!("The microphone audio stream failed: {detail}.")
        }
        AudioError::NotRunning => "The microphone is not currently capturing audio.".to_owned(),
    }
}

fn format_combined_device(mic_name: &str, sys_source: &str) -> String {
    format!("{mic_name} + {sys_source}")
}

fn format_mic_only_notice(err: &SystemAudioError) -> String {
    format!(
        "Only your microphone is being recorded: the speaker output could not be opened ({err})."
    )
}

fn format_interrupted_failure() -> String {
    "Audio capture lost data, so the recording was stopped. Start a new recording to continue."
        .to_owned()
}

fn format_mic_stopped(reason: Option<&str>) -> String {
    match reason {
        Some(detail) if !detail.is_empty() => {
            format!("Microphone capture stopped unexpectedly: {detail}")
        }
        _ => "Microphone capture stopped unexpectedly.".to_owned(),
    }
}

/// Active audio capture session driven by periodic [`ListenSession::pump`] calls.
pub struct ListenSession {
    mic: Option<Microphone>,
    sys: Option<SystemAudio>,
    output_device: Option<String>,
    assembler: StereoAssembler,
    echo_gate: Option<EchoGate>,
    pending_audio: Option<(Vec<f32>, Vec<f32>)>,
    started_at: Instant,
    elapsed: Duration,
    emitted_samples: usize,
    last_mic_delivery: Instant,
    last_sys_delivery: Instant,
}

impl ListenSession {
    /// Open the configured capture devices and start the session clock.
    pub fn start(config: &SessionConfig) -> (Self, Vec<SessionEvent>) {
        let mut events = Vec::new();
        let echo_gate = config.echo_gate.then(EchoGate::default);
        let mic = match Microphone::start(config.device.as_deref()) {
            Ok(handle) => {
                events.push(SessionEvent::Started {
                    device: handle.device_name().to_owned(),
                });
                if handle.echo_cancel_bypassed() {
                    events.push(SessionEvent::Notice(ECHO_CANCEL_NOTICE.to_owned()));
                }
                Some(handle)
            }
            Err(err) => {
                events.push(SessionEvent::Failed(describe_mic_error(&err)));
                None
            }
        };

        let sys = if let Some(ref mic_handle) = mic
            && config.system_audio
        {
            match SystemAudio::start(config.output_device.as_deref()) {
                Ok(sys_handle) => {
                    events.push(SessionEvent::Started {
                        device: format_combined_device(
                            mic_handle.device_name(),
                            sys_handle.source(),
                        ),
                    });
                    Some(sys_handle)
                }
                Err(err) => {
                    events.push(SessionEvent::Notice(format_mic_only_notice(&err)));
                    None
                }
            }
        } else {
            None
        };

        if let Some(ref mic_handle) = mic {
            let _ = mic_handle.take(usize::MAX);
        }
        if let Some(ref sys_handle) = sys {
            let _ = sys_handle.take(usize::MAX);
        }

        let now = Instant::now();
        let session = Self {
            assembler: StereoAssembler::new(sys.is_some()),
            mic,
            sys,
            output_device: config.output_device.clone(),
            echo_gate,
            pending_audio: None,
            started_at: now,
            elapsed: Duration::ZERO,
            emitted_samples: 0,
            last_mic_delivery: now,
            last_sys_delivery: now,
        };
        (session, events)
    }

    /// Enable or disable system audio capture while keeping the microphone running.
    pub fn set_system_audio(&mut self, enabled: bool) -> Vec<SessionEvent> {
        if !enabled {
            let mut events = Vec::new();
            if let Some(flushed) = self.flush_audio() {
                events.push(flushed);
            }
            self.assembler.configure_system(false);
            if let Some(gate) = &mut self.echo_gate {
                gate.reset();
            }
            self.sys = None;
            let device = self
                .mic
                .as_ref()
                .map(|m| m.device_name().to_owned())
                .unwrap_or_default();
            events.push(SessionEvent::Started { device });
            return events;
        }

        if self.sys.is_some() {
            return Vec::new();
        }

        match SystemAudio::start(self.output_device.as_deref()) {
            Ok(sys_handle) => {
                let _ = sys_handle.take(usize::MAX);
                let sys_name = sys_handle.source().to_owned();
                self.sys = Some(sys_handle);
                self.assembler.configure_system(true);
                self.last_sys_delivery = Instant::now();
                if let Some(gate) = &mut self.echo_gate {
                    gate.reset();
                }
                let mic_name = self
                    .mic
                    .as_ref()
                    .map(Microphone::device_name)
                    .unwrap_or_default();
                vec![SessionEvent::Started {
                    device: format_combined_device(mic_name, &sys_name),
                }]
            }
            Err(err) => vec![SessionEvent::Notice(format_mic_only_notice(&err))],
        }
    }

    /// True when a microphone handle is present and its capture worker is alive.
    pub fn is_capturing(&self) -> bool {
        self.mic.as_ref().is_some_and(Microphone::is_running)
    }

    /// Drain newly captured audio and emit ready 50 ms frames plus a meter reading.
    pub fn pump(&mut self) -> Vec<SessionEvent> {
        if self.mic.is_none() {
            return Vec::new();
        }

        let now = Instant::now();
        let current_elapsed = now.duration_since(self.started_at);
        let due_samples =
            (current_elapsed.as_micros() * u128::from(SAMPLE_RATE) / 1_000_000) as usize;

        let lagged = due_samples.saturating_sub(self.emitted_samples) > MAX_CHANNEL_SAMPLES;
        let mic_dropped = self.mic.as_ref().is_some_and(|m| m.dropped() > 0);
        let sys_failed = self
            .sys
            .as_ref()
            .is_some_and(|s| s.dropped() > 0 || !s.is_running());

        if lagged || mic_dropped || sys_failed {
            self.abort_captures();
            return vec![SessionEvent::Failed(format_interrupted_failure())];
        }

        let mic_samples = self
            .mic
            .as_ref()
            .map(|m| m.take(MAX_CHANNEL_SAMPLES))
            .unwrap_or_default();
        let sys_samples = self
            .sys
            .as_ref()
            .map(|s| s.take(MAX_CHANNEL_SAMPLES))
            .unwrap_or_default();

        if !self.assembler.ingest(&mic_samples, &sys_samples) {
            self.abort_captures();
            return vec![SessionEvent::Failed(format_interrupted_failure())];
        }

        if !mic_samples.is_empty() {
            self.last_mic_delivery = now;
        }
        if !sys_samples.is_empty() {
            self.last_sys_delivery = now;
        }

        let mic_quiet = now.duration_since(self.last_mic_delivery) >= QUIET_AFTER;
        let sys_quiet = now.duration_since(self.last_sys_delivery) >= QUIET_AFTER;

        let mut events = Vec::new();
        while self.emitted_samples + FRAME <= due_samples {
            let Some(frame) = self.assembler.pull_live(mic_quiet, sys_quiet) else {
                break;
            };
            self.emitted_samples += FRAME;
            if let Some(ready) =
                Self::queue_audio(&mut self.echo_gate, &mut self.pending_audio, frame)
            {
                events.push(SessionEvent::Audio(ready));
            }
        }

        let peak_rms = rms(&mic_samples).max(rms(&sys_samples));
        events.push(SessionEvent::Level(peak_rms));
        self.elapsed = current_elapsed;

        if self.mic.as_ref().is_some_and(|m| !m.is_running()) {
            let reason = self.mic.as_ref().and_then(Microphone::failure);
            if let Some(mic_handle) = self.mic.take() {
                mic_handle.stop();
            }
            events.push(SessionEvent::Failed(format_mic_stopped(reason.as_deref())));
        }

        events
    }

    /// Flush remaining buffered audio, stop capture handles, and emit `Stopped`.
    pub fn stop(&mut self) -> Vec<SessionEvent> {
        let mut events = self.pump();
        if let Some(mic_handle) = self.mic.take() {
            let rem_mic = mic_handle.take(MAX_CHANNEL_SAMPLES);
            let rem_sys = self
                .sys
                .as_ref()
                .map(|s| s.take(MAX_CHANNEL_SAMPLES))
                .unwrap_or_default();
            if !self.assembler.ingest(&rem_mic, &rem_sys) {
                events.push(SessionEvent::Failed(
                    "Remaining audio at the end of capture exceeded the buffer limit.".to_owned(),
                ));
            }
            while let Some(frame) = self.assembler.pull_finishing() {
                if let Some(ready) =
                    Self::queue_audio(&mut self.echo_gate, &mut self.pending_audio, frame)
                {
                    events.push(SessionEvent::Audio(ready));
                }
            }
            mic_handle.stop();
        }
        if let Some(flushed) = self.flush_audio() {
            events.push(flushed);
        }
        self.sys = None;
        self.assembler.configure_system(false);
        events.push(SessionEvent::Stopped {
            elapsed: self.elapsed,
        });
        events
    }

    fn abort_captures(&mut self) {
        if let Some(mic_handle) = self.mic.take() {
            mic_handle.stop();
        }
        self.sys = None;
        self.assembler.configure_system(false);
    }

    fn queue_audio(
        gate: &mut Option<EchoGate>,
        pending: &mut Option<(Vec<f32>, Vec<f32>)>,
        mut samples: Vec<f32>,
    ) -> Option<Vec<f32>> {
        let Some(gate) = gate else {
            return Some(samples);
        };
        let original = samples.clone();
        gate.process(&mut samples);
        let previous = pending.replace((samples, original));
        previous.map(|(mut gated, raw)| {
            if gate.protecting() {
                for (pair, original) in gated
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(raw.as_chunks::<2>().0.iter())
                {
                    pair[0] = original[0];
                }
            }
            gated
        })
    }

    fn flush_audio(&mut self) -> Option<SessionEvent> {
        self.pending_audio
            .take()
            .map(|(_, original)| SessionEvent::Audio(original))
    }
}

/// Decode a base64 WAV recording and transcribe it with the given provider.
pub async fn transcribe_utterance(
    config: &ProviderConfig,
    wav_base64: &str,
    language: Option<&str>,
) -> Result<String, String> {
    let wav_bytes = STANDARD
        .decode(wav_base64)
        .map_err(|err| format!("Could not decode base64 audio payload: {err}"))?;
    transcription::transcribe(config, wav_bytes, language)
        .await
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle_session(echo_gate: bool) -> ListenSession {
        let now = Instant::now();
        ListenSession {
            mic: None,
            sys: None,
            output_device: None,
            assembler: StereoAssembler::new(false),
            echo_gate: echo_gate.then(EchoGate::default),
            pending_audio: None,
            started_at: now,
            elapsed: Duration::ZERO,
            emitted_samples: 0,
            last_mic_delivery: now,
            last_sys_delivery: now,
        }
    }

    #[test]
    fn unaligned_channel_packets_keep_every_sample_in_order() {
        let mut assembler = StereoAssembler::new(true);
        let total = 2_150usize;
        let left_in: Vec<f32> = (0..total).map(|i| (i as f32) + 1.0).collect();
        let right_in: Vec<f32> = (0..total).map(|i| -((i as f32) + 1.0)).collect();

        let mut left_cursor = 0;
        let mut right_cursor = 0;
        let mut output = Vec::new();

        while left_cursor < total || right_cursor < total {
            let next_left = (left_cursor + 317).min(total);
            let next_right = (right_cursor + 503).min(total);
            assert!(assembler.ingest(
                &left_in[left_cursor..next_left],
                &right_in[right_cursor..next_right],
            ));
            left_cursor = next_left;
            right_cursor = next_right;

            while let Some(frame) = assembler.pull_live(false, false) {
                output.extend(frame);
            }
        }
        while let Some(frame) = assembler.pull_finishing() {
            output.extend(frame);
        }

        let expected: Vec<f32> = left_in
            .iter()
            .zip(right_in.iter())
            .flat_map(|(&l, &r)| [l, r])
            .collect();
        assert_eq!(output, expected);
    }

    #[test]
    fn live_mode_waits_for_short_system_channel_and_finishing_pads_tail() {
        let mut assembler = StereoAssembler::new(true);
        assert!(assembler.ingest(&vec![0.12; 850], &vec![0.57; 400]));
        assert!(assembler.pull_live(false, false).is_none());

        assert!(assembler.ingest(&[], &vec![0.57; 430]));
        let full = assembler
            .pull_live(false, false)
            .expect("full frame should be emitted once system catches up");
        assert_eq!(full.len(), FRAME * 2);
        assert!(
            full.as_chunks::<2>()
                .0
                .iter()
                .all(|pair| pair == &[0.12, 0.57])
        );

        // 50 mic samples and 30 system samples remain: too short for live mode.
        assert!(assembler.pull_live(false, false).is_none());

        let tail = assembler
            .pull_finishing()
            .expect("finishing should emit padded tail");
        assert_eq!(tail.len(), 50 * 2);
        for (idx, pair) in tail.as_chunks::<2>().0.iter().enumerate() {
            assert_eq!(pair[0], 0.12);
            assert_eq!(pair[1], if idx < 30 { 0.57 } else { 0.0 });
        }
        assert!(assembler.pull_finishing().is_none());
    }

    #[test]
    fn quiet_system_channel_fills_right_with_zeros_until_active_again() {
        let mut assembler = StereoAssembler::new(true);
        assert!(assembler.ingest(&vec![0.21; FRAME], &[]));
        assert!(assembler.pull_live(false, false).is_none());

        let quiet_frame = assembler
            .pull_live(false, true)
            .expect("quiet system channel should allow mic frame out");
        assert_eq!(quiet_frame.len(), FRAME * 2);
        assert!(
            quiet_frame
                .as_chunks::<2>()
                .0
                .iter()
                .all(|pair| pair == &[0.21, 0.0])
        );

        assert!(assembler.ingest(&vec![0.66; FRAME], &vec![0.48; FRAME]));
        let resumed_frame = assembler
            .pull_live(false, false)
            .expect("both channels have a full frame");
        assert!(
            resumed_frame
                .as_chunks::<2>()
                .0
                .iter()
                .all(|pair| pair == &[0.66, 0.48])
        );
    }

    #[test]
    fn full_queue_rejects_addition_until_system_disabled() {
        let mut assembler = StereoAssembler::new(true);
        assert!(assembler.ingest(&vec![0.25; MAX_CHANNEL_SAMPLES], &vec![0.75; 120]));
        assert!(!assembler.ingest(&[0.5], &[]));

        assembler.configure_system(false);
        assert!(assembler.sys_queue.is_empty());

        let mut frames_pulled = 0;
        while let Some(frame) = assembler.pull_live(false, false) {
            assert_eq!(frame.len(), FRAME * 2);
            assert!(
                frame
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .all(|pair| pair == &[0.25, 0.0])
            );
            frames_pulled += 1;
        }
        assert_eq!(frames_pulled, MAX_CHANNEL_SAMPLES / FRAME);
    }

    #[test]
    fn disabled_echo_gate_returns_frame_without_buffering() {
        let mut gate = None;
        let mut pending = None;
        let input: Vec<f32> = (0..(FRAME * 2)).map(|i| (i as f32) * 0.001).collect();
        let out = ListenSession::queue_audio(&mut gate, &mut pending, input.clone());
        assert_eq!(out, Some(input));
        assert!(pending.is_none());
    }

    #[test]
    fn enabled_echo_gate_holds_one_frame_and_flushes_on_stop() {
        let mut session = idle_session(true);
        // Silent system channel: nothing for the gate to treat as echo, so frames pass unchanged.
        let first: Vec<f32> = (0..FRAME).flat_map(|_| [0.15, 0.0]).collect();
        let second: Vec<f32> = (0..FRAME).flat_map(|_| [0.35, 0.0]).collect();

        let first_out = ListenSession::queue_audio(
            &mut session.echo_gate,
            &mut session.pending_audio,
            first.clone(),
        );
        assert!(first_out.is_none());

        let second_out = ListenSession::queue_audio(
            &mut session.echo_gate,
            &mut session.pending_audio,
            second.clone(),
        );
        assert_eq!(second_out, Some(first));

        let flushed = session.flush_audio();
        assert_eq!(flushed, Some(SessionEvent::Audio(second)));
        assert!(session.flush_audio().is_none());
    }

    #[test]
    fn missing_device_id_fails_cleanly_and_names_device() {
        let missing_id = "cpal:id:nonexistent-device-id-987654";
        let config = SessionConfig {
            device: Some(missing_id.to_owned()),
            ..Default::default()
        };
        let (session, events) = ListenSession::start(&config);
        assert!(!session.is_capturing());
        let failure = events.iter().find_map(|event| match event {
            SessionEvent::Failed(text) => Some(text),
            _ => None,
        });
        let message = failure.expect("expected a Failed event");
        assert!(
            message.contains(missing_id),
            "unexpected message: {message}"
        );
    }

    #[test]
    fn stop_without_microphone_emits_stopped_and_is_idempotent() {
        let mut session = idle_session(false);
        assert_eq!(
            session.stop(),
            vec![SessionEvent::Stopped {
                elapsed: Duration::ZERO
            }]
        );
        assert_eq!(
            session.stop(),
            vec![SessionEvent::Stopped {
                elapsed: Duration::ZERO
            }]
        );
    }

    #[test]
    fn pump_without_microphone_returns_no_events() {
        let mut session = idle_session(false);
        assert!(session.pump().is_empty());
    }

    #[tokio::test]
    async fn transcribe_utterance_rejects_malformed_base64() {
        let config = ProviderConfig::new("deepgram-stt", "unused-key", "nova-3");
        let err = transcribe_utterance(&config, "***not-valid-base64***", None)
            .await
            .expect_err("malformed base64 must fail before calling provider");
        assert!(err.to_lowercase().contains("base64") || err.to_lowercase().contains("decode"));
    }
}
