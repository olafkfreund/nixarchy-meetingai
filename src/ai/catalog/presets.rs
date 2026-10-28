    Preset {
        name: "Together",
        speech: false,
        endpoint: "https://api.together.xyz/v1/chat/completions",
        model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        wire: "openai",
        images: false,
        docs: "https://docs.together.ai/docs/openai-api-compatibility",
    },
    // Local servers: loopback http, no key.
    Preset {
        name: "LM Studio",
        speech: false,
        endpoint: "http://127.0.0.1:1234/v1/chat/completions",
        model: "local-model",
        wire: "openai",
        images: false,
        docs: "https://lmstudio.ai/docs/app/api/endpoints/openai",
    },
    Preset {
        name: "vLLM",
        speech: false,
        endpoint: "http://127.0.0.1:8000/v1/chat/completions",
        model: "served-model",
        wire: "openai",
        images: false,
        docs: "https://docs.vllm.ai/en/latest/serving/openai_compatible_server.html",
    },
                // Loopback http presets (LM Studio, vLLM) run without a key.
                auth: if self.endpoint.starts_with("http://") {
                    "none"
                } else {
                    "bearer"
                }
                .into(),
            assert!(
                draft.base_url.starts_with("https://")
                    || draft.base_url.starts_with("http://127.0.0.1:")
            );
