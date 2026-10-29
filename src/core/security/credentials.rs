//! Provider credential records stored inside the encrypted vault.

use super::vault::{Vault, VaultError};
use crate::ai::{
    catalog::custom::CustomProvider,
    models,
    providers::{ChatProvider, ProviderConfig, SttProvider, endpoint_allowed},
};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Chat,
    Speech,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub custom: Option<CustomProvider>,
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key_marker = if self.api_key.is_empty() {
            "<empty>"
        } else {
            "<redacted>"
        };
        f.debug_struct("Credential")
            .field("kind", &self.kind)
            .field("provider", &self.provider)
            .field("api_key", &key_marker)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("custom", &self.custom)
            .finish()
    }
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

    pub fn to_config(&self) -> ProviderConfig {
        let trimmed_model = self.model.trim();
        let chosen_model = if !trimmed_model.is_empty() {
            trimmed_model.to_owned()
        } else if let Some(chat_default) = models::default_for(&self.provider) {
            chat_default.to_owned()
        } else if let Some(stt) = SttProvider::by_id(&self.provider) {
            stt.default_model.to_owned()
        } else {
            String::new()
        };

        ProviderConfig {
            id: self.provider.clone(),
            api_key: self.api_key.trim().to_owned(),
            model: chosen_model,
            base_url: self.base_url.clone(),
            custom: self.custom.clone(),
            language: String::new(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.provider.trim().is_empty() {
            return Err("The provider id is empty.".into());
        }
        match &self.custom {
            Some(custom) => self.check_custom(custom),
            None => self.check_catalog(),
        }
    }

    fn check_custom(&self, custom: &CustomProvider) -> Result<(), String> {
        custom.validate(self.kind == Kind::Speech)?;
        let url_problem = endpoint_error(&self.base_url);
        first_failure(&[
            (
                !self.provider.starts_with("custom-"),
                "A custom provider id has to begin with \"custom-\".",
            ),
            (url_problem.is_some(), url_problem.unwrap_or_default()),
            (
                custom.auth != "none" && self.api_key.trim().is_empty(),
                "This authentication mode needs an API key.",
            ),
            (
                self.kind == Kind::Chat && self.model.trim().is_empty(),
                "A custom chat provider needs a model.",
            ),
        ])
    }

    fn check_catalog(&self) -> Result<(), String> {
        let needs_key = match self.kind {
            Kind::Chat => ChatProvider::by_id(&self.provider).map(|p| p.requires_key),
            Kind::Speech => SttProvider::by_id(&self.provider).map(|_| true),
        };
        let Some(needs_key) = needs_key else {
            let what = if self.kind == Kind::Chat {
                "chat"
            } else {
                "speech"
            };
            return Err(format!(
                "There is no {what} provider named '{}'.",
                self.provider
            ));
        };
        first_failure(&[
            (
                needs_key && self.api_key.trim().is_empty(),
                "This provider needs an API key.",
            ),
            (
                !self.base_url.is_empty() && !endpoint_allowed(&self.base_url),
                "The custom endpoint must be https://, or http:// on localhost.",
            ),
        ])
    }
}

/// Why `url` cannot be used as a provider endpoint, if it cannot.
fn endpoint_error(url: &str) -> Option<&'static str> {
    match reqwest::Url::parse(url) {
        Err(_) => Some("The endpoint is not a well-formed URL."),
        Ok(parsed) if !matches!(parsed.scheme(), "http" | "https") => {
            Some("The endpoint has to start with http:// or https://.")
        }
        Ok(_) if !endpoint_allowed(url) => {
            Some("Plain http:// is only allowed for localhost; use https://.")
        }
        Ok(_) => None,
    }
}

/// The message of the first `(failed, message)` pair whose flag is set.
fn first_failure(checks: &[(bool, &str)]) -> Result<(), String> {
    match checks.iter().find(|(failed, _)| *failed) {
        Some((_, message)) => Err((*message).to_owned()),
        None => Ok(()),
    }
}

fn kind_prefix(kind: Kind) -> &'static str {
    match kind {
        Kind::Chat => "provider:chat:",
        Kind::Speech => "provider:speech:",
    }
}

fn record_name(kind: Kind, provider: &str) -> String {
    format!("{}{provider}", kind_prefix(kind))
}

fn vault_failure(err: VaultError) -> String {
    format!("The credential vault is unavailable (read or write failed): {err}")
}

