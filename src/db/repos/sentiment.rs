//! Encrypted, cached meeting tone assessments and bounded source sampling.
use crate::db::{Result, meetings::LineRecord};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub lines: i64,
    pub last_row: i64,
}

pub fn revision(db: &Connection, id: &str) -> Result<Revision> {
    Ok(db.query_row(
        "SELECT count(*), coalesce(max(rowid),0) FROM meeting_lines WHERE meeting_id=?1",
        [id],
        |r| {
            Ok(Revision {
                lines: r.get(0)?,
                last_row: r.get(1)?,
            })
        },
    )?)
}

#[derive(Clone, Debug)]
pub struct Sample {
    pub revision: Revision,
    pub lines: Vec<LineRecord>,
}

/// At most 96 fragments, distributed through the entire meeting, 800 chars each.
/// Bounded allocations do not turn a large archive into a model-sized prompt.
pub fn sample(db: &Connection, id: &str) -> Result<Sample> {
    let revision = revision(db, id)?;
    let count = revision.lines as usize;
    let mut statement = db.prepare("SELECT id, who, speaker, substr(text,1,800), start_time, end_time FROM meeting_lines WHERE meeting_id=?1 ORDER BY start_time,rowid LIMIT 1 OFFSET ?2")?;
    let mut lines = Vec::new();
    for i in 0..count.min(96) {
        let offset = if count <= 96 { i } else { i * (count - 1) / 95 };
        lines.push(statement.query_row(params![id, offset as i64], |r| {
            Ok(LineRecord {
                id: r.get(0)?,
                who: r.get(1)?,
                speaker: r.get(2)?,
                text: r.get(3)?,
                start_time: r.get(4)?,
                end_time: r.get(5)?,
            })
        })?);
    }
    Ok(Sample { revision, lines })
}

#[derive(Debug, Clone)]
pub struct Saved {
    pub content: String,
    pub status: String,
    pub error: String,
    pub revision: Revision,
    pub updated_at: String,
}

pub fn get(db: &Connection, id: &str) -> Result<Option<Saved>> {
    Ok(db.query_row("SELECT content,status,error,source_lines,source_last_row,updated_at FROM meeting_sentiment WHERE meeting_id=?1", [id], |r| Ok(Saved {
        content:r.get(0)?,status:r.get(1)?,error:r.get(2)?,revision:Revision {lines:r.get(3)?,last_row:r.get(4)?},updated_at:r.get(5)?,
    })).optional()?)
}

pub fn begin(db: &Connection, id: &str, revision: Revision) -> Result<()> {
    db.execute("INSERT INTO meeting_sentiment (meeting_id,source_lines,source_last_row) VALUES (?1,?2,?3) ON CONFLICT(meeting_id) DO UPDATE SET status='pending',error='',source_lines=?2,source_last_row=?3,updated_at=datetime('now')",params![id,revision.lines,revision.last_row])?;
    Ok(())
}

pub fn finish(
    db: &Connection,
    id: &str,
    source: Revision,
    result: std::result::Result<&str, &str>,
) -> Result<()> {
    let (status, content, error) = match result {
        Ok(content) => ("ready", content, ""),
        Err(error) => ("failed", "", error),
    };
    // A deleted meeting cannot be recreated by a late analysis response; an older
    // response cannot overwrite a new generation that sampled a later transcript.
    db.execute("UPDATE meeting_sentiment SET status=?2,content=?3,error=?4,updated_at=datetime('now') WHERE meeting_id=?1 AND source_lines=?5 AND source_last_row=?6",params![id,status,content,error,source.lines,source.last_row])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, meetings};
    #[test]
    fn sampling_covers_beginning_middle_and_end_and_caps_text() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        let id = meetings::start(c, "QA", "auto", "en").unwrap();
        for i in 0..1000 {
            meetings::add_line(
                c,
                &id,
                "them",
                None,
                &format!("{i}:{}", "x".repeat(900)),
                i as f64,
                i as f64 + 1.,
            )
            .unwrap();
        }
        let sample = sample(c, &id).unwrap();
        assert_eq!(sample.lines.len(), 96);
        assert!(sample.lines[0].text.starts_with("0:"));
        assert!(sample.lines[95].text.starts_with("999:"));
        assert!((490.0..510.0).contains(&sample.lines[48].start_time));
        assert!(sample.lines.iter().all(|l| l.text.chars().count() <= 800));
        begin(c, &id, sample.revision).unwrap();
        finish(c, &id, sample.revision, Ok("{}")).unwrap();
        assert_eq!(get(c, &id).unwrap().unwrap().status, "ready");
        meetings::add_line(c, &id, "me", None, "Later", 1001., 1002.).unwrap();
        let later = revision(c, &id).unwrap();
        assert_ne!(sample.revision, later);
        begin(c, &id, later).unwrap();
        finish(c, &id, sample.revision, Ok("old")).unwrap();
        assert_eq!(get(c, &id).unwrap().unwrap().status, "pending");
        meetings::delete(c, &id).unwrap();
        finish(c, &id, later, Ok("late")).unwrap();
        assert!(get(c, &id).unwrap().is_none());
    }
}
