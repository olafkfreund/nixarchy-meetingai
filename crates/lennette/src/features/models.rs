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
pub const MODELS: &[ModelList] = &[
    ModelList {
        provider: "openai",
        models: &["gpt-4o-mini", "gpt-4o", "gpt-4.1", "o4-mini"],
    },
    ModelList {
        provider: "claude",
        models: &["claude-sonnet-4-5", "claude-opus-4-1", "claude-haiku-4-5"],
    },
    ModelList {
        provider: "gemini",
        models: &[
            "gemini-flash-lite-latest",
            "gemini-2.5-flash",
            "gemini-2.5-pro",
        ],
    },
    ModelList {
        provider: "grok",
        models: &["grok-4-fast", "grok-4", "grok-3-mini"],
    },
    ModelList {
        provider: "mistral",
        models: &["mistral-small-latest", "mistral-large-latest"],
    },
    ModelList {
        provider: "groq",
        models: &["llama-3.3-70b-versatile", "llama-3.1-8b-instant"],
    },
    ModelList {
        provider: "cohere",
        models: &["command-r-plus", "command-r"],
    },
    ModelList {
        provider: "perplexity",
        models: &["sonar-pro", "sonar"],
    },
    ModelList {
        provider: "openrouter",
        models: &[
            "anthropic/claude-sonnet-4",
            "openai/gpt-4o-mini",
            "google/gemini-2.5-flash",
        ],
    },
    ModelList {
        provider: "ollama",
        models: &["llama3.2", "qwen2.5", "mistral"],
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
    provider_id
        .and_then(default_for)
        .map(str::to_owned)
        .unwrap_or_else(|| "No model".to_owned())
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
        assert_eq!(label(Some("gpt-4o"), Some("openai")), "gpt-4o");
        assert_eq!(label(None, Some("gemini")), "gemini-flash-lite-latest");
        assert_eq!(
            label(Some("   "), Some("gemini")),
            "gemini-flash-lite-latest"
        );
        assert_eq!(label(None, None), "No model");
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
