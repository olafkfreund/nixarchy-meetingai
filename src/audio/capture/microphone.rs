//! Microphone capture through CPAL.
//!
//! Opens a real input device, downmixes to mono and resamples to 16 kHz, pushing
//! frames into a bounded buffer. A dedicated worker thread owns the stream, so a
//! blocked driver call cannot stall the UI.
//!
//! Startup failures are reported synchronously: [`Microphone::start`] only returns
//! once the stream is actually running, so "Listening" never appears over a device
//! that never opened.

use std::collections::VecDeque;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cpal::FromSample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio::{LinearResampler, SAMPLE_RATE, rms};

/// How long startup may take before it is treated as a failure.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Errors from microphone capture.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no microphone is available")]
    NoDevice,
    #[error("input device '{0}' is no longer available")]
    DeviceMissing(String),
    #[error("the microphone does not support a configurable input: {0}")]
    Config(String),
    #[error("unsupported sample format: {0}")]
    Format(String),
    #[error("microphone startup failed: {0}")]
    Startup(String),
    #[error("the microphone stream reported an error: {0}")]
    Stream(String),
    #[error("capture is not running")]
    NotRunning,
}

/// An input device the user can pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDevice {
    /// Value accepted by `Microphone::start`, prefixed with `cpal:`.
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Shared sample buffer between the audio callback and the consumer.
struct Buffer {
    samples: VecDeque<f32>,
    /// Hard cap so an unread buffer can never grow without bound.
    capacity: usize,
    /// Number of frames dropped because the consumer fell behind.
    dropped: u64,
    failure: Option<String>,
}

impl Buffer {
    fn push(&mut self, samples: impl Iterator<Item = f32>) {
        for sample in samples {
            if self.samples.len() >= self.capacity {
                self.samples.pop_front();
                self.dropped += 1;
            }
            self.samples.push_back(sample);
        }
    }

    fn take(&mut self, limit: usize) -> Vec<f32> {
        let count = limit.min(self.samples.len());
        self.samples.drain(..count).collect()
    }
}

/// A running microphone stream.
pub struct Microphone {
    buffer: Arc<Mutex<Buffer>>,
    stop: Option<std_mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
    device_name: String,
}

impl Microphone {
    /// Start capturing from the named device, or the system default.
    pub fn start(device: Option<&str>) -> Result<Self, AudioError> {
        Self::start_device(device, false)
    }

    /// WASAPI treats an output endpoint opened for input as loopback capture.
    #[cfg(windows)]
    pub(super) fn start_loopback(device: Option<&str>) -> Result<Self, AudioError> {
        Self::start_device(device, true)
    }

