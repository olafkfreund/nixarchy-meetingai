//! Shared connection pools and bounded provider bodies. Worker count is unrelated
//! to the number of asynchronous in-flight requests.
use futures_util::StreamExt;
use std::{sync::OnceLock, time::Duration};

fn client(total: u64, read: u64) -> reqwest::Client {
    super::install_tls_provider();
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(read))
        .timeout(Duration::from_secs(total))
        .pool_max_idle_per_host(2)
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .expect("a default HTTP client always builds")
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
    #[error("provider response exceeded the size limit")]
    TooLarge,
    #[error("provider response failed: {0}")]
    Request(reqwest::Error),
}

pub(crate) async fn read_body(
    response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, BodyError> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err(BodyError::TooLarge);
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| BodyError::Request(e.without_url()))?;
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(BodyError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
