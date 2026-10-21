//! Local database.
//!
//! SQLite with the recovered schema: conversations, messages, live-session
//! transcripts, the document library and shared settings. Repositories below keep
//! SQL in one place so screens never build queries inline.

pub mod conversations;
pub mod library;
pub mod meetings;
pub mod seed;
pub mod settings;

use std::path::Path;

use rusqlite::Connection;

/// Errors from the local database.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DbError>;

/// The embedded schema, applied on open.
const SCHEMA: &str = include_str!("schema.sql");

/// An open database handle.
pub struct Database {
    connection: Connection,
}

impl Database {
    /// Open or create a database at `path`, applying the schema idempotently.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;
        Self::prepare(connection)
    }

    /// Open an in-memory database, used by tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(connection: Connection) -> Result<Self> {
        // WAL keeps readers from blocking the writer while a transcript is
        // streaming in, and foreign keys make the ON DELETE cascades real.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.execute_batch(SCHEMA)?;
        seed::seed_listen_modes(&connection)?;
        Ok(Self { connection })
    }

    /// Borrow the underlying connection.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_applies_and_is_idempotent() {
        let database = Database::open_in_memory().unwrap();
        // Re-applying the schema and seed must not fail or duplicate rows.
        database.connection().execute_batch(SCHEMA).unwrap();
        crate::features::db::seed::seed_listen_modes(database.connection()).unwrap();
        let prompts: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM system_prompts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(prompts, 5);
        let tables: Vec<String> = database
            .connection()
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        for expected in [
            "conversations",
            "messages",
            "meetings",
            "meeting_lines",
            "files",
            "prompt_files",
            "saved_notes",
            "meeting_speakers",
            "app_settings",
            "cached_summaries",
            "system_prompts",
        ] {
            assert!(
                tables.iter().any(|name| name == expected),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn seed_prompts_exist_once() {
        let database = Database::open_in_memory().unwrap();
        database.connection().execute_batch(SCHEMA).unwrap();
        let count: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM system_prompts WHERE listen_slot IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn foreign_keys_are_enabled() {
        let database = Database::open_in_memory().unwrap();
        let enabled: i64 = database
            .connection()
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(enabled, 1);
    }
}
