//! Domain types for nixarchy-meetingai.
//!
//! These mirror the observable behaviour of the reference build: a two-mode
//! overlay (`Ask` / `Listen`), an overlay size preference, and a chat message
//! with a role. Everything here is pure and unit-testable; no I/O, no UI.

use serde::{Deserialize, Serialize};

/// The overlay's two top-level modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// Follow a live conversation and answer in real time.
    #[default]
    Listen,
    /// Ask a question or follow-up query.
    Ask,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::Ask, Mode::Listen];

    pub const fn label(self) -> &'static str {
        match self {
            Mode::Ask => "Ask",
            Mode::Listen => "Listen",
        }
    }

    pub const fn next(self) -> Mode {
        match self {
            Mode::Ask => Mode::Listen,
            Mode::Listen => Mode::Ask,
        }
    }
}

/// The overlay's preset sizes. `Full` fills the monitor work area, which the
/// window layer resolves at apply time rather than storing a fixed size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlaySize {
    #[default]
    Small,
    Medium,
    Large,
    ExtraLarge,
    Full,
}

impl OverlaySize {
    pub const ALL: [OverlaySize; 5] = [
        OverlaySize::Small,
        OverlaySize::Medium,
        OverlaySize::Large,
        OverlaySize::ExtraLarge,
        OverlaySize::Full,
    ];

    /// Short label shown in the size switcher.
    pub const fn label(self) -> &'static str {
        match self {
            OverlaySize::Small => "S",
            OverlaySize::Medium => "M",
            OverlaySize::Large => "L",
            OverlaySize::ExtraLarge => "XL",
            OverlaySize::Full => "Full",
        }
    }

    /// Preset window size in logical pixels. `Full` has no fixed size.
    pub const fn dimensions(self) -> Option<(f32, f32)> {
        match self {
            OverlaySize::Small => Some((720.0, 620.0)),
            OverlaySize::Medium => Some((900.0, 680.0)),
            OverlaySize::Large => Some((1080.0, 760.0)),
            OverlaySize::ExtraLarge => Some((1280.0, 860.0)),
            OverlaySize::Full => None,
        }
    }

    pub fn from_label(label: &str) -> Option<OverlaySize> {
        match label {
            "S" => Some(OverlaySize::Small),
            "M" => Some(OverlaySize::Medium),
            "L" => Some(OverlaySize::Large),
            "XL" => Some(OverlaySize::ExtraLarge),
            "Full" => Some(OverlaySize::Full),
            _ => None,
        }
    }
}

/// Who produced a chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

impl Role {
    pub const fn is_user(self) -> bool {
        matches!(self, Role::User)
    }
}

/// One turn in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Optional image attachments as raw encoded bytes (no data-URL prefix).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            images: Vec::new(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            images: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.content.trim().is_empty() && self.images.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_cycles_between_two_modes() {
        assert_eq!(Mode::Ask.next(), Mode::Listen);
        assert_eq!(Mode::Listen.next(), Mode::Ask);
        assert_eq!(Mode::default(), Mode::Listen);
    }

    #[test]
    fn preset_sizes_match_reference_dimensions() {
        assert_eq!(OverlaySize::Small.dimensions(), Some((720.0, 620.0)));
        assert_eq!(OverlaySize::Medium.dimensions(), Some((900.0, 680.0)));
        assert_eq!(OverlaySize::Large.dimensions(), Some((1080.0, 760.0)));
        assert_eq!(OverlaySize::ExtraLarge.dimensions(), Some((1280.0, 860.0)));
        assert_eq!(OverlaySize::Full.dimensions(), None);
    }

    #[test]
    fn labels_round_trip() {
        for size in OverlaySize::ALL {
            assert_eq!(OverlaySize::from_label(size.label()), Some(size));
        }
        assert_eq!(OverlaySize::from_label("nope"), None);
    }

    #[test]
    fn empty_message_detection() {
        assert!(Message::user("   ").is_empty());
        assert!(!Message::user("hi").is_empty());
        let mut with_image = Message::user("");
        with_image.images.push("AAA".into());
        assert!(!with_image.is_empty());
    }
}
