//! Recorded sessions and their transcripts.
//!
//! A meeting is written incrementally while a live session runs, so a crash never
//! loses the conversation so far. Finished meetings are listed here with their
//! line counts and durations.

use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Result;

#[derive(Debug, PartialEq)]
pub struct Recap {
    pub content: String,
    pub status: String,
    pub error: String,
}
pub fn recap(connection: &Connection, id: &str) -> Result<Option<Recap>> {
    Ok(connection
        .query_row(
            "SELECT content, status, error FROM meeting_recaps WHERE meeting_id = ?1",
            params![id],
            |row| {
                Ok(Recap {
                    content: row.get(0)?,
                    status: row.get(1)?,
                    error: row.get(2)?,
                })
            },
        )
        .optional()?)
}
pub fn begin_recap(connection: &Connection, id: &str) -> Result<()> {
    connection.execute("INSERT INTO meeting_recaps (meeting_id) VALUES (?1) ON CONFLICT(meeting_id) DO UPDATE SET status='pending', error='', updated_at=datetime('now')", params![id])?;
    Ok(())
}
pub fn finish_recap(
    connection: &Connection,
    id: &str,
    result: std::result::Result<&str, &str>,
) -> Result<()> {
    let (status, content, error) = match result {
        Ok(content) => ("ready", content, ""),
        Err(error) => ("failed", "", error),
    };
    // UPDATE cannot recreate a meeting deleted while its AI request was running.
    connection.execute("UPDATE meeting_recaps SET status=?2, content=?3, error=?4, updated_at=datetime('now') WHERE meeting_id=?1", params![id, status, content, error])?;
    Ok(())
}

/// A recorded session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeetingRecord {
    pub id: String,
    pub title: String,
    pub context: String,
    pub language: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: i64,
    pub line_count: i64,
}

/// One transcript line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineRecord {
    pub id: String,
    pub who: String,
    pub speaker: Option<i64>,
    pub text: String,
    pub start_time: f64,
    pub end_time: f64,
}

/// Start a session and return its id.
pub fn start(
    connection: &Connection,
    title: &str,
    context: &str,
    language: &str,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO meetings (id, title, context, language) VALUES (?1, ?2, ?3, ?4)",
        params![id, title, context, language],
    )?;
    Ok(id)
}

/// Append a transcript line as it arrives.
///
/// `who` is `me` for the local microphone or `them` for the other side; `speaker`
/// carries the diarized index when the provider supplies one.
pub fn add_line(
    connection: &Connection,
    meeting_id: &str,
    who: &str,
    speaker: Option<i64>,
    text: &str,
    start_time: f64,
    end_time: f64,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO meeting_lines (id, meeting_id, who, speaker, text, start_time, end_time)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, meeting_id, who, speaker, text, start_time, end_time],
    )?;
    Ok(id)
}

/// Finish a session, recording when it ended and how long it ran.
pub fn finish(connection: &Connection, meeting_id: &str, duration_seconds: i64) -> Result<()> {
    connection.execute(
        "UPDATE meetings
         SET ended_at = datetime('now'), duration_seconds = ?2
         WHERE id = ?1",
        params![meeting_id, duration_seconds],
    )?;
    Ok(())
}

/// Resume recording into the same session without changing its existing speech.
pub fn reopen(connection: &Connection, id: &str) -> Result<()> {
    if connection.execute("UPDATE meetings SET ended_at=NULL WHERE id=?1", [id])? == 0 {
        return Err(crate::db::DbError::Sqlite(
            rusqlite::Error::QueryReturnedNoRows,
        ));
    }
    Ok(())
}

/// Rename a session.
pub fn rename(connection: &Connection, meeting_id: &str, title: &str) -> Result<()> {
    connection.execute(
        "UPDATE meetings SET title = ?2 WHERE id = ?1",
        params![meeting_id, title],
    )?;
    Ok(())
}

/// Finished sessions still carrying the timestamp title from creation, with
/// their transcript as plain text so a title can be generated for each.
pub fn untitled(connection: &Connection, limit: usize) -> Result<Vec<(String, String)>> {
    let mut statement = connection.prepare(
        "SELECT id FROM meetings
         WHERE title LIKE 'Session · %'
         ORDER BY started_at DESC
         LIMIT ?1",
    )?;
    let ids: Vec<String> = statement
        .query_map(params![limit as i64], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut found = Vec::new();
    for id in ids {
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM meeting_lines WHERE meeting_id=?1",
            [&id],
            |r| r.get(0),
        )?;
        if count < 2 {
            continue;
        }
        let body = transcript_excerpt(connection, &id, 6000)?;
        found.push((id, body));
    }
    Ok(found)
}

/// Delete a session; its lines go with it through the cascade.
pub fn delete(connection: &Connection, meeting_id: &str) -> Result<()> {
    connection.execute("DELETE FROM meetings WHERE id = ?1", params![meeting_id])?;
    Ok(())
}

