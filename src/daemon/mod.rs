//! The headless daemon: orchestration of capture, speech, chat and notes,
//! driven over a unix socket by the Quickshell plugin and `ctl`.

pub mod protocol;
pub mod server;
pub mod session;

pub use session::{Daemon, Options};

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
        tracing::info!(version = env!("CARGO_PKG_VERSION"), socket = %socket.display(), "daemon ready");
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
        server::serve(&socket, daemon)
            .await
            .map_err(|error| error.to_string())
    })
}
