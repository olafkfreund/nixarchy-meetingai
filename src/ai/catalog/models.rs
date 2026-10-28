use super::providers::{ChatProvider, ChatWire, ProviderConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn suggestions(id: &str) -> &'static [&'static str] {
    match id {
        "claude-sub" => &["sonnet", "opus", "haiku"],
        "agy-sub" => &["gemini-3.8-flash-low"],
        "codex-sub" => &[
            crate::ai::chat::cli::CLI_DEFAULT_MODEL,
            "gpt-6.1-sol",
            "gpt-6-sol",
            "gpt-5.6-sol",
        ],
        "ollama-cloud" => &["gpt-oss:120b"],
        "ollama" => &["llama3.2"],
        _ => &[],
    }
}

pub fn default_for(provider_id: &str) -> Option<&'static str> {
    suggestions(provider_id).first().copied()
}

/// Whether `model` can run on `provider_id`: OpenCode ids are `provider/model`; agy ids must be
/// one of `agy_known` (accepted while that list is not known yet).
pub fn valid_for(provider_id: &str, model: &str, agy_known: &[String]) -> bool {
    match provider_id {
        "opencode" => model
            .split_once('/')
            .is_some_and(|(p, m)| !p.is_empty() && !m.is_empty()),
        "agy-sub" => agy_known.is_empty() || agy_known.iter().any(|k| k == model),
        _ => true,
    }
}

#[derive(Serialize, Deserialize)]
struct SavedModels {
    fetched_at: u64,
    models: Vec<String>,
}

