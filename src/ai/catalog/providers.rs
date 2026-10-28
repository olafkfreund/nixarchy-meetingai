//! and request shapes for Ollama, the subscription CLIs and Deepgram. Removed providers
    /// A subscription CLI (`claude` / `codex` / `agy`) driven as a subprocess.
    Cli,
    pub requires_key: bool,
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

    /// Decoded inside the daemon (Parakeet through sherpa-onnx); no URL, no key.
    InProcess,
    /// OpenAI-compatible multipart transcription.
    OpenAiMultipart,
pub const STT_PROVIDERS: &[SttProvider] = &[
    SttProvider {
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
    pub fn requires_key(&self) -> bool {
        self.wire == SttWire::RawAudio
    }
        if provider.wire == ChatWire::Cli {
            return Some(String::new());
        }
            return endpoint_allowed(&self.base_url).then(|| self.base_url.clone());
            .map(|provider| provider.requires_key)
        self.speech_usable_in(crate::speech::stt::parakeet::model_dir())
    }

    /// [`Self::speech_usable`] with the Parakeet model directory given.
    fn speech_usable_in(&self, parakeet_dir: Option<std::path::PathBuf>) -> bool {
            SttProvider::by_id(&self.id).is_some_and(|p| {
                if p.wire == SttWire::InProcess {
                    return crate::speech::stt::parakeet::model_ready_in(parakeet_dir);
                }
                (!p.requires_key() || !self.api_key.trim().is_empty())
                    && (p.wire != SttWire::OpenAiMultipart
                        || (endpoint_allowed(&self.base_url) && !self.model.trim().is_empty()))
            })
        match &self.custom {
            Some(custom) => custom.streaming,
            // Only Deepgram's raw-audio wire streams; OpenAI multipart and Parakeet are batch.
            None => SttProvider::by_id(&self.id).is_none_or(|p| p.wire == SttWire::RawAudio),
        }
                requires_key: custom.auth != "none",
            assert!(
                provider.wire == ChatWire::Cli || provider.endpoint.starts_with("http"),
                "{}",
                provider.id
            );
            if provider.wire != ChatWire::Cli && provider.id != "ollama" {
        assert_eq!(
            ChatProvider::by_id("agy-sub").unwrap().name,
            "Antigravity (subscription)"
        );
        assert!(ChatProvider::by_id("agy-sub ").is_none());
            [
                "claude-sub",
                "agy-sub",
                "opencode",
                "codex-sub",
                "ollama-cloud",
                "ollama"
            ]
        assert_eq!(STT_PROVIDERS.len(), 3);
        let retired = ProviderConfig::new("openai", "k", "m");
    fn providers_require_a_model() {
        assert!(ProviderConfig::new("ollama", "", "gpt-4o").is_usable());
        assert!(!ProviderConfig::new("ollama", "", "  ").is_usable());
        // An unknown id has no endpoint.
        assert!(!ProviderConfig::new("unknown", "", "m").is_usable());
        let mut config = ProviderConfig::new("ollama", "k", "m");
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

        assert!(SttProvider::by_id("whisper-local").is_none());

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