/// Decodes one stored record; a bad one is logged by name only.
fn decode(name: &str, raw: serde_json::Value) -> Option<Credential> {
    serde_json::from_value(raw)
        .map_err(
            |_| tracing::warn!(record = %name, "ignoring stored record that is not a credential"),
        )
        .ok()
}

/// Whether a decoded record may be offered for `kind`.
fn usable(credential: &Credential, kind: Kind) -> bool {
    let known = match kind {
        Kind::Chat => ChatProvider::by_id(&credential.provider).is_some(),
        Kind::Speech => SttProvider::by_id(&credential.provider).is_some(),
    };
    credential.kind == kind
        && (known || credential.custom.is_some() && credential.provider.starts_with("custom-"))
}

pub fn save(vault: &Vault, credential: &Credential) -> Result<(), String> {
    credential.validate()?;
    let (name, value) = (
        record_name(credential.kind, &credential.provider),
        serde_json::to_value(credential).map_err(|_| "The credential could not be encoded.")?,
    );
    vault.set(&name, value).map_err(vault_failure)
}

pub fn load(vault: &Vault, kind: Kind, provider: &str) -> Result<Option<Credential>, String> {
    let name = record_name(kind, provider);
    vault
        .get(&name)
        .map_err(vault_failure)?
        .map(|raw| {
            serde_json::from_value(raw)
                .map_err(|_| format!("The stored credential '{name}' is unreadable."))
        })
        .transpose()
}

pub fn list(vault: &Vault, kind: Kind) -> Result<Vec<Credential>, String> {
    let prefix = kind_prefix(kind);
    let mut found: Vec<Credential> = vault
        .read()
        .map_err(vault_failure)?
        .into_iter()
        .filter(|(name, _)| name.starts_with(prefix))
        .filter_map(|(name, raw)| decode(&name, raw))
        .filter(|credential| usable(credential, kind))
        .collect();
    found.sort_by_key(|credential| credential.provider.clone());
    Ok(found)
}

