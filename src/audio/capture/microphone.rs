//! Opens a microphone through Pulse or CPAL and stores short 16 kHz mono chunks.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
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

/// Bounded FIFO of mono samples; the oldest ones are evicted when it is full.
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

    /// Append `samples`, evicting just enough of the oldest data to stay in bounds.
    pub(crate) fn push(&mut self, samples: &[f32]) {
        let len = self.data.len();
        let n = samples.len();
        let overflow = (len + n).saturating_sub(self.capacity);
        self.data.drain(..overflow.min(len));
        self.dropped += overflow as u64;
        self.data.extend(&samples[n - n.min(self.capacity)..]);
    }

    pub(crate) fn take(&mut self, limit: usize) -> Vec<f32> {
        self.data.drain(..limit.min(self.data.len())).collect()
    }

    /// `take` through a shared queue; a poisoned lock yields nothing.
    pub(crate) fn take_from(queue: &Mutex<Self>, limit: usize) -> Vec<f32> {
        queue
            .lock()
            .map(|mut queue| queue.take(limit))
            .unwrap_or_default()
    }

    pub(crate) fn dropped(&self) -> u64 {
        self.dropped
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// How long `start` waits for the CPAL worker to report before giving up.
const OPEN_DEADLINE: Duration = Duration::from_secs(7);

/// Hand-over state between `start` and the thread that owns the CPAL stream.
enum Phase {
    Starting,
    /// The stream is playing on the named device.
    Live(String),
    Failed(AudioError),
    /// Set by the owner of the handle; the worker then drops the stream.
    Stop,
}

type Gate = Arc<(Mutex<Phase>, Condvar)>;

fn lock_phase(gate: &Gate) -> MutexGuard<'_, Phase> {
    gate.0.lock().unwrap_or_else(PoisonError::into_inner)
}

