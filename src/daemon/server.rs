//! Unix socket server: many clients, one broadcast of events.

use std::{os::unix::fs::PermissionsExt, path::Path, sync::Arc};

use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::{broadcast::error::RecvError, mpsc},
};

use super::{
    protocol::{self, MAX_LINE, Request},
    session::Daemon,
};

/// Bind `path` (its directory becomes `0700`, a stale socket is replaced) and
/// serve until the daemon is told to shut down.
pub async fn serve(path: &Path, daemon: Arc<Daemon>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    let result = loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => { tokio::spawn(client(stream, daemon.clone())); }
                Err(error) => break Err(error),
            },
            () = daemon.shutdown.notified() => break Ok(()),
        }
    };
    let _ = std::fs::remove_file(path);
    result
}

async fn client(stream: UnixStream, daemon: Arc<Daemon>) {
    let (read, mut write) = stream.into_split();
    let (out, mut lines) = mpsc::channel::<String>(256);
    let writer = tokio::spawn(async move {
        while let Some(line) = lines.recv().await {
            if write.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let mut events = daemon.subscribe();
    let forward = {
        let out = out.clone();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(value) => {
                        if out.send(format!("{value}\n")).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        })
    };

    let mut reader = BufReader::new(read);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        let read = (&mut reader)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut buffer)
            .await;
        // EOF, an error, a line past the cap, or a final line without a newline.
        if !matches!(read, Ok(n) if n > 0)
            || buffer.len() > MAX_LINE
            || buffer.last() != Some(&b'\n')
        {
            break;
        }
        let request = match serde_json::from_slice::<Request>(&buffer) {
            Ok(request) => request,
            Err(error) => {
                let line = protocol::reply(&serde_json::Value::Null, Err(error.to_string()));
                let _ = out.send(line).await;
                continue;
            }
        };
        // In order on this connection; events still stream through `forward`.
        let result = daemon
            .handle(&request.cmd, request.args, request.origin.as_deref())
            .await;
        let _ = out.send(protocol::reply(&request.id, result)).await;
    }
    forward.abort();
    writer.abort();
}
