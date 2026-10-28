    use crate::db::Database;
        set(connection, "reduce_motion", "false").unwrap();
        set(connection, "reduce_motion", "true").unwrap();
            get(connection, "reduce_motion").unwrap().as_deref(),
            Some("true")
            [
                "General",
                "Interview",
                "Coding",
                "Translate",
                "Meeting",
                "Interviewer"
            ]
        assert_eq!(
            slots,
            [Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)]
        );
