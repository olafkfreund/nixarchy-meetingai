//! Series and the action/decision/question items extracted from team meetings.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Result;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesRecord {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemRecord {
    pub id: String,
    pub meeting_id: String,
    pub series_id: Option<String>,
    pub kind: String,
    pub text: String,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub status: String,
    pub source_line: Option<i64>,
    pub origin_item: Option<String>,
    pub draft: bool,
    pub created_at: String,
    /// Start time of the source line, if it is stored.
    pub t0: Option<f64>,
    /// Text of the item `origin_item` names.
    pub origin_text: Option<String>,
}

/// An item to store; it starts open and as a draft.
#[derive(Debug, Default)]
pub struct NewItem<'a> {
    pub meeting_id: &'a str,
    pub series_id: Option<&'a str>,
    pub kind: &'a str,
    pub text: &'a str,
    pub owner: Option<&'a str>,
    pub due: Option<&'a str>,
    pub source_line: Option<i64>,
    pub origin_item: Option<&'a str>,
}

/// Fields to change; `None` leaves a field alone, and an empty owner or due clears it.
#[derive(Debug, Default, Deserialize)]
pub struct ItemPatch {
    pub status: Option<String>,
    pub text: Option<String>,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub origin_item: Option<String>,
}

/// Which items to list.
pub enum Scope<'a> {
    Meeting(&'a str),
    Series(&'a str),
}

const COLUMNS: &str = "id, meeting_id, series_id, kind, text, owner, due, status, source_line, origin_item, draft, created_at,
    (SELECT start_time FROM meeting_lines WHERE meeting_lines.rowid = source_line) AS t0,
    (SELECT o.text FROM meeting_items o WHERE o.id = meeting_items.origin_item) AS origin_text";

pub fn series_create(connection: &Connection, name: &str) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO series (id, name) VALUES (?1, ?2)",
        params![id, name.trim()],
    )?;
    Ok(id)
}

pub fn series_list(connection: &Connection) -> Result<Vec<SeriesRecord>> {
    let mut statement =
        connection.prepare("SELECT id, name, created_at FROM series ORDER BY name")?;
    let rows = statement.query_map([], series_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn series_get(connection: &Connection, id: &str) -> Result<Option<SeriesRecord>> {
    Ok(connection
        .query_row(
            "SELECT id, name, created_at FROM series WHERE id = ?1",
            params![id],
            series_row,
        )
        .optional()?)
}

fn series_row(row: &rusqlite::Row) -> rusqlite::Result<SeriesRecord> {
    Ok(SeriesRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
    })
}

pub fn insert(connection: &Connection, item: &NewItem) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO meeting_items (id, meeting_id, series_id, kind, text, owner, due, source_line, origin_item)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            item.meeting_id,
            item.series_id,
            item.kind,
            item.text,
            item.owner,
            item.due,
            item.source_line,
            item.origin_item
        ],
    )?;
    Ok(id)
}

fn item_row(row: &rusqlite::Row) -> rusqlite::Result<ItemRecord> {
    Ok(ItemRecord {
        id: row.get(0)?,
        meeting_id: row.get(1)?,
        series_id: row.get(2)?,
        kind: row.get(3)?,
        text: row.get(4)?,
        owner: row.get(5)?,
        due: row.get(6)?,
        status: row.get(7)?,
        source_line: row.get(8)?,
        origin_item: row.get(9)?,
        draft: row.get::<_, i64>(10)? != 0,
        created_at: row.get(11)?,
        t0: row.get(12)?,
        origin_text: row.get(13)?,
    })
}