pub fn remove(vault: &Vault, kind: Kind, provider: &str) -> Result<(), String> {
    vault
        .remove(&[record_name(kind, provider).as_str()])
        .map_err(vault_failure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A keyed chat credential; no keyed provider is built in.
    fn keyed(key: &str) -> Credential {
        let mut credential = Credential::chat("custom-test", key);
        credential.custom = Some(Default::default());
        credential.model = "m".into();
        credential.base_url = "http://127.0.0.1:1/v1".into();
        credential
    }

    fn temp_vault() -> (std::path::PathBuf, Vault) {
        let dir = std::env::temp_dir().join(format!("nm-creds-{}", uuid::Uuid::new_v4()));
        let vault = Vault::open(&dir).unwrap();
        (dir, vault)
    }

    #[test]
    fn save_load_and_json_shape_without_leaking_key() {
        let (dir, vault) = temp_vault();
        let secret = "dummy-key-plaintext-55";
        let mut cred = keyed(secret);
        cred.model = "model-z".into();
        save(&vault, &cred).unwrap();

        let loaded = load(&vault, Kind::Chat, "custom-test").unwrap().unwrap();
        assert_eq!(loaded, cred);

        let bin = std::fs::read(dir.join("vault.bin")).unwrap();
        assert!(!bin.windows(secret.len()).any(|w| w == secret.as_bytes()));

        // Built-in record with empty model and no custom omits those fields
        let builtin = Credential::chat("codex-sub", "");
        save(&vault, &builtin).unwrap();
        let raw_builtin = vault.get("provider:chat:codex-sub").unwrap().unwrap();
        assert_eq!(raw_builtin.get("kind"), Some(&json!("chat")));
        assert!(raw_builtin.get("model").is_none());
        assert!(raw_builtin.get("custom").is_none());

        // Also check provider:chat:custom-test has kind == "chat"
        let raw_custom = vault.get("provider:chat:custom-test").unwrap().unwrap();
        assert_eq!(raw_custom.get("kind"), Some(&json!("chat")));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn legacy_shape_loads_and_resolves_catalog_default_model() {
        let (dir, vault) = temp_vault();
        vault
            .set(
                "provider:speech:deepgram-stt",
                json!({
                    "kind": "speech",
                    "provider": "deepgram-stt",
                    "api_key": "dummy-k"
                }),
            )
            .unwrap();

        let loaded = load(&vault, Kind::Speech, "deepgram-stt").unwrap().unwrap();
        assert_eq!(
            loaded.to_config().model,
            SttProvider::by_id("deepgram-stt").unwrap().default_model
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn validation_checks_keys_ids_and_urls() {
        // Custom credential key and auth = "none"
        let mut c = keyed("");
        assert!(c.validate().is_err());
        c.custom.as_mut().unwrap().auth = "none".into();
        assert!(c.validate().is_ok());

        // Built-in chat and speech key rules
        assert!(Credential::chat("codex-sub", "   ").validate().is_ok());
        assert!(Credential::chat("ollama-cloud", "  ").validate().is_err());
        assert!(Credential::speech("parakeet-local", "").validate().is_err());

        // Unknown ids and wrong kind
        let err_chat = Credential::chat("zz-missing", "dummy-key-q9")
            .validate()
            .unwrap_err();
        assert!(err_chat.contains("zz-missing"));
        let err_speech = Credential::speech("zz-missing", "dummy-key-q9")
            .validate()
            .unwrap_err();
        assert!(err_speech.contains("zz-missing"));
        assert!(
            Credential::chat("deepgram-stt", "dummy-key-q9")
                .validate()
                .is_err()
        );

        // base_url values for custom chat credential
        for ok_url in ["https://api.invalid/v2", "http://localhost:8181/x"] {
            let mut cred = keyed("dummy-key-q9");
            cred.base_url = ok_url.into();
            assert!(cred.validate().is_ok(), "{ok_url}");
        }
        for bad_url in ["http://10.1.2.3/x", "gopher://localhost/x", "not a url"] {
            let mut cred = keyed("dummy-key-q9");
            cred.base_url = bad_url.into();
            assert!(cred.validate().is_err(), "{bad_url}");
        }
    }

    #[test]
    fn list_remove_retired_and_malformed_records() {
        let (dir, vault) = temp_vault();

        // Saving invalid credential writes nothing
        let bad = Credential::chat("zz-missing", "dummy-key-q9");
        assert!(save(&vault, &bad).is_err());
        assert!(list(&vault, Kind::Chat).unwrap().is_empty());

        // Two chat records and one speech record
        let mut chat_b = keyed("dummy-key-b");
        chat_b.provider = "custom-b-one".into();
        let mut chat_a = keyed("dummy-key-a");
        chat_a.provider = "custom-a-one".into();
        let speech = Credential::speech("deepgram-stt", "dummy-key-s");

        save(&vault, &chat_b).unwrap();
        save(&vault, &chat_a).unwrap();
        save(&vault, &speech).unwrap();

        let chats = list(&vault, Kind::Chat).unwrap();
        assert_eq!(chats, vec![chat_a.clone(), chat_b.clone()]);
        let speeches = list(&vault, Kind::Speech).unwrap();
        assert_eq!(speeches, vec![speech]);

        // Retired chat id ("gemini")
        vault
            .set(
                "provider:chat:gemini",
                json!({
                    "kind": "chat",
                    "provider": "gemini",
                    "api_key": "dummy-retired-key"
                }),
            )
            .unwrap();
        assert_eq!(
            list(&vault, Kind::Chat).unwrap(),
            vec![chat_a.clone(), chat_b.clone()]
        );
        assert!(vault.get("provider:chat:gemini").unwrap().is_some());
        let retired = load(&vault, Kind::Chat, "gemini").unwrap().unwrap();
        assert!(!retired.to_config().is_usable());

        // Malformed record missing api_key with distinctive probe value
        let probe = "leak-probe-41";
        vault
            .set(
                "provider:chat:broken",
                json!({
                    "kind": "chat",
                    "provider": probe
                }),
            )
            .unwrap();
        let listed = list(&vault, Kind::Chat).unwrap();
        assert_eq!(listed.len(), 2);
        let load_err = load(&vault, Kind::Chat, "broken").unwrap_err();
        assert!(!load_err.contains(probe));

        // Remove one id, leave the other; removing absent id is Ok
        remove(&vault, Kind::Chat, "custom-a-one").unwrap();
        assert_eq!(list(&vault, Kind::Chat).unwrap(), vec![chat_b]);
        remove(&vault, Kind::Chat, "never-saved").unwrap();

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn to_config_trims_and_debug_redacts_key() {
        let mut cred = Credential::chat("claude-sub", "  dummy-key-q9  ");
        cred.model = "   ".into();
        let cfg = cred.to_config();
        assert_eq!(cfg.api_key, "dummy-key-q9");
        assert_eq!(cfg.model, models::default_for("claude-sub").unwrap());

        cred.model = "  opus  ".into();
        assert_eq!(cred.to_config().model, "opus");

        let dbg = format!("{cred:?}");
        assert!(!dbg.contains("dummy-key-q9"));
    }
}
