use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatWire {
    OpenAi,
    Anthropic,
    Cli,
}

#[derive(Debug, Clone, Copy)]
pub struct ChatProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub endpoint: &'static str,
    pub wire: ChatWire,
    pub supports_images: bool,
    pub requires_key: bool,
}

pub const CHAT_PROVIDERS: &[ChatProvider] = &[
    ChatProvider {
        id: "claude-sub",
        name: "Claude (subscription)",
        endpoint: "",
        wire: ChatWire::Cli,
        supports_images: true,
        requires_key: false,
    },
    ChatProvider {
        id: "agy-sub",
        name: "Antigravity (subscription)",
        endpoint: "",
        wire: ChatWire::Cli,
        supports_images: false,
        requires_key: false,
    },
    ChatProvider {
        id: "opencode",
        name: "OpenCode",
        endpoint: "",
        wire: ChatWire::Cli,
        supports_images: false,
        requires_key: false,
    },
    ChatProvider {
        id: "codex-sub",
        name: "ChatGPT / Codex (subscription)",
        endpoint: "",
        wire: ChatWire::Cli,
        supports_images: true,
        requires_key: false,
    },
    ChatProvider {
        id: "ollama-cloud",
        name: "Ollama Cloud",
        endpoint: "https://ollama.com/v1/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
        requires_key: true,
    },
    ChatProvider {
        id: "ollama",
        name: "Ollama (local)",
        endpoint: "http://127.0.0.1:11434/v1/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
        requires_key: false,
    },
];

impl ChatProvider {
    pub fn by_id(id: &str) -> Option<&'static ChatProvider> {
        CHAT_PROVIDERS.iter().find(|entry| entry.id == id)
    }
}

