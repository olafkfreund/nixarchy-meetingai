//! Bounded in-memory transcript of the active chat exchange for the UI.

use crate::core::domain::Message;

const MAX_STORED_MESSAGES: usize = 200;
const MAX_STORED_BYTES: usize = 2 * 1024 * 1024;

fn message_footprint(msg: &Message) -> usize {
    msg.content.len() + msg.images.iter().map(String::len).sum::<usize>()
}

/// Holds the visible messages of the current chat session within count and memory bounds.
#[derive(Debug, Default, Clone)]
pub struct Conversation {
    items: Vec<Message>,
}

impl Conversation {
    /// Creates an empty conversation list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the stored messages ordered from oldest to newest.
    pub fn messages(&self) -> &[Message] {
        &self.items
    }

    /// Appends a non-empty message and evicts the oldest entries to stay within bounds.
    pub fn push(&mut self, message: Message) {
        if message.is_empty() {
            return;
        }
        self.items.push(message);

        let mut total_bytes: usize = self.items.iter().map(message_footprint).sum();
        let mut drop_count = 0;
        while self.items.len() - drop_count > 1
            && (self.items.len() - drop_count > MAX_STORED_MESSAGES
                || total_bytes > MAX_STORED_BYTES)
        {
            total_bytes -= message_footprint(&self.items[drop_count]);
            drop_count += 1;
        }
        if drop_count > 0 {
            self.items.drain(..drop_count);
        }
    }

    /// Removes every message from the in-memory list.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_assistant_reply_is_ignored() {
        let mut conv = Conversation::new();
        conv.push(Message::assistant("   \n\t "));
        assert!(conv.messages().is_empty());
    }

    #[test]
    fn clearing_removes_previously_pushed_entries() {
        let mut conv = Conversation::new();
        conv.push(Message::user("first question"));
        conv.push(Message::assistant("first reply"));
        assert_eq!(conv.messages().len(), 2);
        conv.clear();
        assert!(conv.messages().is_empty());
    }

    #[test]
    fn bulk_pushes_enforce_count_and_byte_caps_while_keeping_latest() {
        let mut conv = Conversation::new();
        let chunk = "m".repeat(12 * 1024);
        for idx in 0..260 {
            conv.push(Message::user(format!("{idx}:{chunk}")));
        }
        let kept = conv.messages();
        assert!(kept.len() <= MAX_STORED_MESSAGES);
        let bytes: usize = kept.iter().map(message_footprint).sum();
        assert!(bytes <= MAX_STORED_BYTES);
        assert!(kept.last().unwrap().content.starts_with("259:"));
    }

    #[test]
    fn single_oversized_entry_evicts_all_older_entries_and_stays() {
        let mut conv = Conversation::new();
        conv.push(Message::user("small intro"));
        conv.push(Message::assistant("small reply"));
        let huge = "z".repeat(MAX_STORED_BYTES + 512);
        conv.push(Message::user(huge.clone()));
        assert_eq!(conv.messages().len(), 1);
        assert_eq!(conv.messages()[0].content, huge);
    }
}