    fn start_device(device: Option<&str>, loopback: bool) -> Result<Self, AudioError> {
        let requested = device
            .map(str::trim)
            .filter(|name| !name.is_empty() && *name != "default")
            .map(str::to_owned);

        let buffer = Arc::new(Mutex::new(Buffer {
            samples: VecDeque::new(),
            // About two seconds at 16 kHz: enough headroom for a slow consumer
            // without letting a stall grow into a memory leak.
            capacity: (SAMPLE_RATE as usize) * 2,
            dropped: 0,
            failure: None,
        }));
        let sink = Arc::clone(&buffer);
        let (stop_tx, stop_rx) = std_mpsc::channel::<()>();
        let (ready_tx, ready_rx) = std_mpsc::sync_channel::<Result<String, AudioError>>(1);

        let worker = thread::spawn(move || {
            let outcome = run_capture(requested, loopback, sink, ready_tx.clone(), stop_rx);
            if let Err(error) = outcome {
                // If startup already succeeded, this is a runtime failure.
                let _ = ready_tx.try_send(Err(error));
            }
        });

        match ready_rx.recv_timeout(STARTUP_TIMEOUT) {
            Ok(Ok(device_name)) => Ok(Self {
                buffer,
                stop: Some(stop_tx),
                worker: Some(worker),
                device_name,
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => {
                let _ = stop_tx.send(());
                Err(AudioError::Startup(format!(
                    "the device did not start within {} seconds",
                    STARTUP_TIMEOUT.as_secs()
                )))
            }
        }
    }

    /// The device actually opened.
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Drain up to `limit` mono 16 kHz samples.
    pub fn take(&self, limit: usize) -> Vec<f32> {
        self.buffer
            .lock()
            .map(|mut buffer| buffer.take(limit))
            .unwrap_or_default()
    }

    /// Current level of the most recent audio, for meters.
    pub fn level(&self) -> f32 {
        self.buffer
            .lock()
            .map(|buffer| {
                let recent: Vec<f32> = buffer
                    .samples
                    .iter()
                    .rev()
                    .take(1600)
                    .copied()
                    .collect::<Vec<_>>();
                rms(&recent)
            })
            .unwrap_or(0.0)
    }

    /// Frames dropped because the consumer fell behind.
    pub fn dropped(&self) -> u64 {
        self.buffer
            .lock()
            .map(|buffer| buffer.dropped)
            .unwrap_or_default()
    }

    /// Whether the worker is still alive.
    pub fn is_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
            && self.failure().is_none()
    }

    pub fn failure(&self) -> Option<String> {
        self.buffer
            .lock()
            .ok()
            .and_then(|buffer| buffer.failure.clone())
    }

    /// Stop capturing and join the worker.
    pub fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Microphone {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_capture(
    requested: Option<String>,
    loopback: bool,
    sink: Arc<Mutex<Buffer>>,
    ready: std_mpsc::SyncSender<Result<String, AudioError>>,
    stop: std_mpsc::Receiver<()>,
) -> Result<(), AudioError> {
    let host = cpal::default_host();
    let device = match &requested {
        Some(name) => {
            let bare = name.strip_prefix("cpal:").unwrap_or(name);
            if let Some(id) = bare.strip_prefix("id:") {
                id.parse::<cpal::DeviceId>()
                    .ok()
                    .and_then(|id| host.device_by_id(&id))
                    .ok_or_else(|| AudioError::DeviceMissing(name.clone()))?
            } else {
                (if loopback {
                    host.output_devices()
                } else {
                    host.input_devices()
                })
                .map_err(|error| AudioError::Config(error.to_string()))?
                .find(|device| {
                    device
                        .description()
                        .is_ok_and(|description| description.name() == bare)
                })
                .ok_or_else(|| AudioError::DeviceMissing(bare.to_owned()))?
            }
        }
        None => (if loopback {
            host.default_output_device()
        } else {
            host.default_input_device()
        })
        .ok_or(AudioError::NoDevice)?,
    };
    let device_name = device
        .description()
        .map(|description| description.name().to_owned())
        .unwrap_or_else(|_| "Microphone".to_owned());

    let supported = (if loopback {
        device.default_output_config()
    } else {
        device.default_input_config()
    })
    .map_err(|error| AudioError::Config(error.to_string()))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels as usize;
    let input_rate = config.sample_rate;

    let stream = match format {
        cpal::SampleFormat::F32 => build::<f32>(device, config, channels, input_rate, sink),
        cpal::SampleFormat::I16 => build::<i16>(device, config, channels, input_rate, sink),
        cpal::SampleFormat::U16 => build::<u16>(device, config, channels, input_rate, sink),
        cpal::SampleFormat::I32 => build::<i32>(device, config, channels, input_rate, sink),
        other => return Err(AudioError::Format(other.to_string())),
    }?;

    stream
        .play()
        .map_err(|error| AudioError::Startup(error.to_string()))?;
    let _ = ready.send(Ok(device_name));

    // Park until asked to stop; dropping the stream closes the device.
    let _ = stop.recv();
    drop(stream);
    Ok(())
}

/// Build a stream for any supported sample type.
///
/// Callbacks arrive as interleaved frames; each is downmixed, resampled and
/// appended, so consumers always see 16 kHz mono.
fn build<T>(
    device: cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    input_rate: u32,
    sink: Arc<Mutex<Buffer>>,
) -> Result<cpal::Stream, AudioError>
where
    T: cpal::SizedSample + cpal::Sample,
    f32: cpal::FromSample<T>,
{
    let mut resampler = LinearResampler::new(input_rate);
    let mut mono = Vec::new();
    let mut resampled = Vec::new();

    let error_sink = sink.clone();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                mono.clear();
                mono.extend(data.chunks_exact(channels).map(|frame| {
                    frame
                        .iter()
                        .map(|sample| f32::from_sample_(*sample))
                        .sum::<f32>()
                        / channels as f32
                }));
                resampler.process_into(&mono, &mut resampled);
                if let Ok(mut buffer) = sink.lock() {
                    buffer.push(resampled.iter().copied());
                }
            },
            move |error| {
                // WASAPI reports recoverable discontinuities through this same
                // callback. They do not mean the capture stream has stopped.
                if matches!(
                    error.kind(),
                    cpal::ErrorKind::Xrun
                        | cpal::ErrorKind::DeviceChanged
                        | cpal::ErrorKind::RealtimeDenied
                ) {
                    tracing::warn!(%error, "audio capture recovered from a driver notification");
                    return;
                }
                tracing::error!(%error, "microphone stream error");
                if let Ok(mut buffer) = error_sink.lock() {
                    buffer.failure = Some(error.to_string());
                }
            },
            None::<Duration>,
        )
        .map_err(|error| AudioError::Startup(error.to_string()))
}

/// List input devices through CPAL.
pub fn input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|device| device.id().ok());
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let name = device.description().ok()?.name().to_owned();
            let id = device.id().ok()?;
            Some(InputDevice {
                is_default: default_id.as_ref() == Some(&id),
                id: format!("cpal:id:{id}"),
                name,
            })
        })
        .collect()
}

