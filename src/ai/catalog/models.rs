//! Model catalog.
//!
//! Known model ids per provider, so the overlay offers real choices instead of a
//! placeholder. A user-entered id always wins, so new models work without a
//! release.

use super::providers::ChatProvider;

/// Suggested models for one provider.
pub struct ModelList {
    pub provider: &'static str,
    pub models: &'static [&'static str],
}

/// Bundled suggestions. Ordered with the fastest and cheapest first, matching
/// how people pick a model for quick answers.
pub const MERGE_MODEL: &str = "deepseek/deepseek-v4.1-flash-1017";
const EMPTY_CATALOG: &str = "This account's catalog has no text-chat models. Your saved key and configured model have been kept.";

pub const MODELS: &[ModelList] = &[
    ModelList {
        provider: "gemini",
        models: &["gemini-3.8-flash"],
    },
    ModelList {
        provider: "merge-gateway",
        models: &[MERGE_MODEL],
    },
];

/// Suggested models for a provider, empty when unknown.
pub fn for_provider(provider_id: &str) -> &'static [&'static str] {
    MODELS
        .iter()
        .find(|list| list.provider == provider_id)
        .map(|list| list.models)
        .unwrap_or(&[])
}

/// The default model for a provider: the first suggestion.
pub fn default_for(provider_id: &str) -> Option<&'static str> {
    for_provider(provider_id).first().copied()
}

/// A display label for the overlay button.
///
/// Falls back to the provider's default so the control names a real model rather
/// than saying "Select a model".
pub fn label(configured: Option<&str>, provider_id: Option<&str>) -> String {
    if let Some(model) = configured.map(str::trim).filter(|model| !model.is_empty()) {
        return model.to_owned();
    }
    match provider_id {
        Some(provider) => default_for(provider)
            .map(str::to_owned)
            .unwrap_or_else(|| provider.to_owned()),
        // No provider at all: say so plainly rather than "No model", which reads
        // like a picker that failed.
        None => "Add a provider".to_owned(),
    }
}

/// Whether a provider id is known to the catalog.
pub fn is_known_provider(provider_id: &str) -> bool {
    ChatProvider::by_id(provider_id).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_has_models() {
        for provider in super::super::providers::CHAT_PROVIDERS {
            assert!(
                !for_provider(provider.id).is_empty(),
                "{} has no suggested models",
                provider.id
            );
        }
    }

    #[test]
    fn unknown_provider_is_empty_not_a_panic() {
        assert!(for_provider("nope").is_empty());
        assert_eq!(default_for("nope"), None);
    }

    #[test]
    fn label_prefers_the_configured_model() {
        assert_eq!(label(Some("gpt-4o"), Some("gemini")), "gpt-4o");
        assert_eq!(label(None, Some("gemini")), "gemini-3.8-flash");
        assert_eq!(label(Some("   "), Some("gemini")), "gemini-3.8-flash");
        assert_eq!(label(None, None), "Add a provider");
    }

    #[test]
    fn models_are_unique_within_a_provider() {
        for list in MODELS {
            let mut seen = std::collections::HashSet::new();
            for model in list.models {
                assert!(
                    seen.insert(*model),
                    "duplicate {model} in {}",
                    list.provider
                );
            }
        }
    }
}

