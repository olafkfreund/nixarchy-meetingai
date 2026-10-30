//! The most recent folder used to export each meeting.

use rusqlite::{Connection, OptionalExtension, params};

use crate::db::Result;

pub fn get(connection: &Connection, meeting_id: &str) -> Result<Option<String>> {
    Ok(connection
        .query_row(
            "SELECT dir FROM meeting_exports WHERE meeting_id = ?1",
            [meeting_id],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn set(connection: &Connection, meeting_id: &str, dir: &str) -> Result<()> {
    connection.execute(
        "INSERT INTO meeting_exports (meeting_id, dir) VALUES (?1, ?2)
         ON CONFLICT(meeting_id) DO UPDATE SET dir = excluded.dir",
        params![meeting_id, dir],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, meetings};

    #[test]
    fn round_trip_updates_and_cascades_on_delete() {
        let db = Database::in_memory().unwrap();
        let connection = db.connection();
        let meeting =
            meetings::start(connection, "Call", "", "en", None, Default::default()).unwrap();
        assert_eq!(get(connection, &meeting).unwrap(), None);
        set(connection, &meeting, "/first").unwrap();
        assert_eq!(
            get(connection, &meeting).unwrap().as_deref(),
            Some("/first")
        );
        set(connection, &meeting, "/second").unwrap();
        assert_eq!(
            get(connection, &meeting).unwrap().as_deref(),
            Some("/second")
        );
        meetings::delete(connection, &meeting).unwrap();
        assert_eq!(get(connection, &meeting).unwrap(), None);
    }
}
