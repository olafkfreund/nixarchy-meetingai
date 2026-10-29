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
    let mut s=c.prepare("SELECT id,name,personal_context,meeting_context FROM context_profiles ORDER BY lower(name),id")?;
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
    let chosen = match id {
        None => "",
        Some(candidate) => {
            let found = c
                .query_row(
                    "SELECT id FROM context_profiles WHERE id=?1",
                    [candidate],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            found.ok_or("That profile is gone. Choose another profile or No context.")?;
            candidate
        }
    };
    settings::set(c, SELECTED_KEY, chosen).map_err(|error| error.to_string())
}
fn valid_name(name: &str) -> bool {
    match name.chars().count() {
        0 => false,
        1..=80 => !name.chars().any(char::is_control),
        _ => false,
    }
}
pub fn save(c: &Connection, p: &Profile) -> Result<(), String> {
    let cleaned = p.name.trim();
    if !valid_name(cleaned) {
        return Err("Profile name must be 1 to 80 characters without control characters.".into());
    }
    for context in [&p.personal_context, &p.meeting_context] {
        if context.chars().count() > 16_000 {
            return Err("Each context must be at most 16000 characters.".into());
        }
    }
    let changed = c.execute(
        "UPDATE context_profiles SET name=?2,personal_context=?3,meeting_context=?4,updated_at=datetime('now') WHERE id=?1",
        params![p.id, cleaned, p.personal_context.trim(), p.meeting_context.trim()],
    ).map_err(|error| error.to_string())?;
    if changed == 0 {
        c.execute(
            "INSERT INTO context_profiles(id,name,personal_context,meeting_context) VALUES(?1,?2,?3,?4)",
            params![p.id, cleaned, p.personal_context.trim(), p.meeting_context.trim()],
        ).map_err(|error| error.to_string())?;
    }
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
fn legacy_contexts(c: &Connection) -> db::Result<Option<(String, String)>> {
    if settings::get(c, MIGRATED_KEY)?.is_some() {
        return Ok(None);
    }
    let personal = settings::get(c, "personal_context")?.unwrap_or_default();
    let meeting = settings::get(c, "meeting_context")?.unwrap_or_default();
    if personal.trim().is_empty() && meeting.trim().is_empty() {
        return Ok(None);
    }
    let cut = |text: String| text.chars().take(16_000).collect::<String>();
    Ok(Some((cut(personal), cut(meeting))))
}
pub fn migrate_legacy(c: &Connection) -> db::Result<()> {
    let Some((personal, meeting)) = legacy_contexts(c)? else {
        return Ok(());
    };
    let tx = c.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO app_settings(key,value) VALUES(?1,'true')",
        [MIGRATED_KEY],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO context_profiles(id,name,personal_context,meeting_context) VALUES('legacy-precontext','My profile',?1,?2)",
        params![personal, meeting],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO app_settings(key,value) VALUES(?1,'legacy-precontext')",
        [SELECTED_KEY],
    )?;
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
        assert_eq!(
            selected(connection).unwrap().unwrap(),
            Profile {
                name: "Omega".into(),
                personal_context: "x".into(),
                ..p.clone()
            }
        );
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

    #[test]
    fn renaming_a_selected_profile_updates_the_only_list_entry() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let mut profile = Profile {
            id: "work".into(),
            name: "  First name  ".into(),
            personal_context: "  personal  ".into(),
            meeting_context: "  meeting  ".into(),
        };
        save(c, &profile).unwrap();
        select(c, Some(&profile.id)).unwrap();
        assert_eq!(selected(c).unwrap().unwrap().personal_context, "personal");
        profile.name = "  Revised name  ".into();
        save(c, &profile).unwrap();
        let profiles = list(c).unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(
            profiles[0],
            Profile {
                name: "Revised name".into(),
                personal_context: "personal".into(),
                meeting_context: "meeting".into(),
                ..profile.clone()
            }
        );
    }

    #[test]
    fn any_migration_marker_prevents_recreating_legacy_profile() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        settings::set(c, "personal_context", "old notes").unwrap();
        settings::set(c, MIGRATED_KEY, "done elsewhere").unwrap();
        migrate_legacy(c).unwrap();
        assert!(list(c).unwrap().is_empty());
    }
}