/// Whether any input device exists at all.
pub fn microphone_available() -> bool {
    cpal::default_host().default_input_device().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer() -> Buffer {
        Buffer {
            samples: VecDeque::new(),
            capacity: 8,
            dropped: 0,
            failure: None,
        }
    }

    #[test]
    fn buffer_drops_oldest_when_full() {
        let mut buffer = buffer();
        buffer.push((0..8).map(|i| i as f32));
        buffer.push(std::iter::once(99.0));
        assert_eq!(buffer.samples.len(), 8);
        assert_eq!(buffer.dropped, 1);
        // Oldest sample went first, newest is present.
        assert_eq!(buffer.samples.front().copied(), Some(1.0));
        assert_eq!(buffer.samples.back().copied(), Some(99.0));
    }

    #[test]
    fn take_limits_and_drains() {
        let mut buffer = buffer();
        buffer.push((0..5).map(|i| i as f32));
        assert_eq!(buffer.take(2), vec![0.0, 1.0]);
        assert_eq!(buffer.samples.len(), 3);
        assert_eq!(buffer.take(100).len(), 3);
        assert!(buffer.samples.is_empty());
    }

    #[test]
    fn missing_device_is_reported_by_name() {
        // A name that cannot exist: this must fail before any stream is built.
        let error = Microphone::start(Some("lennette-nonexistent-input-xyz"))
            .err()
            .expect("a missing device must not start");
        match error {
            AudioError::DeviceMissing(name) => {
                assert_eq!(name, "lennette-nonexistent-input-xyz")
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn device_listing_is_consistent() {
        let devices = input_devices();
        for device in &devices {
            assert!(device.id.starts_with("cpal:"));
            assert!(!device.name.is_empty());
        }
        // At most one default.
        assert!(devices.iter().filter(|device| device.is_default).count() <= 1);
    }

    #[test]
    fn availability_probe_never_panics() {
        let _ = microphone_available();
    }
}
