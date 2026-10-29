//! Captures a Pulse or PipeWire source with `parec`, including call audio monitors.

use std::io::{BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::devices::{is_speaker_monitor, label};
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
    exited: AtomicBool,
}

impl SystemAudio {
    /// Start recording a selected output sink or the best available monitor.
    pub fn start(source: Option<&str>) -> Result<Self, SystemAudioError> {
        if !cfg!(target_os = "linux") {
            return Err(SystemAudioError::Unsupported);
        }
        if let Some(id) = source.filter(|id| !id.trim().is_empty()) {
            return Self::launch("parec", &monitor_id(id), id);
        }
        let text = super::devices::pactl(&["--format=json", "list", "sources"])
            .map_err(|_| SystemAudioError::NoMonitor)?;
        let monitors = monitor_sources(&text);
        let preferred = super::devices::pactl(&["get-default-sink"])
            .ok()
            .and_then(|text| text.lines().next().map(|line| monitor_id(line.trim())));
        let (id, name) = monitors
            .iter()
            .min_by_key(|(id, _)| Some(id.as_str()) != preferred.as_deref())
            .ok_or(SystemAudioError::NoMonitor)?;
        Self::launch("parec", id, name)
    }

    /// Record one input source exactly as named by the server.
    pub fn start_input(source: &str) -> Result<Self, SystemAudioError> {
        if !cfg!(target_os = "linux") {
            return Err(SystemAudioError::Unsupported);
        }
        Self::launch("parec", source, source)
    }

    /// Run `program` (normally `parec`) on `device` and wait briefly to see that it stays up.
    fn launch(program: &str, device: &str, shown: &str) -> Result<Self, SystemAudioError> {
        let mut child = Command::new(program)
            .args([
                "--latency-msec=50",
                "--format=float32le",
                "--rate=16000",
                "--channels=1",
                "--device",
                device,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    SystemAudioError::NoServer
                } else {
                    SystemAudioError::Spawn(format!("{program} could not start: {error}"))
                }
            })?;
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SystemAudioError::Spawn(format!(
                "{program} did not provide an output pipe"
            )));
        };
        let samples = Arc::new(Mutex::new(SampleQueue::new(
            crate::audio::SAMPLE_RATE as usize * 2,
        )));
        let queue = Arc::clone(&samples);
        // Ends at EOF, which killing the child (see Drop) or its own exit produces.
        let reader = thread::spawn(move || {
            let mut input = BufReader::with_capacity(16 * 1024, stdout);
            let mut frame = [0_u8; 2048];
            let mut carry = Vec::new();
            let mut decoded = Vec::with_capacity(frame.len() / 4);
            loop {
                match input.read(&mut frame) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        decoded.clear();
                        decode_samples(&mut carry, &frame[..count], &mut decoded);
                        if let Ok(mut queue) = queue.lock() {
                            queue.push(&decoded);
                        }
                    }
                }
            }
        });
        let deadline = Instant::now() + Duration::from_millis(200);
        let verdict = loop {
            match child.try_wait() {
                Ok(Some(_)) => break Err(SystemAudioError::NoServer),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
                Ok(None) => break Ok(()),
                Err(error) => {
                    break Err(SystemAudioError::Spawn(format!(
                        "could not inspect {program}: {error}"
                    )));
                }
            }
        };
        if let Err(error) = verdict {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(error);
        }
        Ok(Self {
            source: shown.to_owned(),
            samples,
            child: Mutex::new(child),
            reader: Some(reader),
            exited: AtomicBool::new(false),
        })
    }

    /// The chosen sink label or exact input source.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Remove up to `limit` oldest samples.
    pub fn take(&self, limit: usize) -> Vec<f32> {
        SampleQueue::take_from(&self.samples, limit)
    }

    /// Whether the recorder process is still alive.
    pub fn is_running(&self) -> bool {
        if self.exited.load(Ordering::Relaxed) {
            return false;
        }
        let alive = self
            .child
            .lock()
            .is_ok_and(|mut child| child.try_wait().is_ok_and(|status| status.is_none()));
        if !alive {
            self.exited.store(true, Ordering::Relaxed);
        }
        alive
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
        self.is_running();
        if self.exited.load(Ordering::Relaxed) {
            Some("audio recorder has stopped".to_owned())
        } else {
            None
        }
    }
}

