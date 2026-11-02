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
    #[error("the speaker recording stopped")]
    Stopped,
}

struct Recorder {
    child: Child,
    reader: Option<JoinHandle<()>>,
    /// Only the recorder in use feeds the queue; a replacement stays muted until the old one is gone.
    live: Arc<AtomicBool>,
}

impl Recorder {
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

struct Follow {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// A running `parec` process and its buffered samples.
pub struct SystemAudio {
    source: String,
    samples: Arc<Mutex<SampleQueue>>,
    recorder: Arc<Mutex<Recorder>>,
    exited: AtomicBool,
    follow: Option<Follow>,
    moved: Arc<Mutex<Option<String>>>,
}

fn spawn_recorder(
    program: &str,
    device: &str,
    queue: Arc<Mutex<SampleQueue>>,
    live: bool,
) -> Result<Recorder, SystemAudioError> {
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
    let live = Arc::new(AtomicBool::new(live));
    let feeding = Arc::clone(&live);
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
                    if !feeding.load(Ordering::Acquire) {
                        continue;
                    }
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
        Recorder {
            child,
            reader: Some(reader),
            live,
        }
        .stop();
        return Err(error);
    }
    Ok(Recorder {
        child,
        reader: Some(reader),
        live,
    })
}

/// The server's readable name for a sink's monitor, or the sink id when it can't be listed.
fn describe(sink: &str) -> String {
    let device = monitor_id(sink);
    super::devices::pactl(&["--format=json", "list", "sources"])
        .ok()
        .and_then(|text| {
            monitor_sources(&text)
                .into_iter()
                .find(|(id, _)| *id == device)
        })
        .map_or_else(|| sink.to_owned(), |(_, name)| name)
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
        Self::start_following("parec", || {
            super::devices::pactl(&["get-default-sink"])
                .ok()
                .and_then(|text| text.lines().next().map(str::trim).map(str::to_owned))
                .filter(|sink| !sink.is_empty())
        })
    }