/// Six-hour provider/account/endpoint-scoped cache. Only model IDs and timestamps
/// are persisted; credentials remain in the encrypted vault.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CachedModels {
    pub fetched_at: u64,
    pub models: Vec<String>,
}
impl CachedModels {
    pub fn fresh(&self) -> bool {
        now().saturating_sub(self.fetched_at) < 6 * 60 * 60 && !self.models.is_empty()
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn cache_key(config: &super::providers::ProviderConfig) -> String {
    use sha2::{Digest, Sha256};
    let scope =
        serde_json::to_vec(&(&config.id, config.endpoint(), &config.api_key)).expect("cache scope");
    format!(
        "model_catalog:v2:{}",
        Sha256::digest(scope)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub fn cached(
    path: Option<&std::path::Path>,
    config: &super::providers::ProviderConfig,
) -> Option<CachedModels> {
    let db = crate::db::Database::open(path?).ok()?;
    let raw = crate::db::settings::get(db.connection(), &cache_key(config)).ok()??;
    serde_json::from_str(&raw).ok()
}

/// Model-list endpoint shares the exact origin and API prefix of chat, including
/// custom gateways. Never send a key to a second vendor as a fallback.
pub fn models_endpoint(config: &super::providers::ProviderConfig) -> Result<String, String> {
    let endpoint = config.endpoint().ok_or("Unknown provider")?;
    let prefix = endpoint
        .trim_end_matches('/')
        .strip_suffix("/chat/completions")
        .ok_or("The custom chat endpoint must end in /chat/completions to discover models.")?;
    Ok(format!("{prefix}/models"))
}

fn supports_chat(row: &serde_json::Value) -> bool {
    let outputs = row
        .pointer("/modalities/output")
        .or_else(|| row.pointer("/capabilities/output"))
        .or_else(|| row.get("output_modalities"))
        .and_then(serde_json::Value::as_array);
    if let Some(outputs) = outputs.filter(|items| !items.is_empty()) {
        return outputs.iter().any(|item| item.as_str() == Some("text"));
    }
    if let Some(methods) = row
        .get("supportedGenerationMethods")
        .and_then(serde_json::Value::as_array)
        .filter(|items| !items.is_empty())
    {
        return methods
            .iter()
            .any(|method| matches!(method.as_str(), Some("generateContent" | "generateMessage")));
    }
    // Merge exposes capabilities per vendor, rather than on the model itself.
    if let Some(vendors) = row.get("vendors").and_then(serde_json::Value::as_object) {
        let mut described = false;
        for vendor in vendors.values() {
            match vendor
                .pointer("/capabilities/output")
                .and_then(serde_json::Value::as_array)
                .filter(|v| !v.is_empty())
            {
                Some(output) => {
                    described = true;
                    if output.iter().any(|item| item.as_str() == Some("text")) {
                        return true;
                    }
                }
                None => return true, // Missing metadata is not evidence of incompatibility.
            }
        }
        if described {
            return false;
        }
    }
    true
}

fn page_models(value: &serde_json::Value) -> Result<Vec<String>, String> {
    let rows = value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(serde_json::Value::as_array)
        .ok_or("The provider did not return a model list.")?;
    let mut models: Vec<_> = rows
        .iter()
        .filter(|row| supports_chat(row))
        .filter_map(|row| {
            ["model", "id", "name"].iter().find_map(|field| {
                row.get(field)
                    .and_then(serde_json::Value::as_str)
                    .filter(|id| !id.trim().is_empty())
            })
        })
        .map(str::trim)
        .map(|id| id.strip_prefix("models/").unwrap_or(id))
        .filter(|id| !id.is_empty() && id.len() <= 240 && !id.chars().any(char::is_control))
        .take(2000)
        .map(str::to_owned)
        .collect();
    models.sort();
    models.dedup();
    Ok(models)
}

pub fn parse_models(value: &serde_json::Value) -> Result<Vec<String>, String> {
    let models = page_models(value)?;
    if models.is_empty() {
        Err(EMPTY_CATALOG.into())
    } else {
        Ok(models)
    }
}

pub async fn discover(
    path: Option<std::path::PathBuf>,
    config: super::providers::ProviderConfig,
    force: bool,
) -> Result<Vec<String>, String> {
    if !force
        && let Some(cache) = cached(path.as_deref(), &config)
        && cache.fresh()
    {
        return Ok(cache.models);
    }
    let models = tokio::time::timeout(std::time::Duration::from_secs(30), fetch_catalog(&config))
        .await
        .map_err(|_| "Model refresh timed out. Your saved key and models have been kept.")??;
    if let Some(path) = path
        && let Ok(db) = crate::db::Database::open(path)
    {
        let entry = CachedModels {
            fetched_at: now(),
            models: models.clone(),
        };
        let _ = crate::db::settings::set(
            db.connection(),
            &cache_key(&config),
            &serde_json::to_string(&entry).expect("catalog JSON"),
        );
    }
    Ok(models)
}

async fn fetch_catalog(config: &super::providers::ProviderConfig) -> Result<Vec<String>, String> {
    let client = crate::ai::engine::http_client();
    let endpoint = models_endpoint(config)?;
    let mut continuation: Option<(&str, String)> = None;
    let mut cursors = std::collections::HashSet::new();
    let mut models = Vec::new();
    let mut total_bytes = 0;
    loop {
        let mut url = reqwest::Url::parse(&endpoint)
            .map_err(|_| "The model catalog endpoint is not a valid URL.")?;
        if let Some((parameter, cursor)) = &continuation {
            url.query_pairs_mut().append_pair(parameter, cursor);
        }
        let mut request = client.get(url);
        if let Some(custom) = &config.custom {
            if let Some((name, value)) = custom.header(&config.api_key) {
                request = request.header(name, value);
            }
        } else {
            request = request.bearer_auth(&config.api_key);
        }
        let response = request
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|_| {
                "Could not reach the model catalog. Check your connection or retry.".to_owned()
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(match status.as_u16() {
            401 | 403 => format!("The provider denied model-list access (HTTP {}). Check this key's catalog permissions. Your saved key has been kept.", status.as_u16()),
            404 | 405 => "This endpoint does not offer a model catalog. You can keep using your configured model or enter a custom model ID.".into(),
            _ => format!("Model refresh returned HTTP {}. Retry later; your saved key and models have been kept.", status.as_u16()),
        });
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| "Could not read the model catalog.")?;
        total_bytes += bytes.len();
        if bytes.len() > 4 * 1024 * 1024 || total_bytes > 32 * 1024 * 1024 {
            return Err("The model catalog is too large.".into());
        }
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "The model catalog was not valid JSON.")?;
        models.extend(page_models(&value)?);
        continuation = if let Some(token) = value
            .get("nextPageToken")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            Some(("pageToken", token.to_owned()))
        } else if value.get("has_more").and_then(serde_json::Value::as_bool) != Some(false) {
            value
                .get("next_cursor")
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.is_empty())
                .map(|token| ("cursor", token.to_owned()))
        } else {
            None
        };
        if continuation.is_none() {
            if value.get("has_more").and_then(serde_json::Value::as_bool) == Some(true) {
                return Err(
                    "The model catalog is missing its next-page cursor. Retry refresh.".into(),
                );
            }
            break;
        }
        if models.len() > 10_000 || cursors.len() >= 50 || !cursors.insert(continuation.clone()) {
            return Err(
                "The model catalog pagination did not complete. Saved models are still available."
                    .into(),
            );
        }
    }
    models.sort();
    models.dedup();
    if models.is_empty() {
        return Err(EMPTY_CATALOG.into());
    }
    Ok(models)
}

#[cfg(test)]
mod discovery_tests {
    use super::super::providers::ProviderConfig;
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn merge_catalog_uses_canonical_model_ids_and_vendor_capabilities() {
        let catalog = serde_json::json!({"data":[
            {"model":"anthropic/chat-model", "vendors":{"a":{"capabilities":{"output":["text","tool_use"]}}}},
            {"model":"audio-only", "vendors":{"a":{"capabilities":{"output":["audio"]}}}},
            {"model":"mixed", "vendors":{"a":{"capabilities":{"output":["image"]}},"b":{"capabilities":{"output":["text"]}}}},
            {"model":"unknown-output", "modalities":{"output":[]}},
            {"model":"canonical-name", "id":"internal-database-id"}
        ]});
        assert_eq!(
            parse_models(&catalog).unwrap(),
            [
                "anthropic/chat-model",
                "canonical-name",
                "mixed",
                "unknown-output"
            ]
        );
        assert_eq!(
            parse_models(&serde_json::json!({"models":[
                {"name":"models/gemini-chat", "supportedGenerationMethods":["generateContent"]},
                {"name":"models/embedding", "supportedGenerationMethods":["embedContent"]}
            ]}))
            .unwrap(),
            ["gemini-chat"]
        );
    }

