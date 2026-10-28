use super::providers::{ChatProvider, ChatWire, ProviderConfig};
use crate::core::network::BodyError;
use reqwest::{
    StatusCode, Url,
    header::{HeaderName, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashSet},
    fmt::Write as _,
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
    let mut key = String::from("model_catalog:v3:");
    for byte in Sha256::digest(scope) {
        write!(key, "{byte:02x}").expect("writing to a String");
    }
    key
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
    let age = now_seconds().saturating_sub(entry.fetched_at);
    (!entry.models.is_empty() && age < 6 * 60 * 60).then_some(entry.models)
}

const KEEP: &str = " Saved credentials and models remain available.";
const EMPTY: &str =
    "The model catalogue has no usable models. Saved credentials and models remain available.";
const INCOMPLETE: &str =
    "Model catalogue pagination did not complete. Saved credentials and models remain available.";

fn models_endpoint(config: &ProviderConfig) -> Result<String, String> {
    let endpoint = config
        .endpoint()
        .ok_or_else(|| format!("This provider has no model endpoint.{KEEP}"))?;
    if matches!(config.id.as_str(), "ollama" | "ollama-cloud") {
        let origin = endpoint.split("/v1/").next().unwrap_or(&endpoint);
        return Ok(format!("{}/api/tags", origin.trim_end_matches('/')));
    }
    let prefix = endpoint
        .trim_end_matches('/')
        .strip_suffix("/chat/completions")
        .ok_or_else(|| format!("The chat endpoint does not lead to a model catalogue.{KEEP}"))?;
    Ok(format!("{prefix}/models"))
}

/// How the next page of a listing is requested.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PageCursor {
    /// `nextPageToken` in the reply, sent back as `pageToken`.
    Token(String),
    /// `next_cursor` in the reply, sent back as `cursor`.
    Cursor(String),
}

impl PageCursor {
    fn apply(&self, url: &mut Url) {
        let (name, value) = match self {
            Self::Token(v) => ("pageToken", v),
            Self::Cursor(v) => ("cursor", v),
        };
        url.query_pairs_mut().append_pair(name, value);
    }
}

fn non_empty_str<'a>(body: &'a Value, field: &str) -> Option<&'a str> {
    body.get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// Reads the continuation of a page; `Ok(None)` means it was the last one.
fn next_cursor(body: &Value) -> Result<Option<PageCursor>, String> {
    let more = body.get("has_more").and_then(Value::as_bool);
    match (
        non_empty_str(body, "nextPageToken"),
        more,
        non_empty_str(body, "next_cursor"),
    ) {
        (Some(token), _, _) => Ok(Some(PageCursor::Token(token.to_owned()))),
        (None, Some(false), _) => Ok(None),
        (None, _, Some(cursor)) => Ok(Some(PageCursor::Cursor(cursor.to_owned()))),
        (None, Some(true), None) => Err(format!(
            "The model catalogue announces another page but gives no cursor.{KEEP}"
        )),
        _ => Ok(None),
    }
}

fn usable_id(id: &str) -> bool {
    (1..=240).contains(&id.len()) && id.chars().all(|c| !c.is_control())
}

fn modalities(row: &Value) -> Vec<String> {
    const PATHS: [&str; 3] = [
        "/modalities/output",
        "/capabilities/output",
        "/output_modalities",
    ];
    PATHS
        .iter()
        .find_map(|path| row.pointer(path))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|m| m.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn produces_text(row: &Value) -> bool {
    let output = modalities(row);
    if !output.is_empty() {
        return output.iter().any(|m| m == "text");
    }
    match row
        .get("supportedGenerationMethods")
        .and_then(Value::as_array)
    {
        Some(methods) if !methods.is_empty() => methods
            .iter()
            .any(|m| matches!(m.as_str(), Some("generateContent" | "generateMessage"))),
        _ => true,
    }
}

fn row_id(row: &Value) -> Option<String> {
    let raw = ["model", "id", "name"].iter().find_map(|field| {
        row.get(*field)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
    })?;
    let trimmed = raw.trim();
    let id = trimmed.strip_prefix("models/").unwrap_or(trimmed);
    usable_id(id).then(|| id.to_owned())
}

/// The text-capable model ids of one page (the first 2,000 usable ones).
fn parse_models(page: &Value) -> Result<BTreeSet<String>, String> {
    let rows = ["data", "models"]
        .iter()
        .find_map(|field| page.get(*field).and_then(Value::as_array))
        .ok_or_else(|| format!("The model response contains no model list.{KEEP}"))?;
    Ok(rows
        .iter()
        .filter(|row| produces_text(row))
        .filter_map(row_id)
        .take(2_000)
        .collect())
}

fn status_error(status: StatusCode) -> String {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        format!("Model catalogue access was denied.{KEEP}")
    } else if status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED {
        format!("This endpoint has no model catalogue; keep using your model or enter an id.{KEEP}")
    } else if status.is_client_error() {
        format!(
            "The model catalogue rejected the request (HTTP {}).{KEEP}",
            status.as_u16()
        )
    } else {
        format!(
            "The model catalogue returned HTTP {}.{KEEP}",
            status.as_u16()
        )
    }
}

