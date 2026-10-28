//! Provider catalog.
//!
//! Bring-your-own-key chat and speech-to-text providers. Endpoints, auth headers
//! and request shapes for Ollama, the subscription CLIs and Deepgram. Removed providers
//! are deliberately absent so old vault records cannot become active.

use serde::{Deserialize, Serialize};

/// How a chat provider authenticates and frames a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatWire {
    /// OpenAI-compatible `/chat/completions`.
    OpenAi,
    /// Native Anthropic Messages API.
    Anthropic,
    /// A subscription CLI (`claude` / `codex` / `agy`) driven as a subprocess.
    Cli,
}

/// A chat provider definition.
#[derive(Debug, Clone, Copy)]
pub struct ChatProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub endpoint: &'static str,
    pub wire: ChatWire,
    pub supports_images: bool,
    pub requires_key: bool,
}

/// Every bundled chat provider.
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

impl ChatProvider {
    pub fn by_id(id: &str) -> Option<&'static ChatProvider> {
        CHAT_PROVIDERS.iter().find(|provider| provider.id == id)
    }
}

/// How a speech provider uploads audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttWire {
    /// Raw `audio/wav` body for Deepgram.
    RawAudio,
    /// Multipart `POST /inference` to a local `whisper-server`; no key.
    WhisperServer,
    /// Decoded inside the daemon (Parakeet through sherpa-onnx); no URL, no key.
    InProcess,
    /// OpenAI-compatible multipart transcription.
    OpenAiMultipart,
}

/// A speech-to-text provider definition.
#[derive(Debug, Clone, Copy)]
pub struct SttProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub endpoint: &'static str,
    pub wire: SttWire,
    /// Default model identifier, when the provider needs one.
    pub default_model: &'static str,
}

/// Every bundled speech provider.
pub const STT_PROVIDERS: &[SttProvider] = &[
    SttProvider {
        id: "deepgram-stt",
        name: "Deepgram",
        endpoint: "https://api.deepgram.com/v1/listen",
        wire: SttWire::RawAudio,
        default_model: "nova-3",
    },
    SttProvider {
        id: "whisper-local",
        name: "whisper.cpp (local)",
        endpoint: "http://127.0.0.1:8178/inference",
        wire: SttWire::WhisperServer,
        default_model: "ggml-large-v3-turbo",
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
        STT_PROVIDERS.iter().find(|provider| provider.id == id)
    }

    pub fn requires_key(&self) -> bool {
        self.wire == SttWire::RawAudio
    }
}

/// A user-configured provider instance, as persisted in the vault.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    /// Supported catalog id. Retired providers cannot be used through an endpoint override.
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

    /// Endpoint to call, honouring a user override.
    pub fn endpoint(&self) -> Option<String> {
        if self.id.starts_with("custom-") && self.custom.is_some() {
            return (!self.base_url.is_empty()).then(|| self.base_url.clone());
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

    /// Whether this configuration is usable as-is.
    pub fn is_usable(&self) -> bool {
        if let Some(custom) = &self.custom {
            return self.endpoint().is_some()
                && !self.model.trim().is_empty()
                && (custom.auth == "none" || !self.api_key.trim().is_empty())
                && custom.validate(false).is_ok();
        }
        let needs_key = ChatProvider::by_id(&self.id)
            .map(|provider| provider.requires_key)
            .unwrap_or(true);
        self.endpoint().is_some()
            && !self.model.trim().is_empty()
            && (!needs_key || !self.api_key.trim().is_empty())
    }

    pub fn speech_usable(&self) -> bool {
        self.speech_usable_in(crate::speech::stt::parakeet::model_dir())
    }

    /// [`Self::speech_usable`] with the Parakeet model directory given.
    fn speech_usable_in(&self, parakeet_dir: Option<std::path::PathBuf>) -> bool {
        if let Some(custom) = &self.custom {
            self.id.starts_with("custom-")
                && !self.base_url.is_empty()
                && (custom.auth == "none" || !self.api_key.trim().is_empty())
                && custom.validate(true).is_ok()
        } else {
            SttProvider::by_id(&self.id).is_some_and(|p| {
                if p.wire == SttWire::InProcess {
                    return crate::speech::stt::parakeet::model_ready_in(parakeet_dir);
                }
                (!p.requires_key() || !self.api_key.trim().is_empty())
                    && (p.wire != SttWire::OpenAiMultipart
                        || (endpoint_allowed(&self.base_url) && !self.model.trim().is_empty()))
            })
        }
    }

    pub fn speech_streaming(&self) -> bool {
        match &self.custom {
            Some(custom) => custom.streaming,
            // Only Deepgram's raw-audio wire streams; whisper-server, OpenAI multipart and Parakeet are batch.
            None => SttProvider::by_id(&self.id).is_none_or(|p| p.wire == SttWire::RawAudio),
        }
    }

    pub fn chat_definition(&self) -> Option<ChatProvider> {
        if let Some(custom) = &self.custom {
            self.id.starts_with("custom-").then_some(ChatProvider {
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
            })
        } else {
            ChatProvider::by_id(&self.id).copied()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_has_unique_id_and_https_remote() {
        let mut ids = std::collections::HashSet::new();
        for provider in CHAT_PROVIDERS {
            assert!(ids.insert(provider.id), "duplicate id {}", provider.id);
            assert!(
                provider.wire == ChatWire::Cli || provider.endpoint.starts_with("http"),
                "{}",
                provider.id
            );
            if provider.wire != ChatWire::Cli && provider.id != "ollama" {
                assert!(
                    provider.endpoint.starts_with("https://"),
                    "{} must be https",
                    provider.id
                );
            }
        }
    }

    #[test]
    fn lookup_is_exact() {
        assert_eq!(
            ChatProvider::by_id("agy-sub").unwrap().name,
            "Antigravity (subscription)"
        );
        assert!(ChatProvider::by_id("agy-sub ").is_none());
        assert!(ChatProvider::by_id("gemini").is_none());
        assert!(ChatProvider::by_id("nope").is_none());
    }

    #[test]
    fn only_current_providers_are_available() {
        assert_eq!(
            CHAT_PROVIDERS.iter().map(|p| p.id).collect::<Vec<_>>(),
            [
                "claude-sub",
                "agy-sub",
                "opencode",
                "codex-sub",
                "ollama-cloud",
                "ollama"
            ]
        );
        assert_eq!(STT_PROVIDERS.len(), 4);
        let retired = ProviderConfig::new("openai", "k", "m");
        assert!(!retired.is_usable());
        assert!(retired.endpoint().is_none());
        assert!(SttProvider::by_id("openai-whisper").is_none());
    }

    #[test]
    fn providers_require_a_model() {
        assert!(ProviderConfig::new("ollama", "", "gpt-4o").is_usable());
        assert!(!ProviderConfig::new("ollama", "", "  ").is_usable());
        // An unknown id has no endpoint.
        assert!(!ProviderConfig::new("unknown", "", "m").is_usable());
    }

    #[test]
    fn base_url_override_wins() {
        let mut config = ProviderConfig::new("ollama", "k", "m");
        config.base_url = "http://127.0.0.1:8080/v1/chat/completions".into();
        assert_eq!(
            config.endpoint().unwrap(),
            "http://127.0.0.1:8080/v1/chat/completions"
        );
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
    fn stt_lookup_works() {
        assert_eq!(
            SttProvider::by_id("deepgram-stt").unwrap().default_model,
            "nova-3"
        );
        assert!(SttProvider::by_id("missing").is_none());
        let local = ProviderConfig::new("whisper-local", "", "");
        assert!(local.speech_usable() && !local.speech_streaming());
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
