use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomProvider {
    pub name: String,
    pub method: String,
    pub auth: String,
    pub auth_header: String,
    pub request_format: String,
    pub response_format: String,
    pub response_path: String,
    pub streaming: bool,
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
            return Err("Enter a provider name.".into());
        }
        if !matches!(self.method.as_str(), "POST" | "PUT" | "PATCH") {
            return Err("Choose POST, PUT, or PATCH as the request method.".into());
        }
        if !matches!(self.auth.as_str(), "none" | "bearer" | "token" | "header") {
            return Err("Choose a supported authentication method.".into());
        }
        if self.auth != "none"
            && reqwest::header::HeaderName::from_bytes(self.auth_header.as_bytes()).is_err()
        {
            return Err("Enter a valid authentication header name.".into());
        }
        let format_ok = if speech {
            matches!(
                self.request_format.as_str(),
                "raw-wav" | "multipart-wav" | "json-wav"
            )
        } else {
            matches!(self.request_format.as_str(), "openai" | "anthropic")
        };
        if !format_ok {
            return Err("Choose a request format for this provider kind.".into());
        }
        if !matches!(self.response_format.as_str(), "json" | "text") {
            return Err("Choose JSON or plain text for the response format.".into());
        }
        if self.response_format == "json" && !self.response_path.starts_with('/') {
            return Err("Enter a JSON pointer for the response text.".into());
        }
        if self.streaming {
            if speech {
                let url = reqwest::Url::parse(&self.stream_url)
                    .map_err(|_| "Enter a WebSocket stream URL.".to_owned())?;
                if !matches!(url.scheme(), "ws" | "wss") {
                    return Err("Use a ws or wss stream URL.".into());
                }
            } else {
                let native = if self.request_format == "anthropic" {
                    "/content/0/text"
                } else {
                    "/choices/0/message/content"
                };
                if self.response_format != "json" || self.response_path != native {
                    return Err("Streaming chat needs its native JSON response path.".into());
                }
            }
        }
        Ok(())
    }

    pub fn text(&self, response: &str) -> Result<String, String> {
        let output = if self.response_format == "text" {
            response.to_owned()
        } else {
            let value: serde_json::Value = serde_json::from_str(response)
                .map_err(|_| "The provider returned invalid JSON.".to_owned())?;
            if self.request_format == "anthropic" && self.response_path == "/content/0/text" {
                value
                    .get("content")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| "The response has no content blocks.".to_owned())?
                    .iter()
                    .filter(|block| {
                        block.get("type").and_then(serde_json::Value::as_str) == Some("text")
                    })
                    .filter_map(|block| block.get("text").and_then(serde_json::Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                value
                    .pointer(&self.response_path)
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "The response text is missing.".to_owned())?
                    .to_owned()
            }
        };
        let output = output.trim();
        if output.is_empty() {
            Err("empty text".into())
        } else {
            Ok(output.to_owned())
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_covers_chat_and_speech() {
        let mut p = CustomProvider::default();
        assert!(p.validate(false).is_ok());
        assert!(p.validate(true).is_err());
        p.request_format = "multipart-wav".into();
        p.response_path = "/text".into();
        assert!(p.validate(true).is_ok());
        p.streaming = true;
        p.stream_url = "wss://speech.example/stream".into();
        assert!(p.validate(true).is_ok());
        p.stream_url = "https://speech.example/stream".into();
        assert!(p.validate(true).is_err());
        p.streaming = false;
        p.stream_url = "bad".into();
        assert!(p.validate(true).is_ok());
        p.request_format = "anthropic".into();
        p.streaming = true;
        p.response_path = "/content/0/text".into();
        assert!(p.validate(false).is_ok());
        p.response_path = "/text".into();
        assert!(p.validate(false).is_err());
        p.streaming = false;
        p.name = " ".into();
        assert!(p.validate(false).is_err());
        p.name = "Demo".into();
        p.method = "GET".into();
        assert!(p.validate(false).is_err());
        p.method = "POST".into();
        p.auth = "invalid".into();
        assert!(p.validate(false).is_err());
        p.auth = "bearer".into();
        p.auth_header = "bad header".into();
        assert!(p.validate(false).is_err());
        p.auth_header = "Authorization".into();
        p.response_format = "xml".into();
        assert!(p.validate(false).is_err());
    }

    #[test]
    fn extracts_text_and_headers() {
        let mut p = CustomProvider::default();
        assert_eq!(
            p.text(r#"{"choices":[{"message":{"content":" hello "}}]}"#)
                .unwrap(),
            "hello"
        );
        p.response_format = "text".into();
        assert_eq!(p.text(" plain ").unwrap(), "plain");
        assert_eq!(p.text("  ").unwrap_err(), "empty text");
        p.response_format = "json".into();
        p.request_format = "anthropic".into();
        p.response_path = "/content/0/text".into();
        assert_eq!(p.text(r#"{"content":[{"type":"thinking","text":"skip"},{"type":"text","text":"first"},{"type":"text","text":"second"}]}"#).unwrap(), "first\nsecond");
        assert_eq!(p.header("secret").unwrap().1, "Bearer secret");
        p.auth = "token".into();
        assert_eq!(p.header("secret").unwrap().1, "Token secret");
        p.auth = "header".into();
        assert_eq!(p.header("secret").unwrap().1, "secret");
        p.auth = "none".into();
        assert!(p.header("secret").is_none());
    }
}
