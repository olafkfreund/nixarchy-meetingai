//! A chat session: one conversation, its messages and a live answer stream.
//!
//! Owns the conversation row in the database and keeps it in step as messages
//! arrive. A streaming answer is written once when it starts and updated as text
//! lands, so a crash mid-answer never loses what was already shown.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::ai::providers::ProviderConfig;
use crate::core::domain::Message;
use crate::db::{Database, conversations};

/// Why a send could not proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    NoKey,
    Empty,
    Database(String),
    AlreadyStreaming,
    TooLarge,
}

impl SendError {
    /// Text for the user, not the log.
    pub fn message(&self) -> String {
        match self {
            SendError::NoKey => {
                "Add an AI provider key in Dev Space before asking a question.".to_owned()
            }
            SendError::Empty => "Type a question first.".to_owned(),
            SendError::Database(error) => format!("Could not save this conversation: {error}"),
            SendError::AlreadyStreaming => "Wait for the current answer to finish.".to_owned(),
            SendError::TooLarge => "This question is too large (256,000 characters maximum). Select a smaller reference or ask about a specific excerpt.".to_owned(),
        }
    }
}

/// Current unix time in milliseconds, matching the schema's integer columns.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

/// Bound previous exchanges in long meetings while preserving the current
/// question verbatim. The live transcript and user-selected context are supplied
/// separately, so dropping old assistant suggestions never rewrites what was said.
fn recent_history(mut history: Vec<Message>) -> Vec<Message> {
    let Some(latest) = history.pop() else {
        return Vec::new();
    };
    let mut remaining = 32_000usize.saturating_sub(latest.content.chars().count());
    let mut recent = vec![latest];
    for mut message in history.into_iter().rev().take(23) {
        if remaining < 256 {
            break;
        }
        let mut size = message.content.chars().count();
        if size > remaining {
            // A large pasted reference must not disappear entirely on the next
            // follow-up. Keep its beginning and newest material; originals stay on disk.
            let budget = if message.role == crate::core::domain::Role::User {
                remaining
            } else {
                remaining / 2
            };
            let marker = "\n[Earlier content shortened; the complete original is saved in this conversation.]\n";
            let available = budget.saturating_sub(marker.chars().count());
            let head = available / 3;
            let tail = available - head;
            let prefix: String = message.content.chars().take(head).collect();
            let suffix: String = message
                .content
                .chars()
                .rev()
                .take(tail)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            message.content = format!("{prefix}{marker}{suffix}");
            size = message.content.chars().count();
        }
        remaining -= size;
        recent.push(message);
    }
    recent.reverse();
    // Keep complete user/assistant pairs at the leading edge of the window.
    while recent.len() > 1 && recent[0].role != crate::core::domain::Role::User {
        recent.remove(0);
    }
    recent
}

/// The active conversation and its in-flight answer.
#[derive(Default)]
pub struct ChatSession {
    /// Database id once the conversation exists.
    conversation_id: Option<String>,
    /// Message id of the answer currently streaming.
    streaming_message_id: Option<String>,
    /// Accumulated answer text.
    answer: String,
    /// Set while a request is in flight.
    streaming: bool,
}

