pub use repos::items;
        // One writer at a time: concurrent first opens (daemon, MCP, tests) otherwise both
        // see a column missing and both add it ("duplicate column name").
        connection.execute_batch("BEGIN IMMEDIATE")?;
        let upgraded = (|| -> Result<()> {
            // Databases made before coverage was tracked by line id lack this column.
            let has_last_line: bool = connection.query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('meeting_note_chunks') WHERE name = 'last_line'",
                [],
                |r| r.get(0),
            )?;
            if !has_last_line {
                connection.execute_batch(
                    "ALTER TABLE meeting_note_chunks ADD COLUMN last_line INTEGER NOT NULL DEFAULT 0",
                )?;
            }
            let has_series: bool = connection.query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('meetings') WHERE name = 'series_id'",
                [],
                |r| r.get(0),
            )?;
            if !has_series {
                connection.execute_batch("ALTER TABLE meetings ADD COLUMN series_id TEXT")?;
            }
            let has_mode: bool = connection.query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('meetings') WHERE name = 'mode'",
                [],
                |r| r.get(0),
            )?;
            if !has_mode {
                connection.execute_batch(
                    "ALTER TABLE meetings ADD COLUMN mode TEXT NOT NULL DEFAULT 'general'",
                )?;
            }
            // Recaps written before notes_fts existed become searchable, once.
            let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if version < 1 {
                connection.execute_batch(
                    "INSERT INTO notes_fts (meeting_id, body)
                     SELECT meeting_id, content FROM meeting_recaps r
                     WHERE status = 'ready' AND content != ''
                       AND NOT EXISTS (SELECT 1 FROM notes_fts f WHERE f.meeting_id = r.meeting_id);
                     PRAGMA user_version = 1;",
                )?;
            }
            Ok(())
        })();
        match upgraded {
            Ok(()) => connection.execute_batch("COMMIT")?,
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                return Err(error);
            }
        }
    #[test]
    fn series_column_is_added_to_an_old_database() {
        let dir = std::env::temp_dir().join(format!("nm-old-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("h.db");
        drop(Database::open(&path).unwrap());
        {
            let db = Database::open(&path).unwrap();
            db.connection()
                .execute_batch("ALTER TABLE meetings DROP COLUMN series_id")
                .unwrap();
        }
        let db = Database::open(&path).unwrap();
        meetings::start(
            db.connection(),
            "t",
            "",
            "en",
            Some("s"),
            Default::default(),
        )
        .unwrap();
    }

    #[test]
    fn old_recaps_are_indexed_on_open() {
        let dir = std::env::temp_dir().join(format!("nm-bf-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("h.db");
        let db = Database::open(&path).unwrap();
        let c = db.connection();
        let id = meetings::start(c, "t", "", "en", None, Default::default()).unwrap();
        meetings::begin_recap(c, &id).unwrap();
        meetings::finish_recap(c, &id, Ok("ship the quokka release")).unwrap();
        assert!(meetings::search(c, "quokka", 10).unwrap().is_empty());
        // Opened by a database from before the backfill marker existed.
        c.execute_batch("PRAGMA user_version = 0").unwrap();
        drop(db);
        let db = Database::open(&path).unwrap();
        assert_eq!(
            meetings::search(db.connection(), "quokka", 10)
                .unwrap()
                .len(),
            1
        );
        drop(db);
        // A second open does not duplicate the row.
        let db = Database::open(&path).unwrap();
        let n: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM notes_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        // The backfill ran once: a row deleted now is not brought back by the next open.
        db.connection()
            .execute("DELETE FROM notes_fts", [])
            .unwrap();
        drop(db);
        let db = Database::open(&path).unwrap();
        assert!(
            meetings::search(db.connection(), "quokka", 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn fts5_works_in_the_encrypted_database() {
        let dir = std::env::temp_dir().join(format!("nm-fts-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let db = Database::open(dir.join("h.db")).unwrap();
        let c = db.connection();
        let id = meetings::start(c, "t", "", "en", None, Default::default()).unwrap();
        meetings::index_notes(c, &id, "ship the zebra release").unwrap();
        assert_eq!(meetings::search(c, "zebra", 10).unwrap().len(), 1);
    }

        assert_eq!(prompts.len(), 6);
