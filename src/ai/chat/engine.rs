//! HTTP and CLI chat turn execution with SSE framing, backpressure, and cancellation.

use std::future::Future;
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::ai::providers::{ChatProvider, ChatWire, ProviderConfig};
use crate::core::domain::{Message, Role};

const ERROR_BODY_LIMIT: usize = 16 * 1024;
const BATCH_BODY_LIMIT: usize = 16 * 1024 * 1024;
const SSE_LINE_LIMIT: usize = 256 * 1024;
const ANSWER_BYTE_LIMIT: usize = 2 * 1024 * 1024;
const ANTHROPIC_API_VERSION: &str = "2026-10-21";
const ANTHROPIC_MAX_TOKENS: u32 = 4096;

/// Failure modes produced while preparing or streaming a chat turn.
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("chat provider is not configured")]
    NotConfigured,
    #[error("API key is required for this chat provider")]
    MissingKey,
    #[error("network request failed: {0}")]
    Request(reqwest::Error),
    #[error("provider returned HTTP {status}: {message}")]
    Status { status: u16, message: String },
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("{0}")]
    Cli(String),
}

/// Per-turn prompt and image attachments supplied alongside the conversation history.
#[derive(Debug, Clone, Default)]
pub struct TurnContext {
    pub system: Option<String>,
    pub images: Vec<String>,
}

pub fn loading_http_client() -> reqwest::Client {
    crate::core::network::loading_chat_client()
}