    fn catalog_server(
        pages: Vec<(u16, serde_json::Value)>,
    ) -> (ProviderConfig, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut config = ProviderConfig::new("merge-gateway", "saved-list-key", "my-custom-model");
        config.base_url = format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        );
        let server = std::thread::spawn(move || {
            let mut requests = vec![];
            for (status, page) in pages {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "Catalog page was never requested"
                            );
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                // Winsock can inherit the listener's nonblocking mode. Switch
                // accepted sockets back before the blocking HTTP read/write.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = vec![];
                let mut buffer = [0; 4096];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buffer[..n]);
                }
                requests.push(String::from_utf8(request).unwrap().to_lowercase());
                let body = page.to_string();
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (config, server)
    }

    #[tokio::test]
    async fn merge_and_gemini_pagination_keep_auth_on_the_configured_endpoint() {
        for google in [false, true] {
            let pages = if google {
                vec![
                    (
                        200,
                        serde_json::json!({"models":[{"name":"models/a"}], "nextPageToken":"second"}),
                    ),
                    (200, serde_json::json!({"models":[{"name":"models/b"}]})),
                ]
            } else {
                vec![
                    (
                        200,
                        serde_json::json!({"data":[{"model":"a"}], "has_more":true,"next_cursor":"second"}),
                    ),
                    (
                        200,
                        serde_json::json!({"data":[{"model":"b"}], "has_more":false,"next_cursor":null}),
                    ),
                ]
            };
            let (mut config, server) = catalog_server(pages);
            if google {
                config.id = "gemini".into();
            }
            assert_eq!(discover(None, config, false).await.unwrap(), ["a", "b"]);
            let requests = server.join().unwrap();
            assert!(
                requests
                    .iter()
                    .all(|r| r.contains("authorization: bearer saved-list-key"))
            );
            assert!(requests[1].starts_with(if google {
                "get /v1/models?pagetoken=second "
            } else {
                "get /v1/models?cursor=second "
            }));
        }
    }

