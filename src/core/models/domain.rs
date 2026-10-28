//! Domain types for nixarchy-meetingai.

use serde::{Deserialize, Serialize};

/// Participant role in a chat conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

/// A single turn in a conversation, with optional attached screenshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// base64 image payloads, without a `data:` prefix
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
}

impl Message {
    /// Creates a user message with text and no images.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            images: Vec::new(),
        }
    }

    /// Creates an assistant message with text and no images.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            images: Vec::new(),
        }
    }

    /// Returns `true` when there is neither non-whitespace text nor any attached image.
    pub fn is_empty(&self) -> bool {
        self.content.trim().is_empty() && self.images.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_emptiness_checks_whitespace_and_images() {
        assert!(Message::user("   \n\t  ").is_empty());
        assert!(!Message::user("agenda item").is_empty());

        let with_image = Message {
            role: Role::User,
            content: String::new(),
            images: vec!["aW1hZ2U=".into()],
        };
        assert!(!with_image.is_empty());
    }
}
