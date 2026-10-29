//! Captures a Pulse or PipeWire source with `parec`, including call audio monitors.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::microphone::SampleQueue;

/// Problems encountered while opening a server source.
#[derive(Debug, thiserror::Error)]
pub enum SystemAudioError {
    #[error("no audio server or parec recorder is available")]
    NoServer,
    #[error("no output monitor source is available")]
    NoMonitor,
    #[error("could not launch audio recorder: {0}")]
    Spawn(String),
    #[error("server audio capture requires Linux")]
    Unsupported,
}

/// A running `parec` process and its buffered samples.
pub struct SystemAudio {
    source: String,
    samples: Arc<Mutex<SampleQueue>>,
    child: Mutex<Child>,
    reader: Option<JoinHandle<()>>,
    stopping: Arc<AtomicBool>,
}

impl SystemAudio {
    /// Start recording a selected output sink or the best available monitor.
    pub fn start(source: Option<&str>) -> Result<Self, SystemAudioError> {
        if !cfg!(target_os = "linux") {
            return Err(SystemAudioError::Unsupported);
        }
        if let Some(id) = source.filter(|id| !id.trim().is_empty()) {
            return Self::record(monitor_id(id), id.to_owned());
        }
        let text = super::devices::pactl(&["--format=json", "list", "sources"])
            .map_err(|_| SystemAudioError::NoMonitor)?;
        let monitors = monitor_sources(&text);
        let preferred = super::devices::pactl(&["get-default-sink"])
            .ok()
            .and_then(|text| text.lines().next().map(str::trim).map(monitor_id));
        let selected = preferred
            .as_deref()
            .and_then(|id| monitors.iter().find(|(name, _)| name == id))
            .or_else(|| monitors.first())
            .ok_or(SystemAudioError::NoMonitor)?;
        Self::record(selected.0.clone(), selected.1.clone())
    }

    /// Record one input source exactly as named by the server.
    pub fn start_input(source: &str) -> Result<Self, SystemAudioError> {
        if !cfg!(target_os = "linux") {
            return Err(SystemAudioError::Unsupported);
        }
        Self::record(source.to_owned(), source.to_owned())
    }

    fn record(device: String, label: String) -> Result<Self, SystemAudioError> {
        let mut child = Command::new("parec")
            .args([
                "--device",
                &device,
                "--format=float32le",
                "--rate=16000",
                "--channels=1",
                "--latency-msec=50",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    SystemAudioError::NoServer
                } else {
                    SystemAudioError::Spawn(format!("parec could not start: {error}"))
                }
            })?;
        let mut stdout = child.stdout.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            SystemAudioError::Spawn("parec did not provide an output pipe".into())
        })?;
        let samples = Arc::new(Mutex::new(SampleQueue::new(
            crate::audio::SAMPLE_RATE as usize * 2,
        )));
        let stopping = Arc::new(AtomicBool::new(false));
        let shared = Arc::clone(&samples);
        let stop_reader = Arc::clone(&stopping);
        let reader = thread::spawn(move || {
            let mut decoder = FloatDecoder::default();
            let mut bytes = [0_u8; 4096];
            let mut decoded = Vec::with_capacity(bytes.len() / 4 + 1);
            while !stop_reader.load(Ordering::Relaxed) {
                match stdout.read(&mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        decoded.clear();
                        decoder.decode(&bytes[..count], &mut decoded);
                        if let Ok(mut queue) = shared.lock() {
                            queue.push(&decoded);
                        }
                    }
                }
            }
        });
        thread::sleep(Duration::from_millis(150));
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                stopping.store(true, Ordering::Relaxed);
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(SystemAudioError::Spawn(format!(
                    "could not inspect parec: {error}"
                )));
            }
        };
        if status.is_some() {
            stopping.store(true, Ordering::Relaxed);
            let _ = reader.join();
            return Err(SystemAudioError::NoServer);
        }
        Ok(Self {
            source: label,
            samples,
            child: Mutex::new(child),
            reader: Some(reader),
            stopping,
        })
    }

    /// The chosen sink label or exact input source.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Remove up to `limit` oldest samples.
    pub fn take(&self, limit: usize) -> Vec<f32> {
        self.samples
            .lock()
            .map(|mut queue| queue.take(limit))
            .unwrap_or_default()
    }

    /// Whether the recorder process is still alive.
    pub fn is_running(&self) -> bool {
        self.child
            .lock()
            .is_ok_and(|mut child| child.try_wait().is_ok_and(|status| status.is_none()))
    }

    /// Whether unread samples are available.
    pub fn has_samples(&self) -> bool {
        self.samples.lock().is_ok_and(|queue| !queue.is_empty())
    }

    /// Number of samples evicted by the bounded buffer.
    pub fn dropped(&self) -> u64 {
        self.samples
            .lock()
            .map(|queue| queue.dropped())
            .unwrap_or(0)
    }

    /// Report an ended recorder to the caller.
    pub fn failure(&self) -> Option<String> {
        (!self.is_running()).then(|| "audio recorder has stopped".to_owned())
    }
}