    #[tokio::test]
    async fn repeated_cursors_and_denied_access_do_not_overwrite_saved_models() {
        for denied in [false, true] {
            let pages = if denied {
                vec![(403, serde_json::json!({"error":"forbidden"}))]
            } else {
                vec![
                    (
                        200,
                        serde_json::json!({"data":[{"model":"a"}], "has_more":true,"next_cursor":"same"}),
                    ),
                    (
                        200,
                        serde_json::json!({"data":[{"model":"b"}], "has_more":true,"next_cursor":"same"}),
                    ),
                ]
            };
            let (config, server) = catalog_server(pages);
            let directory = std::env::temp_dir()
                .join(format!("lennette-catalog-failure-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("cache.db");
            let db = crate::db::Database::open(&path).unwrap();
            crate::db::settings::set(
                db.connection(),
                &cache_key(&config),
                &serde_json::to_string(&CachedModels {
                    fetched_at: now(),
                    models: vec!["previous-model".into()],
                })
                .unwrap(),
            )
            .unwrap();
            let error = discover(Some(path.clone()), config.clone(), true)
                .await
                .unwrap_err();
            assert!(error.contains(if denied { "denied" } else { "pagination" }));
            server.join().unwrap();
            assert_eq!(
                cached(Some(&path), &config).unwrap().models,
                ["previous-model"]
            );
            drop(db);
            std::fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn endpoint_and_cache_scope_follow_the_actual_account_and_gateway() {
        let mut config = ProviderConfig::new("gemini", "key-a", "m");
        assert_eq!(
            models_endpoint(&config).unwrap(),
            "https://generativelanguage.googleapis.com/v1beta/openai/models"
        );
        let scope = cache_key(&config);
        config.api_key = "key-b".into();
        assert_ne!(scope, cache_key(&config));
        assert!(!cache_key(&config).contains("key-b"));
        config.base_url = "http://127.0.0.1:1234/v1/chat/completions".into();
        assert_eq!(
            models_endpoint(&config).unwrap(),
            "http://127.0.0.1:1234/v1/models"
        );
        config.base_url = "http://127.0.0.1:1234/custom".into();
        assert!(models_endpoint(&config).is_err());
    }

    #[test]
    fn lists_are_normalized_and_malformed_or_empty_catalogs_fail() {
        assert_eq!(parse_models(&serde_json::json!({"data":[{"id":"b"},{"id":"a"},{"id":"b"},{"id":""},{"id":"image", "modalities":{"output":["image"]}}]})).unwrap(), ["a", "b"]);
        assert_eq!(
            parse_models(&serde_json::json!({"models":[{"name":"models/gemini-test"}]})).unwrap(),
            ["gemini-test"]
        );
        assert!(parse_models(&serde_json::json!({"data":[]})).is_err());
        assert!(parse_models(&serde_json::json!({"error":"bad key"})).is_err());
    }

    #[tokio::test]
    async fn live_discovery_authenticates_then_reuses_persistent_cache_without_network() {
        let directory =
            std::env::temp_dir().join(format!("lennette-models-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("catalog.db");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut config = ProviderConfig::new("merge-gateway", "catalog-test-key", "custom-model");
        config.base_url = format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        );
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let mut buffer = [0; 4096];
            while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let body = r#"{"data":[{"id":"live-chat-b"},{"id":"live-chat-a"}]}"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            String::from_utf8(bytes).unwrap()
        });
        assert_eq!(
            discover(Some(path.clone()), config.clone(), false)
                .await
                .unwrap(),
            ["live-chat-a", "live-chat-b"]
        );
        let request = server.join().unwrap().to_lowercase();
        assert!(request.starts_with("get /v1/models "));
        assert!(request.contains("authorization: bearer catalog-test-key"));
        // The listener has closed. A second discovery can only succeed from disk.
        assert_eq!(
            discover(Some(path.clone()), config.clone(), false)
                .await
                .unwrap()
                .len(),
            2
        );
        let db = crate::db::Database::open(&path).unwrap();
        let raw = crate::db::settings::get(db.connection(), &cache_key(&config))
            .unwrap()
            .unwrap();
        assert!(!raw.contains("catalog-test-key"));
        assert!(cached(Some(&path), &config).unwrap().fresh());
        let stale = CachedModels {
            fetched_at: 0,
            models: vec!["stale-model".into()],
        };
        crate::db::settings::set(
            db.connection(),
            &cache_key(&config),
            &serde_json::to_string(&stale).unwrap(),
        )
        .unwrap();
        assert!(!cached(Some(&path), &config).unwrap().fresh());
        assert!(
            discover(Some(path.clone()), config.clone(), false)
                .await
                .is_err()
        );
        assert_eq!(
            cached(Some(&path), &config).unwrap().models,
            ["stale-model"]
        );
        drop(db);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
