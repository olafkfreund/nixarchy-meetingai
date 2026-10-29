use crate::db::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

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
pub fn insert(
    c: &Connection,
    name: &str,
    mime_type: &str,
    size_bytes: i64,
    pages: i64,
    ocr_text: &str,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    c.execute(
        "INSERT INTO files(id,name,mime_type,size_bytes,pages,ocr_text) VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, name, mime_type, size_bytes, pages, ocr_text],
    )?;
    Ok(id)
}
pub fn list(c: &Connection) -> Result<Vec<FileRecord>> {
    let mut s=c.prepare("SELECT id,name,mime_type,size_bytes,pages,ocr_text,created_at FROM files ORDER BY created_at DESC,rowid DESC")?;
    Ok(s.query_map([], |r| {
        Ok(FileRecord {
            id: r.get(0)?,
            name: r.get(1)?,
            mime_type: r.get(2)?,
            size_bytes: r.get(3)?,
            pages: r.get(4)?,
            ocr_text: r.get(5)?,
            created_at: r.get(6)?,
        })
    })?
    .collect::<std::result::Result<_, _>>()?)
}
pub fn rename(c: &Connection, id: &str, name: &str) -> Result<()> {
    c.execute(
        "UPDATE files SET name=?2,updated_at=datetime('now') WHERE id=?1",
        params![id, name],
    )?;
    Ok(())
}
pub fn delete(c: &Connection, id: &str) -> Result<()> {
    c.execute("DELETE FROM files WHERE id=?1", [id])?;
    Ok(())
}
pub fn attach_to_prompt(c: &Connection, prompt_id: &str, file_id: &str) -> Result<()> {
    c.execute(
        "INSERT OR IGNORE INTO prompt_files(prompt_id,file_id) VALUES(?1,?2)",
        params![prompt_id, file_id],
    )?;
    Ok(())
}
pub fn prompt_files(c: &Connection, prompt_id: &str) -> Result<Vec<String>> {
    let mut s =
        c.prepare("SELECT file_id FROM prompt_files WHERE prompt_id=?1 ORDER BY file_id")?;
    Ok(s.query_map([prompt_id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    #[test]
    fn file_lifecycle() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let id = insert(c, "draft.pdf", "application/pdf", 11, 1, "text").unwrap();
        assert_eq!(list(c).unwrap().len(), 1);
        assert_eq!(list(c).unwrap()[0].name, "draft.pdf");
        rename(c, &id, "final.pdf").unwrap();
        assert_eq!(list(c).unwrap()[0].name, "final.pdf");
        attach_to_prompt(c, "p", &id).unwrap();
        attach_to_prompt(c, "p", &id).unwrap();
        assert_eq!(
            prompt_files(c, "p").unwrap().as_slice(),
            std::slice::from_ref(&id)
        );
        delete(c, &id).unwrap();
        assert!(prompt_files(c, "p").unwrap().is_empty());
    }

    #[test]
    fn files_list_newest_first_with_their_stored_fields() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let first = insert(c, "notes.txt", "text/plain", 17, 0, "draft").unwrap();
        let second = insert(c, "slides.pdf", "application/pdf", 2048, 12, "agenda").unwrap();
        let files = list(c).unwrap();
        assert_eq!(
            files
                .iter()
                .map(|file| file.id.as_str())
                .collect::<Vec<_>>(),
            [second.as_str(), first.as_str()]
        );
        assert_eq!(files[0].mime_type, "application/pdf");
        assert_eq!(files[0].size_bytes, 2048);
        assert_eq!(files[0].pages, 12);
        assert_eq!(files[0].ocr_text, "agenda");
    }
}
