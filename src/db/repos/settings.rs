use crate::db::Result;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeMap;

pub fn all(c: &Connection) -> Result<BTreeMap<String, String>> {
    let mut s = c.prepare("SELECT key, value FROM app_settings")?;
    Ok(s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?)
}
pub fn get(c: &Connection, key: &str) -> Result<Option<String>> {
    Ok(
        c.query_row("SELECT value FROM app_settings WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?,
    )
}
pub fn set(c: &Connection, key: &str, value: &str) -> Result<()> {
    c.execute("INSERT INTO app_settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=datetime('now')", params![key, value])?;
    Ok(())
}
pub fn remove(c: &Connection, key: &str) -> Result<()> {
    c.execute("DELETE FROM app_settings WHERE key=?1", [key])?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptRecord {
    pub id: i64,
    pub name: String,
    pub prompt: String,
    pub listen_slot: Option<i64>,
}
pub fn prompts(c: &Connection) -> Result<Vec<PromptRecord>> {
    let mut s = c.prepare("SELECT id,name,prompt,listen_slot FROM system_prompts ORDER BY listen_slot IS NULL, listen_slot, name")?;
    Ok(s.query_map([], |r| {
        Ok(PromptRecord {
            id: r.get(0)?,
            name: r.get(1)?,
            prompt: r.get(2)?,
            listen_slot: r.get(3)?,
        })
    })?
    .collect::<std::result::Result<_, _>>()?)
}
pub fn listen_modes(c: &Connection) -> Result<Vec<PromptRecord>> {
    prompts(c)
}
pub fn create_prompt(c: &Connection, name: &str, prompt: &str) -> Result<i64> {
    c.execute(
        "INSERT INTO system_prompts(name,prompt) VALUES(?1,?2)",
        params![name, prompt],
    )?;
    Ok(c.last_insert_rowid())
}
pub fn update_prompt(c: &Connection, id: i64, name: &str, prompt: &str) -> Result<()> {
    c.execute(
        "UPDATE system_prompts SET name=?2,prompt=?3 WHERE id=?1",
        params![id, name, prompt],
    )?;
    Ok(())
}
pub fn delete_prompt(c: &Connection, id: i64) -> Result<()> {
    let tx = c.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO app_settings(key,value) SELECT 'deleted_listen_slot_' || listen_slot,'true' FROM system_prompts WHERE id=?1 AND listen_slot IS NOT NULL ON CONFLICT(key) DO UPDATE SET value='true',updated_at=datetime('now')",
        [id],
    )?;
    tx.execute("DELETE FROM system_prompts WHERE id=?1", [id])?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    #[test]
    fn settings_and_prompts() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        assert_eq!(get(c, "ui_scale").unwrap(), None);
        set(c, "ui_scale", "small").unwrap();
        set(c, "ui_scale", "large").unwrap();
        assert_eq!(get(c, "ui_scale").unwrap().as_deref(), Some("large"));
        assert_eq!(all(c).unwrap().len(), 1);
        remove(c, "ui_scale").unwrap();
        assert_eq!(get(c, "ui_scale").unwrap(), None);
        let modes = listen_modes(c).unwrap();
        assert_eq!(
            modes.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            [
                "General",
                "Interview",
                "Coding",
                "Translate",
                "Meeting",
                "Interviewer"
            ]
        );
        assert_eq!(
            modes.iter().map(|p| p.listen_slot).collect::<Vec<_>>(),
            [Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)]
        );
        delete_prompt(c, modes[1].id).unwrap();
        crate::db::seed::seed_listen_modes(c).unwrap();
        assert!(
            !listen_modes(c)
                .unwrap()
                .iter()
                .any(|p| p.listen_slot == Some(1))
        );
        let id = create_prompt(c, "Quick", "first").unwrap();
        update_prompt(c, id, "Brief", "second").unwrap();
        assert!(
            listen_modes(c)
                .unwrap()
                .iter()
                .any(|p| p.id == id && p.name == "Brief" && p.prompt == "second")
        );
        delete_prompt(c, id).unwrap();
        assert!(!prompts(c).unwrap().iter().any(|p| p.id == id));
    }

    #[test]
    fn custom_prompts_follow_slotted_prompts_in_name_order() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let z = create_prompt(c, "Zulu custom", "last").unwrap();
        let a = create_prompt(c, "Alpha custom", "first").unwrap();
        let prompts = prompts(c).unwrap();
        assert_eq!(prompts, listen_modes(c).unwrap());
        assert_eq!(prompts[prompts.len() - 2].id, a);
        assert_eq!(prompts[prompts.len() - 1].id, z);
        assert!(
            prompts[..prompts.len() - 2]
                .iter()
                .all(|prompt| prompt.listen_slot.is_some())
        );
        delete_prompt(c, -1).unwrap();
        assert_eq!(get(c, "deleted_listen_slot_-1").unwrap(), None);
    }
}
