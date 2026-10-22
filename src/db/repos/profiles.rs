//! Reusable, explicitly selected background for conversations.
use super::settings;
use rusqlite::{Connection, OptionalExtension, params};

pub const SELECTED_KEY: &str = "selected_context_profile";
const MIGRATED_KEY: &str = "context_profiles_migrated";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub personal_context: String,
    pub meeting_context: String,
}

pub fn list(connection: &Connection) -> crate::db::Result<Vec<Profile>> {
    let mut query = connection.prepare("SELECT id, name, personal_context, meeting_context FROM context_profiles ORDER BY name COLLATE NOCASE, id")?;
    let rows = query.query_map([], |row| {
        Ok(Profile {
            id: row.get(0)?,
            name: row.get(1)?,
            personal_context: row.get(2)?,
            meeting_context: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn selected(connection: &Connection) -> crate::db::Result<Option<Profile>> {
    let id = settings::get(connection, SELECTED_KEY)?.unwrap_or_default();
    let mut query = connection.prepare(
        "SELECT id, name, personal_context, meeting_context FROM context_profiles WHERE id = ?1",
    )?;
    Ok(query
        .query_row([id], |row| {
            Ok(Profile {
                id: row.get(0)?,
                name: row.get(1)?,
                personal_context: row.get(2)?,
                meeting_context: row.get(3)?,
            })
        })
        .optional()?)
}

pub fn select(connection: &Connection, id: Option<&str>) -> Result<(), String> {
    if let Some(id) = id {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM context_profiles WHERE id = ?1)",
                [id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            return Err(
                "This profile no longer exists. Choose another profile or No context.".into(),
            );
        }
    }
    settings::set(connection, SELECTED_KEY, id.unwrap_or_default()).map_err(|e| e.to_string())
}

pub fn save(connection: &Connection, profile: &Profile) -> Result<(), String> {
    let name = profile.name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err("Give the profile a name of 1 to 80 characters.".into());
    }
    if profile.personal_context.chars().count() > 16_000
        || profile.meeting_context.chars().count() > 16_000
    {
        return Err("Keep each context field under 16,000 characters.".into());
    }
    connection.execute("INSERT INTO context_profiles (id, name, personal_context, meeting_context) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(id) DO UPDATE SET name = excluded.name, personal_context = excluded.personal_context, meeting_context = excluded.meeting_context, updated_at = datetime('now')",
        params![profile.id, name, profile.personal_context.trim(), profile.meeting_context.trim()]).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete(connection: &Connection, id: &str) -> crate::db::Result<()> {
    let tx = connection.unchecked_transaction()?;
    tx.execute("DELETE FROM context_profiles WHERE id = ?1", [id])?;
    if settings::get(&tx, SELECTED_KEY)?.as_deref() == Some(id) {
        settings::set(&tx, SELECTED_KEY, "")?;
    }
    tx.commit()?;
    Ok(())
}

/// Upgrade the previous single background into a profile once, preserving an
/// explicitly chosen No context and never resurrecting a deleted profile.
pub fn migrate_legacy(connection: &Connection) -> crate::db::Result<()> {
    if settings::get(connection, MIGRATED_KEY)?.is_some() {
        return Ok(());
    }
    let personal = settings::get(connection, "personal_context")?.unwrap_or_default();
    let meeting = settings::get(connection, "meeting_context")?.unwrap_or_default();
    if personal.trim().is_empty() && meeting.trim().is_empty() {
        return Ok(());
    }
    let tx = connection.unchecked_transaction()?;
    tx.execute("INSERT OR IGNORE INTO context_profiles (id, name, personal_context, meeting_context) VALUES ('legacy-precontext', 'My profile', ?1, ?2)", params![personal.chars().take(16_000).collect::<String>(), meeting.chars().take(16_000).collect::<String>()])?;
    if settings::get(&tx, SELECTED_KEY)?.is_none() {
        settings::set(&tx, SELECTED_KEY, "legacy-precontext")?;
    }
    settings::set(&tx, MIGRATED_KEY, "true")?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    #[test]
    fn profiles_round_trip_edit_select_and_delete_without_dangling_selection() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        assert!(selected(c).unwrap().is_none());
        let mut profile = Profile {
            id: "one".into(),
            name: "Interview".into(),
            personal_context: "I build Rust apps".into(),
            meeting_context: "Platform role".into(),
        };
        save(c, &profile).unwrap();
        select(c, Some("one")).unwrap();
        assert_eq!(selected(c).unwrap(), Some(profile.clone()));
        profile.name = "Architecture".into();
        save(c, &profile).unwrap();
        assert_eq!(list(c).unwrap(), vec![profile]);
        select(c, None).unwrap();
        assert!(selected(c).unwrap().is_none());
        assert!(select(c, Some("missing")).is_err());
        select(c, Some("one")).unwrap();
        delete(c, "one").unwrap();
        assert!(selected(c).unwrap().is_none());
        assert!(list(c).unwrap().is_empty());
    }
    #[test]
    fn legacy_context_is_migrated_once_and_respects_explicit_no_context() {
        for no_context in [false, true] {
            let db = Database::in_memory().unwrap();
            let c = db.connection();
            settings::set(c, "personal_context", "Existing background").unwrap();
            if no_context {
                select(c, None).unwrap();
            }
            migrate_legacy(c).unwrap();
            migrate_legacy(c).unwrap();
            assert_eq!(list(c).unwrap().len(), 1);
            assert_eq!(selected(c).unwrap().is_none(), no_context);
            delete(c, "legacy-precontext").unwrap();
            migrate_legacy(c).unwrap();
            assert!(list(c).unwrap().is_empty());
        }
    }
}
