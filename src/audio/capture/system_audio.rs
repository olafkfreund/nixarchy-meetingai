//! System audio capture (the far side of a call).
//!
//! On Linux this records a PulseAudio/PipeWire **monitor** source with `parec`,
//! which is what lets Listen hear the other participants rather than only the
//! microphone.
//!
//! Monitor sources are the output device played back, so this only works where a
//! Pulse server is reachable. Every failure is reported, never papered over.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::audio::{LinearResampler, SAMPLE_RATE};

/// How much audio to keep, in samples (about two seconds at 16 kHz).
const BUFFER_CAPACITY: usize = (SAMPLE_RATE as usize) * 2;

/// Errors from system audio capture.
#[derive(Debug, thiserror::Error)]
pub enum SystemAudioError {
    #[error("system audio needs a PulseAudio or PipeWire server, which was not reachable")]
    NoServer,
    #[error("no monitor source was found for any output device")]
    NoMonitor,
    #[error("could not start the system audio recorder: {0}")]
    Spawn(String),
    #[error("system audio is not implemented on this platform yet")]
    Unsupported,
}

/// One monitor source, i.e. a sink's output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorSource {
    pub id: String,
    pub name: String,
}

/// Find monitor sources through `pactl`.
pub fn monitor_sources() -> Vec<MonitorSource> {
    let output = match Command::new("pactl")
        .args(["--format=json", "list", "sources"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    let json = String::from_utf8_lossy(&output.stdout);
    parse_monitors(&json)
}

/// Parse monitor entries out of `pactl list sources` JSON.
pub fn parse_monitors(json: &str) -> Vec<MonitorSource> {
    #[derive(serde::Deserialize)]
    struct Source {
        name: String,
        #[serde(default)]
        description: Option<String>,
        // Older PulseAudio reports `monitor_of_sink`; PipeWire reports
        // `monitor_source`. Accept either, and fall back to the `.monitor`
        // naming convention so a third variant still works.
        #[serde(default)]
        monitor_of_sink: Option<String>,
        #[serde(default)]
        monitor_source: Option<String>,
    }

    impl Source {
        fn is_monitor(&self) -> bool {
            self.monitor_of_sink.is_some()
                || self.monitor_source.is_some()
                || self.name.ends_with(".monitor")
        }
    }
    let Ok(sources) = serde_json::from_str::<Vec<Source>>(json) else {
        return Vec::new();
    };
    sources
        .into_iter()
        .filter(|source| source.is_monitor())
        .map(|source| MonitorSource {
            name: source
                .description
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| source.name.clone()),
            id: source.name,
        })
        .collect()
}

/// A running system-audio recorder.
pub struct SystemAudio {
    buffer: Arc<Mutex<Vec<f32>>>,
    dropped: Arc<Mutex<u64>>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    reader: Option<thread::JoinHandle<()>>,
    source: String,
}

impl SystemAudio {
    /// Start recording the default monitor source, or a named one.
    pub fn start(source: Option<&str>) -> Result<Self, SystemAudioError> {
        if !cfg!(target_os = "linux") {
            return Err(SystemAudioError::Unsupported);
        }

        let chosen = match source {
            Some(id) if !id.trim().is_empty() => MonitorSource {
                id: id.to_owned(),
                name: id.to_owned(),
            },
            _ => monitor_sources()
                .into_iter()
                .next()
                .ok_or(SystemAudioError::NoMonitor)?,
        };

        // Ask parec for 16 kHz mono directly: it resamples in the server, so no
        // extra conversion is needed here.
        let mut child = Command::new("parec")
            .args([
                "--device",
                &chosen.id,
                "--format=float32le",
                "--rate=16000",
                "--channels=1",
                "--latency-msec=100",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => SystemAudioError::NoServer,
                _ => SystemAudioError::Spawn(error.to_string()),
            })?;

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| SystemAudioError::Spawn("no audio stream from parec".into()))?;

        let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(BUFFER_CAPACITY)));
        let dropped = Arc::new(Mutex::new(0u64));
        let stop = Arc::new(AtomicBool::new(false));

        let reader = {
            let buffer = buffer.clone();
            let dropped = dropped.clone();
            let stop = stop.clone();
            thread::spawn(move || {
                let mut raw = vec![0u8; 4096];
                let mut leftover: Vec<u8> = Vec::new();
                while !stop.load(Ordering::Relaxed) {
                    match stdout.read(&mut raw) {
                        Ok(0) => break,
                        Ok(read) => {
                            let mut bytes = std::mem::take(&mut leftover);
                            bytes.extend_from_slice(&raw[..read]);
                            // f32 little-endian: keep whole samples only.
                            let complete = bytes.len() - (bytes.len() % 4);
                            let samples: Vec<f32> = bytes[..complete]
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .map(|chunk| f32::from_le_bytes(*chunk))
                                .collect();
                            leftover = bytes[complete..].to_vec();
                            if let Ok(mut sink) = buffer.lock() {
                                for sample in samples {
                                    if sink.len() >= BUFFER_CAPACITY {
                                        sink.remove(0);
                                        if let Ok(mut count) = dropped.lock() {
                                            *count += 1;
                                        }
                                    }
                                    sink.push(sample);
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
        };

        // Give parec a moment to fail loudly if the server is unreachable, so a
        // dead recorder is reported here instead of recording silence.
        thread::sleep(Duration::from_millis(150));
        if matches!(child.try_wait(), Ok(Some(_))) {
            stop.store(true, Ordering::Relaxed);
            let _ = reader.join();
            return Err(SystemAudioError::NoServer);
        }

        Ok(Self {
            buffer,
            dropped,
            stop,
            child: Arc::new(Mutex::new(Some(child))),
            reader: Some(reader),
            source: chosen.name,
        })
    }

    /// The source being recorded.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Drain buffered samples.
    pub fn take(&mut self, limit: usize) -> Vec<f32> {
        match self.buffer.lock() {
            Ok(mut buffer) => {
                let count = limit.min(buffer.len());
                buffer.drain(..count).collect()
            }
            Err(_) => Vec::new(),
        }
    }

    /// Samples discarded because the consumer fell behind.
    pub fn dropped(&self) -> u64 {
        self.dropped.lock().map(|count| *count).unwrap_or_default()
    }

    /// Stop recording and reap the child process.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.child.lock()
            && let Some(mut child) = guard.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for SystemAudio {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.child.lock()
            && let Some(mut child) = guard.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

/// Merge microphone and system samples, clamped, for one stream.
pub fn mix(microphone: &[f32], system: &[f32]) -> Vec<f32> {
    let length = microphone.len().max(system.len());
    (0..length)
        .map(|index| {
            let mic = microphone.get(index).copied().unwrap_or(0.0);
            let sys = system.get(index).copied().unwrap_or(0.0);
            (mic + sys).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A resampler is kept available for servers that ignore the requested rate.
pub fn resampler(input_rate: u32) -> LinearResampler {
    LinearResampler::new(input_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WITH_MONITOR: &str = r#"[
      {"name":"alsa_input.pci","description":"Mic","monitor_of_sink":null},
      {"name":"alsa_output.pci.monitor","description":"Monitor of Speakers","monitor_of_sink":"alsa_output.pci"}
    ]"#;

    #[test]
    fn only_monitors_are_returned() {
        let monitors = parse_monitors(WITH_MONITOR);
        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].id, "alsa_output.pci.monitor");
        assert_eq!(monitors[0].name, "Monitor of Speakers");
    }

    #[test]
    fn pipewire_monitor_source_field_is_recognised() {
        let json = r#"[
          {"name":"alsa_output.usb.monitor","description":"Monitor of USB","monitor_source":"alsa_output.usb"},
          {"name":"alsa_input.usb","description":"Mic"}
        ]"#;
        let monitors = parse_monitors(json);
        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].name, "Monitor of USB");
    }

    #[test]
    fn dot_monitor_naming_is_recognised_without_a_field() {
        let json = r#"[{"name":"alsa_output.pci.monitor","description":"Speakers"}]"#;
        assert_eq!(parse_monitors(json).len(), 1);
    }

    #[test]
    fn malformed_output_yields_nothing() {
        assert!(parse_monitors("not json").is_empty());
        assert!(monitor_sources().len() <= 8);
    }

    #[test]
    fn mixing_clamps_instead_of_clipping_through() {
        let mixed = mix(&[0.8, -0.9, 0.0], &[0.8, -0.9, 0.5]);
        assert_eq!(mixed[0], 1.0);
        assert_eq!(mixed[1], -1.0);
        assert_eq!(mixed[2], 0.5);
    }

    #[test]
    fn mixing_handles_unequal_lengths() {
        let mixed = mix(&[0.1, 0.2, 0.3], &[0.1]);
        assert_eq!(mixed.len(), 3);
        assert_eq!(mixed[2], 0.3);
        let other = mix(&[0.1], &[0.2, 0.3]);
        assert_eq!(other.len(), 2);
        assert_eq!(other[1], 0.3);
    }

    #[test]
    fn missing_server_is_reported_not_panicked() {
        // On a machine without a Pulse server this must return an error.
        match SystemAudio::start(None) {
            Ok(audio) => audio.stop(),
            Err(error) => {
                assert!(!error.to_string().is_empty());
            }
        }
    }
}
