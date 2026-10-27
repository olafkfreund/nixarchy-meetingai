//! Chat completions over bring-your-own-key providers.
//!
//! Builds the vendor-specific request body, streams server-sent events and emits
//! text deltas as they arrive. Cancellation is cooperative via a watch channel so
//! a stopped stream stops consuming the socket promptly.

use std::sync::Arc;

use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::ai::providers::{ChatProvider, ChatWire, ProviderConfig};
use crate::core::domain::Message;

/// Errors from a chat request.
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("no provider is configured")]
    NotConfigured,
    #[error("this provider needs an API key")]
    MissingKey,
    #[error("the request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("the provider returned {status}: {message}")]
    Status { status: u16, message: String },
    #[error("the provider returned an unexpected response: {0}")]
    Protocol(String),
    #[error("{0}")]
    Cli(String),
}

/// A text delta or the end of the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    Delta(String),
    Done,
}

/// Extra context for one turn.
#[derive(Debug, Clone, Default)]
pub struct TurnContext {
    /// System prompt steering the model.
    pub system: Option<String>,
    /// Base64 PNG attachments, no data-URL prefix.
    pub images: Vec<String>,
}

/// Shared HTTP client, so connections are reused across turns.
pub fn loading_http_client() -> reqwest::Client {
    crate::core::network::loading_chat_client()
}

pub fn http_client() -> reqwest::Client {
    crate::core::network::chat_client()
}

const MAX_SSE_LINE_BYTES: usize = 256 * 1024;
const MAX_ANSWER_BYTES: usize = 2 * 1024 * 1024;

/// Build the vendor request body.
pub fn build_body(
    provider: &ChatProvider,
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
) -> Value {
    if provider.wire == ChatWire::Anthropic {
        return anthropic_body(config, history, context, provider.supports_images);
    }
    let mut messages: Vec<Value> = Vec::with_capacity(history.len() + 1);
    if let Some(system) = &context.system {
        messages.push(json!({ "role": "system", "content": system }));
    }
    for (index, message) in history.iter().enumerate() {
        let images = if provider.supports_images
            && index + 1 == history.len()
            && message.role == crate::core::domain::Role::User
        {
            context.images.as_slice()
        } else {
            &[]
        };
        messages.push(openai_message(message, images));
    }
    json!({ "model": config.model, "messages": messages, "stream": true })
}

fn anthropic_body(
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
    images: bool,
) -> Value {
    let mut system = context.system.iter().cloned().collect::<Vec<_>>();
    let mut messages = Vec::new();
    for (index, message) in history.iter().enumerate() {
        if message.role == crate::core::domain::Role::System {
            system.push(message.content.clone());
            continue;
        }
        let mut content = vec![json!({"type":"text", "text":message.content})];
        if images && index + 1 == history.len() && message.role == crate::core::domain::Role::User {
            content.extend(context.images.iter().map(|image| json!({
                "type":"image", "source":{"type":"base64", "media_type":"image/png", "data":image}
            })));
        }
        messages.push(json!({"role":role_name(message), "content":content}));
    }
    let mut body =
        json!({"model":config.model, "max_tokens":4096, "messages":messages, "stream":true});
    if !system.is_empty() {
        body["system"] = json!(system.join("\n\n"));
    }
    body
}

fn openai_message(message: &Message, images: &[String]) -> Value {
    if images.is_empty() {
        json!({ "role": role_name(message), "content": message.content })
    } else {
        let mut content = vec![json!({"type": "text", "text": message.content})];
        content.extend(images.iter().map(|image| json!({"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{image}")}})));
        json!({"role": role_name(message), "content": content})
    }
}

fn role_name(message: &Message) -> &'static str {
    match message.role {
        crate::core::domain::Role::User => "user",
        crate::core::domain::Role::Assistant => "assistant",
        crate::core::domain::Role::System => "system",
    }
}

