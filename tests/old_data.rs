//! Data written by the code before the rewrite (#3) must still open and read back.
//! The fixture is regenerated only on purpose:
//! `OLD_DATA_WRITE=1 cargo test --test old_data -- --ignored write_fixture`.

use nixarchy_meetingai::db::{Database, conversations, library, meetings, profiles, settings};
use std::path::{Path, PathBuf};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/old-history");

fn copy_fixture() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nm-old-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    for name in ["history.db", "history.db.key"] {
        std::fs::copy(Path::new(FIXTURE).join(name), dir.join(name)).unwrap();
    }
    dir.join("history.db")
}

#[test]
fn data_written_before_the_rewrite_reads_back() {
    let db = Database::open(copy_fixture()).unwrap();
    let c = db.connection();

    let meeting = &meetings::list(c, 10).unwrap()[0];
    assert_eq!(meeting.title, "Fixture standup");
    assert_eq!(meeting.line_count, 2);
    assert_eq!(meeting.duration_seconds, 90);
    let recap = meetings::recap(c, &meeting.id).unwrap().unwrap();
    assert!(recap.content.contains("ship the walrus release"));
    assert_eq!(meetings::search(c, "walrus", 10).unwrap().len(), 1);

    let conversation = &conversations::list(c, 10).unwrap()[0];
    assert_eq!(conversation.title, "Fixture chat");
    let messages = conversations::messages(c, &conversation.id).unwrap();
    let pairs: Vec<_> = messages
        .iter()
        .map(|m| (m.role.as_str(), m.content.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("user", "What is due?"),
            ("assistant", "The walrus release.")
        ]
    );

    let file = &library::list(c).unwrap()[0];
    assert_eq!((file.name.as_str(), file.pages), ("brief.pdf", 3));
    assert_eq!(file.ocr_text, "Brief text");

    let profile = profiles::list(c)
        .unwrap()
        .into_iter()
        .find(|p| p.id == "fixture-profile")
        .unwrap();
    assert_eq!(profile.personal_context, "I lead the platform team.");

    assert_eq!(
        settings::get(c, "custom_providers").unwrap().as_deref(),
        Some(r#"[{"id":"fixture","name":"Fixture LLM"}]"#)
    );
    assert!(
        settings::prompts(c)
            .unwrap()
            .iter()
            .any(|p| p.name == "Fixture prompt")
    );
}

#[test]
#[ignore]
fn write_fixture() {
    if std::env::var_os("OLD_DATA_WRITE").is_none() {
        return;
    }
    let path = Path::new(FIXTURE).join("history.db");
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
    c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    drop(db);
    let _ = std::fs::remove_file(format!("{}.open.lock", path.display()));
}
