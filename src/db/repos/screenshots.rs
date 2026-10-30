//! Screenshots attached to a meeting's chat turns.

use rusqlite::{Connection, params};

use crate::db::Result;

#[derive(Debug, Clone, PartialEq)]
pub struct Screenshot {
    pub id: i64,
    pub meeting_id: String,
    pub taken_at: f64,
    pub question: String,
    pub mime: String,
    pub data: Vec<u8>,
}

pub fn add(
    connection: &Connection,
    meeting_id: &str,
    taken_at: f64,
    question: &str,
    mime: &str,
    data: &[u8],
) -> Result<i64> {
    connection.execute(
        "INSERT INTO meeting_screenshots (meeting_id, taken_at, question, mime, data)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![meeting_id, taken_at, question, mime, data],
    )?;
    Ok(connection.last_insert_rowid())
}

pub fn list(connection: &Connection, meeting_id: &str) -> Result<Vec<Screenshot>> {
    let mut statement = connection.prepare(
        "SELECT id, meeting_id, taken_at, question, mime, data
         FROM meeting_screenshots WHERE meeting_id = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map([meeting_id], |row| {
        Ok(Screenshot {
            id: row.get(0)?,
            meeting_id: row.get(1)?,
            taken_at: row.get(2)?,
            question: row.get(3)?,
            mime: row.get(4)?,
            data: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, meetings};

    #[test]
    fn round_trip_orders_by_id_and_cascades_on_delete() {
        let db = Database::in_memory().unwrap();
        let connection = db.connection();
        let meeting =
            meetings::start(connection, "Call", "", "en", None, Default::default()).unwrap();
        let other =
            meetings::start(connection, "Other", "", "en", None, Default::default()).unwrap();
        let later = add(connection, &meeting, 8.0, "later?", "image/png", &[0, 1]).unwrap();
        let earlier = add(connection, &meeting, 2.0, "earlier?", "image/png", &[2, 3]).unwrap();
        add(connection, &other, 1.0, "other?", "image/png", &[4]).unwrap();

        let shots = list(connection, &meeting).unwrap();
        assert_eq!(
            shots.iter().map(|shot| shot.id).collect::<Vec<_>>(),
            [later, earlier]
        );
        assert_eq!(shots[0].question, "later?");
        assert_eq!(shots[1].data, [2, 3]);
        assert_eq!(shots[1].mime, "image/png");
        meetings::delete(connection, &meeting).unwrap();
        assert!(list(connection, &meeting).unwrap().is_empty());
        assert_eq!(list(connection, &other).unwrap().len(), 1);
    }
}
