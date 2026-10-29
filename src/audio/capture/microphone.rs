//! Opens a microphone through Pulse or CPAL and stores short 16 kHz mono chunks.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio::system_audio::SystemAudio;
use crate::audio::{LinearResampler, SAMPLE_RATE};

/// Errors that prevent microphone capture from starting or continuing.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no default microphone is available")]
    NoDevice,
    #[error("microphone device was not found: {0}")]
    DeviceMissing(String),
    #[error("microphone configuration failed: {0}")]
    Config(String),
    #[error("unsupported microphone sample format: {0}")]
    Format(String),
    #[error("microphone could not start: {0}")]
    Startup(String),
    #[error("microphone stream failed: {0}")]
    Stream(String),
    #[error("microphone capture is stopped")]
    NotRunning,
}

pub(crate) struct SampleQueue {
    data: VecDeque<f32>,
    capacity: usize,
    dropped: u64,
}

impl SampleQueue {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(capacity),
            capacity,
            dropped: 0,
        }
    }
    pub(crate) fn push(&mut self, samples: &[f32]) {
        for &sample in samples {
            if self.data.len() == self.capacity {
                self.data.pop_front();
                self.dropped += 1;
            }
            if self.capacity > 0 {
                self.data.push_back(sample);
            }
        }
    }
    pub(crate) fn take(&mut self, limit: usize) -> Vec<f32> {
        self.data.drain(..limit.min(self.data.len())).collect()
    }
    pub(crate) fn dropped(&self) -> u64 {
        self.dropped
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

enum Capture {
    Pulse(SystemAudio),
    Cpal {
        stop: mpsc::Sender<()>,
        worker: Option<JoinHandle<()>>,
        samples: Arc<Mutex<SampleQueue>>,
        failure: Arc<Mutex<Option<String>>>,
    },
}

/// A microphone handle whose capture resources close when dropped.
pub struct Microphone {
    capture: Capture,
    name: String,
    bypassed: bool,
}

impl Microphone {
    /// Open the saved device or choose an available default input.
    pub fn start(device: Option<&str>) -> Result<Self, AudioError> {
        let sources = pulse_source_names();
        let explicit = device
            .map(str::trim)
            .is_some_and(|name| !name.is_empty() && name != "default");
        match choose_backend(device, &sources) {
            Backend::Pulse(source) => {
                let (handle, name) = open_with_fallback(
                    &source,
                    |id| {
                        SystemAudio::start_input(id)
                            .map_err(|error| AudioError::Startup(error.to_string()))
                    },
                    SystemAudio::has_samples,
                    FIRST_SAMPLES_WAIT,
                )?;
                Ok(Self {
                    capture: Capture::Pulse(handle),
                    name,
                    bypassed: false,
                })
            }
            Backend::Cpal(requested) => {
                let bypassed = explicit && sources.iter().any(|name| name == ECHO_CANCEL_SOURCE);
                let samples = Arc::new(Mutex::new(SampleQueue::new(SAMPLE_RATE as usize * 2)));
                let failure = Arc::new(Mutex::new(None));
                let (stop, receiver) = mpsc::channel();
                let (report, startup) = mpsc::sync_channel(1);
                let queue = Arc::clone(&samples);
                let errors = Arc::clone(&failure);
                let worker = thread::spawn(move || {
                    let result = open_cpal(requested.as_deref(), queue, errors);
                    match result {
                        Ok((stream, name)) => {
                            if report.send(Ok(name)).is_ok() {
                                let _ = receiver.recv();
                            }
                            drop(stream);
                        }
                        Err(error) => {
                            let _ = report.send(Err(error));
                        }
                    }
                });
                match startup.recv_timeout(Duration::from_secs(5)) {
                    Ok(Ok(name)) => Ok(Self {
                        capture: Capture::Cpal {
                            stop,
                            worker: Some(worker),
                            samples,
                            failure,
                        },
                        name,
                        bypassed,
                    }),
                    Ok(Err(error)) => {
                        let _ = worker.join();
                        Err(error)
                    }
                    Err(_) => {
                        let _ = stop.send(());
                        Err(AudioError::Startup(
                            "microphone startup exceeded five seconds".into(),
                        ))
                    }
                }
            }
        }
    }

    /// Whether an explicit CPAL choice bypassed the virtual echo filter.
    pub fn echo_cancel_bypassed(&self) -> bool {
        self.bypassed
    }
    /// The source or device that was opened.
    pub fn device_name(&self) -> &str {
        &self.name
    }
    /// Drain up to `limit` oldest samples.
    pub fn take(&self, limit: usize) -> Vec<f32> {
        match &self.capture {
            Capture::Pulse(handle) => handle.take(limit),
            Capture::Cpal { samples, .. } => samples
                .lock()
                .map(|mut queue| queue.take(limit))
                .unwrap_or_default(),
        }
    }
    /// Count samples displaced by newer audio.
    pub fn dropped(&self) -> u64 {
        match &self.capture {
            Capture::Pulse(handle) => handle.dropped(),
            Capture::Cpal { samples, .. } => {
                samples.lock().map(|queue| queue.dropped()).unwrap_or(0)
            }
        }
    }
    /// Whether capture is active and has no fatal stream error.
    pub fn is_running(&self) -> bool {
        match &self.capture {
            Capture::Pulse(handle) => handle.is_running(),
            Capture::Cpal {
                worker, failure, ..
            } => {
                worker.as_ref().is_some_and(|worker| !worker.is_finished())
                    && failure.lock().is_ok_and(|error| error.is_none())
            }
        }
    }
    /// Return the fatal capture error, if any.
    pub fn failure(&self) -> Option<String> {
        match &self.capture {
            Capture::Pulse(handle) => handle.failure(),
            Capture::Cpal { failure, .. } => failure.lock().ok().and_then(|error| error.clone()),
        }
    }
    /// End capture and release its device.
    pub fn stop(self) {
        drop(self);
    }
}

impl Drop for Microphone {
    fn drop(&mut self) {
        if let Capture::Cpal { stop, worker, .. } = &mut self.capture {
            let _ = stop.send(());
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
    }
}

fn open_cpal(
    requested: Option<&str>,
    samples: Arc<Mutex<SampleQueue>>,
    failure: Arc<Mutex<Option<String>>>,
) -> Result<(cpal::Stream, String), AudioError> {
    let host = cpal::default_host();
    let device = match requested {
        None => host.default_input_device().ok_or(AudioError::NoDevice)?,
        Some(name) if name.starts_with("cpal:id:") => {
            let id = name[8..]
                .parse::<cpal::DeviceId>()
                .map_err(|_| AudioError::DeviceMissing(name.to_owned()))?;
            host.device_by_id(&id)
                .filter(DeviceTrait::supports_input)
                .ok_or_else(|| AudioError::DeviceMissing(name.to_owned()))?
        }
        Some(name) => {
            let name = name.strip_prefix("cpal:").unwrap_or(name);
            host.input_devices()
                .map_err(|error| AudioError::Config(error.to_string()))?
                .find(|device| {
                    device
                        .description()
                        .is_ok_and(|description| description.name() == name)
                })
                .ok_or_else(|| AudioError::DeviceMissing(name.to_owned()))?
        }
    };
    let name = device
        .description()
        .map(|description| description.name().to_owned())
        .unwrap_or_else(|_| "Unnamed microphone".into());
    let supported = device
        .default_input_config()
        .map_err(|error| AudioError::Config(error.to_string()))?;
    let format = supported.sample_format();
    let config = supported.config();
    let channels = usize::from(config.channels);
    let rate = config.sample_rate;
    let stream = match format {
        cpal::SampleFormat::F32 => build_stream(
            &device,
            config,
            channels,
            rate,
            samples,
            failure,
            |value: f32| value,
        ),
        cpal::SampleFormat::I16 => build_stream(
            &device,
            config,
            channels,
            rate,
            samples,
            failure,
            |value: i16| f32::from(value) / 32768.0,
        ),
        cpal::SampleFormat::U16 => build_stream(
            &device,
            config,
            channels,
            rate,
            samples,
            failure,
            |value: u16| (f32::from(value) - 32768.0) / 32768.0,
        ),
        cpal::SampleFormat::I32 => build_stream(
            &device,
            config,
            channels,
            rate,
            samples,
            failure,
            |value: i32| value as f32 / 2147483648.0,
        ),
        other => return Err(AudioError::Format(other.to_string())),
    }?;
    stream
        .play()
        .map_err(|error| AudioError::Startup(error.to_string()))?;
    Ok((stream, name))
}

fn build_stream<T: cpal::SizedSample + Copy + Send + 'static>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    rate: u32,
    samples: Arc<Mutex<SampleQueue>>,
    failure: Arc<Mutex<Option<String>>>,
    convert: fn(T) -> f32,
) -> Result<cpal::Stream, AudioError> {
    let mut resampler = LinearResampler::new(rate);
    let mut mono = Vec::with_capacity(4096);
    let mut output = Vec::with_capacity(4096);
    device
        .build_input_stream(
            config,
            move |input: &[T], _| {
                mono.clear();
                for frame in input.chunks_exact(channels.max(1)) {
                    mono.push(
                        frame.iter().map(|sample| convert(*sample)).sum::<f32>()
                            / channels.max(1) as f32,
                    );
                }
                resampler.process_into(&mono, &mut output);
                if let Ok(mut queue) = samples.lock() {
                    queue.push(&output);
                }
            },
            move |error| match error.kind() {
                cpal::ErrorKind::Xrun
                | cpal::ErrorKind::DeviceChanged
                | cpal::ErrorKind::RealtimeDenied => {
                    tracing::warn!("microphone notification: {error}")
                }
                _ => {
                    tracing::error!("microphone stream stopped: {error}");
                    if let Ok(mut slot) = failure.lock() {
                        *slot = Some(error.to_string());
                    }
                }
            },
            Some(Duration::from_secs(4)),
        )
        .map_err(|error| AudioError::Startup(error.to_string()))
}
/// The echo-cancelled virtual source written by the Home Manager module.
const ECHO_CANCEL_SOURCE: &str = "meetingai-echo-cancel-source";

/// Which capture path serves a requested input device.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Backend {
    /// CPAL, with the requested device (None = host default).
    Cpal(Option<String>),
    /// `parec` on a Pulse/PipeWire source name.
    Pulse(String),
}