/// Parse one SSE payload line into an event.
///
/// Accepts comments and unknown JSON as `None`, returns `Done` for the final
/// sentinel, and errors only on malformed JSON or an explicit provider error.
pub fn parse_sse_line(
    provider: &ChatProvider,
    line: &[u8],
) -> Result<Option<ChatEvent>, ChatError> {
    let text = std::str::from_utf8(line)
        .map_err(|_| ChatError::Protocol("stream contained invalid UTF-8".into()))?;
    let Some(payload) = text.strip_prefix("data:") else {
        return Ok(None);
    };
    let payload = payload.trim();
    if payload.is_empty() {
        return Ok(None);
    }
    if payload == "[DONE]" {
        return Ok(Some(ChatEvent::Done));
    }
    let value: Value = serde_json::from_str(payload)
        .map_err(|_| ChatError::Protocol("stream contained malformed JSON".into()))?;
    if let Some(error) = value.get("error").and_then(message_from_error) {
        return Err(ChatError::Protocol(error));
    }
    if provider.wire == ChatWire::Anthropic {
        return Ok(match value.get("type").and_then(Value::as_str) {
            Some("message_stop") => Some(ChatEvent::Done),
            Some("content_block_delta")
                if value.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta") =>
            {
                value
                    .pointer("/delta/text")
                    .and_then(Value::as_str)
                    .map(|text| ChatEvent::Delta(text.into()))
            }
            _ => None,
        });
    }
    let delta = value
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str);
    Ok(delta.map(|text| ChatEvent::Delta(text.to_owned())))
}