/// Most recent sessions first.
pub fn list(connection: &Connection, limit: usize) -> Result<Vec<MeetingRecord>> {
    let mut statement = connection.prepare(
        "SELECT m.id, m.title, m.context, m.language, m.started_at, m.ended_at,
                m.duration_seconds, COUNT(l.id)
         FROM meetings m
         LEFT JOIN meeting_lines l ON l.meeting_id = m.id
         GROUP BY m.id
         ORDER BY m.started_at DESC
         LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok(MeetingRecord {
            id: row.get(0)?,
            title: row.get(1)?,
            context: row.get(2)?,
            language: row.get(3)?,
            started_at: row.get(4)?,
            ended_at: row.get(5)?,
            duration_seconds: row.get(6)?,
            line_count: row.get(7)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Lines for one session, in spoken order.
pub fn lines(connection: &Connection, meeting_id: &str) -> Result<Vec<LineRecord>> {
    let mut statement = connection.prepare(
        "SELECT id, who, speaker, text, start_time, end_time FROM meeting_lines
         WHERE meeting_id = ?1
         ORDER BY start_time ASC, rowid ASC",
    )?;
    let rows = statement.query_map(params![meeting_id], |row| {
        Ok(LineRecord {
            id: row.get(0)?,
            who: row.get(1)?,
            speaker: row.get(2)?,
            text: row.get(3)?,
            start_time: row.get(4)?,
            end_time: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Recent speech for restoring the live view without loading an entire meeting.
pub fn recent_lines(
    connection: &Connection,
    meeting_id: &str,
    limit: usize,
) -> Result<Vec<LineRecord>> {
    let mut statement = connection.prepare(
        "SELECT id, who, speaker, substr(text, 1, 8000), start_time, end_time FROM meeting_lines
         WHERE meeting_id = ?1 ORDER BY start_time DESC, rowid DESC LIMIT ?2",
    )?;
    let mut rows = statement
        .query_map(params![meeting_id, limit as i64], |r| {
            Ok(LineRecord {
                id: r.get(0)?,
                who: r.get(1)?,
                speaker: r.get(2)?,
                text: r.get(3)?,
                start_time: r.get(4)?,
                end_time: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.reverse();
    Ok(rows)
}

/// Bounded prompt material read incrementally. Excerpts explicitly disclose any
/// omitted middle; originals are never shortened in storage.
pub fn transcript_excerpt(
    connection: &Connection,
    meeting_id: &str,
    max_chars: usize,
) -> Result<String> {
    let total: i64 = connection.query_row(
        "SELECT COALESCE(SUM(length(text) + CASE WHEN who='me' THEN 6 WHEN speaker IS NULL THEN 7 ELSE 11+length(CAST(speaker+1 AS TEXT)) END), 0) FROM meeting_lines WHERE meeting_id=?1",
        [meeting_id], |r| r.get(0),
    )?;
    let truncated = total as usize > max_chars;
    let budget = if truncated { max_chars / 2 } else { max_chars };
    let read = |reverse: bool| -> Result<String> {
        let sql = if reverse {
            "SELECT who, speaker, substr(text, -?2) FROM meeting_lines WHERE meeting_id=?1 ORDER BY start_time DESC, rowid DESC"
        } else {
            "SELECT who, speaker, substr(text, 1, ?2) FROM meeting_lines WHERE meeting_id=?1 ORDER BY start_time ASC, rowid ASC"
        };
        let mut statement = connection.prepare(sql)?;
        let mut rows = statement.query(params![meeting_id, budget.max(1) as i64])?;
        let mut remaining = budget;
        let mut parts = Vec::new();
        while remaining > 0
            && let Some(row) = rows.next()?
        {
            let who: String = row.get(0)?;
            let speaker: Option<i64> = row.get(1)?;
            let text: String = row.get(2)?;
            let label = if who == "me" {
                "You".into()
            } else {
                speaker
                    .map(|s| format!("Speaker {}", s + 1))
                    .unwrap_or("Them".into())
            };
            let line = format!("{label}: {text}\n");
            let part: String = if reverse {
                line.chars()
                    .rev()
                    .take(remaining)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect()
            } else {
                line.chars().take(remaining).collect()
            };
            remaining = remaining.saturating_sub(part.chars().count());
            parts.push(part);
        }
        if reverse {
            parts.reverse();
        }
        Ok(parts.concat())
    };
    let mut text = read(false)?;
    if truncated {
        text.push_str("\n[Middle omitted; this context covers only the supplied excerpts.]\n");
        text.push_str(&read(true)?);
    }
    Ok(text)
}

/// Give a speaker a display name.
pub fn name_speaker(
    connection: &Connection,
    meeting_id: &str,
    key: &str,
    name: &str,
) -> Result<()> {
    connection.execute(
        "INSERT INTO meeting_speakers (meeting_id, speaker_key, name) VALUES (?1, ?2, ?3)
         ON CONFLICT(meeting_id, speaker_key) DO UPDATE SET name = excluded.name",
        params![meeting_id, key, name],
    )?;
    Ok(())
}

/// Speaker names for a session, keyed by diarized identity.
pub fn speaker_names(
    connection: &Connection,
    meeting_id: &str,
) -> Result<std::collections::BTreeMap<String, String>> {
    let mut statement = connection
        .prepare("SELECT speaker_key, name FROM meeting_speakers WHERE meeting_id = ?1")?;
    let rows = statement.query_map(params![meeting_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Format a duration as `mm:ss`.
pub fn format_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::db::Database;

    fn database() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn recap_excerpts_bound_large_meetings_and_disclose_omitted_facts() {
        let db = database();
        let id = start(db.connection(), "Long meeting", "auto", "en").unwrap();
        for i in 0..1000 {
            add_line(
                db.connection(),
                &id,
                "them",
                Some(1),
                &format!("Fact {i}: {}", "é".repeat(100)),
                i as f64,
                i as f64 + 1.0,
            )
            .unwrap();
        }
        let excerpt = transcript_excerpt(db.connection(), &id, 6000).unwrap();
        assert!(excerpt.contains("Fact 0:"));
        assert!(excerpt.contains("Fact 999:"));
        assert!(excerpt.contains("Middle omitted"));
        assert!(!excerpt.contains("Fact 500:"));
        assert!(excerpt.chars().count() < 6100);
        let recent = recent_lines(db.connection(), &id, 3).unwrap();
        assert!(recent[0].text.starts_with("Fact 997:"));
        assert!(recent[2].text.starts_with("Fact 999:"));
        assert_eq!(lines(db.connection(), &id).unwrap().len(), 1000);
    }

    #[test]
    fn session_records_lines_incrementally() {
        let database = database();
        let connection = database.connection();
        let id = start(connection, "Standup", "meeting", "en").unwrap();
        add_line(connection, &id, "me", None, "Good morning", 0.0, 1.2).unwrap();
        add_line(connection, &id, "them", Some(0), "Morning all", 1.3, 2.4).unwrap();

        // Lines are visible before the session ends: this is what survives a crash.
        assert_eq!(lines(connection, &id).unwrap().len(), 2);
        let meeting = &list(connection, 10).unwrap()[0];
        assert_eq!(meeting.line_count, 2);
        assert_eq!(meeting.ended_at, None);
    }

    #[test]
    fn finishing_records_duration() {
        let database = database();
        let connection = database.connection();
        let id = start(connection, "Call", "meeting", "en").unwrap();
        finish(connection, &id, 125).unwrap();
        let meeting = &list(connection, 10).unwrap()[0];
        assert_eq!(meeting.duration_seconds, 125);
        assert!(meeting.ended_at.is_some());
    }

    #[test]
    fn deleting_session_cascades_lines() {
        let database = database();
        let connection = database.connection();
        let id = start(connection, "Call", "meeting", "en").unwrap();
        add_line(connection, &id, "me", None, "hello", 0.0, 1.0).unwrap();
        delete(connection, &id).unwrap();
        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM meeting_lines", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn lines_are_ordered_by_time() {
        let database = database();
        let connection = database.connection();
        let id = start(connection, "Call", "meeting", "en").unwrap();
        add_line(connection, &id, "them", None, "second", 5.0, 6.0).unwrap();
        add_line(connection, &id, "me", None, "first", 1.0, 2.0).unwrap();
        let lines = lines(connection, &id).unwrap();
        assert_eq!(lines[0].text, "first");
        assert_eq!(lines[1].text, "second");
    }

    #[test]
    fn speaker_names_upsert() {
        let database = database();
        let connection = database.connection();
        let id = start(connection, "Call", "meeting", "en").unwrap();
        name_speaker(connection, &id, "them:0", "Sam").unwrap();
        name_speaker(connection, &id, "them:0", "Samantha").unwrap();
        let names = speaker_names(connection, &id).unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names["them:0"], "Samantha");
    }

    #[test]
    fn rename_changes_title() {
        let database = database();
        let connection = database.connection();
        let id = start(connection, "Untitled", "meeting", "en").unwrap();
        rename(connection, &id, "Weekly").unwrap();
        assert_eq!(list(connection, 10).unwrap()[0].title, "Weekly");
    }

    #[test]
    fn durations_format_as_minutes_and_seconds() {
        assert_eq!(format_duration(0), "00:00");
        assert_eq!(format_duration(65), "01:05");
        assert_eq!(format_duration(3725), "62:05");
        assert_eq!(format_duration(-5), "00:00");
    }
}