impl Drop for SystemAudio {
    fn drop(&mut self) {
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

/// The monitor source of an output sink; `.monitor` is never appended twice.
fn monitor_id(id: &str) -> String {
    format!("{}.monitor", id.strip_suffix(".monitor").unwrap_or(id))
}

/// `(id, display name)` of every monitor in a `pactl --format=json list sources` dump.
fn monitor_sources(json: &str) -> Vec<(String, String)> {
    let Ok(serde_json::Value::Array(entries)) = serde_json::from_str::<serde_json::Value>(json)
    else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("name")?.as_str()?;
            (is_speaker_monitor(entry) || id.ends_with(".monitor"))
                .then(|| (id.to_owned(), label(entry, id).to_owned()))
        })
        .collect()
}

/// Append the whole little-endian f32 samples in `carry + bytes` to `out`, keeping the remainder.
fn decode_samples(carry: &mut Vec<u8>, bytes: &[u8], out: &mut Vec<f32>) {
    carry.extend_from_slice(bytes);
    let (whole, _) = carry.as_chunks::<4>();
    out.extend(whole.iter().map(|raw| f32::from_le_bytes(*raw)));
    let used = whole.len() * 4;
    carry.drain(..used);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_rules() {
        let json = r#"[
          {"name":"birch.out","monitor_of_sink":"birch"},
          {"name":"cedar.out","monitor_source":"cedar","description":"Cedar Output"},
          {"name":"elm.monitor"},
          {"name":"plain.in"},
          {"name":"fennel-desk-mic","description":"Fennel Desk Mic","monitor_of_sink":null,"monitor_source":"","state":"SUSPENDED"},
          {"name":"quince-hdmi-out.monitor","description":"Monitor of Quince HDMI","monitor_of_sink":null,"monitor_source":"quince-hdmi-out"}
        ]"#;
        assert_eq!(
            monitor_sources(json),
            [
                ("birch.out".into(), "birch.out".into()),
                ("cedar.out".into(), "Cedar Output".into()),
                ("elm.monitor".into(), "elm.monitor".into()),
                (
                    "quince-hdmi-out.monitor".into(),
                    "Monitor of Quince HDMI".into()
                )
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
    fn decoding_is_independent_of_how_reads_are_cut() {
        let bytes: Vec<u8> = (0..500_i16)
            .map(|n| f32::from(n) * 0.5 - 100.0)
            .flat_map(f32::to_le_bytes)
            .collect();
        let mut whole = Vec::new();
        decode_samples(&mut Vec::new(), &bytes, &mut whole);
        assert_eq!(whole.len(), 500);
        for size in [1, 3, 7, 1021] {
            let (mut carry, mut split) = (Vec::new(), Vec::new());
            for chunk in bytes.chunks(size) {
                decode_samples(&mut carry, chunk, &mut split);
            }
            assert_eq!(split, whole, "chunk size {size}");
            assert!(carry.is_empty());
        }
    }

    #[test]
    fn recorder_that_exits_or_is_absent_is_an_error_and_a_live_one_is_reaped() {
        let absent = SystemAudio::launch("no-such-recorder-8841", "x", "x");
        assert!(matches!(absent, Err(SystemAudioError::NoServer)));
        // `true` ignores its arguments and exits at once.
        let quitter = SystemAudio::launch("true", "x", "x");
        assert!(matches!(quitter, Err(SystemAudioError::NoServer)));
        // A stand-in recorder that ignores its arguments and stays up.
        let script = std::env::temp_dir().join(format!("fake-parec-{}", std::process::id()));
        std::fs::write(&script, "#!/bin/sh\nexec sleep 30\n").unwrap();
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let mut attempt = 0;
        let launched = loop {
            let result = SystemAudio::launch(script.to_str().unwrap(), "x", "shown");
            attempt += 1;
            match &result {
                // Another test forking while the script was open for writing gives ETXTBSY.
                Err(SystemAudioError::Spawn(text))
                    if text.contains("Text file busy") && attempt < 5 =>
                {
                    thread::sleep(Duration::from_millis(50));
                }
                _ => break result,
            }
        };
        let _ = std::fs::remove_file(&script);
        let live = launched.unwrap();
        let pid = live.child.lock().unwrap().id();
        drop(live);
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "child {pid} still exists"
        );
    }
}