/// Pick the capture path. `sources` are the non-monitor Pulse source names.
///
/// An explicit `cpal:` device keeps CPAL. Otherwise the echo-cancelled source is
/// the default when present, then the server's default source. With no Pulse
/// server (no sources) everything stays on CPAL, as before.
fn choose_backend(requested: Option<&str>, sources: &[String]) -> Backend {
    let requested = requested
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != "default");
    match requested {
        Some(name) if name.starts_with("cpal:") => Backend::Cpal(Some(name.to_owned())),
        // A bare name that is not a Pulse source is a legacy CPAL device name.
        Some(name) if sources.iter().any(|source| source == name) => {
            Backend::Pulse(name.to_owned())
        }
        Some(name) => Backend::Cpal(Some(name.to_owned())),
        None if sources.is_empty() => Backend::Cpal(None),
        None if sources.iter().any(|source| source == ECHO_CANCEL_SOURCE) => {
            Backend::Pulse(ECHO_CANCEL_SOURCE.to_owned())
        }
        None => Backend::Pulse("@DEFAULT_SOURCE@".to_owned()),
    }
}

/// How long a Pulse source may stay silent-of-data before falling back.
const FIRST_SAMPLES_WAIT: Duration = Duration::from_millis(500);

/// Open `source`; if it delivers nothing within `wait` (present but not driven),
/// reopen on `@DEFAULT_SOURCE@`. Returns the handle and the source really used.
fn open_with_fallback<T>(
    source: &str,
    open: impl Fn(&str) -> Result<T, AudioError>,
    has_samples: impl Fn(&T) -> bool,
    wait: Duration,
) -> Result<(T, String), AudioError> {
    const DEFAULT: &str = "@DEFAULT_SOURCE@";
    let first = open(source)?;
    let deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < deadline {
        if has_samples(&first) {
            return Ok((first, source.to_owned()));
        }
        thread::sleep(Duration::from_millis(20));
    }
    if has_samples(&first) || source == DEFAULT {
        return Ok((first, source.to_owned()));
    }
    tracing::warn!(
        source,
        "microphone source delivered no audio; using the default source"
    );
    drop(first);
    Ok((open(DEFAULT)?, DEFAULT.to_owned()))
}

