use crate::{
    core::settings::MeetingMode,
    db::{DbError, Result},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, PartialEq)]
pub struct Recap {
    pub content: String,
    pub status: String,
    pub error: String,
    pub updated_at: String,
}
pub fn recap(c: &Connection, id: &str) -> Result<Option<Recap>> {
    Ok(c.query_row(
        "SELECT content,status,error,updated_at FROM meeting_recaps WHERE meeting_id=?1",
        [id],
        |r| {
            Ok(Recap {
                content: r.get(0)?,
                status: r.get(1)?,
                error: r.get(2)?,
                updated_at: r.get(3)?,
            })
        },
    )
    .optional()?)
}
pub fn begin_recap(c: &Connection, id: &str) -> Result<()> {
    if c.execute(
        "UPDATE meeting_recaps SET status='pending',error='',updated_at=datetime('now') WHERE meeting_id=?1",
        [id],
    )? == 0 {
        c.execute("INSERT INTO meeting_recaps(meeting_id) VALUES(?1)", [id])?;
    }
    Ok(())
}
pub fn finish_recap(
    c: &Connection,
    id: &str,
    result: std::result::Result<&str, &str>,
) -> Result<()> {
    let failed = result.is_err();
    let (content, error) = match result {
        Ok(body) => (body, ""),
        Err(reason) => ("", reason),
    };
    c.execute(
        "UPDATE meeting_recaps SET content=?2,error=?3,status=CASE WHEN ?4 THEN 'failed' ELSE 'ready' END,updated_at=datetime('now') WHERE meeting_id=?1",
        params![id, content, error, failed],
    )?;
    Ok(())
}

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineRecord {
    pub id: String,
    pub who: String,
    pub speaker: Option<i64>,
    pub text: String,
    pub start_time: f64,
    pub end_time: f64,
}
pub fn start(
    c: &Connection,
    title: &str,
    context: &str,
    language: &str,
    series_id: Option<&str>,
    mode: MeetingMode,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    c.execute(
        "INSERT INTO meetings(id,title,context,language,series_id,mode) VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, title, context, language, series_id, mode.as_str()],
    )?;
    Ok(id)
}
pub fn mode(c: &Connection, id: &str) -> MeetingMode {
    c.query_row("SELECT mode FROM meetings WHERE id=?1", [id], |r| {
        r.get::<_, String>(0)
    })
    .ok()
    .and_then(|s| MeetingMode::parse(&s))
    .unwrap_or_default()
}
pub fn add_line(
    c: &Connection,
    meeting_id: &str,
    who: &str,
    speaker: Option<i64>,
    text: &str,
    start_time: f64,
    end_time: f64,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    c.execute("INSERT INTO meeting_lines(id,meeting_id,who,speaker,text,start_time,end_time) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,meeting_id,who,speaker,text,start_time,end_time])?;
    Ok(id)
}
/// One matching line with its meeting.
pub struct LineHit {
    pub meeting: String,
    pub title: String,
    pub started_at: String,
    pub id: String,
    pub who: String,
    pub speaker: Option<i64>,
    pub text: String,
    pub start_time: f64,
}
pub fn search_lines(
    c: &Connection,
    query: &str,
    meeting: Option<&str>,
    limit: usize,
) -> Result<Vec<LineHit>> {
    let mut s = c.prepare(
        "SELECT l.meeting_id,m.title,m.started_at,l.id,l.who,l.speaker,l.text,l.start_time
         FROM meeting_lines l JOIN meetings m ON m.id=l.meeting_id
         WHERE instr(lower(l.text),lower(?1))>0 AND (?2 IS NULL OR l.meeting_id=?2)
         ORDER BY m.started_at DESC,l.start_time LIMIT ?3",
    )?;
    Ok(
        s.query_map(params![query, meeting, limit.min(500) as i64], |r| {
            Ok(LineHit {
                meeting: r.get(0)?,
                title: r.get(1)?,
                started_at: r.get(2)?,
                id: r.get(3)?,
                who: r.get(4)?,
                speaker: r.get(5)?,
                text: r.get(6)?,
                start_time: r.get(7)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?,
    )
}
pub fn finish(c: &Connection, meeting_id: &str, duration_seconds: i64) -> Result<()> {
    c.execute(
        "UPDATE meetings SET ended_at=datetime('now'),duration_seconds=?2 WHERE id=?1",
        params![meeting_id, duration_seconds],
    )?;
    Ok(())
}
pub fn reopen(c: &Connection, id: &str) -> Result<()> {
    c.query_row(
        "UPDATE meetings SET ended_at=NULL WHERE id=?1 RETURNING id",
        [id],
        |_| Ok(()),
    )
    .optional()?
    .ok_or(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
}
pub fn rename(c: &Connection, meeting_id: &str, title: &str) -> Result<()> {
    c.execute(
        "UPDATE meetings SET title=?2 WHERE id=?1",
        params![meeting_id, title],
    )?;
    Ok(())
}
pub fn untitled(c: &Connection, limit: usize) -> Result<Vec<(String, String)>> {
    let mut statement = c.prepare(
        "WITH newest AS (SELECT id,started_at,rowid AS ordinal FROM meetings WHERE instr(title,'Session · ')=1 ORDER BY started_at DESC,rowid DESC LIMIT ?1)
         SELECT id FROM newest WHERE (SELECT count(*) FROM meeting_lines WHERE meeting_id=newest.id)>=2 ORDER BY started_at DESC,ordinal DESC",
    )?;
    statement
        .query_map([limit as i64], |row| row.get::<_, String>(0))?
        .map(|found| {
            let id = found?;
            let names = speaker_names(c, &id)?;
            Ok((id.clone(), transcript_excerpt(c, &id, 6000, &names)?))
        })
        .collect()
}
pub fn delete(c: &Connection, meeting_id: &str) -> Result<()> {
    let tx = c.unchecked_transaction()?;
    tx.execute("DELETE FROM meetings WHERE id=?1", [meeting_id])?;
    tx.execute("DELETE FROM notes_fts WHERE meeting_id=?1", [meeting_id])?;
    tx.commit()?;
    Ok(())
}
pub fn list(c: &Connection, limit: usize) -> Result<Vec<MeetingRecord>> {
    search(c, "", limit)
}
pub fn get(c: &Connection, id: &str) -> Result<Option<MeetingRecord>> {
    Ok(c.query_row(
        "SELECT m.id,m.title,m.context,m.language,m.started_at,m.ended_at,m.duration_seconds,
           (SELECT count(*) FROM meeting_lines l WHERE l.meeting_id=m.id)
         FROM meetings m WHERE m.id=?1",
        [id],
        |r| {
            Ok(MeetingRecord {
                id: r.get(0)?,
                title: r.get(1)?,
                context: r.get(2)?,
                language: r.get(3)?,
                started_at: r.get(4)?,
                ended_at: r.get(5)?,
                duration_seconds: r.get(6)?,
                line_count: r.get(7)?,
            })
        },
    )
    .optional()?)
}
pub fn search(c: &Connection, query: &str, limit: usize) -> Result<Vec<MeetingRecord>> {
    let mut s=c.prepare(
        "SELECT m.id,m.title,m.context,m.language,m.started_at,m.ended_at,m.duration_seconds,
           (SELECT count(*) FROM meeting_lines l WHERE l.meeting_id=m.id)
         FROM meetings m
         WHERE ?1='' OR instr(lower(m.title),lower(?1))>0 OR instr(lower(m.context),lower(?1))>0
           OR EXISTS(SELECT 1 FROM meeting_lines l WHERE l.meeting_id=m.id AND instr(lower(l.text),lower(?1))>0)
           OR m.id IN (SELECT meeting_id FROM notes_fts WHERE notes_fts MATCH ?3)
         ORDER BY m.started_at DESC,m.rowid DESC LIMIT ?2")?;
    let q = query.trim();
    let fts = format!("\"{}\"", q.replace('"', "\"\""));
    Ok(s.query_map(params![q, limit.min(500) as i64, fts], |r| {
        Ok(MeetingRecord {
            id: r.get(0)?,
            title: r.get(1)?,
            context: r.get(2)?,
            language: r.get(3)?,
            started_at: r.get(4)?,
            ended_at: r.get(5)?,
            duration_seconds: r.get(6)?,
            line_count: r.get(7)?,
        })
    })?
    .collect::<std::result::Result<_, _>>()?)
}
pub fn index_notes(c: &Connection, id: &str, body: &str) -> Result<()> {
    let tx = c.unchecked_transaction()?;
    tx.execute("DELETE FROM notes_fts WHERE meeting_id=?1", [id])?;
    tx.execute(
        "INSERT INTO notes_fts(meeting_id,body) VALUES(?1,?2)",
        params![id, body],
    )?;
    tx.commit()?;
    Ok(())
}
pub fn lines(c: &Connection, meeting_id: &str) -> Result<Vec<LineRecord>> {
    let mut s=c.prepare("SELECT id,who,speaker,text,start_time,end_time FROM meeting_lines WHERE meeting_id=?1 ORDER BY start_time,rowid")?;
    Ok(s.query_map([meeting_id], |r| {
        Ok(LineRecord {
            id: r.get(0)?,
            who: r.get(1)?,
            speaker: r.get(2)?,
            text: r.get(3)?,
            start_time: r.get(4)?,
            end_time: r.get(5)?,
        })
    })?
    .collect::<std::result::Result<_, _>>()?)
}
pub fn recent_lines(c: &Connection, meeting_id: &str, limit: usize) -> Result<Vec<LineRecord>> {
    let mut s=c.prepare("SELECT id,who,speaker,substr(text,1,8000),start_time,end_time FROM (SELECT rowid,id,who,speaker,text,start_time,end_time FROM meeting_lines WHERE meeting_id=?1 ORDER BY start_time DESC,rowid DESC LIMIT ?2) ORDER BY start_time,rowid")?;
    Ok(s.query_map(params![meeting_id, limit as i64], |r| {
        Ok(LineRecord {
            id: r.get(0)?,
            who: r.get(1)?,
            speaker: r.get(2)?,
            text: r.get(3)?,
            start_time: r.get(4)?,
            end_time: r.get(5)?,
        })
    })?
    .collect::<std::result::Result<_, _>>()?)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NoteChunk {
    pub t0: f64,
    pub t1: f64,
    pub last_line: i64,
    pub body: String,
    pub created_at: String,
}
pub fn add_chunk(c: &Connection, meeting_id: &str, chunk: &NoteChunk) -> Result<()> {
    c.execute(
        "INSERT INTO meeting_note_chunks(meeting_id,t0,t1,last_line,body) VALUES(?1,?2,?3,?4,?5)",
        params![meeting_id, chunk.t0, chunk.t1, chunk.last_line, chunk.body],
    )?;
    Ok(())
}
pub fn chunks(c: &Connection, meeting_id: &str) -> Result<Vec<NoteChunk>> {
    let mut s = c.prepare(
        "SELECT t0,t1,last_line,body,created_at FROM meeting_note_chunks WHERE meeting_id=?1 ORDER BY id",
    )?;
    Ok(s.query_map([meeting_id], |r| {
        Ok(NoteChunk {
            t0: r.get(0)?,
            t1: r.get(1)?,
            last_line: r.get(2)?,
            body: r.get(3)?,
            created_at: r.get(4)?,
        })
    })?
    .collect::<std::result::Result<_, _>>()?)
}
pub fn lines_after(
    c: &Connection,
    meeting_id: &str,
    last_line: i64,
) -> Result<Vec<(i64, LineRecord)>> {
    let mut s=c.prepare("SELECT rowid,id,who,speaker,text,start_time,end_time FROM meeting_lines WHERE meeting_id=?1 AND rowid>?2 ORDER BY rowid")?;
    Ok(s.query_map(params![meeting_id, last_line], |r| {
        Ok((
            r.get(0)?,
            LineRecord {
                id: r.get(1)?,
                who: r.get(2)?,
                speaker: r.get(3)?,
                text: r.get(4)?,
                start_time: r.get(5)?,
                end_time: r.get(6)?,
            },
        ))
    })?
    .collect::<std::result::Result<_, _>>()?)
}
pub fn label(names: &BTreeMap<String, String>, who: &str, speaker: Option<i64>) -> String {
    match who {
        "me" => "You".into(),
        "room" => "Someone in the room".into(),
        _ => match speaker {
            Some(n) => names
                .get(&n.to_string())
                .cloned()
                .unwrap_or_else(|| format!("Speaker {}", n + 1)),
            None => names.get("them").cloned().unwrap_or_else(|| "Them".into()),
        },
    }
}
pub fn format_lines(lines: &[LineRecord], names: &BTreeMap<String, String>) -> String {
    lines
        .iter()
        .map(|l| format!("{}: {}\n", label(names, &l.who, l.speaker), l.text))
        .collect()
}

pub fn transcript_excerpt(
    c: &Connection,
    meeting_id: &str,
    max_chars: usize,
    names: &BTreeMap<String, String>,
) -> Result<String> {
    let mut statement = c.prepare(
        "SELECT who,speaker,text FROM meeting_lines WHERE meeting_id=?1 ORDER BY start_time,rowid",
    )?;
    let lines = statement
        .query_map([meeting_id], |r| {
            let (who, speaker, text): (String, Option<i64>, String) =
                (r.get(0)?, r.get(1)?, r.get(2)?);
            Ok(format!("{}: {text}\n", label(names, &who, speaker)))
        })?
        .collect::<std::result::Result<Vec<String>, _>>()?;
    let total: usize = lines.iter().map(|l| l.chars().count()).sum();
    if total <= max_chars {
        return Ok(lines.concat());
    }
    let half = max_chars / 2;
    let head: String = lines.iter().flat_map(|l| l.chars()).take(half).collect();
    let tail: String = lines
        .iter()
        .flat_map(|l| l.chars())
        .skip(total - half)
        .collect();
    Ok(format!(
        "{head}\n[Middle omitted; excerpts from the start and end]\n{tail}"
    ))
}
pub fn name_speaker(c: &Connection, meeting_id: &str, key: &str, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        c.execute(
            "DELETE FROM meeting_speakers WHERE meeting_id=?1 AND speaker_key=?2",
            params![meeting_id, key],
        )?;
        return Ok(());
    }
    c.execute(
        "INSERT INTO meeting_speakers(meeting_id,speaker_key,name,updated_at)
         VALUES(?1,?2,?3,strftime('%Y-%m-%d %H:%M:%f','now'))
         ON CONFLICT(meeting_id,speaker_key) DO UPDATE SET
           name=excluded.name,
           updated_at=excluded.updated_at",
        params![meeting_id, key, name],
    )?;
    Ok(())
}
pub fn renamed_at(c: &Connection, meeting_id: &str) -> Result<Option<String>> {
    Ok(c.query_row(
        "SELECT MAX(updated_at) FROM meeting_speakers WHERE meeting_id=?1",
        [meeting_id],
        |r| r.get(0),
    )?)
}
pub fn speaker_names(c: &Connection, meeting_id: &str) -> Result<BTreeMap<String, String>> {
    let mut s = c.prepare("SELECT speaker_key,name FROM meeting_speakers WHERE meeting_id=?1")?;
    Ok(s.query_map([meeting_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?)
}
pub fn format_duration(seconds: i64) -> String {
    let s = seconds.max(0);
    format!("{:02}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn archive_search_and_lifecycle() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let id = start(
            c,
            "Orchid review",
            "Saffron agenda",
            "en",
            None,
            Default::default(),
        )
        .unwrap();
        add_line(c, &id, "me", None, "Piñata %_ shipment", 4.0, 5.0).unwrap();
        add_line(c, &id, "them", Some(1), "Earlier", 1.0, 2.0).unwrap();
        let other = start(c, "Other", "", "en", None, Default::default()).unwrap();
        for q in ["ORCHID", "saffron", "%_", "Piñata"] {
            let found = search(c, q, 10).unwrap();
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].id, id);
        }
        assert!(search(c, "%_missing", 10).unwrap().is_empty());
        assert_eq!(lines(c, &id).unwrap()[0].text, "Earlier");
        assert_eq!(
            list(c, 10)
                .unwrap()
                .iter()
                .find(|m| m.id == id)
                .unwrap()
                .line_count,
            2
        );
        assert!(
            list(c, 10)
                .unwrap()
                .iter()
                .find(|m| m.id == id)
                .unwrap()
                .ended_at
                .is_none()
        );
        finish(c, &id, 75).unwrap();
        let record = list(c, 10)
            .unwrap()
            .into_iter()
            .find(|m| m.id == id)
            .unwrap();
        assert_eq!(record.duration_seconds, 75);
        assert!(record.ended_at.is_some());
        reopen(c, &id).unwrap();
        assert!(
            list(c, 10)
                .unwrap()
                .iter()
                .find(|m| m.id == id)
                .unwrap()
                .ended_at
                .is_none()
        );
        rename(c, &id, "Renamed").unwrap();
        assert_eq!(search(c, "Renamed", 10).unwrap()[0].id, id);
        name_speaker(c, &id, "two", "Ada").unwrap();
        name_speaker(c, &id, "two", "Bea").unwrap();
        assert_eq!(speaker_names(c, &id).unwrap()["two"], "Bea");
        index_notes(c, &id, "zebra approval").unwrap();
        assert_eq!(search(c, "zebra", 10).unwrap()[0].id, id);
        for q in ["'", "+", ")", "NEAR / *"] {
            search(c, q, 10).unwrap();
        }
        begin_recap(c, &id).unwrap();
        finish_recap(c, &id, Ok("ready text")).unwrap();
        begin_recap(c, &id).unwrap();
        assert_eq!(recap(c, &id).unwrap().unwrap().content, "ready text");
        finish_recap(c, &id, Err("provider failed")).unwrap();
        assert_eq!(recap(c, &id).unwrap().unwrap().status, "failed");
        delete(c, &id).unwrap();
        assert!(lines(c, &id).unwrap().is_empty());
        assert!(search(c, "zebra", 10).unwrap().is_empty());
        assert_eq!(list(c, 10).unwrap()[0].id, other);
        finish_recap(c, &id, Ok("late")).unwrap();
        assert!(recap(c, &id).unwrap().is_none());
    }

    #[test]
    fn bounded_transcript_and_chunks() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let id = start(c, "Session · 09:00", "", "en", None, Default::default()).unwrap();
        for n in 0..1000 {
            let text = format!("line-{n} {}", "界".repeat(500));
            add_line(c, &id, "them", Some(0), &text, n as f64, n as f64 + 1.0).unwrap();
        }
        let excerpt = transcript_excerpt(c, &id, 6000, &BTreeMap::new()).unwrap();
        assert!(excerpt.contains("line-0") && excerpt.contains("line-999"));
        assert!(excerpt.contains("Middle omitted"));
        assert!(!excerpt.contains("line-500"));
        assert!(
            excerpt.chars().count()
                <= 6000
                    + "\n[Middle omitted; excerpts from the start and end]\n"
                        .chars()
                        .count()
        );
        assert_eq!(recent_lines(c, &id, 3).unwrap().len(), 3);
        assert!(
            recent_lines(c, &id, 3).unwrap()[0]
                .text
                .starts_with("line-997")
        );
        assert_eq!(lines(c, &id).unwrap().len(), 1000);
        assert_eq!(untitled(c, 1).unwrap().len(), 1);
        add_chunk(
            c,
            &id,
            &NoteChunk {
                t0: 0.0,
                t1: 1.0,
                last_line: 1,
                body: "summary".into(),
                created_at: String::new(),
            },
        )
        .unwrap();
        let saved_chunks = chunks(c, &id).unwrap();
        assert_eq!(saved_chunks.len(), 1);
        assert!(!saved_chunks[0].created_at.is_empty());
        assert_eq!(lines_after(c, &id, 1).unwrap().len(), 999);
        delete(c, &id).unwrap();
        assert!(chunks(c, &id).unwrap().is_empty());
    }

    #[test]
    fn duration_format() {
        assert_eq!(format_duration(0), "00:00");
        assert_eq!(format_duration(63), "01:03");
        assert_eq!(format_duration(3700), "61:40");
        assert_eq!(format_duration(-42), "00:00");
    }

    #[test]
    fn notes_search_finds_only_the_indexed_meeting_and_delete_removes_it() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let older = start(c, "Old", "", "en", None, Default::default()).unwrap();
        let newer = start(c, "New", "", "en", None, Default::default()).unwrap();
        index_notes(c, &older, "maple decision").unwrap();
        index_notes(c, &newer, "cedar decision").unwrap();
        let found = search(c, "cedar", 10).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, newer);
        for query in ["'", "+", ")", "NEAR / *"] {
            search(c, query, 10).unwrap();
        }
        delete(c, &newer).unwrap();
        assert!(search(c, "cedar", 10).unwrap().is_empty());
        assert_eq!(search(c, "maple", 10).unwrap()[0].id, older);
    }

    #[test]
    fn recent_lines_keep_the_last_three_in_time_order() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let id = start(c, "Call", "", "en", None, Default::default()).unwrap();
        for n in [4, 1, 3, 2] {
            add_line(
                c,
                &id,
                "them",
                None,
                &format!("utterance {n}"),
                n as f64,
                n as f64,
            )
            .unwrap();
        }
        let recent = recent_lines(c, &id, 3).unwrap();
        assert_eq!(
            recent
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["utterance 2", "utterance 3", "utterance 4"]
        );
        assert_eq!(lines(c, &id).unwrap().len(), 4);
    }

    #[test]
    fn renamed_meeting_and_replaced_speaker_are_visible() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let id = start(c, "Before", "", "en", None, Default::default()).unwrap();
        rename(c, &id, "After").unwrap();
        let listed = list(c, 10).unwrap();
        assert_eq!(listed[0].title, "After");
        name_speaker(c, &id, "speaker-a", "Mira").unwrap();
        let first_ts = renamed_at(c, &id).unwrap().unwrap();
        name_speaker(c, &id, "speaker-a", "Nora").unwrap();
        let second_ts = renamed_at(c, &id).unwrap().unwrap();
        assert!(second_ts >= first_ts);
        let names = speaker_names(c, &id).unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names["speaker-a"], "Nora");
        name_speaker(c, &id, "speaker-a", "  ").unwrap();
        assert!(speaker_names(c, &id).unwrap().is_empty());
        assert_eq!(renamed_at(c, &id).unwrap(), None);
    }

    #[test]
    fn untitled_limit_applies_before_skipping_short_transcripts() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let ready = start(c, "Session · 10:00", "", "en", None, Default::default()).unwrap();
        add_line(c, &ready, "me", None, "first", 0.0, 1.0).unwrap();
        add_line(c, &ready, "them", None, "second", 1.0, 2.0).unwrap();
        let short = start(c, "Session · 10:01", "", "en", None, Default::default()).unwrap();
        add_line(c, &short, "me", None, "only", 0.0, 1.0).unwrap();
        assert!(untitled(c, 1).unwrap().is_empty());
        let eligible = untitled(c, 2).unwrap();
        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].0, ready);
        assert_eq!(eligible[0].1, "You: first\nThem: second\n");
    }

    #[test]
    fn recap_retries_keep_previous_content_until_a_new_result() {
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let id = start(c, "Review", "", "en", None, Default::default()).unwrap();
        assert!(recap(c, &id).unwrap().is_none());
        begin_recap(c, &id).unwrap();
        assert_eq!(recap(c, &id).unwrap().unwrap().status, "pending");
        finish_recap(c, &id, Ok("first pass")).unwrap();
        begin_recap(c, &id).unwrap();
        let r = recap(c, &id).unwrap().unwrap();
        assert!(!r.updated_at.is_empty());
        assert_eq!(
            (r.content.as_str(), r.status.as_str(), r.error.as_str()),
            ("first pass", "pending", "")
        );
        finish_recap(c, &id, Err("timeout")).unwrap();
        let r = recap(c, &id).unwrap().unwrap();
        assert_eq!(
            (r.content.as_str(), r.status.as_str(), r.error.as_str()),
            ("", "failed", "timeout")
        );
        finish_recap(c, "absent", Ok("late")).unwrap();
        assert!(recap(c, "absent").unwrap().is_none());
        finish_recap(c, &id, Err("")).unwrap();
        assert_eq!(recap(c, &id).unwrap().unwrap().status, "failed");
    }

    #[test]
    fn format_lines_uses_app_speaker_labels() {
        let utterances = [
            LineRecord {
                id: "1".into(),
                who: "me".into(),
                speaker: Some(9),
                text: "hello".into(),
                start_time: 0.0,
                end_time: 1.0,
            },
            LineRecord {
                id: "2".into(),
                who: "other".into(),
                speaker: None,
                text: "welcome".into(),
                start_time: 1.0,
                end_time: 2.0,
            },
            LineRecord {
                id: "3".into(),
                who: "other".into(),
                speaker: Some(2),
                text: "next".into(),
                start_time: 2.0,
                end_time: 3.0,
            },
            LineRecord {
                id: "4".into(),
                who: "room".into(),
                speaker: Some(0),
                text: "passing by".into(),
                start_time: 3.0,
                end_time: 4.0,
            },
        ];
        assert_eq!(
            format_lines(&utterances, &BTreeMap::new()),
            "You: hello\nThem: welcome\nSpeaker 3: next\nSomeone in the room: passing by\n"
        );
        let mut names = BTreeMap::new();
        names.insert("2".into(), "Ada".into());
        names.insert("them".into(), "Panel".into());
        assert_eq!(label(&names, "me", Some(2)), "You");
        assert_eq!(label(&names, "room", Some(2)), "Someone in the room");
        assert_eq!(label(&names, "them", Some(2)), "Ada");
        assert_eq!(label(&names, "them", Some(0)), "Speaker 1");
        assert_eq!(label(&names, "them", None), "Panel");
        assert_eq!(label(&BTreeMap::new(), "them", None), "Them");
        assert_eq!(
            format_lines(&utterances, &names),
            "You: hello\nPanel: welcome\nAda: next\nSomeone in the room: passing by\n"
        );
        let database = Database::in_memory().unwrap();
        let c = database.connection();
        let id = start(c, "Named", "", "en", None, Default::default()).unwrap();
        add_line(c, &id, "them", Some(2), "from ada", 0.0, 1.0).unwrap();
        add_line(c, &id, "them", None, "from panel", 1.0, 2.0).unwrap();
        add_line(c, &id, "them", Some(0), "from one", 2.0, 3.0).unwrap();
        assert_eq!(
            transcript_excerpt(c, &id, 6000, &names).unwrap(),
            "Ada: from ada\nPanel: from panel\nSpeaker 1: from one\n"
        );
    }
}
