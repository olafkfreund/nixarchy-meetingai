//! Conversations and their messages.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::{DbError, Result};

/// A conversation summary for lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationRecord {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: i64,
    #[serde(default)]
    pub preview: String,
}

/// A persisted message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRecord {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

/// Create a conversation and return its id.
pub fn create(connection: &Connection, title: &str, now: i64) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, title, now, now],
    )?;
    Ok(id)
}

/// Rename a conversation.
pub fn rename(connection: &Connection, id: &str, title: &str) -> Result<()> {
    connection.execute(
        "UPDATE conversations SET title = ?2 WHERE id = ?1",
        params![id, title],
    )?;
    Ok(())
}

/// Delete a conversation. Messages go with it through the cascade.
pub fn delete(connection: &Connection, id: &str) -> Result<()> {
    connection.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
    Ok(())
}

/// Most recently updated conversations first.
pub fn list(connection: &Connection, limit: usize) -> Result<Vec<ConversationRecord>> {
    search(connection, "", limit)
}

/// Search titles and every saved message without loading full bodies into memory.
/// `instr` treats %, _ and quotes as literal search text.
pub fn search(
    connection: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<ConversationRecord>> {
    let mut statement = connection.prepare(
        "SELECT c.id, c.title, c.created_at, c.updated_at, COUNT(m.id),
                COALESCE((SELECT substr(p.content, 1, 320) FROM messages p
                          WHERE p.conversation_id = c.id AND trim(p.content) != ''
                          ORDER BY p.timestamp DESC, p.rowid DESC LIMIT 1), '')
         FROM conversations c
         LEFT JOIN messages m ON m.conversation_id = c.id
         WHERE ?2 = '' OR instr(lower(c.title), lower(?2)) > 0
            OR EXISTS (SELECT 1 FROM messages s WHERE s.conversation_id=c.id
                       AND instr(lower(s.content), lower(?2)) > 0)
         GROUP BY c.id
         ORDER BY c.updated_at DESC, c.rowid DESC
         LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit.min(500) as i64, query.trim()], |row| {
        Ok(ConversationRecord {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
            message_count: row.get(4)?,
            preview: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Append a message, returning its id.
///
/// The conversation's `updated_at` is advanced by the schema trigger, so every
/// write path keeps ordering consistent without duplicating that logic.
pub fn append_message(
    connection: &Connection,
    conversation_id: &str,
    role: &str,
    content: &str,
    now: i64,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO messages (id, conversation_id, role, content, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, conversation_id, role, content, now],
    )?;
    Ok(id)
}

/// Replace a message body, used while an answer streams in.
pub fn update_message(connection: &Connection, id: &str, content: &str) -> Result<()> {
    connection.execute(
        "UPDATE messages SET content = ?2 WHERE id = ?1",
        params![id, content],
    )?;
    Ok(())
}

/// Messages in a conversation, oldest first.
pub fn messages(connection: &Connection, conversation_id: &str) -> Result<Vec<MessageRecord>> {
    let mut statement = connection.prepare(
        "SELECT id, role, content, timestamp FROM messages
         WHERE conversation_id = ?1
         ORDER BY timestamp ASC, rowid ASC",
    )?;
    let rows = statement.query_map(params![conversation_id], |row| {
        Ok(MessageRecord {
            id: row.get(0)?,
            role: row.get(1)?,
            content: row.get(2)?,
            timestamp: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Read only the recent window, bounding old message bodies in SQLite before
/// allocating Rust strings. The complete original remains available in history.
pub fn recent_messages(
    connection: &Connection,
    conversation_id: &str,
    limit: usize,
) -> Result<Vec<MessageRecord>> {
    let mut statement = connection.prepare(
        "SELECT id, role,
                CASE WHEN length(content) > 32000 THEN
                    substr(content, 1, 10600) || char(10) ||
                    '[Earlier content shortened; the complete original is saved in this conversation.]' || char(10) ||
                    substr(content, -21300)
                ELSE content END, timestamp
         FROM messages WHERE conversation_id = ?1 AND content != ''
         ORDER BY timestamp DESC, rowid DESC LIMIT ?2",
    )?;
    let mut rows = statement
        .query_map(params![conversation_id, limit as i64], |row| {
            Ok(MessageRecord {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                timestamp: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.reverse();
    Ok(rows)
}

/// Derive a conversation title from its first user message.
pub fn title_from(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return "New conversation".to_owned();
    }
    let mut title: String = trimmed.chars().take(60).collect();
    if trimmed.chars().count() > 60 {
        title.push('…');
    }
    title
}

pub fn require_conversation(connection: &Connection, id: &str) -> Result<()> {
    let exists: i64 = connection.query_row(
        "SELECT COUNT(*) FROM conversations WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Err(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
    }
    Ok(())
}

#[cfg(test)]
mod archive_search_tests {
    use super::*;
    use crate::db::Database;
    #[test]
    fn search_finds_old_message_content_beyond_the_initial_card_limit() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        let old = create(c, "Unrelated title", 1).unwrap();
        append_message(c, &old, "user", "Discuss the 50%_migration. Café 東京.", 1).unwrap();
        for i in 2..205 {
            create(c, &format!("Recent {i}"), i).unwrap();
        }
        assert!(!list(c, 200).unwrap().iter().any(|r| r.id == old));
        for query in ["MIGRATION", "50%_", "東京"] {
            let found = search(c, query, 200).unwrap();
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].id, old);
        }
        assert!(search(c, "' OR 1=1 --", 200).unwrap().is_empty());
        assert_eq!(search(c, "  ", 1).unwrap().len(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn previews_use_the_latest_nonempty_message_in_the_right_conversation() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        let first = create(connection, "First", 1).unwrap();
        let second = create(connection, "Second", 2).unwrap();
        append_message(connection, &first, "user", "Question", 3).unwrap();
        append_message(connection, &first, "assistant", "Newest answer", 4).unwrap();
        append_message(connection, &first, "assistant", "  ", 5).unwrap();
        append_message(connection, &second, "user", "Different context", 6).unwrap();
        let records = list(connection, 10).unwrap();
        let first = records.iter().find(|r| r.id == first).unwrap();
        assert_eq!(first.preview, "Newest answer");
        assert_eq!(first.message_count, 3);
        assert_eq!(
            records.iter().find(|r| r.id == second).unwrap().preview,
            "Different context"
        );
    }

    #[test]
    fn message_updates_conversation_timestamp() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        let id = create(connection, "Test", 1).unwrap();
        append_message(connection, &id, "user", "hello", 42).unwrap();
        let updated: i64 = connection
            .query_row(
                "SELECT updated_at FROM conversations WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(updated, 42);
    }

    #[test]
    fn deleting_conversation_cascades_messages() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        let id = create(connection, "Test", 1).unwrap();
        append_message(connection, &id, "user", "hello", 2).unwrap();
        delete(connection, &id).unwrap();
        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn listing_orders_by_recent_activity() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        let first = create(connection, "First", 1).unwrap();
        let second = create(connection, "Second", 2).unwrap();
        append_message(connection, &first, "user", "later", 99).unwrap();
        let list = list(connection, 10).unwrap();
        assert_eq!(list[0].id, first);
        assert_eq!(list[1].id, second);
        assert_eq!(list[0].message_count, 1);
    }

    #[test]
    fn messages_are_chronological() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        let id = create(connection, "Test", 1).unwrap();
        append_message(connection, &id, "user", "one", 3).unwrap();
        append_message(connection, &id, "assistant", "two", 1).unwrap();
        let messages = messages(connection, &id).unwrap();
        assert_eq!(messages[0].content, "two");
        assert_eq!(messages[1].content, "one");
    }

    #[test]
    fn recent_reads_bound_rows_and_large_unicode_bodies_before_allocation() {
        let db = Database::in_memory().unwrap();
        let id = create(db.connection(), "Long history", 1).unwrap();
        for i in 0..100 {
            append_message(db.connection(), &id, "user", &format!("message {i}"), 1).unwrap();
        }
        let original = format!("Beginning {} End", "é".repeat(1_000_000));
        append_message(db.connection(), &id, "user", &original, 1).unwrap();
        let recent = recent_messages(db.connection(), &id, 3).unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].content, "message 98");
        assert!(recent[2].content.starts_with("Beginning"));
        assert!(recent[2].content.ends_with("End"));
        assert!(recent[2].content.chars().count() <= 32_000);
        assert_eq!(
            messages(db.connection(), &id)
                .unwrap()
                .last()
                .unwrap()
                .content,
            original
        );
    }

    #[test]
    fn update_rewrites_content() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        let id = create(connection, "Test", 1).unwrap();
        let message = append_message(connection, &id, "assistant", "partial", 2).unwrap();
        update_message(connection, &message, "complete").unwrap();
        let messages = messages(connection, &id).unwrap();
        assert_eq!(messages[0].content, "complete");
    }

    #[test]
    fn titles_are_shortened() {
        assert_eq!(title_from("  "), "New conversation");
        assert_eq!(title_from("hello world"), "hello world");
        let long = "a".repeat(80);
        let title = title_from(&long);
        assert_eq!(title.chars().count(), 61);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn role_check_is_enforced() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        let id = create(connection, "Test", 1).unwrap();
        assert!(append_message(connection, &id, "robot", "x", 1).is_err());
    }
}
