//! Provider credentials, stored in the encrypted vault.
//!
//! One record per provider: which catalog entry it maps to, its key, an optional
//! model override and an optional custom endpoint. Keys never leave the vault
//! except to build a request.

use serde::{Deserialize, Serialize};

use super::vault::{Vault, VaultError};
use crate::ai::providers::{ChatProvider, ProviderConfig, SttProvider};

/// Where a credential is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Chat,
    Speech,
}

/// A stored credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    pub kind: Kind,
    /// Catalog id, e.g. `claude-sub` or `deepgram-stt`.
    pub provider: String,
    pub api_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<crate::ai::catalog::custom::CustomProvider>,
}

impl Credential {
    pub fn chat(provider: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            kind: Kind::Chat,
            provider: provider.into(),
            api_key: api_key.into(),
            model: String::new(),
            base_url: String::new(),
            custom: None,
        }
    }

    pub fn speech(provider: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            kind: Kind::Speech,
            provider: provider.into(),
            api_key: api_key.into(),
            model: String::new(),
            base_url: String::new(),
            custom: None,
        }
    }

    /// Convert to the provider config used by the request layers.
    pub fn to_config(&self) -> ProviderConfig {
        ProviderConfig {
            id: self.provider.clone(),
            api_key: self.api_key.trim().to_owned(),
            model: if self.model.trim().is_empty() {
                crate::ai::models::default_for(&self.provider)
                    .or_else(|| SttProvider::by_id(&self.provider).map(|p| p.default_model))
                    .unwrap_or_default()
                    .to_owned()
            } else {
                self.model.trim().to_owned()
            },
            base_url: self.base_url.clone(),
            custom: self.custom.clone(),
            language: String::new(),
        }
    }

    /// Validation shared by both kinds.
    pub fn validate(&self) -> Result<(), String> {
        if self.provider.trim().is_empty() {
            return Err("Choose a provider first.".to_owned());
        }
        if let Some(custom) = &self.custom {
            if !self.provider.starts_with("custom-") {
                return Err("Custom providers need their own identifier.".into());
            }
            custom.validate(self.kind == Kind::Speech)?;
            let url = reqwest::Url::parse(&self.base_url)
                .map_err(|_| "Enter a complete HTTP endpoint.")?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err("Use an http:// or https:// endpoint.".into());
            }
            if !crate::ai::providers::endpoint_allowed(&self.base_url) {
                return Err("Plain http:// is only allowed for localhost; use https://.".into());
            }
            if custom.auth != "none" && self.api_key.trim().is_empty() {
                return Err("Enter an API key or choose no authentication.".into());
            }
            if self.kind == Kind::Chat && self.model.trim().is_empty() {
                return Err("Enter a model ID.".into());
            }
            return Ok(());
        }
        match self.kind {
            Kind::Chat => {
                if ChatProvider::by_id(&self.provider).is_none() {
                    return Err(format!("'{}' is not a known chat provider.", self.provider));
                }
            }
            Kind::Speech => {
                if SttProvider::by_id(&self.provider).is_none() {
                    return Err(format!(
                        "'{}' is not a known speech provider.",
                        self.provider
                    ));
                }
            }
        }
        let needs_key = ChatProvider::by_id(&self.provider)
            .map(|provider| provider.requires_key)
            .unwrap_or(true);
        if needs_key && self.api_key.trim().is_empty() {
            return Err("Enter an API key.".to_owned());
        }
        if !self.base_url.is_empty() && !crate::ai::providers::endpoint_allowed(&self.base_url) {
            return Err(
                "The custom endpoint must be https://, or http:// on localhost.".to_owned(),
            );
        }
        Ok(())
    }

    /// Mask a key for display: keep only the last four characters.
    pub fn masked_key(&self) -> String {
        let key = self.api_key.trim();
        if key.is_empty() {
            "Not set".to_owned()
        } else if key.len() <= 4 {
            "••••".to_owned()
        } else {
            format!("••••{}", &key[key.len() - 4..])
        }
    }
}

/// Vault key for a credential record.
fn key(kind: Kind, provider: &str) -> String {
    match kind {
        Kind::Chat => format!("provider:chat:{provider}"),
        Kind::Speech => format!("provider:speech:{provider}"),
    }
}

/// Persist a credential.
pub fn save(vault: &Vault, credential: &Credential) -> Result<(), String> {
    credential.validate()?;
    let value = serde_json::to_value(credential).map_err(|error| error.to_string())?;
    vault
        .set(&key(credential.kind, &credential.provider), value)
        .map_err(describe)
}

/// Load one credential.
pub fn load(vault: &Vault, kind: Kind, provider: &str) -> Result<Option<Credential>, String> {
    let value = vault.get(&key(kind, provider)).map_err(describe)?;
    match value {
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|error| format!("stored credential is unreadable: {error}")),
        None => Ok(None),
    }
}

/// Every stored credential of a kind.
pub fn list(vault: &Vault, kind: Kind) -> Result<Vec<Credential>, String> {
    let store = vault.read().map_err(describe)?;
    let prefix = match kind {
        Kind::Chat => "provider:chat:",
        Kind::Speech => "provider:speech:",
    };
    let mut credentials = Vec::new();
    for (name, value) in store {
        if !name.starts_with(prefix) {
            continue;
        }
        match serde_json::from_value::<Credential>(value) {
            Ok(credential) => {
                let supported = credential.kind == kind
                    && (credential.custom.is_some() && credential.provider.starts_with("custom-")
                        || match kind {
                            Kind::Chat => ChatProvider::by_id(&credential.provider).is_some(),
                            Kind::Speech => SttProvider::by_id(&credential.provider).is_some(),
                        });
                if supported {
                    credentials.push(credential);
                }
            }
            Err(error) => {
                tracing::warn!(%error, %name, "skipping unreadable credential");
            }
        }
    }
    credentials.sort_by(|a, b| a.provider.cmp(&b.provider));
    Ok(credentials)
}

