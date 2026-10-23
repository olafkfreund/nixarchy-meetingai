//! Explicit wire settings for user-created providers, encrypted with credentials.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomProvider {
    pub name: String,
    pub method: String,
    /// none, bearer, token, or header (send the key verbatim).
    pub auth: String,
    pub auth_header: String,
    /// openai, raw-wav, multipart-wav, or json-wav.
    pub request_format: String,
    /// json or text.
    pub response_format: String,
    pub response_path: String,
    pub streaming: bool,
    /// Speech streaming uses the Deepgram two-channel PCM WebSocket protocol.
    pub stream_url: String,
    pub supports_images: bool,
}

impl Default for CustomProvider {
    fn default() -> Self {
        Self {
            name: "Custom provider".into(),
            method: "POST".into(),
            auth: "bearer".into(),
            auth_header: "Authorization".into(),
            request_format: "openai".into(),
            response_format: "json".into(),
            response_path: "/choices/0/message/content".into(),
            streaming: false,
            stream_url: String::new(),
            supports_images: true,
        }
    }
}

impl CustomProvider {
    pub fn validate(&self, speech: bool) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Give this provider a name.".into());
        }
        if !matches!(self.method.as_str(), "POST" | "PUT" | "PATCH") {
            return Err("Choose POST, PUT or PATCH.".into());
        }
        if !matches!(self.auth.as_str(), "none" | "bearer" | "token" | "header") {
            return Err("Choose a supported authentication method.".into());
        }
        if self.auth != "none" {
            reqwest::header::HeaderName::from_bytes(self.auth_header.as_bytes())
                .map_err(|_| "Invalid authentication header name.")?;
        }
        if !(if speech {
            matches!(
                self.request_format.as_str(),
                "raw-wav" | "multipart-wav" | "json-wav"
            )
        } else {
            self.request_format == "openai"
        }) {
            return Err("Choose a supported request format.".into());
        }
        if !matches!(self.response_format.as_str(), "json" | "text") {
            return Err("Choose JSON or plain text responses.".into());
        }
        if self.response_format == "json" && !self.response_path.starts_with('/') {
            return Err("Use a JSON pointer such as /text for the response field.".into());
        }
        if self.streaming && speech {
            let url = reqwest::Url::parse(&self.stream_url)
                .map_err(|_| "Enter a WebSocket streaming endpoint.")?;
            if !matches!(url.scheme(), "ws" | "wss") {
                return Err(
                    "Streaming needs a ws:// or wss:// endpoint using the Deepgram protocol."
                        .into(),
                );
            }
        }
        if self.streaming
            && !speech
            && (self.response_format != "json"
                || self.response_path != "/choices/0/message/content")
        {
            return Err("Chat streaming requires OpenAI SSE responses. Disable streaming for a custom JSON path or plain text.".into());
        }
        Ok(())
    }

    pub fn text(&self, response: &str) -> Result<String, String> {
        let text = if self.response_format == "text" {
            response.to_owned()
        } else {
            let value: Value =
                serde_json::from_str(response).map_err(|_| "Response was not JSON.")?;
            value
                .pointer(&self.response_path)
                .and_then(Value::as_str)
                .ok_or("The response field did not contain text.")?
                .to_owned()
        };
        if text.trim().is_empty() {
            return Err("The provider returned empty text.".into());
        }
        Ok(text.trim().to_owned())
    }

    pub fn header(&self, key: &str) -> Option<(String, String)> {
        let value = match self.auth.as_str() {
            "none" => return None,
            "bearer" => format!("Bearer {key}"),
            "token" => format!("Token {key}"),
            _ => key.to_owned(),
        };
        Some((self.auth_header.clone(), value))
    }
}
