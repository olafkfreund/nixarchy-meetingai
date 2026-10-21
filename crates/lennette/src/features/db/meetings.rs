//! Recorded sessions and their transcripts.
//!
//! A meeting is written incrementally while a live session runs, so a crash never
//! loses the conversation so far. Finished meetings are listed here with their
//! line counts and durations.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Result;

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

/// Rename a session.
pub fn rename(connection: &Connection, meeting_id: &str, title: &str) -> Result<()> {
    connection.execute(
        "UPDATE meetings SET title = ?2 WHERE id = ?1",
        params![meeting_id, title],
    )?;
    Ok(())
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