/// Returns the shared HTTP client configured for standard chat turns.
pub fn http_client() -> reqwest::Client {
    crate::core::network::chat_client()
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

pub(super) async fn wait_for_cancellation(cancel: &mut watch::Receiver<bool>) {
    while !*cancel.borrow_and_update() {
        if cancel.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

/// Streams a chat turn and invokes `on_text` synchronously for each text delta.
pub async fn stream_chat(
    client: &reqwest::Client,
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
    cancel: Arc<watch::Receiver<bool>>,
    mut on_text: impl FnMut(&str),
) -> Result<String, ChatError> {
    stream_chat_bounded(client, config, history, context, cancel, |delta| {
        on_text(&delta);
        std::future::ready(())
    })
    .await
}

/// Streams a chat turn and awaits `on_text` for each text delta before reading further.
pub async fn stream_chat_bounded<F: Future<Output = ()>>(
    client: &reqwest::Client,
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
    cancel: Arc<watch::Receiver<bool>>,
    on_text: impl FnMut(String) -> F,
) -> Result<String, ChatError> {
    let started = Instant::now();
    let outcome = execute_turn(client, config, history, context, cancel, on_text)
        .await
        .map_err(|err| scrub_error(err, &config.api_key));
    tracing::debug!(
        provider = %config.id,
        elapsed_ms = started.elapsed().as_millis() as u64,
        ok = outcome.is_ok(),
        "chat turn finished"
    );
    outcome
}

async fn execute_turn<F: Future<Output = ()>>(
    client: &reqwest::Client,
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
    cancel: Arc<watch::Receiver<bool>>,
    mut on_text: impl FnMut(String) -> F,
) -> Result<String, ChatError> {
    let provider = config.chat_definition().ok_or(ChatError::NotConfigured)?;
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

    let streaming = config.custom.as_ref().is_none_or(|c| c.streaming);
    let payload = encode_request_payload(provider, config, history, context, streaming);

    let mut builder = if let Some(custom) = &config.custom {
        let method = reqwest::Method::from_bytes(custom.method.as_bytes()).map_err(|_| {
            ChatError::Protocol(format!("unsupported HTTP method '{}'", custom.method))
        })?;
        let mut req = client.request(method, endpoint);
        if let Some((header_name, header_val)) = custom.header(&config.api_key) {
            req = req.header(header_name, header_val);
        }
        req
    } else {
        with_key(client.post(endpoint), provider.wire, &config.api_key)
    };

    if provider.wire == ChatWire::Anthropic {
        builder = builder.header("anthropic-version", ANTHROPIC_API_VERSION);
    }
    let request = builder.json(&payload);

    let mut cancel_rx = (*cancel).clone();
    let response = tokio::select! {
        sent = request.send() => sent.map_err(ChatError::Request)?,
        _ = wait_for_cancellation(&mut cancel_rx) => return Ok(String::new()),
    };

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let raw_body = tokio::select! {
            read = crate::core::network::read_body(response, ERROR_BODY_LIMIT) => read,
            _ = wait_for_cancellation(&mut cancel_rx) => return Ok(String::new()),
        };
        let message = match raw_body {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(err) => err.to_string(),
        };
        return Err(ChatError::Status { status, message });
    }

    if let Some(custom) = config.custom.as_ref().filter(|c| !c.streaming) {
        let bytes = tokio::select! {
            read = crate::core::network::read_body(response, BATCH_BODY_LIMIT) => {
                read.map_err(|err| ChatError::Protocol(err.to_string()))?
            }
            _ = wait_for_cancellation(&mut cancel_rx) => return Ok(String::new()),
        };
        let text_body = String::from_utf8(bytes)
            .map_err(|_| ChatError::Protocol("response body was not valid UTF-8".into()))?;
        let extracted = custom.text(&text_body).map_err(ChatError::Protocol)?;
        if extracted.len() > ANSWER_BYTE_LIMIT {
            return Err(ChatError::Protocol(
                "response text exceeded the size limit".into(),
            ));
        }
        tokio::select! {
            () = on_text(extracted.clone()) => {}
            _ = wait_for_cancellation(&mut cancel_rx) => return Ok(String::new()),
        }
        return Ok(extracted);
    }

    let mut byte_stream = response.bytes_stream();
    let mut pending = Vec::new();
    let mut assembled = String::new();
    let started = Instant::now();
    let mut logged_first_delta = false;

    loop {
        let maybe_chunk = tokio::select! {
            item = byte_stream.next() => item,
            _ = wait_for_cancellation(&mut cancel_rx) => return Ok(assembled),
        };
        let Some(chunk_res) = maybe_chunk else {
            break;
        };
        let chunk = chunk_res.map_err(ChatError::Request)?;
        pending.extend_from_slice(&chunk);

        while let Some(nl) = pending.iter().position(|&b| b == b'\n') {
            let mut raw_line: Vec<u8> = pending.drain(..=nl).collect();
            if raw_line.last() == Some(&b'\n') {
                raw_line.pop();
            }
            if raw_line.last() == Some(&b'\r') {
                raw_line.pop();
            }
            if raw_line.len() > SSE_LINE_LIMIT {
                return Err(ChatError::Protocol(
                    "SSE line exceeded the size limit".into(),
                ));
            }
            match decode_sse_line(provider.wire, &raw_line)? {
                SseOutcome::Ignore => {}
                SseOutcome::Delta(piece) => {
                    if piece.is_empty() {
                        continue;
                    }
                    if piece.len() > ANSWER_BYTE_LIMIT.saturating_sub(assembled.len()) {
                        return Err(ChatError::Protocol(
                            "streamed answer exceeded the size limit".into(),
                        ));
                    }
                    if !logged_first_delta {
                        logged_first_delta = true;
                        tracing::debug!(
                            provider = %config.id,
                            ttft_ms = started.elapsed().as_millis() as u64,
                            "received first chat delta"
                        );
                    }
                    assembled.push_str(&piece);
                    tokio::select! {
                        () = on_text(piece) => {}
                        _ = wait_for_cancellation(&mut cancel_rx) => return Ok(assembled),
                    }
                }
                SseOutcome::Done => {
                    if assembled.trim().is_empty() {
                        return Err(ChatError::Protocol(
                            "stream completed without an answer".into(),
                        ));
                    }
                    return Ok(assembled);
                }
            }
        }

        if pending.len() > SSE_LINE_LIMIT {
            return Err(ChatError::Protocol(
                "unterminated SSE line exceeded the size limit".into(),
            ));
        }
    }

    if !pending.is_empty() {
        if pending.last() == Some(&b'\r') {
            pending.pop();
        }
        if pending.len() > SSE_LINE_LIMIT {
            return Err(ChatError::Protocol(
                "trailing SSE line exceeded the size limit".into(),
            ));
        }
        match decode_sse_line(provider.wire, &pending)? {
            SseOutcome::Ignore => {}
            SseOutcome::Delta(piece) => {
                if !piece.is_empty() {
                    if piece.len() > ANSWER_BYTE_LIMIT.saturating_sub(assembled.len()) {
                        return Err(ChatError::Protocol(
                            "streamed answer exceeded the size limit".into(),
                        ));
                    }
                    assembled.push_str(&piece);
                    tokio::select! {
                        () = on_text(piece) => {}
                        _ = wait_for_cancellation(&mut cancel_rx) => return Ok(assembled),
                    }
                }
            }
            SseOutcome::Done => {
                if assembled.trim().is_empty() {
                    return Err(ChatError::Protocol(
                        "stream completed without an answer".into(),
                    ));
                }
                return Ok(assembled);
            }
        }
    }

    if assembled.trim().is_empty() {
        Err(ChatError::Protocol(
            "stream closed without an answer".into(),
        ))
    } else {
        Err(ChatError::Protocol(
            "stream ended before confirming completion; partial text was kept".into(),
        ))
    }
}

fn encode_request_payload(
    provider: ChatProvider,
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
    stream: bool,
) -> Value {
    let attach_images = provider.supports_images
        && !context.images.is_empty()
        && history.last().is_some_and(|m| m.role == Role::User);
    let last_index = history.len().saturating_sub(1);

    if provider.wire == ChatWire::Anthropic {
        let mut system_parts: Vec<&str> = Vec::new();
        if let Some(sys) = context.system.as_deref().filter(|s| !s.is_empty()) {
            system_parts.push(sys);
        }
        for msg in history.iter().filter(|m| m.role == Role::System) {
            if !msg.content.is_empty() {
                system_parts.push(&msg.content);
            }
        }
        let mut messages = Vec::new();
        for (idx, msg) in history.iter().enumerate() {
            if msg.role == Role::System {
                continue;
            }
            let role_label = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => unreachable!(),
            };
            let mut blocks = vec![json!({ "type": "text", "text": msg.content })];
            if attach_images && idx == last_index {
                for img in &context.images {
                    blocks.push(json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": "image/png",
                            "data": img,
                        }
                    }));
                }
            }
            messages.push(json!({
                "role": role_label,
                "content": blocks,
            }));
        }
        let mut body = json!({
            "model": config.model,
            "max_tokens": ANTHROPIC_MAX_TOKENS,
            "messages": messages,
            "stream": stream,
        });
        if !system_parts.is_empty() {
            body["system"] = Value::String(system_parts.join("\n\n"));
        }
        body
    } else {
        let mut messages = Vec::new();
        if let Some(sys) = &context.system {
            messages.push(json!({
                "role": "system",
                "content": sys,
            }));
        }
        for (idx, msg) in history.iter().enumerate() {
            let role_label = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };
            let content = if attach_images && idx == last_index {
                let mut parts = vec![json!({ "type": "text", "text": msg.content })];
                for img in &context.images {
                    parts.push(json!({
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:image/png;base64,{img}"),
                        }
                    }));
                }
                Value::Array(parts)
            } else {
                Value::String(msg.content.clone())
            };
            messages.push(json!({
                "role": role_label,
                "content": content,
            }));
        }
        if stream {
            json!({ "model": config.model, "messages": messages, "stream": true })
        } else {
            json!({ "model": config.model, "messages": messages, "stream": false })
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SseOutcome {
    Ignore,
    Delta(String),
    Done,
}

fn decode_sse_line(wire: ChatWire, raw_line: &[u8]) -> Result<SseOutcome, ChatError> {
    let text_line = std::str::from_utf8(raw_line)
        .map_err(|_| ChatError::Protocol("SSE stream contained invalid UTF-8".into()))?;
    let Some(after_prefix) = text_line.strip_prefix("data:") else {
        return Ok(SseOutcome::Ignore);
    };
    let payload = after_prefix.trim();
    if payload.is_empty() {
        return Ok(SseOutcome::Ignore);
    }
    if payload == "[DONE]" {
        return Ok(SseOutcome::Done);
    }
    let parsed: Value = serde_json::from_str(payload)
        .map_err(|err| ChatError::Protocol(format!("malformed SSE JSON: {err}")))?;

    if let Some(err_field) = parsed.get("error")
        && let Some(msg) = err_field
            .as_str()
            .or_else(|| err_field.get("message").and_then(Value::as_str))
    {
        return Err(ChatError::Protocol(msg.to_owned()));
    }

    match wire {
        ChatWire::OpenAi => {
            if let Some(delta) = parsed
                .pointer("/choices/0/delta/content")
                .and_then(Value::as_str)
            {
                Ok(SseOutcome::Delta(delta.to_owned()))
            } else {
                Ok(SseOutcome::Ignore)
            }
        }
        ChatWire::Anthropic => match parsed.get("type").and_then(Value::as_str) {
            Some("message_stop") => Ok(SseOutcome::Done),
            Some("content_block_delta")
                if parsed.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta") =>
            {
                if let Some(delta) = parsed.pointer("/delta/text").and_then(Value::as_str) {
                    Ok(SseOutcome::Delta(delta.to_owned()))
                } else {
                    Ok(SseOutcome::Ignore)
                }
            }
            _ => Ok(SseOutcome::Ignore),
        },
        ChatWire::Cli => Ok(SseOutcome::Ignore),
    }
}

fn scrub_error(err: ChatError, api_key: &str) -> ChatError {
    let mask = |input: String| -> String {
        if api_key.is_empty() {
            return input;
        }
        let replaced = input.replace(api_key, "[redacted]");
        let trimmed = api_key.trim();
        if !trimmed.is_empty() && trimmed != api_key {
            replaced.replace(trimmed, "[redacted]")
        } else {
            replaced
        }
    };
    match err {
        ChatError::Request(inner) => ChatError::Request(inner.without_url()),
        ChatError::Status { status, message } => ChatError::Status {
            status,
            message: mask(message),
        },
        ChatError::Protocol(message) => ChatError::Protocol(mask(message)),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_payload_places_system_prompt_before_history_with_streaming() {
        let provider = ChatProvider::by_id("ollama").unwrap();
        let config = ProviderConfig::new("ollama", "k", "gpt-4o");
        let history = [
            Message::user("What is ownership?"),
            Message::assistant("Each value has one owner."),
        ];
        let ctx = TurnContext {
            system: Some("Reply tersely.".into()),
            images: Vec::new(),
        };
        let body = encode_request_payload(*provider, &config, &history, &ctx, true);
        assert_eq!(body["stream"], true);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(
            msgs[0],
            json!({"role": "system", "content": "Reply tersely."})
        );
        assert_eq!(
            msgs[1],
            json!({"role": "user", "content": "What is ownership?"})
        );
        assert_eq!(
            msgs[2],
            json!({"role": "assistant", "content": "Each value has one owner."})
        );
    }

    #[test]
    fn openai_payload_formats_multiple_images_on_single_user_turn() {
        let provider = ChatProvider::by_id("ollama").unwrap();
        let config = ProviderConfig::new("ollama", "k", "gpt-4o");
        let history = [Message::user("Inspect these screenshots")];
        let ctx = TurnContext {
            system: None,
            images: vec!["IMG_ONE".into(), "IMG_TWO".into()],
        };
        let body = encode_request_payload(*provider, &config, &history, &ctx, true);
        let parts = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(
            parts[0],
            json!({"type": "text", "text": "Inspect these screenshots"})
        );
        assert_eq!(
            parts[1],
            json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,IMG_ONE"}})
        );
        assert_eq!(
            parts[2],
            json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,IMG_TWO"}})
        );
    }

    #[test]
    fn openai_payload_attaches_images_only_to_final_user_turn() {
        let provider = ChatProvider::by_id("ollama").unwrap();
        let config = ProviderConfig::new("ollama", "k", "m");
        let history = [
            Message::user("Earlier question"),
            Message::assistant("Earlier answer"),
            Message::user("Latest question with image"),
        ];
        let ctx = TurnContext {
            system: None,
            images: vec!["BASE64_PNG".into()],
        };
        let body = encode_request_payload(*provider, &config, &history, &ctx, true);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["content"], "Earlier question");
        assert_eq!(msgs[1]["content"], "Earlier answer");
        assert!(msgs[2]["content"].is_array());
    }

    #[test]
    fn openai_sse_recognizes_deltas_done_and_skips_noise() {
        let openai = ChatProvider::by_id("ollama").unwrap();
        assert_eq!(
            decode_sse_line(
                openai.wire,
                br#"data: {"choices":[{"delta":{"content":"chunk"}}]}"#
            )
            .unwrap(),
            SseOutcome::Delta("chunk".into())
        );
        assert_eq!(
            decode_sse_line(openai.wire, b"data: [DONE]").unwrap(),
            SseOutcome::Done
        );
        assert_eq!(
            decode_sse_line(openai.wire, b": keepalive").unwrap(),
            SseOutcome::Ignore
        );
        assert_eq!(
            decode_sse_line(openai.wire, b"").unwrap(),
            SseOutcome::Ignore
        );
    }

    #[test]
    fn sse_surfaces_error_fields_and_invalid_json_as_protocol_errors() {
        let openai = ChatProvider::by_id("ollama").unwrap();
        let err = decode_sse_line(openai.wire, br#"data: {"error":"rate limit hit"}"#).unwrap_err();
        assert!(matches!(err, ChatError::Protocol(ref s) if s.contains("rate limit hit")));

        let bad = decode_sse_line(openai.wire, b"data: {unclosed").unwrap_err();
        assert!(matches!(bad, ChatError::Protocol(_)));
    }

    #[test]
    fn anthropic_wire_sends_x_api_key_not_bearer() {
        crate::core::install_tls_provider();
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

    #[tokio::test]
    async fn custom_bearer_provider_without_key_fails_before_connecting() {
        let mut config = ProviderConfig::new("custom-test", "", "gpt-4o");
        config.custom = Some(Default::default());
        config.base_url = "http://127.0.0.1:1/v1/chat/completions".into();
        let res = stream_chat(
            &http_client(),
            &config,
            &[Message::user("ping")],
            &TurnContext::default(),
            Arc::new(watch::channel(false).1),
            |_| {},
        )
        .await;
        assert!(matches!(res, Err(ChatError::MissingKey)));
    }

    #[test]
    fn anthropic_payload_merges_system_entries_and_formats_image_blocks() {
        let provider = ChatProvider {
            id: "anthropic-test",
            name: "Anthropic",
            endpoint: "https://example.test/v1/messages",
            wire: ChatWire::Anthropic,
            supports_images: true,
            requires_key: true,
        };
        let config = ProviderConfig::new("anthropic-test", "k", "claude-sonnet");
        let history = [
            Message {
                role: Role::System,
                content: "History system note.".into(),
                images: Vec::new(),
            },
            Message::user("Describe this diagram"),
        ];
        let ctx = TurnContext {
            system: Some("Context system note.".into()),
            images: vec!["RAW_PNG".into()],
        };
        let body = encode_request_payload(provider, &config, &history, &ctx, true);
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(
            body["system"],
            "Context system note.\n\nHistory system note."
        );
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        let blocks = msgs[0]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            blocks[0],
            json!({"type": "text", "text": "Describe this diagram"})
        );
        assert_eq!(
            blocks[1],
            json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": "RAW_PNG",
                }
            })
        );
    }

    #[test]
    fn anthropic_sse_extracts_text_deltas_ignores_thinking_and_stops() {
        let text_line = br#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"visible"}}"#;
        let thinking_line = br#"data: {"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"hidden"}}"#;
        let stop_line = br#"data: {"type":"message_stop"}"#;

        assert_eq!(
            decode_sse_line(ChatWire::Anthropic, text_line).unwrap(),
            SseOutcome::Delta("visible".into())
        );
        assert_eq!(
            decode_sse_line(ChatWire::Anthropic, thinking_line).unwrap(),
            SseOutcome::Ignore
        );
        assert_eq!(
            decode_sse_line(ChatWire::Anthropic, stop_line).unwrap(),
            SseOutcome::Done
        );
    }
}
