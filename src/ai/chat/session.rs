//! Persistent chat turn lifecycle, context window budgeting, and streaming checkpoints.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::ai::providers::ProviderConfig;
use crate::core::credentials::{self, Kind};
use crate::core::domain::{Message, Role};
use crate::core::vault::Vault;
use crate::db::{Database, conversations};

const MAX_QUESTION_CHARS: usize = 256_000;
const RESTORE_MESSAGE_LIMIT: usize = 200;
const PRIOR_HISTORY_LIMIT: usize = 23;
const WINDOW_CHAR_BUDGET: usize = 32_000;
const MIN_REMAINING_BUDGET: usize = 256;
const SHORTENED_NOTICE: &str =
    "\n[...middle omitted to fit context; the full message is saved in this conversation...]\n";
const NOTICE_CHARS: usize = SHORTENED_NOTICE.len(); // the notice is ASCII

/// Reasons starting a chat turn can be rejected before contacting a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    NoKey,
    Empty,
    Database(String),
    AlreadyStreaming,
    TooLarge,
}

impl SendError {
    /// Returns a single-sentence explanation suitable for displaying to the user.
    pub fn message(&self) -> String {
        match self {
            Self::NoKey => {
                "No AI provider is configured yet; add one in settings before sending a message."
                    .into()
            }
            Self::Empty => "There is nothing to send; type a question first.".into(),
            Self::Database(detail) => {
                format!("The conversation could not be saved to the database: {detail}.")
            }
            Self::AlreadyStreaming => {
                "An answer is still streaming; wait for it to finish before sending another.".into()
            }
            Self::TooLarge => {
                "That question exceeds the 256,000-character limit; please send a smaller excerpt."
                    .into()
            }
        }
    }
}

/// Returns the current Unix timestamp in milliseconds, or `0` if before the epoch.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// Tracks the active database conversation and the assistant reply currently being streamed.
#[derive(Default)]
pub struct ChatSession {
    active_conversation: Option<String>,
    pending_reply_row: Option<String>,
    reply_buffer: String,
    in_progress: bool,
}

impl ChatSession {
    /// Creates an idle session with no bound conversation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the database identifier of the current conversation, if one has started.
    pub fn conversation_id(&self) -> Option<&str> {
        self.active_conversation.as_deref()
    }

    /// Returns whether an assistant answer is currently streaming.
    pub fn is_streaming(&self) -> bool {
        self.in_progress
    }

    /// Returns the text accumulated for the current or most recent turn.
    pub fn answer(&self) -> &str {
        &self.reply_buffer
    }

    /// Clears all in-memory session state without touching the database.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Binds this session to an existing conversation and returns up to 200 recent non-blank messages.
    pub fn restore(&mut self, database: &Database, id: &str) -> Result<Vec<Message>, String> {
        let conn = database.connection();
        conversations::require_conversation(conn, id).map_err(|err| err.to_string())?;
        let records = conversations::recent_messages(conn, id, RESTORE_MESSAGE_LIMIT)
            .map_err(|err| err.to_string())?;
        let loaded = records
            .into_iter()
            .filter(|rec| !rec.content.trim().is_empty())
            .map(record_to_message)
            .collect();
        self.reset();
        self.active_conversation = Some(id.to_owned());
        Ok(loaded)
    }

    /// Validates `question`, records the user and placeholder assistant rows atomically, and returns the bounded history.
    pub fn begin(
        &mut self,
        database: Option<&Database>,
        question: &str,
    ) -> Result<Vec<Message>, SendError> {
        if self.in_progress {
            return Err(SendError::AlreadyStreaming);
        }
        let cleaned = question.trim();
        if cleaned.is_empty() {
            return Err(SendError::Empty);
        }
        if question.chars().count() > MAX_QUESTION_CHARS {
            return Err(SendError::TooLarge);
        }
        let Some(db) = database else {
            return Err(SendError::Database("database handle is unavailable".into()));
        };

        let conn = db.connection();
        let tx = conn
            .unchecked_transaction()
            .map_err(|err| SendError::Database(err.to_string()))?;

        let (conv_id, prior_records) = match &self.active_conversation {
            Some(existing_id) => {
                let prior = conversations::recent_messages(&tx, existing_id, PRIOR_HISTORY_LIMIT)
                    .map_err(|err| SendError::Database(err.to_string()))?;
                (existing_id.clone(), prior)
            }
            None => {
                let title = conversations::title_from(cleaned);
                let created_id = conversations::create(&tx, &title, now_millis())
                    .map_err(|err| SendError::Database(err.to_string()))?;
                (created_id, Vec::new())
            }
        };

        let timestamp = now_millis();
        conversations::append_message(&tx, &conv_id, "user", cleaned, timestamp)
            .map_err(|err| SendError::Database(err.to_string()))?;
        let answer_row = conversations::append_message(&tx, &conv_id, "assistant", "", timestamp)
            .map_err(|err| SendError::Database(err.to_string()))?;
        tx.commit()
            .map_err(|err| SendError::Database(err.to_string()))?;

        self.active_conversation = Some(conv_id);
        self.pending_reply_row = Some(answer_row);
        self.reply_buffer.clear();
        self.in_progress = true;

        let mut turns: Vec<Message> = prior_records
            .into_iter()
            .filter(|rec| !rec.content.trim().is_empty())
            .map(record_to_message)
            .collect();
        turns.push(Message::user(cleaned));
        Ok(window_history(turns))
    }

