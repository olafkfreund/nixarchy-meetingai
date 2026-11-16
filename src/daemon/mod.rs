//! The headless daemon: orchestration of capture, speech, chat and notes,
//! driven over a unix socket by the Quickshell plugin and `ctl`.

pub mod calendar;
pub mod callstate;
pub mod nix_settings;
pub mod protocol;
pub mod server;
pub mod session;
pub mod tap;

pub use session::{Daemon, Options};

fn which_on_path(name: &str) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

/// Run the daemon until `quit` or a termination signal.
pub fn run() -> Result<(), String> {
    crate::core::install_tls_provider();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("nixarchy_meetingai=info")),
        )
        .init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let socket = crate::cli::socket_path().ok_or("XDG_RUNTIME_DIR is not set")?;
        let daemon = Daemon::new(Options::from_env()?)?;
        if let Some(path) = nix_settings::path() {
            daemon.apply_nix_settings(&path);
        }
        tracing::info!(version = env!("CARGO_PKG_VERSION"), socket = %socket.display(), "daemon ready");
        for name in ["claude", "codex", "agy", "opencode"] {
            if let Some(path) = which_on_path(name) {
                tracing::info!(cli = name, path = %path.display(), "provider CLI found");
            }
        }
        let signal = {
            let daemon = daemon.clone();
            async move {
                let mut term =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .ok();
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = async { match term.as_mut() { Some(t) => t.recv().await, None => std::future::pending().await } } => {}
                }
                daemon.quit();
            }
        };
        tokio::spawn(signal);
        {
            // Every 5 minutes while idle; the shell also asks on each call change.
            let daemon = daemon.clone();
            tokio::spawn(async move {
                loop {
                    daemon.calendar_refresh(true).await;
                    tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                }
            });
        }
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
            let daemon = daemon.clone();
            let path = std::path::PathBuf::from(runtime).join("meeting-mode.json");
            tokio::spawn(async move {
                let mut signature = None;
                let mut bytes = Vec::new();
                let mut previous = callstate::CallReading::default();
                loop {
                    let metadata = tokio::fs::metadata(&path).await.ok();
                    let next = metadata.as_ref().map(|m| (m.modified().ok(), m.len()));
                    let changed = next != signature;
                    if changed {
                        match tokio::fs::read(&path).await {
                            Ok(read) => {
                                bytes = read;
                                signature = next;
                            }
                            Err(_) => {
                                bytes.clear();
                                signature = None;
                            }
                        }
                    }
                    let alive = if let Some(pid) = callstate::pid(&bytes) {
                        tokio::fs::metadata(format!("/proc/{pid}")).await.is_ok()
                    } else {
                        false
                    };
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;
                    let reading = callstate::parse(&bytes, now, |_| alive);
                    if reading != previous {
                        daemon.on_call_reading(reading.clone());
                        previous = reading;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            });
        }
        let served = server::serve(&socket, daemon)
            .await
            .map_err(|error| error.to_string());
        crate::ai::chat::opencode::shutdown().await;
        served
    })
}