impl ChatSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn conversation_id(&self) -> Option<&str> {
        self.conversation_id.as_deref()
    }

    pub fn is_streaming(&self) -> bool {
        self.streaming
    }

    pub fn answer(&self) -> &str {
        &self.answer
    }

    /// Start a fresh conversation, forgetting the current one.
    pub fn reset(&mut self) {
        self.conversation_id = None;
        self.streaming_message_id = None;
        self.answer.clear();
        self.streaming = false;
    }

    /// Reuse the original thread; the next turn still uses the bounded history window.
    pub fn restore(&mut self, database: &Database, id: &str) -> Result<Vec<Message>, String> {
        conversations::require_conversation(database.connection(), id)
            .map_err(|e| e.to_string())?;
        let records = conversations::recent_messages(database.connection(), id, 200)
            .map_err(|e| e.to_string())?;
        self.reset();
        self.conversation_id = Some(id.to_owned());
        Ok(records
            .into_iter()
            .filter(|r| !r.content.trim().is_empty())
            .map(record_message)
            .collect())
    }

    /// Begin a turn.
    ///
    /// Creates the conversation on first use, saves the question, and creates the
    /// empty answer row that later updates fill in. Returns the history to send,
    /// which excludes the answer placeholder.
    pub fn begin(
        &mut self,
        database: Option<&Database>,
        question: &str,
    ) -> Result<Vec<Message>, SendError> {
        if self.streaming {
            return Err(SendError::AlreadyStreaming);
        }
        if question.trim().is_empty() {
            return Err(SendError::Empty);
        }
        if question.chars().nth(256_000).is_some() {
            return Err(SendError::TooLarge);
        }
        let database =
            database.ok_or_else(|| SendError::Database("database unavailable".into()))?;
        let connection = database.connection();
        let transaction = connection
            .unchecked_transaction()
            .map_err(|e| SendError::Database(e.to_string()))?;
        let connection = &transaction;
        let now = now_millis();

        let conversation_id = match self.conversation_id.clone() {
            Some(id) => id,
            None => {
                let title = conversations::title_from(question);
                conversations::create(connection, &title, now)
                    .map_err(|error| SendError::Database(error.to_string()))?
            }
        };

        let mut history: Vec<Message> =
            conversations::recent_messages(connection, &conversation_id, 23)
                .map_err(|e| SendError::Database(e.to_string()))?
                .into_iter()
                .map(record_message)
                .collect();
        history.push(Message::user(question.trim()));

        conversations::append_message(connection, &conversation_id, "user", question.trim(), now)
            .map_err(|error| SendError::Database(error.to_string()))?;

        let answer_id =
            conversations::append_message(connection, &conversation_id, "assistant", "", now)
                .map_err(|error| SendError::Database(error.to_string()))?;
        transaction
            .commit()
            .map_err(|e| SendError::Database(e.to_string()))?;
        self.conversation_id = Some(conversation_id);
        self.streaming_message_id = Some(answer_id);
        self.answer.clear();
        self.streaming = true;

        Ok(recent_history(history))
    }

    /// Append a streamed delta and persist it.
    pub fn push_delta(&mut self, database: Option<&Database>, delta: &str) {
        self.append_delta(delta);
        self.checkpoint(database);
    }

    /// Append without disk I/O; native UI frames checkpoint at a bounded cadence.
    pub fn append_delta(&mut self, delta: &str) {
        self.answer.push_str(delta);
    }

    pub fn checkpoint(&self, database: Option<&Database>) {
        self.persist_answer(database);
    }

    /// Mark the stream finished.
    pub fn finish(&mut self) {
        self.streaming = false;
        self.streaming_message_id = None;
    }

    /// Record a failure, keeping whatever text arrived.
    pub fn fail(&mut self, database: Option<&Database>, message: &str) -> String {
        let text = if self.answer.is_empty() {
            format!("Could not get an answer: {message}")
        } else {
            format!("{}\n\n_(stopped: {message})_", self.answer)
        };
        self.answer = text.clone();
        self.persist_answer(database);
        self.finish();
        text
    }

    fn persist_answer(&self, database: Option<&Database>) {
        let (Some(database), Some(id)) = (database, self.streaming_message_id.as_ref()) else {
            return;
        };
        if let Err(error) = conversations::update_message(database.connection(), id, &self.answer) {
            tracing::warn!(%error, "could not persist a streamed answer");
        }
    }

    /// Rename the current conversation.
    pub fn rename(&mut self, database: &Database, title: &str) -> Result<(), String> {
        let id = self
            .conversation_id
            .as_ref()
            .ok_or_else(|| "There is no conversation yet.".to_owned())?;
        conversations::rename(database.connection(), id, title.trim())
            .map_err(|error| error.to_string())
    }

    /// Delete the current conversation and start over.
    pub fn delete(&mut self, database: &Database) -> Result<(), String> {
        if let Some(id) = self.conversation_id.take() {
            conversations::delete(database.connection(), &id).map_err(|error| error.to_string())?;
        }
        self.reset();
        Ok(())
    }
}

