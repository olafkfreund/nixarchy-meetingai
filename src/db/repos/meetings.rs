use crate::core::settings::MeetingMode;
    series_id: Option<&str>,
    mode: MeetingMode,
        "INSERT INTO meetings (id, title, context, language, series_id, mode) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, title, context, language, series_id, mode.as_str()],
/// The mode a meeting was recorded in; unknown or missing is General.
pub fn mode(connection: &Connection, id: &str) -> MeetingMode {
    connection
        .query_row(
            "SELECT mode FROM meetings WHERE id = ?1",
            params![id],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|m| MeetingMode::parse(&m))
        .unwrap_or_default()
}

    let tx = connection.unchecked_transaction()?;
    tx.execute("DELETE FROM meetings WHERE id = ?1", params![meeting_id])?;
    tx.execute(
        "DELETE FROM notes_fts WHERE meeting_id = ?1",
        params![meeting_id],
    )?;
    Ok(tx.commit()?)
            OR m.id IN (SELECT meeting_id FROM notes_fts WHERE notes_fts MATCH ?3)
    let query = query.trim();
    // One quoted FTS5 string, so user text can never be FTS syntax.
    let fts = format!("\"{}\"", query.replace('"', "\"\""));
    let rows = statement.query_map(params![limit.min(500) as i64, query, fts], |row| {
/// Make a meeting's recap searchable, replacing any earlier text.
pub fn index_notes(connection: &Connection, id: &str, body: &str) -> Result<()> {
    connection.execute("DELETE FROM notes_fts WHERE meeting_id = ?1", params![id])?;
    connection.execute(
        "INSERT INTO notes_fts (meeting_id, body) VALUES (?1, ?2)",
        params![id, body],
    )?;
    Ok(())
}

/// One rolling-notes summary covering transcript time `t0..t1` and every line up to `last_line`.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteChunk {
    pub t0: f64,
    pub t1: f64,
    /// Highest line rowid the summary covers (arrival order, not time).
    pub last_line: i64,
    pub body: String,
}

/// Save a summary of a slice of final lines.
pub fn add_chunk(connection: &Connection, meeting_id: &str, chunk: &NoteChunk) -> Result<()> {
    connection.execute(
        "INSERT INTO meeting_note_chunks (meeting_id, t0, t1, last_line, body) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![meeting_id, chunk.t0, chunk.t1, chunk.last_line, chunk.body],
    )?;
    Ok(())
}

/// Rolling-notes summaries in time order.
pub fn chunks(connection: &Connection, meeting_id: &str) -> Result<Vec<NoteChunk>> {
    let mut statement = connection.prepare(
        "SELECT t0, t1, last_line, body FROM meeting_note_chunks WHERE meeting_id = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map(params![meeting_id], |r| {
        Ok(NoteChunk {
            t0: r.get(0)?,
            t1: r.get(1)?,
            last_line: r.get(2)?,
            body: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Lines stored after rowid `last_line`, in arrival order, with their rowids: what no chunk
/// covers yet. Arrival order, not time, so a late line with an early end time is never skipped.
pub fn lines_after(
    connection: &Connection,
    meeting_id: &str,
    last_line: i64,
) -> Result<Vec<(i64, LineRecord)>> {
    let mut statement = connection.prepare(
        "SELECT rowid, id, who, speaker, text, start_time, end_time FROM meeting_lines
         WHERE meeting_id = ?1 AND rowid > ?2
         ORDER BY rowid ASC",
    )?;
    let rows = statement.query_map(params![meeting_id, last_line], |row| {
        Ok((
            row.get(0)?,
            LineRecord {
                id: row.get(1)?,
                who: row.get(2)?,
                speaker: row.get(3)?,
                text: row.get(4)?,
                start_time: row.get(5)?,
                end_time: row.get(6)?,
            },
        ))
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// `Label: text` per line, the same labels as `transcript_excerpt`.
pub fn format_lines(lines: &[LineRecord]) -> String {
    lines
        .iter()
        .map(|l| {
            let label = if l.who == "me" {
                "You".into()
            } else {
                l.speaker
                    .map_or("Them".into(), |s| format!("Speaker {}", s + 1))
            };
            format!("{label}: {}\n", l.text)
        })
        .collect()
}

        let id = start(
            c,
            "Design review",
            "Orion context",
            "en",
            None,
            Default::default(),
        )
        .unwrap();
        start(c, "Other", "auto", "en", None, Default::default()).unwrap();
        // Recap text is searchable; quotes, wildcards and operators are literal.
        index_notes(c, &id, "Decided to adopt \"Kubernetes\" - 50%_ faster").unwrap();
        index_notes(c, &id, "We adopted Kubernetes").unwrap();
        assert_eq!(search(c, "kubernetes", 200).unwrap()[0].id, id);
        for query in ["\"", "50%_", "-", "a \"b\" - AND OR NOT *", "("] {
            search(c, query, 200).unwrap();
        }
        delete(c, &id).unwrap();
        assert!(search(c, "kubernetes", 200).unwrap().is_empty());
    use crate::db::Database;
        let id = start(
            db.connection(),
            "Long meeting",
            "auto",
            "en",
            None,
            Default::default(),
        )
        .unwrap();
        let id = start(
            connection,
            "Standup",
            "meeting",
            "en",
            None,
            Default::default(),
        )
        .unwrap();
        let id = start(
            connection,
            "Call",
            "meeting",
            "en",
            None,
            Default::default(),
        )
        .unwrap();
        let id = start(
            connection,
            "Call",
            "meeting",
            "en",
            None,
            Default::default(),
        )
        .unwrap();
        let id = start(
            connection,
            "Call",
            "meeting",
            "en",
            None,
            Default::default(),
        )
        .unwrap();
        let id = start(
            connection,
            "Call",
            "meeting",
            "en",
            None,
            Default::default(),
        )
        .unwrap();
        let id = start(
            connection,
            "Untitled",
            "meeting",
            "en",
            None,
            Default::default(),
        )
        .unwrap();

    #[test]
    fn chunks_cover_lines_and_leave_the_rest() {
        let db = database();
        let c = db.connection();
        let id = start(c, "M", "", "en", None, Default::default()).unwrap();
        add_line(c, &id, "me", None, "one", 0.0, 1.0).unwrap();
        add_line(c, &id, "them", None, "two", 1.0, 2.0).unwrap();
        add_chunk(
            c,
            &id,
            &NoteChunk {
                t0: 0.0,
                t1: 1.0,
                last_line: 1,
                body: "n".into(),
            },
        )
        .unwrap();
        assert_eq!(chunks(c, &id).unwrap().len(), 1);
        let rest: Vec<_> = lines_after(c, &id, 1)
            .unwrap()
            .into_iter()
            .map(|p| p.1)
            .collect();
        assert_eq!(format_lines(&rest), "Them: two\n");
        delete(c, &id).unwrap();
        assert!(chunks(c, &id).unwrap().is_empty());
    }