enum Capture {
    Pulse(SystemAudio),
    Cpal {
        gate: Gate,
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
                let gate: Gate = Arc::new((Mutex::new(Phase::Starting), Condvar::new()));
                let worker = {
                    let (gate, samples, failure) = (
                        Arc::clone(&gate),
                        Arc::clone(&samples),
                        Arc::clone(&failure),
                    );
                    thread::spawn(move || {
                        let opened = open_cpal(requested.as_deref(), samples, failure);
                        let mut phase = lock_phase(&gate);
                        if matches!(*phase, Phase::Stop) {
                            return; // start gave up; the stream is dropped here
                        }
                        match opened {
                            Ok((stream, name)) => {
                                *phase = Phase::Live(name);
                                gate.1.notify_all();
                                while !matches!(*phase, Phase::Stop) {
                                    phase =
                                        gate.1.wait(phase).unwrap_or_else(PoisonError::into_inner);
                                }
                                drop(phase);
                                drop(stream);
                            }
                            Err(error) => {
                                *phase = Phase::Failed(error);
                                gate.1.notify_all();
                            }
                        }
                    })
                };
                let name = {
                    let guard = lock_phase(&gate);
                    let (mut phase, _) = gate
                        .1
                        .wait_timeout_while(guard, OPEN_DEADLINE, |p| matches!(p, Phase::Starting))
                        .unwrap_or_else(PoisonError::into_inner);
                    match std::mem::replace(&mut *phase, Phase::Stop) {
                        Phase::Live(name) => {
                            *phase = Phase::Live(name.clone());
                            name
                        }
                        Phase::Failed(error) => {
                            drop(phase);
                            let _ = worker.join();
                            return Err(error);
                        }
                        _ => {
                            drop(phase);
                            gate.1.notify_all();
                            return Err(AudioError::Startup(
                                "the input stream did not report within 7 seconds".into(),
                            ));
                        }
                    }
                };
                Ok(Self {
                    capture: Capture::Cpal {
                        gate,
                        worker: Some(worker),
                        samples,
                        failure,
                    },
                    name,
                    bypassed,
                })
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
            Capture::Cpal { samples, .. } => SampleQueue::take_from(samples, limit),
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
                gate,
                worker,
                failure,
                ..
            } => {
                let live = matches!(*lock_phase(gate), Phase::Live(_));
                let thread_up = worker.as_ref().is_some_and(|w| !w.is_finished());
                live && thread_up && failure.lock().is_ok_and(|slot| slot.is_none())
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
        if let Capture::Cpal { gate, worker, .. } = &mut self.capture {
            *lock_phase(gate) = Phase::Stop;
            gate.1.notify_all();
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
    }
}

/// The input device whose description name is exactly `wanted`, if any.
fn input_named(host: &cpal::Host, wanted: &str) -> Result<Option<cpal::Device>, AudioError> {
    let devices = host
        .input_devices()
        .map_err(|error| AudioError::Config(error.to_string()))?;
    for device in devices {
        if let Ok(d) = device.description()
            && d.name() == wanted
        {
            return Ok(Some(device));
        }
    }
    Ok(None)
}

/// A requested device name without its optional `cpal:` prefix.
fn bare_device_name(requested: &str) -> &str {
    match requested.split_once(':') {
        Some(("cpal", rest)) => rest,
        _ => requested,
    }
}

/// Average each whole `ch`-sample frame of `input` into `mono` (a partial tail is dropped).
fn downmix<T: Copy>(input: &[T], ch: usize, convert: impl Fn(T) -> f32, mono: &mut Vec<f32>) {
    mono.clear();
    let inv_channels = 1.0 / ch as f32;
    let mut acc = 0.0;
    for (i, &sample) in input.iter().enumerate() {
        acc += convert(sample);
        if (i + 1) % ch == 0 {
            mono.push(acc * inv_channels);
            acc = 0.0;
        }
    }
}

fn open_cpal(
    requested: Option<&str>,
    samples: Arc<Mutex<SampleQueue>>,
    failure: Arc<Mutex<Option<String>>>,
) -> Result<(cpal::Stream, String), AudioError> {
    use cpal::SampleFormat::{F32, I16, I32, U16};
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
            let name = bare_device_name(name);
            input_named(&host, name)?.ok_or_else(|| AudioError::DeviceMissing(name.to_owned()))?
        }
    };
    let name = match device.description() {
        Ok(description) => description.name().to_owned(),
        Err(_) => "Input device (unnamed)".to_owned(),
    };
    let default_input = device
        .default_input_config()
        .map_err(|error| AudioError::Config(error.to_string()))?;
    let (format, stream_config) = (default_input.sample_format(), default_input.config());
    if !matches!(format, F32 | I16 | U16 | I32) {
        return Err(AudioError::Format(format.to_string()));
    }
    let channels = usize::from(stream_config.channels).max(1);
    let mut resampler = LinearResampler::new(stream_config.sample_rate);
    let mut frames = Vec::with_capacity(4096);
    let mut output = Vec::with_capacity(4096);
    let stream = device
        .build_input_stream_raw(
            stream_config,
            format,
            move |data, _| {
                let mixed = match data.sample_format() {
                    F32 => data
                        .as_slice::<f32>()
                        .map(|s| downmix(s, channels, |v| v, &mut frames)),
                    I16 => data
                        .as_slice::<i16>()
                        .map(|s| downmix(s, channels, |v| f32::from(v) / 32768.0, &mut frames)),
                    U16 => data.as_slice::<u16>().map(|s| {
                        downmix(
                            s,
                            channels,
                            |v| (f32::from(v) - 32768.0) / 32768.0,
                            &mut frames,
                        )
                    }),
                    I32 => data
                        .as_slice::<i32>()
                        .map(|s| downmix(s, channels, |v| v as f32 / 2147483648.0, &mut frames)),
                    _ => None,
                };
                if mixed.is_none() {
                    return;
                }
                resampler.process_into(&frames, &mut output);
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
        .map_err(|error| AudioError::Startup(error.to_string()))?;
    stream
        .play()
        .map_err(|error| AudioError::Startup(error.to_string()))?;
    Ok((stream, name))
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
        assert_eq!(queue.take(1), [3.0]);
        assert_eq!(queue.take(99), [4.0, 5.0, 6.0]);
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
    fn queue_push_over_existing_content_drops_exactly_the_overflow() {
        let mut queue = SampleQueue::new(5);
        queue.push(&[1.0, 2.0, 3.0]);
        queue.push(&[4.0, 5.0, 6.0, 7.0]);
        assert_eq!(queue.dropped(), 2);
        assert_eq!(queue.take(99), [3.0, 4.0, 5.0, 6.0, 7.0]);
        queue.push(&[0.0; 9]);
        assert_eq!(queue.dropped(), 6);
        assert_eq!(queue.take(99).len(), 5);
        let shared = Mutex::new(SampleQueue::new(3));
        shared.lock().unwrap().push(&[1.0, 2.0]);
        assert_eq!(SampleQueue::take_from(&shared, 1), [1.0]);
    }

    #[test]
    fn downmix_averages_whole_frames_and_drops_a_partial_tail() {
        let mut mono = vec![9.0];
        downmix(&[1.0_f32, 3.0, 2.0, 4.0, 8.0], 2, |v| v, &mut mono);
        assert_eq!(mono, [2.0, 3.0]);
        downmix(&[5_i16, 7], 1, f32::from, &mut mono);
        assert_eq!(mono, [5.0, 7.0]);
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
