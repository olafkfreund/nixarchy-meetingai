use super::custom::CustomProvider;
use crate::core::credentials::{Credential, Kind};

pub struct Preset {
    pub name: &'static str,
    pub speech: bool,
    pub endpoint: &'static str,
    pub model: &'static str,
    pub wire: &'static str,
    pub images: bool,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "OpenAI",
        speech: false,
        endpoint: "https://api.openai.com/v1/chat/completions",
        model: "gpt-4.1-mini",
        wire: "openai",
        images: true,
    },
    Preset {
        name: "Anthropic",
        speech: false,
        endpoint: "https://api.anthropic.com/v1/messages",
        model: "claude-sonnet-5-5",
        wire: "anthropic",
        images: true,
    },
    Preset {
        name: "Groq",
        speech: false,
        endpoint: "https://api.groq.com/openai/v1/chat/completions",
        model: "openai/gpt-oss-20b",
        wire: "openai",
        images: false,
    },
    Preset {
        name: "OpenRouter",
        speech: false,
        endpoint: "https://openrouter.ai/api/v1/chat/completions",
        model: "openai/gpt-4.1-mini",
        wire: "openai",
        images: true,
    },
    Preset {
        name: "Mistral",
        speech: false,
        endpoint: "https://api.mistral.ai/v1/chat/completions",
        model: "mistral-small-latest",
        wire: "openai",
        images: true,
    },
    Preset {
        name: "DeepSeek",
        speech: false,
        endpoint: "https://api.deepseek.com/chat/completions",
        model: "deepseek-flash",
        wire: "openai",
        images: false,
    },
    Preset {
        name: "Together",
        speech: false,
        endpoint: "https://api.together.xyz/v1/chat/completions",
        model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        wire: "openai",
        images: false,
    },
    Preset {
        name: "LM Studio",
        speech: false,
        endpoint: "http://127.0.0.1:1234/v1/chat/completions",
        model: "local-model",
        wire: "openai",
        images: false,
    },
    Preset {
        name: "vLLM",
        speech: false,
        endpoint: "http://127.0.0.1:8000/v1/chat/completions",
        model: "served-model",
        wire: "openai",
        images: false,
    },
    Preset {
        name: "OpenAI",
        speech: true,
        endpoint: "https://api.openai.com/v1/audio/transcriptions",
        model: "whisper-1",
        wire: "multipart-wav",
        images: false,
    },
    Preset {
        name: "Groq",
        speech: true,
        endpoint: "https://api.groq.com/openai/v1/audio/transcriptions",
        model: "whisper-large-v3-turbo",
        wire: "multipart-wav",
        images: false,
    },
];

impl Preset {
    pub fn slug(&self) -> String {
        let slug = self.name.to_lowercase().replace(' ', "-");
        if self.speech && PRESETS.iter().any(|p| !p.speech && p.name == self.name) {
            format!("{slug}-speech")
        } else {
            slug
        }
    }

    pub fn draft(&self) -> Credential {
        let response_path = match (self.speech, self.wire) {
            (true, _) => "/text",
            (false, "anthropic") => "/content/0/text",
            (false, _) => "/choices/0/message/content",
        };
        let custom = CustomProvider {
            name: self.name.into(),
            auth: match (self.endpoint.starts_with("http://"), self.wire) {
                (true, _) => "none",
                (_, "anthropic") => "header",
                _ => "bearer",
            }
            .into(),
            auth_header: if self.wire == "anthropic" {
                "x-api-key".into()
            } else {
                "Authorization".into()
            },
            request_format: self.wire.into(),
            response_path: response_path.into(),
            streaming: !self.speech,
            supports_images: self.images,
            ..Default::default()
        };
        Credential {
            kind: if self.speech {
                Kind::Speech
            } else {
                Kind::Chat
            },
            provider: format!("custom-{}", uuid::Uuid::new_v4()),
            api_key: String::new(),
            model: self.model.into(),
            base_url: self.endpoint.into(),
            custom: Some(custom),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drafts_are_distinct_and_ready_for_keys() {
        for template in PRESETS {
            let mut first = template.draft();
            assert_ne!(first.provider, template.draft().provider);
            assert!(first.provider.starts_with("custom-"));
            assert!(first.api_key.is_empty());
            assert!(super::super::providers::endpoint_allowed(&first.base_url));
            assert_eq!(
                first.to_config().custom.unwrap().streaming,
                !template.speech
            );
            let streaming = first.to_config().speech_streaming();
            assert_eq!(streaming, !template.speech);
            first.api_key = "sample-secret".into();
            assert!(first.validate().is_ok(), "{}", template.name);
        }
    }
}
