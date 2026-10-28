use crate::db::{DbError, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRecord {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

pub fn create(c: &Connection, title: &str, now: i64) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    c.execute(
        "INSERT INTO conversations(id,title,created_at,updated_at) VALUES(?1,?2,?3,?3)",
        params![id, title, now],
    )?;
    Ok(id)
}
pub fn rename(c: &Connection, id: &str, title: &str) -> Result<()> {
    c.execute(
        "UPDATE conversations SET title=?2 WHERE id=?1",
        params![id, title],
    )?;
    Ok(())
}
pub fn delete(c: &Connection, id: &str) -> Result<()> {
    c.execute("DELETE FROM conversations WHERE id=?1", [id])?;
    Ok(())
}
pub fn list(c: &Connection, limit: usize) -> Result<Vec<ConversationRecord>> {
    search(c, "", limit)
}
pub fn search(c: &Connection, query: &str, limit: usize) -> Result<Vec<ConversationRecord>> {
    let mut s = c.prepare(
        "SELECT c.id,c.title,c.created_at,c.updated_at,
            (SELECT count(*) FROM messages m WHERE m.conversation_id=c.id),
            COALESCE((SELECT substr(m.content,1,320) FROM messages m
              WHERE m.conversation_id=c.id AND trim(m.content)<>''
              ORDER BY m.timestamp DESC,m.rowid DESC LIMIT 1),'')
         FROM conversations c
         WHERE ?1='' OR instr(lower(c.title),lower(?1))>0
           OR EXISTS(SELECT 1 FROM messages m WHERE m.conversation_id=c.id
                     AND instr(lower(m.content),lower(?1))>0)
         ORDER BY c.updated_at DESC,c.rowid DESC LIMIT ?2",
    )?;
    let q = query.trim();
    Ok(s.query_map(params![q, limit.min(500) as i64], |r| {
        Ok(ConversationRecord {
            id: r.get(0)?,
            title: r.get(1)?,
            created_at: r.get(2)?,
            updated_at: r.get(3)?,
            message_count: r.get(4)?,
            preview: r.get(5)?,
        })
    })?
    .collect::<std::result::Result<_, _>>()?)
}
pub fn append_message(
    c: &Connection,
    conversation_id: &str,
    role: &str,
    content: &str,
    now: i64,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    c.execute(
        "INSERT INTO messages(id,conversation_id,role,content,timestamp) VALUES(?1,?2,?3,?4,?5)",
        params![id, conversation_id, role, content, now],
    )?;
    Ok(id)
}
pub fn update_message(c: &Connection, id: &str, content: &str) -> Result<()> {
    c.execute(
        "UPDATE messages SET content=?2 WHERE id=?1",
        params![id, content],
    )?;
    Ok(())
}
pub fn messages(c: &Connection, conversation_id: &str) -> Result<Vec<MessageRecord>> {
    let mut s=c.prepare("SELECT id,role,content,timestamp FROM messages WHERE conversation_id=?1 ORDER BY timestamp,rowid")?;
    Ok(s.query_map([conversation_id], |r| {
        Ok(MessageRecord {
            id: r.get(0)?,
            role: r.get(1)?,
            content: r.get(2)?,
            timestamp: r.get(3)?,
        })
    })?
    .collect::<std::result::Result<_, _>>()?)
}
pub fn recent_messages(
    c: &Connection,
    conversation_id: &str,
    limit: usize,
) -> Result<Vec<MessageRecord>> {
    const MARKER: &str = "\n[Middle shortened; full text is saved]\n";
    let mut s=c.prepare(
        "SELECT id,role,
          CASE WHEN length(content)>32000 THEN substr(content,1,16000)||?3||substr(content,-(32000-16000-length(?3)))
               ELSE content END,timestamp
         FROM (SELECT rowid,id,role,content,timestamp FROM messages
               WHERE conversation_id=?1 AND content<>'' ORDER BY timestamp DESC,rowid DESC LIMIT ?2)
         ORDER BY timestamp,rowid")?;
    Ok(
        s.query_map(params![conversation_id, limit as i64, MARKER], |r| {
            Ok(MessageRecord {
                id: r.get(0)?,
                role: r.get(1)?,
                content: r.get(2)?,
                timestamp: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?,
    )
}
pub fn title_from(content: &str) -> String {
    let text = content.trim();
    if text.is_empty() {
        return "New conversation".into();
    }
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(60).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}
pub fn require_conversation(c: &Connection, id: &str) -> Result<()> {
    c.query_row("SELECT 1 FROM conversations WHERE id=?1", [id], |_| Ok(()))
        .optional()?
        .ok_or(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    #[test]
    fn search_and_preview() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        let old = create(c, "Archive", 1).unwrap();
        append_message(c, &old, "user", "Café %_ PLAN", 2).unwrap();
        for n in 0..211 {
            create(c, &format!("Recent {n}"), n + 3).unwrap();
        }
        assert!(!list(c, 200).unwrap().iter().any(|r| r.id == old));
        for q in ["PLAN", "%_", "Café"] {
            assert_eq!(search(c, q, 500).unwrap()[0].id, old)
        }
        assert!(search(c, "' OR 1=1 --", 500).unwrap().is_empty());
        assert_eq!(search(c, "  ", 1).unwrap().len(), 1);
        let other = create(c, "Other", 300).unwrap();
        append_message(c, &other, "user", "other body", 301).unwrap();
        append_message(c, &old, "assistant", "answer", 302).unwrap();
        append_message(c, &old, "system", "  ", 303).unwrap();
        assert_eq!(list(c, 1).unwrap()[0].id, old);
        let row = search(c, "Archive", 10).unwrap().remove(0);
        assert_eq!(
            (row.preview.as_str(), row.message_count, row.updated_at),
            ("answer", 3, 303)
        );
        assert_eq!(search(c, "Other", 10).unwrap()[0].preview, "other body");
        assert!(append_message(c, &old, "tool", "x", 304).is_err());
    }
    #[test]
    fn message_order_and_shortening() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        let id = create(c, "Thread", 0).unwrap();
        for n in 0..100 {
            append_message(c, &id, "user", &format!("note {n}"), n).unwrap();
        }
        let big = format!("START{}END", "é".repeat(600_000));
        let mid = append_message(c, &id, "assistant", &big, 100).unwrap();
        let recent = recent_messages(c, &id, 3).unwrap();
        assert_eq!(recent[0].content, "note 98");
        assert!(recent[2].content.starts_with("START") && recent[2].content.ends_with("END"));
        assert!(recent[2].content.chars().count() <= 32_000);
        assert_eq!(messages(c, &id).unwrap().last().unwrap().content, big);
        update_message(c, &mid, "changed").unwrap();
        assert_eq!(messages(c, &id).unwrap().last().unwrap().content, "changed");
        append_message(c, &id, "user", "early", -1).unwrap();
        assert_eq!(messages(c, &id).unwrap()[0].content, "early");
        delete(c, &id).unwrap();
        assert!(messages(c, &id).unwrap().is_empty());
    }
    #[test]
    fn title_and_presence() {
        assert_eq!(title_from(" \n "), "New conversation");
        assert_eq!(title_from(" Short "), "Short");
        let title = title_from(&"a".repeat(80));
        assert_eq!(title.chars().count(), 61);
        assert!(title.ends_with('…'));
        let db = Database::in_memory().unwrap();
        assert!(require_conversation(db.connection(), "missing").is_err());
    }
}
