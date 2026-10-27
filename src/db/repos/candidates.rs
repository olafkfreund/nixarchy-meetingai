//! Interviewer mode: a candidate, their documents and the question bank for one interview.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Result;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub local_only: bool,
    pub meeting_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Document {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Question {
    pub id: String,
    pub candidate_id: String,
    pub text: String,
    pub source: String,
    pub competency: Option<String>,
    pub status: String,
    pub note: Option<String>,
    pub position: i64,
}

/// Fields to change; `None` leaves a field alone.
#[derive(Debug, Default, Deserialize)]
pub struct QuestionPatch {
    pub text: Option<String>,
    pub status: Option<String>,
    pub note: Option<String>,
    pub competency: Option<String>,
}

const COLUMNS: &str = "id, name, local_only, meeting_id, created_at";

fn candidate_row(row: &rusqlite::Row) -> rusqlite::Result<Candidate> {
    Ok(Candidate {
        id: row.get(0)?,
        name: row.get(1)?,
        local_only: row.get::<_, i64>(2)? != 0,
        meeting_id: row.get(3)?,
        created_at: row.get(4)?,
    })
}

pub fn create(connection: &Connection, name: &str) -> Result<Candidate> {
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO candidates (id, name) VALUES (?1, ?2)",
        params![id, name.trim()],
    )?;
    Ok(get(connection, &id)?.expect("just inserted"))
}

pub fn get(connection: &Connection, id: &str) -> Result<Option<Candidate>> {
    Ok(connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM candidates WHERE id = ?1"),
            params![id],
            candidate_row,
        )
        .optional()?)
}

/// Candidates without a meeting first, then the most recent.
pub fn list(connection: &Connection) -> Result<Vec<Candidate>> {
    let mut statement = connection.prepare(&format!(
        "SELECT {COLUMNS} FROM candidates ORDER BY meeting_id IS NOT NULL, rowid DESC"
    ))?;
    let rows = statement.query_map([], candidate_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn link_meeting(connection: &Connection, id: &str, meeting_id: &str) -> Result<()> {
    connection.execute(
        "UPDATE candidates SET meeting_id = ?2 WHERE id = ?1",
        params![id, meeting_id],
    )?;
    Ok(())
}

pub fn set_local_only(connection: &Connection, id: &str, on: bool) -> Result<bool> {
    Ok(connection.execute(
        "UPDATE candidates SET local_only = ?2 WHERE id = ?1",
        params![id, on],
    )? > 0)
}

/// The candidate linked to a meeting, if any.
pub fn for_meeting(connection: &Connection, meeting_id: &str) -> Result<Option<Candidate>> {
    Ok(connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM candidates WHERE meeting_id = ?1"),
            params![meeting_id],
            candidate_row,
        )
        .optional()?)
}

pub fn add_document(
    connection: &Connection,
    candidate_id: &str,
    kind: &str,
    name: &str,
    text: &str,
) -> Result<Document> {
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO candidate_documents (id, candidate_id, kind, name, text) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, candidate_id, kind, name, text],
    )?;
    Ok(Document {
        id,
        kind: kind.to_owned(),
        name: name.to_owned(),
        text: text.to_owned(),
    })
}