fn message_from_error(error: &Value) -> Option<String> {
    match error {
        Value::String(text) => Some(text.clone()),
        Value::Object(_) => error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

/// Stream a completion, invoking `on_text` for each delta.
///
/// Returns the assembled answer. Cancelling `cancel` ends the stream early and
/// returns whatever was produced so far.
pub async fn stream_chat(
    client: &reqwest::Client,
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
    cancel: Arc<watch::Receiver<bool>>,
    mut on_text: impl FnMut(&str),
) -> Result<String, ChatError> {
    stream_chat_bounded(client, config, history, context, cancel, |text| {
        on_text(&text);
        std::future::ready(())
    })
    .await
}

/// Async delivery lets a bounded UI queue apply backpressure instead of accumulating
/// one allocation per token while the foreground executor is busy.
pub async fn stream_chat_bounded<F: std::future::Future<Output = ()>>(
    client: &reqwest::Client,
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
    cancel: Arc<watch::Receiver<bool>>,
    mut on_text: impl FnMut(String) -> F,
) -> Result<String, ChatError> {
    let started = std::time::Instant::now();
    let mut first = true;
    let result = stream_chat_inner(client, config, history, context, cancel, |text| {
        if first {
            first = false;
            tracing::debug!(provider = %config.id, elapsed_ms = started.elapsed().as_millis(), "provider first text");
        }
        on_text(text)
    }).await.map_err(|error| {
        let redact = |text: String| if config.api_key.is_empty() { text } else { text.replace(&config.api_key, "[redacted]") };
        match error {
            ChatError::Request(error) => ChatError::Request(error.without_url()),
            ChatError::Status { status, message } => ChatError::Status { status, message: redact(message) },
            ChatError::Protocol(message) => ChatError::Protocol(redact(message)),
            error => error,
        }
    });
    tracing::debug!(provider = %config.id, elapsed_ms = started.elapsed().as_millis(), success = result.is_ok(), "provider request finished");
    result
}

/// Anthropic's native API takes `x-api-key`; everything else takes a bearer token.
fn with_key(
    request: reqwest::RequestBuilder,
    wire: ChatWire,
    key: &str,
) -> reqwest::RequestBuilder {
    if wire == ChatWire::Anthropic {
        request.header("x-api-key", key)
    } else {
        request.bearer_auth(key)
    }
}

async fn stream_chat_inner<F: std::future::Future<Output = ()>>(
    client: &reqwest::Client,
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
    cancel: Arc<watch::Receiver<bool>>,
    mut on_text: impl FnMut(String) -> F,
) -> Result<String, ChatError> {
    let definition = config.chat_definition().ok_or(ChatError::NotConfigured)?;
    let provider = &definition;
    let needs_key = config
        .custom
        .as_ref()
        .map_or(provider.requires_key, |c| c.auth != "none");
    if needs_key && config.api_key.trim().is_empty() {
        return Err(ChatError::MissingKey);
    }
    if provider.wire == ChatWire::Cli {
        return super::cli::stream(config, history, context, cancel, on_text).await;
    }
    let endpoint = config.endpoint().ok_or(ChatError::NotConfigured)?;
    if *cancel.borrow() {
        return Ok(String::new());
    }
    let mut cancellation = (*cancel).clone();

    let mut body = build_body(provider, config, history, context);
    let streaming = config.custom.as_ref().is_none_or(|custom| custom.streaming);
    body["stream"] = json!(streaming);
    let mut request = if let Some(custom) = &config.custom {
        let method = reqwest::Method::from_bytes(custom.method.as_bytes())
            .map_err(|_| ChatError::Protocol("invalid request method".into()))?;
        let mut request = client.request(method, endpoint);
        if let Some((header, value)) = custom.header(&config.api_key) {
            request = request.header(header, value);
        }
        request
    } else {
        with_key(client.post(endpoint), provider.wire, &config.api_key)
    };
    if provider.wire == ChatWire::Anthropic {
        request = request.header("anthropic-version", "2026-10-21");
    }
    request = request.json(&body);
    let response = tokio::select! {
        result = request.send() => result?,
        _ = wait_for_cancellation(&mut cancellation) => return Ok(String::new()),
    };

    let status = response.status();
    if !status.is_success() {
        let bytes = tokio::select! {
            result = crate::core::network::read_body(response, 16 * 1024) => result,
            _ = wait_for_cancellation(&mut cancellation) => return Ok(String::new()),
        };
        let message = bytes
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_else(|e| e.to_string());
        return Err(ChatError::Status {
            status: status.as_u16(),
            message,
        });
    }

    if !streaming {
        let bytes = tokio::select! {
            bytes = crate::core::network::read_body(response, 16 * 1024 * 1024) => bytes.map_err(|e| ChatError::Protocol(e.to_string()))?,
            _ = wait_for_cancellation(&mut cancellation) => return Ok(String::new()),
        };
        let text = String::from_utf8(bytes)
            .map_err(|_| ChatError::Protocol("response contained invalid UTF-8".into()))?;
        let answer = config
            .custom
            .as_ref()
            .expect("batch chat has custom settings")
            .text(&text)
            .map_err(ChatError::Protocol)?;
        if answer.len() > MAX_ANSWER_BYTES {
            return Err(ChatError::Protocol("answer exceeded the size limit".into()));
        }
        tokio::select! {
            _ = on_text(answer.clone()) => {},
            _ = wait_for_cancellation(&mut cancellation) => return Ok(String::new()),
        }
        return completed_answer(answer);
    }
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut answer = String::new();

    loop {
        let chunk = tokio::select! {
            chunk = stream.next() => match chunk { Some(chunk) => chunk, None => break },
            _ = wait_for_cancellation(&mut cancellation) => return Ok(answer),
        };
        let chunk = chunk?;
        buffer.extend_from_slice(&chunk);

        let mut consumed = 0;
        while let Some(offset) = buffer[consumed..].iter().position(|byte| *byte == b'\n') {
            let newline = consumed + offset;
            let line = &buffer[consumed..newline];
            if line.len() > MAX_SSE_LINE_BYTES {
                return Err(ChatError::Protocol(
                    "stream event exceeded the size limit".into(),
                ));
            }
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            consumed = newline + 1;
            match parse_sse_line(provider, line) {
                Ok(Some(ChatEvent::Delta(text))) => {
                    if text.len() > MAX_ANSWER_BYTES.saturating_sub(answer.len()) {
                        return Err(ChatError::Protocol("answer exceeded the size limit".into()));
                    }
                    answer.push_str(&text);
                    tokio::select! {
                        _ = on_text(text) => {},
                        _ = wait_for_cancellation(&mut cancellation) => return Ok(answer),
                    }
                }
                Ok(Some(ChatEvent::Done)) => return completed_answer(answer),
                Ok(None) => {}
                Err(error) => return Err(error),
            }
        }
        buffer.drain(..consumed);
        if buffer.len() > MAX_SSE_LINE_BYTES {
            return Err(ChatError::Protocol(
                "stream event exceeded the size limit".into(),
            ));
        }
    }

    // A final line without a newline still counts and must surface errors.
    if !buffer.is_empty() {
        match parse_sse_line(provider, &buffer)? {
            Some(ChatEvent::Delta(text)) => {
                if text.len() > MAX_ANSWER_BYTES.saturating_sub(answer.len()) {
                    return Err(ChatError::Protocol("answer exceeded the size limit".into()));
                }
                answer.push_str(&text);
                tokio::select! {
                    _ = on_text(text) => {},
                    _ = wait_for_cancellation(&mut cancellation) => return Ok(answer),
                }
            }
            Some(ChatEvent::Done) => return completed_answer(answer),
            None => {}
        }
    }
    if answer.trim().is_empty() {
        completed_answer(answer)
    } else {
        Err(ChatError::Protocol(
            "the stream disconnected before confirming completion; the partial answer was kept"
                .into(),
        ))
    }
}

pub(super) async fn wait_for_cancellation(cancel: &mut watch::Receiver<bool>) {
    loop {
        if *cancel.borrow() {
            return;
        }
        if cancel.changed().await.is_err() {
            // A caller with no Stop control can drop its sender. That is not a
            // cancellation request; avoid closing valid streams immediately.
            std::future::pending::<()>().await;
        }
    }
}

fn completed_answer(answer: String) -> Result<String, ChatError> {
    if answer.trim().is_empty() {
        Err(ChatError::Protocol(
            "the stream ended without an answer".into(),
        ))
    } else {
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::Message;

    fn context(system: &str) -> TurnContext {
        TurnContext {
            system: Some(system.into()),
            images: vec![],
        }
    }

    #[test]
    fn openai_body_puts_system_first_and_streams() {
        let provider = ChatProvider::by_id("gemini").unwrap();
        let config = ProviderConfig::new("gemini", "k", "gpt-4o");
        let history = [Message::user("hi"), Message::assistant("hello")];
        let body = build_body(provider, &config, &history, &context("be brief"));
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "be brief");
        assert_eq!(body["messages"][2]["content"], "hello");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn image_attaches_to_openai_content_array() {
        let provider = ChatProvider::by_id("gemini").unwrap();
        let config = ProviderConfig::new("gemini", "k", "gpt-4o");
        let history = [Message::user("what is this")];
        let context = TurnContext {
            system: None,
            images: vec!["QUJD".into(), "REVG".into()],
        };
        let body = build_body(provider, &config, &history, &context);
        let content = &body["messages"][0]["content"];
        assert_eq!(content.as_array().unwrap().len(), 3);
        assert_eq!(content[2]["image_url"]["url"], "data:image/png;base64,REVG");
        assert_eq!(content[1]["image_url"]["url"], "data:image/png;base64,QUJD");
    }

    #[test]
    fn attachments_only_belong_to_the_current_user_turn() {
        let provider = ChatProvider::by_id("gemini").unwrap();
        let config = ProviderConfig::new("gemini", "k", "m");
        let body = build_body(
            provider,
            &config,
            &[
                Message::user("old"),
                Message::assistant("reply"),
                Message::user("look"),
            ],
            &TurnContext {
                system: None,
                images: vec!["QUJD".into()],
            },
        );
        assert!(body["messages"][0]["content"].is_string());
        assert!(body["messages"][1]["content"].is_string());
        assert!(body["messages"][2]["content"].is_array());
    }

    #[test]
    fn sse_parses_supported_streams_and_done() {
        let openai = ChatProvider::by_id("gemini").unwrap();
        assert_eq!(
            parse_sse_line(openai, br#"data: {"choices":[{"delta":{"content":"hi"}}]}"#).unwrap(),
            Some(ChatEvent::Delta("hi".into()))
        );
        assert_eq!(
            parse_sse_line(openai, b"data: [DONE]").unwrap(),
            Some(ChatEvent::Done)
        );
        assert_eq!(parse_sse_line(openai, b": keep-alive").unwrap(), None);
        assert_eq!(parse_sse_line(openai, b"").unwrap(), None);
    }

    #[test]
    fn sse_surfaces_provider_errors() {
        let openai = ChatProvider::by_id("gemini").unwrap();
        assert!(matches!(
            parse_sse_line(openai, br#"data: {"error":"quota"}"#),
            Err(ChatError::Protocol(_))
        ));
        assert!(matches!(
            parse_sse_line(openai, b"data: {broken"),
            Err(ChatError::Protocol(_))
        ));
    }

    #[test]
    fn anthropic_wire_sends_x_api_key_not_bearer() {
        let client = reqwest::Client::new();
        let request = with_key(
            client.post("https://example.test"),
            ChatWire::Anthropic,
            "sk-a",
        )
        .build()
        .unwrap();
        assert_eq!(request.headers()["x-api-key"], "sk-a");
        assert!(request.headers().get("authorization").is_none());
        let request = with_key(
            client.post("https://example.test"),
            ChatWire::OpenAi,
            "sk-a",
        )
        .build()
        .unwrap();
        assert_eq!(request.headers()["authorization"], "Bearer sk-a");
    }

    #[test]
    fn missing_key_is_reported_before_network() {
        let config = ProviderConfig::new("gemini", "", "gpt-4o");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (_tx, rx) = watch::channel(false);
        let result = runtime.block_on(stream_chat(
            &http_client(),
            &config,
            &[Message::user("hi")],
            &TurnContext::default(),
            Arc::new(rx),
            |_| {},
        ));
        assert!(matches!(result, Err(ChatError::MissingKey)));
    }
}