fn record_message(record: conversations::MessageRecord) -> Message {
    Message {
        role: match record.role.as_str() {
            "assistant" => crate::core::domain::Role::Assistant,
            "system" => crate::core::domain::Role::System,
            _ => crate::core::domain::Role::User,
        },
        content: record.content,
        images: Vec::new(),
    }
}

/// Provider config, if a usable credential exists.
pub fn chat_config(vault: Option<&crate::core::vault::Vault>) -> Option<ProviderConfig> {
    let vault = vault?;
    let credentials =
        crate::core::credentials::list(vault, crate::core::credentials::Kind::Chat).ok()?;
    credentials
        .into_iter()
        .map(|credential| credential.to_config())
        .find(|config| config.is_usable())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn long_meetings_bound_old_answers_without_truncating_the_latest_question() {
        let mut history = Vec::new();
        for i in 0..100 {
            history.push(Message::user(format!("Question {i}")));
            history.push(Message::assistant("é".repeat(4000)));
        }
        history.push(Message::user("Latest question with context"));
        let recent = recent_history(history);
        assert!(recent.len() <= 24);
        assert!(
            recent
                .iter()
                .map(|m| m.content.chars().count())
                .sum::<usize>()
                <= 32_000
        );
        assert_eq!(
            recent.last().unwrap().content,
            "Latest question with context"
        );
        assert_eq!(
            recent.first().unwrap().role,
            crate::core::domain::Role::User
        );
        assert!(!recent.iter().any(|m| m.content == "Question 0"));
        let huge_question = "x".repeat(40_000);
        let recent = recent_history(vec![
            Message::assistant("old"),
            Message::user(&huge_question),
        ]);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].content, huge_question);
        let recent = recent_history(vec![
            Message::user(format!(
                "Reference start {} Reference end",
                "é".repeat(64_000)
            )),
            Message::assistant("Earlier analysis"),
            Message::user("What about that reference?"),
        ]);
        assert!(recent[0].content.starts_with("Reference start"));
        assert!(recent[0].content.ends_with("Reference end"));
        assert!(recent[0].content.contains("complete original is saved"));
        assert!(
            recent
                .iter()
                .map(|m| m.content.chars().count())
                .sum::<usize>()
                <= 32_000
        );
    }

    #[test]
    fn begin_creates_conversation_and_question() {
        let database = database();
        let mut session = ChatSession::new();
        let history = session
            .begin(Some(&database), "  What is Rust?  ")
            .expect("begin");
        assert!(session.conversation_id().is_some());
        assert!(session.is_streaming());
        assert_eq!(history.last().unwrap().content, "What is Rust?");
        // The empty answer placeholder must not be sent to the model.
        assert!(history.iter().all(|message| !message.content.is_empty()));

        let records =
            conversations::messages(database.connection(), session.conversation_id().unwrap())
                .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].role, "user");
        assert_eq!(records[1].role, "assistant");
    }

    #[test]
    fn title_comes_from_the_first_question() {
        let database = database();
        let mut session = ChatSession::new();
        session
            .begin(Some(&database), "Explain borrow checking")
            .unwrap();
        let list = conversations::list(database.connection(), 10).unwrap();
        assert_eq!(list[0].title, "Explain borrow checking");
    }

    #[test]
    fn deltas_are_persisted() {
        let database = database();
        let mut session = ChatSession::new();
        session.begin(Some(&database), "hi").unwrap();
        session.push_delta(Some(&database), "Hel");
        session.push_delta(Some(&database), "lo");
        let records =
            conversations::messages(database.connection(), session.conversation_id().unwrap())
                .unwrap();
        assert_eq!(records[1].content, "Hello");
        assert_eq!(session.answer(), "Hello");
    }

    #[test]
    fn finish_clears_streaming_state() {
        let database = database();
        let mut session = ChatSession::new();
        session.begin(Some(&database), "hi").unwrap();
        session.finish();
        assert!(!session.is_streaming());
        // A second turn is allowed once finished.
        assert!(session.begin(Some(&database), "again").is_ok());
    }

    #[test]
    fn second_send_while_streaming_is_rejected() {
        let database = database();
        let mut session = ChatSession::new();
        session.begin(Some(&database), "hi").unwrap();
        assert_eq!(
            session.begin(Some(&database), "again").unwrap_err(),
            SendError::AlreadyStreaming
        );
    }

    #[test]
    fn empty_question_is_rejected() {
        let database = database();
        let mut session = ChatSession::new();
        assert_eq!(
            session.begin(Some(&database), "   ").unwrap_err(),
            SendError::Empty
        );
    }

    #[test]
    fn missing_database_is_reported() {
        let mut session = ChatSession::new();
        assert!(matches!(
            session.begin(None, "hi").unwrap_err(),
            SendError::Database(_)
        ));
    }

    #[test]
    fn a_failed_answer_placeholder_rolls_back_the_entire_turn() {
        let db = database();
        db.connection().execute_batch("CREATE TRIGGER fail_answer BEFORE INSERT ON messages WHEN NEW.role='assistant' BEGIN SELECT RAISE(ABORT, 'simulated disk failure'); END;").unwrap();
        let mut session = ChatSession::new();
        assert!(matches!(
            session.begin(Some(&db), "Keep this question"),
            Err(SendError::Database(_))
        ));
        assert!(!session.is_streaming());
        assert!(session.conversation_id().is_none());
        assert!(conversations::list(db.connection(), 10).unwrap().is_empty());
        db.connection()
            .execute_batch("DROP TRIGGER fail_answer")
            .unwrap();
        assert!(session.begin(Some(&db), "Retry").is_ok());
    }

    #[test]
    fn oversized_questions_are_rejected_before_writing_history() {
        let db = database();
        let mut session = ChatSession::new();
        assert_eq!(
            session.begin(Some(&db), &"é".repeat(256_001)).unwrap_err(),
            SendError::TooLarge
        );
        assert!(conversations::list(db.connection(), 10).unwrap().is_empty());
    }

    #[test]
    fn failure_keeps_partial_text() {
        let database = database();
        let mut session = ChatSession::new();
        session.begin(Some(&database), "hi").unwrap();
        session.push_delta(Some(&database), "Partial");
        let text = session.fail(Some(&database), "connection reset");
        assert!(text.contains("Partial"));
        assert!(text.contains("connection reset"));
        assert!(!session.is_streaming());
    }

    #[test]
    fn failure_without_text_explains_it() {
        let database = database();
        let mut session = ChatSession::new();
        session.begin(Some(&database), "hi").unwrap();
        let text = session.fail(Some(&database), "no key");
        assert!(text.starts_with("Could not get an answer"));
    }

    #[test]
    fn rename_and_delete_manage_the_row() {
        let database = database();
        let mut session = ChatSession::new();
        session.begin(Some(&database), "hi").unwrap();
        session.rename(&database, "Renamed").unwrap();
        assert_eq!(
            conversations::list(database.connection(), 10).unwrap()[0].title,
            "Renamed"
        );

        session.delete(&database).unwrap();
        assert!(
            conversations::list(database.connection(), 10)
                .unwrap()
                .is_empty()
        );
        assert!(session.conversation_id().is_none());
    }

    #[test]
    fn history_excludes_previous_answers_but_keeps_questions() {
        let database = database();
        let mut session = ChatSession::new();
        session.begin(Some(&database), "first").unwrap();
        session.push_delta(Some(&database), "answer one");
        session.finish();
        let history = session.begin(Some(&database), "second").unwrap();
        let contents: Vec<&str> = history.iter().map(|m| m.content.as_str()).collect();
        assert!(contents.contains(&"first"));
        assert!(contents.contains(&"answer one"));
        assert_eq!(contents.last().copied(), Some("second"));
    }
}