    fn start_following(
        program: &str,
        probe: impl Fn() -> Option<String> + Send + 'static,
    ) -> Result<Self, SystemAudioError> {
        let preferred = probe();
        let monitors = super::devices::pactl(&["--format=json", "list", "sources"])
            .ok()
            .map(|text| monitor_sources(&text))
            .unwrap_or_default();
        let (device, name, current_sink) = if let Some(sink) = preferred {
            let device = monitor_id(&sink);
            let name = monitors
                .iter()
                .find(|(id, _)| id == &device)
                .map_or_else(|| sink.clone(), |(_, name)| name.clone());
            (device, name, sink)
        } else {
            let (device, name) = monitors.first().ok_or(SystemAudioError::NoMonitor)?;
            let sink = device.strip_suffix(".monitor").unwrap_or(device).to_owned();
            (device.clone(), name.clone(), sink)
        };
        let mut audio = Self::launch(program, &device, &name)?;
        let stop = Arc::new(AtomicBool::new(false));
        let recorder = Arc::clone(&audio.recorder);
        let samples = Arc::clone(&audio.samples);
        let moved = Arc::clone(&audio.moved);
        let thread_stop = Arc::clone(&stop);
        let program = program.to_owned();
        let thread = thread::spawn(move || {
            let mut current_sink = current_sink;
            let mut next_probe = Instant::now() + Duration::from_secs(2);
            'watch: while !thread_stop.load(Ordering::Relaxed) {
                // 250 ms between checks, in slices, so Drop waits at most about 50 ms for a quiet watcher.
                for _ in 0..5 {
                    thread::sleep(Duration::from_millis(50));
                    if thread_stop.load(Ordering::Relaxed) {
                        break 'watch;
                    }
                }
                let alive = recorder.lock().is_ok_and(|mut recorder| {
                    recorder
                        .child
                        .try_wait()
                        .is_ok_and(|status| status.is_none())
                });
                // A dead recorder usually means its sink vanished: ask for the new default at once.
                let due = !alive || Instant::now() >= next_probe;
                let selected = if due {
                    next_probe = Instant::now() + Duration::from_secs(2);
                    probe().filter(|sink| !sink.trim().is_empty())
                } else {
                    None
                };
                let target = selected.unwrap_or_else(|| current_sink.clone());
                if alive && target == current_sink {
                    continue;
                }
                // Spawning includes a 200 ms wait, so it must stay outside the lock.
                match spawn_recorder(&program, &monitor_id(&target), Arc::clone(&samples), false) {
                    Ok(new) => {
                        // Before the handover, so a slow lookup cannot delay the notice (and its echo-gate reset).
                        let label = (target != current_sink).then(|| describe(&target));
                        let live = Arc::clone(&new.live);
                        let old = {
                            let mut recorder = recorder
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            std::mem::replace(&mut *recorder, new)
                        };
                        let mut old = old;
                        old.stop();
                        if let Some(label) = label {
                            *moved
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(label);
                            current_sink = target;
                        }
                        live.store(true, Ordering::Release);
                    }
                    Err(_) if !alive => break,
                    Err(_) => {}
                }
            }
        });
        audio.follow = Some(Follow {
            stop,
            thread: Some(thread),
        });
        Ok(audio)
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
        let samples = Arc::new(Mutex::new(SampleQueue::new(
            crate::audio::SAMPLE_RATE as usize * 2,
        )));
        let recorder = spawn_recorder(program, device, Arc::clone(&samples), true)?;
        Ok(Self {
            source: shown.to_owned(),
            samples,
            recorder: Arc::new(Mutex::new(recorder)),
            exited: AtomicBool::new(false),
            follow: None,
            moved: Arc::new(Mutex::new(None)),
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
        if let Some(follow) = &self.follow {
            return follow
                .thread
                .as_ref()
                .is_some_and(|thread| !thread.is_finished());
        }
        if self.exited.load(Ordering::Relaxed) {
            return false;
        }
        let alive = self.recorder.lock().is_ok_and(|mut recorder| {
            recorder
                .child
                .try_wait()
                .is_ok_and(|status| status.is_none())
        });
        if !alive {
            self.exited.store(true, Ordering::Relaxed);
        }
        alive
    }

    /// Take the latest output name reported by the watcher.
    pub fn take_moved(&self) -> Option<String> {
        self.moved.lock().ok().and_then(|mut moved| moved.take())
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
        if !self.is_running() {
            Some("audio recorder has stopped".to_owned())
        } else {
            None
        }
    }
}

#[cfg(test)]
impl SystemAudio {
    /// A recorder whose process has already exited, for tests of the callers.
    pub(crate) fn stopped_for_test() -> Self {
        let mut child = Command::new("true").spawn().expect("spawn true");
        let _ = child.wait();
        Self {
            source: "fake output".to_owned(),
            samples: Arc::new(Mutex::new(SampleQueue::new(16))),
            recorder: Arc::new(Mutex::new(Recorder {
                child,
                reader: None,
                live: Arc::new(AtomicBool::new(true)),
            })),
            exited: AtomicBool::new(true),
            follow: None,
            moved: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn moved_for_test(name: &str) -> Self {
        let mut audio = Self::stopped_for_test();
        let child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        audio.recorder = Arc::new(Mutex::new(Recorder {
            child,
            reader: None,
            live: Arc::new(AtomicBool::new(true)),
        }));
        audio.exited.store(false, Ordering::Relaxed);
        *audio.moved.lock().expect("lock moved") = Some(name.to_owned());
        audio
    }
}

impl Drop for SystemAudio {
    fn drop(&mut self) {
        if let Some(follow) = &mut self.follow {
            follow.stop.store(true, Ordering::Relaxed);
            if let Some(thread) = follow.thread.take() {
                let _ = thread.join();
            }
        }
        let mut recorder = self
            .recorder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        recorder.stop();
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
    use std::path::PathBuf;

    fn fake_parec(name: &str) -> (PathBuf, PathBuf) {
        fake_parec_with(name, "")
    }

    /// `extra` is shell run before the recorder settles, e.g. to refuse a device.
    fn fake_parec_with(name: &str, extra: &str) -> (PathBuf, PathBuf) {
        let stem = format!(
            "fake-parec-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let script = std::env::temp_dir().join(&stem);
        let args = std::env::temp_dir().join(format!("{stem}-args"));
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n{extra}\nexec sleep 30\n",
                args.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        (script, args)
    }

    fn wait_until(mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(Instant::now() < deadline, "condition did not become true");
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn recorded_device(args: &std::path::Path) -> Option<String> {
        let text = std::fs::read_to_string(args).ok()?;
        let mut args = text.lines();
        args.find(|arg| *arg == "--device")?;
        args.next().map(str::to_owned)
    }

    /// Another test forking while a fake script was open for writing gives ETXTBSY; retry that.
    fn retry_busy(mut start: impl FnMut() -> Result<SystemAudio, SystemAudioError>) -> SystemAudio {
        for _ in 0..20 {
            match start() {
                Err(SystemAudioError::Spawn(text)) if text.contains("Text file busy") => {
                    thread::sleep(Duration::from_millis(50));
                }
                other => return other.unwrap(),
            }
        }
        panic!("fake recorder stayed busy");
    }

    fn following(script: &std::path::Path, sink: &Arc<Mutex<Option<String>>>) -> SystemAudio {
        retry_busy(|| {
            let probe_sink = Arc::clone(sink);
            SystemAudio::start_following(script.to_str().unwrap(), move || {
                probe_sink.lock().unwrap().clone()
            })
        })
    }

    #[test]
    fn follows_new_default_once_and_stays_running() {
        let (script, args) = fake_parec("switch");
        let sink = Arc::new(Mutex::new(Some("A".to_owned())));
        let audio = following(&script, &sink);
        wait_until(|| recorded_device(&args).as_deref() == Some("A.monitor"));
        assert!(audio.is_running());
        *sink.lock().unwrap() = Some("B".to_owned());
        wait_until(|| {
            assert!(audio.is_running());
            recorded_device(&args).as_deref() == Some("B.monitor")
        });
        wait_until(|| audio.take_moved().as_deref() == Some("B"));
        assert!(audio.is_running());
        assert_eq!(audio.take_moved(), None);
        drop(audio);
        let _ = std::fs::remove_file(script);
        let _ = std::fs::remove_file(args);
    }

    #[test]
    fn dead_child_restarts_on_new_default() {
        // Once `gone` exists, sink A has vanished: parec on A.monitor exits at once.
        let gone = std::env::temp_dir().join(format!("fake-parec-gone-{}", std::process::id()));
        let _ = std::fs::remove_file(&gone);
        let (script, args) = fake_parec_with(
            "dead-switch",
            &format!(
                "case \"$*\" in *A.monitor*) [ -e '{}' ] && exit 1;; esac",
                gone.display()
            ),
        );
        let sink = Arc::new(Mutex::new(Some("A".to_owned())));
        let audio = following(&script, &sink);
        *sink.lock().unwrap() = Some("C".to_owned());
        std::fs::write(&gone, "").unwrap();
        audio.recorder.lock().unwrap().child.kill().unwrap();
        wait_until(|| recorded_device(&args).as_deref() == Some("C.monitor"));
        wait_until(|| audio.take_moved().as_deref() == Some("C"));
        assert!(audio.is_running());
        drop(audio);
        let _ = std::fs::remove_file(script);
        let _ = std::fs::remove_file(args);
        let _ = std::fs::remove_file(gone);
    }

    #[test]
    fn dead_child_with_failed_relaunch_ends_watcher() {
        let (script, args) = fake_parec("failed-relaunch");
        let sink = Arc::new(Mutex::new(Some("A".to_owned())));
        let audio = following(&script, &sink);
        std::fs::remove_file(&script).unwrap();
        audio.recorder.lock().unwrap().child.kill().unwrap();
        wait_until(|| !audio.is_running());
        drop(audio);
        let _ = std::fs::remove_file(args);
    }

    #[test]
    fn explicit_launch_has_no_watcher() {
        let (script, args) = fake_parec("explicit");
        let audio = retry_busy(|| {
            SystemAudio::launch(script.to_str().unwrap(), "chosen.monitor", "chosen")
        });
        assert!(audio.follow.is_none());
        assert_eq!(recorded_device(&args).as_deref(), Some("chosen.monitor"));
        drop(audio);
        let _ = std::fs::remove_file(script);
        let _ = std::fs::remove_file(args);
    }

    #[test]
    fn dropping_following_recorder_reaps_last_child() {
        let (script, args) = fake_parec("drop");
        let sink = Arc::new(Mutex::new(Some("A".to_owned())));
        let audio = following(&script, &sink);
        *sink.lock().unwrap() = Some("B".to_owned());
        wait_until(|| recorded_device(&args).as_deref() == Some("B.monitor"));
        wait_until(|| audio.take_moved().as_deref() == Some("B"));
        let pid = audio.recorder.lock().unwrap().child.id();
        drop(audio);
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        let _ = std::fs::remove_file(script);
        let _ = std::fs::remove_file(args);
    }

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
        let pid = live.recorder.lock().unwrap().child.id();
        drop(live);
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "child {pid} still exists"
        );
    }
}
