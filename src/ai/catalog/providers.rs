//! Provider catalog.
//!
//! Bring-your-own-key chat and speech-to-text providers. Endpoints, auth headers
//! and request shapes for Gemini, Merge Gateway and Deepgram. Removed providers
//! are deliberately absent so old vault records cannot become active.

use serde::{Deserialize, Serialize};

/// How a chat provider authenticates and frames a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatWire {
    /// OpenAI-compatible `/chat/completions`.
    OpenAi,
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
        id: "gemini",
        name: "Google Gemini",
        endpoint: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
    },
    ChatProvider {
        id: "merge-gateway",
        name: "Merge Gateway",
        endpoint: "https://api-gateway.merge.dev/v1/chat/completions",
        wire: ChatWire::OpenAi,
        supports_images: true,
    },
];

impl ChatProvider {
    pub fn by_id(id: &str) -> Option<&'static ChatProvider> {
        CHAT_PROVIDERS.iter().find(|provider| provider.id == id)
    }

    /// Both supported chat services require an API key.
    pub fn requires_key(self) -> bool {
        true
    }
}

/// How a speech provider uploads audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttWire {
    /// Raw `audio/wav` body for Deepgram.
    RawAudio,
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
pub const STT_PROVIDERS: &[SttProvider] = &[SttProvider {
    id: "deepgram-stt",
    name: "Deepgram",
    endpoint: "https://api.deepgram.com/v1/listen",
    wire: SttWire::RawAudio,
    default_model: "nova-3",
}];

impl SttProvider {
    pub fn by_id(id: &str) -> Option<&'static SttProvider> {
        STT_PROVIDERS.iter().find(|provider| provider.id == id)
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
        let provider = ChatProvider::by_id(&self.id)?;
        if !self.base_url.is_empty() {
            return Some(self.base_url.clone());
        }
        Some(provider.endpoint.to_owned())
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
        assert_eq!(ChatProvider::by_id("gemini").unwrap().name, "Google Gemini");
        assert!(ChatProvider::by_id("gemini ").is_none());
        assert!(ChatProvider::by_id("nope").is_none());
    }

    #[test]
    fn only_current_providers_are_available() {
        assert_eq!(
            CHAT_PROVIDERS.iter().map(|p| p.id).collect::<Vec<_>>(),
            ["gemini", "merge-gateway"]
        );
        assert_eq!(STT_PROVIDERS.len(), 1);
        let mut retired = ProviderConfig::new("ollama", "k", "m");
        retired.base_url = "http://localhost:1234".into();
        assert!(!retired.is_usable());
        assert!(retired.endpoint().is_none());
        assert!(SttProvider::by_id("openai-whisper").is_none());
    }

    #[test]
    fn hosted_providers_require_a_key_and_model() {
        let blank = ProviderConfig::new("gemini", "", "gpt-4o");
        assert!(!blank.is_usable());
        let complete = ProviderConfig::new("gemini", "sk-test", "gpt-4o");
        assert!(complete.is_usable());
        let no_model = ProviderConfig::new("gemini", "sk-test", "  ");
        assert!(!no_model.is_usable());
    }

    #[test]
    fn base_url_override_wins() {
        let mut config = ProviderConfig::new("gemini", "k", "m");
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
