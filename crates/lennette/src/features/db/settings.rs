//! Shared application settings.
//!
//! A small key/value store read once at startup and written through, so settings
//! persist without touching the credential vault.

use std::collections::BTreeMap;

use rusqlite::{Connection, params};

use super::Result;

/// Read every setting.
pub fn all(connection: &Connection) -> Result<BTreeMap<String, String>> {
    let mut statement = connection.prepare("SELECT key, value FROM app_settings")?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Read one setting.
pub fn get(connection: &Connection, key: &str) -> Result<Option<String>> {
    let mut statement = connection.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
    let mut rows = statement.query(params![key])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// Write one setting, replacing any previous value.
pub fn set(connection: &Connection, key: &str, value: &str) -> Result<()> {
    connection.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
        params![key, value],
    )?;
    Ok(())
}

/// Remove one setting.
pub fn remove(connection: &Connection, key: &str) -> Result<()> {
    connection.execute("DELETE FROM app_settings WHERE key = ?1", params![key])?;
    Ok(())
}

/// Editable prompt definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptRecord {
    pub id: i64,
    pub name: String,
    pub prompt: String,
    pub listen_slot: Option<i64>,
}

/// All prompts, listen modes first by slot then the rest by name.
pub fn prompts(connection: &Connection) -> Result<Vec<PromptRecord>> {
    let mut statement = connection.prepare(
        "SELECT id, name, prompt, listen_slot FROM system_prompts
         ORDER BY listen_slot IS NULL, listen_slot ASC, name ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(PromptRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            prompt: row.get(2)?,
            listen_slot: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// The five listen-mode prompts, in tab order.
pub fn listen_modes(connection: &Connection) -> Result<Vec<PromptRecord>> {
    Ok(prompts(connection)?
        .into_iter()
        .filter(|prompt| prompt.listen_slot.is_some())
        .collect())
}

/// Create a prompt.
pub fn create_prompt(connection: &Connection, name: &str, prompt: &str) -> Result<i64> {
    connection.execute(
        "INSERT INTO system_prompts (name, prompt) VALUES (?1, ?2)",
        params![name, prompt],
    )?;
    Ok(connection.last_insert_rowid())
}

/// Update a prompt body.
pub fn update_prompt(connection: &Connection, id: i64, name: &str, prompt: &str) -> Result<()> {
    connection.execute(
        "UPDATE system_prompts SET name = ?2, prompt = ?3 WHERE id = ?1",
        params![id, name, prompt],
    )?;
    Ok(())
}

/// Delete a prompt.
pub fn delete_prompt(connection: &Connection, id: i64) -> Result<()> {
    connection.execute("DELETE FROM system_prompts WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::db::Database;

    #[test]
    fn settings_upsert_and_read_back() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        set(connection, "overlay_size", "sm").unwrap();
        set(connection, "overlay_size", "lg").unwrap();
        assert_eq!(
            get(connection, "overlay_size").unwrap().as_deref(),
            Some("lg")
        );
        assert_eq!(all(connection).unwrap().len(), 1);
    }

    #[test]
    fn missing_setting_is_none() {
        let database = Database::open_in_memory().unwrap();
        assert_eq!(get(database.connection(), "nope").unwrap(), None);
    }

    #[test]
    fn remove_clears_setting() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        set(connection, "k", "v").unwrap();
        remove(connection, "k").unwrap();
        assert!(get(connection, "k").unwrap().is_none());
    }

    #[test]
    fn listen_modes_are_seeded_in_order() {
        let database = Database::open_in_memory().unwrap();
        let modes = listen_modes(database.connection()).unwrap();
        let names: Vec<_> = modes.iter().map(|mode| mode.name.as_str()).collect();
        assert_eq!(
            names,
            ["General", "Interview", "Coding", "Translate", "Meeting"]
        );
        let slots: Vec<_> = modes.iter().map(|mode| mode.listen_slot).collect();
        assert_eq!(slots, [Some(0), Some(1), Some(2), Some(3), Some(4)]);
    }

    #[test]
    fn prompts_crud() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        let id = create_prompt(connection, "Custom", "Be nice").unwrap();
        update_prompt(connection, id, "Custom", "Be very nice").unwrap();
        let found = prompts(connection)
            .unwrap()
            .into_iter()
            .find(|prompt| prompt.id == id)
            .unwrap();
        assert_eq!(found.prompt, "Be very nice");
        delete_prompt(connection, id).unwrap();
        assert!(
            prompts(connection)
                .unwrap()
                .iter()
                .all(|prompt| prompt.id != id)
        );
    }
}
