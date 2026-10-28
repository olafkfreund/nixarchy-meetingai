#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub id: i64,
    pub name: String,
    pub text: String,
}

/// A profile's documents, most recently added first.
pub fn documents(connection: &Connection, profile: &str) -> crate::db::Result<Vec<Document>> {
    let mut query = connection.prepare(
        "SELECT id, name, text FROM profile_documents WHERE profile = ?1 ORDER BY id DESC",
    )?;
    let rows = query.query_map([profile], |row| {
        Ok(Document {
            id: row.get(0)?,
            name: row.get(1)?,
            text: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn add_document(
    connection: &Connection,
    profile: &str,
    name: &str,
    text: &str,
) -> crate::db::Result<i64> {
    connection.execute(
        "INSERT INTO profile_documents (profile, name, text) VALUES (?1, ?2, ?3)",
        params![profile, name, text],
    )?;
    Ok(connection.last_insert_rowid())
}

/// Returns whether a document of that profile was removed.
pub fn remove_document(connection: &Connection, profile: &str, id: i64) -> crate::db::Result<bool> {
    Ok(connection.execute(
        "DELETE FROM profile_documents WHERE profile = ?1 AND id = ?2",
        params![profile, id],
    )? > 0)
}

    fn documents_come_back_newest_first_and_die_with_the_profile() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        let p = Profile {
            id: "one".into(),
            name: "Interview".into(),
            personal_context: String::new(),
            meeting_context: String::new(),
        };
        save(c, &p).unwrap();
        let cv = add_document(c, "one", "cv.pdf", "CV text").unwrap();
        add_document(c, "one", "jd.md", "Job text").unwrap();
        let names: Vec<_> = documents(c, "one")
            .unwrap()
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert_eq!(names, ["jd.md", "cv.pdf"]);
        assert!(!remove_document(c, "other", cv).unwrap());
        assert!(remove_document(c, "one", cv).unwrap());
        delete(c, "one").unwrap();
        assert!(documents(c, "one").unwrap().is_empty());
    }
    #[test]
