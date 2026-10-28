//! Data written by the code before the rewrite (#3) must still open and read back.
//! The fixture is regenerated only on purpose, and never once any rewrite step has merged
//! (it must stay data written by the old code):
//! `OLD_DATA_WRITE=1 cargo test --test old_data -- --ignored write_fixture`.

use nixarchy_meetingai::core::settings::Settings;
use nixarchy_meetingai::db::{Database, conversations, library, meetings, profiles, settings};
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/old-history");

fn copy_fixture(names: &[&str]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nm-old-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    for name in names {
        std::fs::copy(Path::new(FIXTURE).join(name), dir.join(name)).unwrap();
    }
    dir.join(names[0])
}

/// Every table's columns (type, not-null, default, key) and every index and trigger.
fn shape(c: &Connection) -> Vec<String> {
    let mut out = Vec::new();
    let mut objects = c
        .prepare(
            "SELECT type, name, coalesce(sql, '') FROM sqlite_schema
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .unwrap();
    let rows: Vec<(String, String, String)> = objects
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for (kind, name, sql) in rows {
        if kind == "table" {
            let mut columns = c
                .prepare("SELECT name, type, \"notnull\", coalesce(dflt_value, ''), pk FROM pragma_table_info(?1)")
                .unwrap();
            let cols: Vec<String> = columns
                .query_map([&name], |r| {
                    Ok(format!(
                        "{}:{}:{}:{}:{}",
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?
                    ))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            out.push(format!("table {name} {}", cols.join(",")));
        } else {
            out.push(format!(
                "{kind} {name} {}",
                sql.split_whitespace().collect::<Vec<_>>().join(" ")
            ));
        }
    }
    out
}

fn setting_keys(c: &Connection) -> BTreeSet<String> {
    settings::all(c).unwrap().into_keys().collect()
}

#[test]
fn a_new_database_has_the_same_schema_as_the_old_one() {
    let old = Database::open(copy_fixture(&["history.db", "history.db.key"])).unwrap();
    let new = Database::in_memory().unwrap();
    assert_eq!(shape(old.connection()), shape(new.connection()));
}

#[test]
fn settings_keep_their_keys_and_values() {
    let old = Database::open(copy_fixture(&["history.db", "history.db.key"])).unwrap();
    let loaded = Settings::load(&old).unwrap();
    assert_eq!(loaded.chat_provider, "fixture-provider");
    assert_eq!(loaded.speech_language, "de");
    assert_eq!(loaded.capture_delay_seconds, 7);
    assert_eq!(loaded.text_scale, 125);
    assert!(loaded.auto_record_on_call);
    assert_eq!(loaded.personal_context, "Fixture me");
    // A renamed key would leave the old key behind, unread.
    let fresh = Database::in_memory().unwrap();
    Settings::default().save(&fresh).unwrap();
    let missing: Vec<_> = setting_keys(old.connection())
        .difference(&setting_keys(fresh.connection()))
        .cloned()
        .collect();
    // Keys written by other code (profiles, the fixture) rather than Settings.
    assert_eq!(
        missing,
        [
            "context_profiles_migrated",
            "custom_providers",
            "selected_context_profile"
        ]
    );
}

#[test]
fn an_old_plaintext_database_is_encrypted_and_kept() {
    let path = copy_fixture(&["history-plain.db"]);
    assert!(
        std::fs::read(&path)
            .unwrap()
            .starts_with(b"SQLite format 3")
    );
    let db = Database::open(&path).unwrap();
    assert!(
        !std::fs::read(&path)
            .unwrap()
            .starts_with(b"SQLite format 3")
    );
    assert!(path.with_extension("db.key").exists());
    assert_eq!(
        meetings::list(db.connection(), 10).unwrap()[0].title,
        "Fixture standup"
    );
}

#[test]
fn data_written_before_the_rewrite_reads_back() {
    let db = Database::open(copy_fixture(&["history.db", "history.db.key"])).unwrap();
    let c = db.connection();

    let meeting = &meetings::list(c, 10).unwrap()[0];
    assert_eq!(meeting.title, "Fixture standup");
    assert_eq!(meeting.line_count, 2);
    assert_eq!(meeting.duration_seconds, 90);
    assert_eq!(meeting.language, "en");
    let lines: Vec<_> = meetings::lines(c, &meeting.id)
        .unwrap()
        .into_iter()
        .map(|l| (l.who, l.speaker, l.text, l.start_time, l.end_time))
        .collect();
    assert_eq!(
        lines,
        [
            ("me".into(), None, "What ships next?".into(), 0.0, 2.0),
            (
                "them".into(),
                Some(1),
                "The walrus release.".into(),
                2.0,
                4.0
            ),
        ]
    );
    let recap = meetings::recap(c, &meeting.id).unwrap().unwrap();
    assert!(recap.content.contains("ship the walrus release"));
    assert_eq!(meetings::search(c, "walrus", 10).unwrap().len(), 1);
    // Only in the notes index, so this goes through notes_fts.
    assert_eq!(meetings::search(c, "okapi", 10).unwrap().len(), 1);

    let conversation = &conversations::list(c, 10).unwrap()[0];
    assert_eq!(conversation.title, "Fixture chat");
    let messages = conversations::messages(c, &conversation.id).unwrap();
    let pairs: Vec<_> = messages
        .iter()
        .map(|m| (m.role.as_str(), m.content.as_str(), m.timestamp))
        .collect();
    assert_eq!(
        pairs,
        [
            ("user", "What is due?", 1_700_000_001),
            ("assistant", "The walrus release.", 1_700_000_002)
        ]
    );

    let file = &library::list(c).unwrap()[0];
    assert_eq!(
        (
            file.name.as_str(),
            file.mime_type.as_str(),
            file.size_bytes,
            file.pages
        ),
        ("brief.pdf", "application/pdf", 1234, 3)
    );
    assert_eq!(file.ocr_text, "Brief text");

    let profile = profiles::list(c)
        .unwrap()
        .into_iter()
        .find(|p| p.id == "fixture-profile")
        .unwrap();
    assert_eq!(
        (
            profile.name.as_str(),
            profile.personal_context.as_str(),
            profile.meeting_context.as_str()
        ),
        ("Fixture", "I lead the platform team.", "Weekly standup.")
    );

    assert_eq!(
        settings::get(c, "custom_providers").unwrap().as_deref(),
        Some(r#"[{"id":"fixture","name":"Fixture LLM"}]"#)
    );
    assert!(
        settings::prompts(c)
            .unwrap()
            .iter()
            .any(|p| p.name == "Fixture prompt" && p.prompt == "Summarise.")
    );

    // The old database also takes new writes, and search sees them.
    meetings::add_line(
        c,
        &meeting.id,
        "me",
        None,
        "Add the narwhal task.",
        4.0,
        6.0,
    )
    .unwrap();
    meetings::index_notes(c, &meeting.id, "narwhal follow-up").unwrap();
    assert_eq!(meetings::search(c, "narwhal", 10).unwrap().len(), 1);
}

#[test]
#[ignore]
fn write_fixture() {
    if std::env::var_os("OLD_DATA_WRITE").is_none() {
        return;
    }
    let path = Path::new(FIXTURE).join("history.db");
    let plain = Path::new(FIXTURE).join("history-plain.db");
    let _ = std::fs::remove_file(&plain);
    for suffix in ["", ".key", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
    let db = Database::open(&path).unwrap();
    let c = db.connection();

    let id = meetings::start(c, "Fixture standup", "", "en", None, Default::default()).unwrap();
    meetings::add_line(c, &id, "me", None, "What ships next?", 0.0, 2.0).unwrap();
    meetings::add_line(c, &id, "them", Some(1), "The walrus release.", 2.0, 4.0).unwrap();
    meetings::finish(c, &id, 90).unwrap();
    meetings::begin_recap(c, &id).unwrap();
    meetings::finish_recap(c, &id, Ok("We agreed to ship the walrus release.")).unwrap();
    meetings::index_notes(c, &id, "Decision: the okapi plan.").unwrap();

    let chat = conversations::create(c, "Fixture chat", 1_700_000_000).unwrap();
    conversations::append_message(c, &chat, "user", "What is due?", 1_700_000_001).unwrap();
    conversations::append_message(c, &chat, "assistant", "The walrus release.", 1_700_000_002)
        .unwrap();

    library::insert(c, "brief.pdf", "application/pdf", 1234, 3, "Brief text").unwrap();
    profiles::save(
        c,
        &profiles::Profile {
            id: "fixture-profile".into(),
            name: "Fixture".into(),
            personal_context: "I lead the platform team.".into(),
            meeting_context: "Weekly standup.".into(),
        },
    )
    .unwrap();
    settings::set(
        c,
        "custom_providers",
        r#"[{"id":"fixture","name":"Fixture LLM"}]"#,
    )
    .unwrap();
    settings::create_prompt(c, "Fixture prompt", "Summarise.").unwrap();
    let mut saved = Settings::load(&db).unwrap();
    saved.chat_provider = "fixture-provider".into();
    saved.speech_language = "de".into();
    saved.capture_delay_seconds = 7;
    saved.text_scale = 125;
    saved.auto_record_on_call = true;
    saved.personal_context = "Fixture me".into();
    saved.save(&db).unwrap();
    c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    // The same data as an unencrypted database, as installs before encryption had it.
    c.execute(
        "ATTACH DATABASE ?1 AS plaintext KEY ''",
        [plain.to_str().unwrap()],
    )
    .unwrap();
    c.query_row("SELECT sqlcipher_export('plaintext')", [], |_| Ok(()))
        .unwrap();
    c.execute_batch("DETACH DATABASE plaintext").unwrap();
    drop(db);
    let _ = std::fs::remove_file(format!("{}.open.lock", path.display()));
}