/// Non-monitor Pulse source names, empty when there is no server.
fn pulse_source_names() -> Vec<String> {
    super::devices::pactl(&["--format=json", "list", "sources"])
        .ok()
        .and_then(|text| super::devices::parse_sources(&text).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|device| device.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_evicts_oldest_and_drains_in_order() {
        let mut queue = SampleQueue::new(4);
        queue.push(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(queue.dropped(), 2);
        assert!(queue.take(0).is_empty());
        assert_eq!(queue.take(2), [3.0, 4.0]);
        assert_eq!(queue.take(99), [5.0, 6.0]);
        assert!(queue.is_empty());
    }

    #[test]
    fn missing_bare_device_keeps_its_name() {
        let name = "Fictional Juniper Microphone 834";
        assert!(
            matches!(Microphone::start(Some(name)), Err(AudioError::DeviceMissing(found)) if found == name)
        );
    }

    #[test]
    fn handles_are_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Microphone>();
        assert_send::<SystemAudio>();
    }
    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn echo_cancel_source_is_the_default_when_present() {
        let sources = names(&["alsa_input.pci", ECHO_CANCEL_SOURCE]);
        assert_eq!(
            choose_backend(None, &sources),
            Backend::Pulse(ECHO_CANCEL_SOURCE.into())
        );
        assert_eq!(
            choose_backend(Some("default"), &sources),
            Backend::Pulse(ECHO_CANCEL_SOURCE.into())
        );
    }

    #[test]
    fn default_source_is_used_without_echo_cancel() {
        assert_eq!(
            choose_backend(None, &names(&["alsa_input.pci"])),
            Backend::Pulse("@DEFAULT_SOURCE@".into())
        );
    }

    #[test]
    fn no_pulse_server_stays_on_cpal() {
        assert_eq!(choose_backend(None, &[]), Backend::Cpal(None));
    }

    #[test]
    fn explicit_cpal_device_keeps_cpal() {
        let sources = names(&[ECHO_CANCEL_SOURCE]);
        assert_eq!(
            choose_backend(Some("cpal:id:alsa:x"), &sources),
            Backend::Cpal(Some("cpal:id:alsa:x".into()))
        );
    }

    #[test]
    fn named_source_is_pulse_and_unknown_bare_name_is_legacy_cpal() {
        let sources = names(&["alsa_input.usb", ECHO_CANCEL_SOURCE]);
        assert_eq!(
            choose_backend(Some("alsa_input.usb"), &sources),
            Backend::Pulse("alsa_input.usb".into())
        );
        assert_eq!(
            choose_backend(Some("Old Device"), &sources),
            Backend::Cpal(Some("Old Device".into()))
        );
    }

    #[test]
    fn silent_source_falls_back_to_the_default_source() {
        use std::cell::RefCell;
        let opened = RefCell::new(Vec::new());
        let (handle, used) = open_with_fallback(
            ECHO_CANCEL_SOURCE,
            |name| {
                opened.borrow_mut().push(name.to_owned());
                Ok(name.to_owned())
            },
            |name: &String| name == "@DEFAULT_SOURCE@",
            Duration::from_millis(60),
        )
        .unwrap();
        assert_eq!(
            (handle.as_str(), used.as_str()),
            ("@DEFAULT_SOURCE@", "@DEFAULT_SOURCE@")
        );
        assert_eq!(*opened.borrow(), [ECHO_CANCEL_SOURCE, "@DEFAULT_SOURCE@"]);
    }

    #[test]
    fn live_source_is_kept() {
        let (_, used) = open_with_fallback(
            ECHO_CANCEL_SOURCE,
            |name| Ok(name.to_owned()),
            |_: &String| true,
            Duration::from_millis(60),
        )
        .unwrap();
        assert_eq!(used, ECHO_CANCEL_SOURCE);
    }
}
