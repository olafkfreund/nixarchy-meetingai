//! Per-user local named pipe. No network port or Unix socket is required.
use super::Action;
pub use super::parse_command;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use windows_sys::Win32::{Foundation::*, Storage::FileSystem::*, System::Pipes::*};

type Queue = Arc<Mutex<Vec<Action>>>;

pub fn pipe_name() -> Result<String, String> {
    use sha2::Digest;
    let path = crate::cli::instance_lock_path().ok_or("no local data directory")?;
    let digest = sha2::Sha256::digest(path.to_string_lossy().as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!(r"\\.\pipe\nixarchy-meetingai-{hex}"))
}

pub struct ControlListener {
    queue: Queue,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl ControlListener {
    pub fn start() -> Result<Self, String> {
        Self::start_at(pipe_name()?)
    }

    fn start_at(name: String) -> Result<Self, String> {
        let queue: Queue = Default::default();
        let stop = Arc::new(AtomicBool::new(false));
        let sink = queue.clone();
        let halt = stop.clone();
        let (ready, started) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
            // SAFETY: null-terminated name lives through the call. Default DACL
            // restricts the pipe to the creating user/system; remote clients denied.
            let handle = unsafe {
                CreateNamedPipeW(
                    name.as_ptr(),
                    PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    256,
                    256,
                    0,
                    std::ptr::null(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                let _ = ready.send(Err(std::io::Error::last_os_error().to_string()));
                return;
            }
            // SAFETY: this thread takes unique ownership of the new pipe handle.
            let pipe = unsafe { std::fs::File::from_raw_handle(handle) };
            let _ = ready.send(Ok(()));
            while !halt.load(Ordering::Relaxed) {
                // SAFETY: pipe remains open and PIPE_NOWAIT makes this nonblocking.
                let listening =
                    unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) } != 0;
                let error = std::io::Error::last_os_error().raw_os_error();
                // In PIPE_NOWAIT mode success only means the instance became
                // available. It does not mean a client connected. Disconnecting
                // it now can discard a command arriving just after this call.
                // A one-shot client can write and close between our polls. Read
                // its buffered command before resetting the pipe instance.
                let connected = !listening
                    && matches!(error,
                    Some(code) if code == ERROR_PIPE_CONNECTED as i32 || code == ERROR_NO_DATA as i32);
                if !connected {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                let deadline = std::time::Instant::now() + Duration::from_millis(500);
                let mut payload = Vec::new();
                while !halt.load(Ordering::Relaxed)
                    && std::time::Instant::now() < deadline
                    && payload.len() < 128
                {
                    let mut bytes = [0; 128];
                    let mut read = 0;
                    // SAFETY: the pipe and writable buffer live through this
                    // synchronous, nonblocking read. Use ReadFile directly:
                    // std::fs::File maps ERROR_NO_DATA to EOF, although here it
                    // means a connected client has not written its bytes yet.
                    let ok = unsafe {
                        ReadFile(
                            pipe.as_raw_handle(),
                            bytes.as_mut_ptr(),
                            bytes.len() as u32,
                            &mut read,
                            std::ptr::null_mut(),
                        )
                    } != 0;
                    if ok {
                        payload.extend_from_slice(&bytes[..read as usize]);
                        if payload.contains(&b'\n') {
                            break;
                        }
                    } else if std::io::Error::last_os_error().raw_os_error()
                        != Some(ERROR_NO_DATA as i32)
                    {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                if payload.len() <= 128
                    && let Ok(text) = std::str::from_utf8(&payload)
                    && let Some(action) = parse_command(text.trim())
                    && let Ok(mut queue) = sink.lock()
                {
                    queue.push(action);
                }
                // SAFETY: disconnect only this owned instance; keep the pipe for reuse.
                unsafe { DisconnectNamedPipe(pipe.as_raw_handle()) };
            }
        });
        match started.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Self {
                queue,
                stop,
                worker: Some(worker),
            }),
            result => {
                stop.store(true, Ordering::Relaxed);
                Err(match result {
                    Ok(Err(error)) => error,
                    _ => "named pipe startup timed out".into(),
                })
            }
        }
    }

    pub fn drain(&self) -> Vec<Action> {
        self.queue
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }
}
impl Drop for ControlListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn connect(name: &str) -> std::fs::File {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match std::fs::OpenOptions::new().write(true).open(name) {
                Ok(pipe) => return pipe,
                Err(error)
                    if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("connect to test pipe: {error}"),
            }
        }
    }

    fn receive(listener: &ControlListener, expected: Action) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut actions = Vec::new();
        while std::time::Instant::now() < deadline {
            actions.extend(listener.drain());
            if !actions.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(actions, [expected]);
    }

    #[test]
    fn named_pipe_delivers_commands_and_releases_on_drop() {
        let name = format!(r"\\.\pipe\nixarchy-meetingai-test-{}", uuid::Uuid::new_v4());
        let listener = ControlListener::start_at(name.clone()).unwrap();
        // Exercise both initial listening and reconnects. Clients close without
        // waiting for the worker, just like one-shot shortcut invocations.
        for _ in 0..50 {
            let mut pipe = connect(&name);
            writeln!(pipe, "toggle-window").unwrap();
            drop(pipe);
            receive(&listener, Action::ToggleWindow);
        }
        drop(listener);
        assert!(ControlListener::start_at(name).is_ok());
    }

    #[test]
    fn named_pipe_waits_for_delayed_and_partial_commands() {
        let name = format!(r"\\.\pipe\nixarchy-meetingai-test-{}", uuid::Uuid::new_v4());
        let listener = ControlListener::start_at(name.clone()).unwrap();
        // The initial ConnectNamedPipe succeeds without a connected client.
        std::thread::sleep(Duration::from_millis(50));
        let mut pipe = connect(&name);
        std::thread::sleep(Duration::from_millis(50));
        pipe.write_all(b"toggle-").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(listener.drain().is_empty());
        pipe.write_all(b"window\n").unwrap();
        receive(&listener, Action::ToggleWindow);
    }
}
