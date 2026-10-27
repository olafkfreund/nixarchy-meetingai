//! Restore a conversation's explicit context and the AI thread belonging to a meeting.
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct SavedContext {
    pub profile_id: Option<String>,
    pub prompt_id: Option<i64>,
    pub sources: Vec<crate::core::context::Attachment>,
    pub response_length: String,
    pub response_language: String,
}

pub fn save(
    connection: &Connection,
    id: &str,
    meeting: Option<&str>,
    context: &SavedContext,
) -> Result<(), String> {
    let json = serde_json::to_string(context).map_err(|e| e.to_string())?;
    connection.execute("INSERT INTO conversation_state VALUES (?1, ?2) ON CONFLICT(conversation_id) DO UPDATE SET context_json=excluded.context_json", params![id, json]).map_err(|e| e.to_string())?;
    if let Some(meeting) = meeting {
        connection
            .execute(
                "INSERT OR IGNORE INTO meeting_conversations VALUES (?1, ?2)",
                params![meeting, id],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn load(connection: &Connection, id: &str) -> Result<Option<SavedContext>, String> {
    let json: Option<String> = connection
        .query_row(
            "SELECT context_json FROM conversation_state WHERE conversation_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    json.map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        .transpose()
}
pub fn meeting_chat(connection: &Connection, meeting: &str) -> crate::db::Result<Option<String>> {
    Ok(connection.query_row("SELECT c.id FROM conversations c JOIN meeting_conversations mc ON mc.conversation_id=c.id WHERE mc.meeting_id=?1 ORDER BY c.updated_at DESC LIMIT 1", [meeting], |r| r.get(0)).optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_context_stays_with_its_thread_and_cascades_on_delete() {
        let db = crate::db::Database::in_memory().unwrap();
        let c = db.connection();
        let meeting = crate::db::meetings::start(c, "Call", "", "en", None).unwrap();
        let chat = crate::db::conversations::create(c, "Chat", 1).unwrap();
        save(
            c,
            &chat,
            Some(&meeting),
            &SavedContext {
                prompt_id: Some(1),
                response_language: "Hindi".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(meeting_chat(c, &meeting).unwrap(), Some(chat.clone()));
        assert_eq!(load(c, &chat).unwrap().unwrap().response_language, "Hindi");
        crate::db::conversations::delete(c, &chat).unwrap();
        assert!(load(c, &chat).unwrap().is_none());
        assert!(meeting_chat(c, &meeting).unwrap().is_none());
    }
}
