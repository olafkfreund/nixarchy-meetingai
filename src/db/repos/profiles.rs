use crate::db::{self, settings};
use rusqlite::{Connection, OptionalExtension, params};

pub const SELECTED_KEY: &str = "selected_context_profile";
const MIGRATED_KEY: &str = "context_profiles_migrated";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub personal_context: String,
    pub meeting_context: String,
}
pub fn list(c: &Connection) -> db::Result<Vec<Profile>> {
    let mut s=c.prepare("SELECT id,name,personal_context,meeting_context FROM context_profiles ORDER BY name COLLATE NOCASE,id")?;
    Ok(s.query_map([], |r| {
        Ok(Profile {
            id: r.get(0)?,
            name: r.get(1)?,
            personal_context: r.get(2)?,
            meeting_context: r.get(3)?,
        })
    })?
    .collect::<std::result::Result<_, _>>()?)
}
pub fn selected(c: &Connection) -> db::Result<Option<Profile>> {
    let Some(id) = settings::get(c, SELECTED_KEY)? else {
        return Ok(None);
    };
    Ok(c.query_row(
        "SELECT id,name,personal_context,meeting_context FROM context_profiles WHERE id=?1",
        [id],
        |r| {
            Ok(Profile {
                id: r.get(0)?,
                name: r.get(1)?,
                personal_context: r.get(2)?,
                meeting_context: r.get(3)?,
            })
        },
    )
    .optional()?)
}
pub fn select(c: &Connection, id: Option<&str>) -> Result<(), String> {
    if let Some(id) = id {
        let exists: bool = c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM context_profiles WHERE id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            return Err("That profile is gone. Choose another profile or No context.".into());
        }
    }
    settings::set(c, SELECTED_KEY, id.unwrap_or("")).map_err(|e| e.to_string())
}
pub fn save(c: &Connection, p: &Profile) -> Result<(), String> {
    let name = p.name.trim();
    let len = name.chars().count();
    if !(1..=80).contains(&len) || name.chars().any(char::is_control) {
        return Err("Profile name must be 1 to 80 characters without control characters.".into());
    }
    if p.personal_context.chars().count() > 16_000 || p.meeting_context.chars().count() > 16_000 {
        return Err("Each context must be at most 16000 characters.".into());
    }
    c.execute("INSERT INTO context_profiles(id,name,personal_context,meeting_context) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET name=excluded.name,personal_context=excluded.personal_context,meeting_context=excluded.meeting_context,updated_at=datetime('now')",params![p.id,name,p.personal_context.trim(),p.meeting_context.trim()]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn delete(c: &Connection, id: &str) -> db::Result<()> {
    let tx = c.unchecked_transaction()?;
    tx.execute("DELETE FROM context_profiles WHERE id=?1", [id])?;
    tx.execute(
        "UPDATE app_settings SET value='',updated_at=datetime('now') WHERE key=?1 AND value=?2",
        params![SELECTED_KEY, id],
    )?;
    tx.commit()?;
    Ok(())
}
pub fn migrate_legacy(c: &Connection) -> db::Result<()> {
    if settings::get(c, MIGRATED_KEY)?.as_deref() == Some("true") {
        return Ok(());
    }
    let personal = settings::get(c, "personal_context")?.unwrap_or_default();
    let meeting = settings::get(c, "meeting_context")?.unwrap_or_default();
    if personal.trim().is_empty() && meeting.trim().is_empty() {
        return Ok(());
    }
    let tx = c.unchecked_transaction()?;
    let personal: String = personal.chars().take(16_000).collect();
    let meeting: String = meeting.chars().take(16_000).collect();
    tx.execute("INSERT OR IGNORE INTO context_profiles(id,name,personal_context,meeting_context) VALUES('legacy-precontext','My profile',?1,?2)",params![personal,meeting])?;
    if settings::get(&tx, SELECTED_KEY)?.is_none() {
        settings::set(&tx, SELECTED_KEY, "legacy-precontext")?
    }
    settings::set(&tx, MIGRATED_KEY, "true")?;
    tx.commit()?;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Document {
    pub id: i64,
    pub name: String,
    pub text: String,
}
pub fn documents(c: &Connection, profile: &str) -> db::Result<Vec<Document>> {
    let mut s =
        c.prepare("SELECT id,name,text FROM profile_documents WHERE profile=?1 ORDER BY id DESC")?;
    Ok(s.query_map([profile], |r| {
        Ok(Document {
            id: r.get(0)?,
            name: r.get(1)?,
            text: r.get(2)?,
        })
    })?
    .collect::<std::result::Result<_, _>>()?)
}
pub fn add_document(c: &Connection, profile: &str, name: &str, text: &str) -> db::Result<i64> {
    c.execute(
        "INSERT INTO profile_documents(profile,name,text) VALUES(?1,?2,?3)",
        params![profile, name, text],
    )?;
    Ok(c.last_insert_rowid())
}
pub fn remove_document(c: &Connection, profile: &str, id: i64) -> db::Result<bool> {
    Ok(c.execute(
        "DELETE FROM profile_documents WHERE profile=?1 AND id=?2",
        params![profile, id],
    )? > 0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    #[test]
    fn selection_and_documents() {
        let database = Database::in_memory().unwrap();
        let connection = database.connection();
        assert_eq!(selected(connection).unwrap(), None);
        let mut p = Profile {
            id: "p".into(),
            name: "  Omega  ".into(),
            personal_context: " x ".into(),
            meeting_context: "".into(),
        };
        save(connection, &p).unwrap();
        select(connection, Some("p")).unwrap();
        assert_eq!(selected(connection).unwrap().unwrap().name, "Omega");
        p.name = "Alpha".into();
        save(connection, &p).unwrap();
        assert_eq!(list(connection).unwrap().len(), 1);
        let doc = add_document(connection, "p", "first", "text").unwrap();
        add_document(connection, "p", "second", "text").unwrap();
        assert_eq!(documents(connection, "p").unwrap()[0].name, "second");
        assert!(!remove_document(connection, "other", doc).unwrap());
        assert!(remove_document(connection, "p", doc).unwrap());
        select(connection, None).unwrap();
        assert_eq!(selected(connection).unwrap(), None);
        assert!(select(connection, Some("gone")).is_err());
        select(connection, Some("p")).unwrap();
        delete(connection, "p").unwrap();
        assert_eq!(selected(connection).unwrap(), None);
        assert!(list(connection).unwrap().is_empty());
        assert!(documents(connection, "p").unwrap().is_empty());
    }
    #[test]
    fn migration_respects_explicit_selection() {
        for explicit in [false, true] {
            let database = Database::in_memory().unwrap();
            let connection = database.connection();
            if explicit {
                select(connection, None).unwrap()
            }
            settings::set(connection, "personal_context", "legacy notes").unwrap();
            migrate_legacy(connection).unwrap();
            migrate_legacy(connection).unwrap();
            assert_eq!(list(connection).unwrap().len(), 1);
            assert_eq!(selected(connection).unwrap().is_some(), !explicit);
            delete(connection, "legacy-precontext").unwrap();
            migrate_legacy(connection).unwrap();
            assert!(list(connection).unwrap().is_empty());
        }
    }
}
