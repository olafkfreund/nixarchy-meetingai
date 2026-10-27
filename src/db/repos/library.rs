//! Document library and attached prompt knowledge.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Result;

/// A stored document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub pages: i64,
    pub ocr_text: String,
    pub created_at: String,
}

/// Insert a document and return its id.
pub fn insert(
    connection: &Connection,
    name: &str,
    mime_type: &str,
    size_bytes: i64,
    pages: i64,
    ocr_text: &str,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO files (id, name, mime_type, size_bytes, pages, ocr_text)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, name, mime_type, size_bytes, pages, ocr_text],
    )?;
    Ok(id)
}

/// All documents, newest first.
pub fn list(connection: &Connection) -> Result<Vec<FileRecord>> {
    let mut statement = connection.prepare(
        "SELECT id, name, mime_type, size_bytes, pages, ocr_text, created_at
         FROM files ORDER BY created_at DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(FileRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            mime_type: row.get(2)?,
            size_bytes: row.get(3)?,
            pages: row.get(4)?,
            ocr_text: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Rename a document.
pub fn rename(connection: &Connection, id: &str, name: &str) -> Result<()> {
    connection.execute(
        "UPDATE files SET name = ?2, updated_at = datetime('now') WHERE id = ?1",
        params![id, name],
    )?;
    Ok(())
}

/// Delete a document, which also detaches it from prompts via the cascade.
pub fn delete(connection: &Connection, id: &str) -> Result<()> {
    connection.execute("DELETE FROM files WHERE id = ?1", params![id])?;
    Ok(())
}

/// Attach a document to a prompt.
pub fn attach_to_prompt(connection: &Connection, prompt_id: &str, file_id: &str) -> Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO prompt_files (prompt_id, file_id) VALUES (?1, ?2)",
        params![prompt_id, file_id],
    )?;
    Ok(())
}

/// Document ids attached to a prompt.
pub fn prompt_files(connection: &Connection, prompt_id: &str) -> Result<Vec<String>> {
    let mut statement =
        connection.prepare("SELECT file_id FROM prompt_files WHERE prompt_id = ?1")?;
    let rows = statement.query_map(params![prompt_id], |row| row.get(0))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn insert_and_list() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        insert(connection, "notes.pdf", "application/pdf", 100, 3, "text").unwrap();
        let files = list(connection).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "notes.pdf");
    }

    #[test]
    fn deleting_file_removes_prompt_links() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        let id = insert(connection, "a.txt", "text/plain", 10, 1, "x").unwrap();
        attach_to_prompt(connection, "1", &id).unwrap();
        assert_eq!(prompt_files(connection, "1").unwrap().len(), 1);
        delete(connection, &id).unwrap();
        assert!(prompt_files(connection, "1").unwrap().is_empty());
    }

    #[test]
    fn rename_updates_name() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        let id = insert(connection, "old", "text/plain", 1, 1, "").unwrap();
        rename(connection, &id, "new").unwrap();
        assert_eq!(list(connection).unwrap()[0].name, "new");
    }

    #[test]
    fn attaching_twice_is_idempotent() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        let id = insert(connection, "a", "text/plain", 1, 1, "").unwrap();
        attach_to_prompt(connection, "1", &id).unwrap();
        attach_to_prompt(connection, "1", &id).unwrap();
        assert_eq!(prompt_files(connection, "1").unwrap().len(), 1);
    }
}