    /// Appends `delta` to the answer buffer and persists a checkpoint to the database.
    pub fn push_delta(&mut self, database: Option<&Database>, delta: &str) {
        self.append_delta(delta);
        self.checkpoint(database);
    }

    /// Appends `delta` to the in-memory answer buffer without database I/O.
    pub fn append_delta(&mut self, delta: &str) {
        self.reply_buffer.push_str(delta);
    }

    /// Writes the current answer buffer to the active assistant row if both database and row exist.
    pub fn checkpoint(&self, database: Option<&Database>) {
        if let (Some(db), Some(row_id)) = (database, self.pending_reply_row.as_deref())
            && let Err(err) =
                conversations::update_message(db.connection(), row_id, &self.reply_buffer)
        {
            tracing::warn!(%err, "failed to checkpoint streaming assistant reply");
        }
    }

    /// Marks the streaming turn complete while keeping the final answer text accessible.
    pub fn finish(&mut self) {
        self.in_progress = false;
        self.pending_reply_row = None;
    }

    /// Formats a failure note into the answer buffer, persists it, finishes the turn, and returns the resulting text.
    pub fn fail(&mut self, database: Option<&Database>, message: &str) -> String {
        if self.reply_buffer.trim().is_empty() {
            self.reply_buffer =
                format!("No answer could be obtained from the provider ({message}).");
        } else {
            self.reply_buffer = format!(
                "{}\n\n[Answer stopped before completion: {message}]",
                self.reply_buffer
            );
        }
        self.checkpoint(database);
        self.finish();
        self.reply_buffer.clone()
    }
}

/// Returns the first usable chat provider configuration stored in `vault`, if any.
pub fn chat_config(vault: Option<&Vault>) -> Option<ProviderConfig> {
    let vault = vault?;
    let stored = credentials::list(vault, Kind::Chat).ok()?;
    stored
        .into_iter()
        .map(|cred| cred.to_config())
        .find(ProviderConfig::is_usable)
}

fn record_to_message(rec: conversations::MessageRecord) -> Message {
    let role = match rec.role.as_str() {
        "assistant" => Role::Assistant,
        "system" => Role::System,
        _ => Role::User,
    };
    Message {
        role,
        content: rec.content,
        images: Vec::new(),
    }
}

/// Fits the conversation into the character budget; the last element is the new question.
fn window_history(mut history: Vec<Message>) -> Vec<Message> {
    let Some(question) = history.pop() else {
        return Vec::new();
    };
    let mut budget_left = WINDOW_CHAR_BUDGET.saturating_sub(question.content.chars().count());

    // Pass 1, newest first: decide how much of each earlier message survives.
    // `None` keeps the message whole, `Some(n)` cuts it down to `n` characters.
    let mut cuts: Vec<Option<usize>> = Vec::new();
    for message in history.iter().rev().take(PRIOR_HISTORY_LIMIT) {
        if budget_left < MIN_REMAINING_BUDGET {
            break;
        }
        let size = message.content.chars().count();
        if size <= budget_left {
            budget_left -= size;
            cuts.push(None);
        } else {
            let cap = match message.role {
                Role::User => budget_left,
                _ => budget_left / 2,
            };
            let (head, tail) = split_kept(cap);
            budget_left = budget_left.saturating_sub(head + tail + NOTICE_CHARS);
            cuts.push(Some(cap));
        }
    }

    // Pass 2, oldest first: the picked messages are the newest `cuts.len()` ones.
    let first_picked = history.len() - cuts.len();
    let mut windowed: Vec<Message> = history
        .drain(first_picked..)
        .zip(cuts.into_iter().rev())
        .map(|(mut message, cut)| {
            if let Some(cap) = cut {
                message.content = shorten_middle(&message.content, cap);
            }
            message
        })
        .collect();
    windowed.push(question);

    let first_user = windowed
        .iter()
        .position(|m| m.role == Role::User)
        .unwrap_or(windowed.len() - 1);
    windowed.drain(..first_user);
    windowed
}