/// `https` anywhere, `http` only for loopback hosts.
pub fn endpoint_allowed(url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    match url.scheme() {
        "https" => true,
        "http" => matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttWire {
    RawAudio,
    InProcess,
    OpenAiMultipart,
}

#[derive(Debug, Clone, Copy)]
pub struct SttProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub endpoint: &'static str,
    pub wire: SttWire,
    pub default_model: &'static str,
}

pub const STT_PROVIDERS: &[SttProvider] = &[
    SttProvider {
        id: "deepgram-stt",
        name: "Deepgram",
        endpoint: "https://api.deepgram.com/v1/listen",
        wire: SttWire::RawAudio,
        default_model: "nova-3",
    },
    SttProvider {
        id: "parakeet-local",
        name: "Parakeet (local)",
        endpoint: "",
        wire: SttWire::InProcess,
        default_model: "parakeet-tdt-0.6b-v3",
    },
    SttProvider {
        id: "openai-compatible-stt",
        name: "OpenAI-compatible speech",
        endpoint: "",
        wire: SttWire::OpenAiMultipart,
        default_model: "",
    },
];

impl SttProvider {
    pub fn by_id(id: &str) -> Option<&'static SttProvider> {
        STT_PROVIDERS.iter().find(|entry| entry.id == id)
    }

    pub fn requires_key(&self) -> bool {
        self.wire == SttWire::RawAudio
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    pub id: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<super::custom::CustomProvider>,
    #[serde(default)]
    pub language: String,
}

impl ProviderConfig {
    pub fn new(
        id: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            api_key: api_key.into(),
            model: model.into(),
            base_url: String::new(),
            custom: None,
            language: String::new(),
        }
    }

    pub fn endpoint(&self) -> Option<String> {
        if self.custom.is_some() {
            return (self.id.starts_with("custom-") && !self.base_url.is_empty())
                .then(|| self.base_url.clone());
        }
        let provider = ChatProvider::by_id(&self.id)?;
        if provider.wire == ChatWire::Cli {
            return Some(String::new());
        }
        if !self.base_url.is_empty() {
            return endpoint_allowed(&self.base_url).then(|| self.base_url.clone());
        }
        Some(provider.endpoint.to_owned())
    }

    pub fn is_usable(&self) -> bool {
        if let Some(custom) = &self.custom {
            return self.endpoint().is_some()
                && !self.model.trim().is_empty()
                && (custom.auth == "none" || !self.api_key.trim().is_empty())
                && custom.validate(false).is_ok();
        }
        self.endpoint().is_some()
            && !self.model.trim().is_empty()
            && ChatProvider::by_id(&self.id)
                .is_some_and(|p| !p.requires_key || !self.api_key.trim().is_empty())
    }

    pub fn speech_usable(&self) -> bool {
        self.speech_usable_in(crate::speech::stt::parakeet::model_dir())
    }

    fn speech_usable_in(&self, parakeet_dir: Option<std::path::PathBuf>) -> bool {
        if let Some(custom) = &self.custom {
            return self.id.starts_with("custom-")
                && !self.base_url.is_empty()
                && (custom.auth == "none" || !self.api_key.trim().is_empty())
                && custom.validate(true).is_ok();
        }
        let Some(provider) = SttProvider::by_id(&self.id) else {
            return false;
        };
        if provider.wire == SttWire::InProcess {
            return crate::speech::stt::parakeet::model_ready_in(parakeet_dir);
        }
        (!provider.requires_key() || !self.api_key.trim().is_empty())
            && (provider.wire != SttWire::OpenAiMultipart
                || (endpoint_allowed(&self.base_url) && !self.model.trim().is_empty()))
    }

    pub fn speech_streaming(&self) -> bool {
        match &self.custom {
            Some(custom) => custom.streaming,
            None => SttProvider::by_id(&self.id).is_none_or(|p| p.wire == SttWire::RawAudio),
        }
    }

    pub fn chat_definition(&self) -> Option<ChatProvider> {
        if let Some(custom) = &self.custom {
            return self.id.starts_with("custom-").then(|| ChatProvider {
                id: "custom",
                name: "Custom",
                endpoint: "",
                wire: if custom.request_format == "anthropic" {
                    ChatWire::Anthropic
                } else {
                    ChatWire::OpenAi
                },
                supports_images: custom.supports_images,
                requires_key: custom.auth != "none",
            });
        }
        ChatProvider::by_id(&self.id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_identity_and_order() {
        let ids: Vec<_> = CHAT_PROVIDERS.iter().map(|p| p.id).collect();
        assert_eq!(
            ids,
            [
                "claude-sub",
                "agy-sub",
                "opencode",
                "codex-sub",
                "ollama-cloud",
                "ollama"
            ]
        );
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
        for p in CHAT_PROVIDERS {
            if p.wire != ChatWire::Cli {
                assert!(p.endpoint.starts_with("http"));
                if p.id != "ollama" {
                    assert!(p.endpoint.starts_with("https://"));
                }
            }
        }
        assert_eq!(
            ChatProvider::by_id("agy-sub").unwrap().name,
            "Antigravity (subscription)"
        );
        assert!(ChatProvider::by_id("agy-sub ").is_none());
        assert!(ChatProvider::by_id("legacy-chat").is_none());
        assert_eq!(STT_PROVIDERS.len(), 3);
        assert_eq!(
            SttProvider::by_id("deepgram-stt").unwrap().default_model,
            "nova-3"
        );
        assert!(SttProvider::by_id("retired-speech").is_none());
    }

    #[test]
    fn endpoint_and_model_rules() {
        let mut config = ProviderConfig::new("ollama", "", "sample-model");
        assert!(config.is_usable());
        config.model = "  ".into();
        assert!(!config.is_usable());
        config.model = "sample-model".into();
        config.base_url = "http://localhost:7777/v1/chat/completions".into();
        assert_eq!(
            config.endpoint().as_deref(),
            Some("http://localhost:7777/v1/chat/completions")
        );
        let unknown = ProviderConfig::new("retired-chat", "key", "model");
        assert!(unknown.endpoint().is_none() && !unknown.is_usable());
    }

    #[test]
    fn keyless_providers_only_need_a_model() {
        for id in ["ollama", "claude-sub", "codex-sub", "agy-sub", "opencode"] {
            assert!(ProviderConfig::new(id, "", "m").is_usable(), "{id}");
        }
    }

    #[test]
    fn plain_http_is_loopback_only() {
        for ok in [
            "https://example.com/v1",
            "http://localhost:1/v1",
            "http://127.0.0.1/x",
            "http://[::1]:11434/x",
        ] {
            assert!(endpoint_allowed(ok), "{ok}");
        }
        for bad in [
            "http://example.com/v1",
            "http://192.168.1.5/v1",
            "ftp://localhost/x",
            "nope",
        ] {
            assert!(!endpoint_allowed(bad), "{bad}");
        }
        let mut config = ProviderConfig::new("ollama", "k", "m");
        config.base_url = "http://evil.example/v1/chat/completions".into();
        assert!(config.endpoint().is_none());
        assert!(!config.is_usable());
    }

    #[test]
    fn parakeet_is_ready_only_with_its_model_files() {
        let dir = std::env::temp_dir().join(format!("parakeet-ready-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = ProviderConfig::new("parakeet-local", "", "");
        let provider = SttProvider::by_id("parakeet-local").unwrap();
        assert!(!provider.requires_key());
        assert!(!config.speech_usable_in(None));
        assert!(
            !config.speech_usable_in(Some(dir.clone())),
            "empty directory"
        );
        for f in [
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "joiner.int8.onnx",
            "tokens.txt",
        ] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        assert!(config.speech_usable_in(Some(dir.clone())) && !config.speech_streaming());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