fn body_error_text(error: &BodyError) -> String {
    if matches!(error, BodyError::TooLarge) {
        format!("The model catalogue is too large.{KEEP}")
    } else {
        format!("The model catalogue response could not be read.{KEEP}")
    }
}

fn auth_headers(config: &ProviderConfig) -> Result<Vec<(HeaderName, HeaderValue)>, String> {
    let bad = || format!("The saved credentials cannot be sent as a header.{KEEP}");
    let pair = match &config.custom {
        Some(custom) => custom.header(&config.api_key),
        None if config.api_key.is_empty() => None,
        None => Some((
            "Authorization".to_owned(),
            format!("Bearer {}", config.api_key),
        )),
    };
    let Some((name, value)) = pair else {
        return Ok(Vec::new());
    };
    let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| bad())?;
    let mut value = HeaderValue::from_str(&value).map_err(|_| bad())?;
    value.set_sensitive(true);
    Ok(vec![(name, value)])
}

async fn get_page(
    client: &reqwest::Client,
    url: Url,
    headers: &[(HeaderName, HeaderValue)],
    budget: usize,
) -> Result<Vec<u8>, String> {
    let mut request = client.get(url).timeout(Duration::from_secs(15));
    for (name, value) in headers {
        request = request.header(name.clone(), value.clone());
    }
    let response = request
        .send()
        .await
        .map_err(|_| format!("The model catalogue could not be reached.{KEEP}"))?;
    if !response.status().is_success() {
        return Err(status_error(response.status()));
    }
    crate::core::network::read_body(response, budget)
        .await
        .map_err(|e| body_error_text(&e))
}