fn cache_key(config: &ProviderConfig) -> String {
    let scope = serde_json::to_vec(&serde_json::json!([
        config.id,
        config.endpoint(),
        config.api_key
    ]))
    .expect("scope serializes");
    let hash: String = Sha256::digest(scope)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("model_catalog:v3:{hash}")
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cached(db: &crate::db::Database, key: &str) -> Option<Vec<String>> {
    let raw = crate::db::settings::get(db.connection(), key).ok()??;
    let entry: SavedModels = serde_json::from_str(&raw).ok()?;
    (!entry.models.is_empty() && now_seconds().saturating_sub(entry.fetched_at) < 6 * 60 * 60)
        .then_some(entry.models)
}

fn models_endpoint(config: &ProviderConfig) -> Result<String, String> {
    let endpoint = config
        .endpoint()
        .ok_or_else(|| format!("This provider has no model endpoint.{KEEP}"))?;
    if matches!(config.id.as_str(), "ollama" | "ollama-cloud") {
        let origin = endpoint.split("/v1/").next().unwrap_or(&endpoint);
        return Ok(format!("{}/api/tags", origin.trim_end_matches('/')));
    }
    let end = endpoint.trim_end_matches('/');
    let prefix = end.strip_suffix("/chat/completions")
        .ok_or_else(|| "The chat endpoint does not lead to a model catalogue; saved credentials and models remain available.".to_owned())?;
    Ok(format!("{prefix}/models"))
}

const KEEP: &str = " Saved credentials and models remain available.";
const EMPTY: &str =
    "The model catalogue has no usable models. Saved credentials and models remain available.";

fn parse_models(page: &Value) -> Result<Vec<String>, String> {
    let rows = page
        .get("data")
        .or_else(|| page.get("models"))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("The model response contains no model list.{KEEP}"))?;
    let mut ids = Vec::new();
    for row in rows {
        let output = row
            .pointer("/modalities/output")
            .or_else(|| row.pointer("/capabilities/output"))
            .or_else(|| row.get("output_modalities"));
        let outputs = output.and_then(Value::as_array);
        if let Some(values) = outputs.filter(|values| !values.is_empty()) {
            if !values.iter().any(|v| v.as_str() == Some("text")) {
                continue;
            }
        } else if let Some(methods) = row
            .get("supportedGenerationMethods")
            .and_then(Value::as_array)
            && !methods.is_empty()
            && !methods
                .iter()
                .any(|v| matches!(v.as_str(), Some("generateContent" | "generateMessage")))
        {
            continue;
        }
        let raw = ["model", "id", "name"]
            .iter()
            .filter_map(|field| row.get(*field).and_then(Value::as_str))
            .find(|id| !id.trim().is_empty());
        let Some(raw) = raw else {
            continue;
        };
        let id = raw.trim().strip_prefix("models/").unwrap_or(raw.trim());
        if id.is_empty() || id.len() > 240 || id.chars().any(char::is_control) {
            continue;
        }
        ids.push(id.to_owned());
        if ids.len() > 2_000 {
            return Err(format!("One model page has too many entries.{KEEP}"));
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

async fn fetch(config: &ProviderConfig) -> Result<Vec<String>, String> {
    let address = models_endpoint(config)?;
    let client = crate::ai::engine::http_client();
    let mut next: Option<(&str, String)> = None;
    let mut visited = HashSet::new();
    let mut all = Vec::new();
    let mut bytes_seen = 0usize;
    for page_index in 0..=50 {
        let mut url = reqwest::Url::parse(&address)
            .map_err(|_| format!("The model catalogue URL is invalid.{KEEP}"))?;
        if let Some((parameter, token)) = &next {
            url.query_pairs_mut().append_pair(parameter, token);
        }
        let mut request = client.get(url).timeout(Duration::from_secs(15));
        if let Some(custom) = &config.custom {
            if let Some((name, value)) = custom.header(&config.api_key) {
                request = request.header(name, value);
            }
        } else if !config.api_key.is_empty() {
            request = request.bearer_auth(&config.api_key);
        }
        let response = request
            .send()
            .await
            .map_err(|_| format!("The model catalogue could not be reached.{KEEP}"))?;
        let code = response.status();
        if !code.is_success() {
            return Err(match code.as_u16() {
                401 | 403 => format!("Model catalogue access was denied.{KEEP}"),
                404 | 405 => format!(
                    "This endpoint has no model catalogue; keep using your model or enter an id.{KEEP}"
                ),
                _ => format!("The model catalogue returned HTTP {}.{KEEP}", code.as_u16()),
            });
        }
        let remaining = (32 * 1024 * 1024usize)
            .saturating_sub(bytes_seen)
            .min(4 * 1024 * 1024);
        let body = crate::core::network::read_body(response, remaining)
            .await
            .map_err(|error| match error {
                crate::core::network::BodyError::TooLarge => {
                    format!("The model catalogue is too large.{KEEP}")
                }
                _ => format!("The model catalogue response could not be read.{KEEP}"),
            })?;
        bytes_seen += body.len();
        let page: Value = serde_json::from_slice(&body)
            .map_err(|_| format!("The model catalogue is not valid JSON.{KEEP}"))?;
        all.extend(parse_models(&page)?);
        if all.len() > 10_000 {
            return Err(format!(
                "Model catalogue pagination did not complete.{KEEP}"
            ));
        }
        let token = page
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| ("pageToken", s.to_owned()))
            .or_else(|| {
                (page.get("has_more").and_then(Value::as_bool) != Some(false))
                    .then(|| {
                        page.get("next_cursor")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                    })
                    .flatten()
                    .map(|s| ("cursor", s.to_owned()))
            });
        let Some(token) = token else {
            if page.get("has_more").and_then(Value::as_bool) == Some(true) {
                return Err(format!(
                    "The model catalogue says another page exists but gives no cursor.{KEEP}"
                ));
            }
            all.sort();
            all.dedup();
            return if all.is_empty() {
                Err(EMPTY.into())
            } else {
                Ok(all)
            };
        };
        if page_index == 50 || !visited.insert(token.clone()) {
            return Err(format!(
                "Model catalogue pagination did not complete.{KEEP}"
            ));
        }
        next = Some(token);
    }
    Err(format!(
        "Model catalogue pagination did not complete.{KEEP}"
    ))
}

/// Whether an `/api/show` reply says the model can generate text (embedding-only models cannot).
/// Older servers send no `capabilities`; those are kept.
fn has_completion(show: &serde_json::Value) -> bool {
    show.get("capabilities")
        .and_then(serde_json::Value::as_array)
        .is_none_or(|caps| caps.iter().any(|c| c.as_str() == Some("completion")))
}

/// Keep the models Ollama says can complete text. Answers are cached for the daemon's run;
/// a model whose `/api/show` fails is kept and asked again next time.
async fn completion_only(
    config: &super::providers::ProviderConfig,
    models: Vec<String>,
) -> Vec<String> {
    use futures_util::StreamExt;
    static SHOWN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, bool>>> =
        std::sync::OnceLock::new();
    let Ok(tags) = models_endpoint(config) else {
        return models;
    };
    let origin = tags.trim_end_matches("/api/tags").to_owned();
    let cache = SHOWN.get_or_init(Default::default);
    let client = crate::ai::engine::http_client();
    let check = |model: String| {
        let (client, origin) = (client.clone(), origin.clone());
        async move {
            let key = format!("{origin}\0{model}");
            if let Some(known) = cache.lock().expect("show cache").get(&key) {
                return (model, *known);
            }
            let shown = async {
                let response = client
                    .post(format!("{origin}/api/show"))
                    .json(&serde_json::json!({"model": model}))
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                    .ok()?;
                response.status().is_success().then_some(())?;
                let bytes = crate::core::network::read_body(response, 1024 * 1024)
                    .await
                    .ok()?;
                serde_json::from_slice::<serde_json::Value>(&bytes).ok()
            }
            .await;
            match shown {
                Some(show) => {
                    let ok = has_completion(&show);
                    cache.lock().expect("show cache").insert(key, ok);
                    (model, ok)
                }
                None => (model, true),
            }
        }
    };
    let mut kept: Vec<String> = futures_util::stream::iter(models)
        .map(check)
        .buffer_unordered(8)
        .filter_map(|(model, ok)| async move { ok.then_some(model) })
        .collect()
        .await;
    kept.sort();
    kept
}

pub async fn discover(
    path: Option<PathBuf>,
    config: ProviderConfig,
    force: bool,
) -> Result<Vec<String>, String> {
    if ChatProvider::by_id(&config.id).is_some_and(|p| p.wire == ChatWire::Cli) {
        let live = match config.id.as_str() {
            "opencode" => crate::ai::chat::opencode::models().await,
            "codex-sub" => crate::ai::chat::cli::codex_models().await,
            "agy-sub" => crate::ai::chat::cli::agy_models().await,
            _ => Err("No live list".into()),
        };
        return Ok(live.unwrap_or_else(|_| {
            suggestions(&config.id)
                .iter()
                .map(|s| (*s).to_owned())
                .collect()
        }));
    }
    let db = path
        .as_ref()
        .and_then(|p| crate::db::Database::open(p).ok());
    let key = cache_key(&config);
    if !force && let Some(models) = db.as_ref().and_then(|db| cached(db, &key)) {
        return Ok(models);
    }
    let fetched = tokio::time::timeout(Duration::from_secs(30), fetch(&config))
        .await
        .map_err(|_| format!("Model catalogue refresh timed out.{KEEP}"))??;
    let models = if config.id == "ollama" {
        completion_only(&config, fetched).await
    } else {
        fetched
    };
    if models.is_empty() {
        return Err(EMPTY.into());
    }
    if let Some(db) = &db {
        let saved = SavedModels {
            fetched_at: now_seconds(),
            models: models.clone(),
        };
        if let Ok(value) = serde_json::to_string(&saved) {
            let _ = crate::db::settings::set(db.connection(), &key, &value);
        }
    }
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn server(replies: Vec<(u16, String)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/v3/chat/completions",
            listener.local_addr().unwrap()
        );
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0_u8; 4096];
                let count = stream.read(&mut buffer).unwrap();
                requests.push(String::from_utf8_lossy(&buffer[..count]).into_owned());
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
            requests
        });
        (url, worker)
    }

    fn gateway(url: String) -> ProviderConfig {
        let mut config = ProviderConfig::new("custom-catalogue", "fixture-token", "chosen-model");
        config.custom = Some(crate::ai::catalog::custom::CustomProvider::default());
        config.base_url = url;
        config
    }

    #[test]
    fn models_are_checked_against_their_provider() {
        use super::valid_for;
        assert!(valid_for("opencode", "openai/gpt-a", &[]));
        assert!(!valid_for("opencode", "gpt-a", &[]));
        assert!(!valid_for("opencode", "/gpt-a", &[]));
        let known = ["gemini-3.8-flash-low".to_owned()];
        assert!(valid_for("agy-sub", "gemini-3.8-flash-low", &known));
        assert!(!valid_for("agy-sub", "gemini-2.5-flash", &known));
        assert!(valid_for("agy-sub", "anything", &[]));
        assert!(valid_for("ollama", "llama3.2", &[]));
        assert_eq!(default_for("opencode"), None);
    }

    #[test]
    fn show_capabilities_decide_whether_a_model_can_chat() {
        let chat = serde_json::json!({"capabilities":["completion","tools"]});
        let embed = serde_json::json!({"capabilities":["embedding"]});
        let old = serde_json::json!({"details":{}});
        assert!(has_completion(&chat));
        assert!(!has_completion(&embed));
        assert!(has_completion(&old));
    }

    #[test]
    fn suggestions_and_page_shapes() {
        for provider in super::super::providers::CHAT_PROVIDERS
            .iter()
            .filter(|p| p.id != "opencode")
        {
            let list = suggestions(provider.id);
            assert!(!list.is_empty());
            let unique: HashSet<_> = list.iter().collect();
            assert_eq!(unique.len(), list.len());
        }
        assert!(suggestions("retired-chat").is_empty());
        assert_eq!(default_for("retired-chat"), None);
        let rows = serde_json::json!({"data":[
            {"model":"alpha", "id":"wrong", "modalities":{"output":["text"]}},
            {"id":"audio", "capabilities":{"output":["audio"]}},
            {"id":"beta", "output_modalities":[]},
            {"id":"alpha"}, {"id":" "}, {"id":"image", "output_modalities":["image"]}
        ]});
        assert_eq!(parse_models(&rows).unwrap(), ["alpha", "beta"]);
        assert_eq!(parse_models(&serde_json::json!({"models":[{"name":"models/gamma", "supportedGenerationMethods":["generateContent"]}, {"name":"models/delta", "supportedGenerationMethods":["embedContent"]}]})).unwrap(), ["gamma"]);
        assert!(parse_models(&serde_json::json!({})).is_err());
        assert!(
            parse_models(&serde_json::json!({"data":[]}))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn endpoint_and_scope() {
        let mut config = ProviderConfig::new("custom-gateway", "secret-one", "model");
        config.custom = Some(crate::ai::catalog::custom::CustomProvider::default());
        config.base_url = "https://gateway.example/v2/chat/completions/".into();
        assert_eq!(
            models_endpoint(&config).unwrap(),
            "https://gateway.example/v2/models"
        );
        let first = cache_key(&config);
        assert!(!first.contains("secret-one"));
        config.api_key = "secret-two".into();
        assert_ne!(first, cache_key(&config));
        config.base_url = "http://localhost:9000/v1/chat/completions".into();
        assert_eq!(
            models_endpoint(&config).unwrap(),
            "http://localhost:9000/v1/models"
        );
        config.base_url = "https://gateway.example/reply".into();
        assert!(models_endpoint(&config).is_err());
        assert_eq!(
            models_endpoint(&ProviderConfig::new("ollama", "", "m")).unwrap(),
            "http://127.0.0.1:11434/api/tags"
        );
    }

    #[tokio::test]
    async fn both_pagination_styles_keep_auth_and_sort_ids() {
        for (first, query) in [
            (
                r#"{"data":[{"id":"zeta"}],"nextPageToken":"two"}"#,
                "pageToken=two",
            ),
            (
                r#"{"data":[{"id":"zeta"}],"has_more":true,"next_cursor":"two"}"#,
                "cursor=two",
            ),
        ] {
            let (url, requests) = server(vec![
                (200, first.into()),
                (
                    200,
                    r#"{"data":[{"id":"alpha"},{"id":"zeta"}],"has_more":false}"#.into(),
                ),
            ]);
            assert_eq!(fetch(&gateway(url)).await.unwrap(), ["alpha", "zeta"]);
            let requests = requests.join().unwrap();
            assert_eq!(requests.len(), 2);
            for request in &requests {
                assert!(request.starts_with("GET /v3/models"));
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer fixture-token")
                );
            }
            assert!(requests[1].contains(query));
        }
    }

    #[tokio::test]
    async fn refresh_failure_preserves_the_saved_row() {
        for replies in [
            vec![(403, "{}".into())],
            vec![
                (
                    200,
                    r#"{"data":[{"id":"one"}],"next_cursor":"loop"}"#.into(),
                ),
                (
                    200,
                    r#"{"data":[{"id":"two"}],"next_cursor":"loop"}"#.into(),
                ),
            ],
        ] {
            let (url, requests) = server(replies);
            let config = gateway(url);
            let directory =
                std::env::temp_dir().join(format!("catalogue-failure-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("state.db");
            let db = crate::db::Database::open(&path).unwrap();
            let key = cache_key(&config);
            let saved = r#"{"fetched_at":1,"models":["saved-choice"]}"#;
            crate::db::settings::set(db.connection(), &key, saved).unwrap();
            drop(db);
            assert!(discover(Some(path.clone()), config, true).await.is_err());
            let db = crate::db::Database::open(&path).unwrap();
            assert_eq!(
                crate::db::settings::get(db.connection(), &key)
                    .unwrap()
                    .as_deref(),
                Some(saved)
            );
            drop(db);
            requests.join().unwrap();
            std::fs::remove_dir_all(directory).unwrap();
        }
    }

    #[tokio::test]
    async fn discovery_reuses_fresh_cache_and_does_not_replace_stale_on_failure() {
        let (url, requests) = server(vec![(
            200,
            r#"{"data":[{"id":"omega"},{"id":"beta"}]}"#.into(),
        )]);
        let config = gateway(url);
        let directory =
            std::env::temp_dir().join(format!("catalogue-cache-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("state.db");
        assert_eq!(
            discover(Some(path.clone()), config.clone(), false)
                .await
                .unwrap(),
            ["beta", "omega"]
        );
        let request = requests.join().unwrap().pop().unwrap();
        assert!(request.starts_with("GET /v3/models"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-token")
        );
        assert_eq!(
            discover(Some(path.clone()), config.clone(), false)
                .await
                .unwrap(),
            ["beta", "omega"]
        );
        let db = crate::db::Database::open(&path).unwrap();
        let key = cache_key(&config);
        let fresh = crate::db::settings::get(db.connection(), &key)
            .unwrap()
            .unwrap();
        assert!(!fresh.contains("fixture-token"));
        let entry: SavedModels = serde_json::from_str(&fresh).unwrap();
        assert!(
            entry.fetched_at <= now_seconds() && now_seconds() - entry.fetched_at < 6 * 60 * 60
        );
        let old = r#"{"fetched_at":1,"models":["prior-choice"]}"#;
        crate::db::settings::set(db.connection(), &key, old).unwrap();
        drop(db);
        assert!(discover(Some(path.clone()), config, false).await.is_err());
        let db = crate::db::Database::open(&path).unwrap();
        assert_eq!(
            crate::db::settings::get(db.connection(), &key)
                .unwrap()
                .as_deref(),
            Some(old)
        );
        drop(db);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
