//! Chat completions over bring-your-own-key providers.
//!
//! Builds the vendor-specific request body, streams server-sent events and emits
//! text deltas as they arrive. Cancellation is cooperative via a watch channel so
//! a stopped stream stops consuming the socket promptly.

use std::sync::Arc;

use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::sync::watch;

use super::domain::Message;
use super::providers::{ChatProvider, ChatWire, ProviderConfig};

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
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .expect("a default HTTP client always builds")
}

/// Build the vendor request body.
pub fn build_body(
    provider: &ChatProvider,
    config: &ProviderConfig,
    history: &[Message],
    context: &TurnContext,
) -> Value {
    match provider.wire {
        ChatWire::OpenAi => {
            let mut messages: Vec<Value> = Vec::with_capacity(history.len() + 1);
            if let Some(system) = &context.system {
                messages.push(json!({ "role": "system", "content": system }));
            }
            for message in history {
                messages.push(openai_message(message, context.images.first()));
            }
            json!({
                "model": config.model,
                "messages": messages,
                "stream": true,
            })
        }
        ChatWire::Anthropic => {
            let mut messages: Vec<Value> = Vec::with_capacity(history.len());
            for (index, message) in history.iter().enumerate() {
                let image = if index == history.len() - 1 {
                    context.images.first()
                } else {
                    None
                };
                messages.push(anthropic_message(message, image));
            }
            let mut body = json!({
                "model": config.model,
                "messages": messages,
                "max_tokens": 4096,
                "stream": true,
            });
            if let Some(system) = &context.system {
                body["system"] = json!(system);
            }
            body
        }
        ChatWire::Cohere => {
            let mut messages: Vec<Value> = Vec::with_capacity(history.len() + 1);
            if let Some(system) = &context.system {
                messages.push(json!({ "role": "system", "content": system }));
            }
            for message in history {
                messages.push(openai_message(message, context.images.first()));
            }
            json!({
                "model": config.model,
                "messages": messages,
                "stream": true,
            })
        }
    }
}