async fn fetch(config: &ProviderConfig) -> Result<Vec<String>, String> {
    let base = Url::parse(&models_endpoint(config)?)
        .map_err(|_| format!("The model catalogue URL is invalid.{KEEP}"))?;
    let headers = auth_headers(config)?;
    let client = crate::ai::engine::http_client();
    let mut ids = BTreeSet::new();
    let mut requested = HashSet::new();
    let mut cursor: Option<PageCursor> = None;
    let mut follow_ups = 0;
    let mut read = 0usize;
    loop {
        let mut url = base.clone();
        if let Some(c) = &cursor {
            c.apply(&mut url);
        }
        let budget = (32 * 1024 * 1024usize)
            .saturating_sub(read)
            .min(4 * 1024 * 1024);
        let body = get_page(&client, url, &headers, budget).await?;
        read += body.len();
        let page: Value = serde_json::from_slice(&body)
            .map_err(|_| format!("The model catalogue is not valid JSON.{KEEP}"))?;
        ids.extend(parse_models(&page)?);
        let Some(next) = next_cursor(&page)? else {
            break;
        };
        follow_ups += 1;
        if ids.len() > 10_000 || follow_ups > 50 || !requested.insert(next.clone()) {
            return Err(INCOMPLETE.to_owned());
        }
        cursor = Some(next);
    }
    if ids.is_empty() {
        return Err(EMPTY.to_owned());
    }
    Ok(ids.into_iter().collect())
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
    let fetched = tokio::select! {
        result = fetch(&config) => result?,
        () = tokio::time::sleep(Duration::from_secs(30)) => {
            return Err(format!("Model catalogue refresh timed out.{KEEP}"));
        }
    };
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
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Instant;

    /// Reads one request (head plus any declared body), giving up after two seconds.
    fn read_request(stream: &mut std::net::TcpStream) -> String {
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut data = Vec::new();
        let mut chunk = [0_u8; 1024];
        while Instant::now() < deadline {
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&data[..end]).to_ascii_lowercase();
                let wanted = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if data.len() >= end + 4 + wanted {
                    break;
                }
            }
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => data.extend_from_slice(&chunk[..n]),
                Err(_) => {}
            }
        }
        String::from_utf8_lossy(&data).into_owned()
    }

    /// Serves the canned replies in order on loopback; stops waiting for a client after 5 s.
    fn server(replies: Vec<(u16, String)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!(
            "http://{}/v3/chat/completions",
            listener.local_addr().unwrap()
        );
        let worker = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, body) in replies {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(_) if Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => return seen,
                    }
                };
                seen.push(read_request(&mut stream));
                let reply = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
            }
            seen
        });
        (url, worker)
    }

    fn gateway(url: String) -> ProviderConfig {
        let mut config = ProviderConfig::new("custom-catalogue", "fixture-token", "chosen-model");
        config.custom = Some(crate::ai::catalog::custom::CustomProvider::default());
        config.base_url = url;
        config
    }

    fn temp_db_path(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.db");
        (dir, path)
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
    fn suggestions_are_complete_and_unique() {
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
    }

    #[test]
    fn page_rows_are_filtered_and_normalised() {
        let page = json!({"data":[
            {"model":"alpha", "id":"wrong", "modalities":{"output":["text"]}},
            {"id":"voice", "capabilities":{"output":["audio"]}},
            {"id":"beta", "output_modalities":[]},
            {"id":"alpha"}, {"id":" "}, {"id":"pic", "output_modalities":["image"]}
        ]});
        let ids: Vec<_> = parse_models(&page).unwrap().into_iter().collect();
        assert_eq!(ids, ["alpha", "beta"]);
        let google = json!({"models":[
            {"name":"models/gamma", "supportedGenerationMethods":["generateContent"]},
            {"name":"models/delta", "supportedGenerationMethods":["countTokens"]}
        ]});
        let ids: Vec<_> = parse_models(&google).unwrap().into_iter().collect();
        assert_eq!(ids, ["gamma"]);
        assert!(parse_models(&json!({})).is_err());
        assert!(parse_models(&json!({"data":[]})).unwrap().is_empty());
    }

    #[test]
    fn a_page_keeps_its_first_two_thousand_ids() {
        let rows: Vec<_> = (0..2_500)
            .map(|i| json!({"id": format!("m{i:04}")}))
            .collect();
        let ids = parse_models(&json!({"data": rows})).unwrap();
        assert_eq!(ids.len(), 2_000);
        assert!(ids.contains("m1999") && !ids.contains("m2000"));
    }

    #[test]
    fn cursors_follow_the_three_reply_styles() {
        assert_eq!(
            next_cursor(&json!({"nextPageToken":"t","has_more":false})).unwrap(),
            Some(PageCursor::Token("t".into()))
        );
        assert_eq!(
            next_cursor(&json!({"next_cursor":"c"})).unwrap(),
            Some(PageCursor::Cursor("c".into()))
        );
        assert_eq!(
            next_cursor(&json!({"has_more":false,"next_cursor":"c"})).unwrap(),
            None
        );
        assert_eq!(next_cursor(&json!({"data":[]})).unwrap(), None);
        assert!(next_cursor(&json!({"has_more":true})).is_err());
        let mut url = Url::parse("http://host/models").unwrap();
        PageCursor::Cursor("c 1".into()).apply(&mut url);
        assert_eq!(url.query(), Some("cursor=c+1"));
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

    #[test]
    fn cache_key_is_pinned() {
        let mut config = ProviderConfig::new("custom-pin", "pin-key", "m");
        config.custom = Some(crate::ai::catalog::custom::CustomProvider::default());
        config.base_url = "https://pin.example/v1/chat/completions".into();
        let json = br#"["custom-pin","https://pin.example/v1/chat/completions","pin-key"]"#;
        let hex: String = Sha256::digest(json)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(cache_key(&config), format!("model_catalog:v3:{hex}"));
        assert_eq!(cache_key(&config).len(), "model_catalog:v3:".len() + 64);
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
    async fn has_more_false_stops_paging_despite_a_cursor() {
        let (url, requests) = server(vec![(
            200,
            r#"{"data":[{"id":"solo"}],"has_more":false,"next_cursor":"unused"}"#.into(),
        )]);
        assert_eq!(fetch(&gateway(url)).await.unwrap(), ["solo"]);
        assert_eq!(requests.join().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn too_many_ids_across_pages_is_an_incomplete_listing() {
        let replies = (0..6)
            .map(|page| {
                let rows: Vec<_> = (0..2_000)
                    .map(|i| json!({"id": format!("p{page}-{i}")}))
                    .collect();
                let body = json!({"data": rows, "next_cursor": format!("c{page}")});
                (200, body.to_string())
            })
            .collect();
        let (url, requests) = server(replies);
        let error = fetch(&gateway(url)).await.unwrap_err();
        assert!(error.contains("pagination"), "{error}");
        requests.join().unwrap();
    }

    #[tokio::test]
    async fn refresh_failure_preserves_the_saved_row() {
        let looping = |id: &str| {
            (
                200,
                format!(r#"{{"data":[{{"id":"{id}"}}],"next_cursor":"loop"}}"#),
            )
        };
        let cases = [
            (vec![(403, "{}".to_owned())], "denied"),
            (vec![looping("one"), looping("two")], "pagination"),
        ];
        for (replies, wording) in cases {
            let (url, requests) = server(replies);
            let config = gateway(url);
            let (dir, path) = temp_db_path("catalogue-failure");
            let db = crate::db::Database::open(&path).unwrap();
            let key = cache_key(&config);
            let saved = r#"{"fetched_at":1,"models":["saved-choice"]}"#;
            crate::db::settings::set(db.connection(), &key, saved).unwrap();
            drop(db);
            let error = discover(Some(path.clone()), config, true)
                .await
                .unwrap_err();
            assert!(error.contains(wording), "{error}");
            let db = crate::db::Database::open(&path).unwrap();
            let stored = crate::db::settings::get(db.connection(), &key).unwrap();
            assert_eq!(stored.as_deref(), Some(saved));
            drop(db);
            requests.join().unwrap();
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[tokio::test]
    async fn an_empty_catalogue_is_an_error_not_an_empty_list() {
        let (url, requests) = server(vec![(200, r#"{"data":[]}"#.into())]);
        let error = discover(None, gateway(url), true).await.unwrap_err();
        assert!(error.contains("no usable models"), "{error}");
        requests.join().unwrap();
    }

    #[tokio::test]
    async fn discovery_reuses_fresh_cache_and_does_not_replace_stale_on_failure() {
        let (url, requests) = server(vec![(
            200,
            r#"{"data":[{"id":"omega"},{"id":"beta"}]}"#.into(),
        )]);
        let config = gateway(url);
        let (dir, path) = temp_db_path("catalogue-cache");
        let listed = discover(Some(path.clone()), config.clone(), false).await;
        assert_eq!(listed.unwrap(), ["beta", "omega"]);
        let request = requests.join().unwrap().pop().unwrap();
        assert!(request.starts_with("GET /v3/models"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-token")
        );
        let again = discover(Some(path.clone()), config.clone(), false).await;
        assert_eq!(again.unwrap(), ["beta", "omega"]);
        let db = crate::db::Database::open(&path).unwrap();
        let key = cache_key(&config);
        let fresh = crate::db::settings::get(db.connection(), &key)
            .unwrap()
            .unwrap();
        assert!(!fresh.contains("fixture-token"));
        let entry: SavedModels = serde_json::from_str(&fresh).unwrap();
        assert!(entry.fetched_at <= now_seconds() && now_seconds() - entry.fetched_at < 6 * 3600);
        let old = r#"{"fetched_at":1,"models":["prior-choice"]}"#;
        crate::db::settings::set(db.connection(), &key, old).unwrap();
        drop(db);
        assert!(discover(Some(path.clone()), config, false).await.is_err());
        let db = crate::db::Database::open(&path).unwrap();
        let stored = crate::db::settings::get(db.connection(), &key).unwrap();
        assert_eq!(stored.as_deref(), Some(old));
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
