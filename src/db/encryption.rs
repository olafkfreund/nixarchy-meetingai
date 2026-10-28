    connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let directory = std::env::temp_dir().join(format!(
            "nixarchy-meetingai-encryption-{}",
            uuid::Uuid::new_v4()
        ));
        let meeting = meetings::start(
            db.connection(),
            secret,
            "auto",
            "en",
            None,
            Default::default(),
        )
        .unwrap();
        // A legacy database has no series_id column, so insert without it.
        let meeting = "legacy-meeting".to_owned();
        original
            .execute(
                "INSERT INTO meetings (id, title, context, language) VALUES (?1, 'Legacy meeting', 'context', 'en')",
                [&meeting],
            )
            .unwrap();