/// Remove a credential.
pub fn remove(vault: &Vault, kind: Kind, provider: &str) -> Result<(), String> {
    vault
        .remove(&[key(kind, provider).as_str()])
        .map_err(describe)
}

fn describe(error: VaultError) -> String {
    format!("The credential vault could not be read or written: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> Vault {
        let dir =
            std::env::temp_dir().join(format!("nixarchy-meetingai-cred-{}", uuid::Uuid::new_v4()));
        Vault::open(dir).unwrap()
    }

    /// A keyed chat credential; no keyed provider is built in.
    fn keyed(key: &str) -> Credential {
        let mut credential = Credential::chat("custom-test", key);
        credential.custom = Some(Default::default());
        credential.model = "m".into();
        credential.base_url = "http://127.0.0.1:1/v1".into();
        credential
    }

    #[test]
    fn saves_and_loads_a_chat_key() {
        let vault = vault();
        let mut credential = keyed("sk-test-123");
        credential.model = "gpt-4o".into();
        save(&vault, &credential).unwrap();
        let loaded = load(&vault, Kind::Chat, "custom-test").unwrap().unwrap();
        assert_eq!(loaded.api_key, "sk-test-123");
        assert_eq!(loaded.model, "gpt-4o");
    }

    #[test]
    fn keys_are_never_stored_in_plaintext_in_settings_but_are_encrypted() {
        let dir =
            std::env::temp_dir().join(format!("nixarchy-meetingai-cred2-{}", uuid::Uuid::new_v4()));
        let vault = Vault::open(&dir).unwrap();
        save(&vault, &keyed("sk-secret-value")).unwrap();
        let bytes = std::fs::read(dir.join("vault.bin")).unwrap();
        assert!(
            !bytes.windows(16).any(|w| w == b"sk-secret-value"),
            "the key must not appear in ciphertext"
        );
    }

    #[test]
    fn empty_key_is_rejected_for_hosted_providers() {
        let credential = keyed("");
        assert!(credential.validate().unwrap_err().contains("API key"));
    }

    #[test]
    fn retired_provider_is_rejected() {
        assert!(Credential::chat("openai", "key").validate().is_err());
    }

    #[test]
    fn unknown_providers_are_rejected() {
        assert!(Credential::chat("nope", "k").validate().is_err());
        assert!(Credential::speech("nope", "k").validate().is_err());
    }

    #[test]
    fn custom_endpoint_must_be_http() {
        let mut credential = keyed("k");
        credential.base_url = "ftp://nope".into();
        assert!(credential.validate().unwrap_err().contains("http"));
        credential.base_url = "http://localhost:1234/v1".into();
        assert!(credential.validate().is_ok());
        credential.base_url = "http://[::1]:1234/v1".into();
        assert!(credential.validate().is_ok());
        credential.base_url = "http://example.com/v1".into();
        assert!(credential.validate().is_err());
        let mut custom = Credential::chat("custom-x", "k");
        custom.custom = Some(Default::default());
        custom.model = "m".into();
        custom.base_url = "http://example.com/v1".into();
        assert!(custom.validate().is_err());
        custom.base_url = "http://127.0.0.1:1/v1".into();
        assert!(custom.validate().is_ok());
    }

    #[test]
    fn listing_filters_by_kind() {
        let vault = vault();
        save(&vault, &keyed("a")).unwrap();
        save(&vault, &Credential::speech("deepgram-stt", "b")).unwrap();
        let chats = list(&vault, Kind::Chat).unwrap();
        let speech = list(&vault, Kind::Speech).unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(speech.len(), 1);
        assert_eq!(chats[0].provider, "custom-test");
        assert_eq!(speech[0].provider, "deepgram-stt");
    }

    #[test]
    fn removing_clears_only_that_provider() {
        let vault = vault();
        save(&vault, &keyed("a")).unwrap();
        save(&vault, &Credential::chat("ollama", "b")).unwrap();
        remove(&vault, Kind::Chat, "custom-test").unwrap();
        assert!(load(&vault, Kind::Chat, "custom-test").unwrap().is_none());
        assert!(load(&vault, Kind::Chat, "ollama").unwrap().is_some());
    }

    #[test]
    fn old_credentials_stay_encrypted_but_are_not_active() {
        let vault = vault();
        vault.set("provider:chat:openai", serde_json::json!({"kind":"chat","provider":"openai","api_key":"old-key","model":"old-model"})).unwrap();
        assert!(list(&vault, Kind::Chat).unwrap().is_empty());
        assert!(vault.get("provider:chat:openai").unwrap().is_some());
        assert!(
            !load(&vault, Kind::Chat, "openai")
                .unwrap()
                .unwrap()
                .to_config()
                .is_usable()
        );
    }

    #[test]
    fn models_are_selectable_and_blank_models_have_working_defaults() {
        let config = Credential::chat("claude-sub", "").to_config();
        assert_eq!(config.model, "sonnet");
        assert!(config.is_usable());
        let mut credential = Credential::chat("ollama", "");
        credential.model = "some-other-model".into();
        assert!(credential.validate().is_ok());
        assert_eq!(
            Credential::speech("deepgram-stt", "test-key")
                .to_config()
                .model,
            "nova-3"
        );
    }

    #[test]
    fn masked_keys_show_only_the_tail() {
        let credential = keyed("sk-abcdefgh1234");
        assert_eq!(credential.masked_key(), "••••1234");
        assert_eq!(keyed("").masked_key(), "Not set");
    }
}
