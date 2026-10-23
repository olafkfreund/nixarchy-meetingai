//! Explicit connection templates, reviewed against provider docs on 2026-10-24.
//! These create ordinary custom credentials; keys and model choices remain editable.
use super::custom::CustomProvider;
use crate::core::credentials::{Credential, Kind};

pub struct Preset {
    pub name: &'static str,
    pub speech: bool,
    pub endpoint: &'static str,
    pub model: &'static str,
    pub wire: &'static str,
    pub images: bool,
    pub docs: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "OpenAI",
        speech: false,
        endpoint: "https://api.openai.com/v1/chat/completions",
        model: "gpt-4.1-mini",
        wire: "openai",
        images: true,
        docs: "https://developers.openai.com/api/reference/resources/chat",
    },
    Preset {
        name: "Anthropic",
        speech: false,
        endpoint: "https://api.anthropic.com/v1/messages",
        model: "claude-sonnet-5-5",
        wire: "anthropic",
        images: true,
        docs: "https://platform.claude.com/docs/en/api/messages/create",
    },
    Preset {
        name: "Groq",
        speech: false,
        endpoint: "https://api.groq.com/openai/v1/chat/completions",
        model: "openai/gpt-oss-20b",
        wire: "openai",
        images: false,
        docs: "https://console.groq.com/docs/openai",
    },
    Preset {
        name: "OpenRouter",
        speech: false,
        endpoint: "https://openrouter.ai/api/v1/chat/completions",
        model: "openai/gpt-4.1-mini",
        wire: "openai",
        images: true,
        docs: "https://openrouter.ai/docs/api_reference/overview",
    },
    Preset {
        name: "Mistral",
        speech: false,
        endpoint: "https://api.mistral.ai/v1/chat/completions",
        model: "mistral-small-latest",
        wire: "openai",
        images: true,
        docs: "https://docs.mistral.ai/api/endpoint/chat",
    },
    Preset {
        name: "DeepSeek",
        speech: false,
        endpoint: "https://api.deepseek.com/chat/completions",
        model: "deepseek-flash",
        wire: "openai",
        images: false,
        docs: "https://api-docs.deepseek.com/",
    },
    Preset {
        name: "OpenAI",
        speech: true,
        endpoint: "https://api.openai.com/v1/audio/transcriptions",
        model: "whisper-1",
        wire: "multipart-wav",
        images: false,
        docs: "https://developers.openai.com/api/reference/resources/audio/subresources/transcriptions/methods/create",
    },
    Preset {
        name: "Groq",
        speech: true,
        endpoint: "https://api.groq.com/openai/v1/audio/transcriptions",
        model: "whisper-large-v3-turbo",
        wire: "multipart-wav",
        images: false,
        docs: "https://console.groq.com/docs/speech-to-text",
    },
];

impl Preset {
    pub fn draft(&self) -> Credential {
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
            custom: Some(CustomProvider {
                name: self.name.into(),
                request_format: self.wire.into(),
                response_path: if self.speech {
                    "/text"
                } else if self.wire == "anthropic" {
                    "/content/0/text"
                } else {
                    "/choices/0/message/content"
                }
                .into(),
                streaming: !self.speech,
                supports_images: self.images,
                ..Default::default()
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn templates_validate_and_stay_separate_from_retired_provider_ids() {
        for preset in PRESETS {
            let mut draft = preset.draft();
            assert!(draft.provider.starts_with("custom-"));
            assert!(draft.api_key.is_empty());
            draft.api_key = "test-key".into();
            draft.validate().unwrap();
            assert!(draft.base_url.starts_with("https://"));
            assert!(preset.docs.starts_with("https://"));
            assert_eq!(draft.to_config().speech_streaming(), !preset.speech);
            assert_ne!(draft.provider, preset.draft().provider);
        }
    }
}
