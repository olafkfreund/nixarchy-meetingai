//! Provider catalog.
//!
//! Bring-your-own-key chat and speech-to-text providers. Endpoints, auth headers
//! and request shapes mirror the reference build so existing keys and habits
//! carry over. Requests are built directly to each vendor; there is no hosted
//! intermediary.

use serde::{Deserialize, Serialize};

/// How a chat provider authenticates and frames a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatWire {
    /// OpenAI-compatible `/chat/completions`.
    OpenAi,
    /// Anthropic `/v1/messages`.
    Anthropic,
    /// Cohere `/v2/chat`.
    Cohere,
}

/// A chat provider definition.
#[derive(Debug, Clone, Copy)]
pub struct ChatProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub endpoint: &'static str,
    pub wire: ChatWire,
    pub supports_images: bool,
}

/// Every bundled chat provider.
pub const CHAT_PROVIDERS: &[ChatProvider] = &[
    ChatProvider {
        id: "openai",
        name: "OpenAI",
        endpoint: "https://api.openai.com/v1/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
    },
    ChatProvider {
        id: "claude",
        name: "Anthropic",
        endpoint: "https://api.anthropic.com/v1/messages",
        wire: ChatWire::Anthropic,
        supports_images: true,
    },
    ChatProvider {
        id: "grok",
        name: "xAI Grok",
        endpoint: "https://api.x.ai/v1/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
    },
    ChatProvider {
        id: "gemini",
        name: "Google Gemini",
        endpoint: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
    },
    ChatProvider {
        id: "mistral",
        name: "Mistral",
        endpoint: "https://api.mistral.ai/v1/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
    },
    ChatProvider {
        id: "cohere",
        name: "Cohere",
        endpoint: "https://api.cohere.ai/v2/chat",
        wire: ChatWire::Cohere,
        supports_images: true,
    },
    ChatProvider {
        id: "groq",
        name: "Groq",
        endpoint: "https://api.groq.com/openai/v1/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
    },
    ChatProvider {
        id: "perplexity",
        name: "Perplexity",
        endpoint: "https://api.perplexity.ai/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
    },
    ChatProvider {
        id: "openrouter",
        name: "OpenRouter",
        endpoint: "https://openrouter.ai/api/v1/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
    },
    ChatProvider {
        id: "ollama",
        name: "Ollama (local)",
        endpoint: "http://localhost:11434/v1/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
    },
];

impl ChatProvider {
    pub fn by_id(id: &str) -> Option<&'static ChatProvider> {
        CHAT_PROVIDERS.iter().find(|provider| provider.id == id)
    }

    /// Whether this provider needs a key. A local Ollama install does not.
    pub fn requires_key(self) -> bool {
        self.wire != ChatWire::OpenAi || self.id != "ollama"
    }
}

/// How a speech provider uploads audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttWire {
    /// Multipart upload returning `{ "text": ... }` (OpenAI-compatible).
    MultipartText,
    /// Raw `audio/wav` body (Deepgram, Azure, IBM).
    RawAudio,
    /// JSON body carrying base64 audio plus config (Google).
    GoogleJson,
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
        id: "openai-whisper",
        name: "OpenAI Whisper",
        endpoint: "https://api.openai.com/v1/audio/transcriptions",
        wire: SttWire::MultipartText,
        default_model: "whisper-1",
    },
    SttProvider {
        id: "groq-whisper",
        name: "Groq Whisper",
        endpoint: "https://api.groq.com/openai/v1/audio/transcriptions",
        wire: SttWire::MultipartText,
        default_model: "whisper-large-v3-turbo",
    },
    SttProvider {
        id: "deepgram-stt",
        name: "Deepgram",
        endpoint: "https://api.deepgram.com/v1/listen",
        wire: SttWire::RawAudio,
        default_model: "nova-3",
    },
    SttProvider {
        id: "google-stt",
        name: "Google Speech",
        endpoint: "https://speech.googleapis.com/v1/speech:recognize",
        wire: SttWire::GoogleJson,
        default_model: "",
    },
];

impl SttProvider {
    pub fn by_id(id: &str) -> Option<&'static SttProvider> {
        STT_PROVIDERS.iter().find(|provider| provider.id == id)
    }
}

/// A user-configured provider instance, as persisted in the vault.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    /// Catalog id, or `custom` for a user-supplied endpoint.
    pub id: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub base_url: String,
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
        }
    }

    /// Endpoint to call, honouring a user override.
    pub fn endpoint(&self) -> Option<String> {
        if !self.base_url.is_empty() {
            return Some(self.base_url.clone());
        }
        ChatProvider::by_id(&self.id).map(|provider| provider.endpoint.to_owned())
    }

    /// Whether this configuration is usable as-is.
    pub fn is_usable(&self) -> bool {
        let needs_key = ChatProvider::by_id(&self.id)
            .map(|provider| provider.requires_key())
            .unwrap_or(true);
        self.endpoint().is_some()
            && !self.model.trim().is_empty()
            && (!needs_key || !self.api_key.trim().is_empty())
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
            assert!(provider.endpoint.starts_with("http"), "{}", provider.id);
            if provider.id != "ollama" {
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
        assert_eq!(ChatProvider::by_id("openai").unwrap().name, "OpenAI");
        assert!(ChatProvider::by_id("openai ").is_none());
        assert!(ChatProvider::by_id("nope").is_none());
    }

    #[test]
    fn ollama_does_not_require_a_key() {
        let ollama = ChatProvider::by_id("ollama").unwrap();
        assert!(!ollama.requires_key());
        let config = ProviderConfig::new("ollama", "", "llama3.2");
        assert!(config.is_usable());
    }

    #[test]
    fn hosted_providers_require_a_key_and_model() {
        let blank = ProviderConfig::new("openai", "", "gpt-4o");
        assert!(!blank.is_usable());
        let complete = ProviderConfig::new("openai", "sk-test", "gpt-4o");
        assert!(complete.is_usable());
        let no_model = ProviderConfig::new("openai", "sk-test", "  ");
        assert!(!no_model.is_usable());
    }

    #[test]
    fn base_url_override_wins() {
        let mut config = ProviderConfig::new("openai", "k", "m");
        config.base_url = "http://127.0.0.1:8080/v1/chat/completions".into();
        assert_eq!(
            config.endpoint().unwrap(),
            "http://127.0.0.1:8080/v1/chat/completions"
        );
    }

    #[test]
    fn stt_lookup_works() {
        assert_eq!(
            SttProvider::by_id("deepgram-stt").unwrap().default_model,
            "nova-3"
        );
        assert!(SttProvider::by_id("missing").is_none());
    }
}