fn openai_message(message: &Message, image: Option<&String>) -> Value {
    if let Some(image) = image {
        json!({
            "role": role_name(message),
            "content": [
                { "type": "text", "text": message.content },
                { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{image}") } }
            ]
        })
    } else {
        json!({ "role": role_name(message), "content": message.content })
    }
}

fn anthropic_message(message: &Message, image: Option<&String>) -> Value {
    if let Some(image) = image {
        json!({
            "role": role_name(message),
            "content": [
                { "type": "text", "text": message.content },
                { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": image } }
            ]
        })
    } else {
        json!({ "role": role_name(message), "content": message.content })
    }
}

fn role_name(message: &Message) -> &'static str {
    match message.role {
        super::domain::Role::User => "user",
        super::domain::Role::Assistant => "assistant",
        super::domain::Role::System => "system",
    }
}

/// Apply vendor auth headers.
fn authorise(
    request: reqwest::RequestBuilder,
    provider: &ChatProvider,
    config: &ProviderConfig,
) -> reqwest::RequestBuilder {
    match provider.wire {
        ChatWire::Anthropic => request
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01"),
        ChatWire::OpenAi | ChatWire::Cohere => {
            if config.api_key.is_empty() {
                request
            } else {
                request.bearer_auth(&config.api_key)
            }
        }
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
    let delta = match provider.wire {
        ChatWire::OpenAi | ChatWire::Cohere => value
            .pointer("/choices/0/delta/content")
            .and_then(Value::as_str)
            .or_else(|| {
                value
                    .pointer("/message/content/0/text")
                    .and_then(Value::as_str)
            })
            .or_else(|| {
                value
                    .pointer("/delta/message/content/0/text")
                    .and_then(Value::as_str)
            }),
        ChatWire::Anthropic => value
            .pointer("/delta/text")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/content_block/text").and_then(Value::as_str)),
    };
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
    let provider = ChatProvider::by_id(&config.id).ok_or(ChatError::NotConfigured)?;
    if provider.requires_key() && config.api_key.trim().is_empty() {
        return Err(ChatError::MissingKey);
    }
    let endpoint = config.endpoint().ok_or(ChatError::NotConfigured)?;

    let body = build_body(provider, config, history, context);
    let request = authorise(client.post(endpoint), provider, config).json(&body);
    let response = request.send().await?;

    let status = response.status();
    if !status.is_success() {
        let message = response.text().await.unwrap_or_default();
        return Err(ChatError::Status {
            status: status.as_u16(),
            message,
        });
    }

    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut answer = String::new();

    while let Some(chunk) = stream.next().await {
        if *cancel.borrow() {
            break;
        }
        let chunk = chunk?;
        buffer.extend_from_slice(&chunk);

        while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = buffer.drain(..=newline).collect::<Vec<_>>();
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            match parse_sse_line(provider, &line) {
                Ok(Some(ChatEvent::Delta(text))) => {
                    answer.push_str(&text);
                    on_text(&text);
                }
                Ok(Some(ChatEvent::Done)) => return Ok(answer),
                Ok(None) => {}
                Err(error) => return Err(error),
            }
        }
    }

    // A final line without a trailing newline still counts.
    if !buffer.is_empty()
        && let Ok(Some(ChatEvent::Delta(text))) = parse_sse_line(provider, &buffer)
    {
        answer.push_str(&text);
        on_text(&text);
    }
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::domain::Message;

    fn context(system: &str) -> TurnContext {
        TurnContext {
            system: Some(system.into()),
            images: vec![],
        }
    }

    #[test]
    fn openai_body_puts_system_first_and_streams() {
        let provider = ChatProvider::by_id("openai").unwrap();
        let config = ProviderConfig::new("openai", "k", "gpt-4o");
        let history = [Message::user("hi"), Message::assistant("hello")];
        let body = build_body(provider, &config, &history, &context("be brief"));
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "be brief");
        assert_eq!(body["messages"][2]["content"], "hello");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn anthropic_body_uses_top_level_system_and_max_tokens() {
        let provider = ChatProvider::by_id("claude").unwrap();
        let config = ProviderConfig::new("claude", "k", "claude-sonnet-4");
        let history = [Message::user("hi")];
        let body = build_body(provider, &config, &history, &context("sys"));
        assert_eq!(body["system"], "sys");
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn image_attaches_to_openai_content_array() {
        let provider = ChatProvider::by_id("openai").unwrap();
        let config = ProviderConfig::new("openai", "k", "gpt-4o");
        let history = [Message::user("what is this")];
        let context = TurnContext {
            system: None,
            images: vec!["QUJD".into()],
        };
        let body = build_body(provider, &config, &history, &context);
        let content = &body["messages"][0]["content"];
        assert!(content.is_array());
        assert_eq!(content[1]["image_url"]["url"], "data:image/png;base64,QUJD");
    }

    #[test]
    fn anthropic_image_uses_base64_source() {
        let provider = ChatProvider::by_id("claude").unwrap();
        let config = ProviderConfig::new("claude", "k", "m");
        let history = [Message::user("look")];
        let context = TurnContext {
            system: None,
            images: vec!["QUJD".into()],
        };
        let body = build_body(provider, &config, &history, &context);
        assert_eq!(body["messages"][0]["content"][1]["source"]["data"], "QUJD");
    }

    #[test]
    fn sse_parses_openai_and_anthropic_and_done() {
        let openai = ChatProvider::by_id("openai").unwrap();
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

        let claude = ChatProvider::by_id("claude").unwrap();
        assert_eq!(
            parse_sse_line(
                claude,
                br#"data: {"type":"content_block_delta","delta":{"text":"yo"}}"#
            )
            .unwrap(),
            Some(ChatEvent::Delta("yo".into()))
        );
    }

    #[test]
    fn sse_surfaces_provider_errors() {
        let openai = ChatProvider::by_id("openai").unwrap();
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
    fn missing_key_is_reported_before_network() {
        let config = ProviderConfig::new("openai", "", "gpt-4o");
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