/// How many characters of the start and of the end survive when `cap` is the whole allowance.
/// The end gets twice as much as the start because recent text matters more.
fn split_kept(cap: usize) -> (usize, usize) {
    let kept = cap.saturating_sub(NOTICE_CHARS);
    let head = (kept / 3).max(1);
    (head, kept.saturating_sub(head).max(1))
}

fn shorten_middle(text: &str, cap: usize) -> String {
    let (head, tail) = split_kept(cap);
    let total = text.chars().count();
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(total.saturating_sub(tail)).collect();
    format!("{start}{SHORTENED_NOTICE}{end}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_window_enforces_count_and_char_budgets_and_trims_middle() {
        let mut many = Vec::new();
        for idx in 0..25 {
            many.push(Message::user(format!("Question number {idx}")));
            many.push(Message::assistant("λ".repeat(2_000)));
        }
        many.push(Message::user("Final verbatim question"));
        let windowed = window_history(many);
        assert!(windowed.len() <= 24);
        let total_chars: usize = windowed.iter().map(|m| m.content.chars().count()).sum();
        assert!(total_chars <= WINDOW_CHAR_BUDGET);
        assert_eq!(windowed.first().unwrap().role, Role::User);
        assert_ne!(windowed.first().unwrap().content, "Question number 0");
        assert_eq!(windowed.last().unwrap().content, "Final verbatim question");

        let over_budget_question = "Ω".repeat(WINDOW_CHAR_BUDGET + 500);
        let single = window_history(vec![
            Message::assistant("Older reply"),
            Message::user(over_budget_question.clone()),
        ]);
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].content, over_budget_question);

        let huge_user = format!("START_MARKER_{}_END_MARKER", "π".repeat(40_000));
        let shortened_window = window_history(vec![
            Message::user(huge_user),
            Message::assistant("Brief reply"),
            Message::user("Follow-up question"),
        ]);
        assert_eq!(shortened_window.len(), 3);
        let first_text = &shortened_window[0].content;
        assert!(first_text.starts_with("START_MARKER_"));
        assert!(first_text.ends_with("_END_MARKER"));
        assert!(first_text.contains(SHORTENED_NOTICE));
        let sum_chars: usize = shortened_window
            .iter()
            .map(|m| m.content.chars().count())
            .sum();
        assert!(sum_chars <= WINDOW_CHAR_BUDGET);
    }

    #[test]
    fn begin_on_fresh_session_creates_conversation_and_placeholder_row() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        let history = session
            .begin(Some(&db), "  How does tokio work?  ")
            .unwrap();
        assert!(session.conversation_id().is_some());
        assert!(session.is_streaming());
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "How does tokio work?");

        let stored =
            conversations::messages(db.connection(), session.conversation_id().unwrap()).unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].role, "user");
        assert_eq!(stored[0].content, "How does tokio work?");
        assert_eq!(stored[1].role, "assistant");
        assert_eq!(stored[1].content, "");
    }

    #[test]
    fn conversation_title_is_derived_from_opening_question() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        session
            .begin(Some(&db), "Explain Raft leader election")
            .unwrap();
        let list = conversations::list(db.connection(), 10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Explain Raft leader election");
    }

    #[test]
    fn push_delta_concatenates_chunks_in_memory_and_database() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        session.begin(Some(&db), "Hello").unwrap();
        session.push_delta(Some(&db), "First part, ");
        session.push_delta(Some(&db), "second part.");
        assert_eq!(session.answer(), "First part, second part.");

        let stored =
            conversations::messages(db.connection(), session.conversation_id().unwrap()).unwrap();
        assert_eq!(stored.last().unwrap().content, "First part, second part.");
    }

    #[test]
    fn finish_clears_streaming_state_and_allows_subsequent_turn() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        session.begin(Some(&db), "Turn one").unwrap();
        session.push_delta(Some(&db), "Reply one");
        session.finish();
        assert!(!session.is_streaming());
        assert!(session.begin(Some(&db), "Turn two").is_ok());
    }

    #[test]
    fn second_begin_while_streaming_returns_already_streaming() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        session.begin(Some(&db), "Turn one").unwrap();
        assert_eq!(
            session.begin(Some(&db), "Concurrent turn"),
            Err(SendError::AlreadyStreaming)
        );
    }

    #[test]
    fn blank_question_is_rejected_as_empty() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        assert_eq!(session.begin(Some(&db), "   \t\n  "), Err(SendError::Empty));
    }

    #[test]
    fn missing_database_returns_database_error() {
        let mut session = ChatSession::new();
        assert!(matches!(
            session.begin(None, "Valid question"),
            Err(SendError::Database(_))
        ));
    }

    #[test]
    fn failed_assistant_insert_rolls_back_conversation_and_leaves_session_idle() {
        let db = Database::in_memory().unwrap();
        db.connection()
            .execute_batch(
                "CREATE TRIGGER fail_assistant_insert
                 BEFORE INSERT ON messages
                 WHEN NEW.role = 'assistant'
                 BEGIN
                     SELECT RAISE(ABORT, 'simulated assistant insert failure');
                 END;",
            )
            .unwrap();
        let mut session = ChatSession::new();
        assert!(matches!(
            session.begin(Some(&db), "Will this roll back?"),
            Err(SendError::Database(_))
        ));
        assert!(!session.is_streaming());
        assert!(session.conversation_id().is_none());
        assert!(conversations::list(db.connection(), 10).unwrap().is_empty());

        db.connection()
            .execute_batch("DROP TRIGGER fail_assistant_insert;")
            .unwrap();
        assert!(session.begin(Some(&db), "Will this roll back?").is_ok());
    }

    #[test]
    fn multibyte_question_enforces_exact_character_limit() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        let over_limit = "é".repeat(MAX_QUESTION_CHARS + 1);
        assert_eq!(
            session.begin(Some(&db), &over_limit),
            Err(SendError::TooLarge)
        );
        assert!(conversations::list(db.connection(), 10).unwrap().is_empty());

        let exact_limit = "é".repeat(MAX_QUESTION_CHARS);
        assert!(session.begin(Some(&db), &exact_limit).is_ok());
    }

    #[test]
    fn fail_after_partial_text_preserves_prefix_and_appends_reason() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        session.begin(Some(&db), "Ask").unwrap();
        session.push_delta(Some(&db), "Half an ans");
        let combined = session.fail(Some(&db), "upstream hung up");
        assert!(!session.is_streaming());
        assert!(combined.contains("Half an ans"));
        assert!(combined.contains("upstream hung up"));
    }

    #[test]
    fn fail_without_prior_text_explains_absence_and_includes_reason() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        session.begin(Some(&db), "Ask").unwrap();
        let note = session.fail(Some(&db), "timeout");
        assert!(!session.is_streaming());
        assert!(note.contains("timeout"));
        assert_ne!(note.trim(), "timeout");
    }

    #[test]
    fn subsequent_begin_includes_completed_turn_in_history() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        session.begin(Some(&db), "First question").unwrap();
        session.push_delta(Some(&db), "First answer");
        session.finish();

        let history = session.begin(Some(&db), "Second question").unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].content, "First question");
        assert_eq!(history[1].content, "First answer");
        assert_eq!(history[2].content, "Second question");
    }

    #[test]
    fn blank_earlier_messages_are_not_sent_back() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        session.begin(Some(&db), "Opening").unwrap();
        session.finish();
        let id = session.conversation_id().unwrap().to_owned();
        conversations::append_message(db.connection(), &id, "assistant", " \n ", 5).unwrap();
        let history = session.begin(Some(&db), "Next").unwrap();
        assert!(history.iter().all(|m| !m.content.trim().is_empty()));
        assert_eq!(history.last().unwrap().content, "Next");
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn shorten_middle_keeps_a_third_head_and_two_thirds_tail() {
        let text = format!("{}{}", "h".repeat(5_000), "t".repeat(5_000));
        let out = shorten_middle(&text, NOTICE_CHARS + 900);
        let (start, end) = out.split_once(SHORTENED_NOTICE).unwrap();
        assert_eq!(start.chars().count(), 300);
        assert_eq!(end.chars().count(), 600);
        assert!(start.chars().all(|c| c == 'h'));
        assert!(end.chars().all(|c| c == 't'));
    }

    #[test]
    fn restore_rejects_unknown_id_and_loads_non_blank_messages_for_existing_conversation() {
        let db = Database::in_memory().unwrap();
        let mut session = ChatSession::new();
        assert!(session.restore(&db, "missing-conv-id").is_err());
        assert!(session.conversation_id().is_none());

        let cid = conversations::create(db.connection(), "Saved", 10).unwrap();
        conversations::append_message(db.connection(), &cid, "system", "System setup", 11).unwrap();
        conversations::append_message(db.connection(), &cid, "user", "User prompt", 12).unwrap();
        conversations::append_message(db.connection(), &cid, "assistant", "   ", 13).unwrap();
        conversations::append_message(db.connection(), &cid, "assistant", "Assistant reply", 14)
            .unwrap();

        let restored = session.restore(&db, &cid).unwrap();
        assert_eq!(session.conversation_id(), Some(cid.as_str()));
        assert_eq!(restored.len(), 3);
        assert_eq!(restored[0].role, Role::System);
        assert_eq!(restored[0].content, "System setup");
        assert_eq!(restored[1].role, Role::User);
        assert_eq!(restored[1].content, "User prompt");
        assert_eq!(restored[2].role, Role::Assistant);
        assert_eq!(restored[2].content, "Assistant reply");
    }
}