pub fn documents(connection: &Connection, candidate_id: &str) -> Result<Vec<Document>> {
    let mut statement = connection.prepare(
        "SELECT id, kind, name, text FROM candidate_documents WHERE candidate_id = ?1 ORDER BY rowid",
    )?;
    let rows = statement.query_map(params![candidate_id], |row| {
        Ok(Document {
            id: row.get(0)?,
            kind: row.get(1)?,
            name: row.get(2)?,
            text: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

const QUESTION_COLUMNS: &str = "id, candidate_id, text, source, competency, status, note, position";

fn question_row(row: &rusqlite::Row) -> rusqlite::Result<Question> {
    Ok(Question {
        id: row.get(0)?,
        candidate_id: row.get(1)?,
        text: row.get(2)?,
        source: row.get(3)?,
        competency: row.get(4)?,
        status: row.get(5)?,
        note: row.get(6)?,
        position: row.get(7)?,
    })
}

/// Append a planned question at the end of the bank.
pub fn add_question(
    connection: &Connection,
    candidate_id: &str,
    text: &str,
    source: &str,
    competency: Option<&str>,
) -> Result<Question> {
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO interview_questions (id, candidate_id, text, source, competency, position)
         VALUES (?1, ?2, ?3, ?4, ?5,
           (SELECT COALESCE(MAX(position), -1) + 1 FROM interview_questions WHERE candidate_id = ?2))",
        params![id, candidate_id, text, source, competency],
    )?;
    Ok(question(connection, &id)?.expect("just inserted"))
}

pub fn question(connection: &Connection, id: &str) -> Result<Option<Question>> {
    Ok(connection
        .query_row(
            &format!("SELECT {QUESTION_COLUMNS} FROM interview_questions WHERE id = ?1"),
            params![id],
            question_row,
        )
        .optional()?)
}

pub fn questions(connection: &Connection, candidate_id: &str) -> Result<Vec<Question>> {
    let mut statement = connection.prepare(&format!(
        "SELECT {QUESTION_COLUMNS} FROM interview_questions WHERE candidate_id = ?1 ORDER BY position"
    ))?;
    let rows = statement.query_map(params![candidate_id], question_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Apply a patch; false when the question does not exist. A bad status fails the CHECK.
pub fn update_question(connection: &Connection, id: &str, patch: &QuestionPatch) -> Result<bool> {
    let changed = connection.execute(
        "UPDATE interview_questions SET text = COALESCE(?2, text), status = COALESCE(?3, status),
           note = COALESCE(?4, note), competency = COALESCE(?5, competency) WHERE id = ?1",
        params![id, patch.text, patch.status, patch.note, patch.competency],
    )?;
    Ok(changed > 0)
}

/// Number the listed questions 0.. in the given order; others of the candidate keep their relative order after them.
pub fn reorder(connection: &Connection, candidate_id: &str, ids: &[String]) -> Result<()> {
    let tx = connection.unchecked_transaction()?;
    let mut position = 0;
    for id in ids {
        position += tx.execute(
            "UPDATE interview_questions SET position = ?3 WHERE id = ?1 AND candidate_id = ?2",
            params![id, candidate_id, position],
        )? as i64;
    }
    // shortcut: unlisted questions all go to the end with the same position, ties break by rowid.
    tx.execute(
        "UPDATE interview_questions SET position = ?2 WHERE candidate_id = ?1 AND id NOT IN (SELECT value FROM json_each(?3))",
        params![candidate_id, position, serde_json::to_string(ids).unwrap_or_default()],
    )?;
    Ok(tx.commit()?)
}

pub fn delete_question(connection: &Connection, id: &str) -> Result<bool> {
    Ok(connection.execute("DELETE FROM interview_questions WHERE id = ?1", params![id])? > 0)
}

/// Remove the candidate, their documents and questions, and the linked meeting with every
/// row that hangs off it, in one transaction. Returns false when the candidate is unknown.
pub fn delete_all(connection: &Connection, id: &str) -> Result<bool> {
    let Some(candidate) = get(connection, id)? else {
        return Ok(false);
    };
    let tx = connection.unchecked_transaction()?;
    if let Some(meeting) = candidate.meeting_id {
        // Chat about the interview is candidate data too; messages cascade from the conversation.
        tx.execute(
            "DELETE FROM conversations WHERE id IN (SELECT conversation_id FROM meeting_conversations WHERE meeting_id = ?1)",
            params![meeting],
        )?;
        tx.execute("DELETE FROM meetings WHERE id = ?1", params![meeting])?;
        tx.execute(
            "DELETE FROM notes_fts WHERE meeting_id = ?1",
            params![meeting],
        )?;
    }
    tx.execute("DELETE FROM candidates WHERE id = ?1", params![id])?;
    tx.commit()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, meetings};

    fn count(c: &Connection, table: &str) -> i64 {
        c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn crud_order_and_listing() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        let a = create(c, " Ada ").unwrap();
        assert!(a.name == "Ada" && !a.local_only && a.meeting_id.is_none());
        let b = create(c, "Bo").unwrap();
        let m = meetings::start(c, "", "", "en", None, Default::default()).unwrap();
        link_meeting(c, &a.id, &m).unwrap();
        // Unlinked first, then recent.
        let names: Vec<_> = list(c).unwrap().into_iter().map(|x| x.name).collect();
        assert_eq!(names, ["Bo", "Ada"]);
        assert_eq!(for_meeting(c, &m).unwrap().unwrap().id, a.id);
        assert!(
            set_local_only(c, &b.id, true).unwrap() && get(c, &b.id).unwrap().unwrap().local_only
        );
        assert!(!set_local_only(c, "nope", true).unwrap());

        add_document(c, &a.id, "cv", "cv.md", "Built Rust").unwrap();
        assert!(add_document(c, &a.id, "other", "x", "y").is_err());
        assert_eq!(documents(c, &a.id).unwrap()[0].text, "Built Rust");

        let q1 = add_question(c, &a.id, "Q1?", "Built Rust", Some("rust")).unwrap();
        let q2 = add_question(c, &a.id, "Q2?", "Built Rust", None).unwrap();
        let q3 = add_question(c, &a.id, "Q3?", "Built Rust", None).unwrap();
        assert_eq!((q1.position, q2.position, q3.position), (0, 1, 2));
        reorder(c, &a.id, &[q3.id.clone(), q1.id.clone()]).unwrap();
        let ids: Vec<_> = questions(c, &a.id)
            .unwrap()
            .into_iter()
            .map(|q| q.id)
            .collect();
        assert_eq!(ids, [q3.id.clone(), q1.id.clone(), q2.id.clone()]);

        let patch = QuestionPatch {
            status: Some("asked".into()),
            note: Some("said X".into()),
            ..QuestionPatch::default()
        };
        assert!(update_question(c, &q1.id, &patch).unwrap());
        let got = question(c, &q1.id).unwrap().unwrap();
        assert!(
            got.status == "asked" && got.note.as_deref() == Some("said X") && got.text == "Q1?"
        );
        let bad = QuestionPatch {
            status: Some("later".into()),
            ..QuestionPatch::default()
        };
        assert!(update_question(c, &q1.id, &bad).is_err());
        assert!(!update_question(c, "nope", &QuestionPatch::default()).unwrap());
        assert!(delete_question(c, &q2.id).unwrap() && !delete_question(c, &q2.id).unwrap());
    }

    #[test]
    fn delete_all_leaves_nothing_of_the_interview() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        let keep = create(c, "Keep").unwrap();
        add_question(c, &keep.id, "K?", "k", None).unwrap();
        let other = meetings::start(c, "Other", "", "en", None, Default::default()).unwrap();
        meetings::add_line(c, &other, "me", None, "other talk", 0.0, 1.0).unwrap();

        let a = create(c, "Ada").unwrap();
        let m = meetings::start(c, "Ada", "", "en", None, Default::default()).unwrap();
        link_meeting(c, &a.id, &m).unwrap();
        add_document(c, &a.id, "cv", "cv.md", "Built Rust").unwrap();
        add_question(c, &a.id, "Q?", "Built Rust", None).unwrap();
        meetings::add_line(c, &m, "them", None, "I built it", 0.0, 1.0).unwrap();
        meetings::add_chunk(
            c,
            &m,
            &meetings::NoteChunk {
                t0: 0.0,
                t1: 1.0,
                body: "notes".into(),
                last_line: 1,
            },
        )
        .unwrap();
        meetings::begin_recap(c, &m).unwrap();
        meetings::index_notes(c, &m, "recap text").unwrap();
        c.execute(
            "INSERT INTO meeting_items (id, meeting_id, kind, text) VALUES ('i', ?1, 'action', 'x')",
            params![m],
        )
        .unwrap();
        c.execute("INSERT INTO conversations (id, title, created_at, updated_at) VALUES ('conv', 't', 0, 0)", []).unwrap();
        c.execute(
            "INSERT INTO meeting_conversations VALUES (?1, 'conv')",
            params![m],
        )
        .unwrap();

        assert!(delete_all(c, &a.id).unwrap());
        assert!(!delete_all(c, &a.id).unwrap());
        for table in [
            "meeting_lines",
            "meeting_note_chunks",
            "meeting_recaps",
            "meeting_items",
            "meeting_conversations",
            "conversations",
            "notes_fts",
            "candidate_documents",
        ] {
            let left: i64 = match table {
                "meeting_lines"
                | "meeting_note_chunks"
                | "meeting_recaps"
                | "meeting_items"
                | "notes_fts" => c
                    .query_row(
                        &format!("SELECT COUNT(*) FROM {table} WHERE meeting_id = ?1"),
                        params![m],
                        |r| r.get(0),
                    )
                    .unwrap(),
                _ => count(c, table),
            };
            assert_eq!(left, 0, "{table}");
        }
        assert_eq!(count(c, "meetings"), 1);
        assert_eq!(count(c, "candidates"), 1);
        assert_eq!(count(c, "interview_questions"), 1);
        assert_eq!(count(c, "meeting_lines"), 1);
    }
}
