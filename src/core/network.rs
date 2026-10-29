//! Shared HTTP clients and a size-capped body reader for provider traffic.

use futures_util::TryStreamExt;
use std::sync::OnceLock;
use std::time::Duration;

fn client(total: u64, read: u64) -> reqwest::Client {
    super::install_tls_provider();
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(total))
        .read_timeout(Duration::from_secs(read))
        .pool_max_idle_per_host(2)
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .expect("HTTP client settings are valid")
}

pub(crate) fn chat_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| client(180, 30)).clone()
}

/// Loopback Ollama may spend minutes loading a cold model before its first byte.
pub(crate) fn loading_chat_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| client(600, 300)).clone()
}

pub(crate) fn speech_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| client(120, 30)).clone()
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BodyError {
    #[error("The provider's response was larger than the allowed size")]
    TooLarge,
    #[error("The provider's response failed: {0}")]
    Request(reqwest::Error),
}

/// Read a response body, refusing anything over `limit` bytes without buffering past it.
pub(crate) async fn read_body(
    response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, BodyError> {
    if response
        .content_length()
        .is_some_and(|declared| declared > limit as u64)
    {
        return Err(BodyError::TooLarge);
    }
    response
        .bytes_stream()
        .map_err(|e| BodyError::Request(e.without_url()))
        .try_fold(Vec::new(), |mut acc, piece| async move {
            if piece.len() > limit - acc.len() {
                return Err(BodyError::TooLarge);
            }
            acc.extend_from_slice(&piece);
            Ok(acc)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Serve `body` once, optionally claiming a different Content-Length.
    async fn serve(body: &'static [u8], claimed: Option<usize>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut scratch = [0u8; 1024];
            let _ = socket.read(&mut scratch).await;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                claimed.unwrap_or(body.len())
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(body).await;
        });
        url
    }

    /// Serve `chunks` once as a chunked response, so there is no Content-Length.
    async fn serve_chunked(chunks: &'static [&'static [u8]]) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut scratch = [0u8; 1024];
            let _ = socket.read(&mut scratch).await;
            let head = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
            let _ = socket.write_all(head.as_bytes()).await;
            for chunk in chunks {
                let _ = socket
                    .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                    .await;
                let _ = socket.write_all(chunk).await;
                let _ = socket.write_all(b"\r\n").await;
            }
            let _ = socket.write_all(b"0\r\n\r\n").await;
        });
        url
    }

    async fn fetch(url: &str) -> reqwest::Response {
        chat_client().get(url).send().await.unwrap()
    }

    #[tokio::test]
    async fn body_at_the_limit_is_accepted() {
        let url = serve(b"seven!!", None).await;
        assert_eq!(read_body(fetch(&url).await, 7).await.unwrap(), b"seven!!");
    }

    #[tokio::test]
    async fn body_over_the_limit_is_refused() {
        let url = serve(b"seven!!", None).await;
        assert!(matches!(
            read_body(fetch(&url).await, 6).await,
            Err(BodyError::TooLarge)
        ));
    }

    #[tokio::test]
    async fn declared_length_over_the_limit_is_refused() {
        let url = serve(b"", Some(5_000_000)).await;
        assert!(matches!(
            read_body(fetch(&url).await, 1024).await,
            Err(BodyError::TooLarge)
        ));
    }

    #[tokio::test]
    async fn empty_body_is_empty() {
        let url = serve(b"", None).await;
        assert!(read_body(fetch(&url).await, 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn chunked_body_over_the_limit_is_refused() {
        let url = serve_chunked(&[b"abcd", b"efgh"]).await;
        let response = fetch(&url).await;
        assert!(response.content_length().is_none());
        assert!(matches!(
            read_body(response, 7).await,
            Err(BodyError::TooLarge)
        ));
    }

    #[tokio::test]
    async fn chunked_body_under_the_limit_is_accepted() {
        let url = serve_chunked(&[b"abcd", b"efg"]).await;
        let response = fetch(&url).await;
        assert!(response.content_length().is_none());
        assert_eq!(read_body(response, 8).await.unwrap(), b"abcdefg");
    }
}
