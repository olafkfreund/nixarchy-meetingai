//! Local database.
//!
//! SQLite with the schema: conversations, messages, live-session transcripts,
//! the document library and shared settings. Repositories keep SQL in one place
//! so views never build queries inline.

pub mod repos;
pub mod schema;

pub use repos::conversations;
pub use repos::library;
pub use repos::meetings;
pub use repos::settings;
pub use schema::seed;

use rusqlite::Connection;
use std::path::Path;

/// Errors from the local database.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DbError>;

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

    /// Open an in-memory database, useful for unit tests.
    pub fn in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        Self::prepare(connection)
    }

    /// Alias for `in_memory`.
    pub fn open_in_memory() -> Result<Self> {
        Self::in_memory()
    }

    fn prepare(connection: Connection) -> Result<Self> {
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        connection.execute_batch(schema::SCHEMA)?;
        seed::seed_listen_modes(&connection)?;
        Ok(Self { connection })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_applies_and_is_idempotent() {
        let db = Database::in_memory().unwrap();
        db.connection().execute_batch(schema::SCHEMA).unwrap();
    }

    #[test]
    fn seed_prompts_exist_once() {
        let db = Database::in_memory().unwrap();
        let prompts = repos::settings::prompts(db.connection()).unwrap();
        assert_eq!(prompts.len(), 5);
    }

    #[test]
    fn foreign_keys_are_enabled() {
        let db = Database::in_memory().unwrap();
        let enabled: i64 = db
            .connection()
            .query_row("PRAGMA foreign_keys;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(enabled, 1);
    }
}
