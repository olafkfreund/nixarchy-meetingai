//! Conversation state.
//!
//! Holds the current thread and the composer draft. Pure state transitions so
//! they can be unit-tested without a window.

use crate::core::domain::Message;

/// The active conversation.
#[derive(Debug, Default, Clone)]
pub struct Conversation {
    messages: Vec<Message>,
    draft: String,
}

impl Conversation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn draft(&self) -> &str {
        &self.draft
    }

    pub fn push(&mut self, message: Message) {
        if !message.is_empty() {
            self.messages.push(message);
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.draft.clear();
    }

    /// Append every non-empty draft chunk.
    pub fn set_draft(&mut self, value: impl Into<String>) {
        self.draft = value.into();
    }

    pub fn extend_draft(&mut self, chunk: &str) {
        self.draft.push_str(chunk);
    }

    /// Whether the composer currently holds something worth sending.
    pub fn can_submit(&self) -> bool {
        !self.draft.trim().is_empty()
    }

    /// Take the draft as a user message. Returns `None` when the draft is blank,
    /// so callers can ignore accidental empty submits.
    pub fn submit(&mut self) -> Option<&Message> {
        if !self.can_submit() {
            return None;
        }
        let content = std::mem::take(&mut self.draft);
        self.messages.push(Message::user(content.trim().to_owned()));
        self.messages.last()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_submit_is_ignored() {
        let mut c = Conversation::new();
        c.set_draft("   ");
        assert!(c.submit().is_none());
        assert!(c.is_empty());
    }

    #[test]
    fn submit_trims_and_takes_draft() {
        let mut c = Conversation::new();
        c.set_draft("  hello  ");
        let msg = c.submit().expect("message");
        assert_eq!(msg.content, "hello");
        assert_eq!(c.draft(), "");
        assert_eq!(c.messages().len(), 1);
    }

    #[test]
    fn streaming_appends_to_draft() {
        let mut c = Conversation::new();
        c.extend_draft("a");
        c.extend_draft("bc");
        assert_eq!(c.draft(), "abc");
    }

    #[test]
    fn clear_resets_everything() {
        let mut c = Conversation::new();
        c.set_draft("x");
        c.submit();
        c.clear();
        assert!(c.is_empty());
        assert_eq!(c.draft(), "");
    }

    #[test]
    fn empty_messages_are_not_stored() {
        let mut c = Conversation::new();
        c.push(Message::assistant("  "));
        assert!(c.is_empty());
    }
}
