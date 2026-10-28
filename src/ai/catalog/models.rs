        provider: "agy-sub",
        models: &["gemini-3.8-flash-low"],
    // Ollama's real list comes from /api/tags; this is only the starting choice.
        provider: "ollama",
        models: &["llama3.2"],
    },
    ModelList {
        provider: "claude-sub",
        models: &["sonnet", "opus", "haiku"],
    },
    // "default" lets the CLI choose; any other id is passed with -m.
    ModelList {
        provider: "codex-sub",
        models: &[
            super::super::chat::cli::CLI_DEFAULT_MODEL,
            "gpt-6.1-sol",
            "gpt-6-sol",
            "gpt-5.6-sol",
        ],
    },
    // OpenCode has no static default: its free models run out and the real list comes from
    // `opencode serve`, so it joins automatic chains only once a model was picked.
    // The real list comes from ollama.com/api/tags.
    ModelList {
        provider: "ollama-cloud",
        models: &["gpt-oss:120b"],
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

        // OpenCode has no static list on purpose (see MODELS).
        for provider in super::super::providers::CHAT_PROVIDERS
            .iter()
            .filter(|p| p.id != "opencode")
        {
    #[test]
    fn show_capabilities_decide_whether_a_model_can_chat() {
        let chat = serde_json::json!({"capabilities":["completion","tools"]});
        let embed = serde_json::json!({"capabilities":["embedding"]});
        let old = serde_json::json!({"details":{}});
        assert!(has_completion(&chat));
        assert!(!has_completion(&embed));
        assert!(has_completion(&old));
    }

        assert_eq!(label(Some("gpt-4o"), Some("agy-sub")), "gpt-4o");
        assert_eq!(label(None, Some("agy-sub")), "gemini-3.8-flash-low");
        assert_eq!(label(Some("   "), Some("agy-sub")), "gemini-3.8-flash-low");
        "model_catalog:v3:{}",
    if matches!(config.id.as_str(), "ollama" | "ollama-cloud") {
        let origin = endpoint.split("/v1/").next().unwrap_or(&endpoint);
        return Ok(format!("{}/api/tags", origin.trim_end_matches('/')));
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

    if super::providers::ChatProvider::by_id(&config.id)
        .is_some_and(|p| p.wire == super::providers::ChatWire::Cli)
    {
        if config.id == "opencode"
            && let Ok(models) = crate::ai::chat::opencode::models().await
        {
            return Ok(models);
        }
        if config.id == "codex-sub"
            && let Ok(models) = crate::ai::chat::cli::codex_models().await
        {
            return Ok(models);
        }
        if config.id == "agy-sub"
            && let Ok(models) = crate::ai::chat::cli::agy_models().await
        {
            return Ok(models);
        }
        return Ok(for_provider(&config.id)
            .iter()
            .map(|m| (*m).to_owned())
            .collect());
    }
    let models = if config.id == "ollama" {
        completion_only(&config, models).await
    } else {
        models
    };
    if models.is_empty() {
        return Err(EMPTY_CATALOG.into());
    }
        } else if !config.api_key.is_empty() {
    fn catalog_uses_canonical_model_ids_and_output_modalities() {
            {"model":"chat-model", "modalities":{"output":["text"]}},
            {"model":"audio-only", "modalities":{"output":["audio"]}},
            ["canonical-name", "chat-model", "unknown-output"]
        );
        assert_eq!(
            parse_models(&serde_json::json!({"models":[{"name":"llama3.2:latest"}]})).unwrap(),
            ["llama3.2:latest"]
    /// A keyed OpenAI-style provider; the catalog has none built in.
    fn openai_config(key: &str, model: &str) -> ProviderConfig {
        let mut config = ProviderConfig::new("custom-test", key, model);
        config.custom = Some(crate::ai::catalog::custom::CustomProvider::default());
        config
    }

        let mut config = openai_config("saved-list-key", "my-custom-model");
    async fn pagination_keep_auth_on_the_configured_endpoint() {
            let directory = std::env::temp_dir().join(format!(
                "nixarchy-meetingai-catalog-failure-{}",
                uuid::Uuid::new_v4()
            ));
        let mut config = openai_config("key-a", "m");
        config.base_url = "https://api.example.test/v1/chat/completions".into();
            "https://api.example.test/v1/models"
        let ollama = ProviderConfig::new("ollama", "", "m");
        assert_eq!(
            models_endpoint(&ollama).unwrap(),
            "http://127.0.0.1:11434/api/tags"
        );
        let directory = std::env::temp_dir().join(format!(
            "nixarchy-meetingai-models-{}",
            uuid::Uuid::new_v4()
        ));
        let mut config = openai_config("catalog-test-key", "custom-model");
