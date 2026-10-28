use crate::ai::providers::ChatWire;
    #[error("{0}")]
    Cli(String),
pub fn loading_http_client() -> reqwest::Client {
    crate::core::network::loading_chat_client()
}

    json!({ "model": config.model, "messages": messages, "stream": true })
/// Anthropic's native API takes `x-api-key`; everything else takes a bearer token.
fn with_key(
    request: reqwest::RequestBuilder,
    wire: ChatWire,
    key: &str,
) -> reqwest::RequestBuilder {
    if wire == ChatWire::Anthropic {
        request.header("x-api-key", key)
    } else {
        request.bearer_auth(key)
    }
}

    let needs_key = config
        .map_or(provider.requires_key, |c| c.auth != "none");
    if needs_key && config.api_key.trim().is_empty() {
    if provider.wire == ChatWire::Cli {
        return super::cli::stream(config, history, context, cancel, on_text).await;
    }
        with_key(client.post(endpoint), provider.wire, &config.api_key)
pub(super) async fn wait_for_cancellation(cancel: &mut watch::Receiver<bool>) {
        let provider = ChatProvider::by_id("ollama").unwrap();
        let config = ProviderConfig::new("ollama", "k", "gpt-4o");
        let provider = ChatProvider::by_id("ollama").unwrap();
        let config = ProviderConfig::new("ollama", "k", "gpt-4o");
        let provider = ChatProvider::by_id("ollama").unwrap();
        let config = ProviderConfig::new("ollama", "k", "m");
        let openai = ChatProvider::by_id("ollama").unwrap();
        let openai = ChatProvider::by_id("ollama").unwrap();
    #[test]
    fn anthropic_wire_sends_x_api_key_not_bearer() {
        let client = reqwest::Client::new();
        let request = with_key(
            client.post("https://example.test"),
            ChatWire::Anthropic,
            "sk-a",
        )
        .build()
        .unwrap();
        assert_eq!(request.headers()["x-api-key"], "sk-a");
        assert!(request.headers().get("authorization").is_none());
        let request = with_key(
            client.post("https://example.test"),
            ChatWire::OpenAi,
            "sk-a",
        )
        .build()
        .unwrap();
        assert_eq!(request.headers()["authorization"], "Bearer sk-a");
    }

        let mut config = ProviderConfig::new("custom-test", "", "gpt-4o");
        config.custom = Some(Default::default());
        config.base_url = "http://127.0.0.1:1/v1/chat/completions".into();