/// Items of a meeting or a series, oldest first, optionally only one status.
pub fn list(
    connection: &Connection,
    scope: Scope,
    status: Option<&str>,
) -> Result<Vec<ItemRecord>> {
    let (column, id) = match scope {
        Scope::Meeting(id) => ("meeting_id", id),
        Scope::Series(id) => ("series_id", id),
    };
    let mut statement = connection.prepare(&format!(
        "SELECT {COLUMNS} FROM meeting_items WHERE {column} = ?1 AND (?2 IS NULL OR status = ?2)
         ORDER BY created_at, rowid"
    ))?;
    let rows = statement.query_map(params![id, status], item_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn get(connection: &Connection, id: &str) -> Result<Option<ItemRecord>> {
    Ok(connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM meeting_items WHERE id = ?1"),
            params![id],
            item_row,
        )
        .optional()?)
}

/// The series' newest meeting as (id, title, started_at) that came before `before`, or the
/// newest of all without it.
pub fn latest_meeting(
    connection: &Connection,
    series_id: &str,
    before: Option<&str>,
) -> Result<Option<(String, String, String)>> {
    Ok(connection
        .query_row(
            "SELECT id, title, started_at FROM meetings WHERE series_id = ?1
               AND rowid < COALESCE((SELECT rowid FROM meetings WHERE id = ?2), 9223372036854775807)
             ORDER BY rowid DESC LIMIT 1",
            params![series_id, before],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?)
}

pub fn meeting_series(connection: &Connection, meeting_id: &str) -> Result<Option<String>> {
    Ok(connection
        .query_row(
            "SELECT series_id FROM meetings WHERE id = ?1",
            params![meeting_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten())
}

/// Earlier items this meeting touched: the ones its own items continue (`origin_item`).
pub fn touched(connection: &Connection, meeting_id: &str) -> Result<Vec<ItemRecord>> {
    let mut statement = connection.prepare(&format!(
        "SELECT {COLUMNS} FROM meeting_items WHERE meeting_id != ?1
           AND id IN (SELECT origin_item FROM meeting_items WHERE meeting_id = ?1)
         ORDER BY created_at, rowid"
    ))?;
    let rows = statement.query_map(params![meeting_id], item_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// A series' open items from finished (non-draft) meetings: the "earlier task" candidates.
/// Items of `current` (the meeting being chunked or resumed) never point back at themselves.
pub fn open_for_series(
    connection: &Connection,
    series_id: &str,
    current: Option<&str>,
) -> Result<Vec<ItemRecord>> {
    let mut statement = connection.prepare(&format!(
        "SELECT {COLUMNS} FROM meeting_items WHERE series_id = ?1 AND status = 'open' AND draft = 0
           AND meeting_id IS NOT ?2
         ORDER BY created_at, rowid"
    ))?;
    let rows = statement.query_map(params![series_id, current], item_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Returns false when no such item exists. A bad status fails the CHECK.
pub fn update(connection: &Connection, id: &str, patch: &ItemPatch) -> Result<bool> {
    let changed = connection.execute(
        "UPDATE meeting_items SET status = COALESCE(?2, status), text = COALESCE(?3, text),
             owner = CASE WHEN ?4 IS NULL THEN owner ELSE NULLIF(?4, '') END,
             due = CASE WHEN ?5 IS NULL THEN due ELSE NULLIF(?5, '') END,
             origin_item = CASE WHEN ?6 IS NULL THEN origin_item ELSE NULLIF(?6, '') END
         WHERE id = ?1",
        params![
            id,
            patch.status,
            patch.text,
            patch.owner,
            patch.due,
            patch.origin_item
        ],
    )?;
    Ok(changed > 0)
}

/// Make a meeting's draft items final.
pub fn finalize(connection: &Connection, meeting_id: &str) -> Result<()> {
    connection.execute(
        "UPDATE meeting_items SET draft = 0 WHERE meeting_id = ?1",
        params![meeting_id],
    )?;
    Ok(())
}

pub fn delete(connection: &Connection, id: &str) -> Result<bool> {
    Ok(connection.execute("DELETE FROM meeting_items WHERE id = ?1", params![id])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, meetings};

    #[test]
    fn series_items_crud_and_status_check() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        let s = series_create(c, " Weekly ").unwrap();
        assert!(series_create(c, "Weekly").is_err());
        assert_eq!(series_get(c, &s).unwrap().unwrap().name, "Weekly");
        assert_eq!(series_list(c).unwrap().len(), 1);
        assert!(series_get(c, "nope").unwrap().is_none());

        let m1 = meetings::start(c, "One", "meeting", "en", Some(&s), Default::default()).unwrap();
        let m2 = meetings::start(c, "Two", "meeting", "en", Some(&s), Default::default()).unwrap();
        let before = |m| latest_meeting(c, &s, Some(m)).unwrap().map(|r| r.0);
        assert_eq!(before(&m2), Some(m1.clone()));
        assert_eq!(before(&m1), None);
        assert_eq!(
            latest_meeting(c, &s, None).unwrap().map(|r| r.0),
            Some(m2.clone())
        );

        let item = NewItem {
            meeting_id: &m1,
            series_id: Some(&s),
            kind: "action",
            text: "Ship it",
            ..NewItem::default()
        };
        let id = insert(c, &item).unwrap();
        assert!(
            insert(
                c,
                &NewItem {
                    kind: "bogus",
                    ..item
                }
            )
            .is_err()
        );
        let got = &list(c, Scope::Meeting(&m1), None).unwrap()[0];
        assert!(got.draft && got.owner.is_none() && got.status == "open");
        // Drafts are not earlier-task candidates until the meeting is finished.
        assert!(open_for_series(c, &s, None).unwrap().is_empty());
        c.execute("UPDATE meeting_items SET draft = 0", []).unwrap();
        assert_eq!(open_for_series(c, &s, None).unwrap().len(), 1);
        // The meeting being chunked or resumed never offers its own items.
        assert!(open_for_series(c, &s, Some(&m1)).unwrap().is_empty());
        assert_eq!(open_for_series(c, &s, Some(&m2)).unwrap().len(), 1);

        let patch = ItemPatch {
            owner: Some("Ann".into()),
            ..ItemPatch::default()
        };
        assert!(update(c, &id, &patch).unwrap());
        let patch = ItemPatch {
            status: Some("done".into()),
            owner: Some(String::new()),
            ..ItemPatch::default()
        };
        update(c, &id, &patch).unwrap();
        let got = &list(c, Scope::Series(&s), Some("done")).unwrap()[0];
        assert!(got.owner.is_none() && got.text == "Ship it");
        assert!(list(c, Scope::Series(&s), Some("open")).unwrap().is_empty());
        let bad = ItemPatch {
            status: Some("later".into()),
            ..ItemPatch::default()
        };
        assert!(update(c, &id, &bad).is_err());
        assert!(!update(c, "nope", &ItemPatch::default()).unwrap());
        assert!(delete(c, &id).unwrap() && !delete(c, &id).unwrap());
    }

    #[test]
    fn touched_follows_new_items_and_lines_give_t0_and_mode_is_stored() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        let team = crate::core::settings::MeetingMode::TeamMeeting;
        let m1 = meetings::start(c, "One", "", "en", None, team).unwrap();
        let m2 = meetings::start(c, "Two", "", "en", None, Default::default()).unwrap();
        assert_eq!(meetings::mode(c, &m1), team);
        assert_ne!(meetings::mode(c, &m2), team);
        let old = insert(
            c,
            &NewItem {
                meeting_id: &m1,
                kind: "action",
                text: "A",
                ..NewItem::default()
            },
        )
        .unwrap();
        meetings::add_line(c, &m2, "me", None, "hi", 7.5, 8.0).unwrap();
        let rowid: i64 = c
            .query_row("SELECT rowid FROM meeting_lines", [], |r| r.get(0))
            .unwrap();
        let new = insert(
            c,
            &NewItem {
                meeting_id: &m2,
                kind: "action",
                text: "A2",
                source_line: Some(rowid),
                origin_item: Some(&old),
                ..NewItem::default()
            },
        )
        .unwrap();
        assert_eq!(touched(c, &m2).unwrap()[0].id, old);
        assert_eq!(
            get(c, &new).unwrap().unwrap().origin_text.as_deref(),
            Some("A")
        );
        assert!(touched(c, &m1).unwrap().is_empty());
        assert_eq!(get(c, &new).unwrap().unwrap().t0, Some(7.5));
        let clear = ItemPatch {
            origin_item: Some(String::new()),
            ..ItemPatch::default()
        };
        update(c, &new, &clear).unwrap();
        assert!(touched(c, &m2).unwrap().is_empty());
    }
}
