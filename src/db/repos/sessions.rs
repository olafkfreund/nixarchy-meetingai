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
    c: &Connection,
    id: &str,
    meeting: Option<&str>,
    context: &SavedContext,
) -> Result<(), String> {
    let json = serde_json::to_string(context).map_err(|e| e.to_string())?;
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO conversation_state(conversation_id,context_json) VALUES(?1,?2) ON CONFLICT(conversation_id) DO UPDATE SET context_json=excluded.context_json",params![id,json]).map_err(|e|e.to_string())?;
    if let Some(meeting) = meeting {
        tx.execute(
            "INSERT OR IGNORE INTO meeting_conversations(meeting_id,conversation_id) VALUES(?1,?2)",
            params![meeting, id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
pub fn load(c: &Connection, id: &str) -> Result<Option<SavedContext>, String> {
    let json: Option<String> = c
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
pub fn meeting_chat(c: &Connection, meeting: &str) -> crate::db::Result<Option<String>> {
    Ok(c.query_row("SELECT c.id FROM conversations c JOIN meeting_conversations mc ON mc.conversation_id=c.id WHERE mc.meeting_id=?1 ORDER BY c.updated_at DESC,c.rowid DESC LIMIT 1",[meeting],|r|r.get(0)).optional()?)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, conversations, meetings};
    #[test]
    fn linked_context_disappears_with_chat() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        let meeting = meetings::start(c, "Call", "", "en", None, Default::default()).unwrap();
        let chat = conversations::create(c, "Context", 5).unwrap();
        let context = SavedContext {
            prompt_id: Some(2),
            response_language: "fr".into(),
            ..Default::default()
        };
        save(c, &chat, Some(&meeting), &context).unwrap();
        assert_eq!(meeting_chat(c, &meeting).unwrap(), Some(chat.clone()));
        assert_eq!(load(c, &chat).unwrap().unwrap().response_language, "fr");
        conversations::delete(c, &chat).unwrap();
        assert!(load(c, &chat).unwrap().is_none());
        assert_eq!(meeting_chat(c, &meeting).unwrap(), None);
    }

    #[test]
    fn saved_context_is_replaced_and_unknown_chat_is_rejected() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        let chat = conversations::create(c, "Choices", 8).unwrap();
        let mut context = SavedContext {
            response_length: "brief".into(),
            ..Default::default()
        };
        save(c, &chat, None, &context).unwrap();
        context.response_length = "detailed".into();
        save(c, &chat, None, &context).unwrap();
        assert_eq!(load(c, &chat).unwrap().unwrap().response_length, "detailed");
        assert!(save(c, "missing", None, &context).is_err());
    }
}