impl Drop for SystemAudio {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        let mut child = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = child.kill();
        let _ = child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn monitor_id(id: &str) -> String {
    if id.ends_with(".monitor") {
        id.to_owned()
    } else {
        format!("{id}.monitor")
    }
}

fn monitor_sources(json: &str) -> Vec<(String, String)> {
    let Ok(serde_json::Value::Array(entries)) = serde_json::from_str::<serde_json::Value>(json)
    else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter_map(|entry| {
            let id = entry.get("name")?.as_str()?;
            let marked = ["monitor_of_sink", "monitor_source"]
                .iter()
                .any(|key| entry.get(*key).is_some_and(|value| !value.is_null()));
            if !marked && !id.ends_with(".monitor") {
                return None;
            }
            let name = entry
                .get("description")
                .and_then(serde_json::Value::as_str)
                .filter(|name| !name.is_empty())
                .unwrap_or(id);
            Some((id.to_owned(), name.to_owned()))
        })
        .collect()
}

#[derive(Default)]
struct FloatDecoder {
    pending: Vec<u8>,
}

impl FloatDecoder {
    fn decode(&mut self, bytes: &[u8], output: &mut Vec<f32>) {
        let mut rest = bytes;
        if !self.pending.is_empty() {
            let needed = 4 - self.pending.len();
            let count = needed.min(rest.len());
            self.pending.extend_from_slice(&rest[..count]);
            rest = &rest[count..];
            if self.pending.len() == 4 {
                output.push(f32::from_le_bytes(
                    self.pending[..4].try_into().unwrap_or([0; 4]),
                ));
                self.pending.clear();
            }
        }
        let (chunks, remainder) = rest.as_chunks::<4>();
        for chunk in chunks {
            output.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        self.pending.extend_from_slice(remainder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_rules() {
        let json = r#"[{"name":"birch.out","monitor_of_sink":7},{"name":"cedar.out","monitor_source":true,"description":"Cedar Output"},{"name":"elm.monitor"},{"name":"plain.in"}]"#;
        assert_eq!(
            monitor_sources(json),
            [
                ("birch.out".into(), "birch.out".into()),
                ("cedar.out".into(), "Cedar Output".into()),
                ("elm.monitor".into(), "elm.monitor".into())
            ]
        );
        assert!(monitor_sources("bad").is_empty());
    }

    #[test]
    fn monitor_suffix_once() {
        assert_eq!(monitor_id("cedar"), "cedar.monitor");
        assert_eq!(monitor_id("cedar.monitor"), "cedar.monitor");
    }

    #[test]
    fn decoder_keeps_partial_samples() {
        let bytes: Vec<u8> = [0.25_f32, -0.75, 1.5]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let mut whole = Vec::new();
        FloatDecoder::default().decode(&bytes, &mut whole);
        let mut split = Vec::new();
        let mut decoder = FloatDecoder::default();
        for chunk in bytes.chunks(5) {
            decoder.decode(chunk, &mut split);
        }
        assert_eq!(split, whole);
    }

    #[test]
    fn missing_server_does_not_panic() {
        let result = SystemAudio::start(None);
        drop(result);
    }
}
