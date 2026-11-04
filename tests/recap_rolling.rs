//! A 3-hour meeting is recapped from rolling chunks, not one giant prompt.
use nixarchy_meetingai::ai::{catalog::custom::CustomProvider, providers::ProviderConfig, recap};
use nixarchy_meetingai::db::{Database, items, meetings};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Fake provider: answers every request with each `DECISION-*` token it sees.
fn fake_provider(largest: Arc<Mutex<usize>>) -> String {
    fake_provider_editing(largest, None)
}

fn fake_provider_editing(largest: Arc<Mutex<usize>>, edit: Option<(PathBuf, String)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/test", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for socket in listener.incoming().flatten() {
            let largest = largest.clone();
            let edit = edit.clone();
            std::thread::spawn(move || answer(socket, largest, edit));
        }
    });
    url
}

fn answer(mut socket: TcpStream, largest: Arc<Mutex<usize>>, edit: Option<(PathBuf, String)>) {
    let mut request = Vec::new();
    let mut chunk = [0; 16384];
    loop {
        let n = socket.read(&mut chunk).unwrap();
        assert!(n > 0);
        request.extend_from_slice(&chunk[..n]);
        if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
            let length: usize = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .unwrap()
                .parse()
                .unwrap();
            if request.len() >= end + 4 + length {
                break;
            }
        }
    }
    {
        let mut max = largest.lock().unwrap();
        *max = (*max).max(request.len());
    }
    let text = String::from_utf8_lossy(&request);
    let echoed: Vec<&str> = text
        .match_indices("DECISION-")
        .map(|(i, _)| {
            let rest = &text[i..];
            &rest[..rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .unwrap_or(rest.len())]
        })
        .collect();
    let body = if echoed.is_empty() {
        "notes: none".to_owned()
    } else {
        echoed.join("\n")
    };
    let body = if text.contains("continues_item") {
        format!(
            "{body}\n```json\n[{{\"kind\":\"action\",\"text\":\"Send corrected memo\",\"source_line\":1}}]\n```"
        )
    } else {
        body
    };
    if let Some((path, line)) = edit {
        std::thread::sleep(std::time::Duration::from_millis(5));
        let db = Database::open(&path).unwrap();
        meetings::edit_line(db.connection(), &line, "DECISION-LATER").unwrap();
    }
    write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn config() -> ProviderConfig {
    let mut config = ProviderConfig::new("custom-recap", "", "model");
    config.base_url = fake_provider(Arc::new(Mutex::new(0)));
    config.custom = Some(CustomProvider {
        auth: "none".into(),
        response_format: "text".into(),
        ..Default::default()
    });
    config
}

#[tokio::test]
async fn refreshes_only_the_edited_chunk_and_keeps_items_and_title() {
    let path = std::env::temp_dir().join(format!("recap-refresh-{}.db", uuid::Uuid::new_v4()));
    let db = Database::open(&path).unwrap();
    let c = db.connection();
    let id = meetings::start(
        c,
        "My title",
        "",
        "en",
        None,
        nixarchy_meetingai::core::settings::MeetingMode::TeamMeeting,
    )
    .unwrap();
    let first = meetings::add_line(c, &id, "them", None, "DECISION-OLD", 0.0, 1.0).unwrap();
    let first_row: i64 = c
        .query_row(
            "SELECT rowid FROM meeting_lines WHERE id=?1",
            [&first],
            |r| r.get(0),
        )
        .unwrap();
    meetings::add_chunk(
        c,
        &id,
        &meetings::NoteChunk {
            last_line: first_row,
            body: "DECISION-OLD".into(),
            read_at: Some("2000-01-01 00:00:00.000".into()),
            ..Default::default()
        },
    )
    .unwrap();
    meetings::add_line(c, &id, "them", None, "DECISION-SECOND", 2.0, 3.0).unwrap();
    let second_row: i64 = c
        .query_row(
            "SELECT max(rowid) FROM meeting_lines WHERE meeting_id=?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    meetings::add_chunk(
        c,
        &id,
        &meetings::NoteChunk {
            last_line: second_row,
            body: "DECISION-SECOND".into(),
            read_at: Some("2000-01-01 00:00:00.000".into()),
            ..Default::default()
        },
    )
    .unwrap();
    for text in ["Send the draft", "Send the draft."] {
        items::insert(
            c,
            &items::NewItem {
                meeting_id: &id,
                kind: "action",
                text,
                owner: Some("Ada"),
                due: Some("Friday"),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let before = items::list(c, items::Scope::Meeting(&id), None).unwrap();
    meetings::edit_line(c, &first, "DECISION-CORRECTED").unwrap();
    assert_eq!(
        recap::refresh_chunks(&path, &id, &config(), "en")
            .await
            .unwrap(),
        1
    );
    let chunks = meetings::chunks(c, &id).unwrap();
    assert!(chunks[0].body.contains("DECISION-CORRECTED"));
    assert_eq!(chunks[1].body, "DECISION-SECOND");
    meetings::add_line(c, &id, "them", None, "DECISION-NEW", 4.0, 5.0).unwrap();
    recap::generate(&path, &id, Some(config()), "en", false)
        .await
        .unwrap();
    // Existing items are untouched (no reconcile merge); a first summary of the new line may add its own.
    let after = items::list(c, items::Scope::Meeting(&id), None).unwrap();
    let kept: Vec<_> = after
        .into_iter()
        .filter(|i| before.iter().any(|b| b.id == i.id))
        .collect();
    assert_eq!(kept, before);
    let title: String = c
        .query_row("SELECT title FROM meetings WHERE id=?1", [&id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(title, "My title");
}

#[tokio::test]
async fn failed_refresh_preserves_body_and_read_time() {
    let path = std::env::temp_dir().join(format!("recap-refresh-fail-{}.db", uuid::Uuid::new_v4()));
    let db = Database::open(&path).unwrap();
    let c = db.connection();
    let id = meetings::start(c, "Title", "", "en", None, Default::default()).unwrap();
    let line = meetings::add_line(c, &id, "them", None, "old", 0.0, 1.0).unwrap();
    let row: i64 = c
        .query_row(
            "SELECT rowid FROM meeting_lines WHERE id=?1",
            [&line],
            |r| r.get(0),
        )
        .unwrap();
    meetings::add_chunk(
        c,
        &id,
        &meetings::NoteChunk {
            last_line: row,
            body: "original".into(),
            read_at: Some("2000-01-01 00:00:00.000".into()),
            ..Default::default()
        },
    )
    .unwrap();
    meetings::edit_line(c, &line, "corrected").unwrap();
    let before = meetings::chunks(c, &id).unwrap();
    assert!(
        recap::refresh_chunks(&path, &id, &ProviderConfig::new("none", "", ""), "en")
            .await
            .is_err()
    );
    assert_eq!(meetings::chunks(c, &id).unwrap(), before);
}

#[tokio::test]
async fn edit_during_chunk_model_call_stays_newer_than_read_at() {
    let path = std::env::temp_dir().join(format!("recap-race-{}.db", uuid::Uuid::new_v4()));
    let db = Database::open(&path).unwrap();
    let c = db.connection();
    let id = meetings::start(c, "Title", "", "en", None, Default::default()).unwrap();
    let line = meetings::add_line(
        c,
        &id,
        "them",
        None,
        &"DECISION-OLD ".repeat(700),
        0.0,
        301.0,
    )
    .unwrap();
    let mut config = ProviderConfig::new("custom-recap", "", "model");
    config.base_url = fake_provider_editing(Arc::new(Mutex::new(0)), Some((path.clone(), line)));
    config.custom = Some(CustomProvider {
        auth: "none".into(),
        response_format: "text".into(),
        ..Default::default()
    });
    assert!(recap::maybe_chunk(&path, &id, &config, "en").await.unwrap());
    let chunk = meetings::chunks(c, &id).unwrap().remove(0);
    assert!(chunk.body.contains("DECISION-OLD"));
    assert!(meetings::changed_at(c, &id).unwrap().unwrap() > chunk.read_at.unwrap());
}

#[tokio::test]
async fn recap_read_time_clears_a_same_second_rename() {
    let path = std::env::temp_dir().join(format!("recap-rename-{}.db", uuid::Uuid::new_v4()));
    let db = Database::open(&path).unwrap();
    let c = db.connection();
    let id = meetings::start(c, "Title", "", "en", None, Default::default()).unwrap();
    meetings::add_line(c, &id, "them", Some(0), "DECISION-RENAMED", 0.0, 1.0).unwrap();
    let provider = config();
    for _ in 0..3 {
        meetings::name_speaker(c, &id, "0", "Ada").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        recap::generate(&path, &id, Some(provider.clone()), "en", false)
            .await
            .unwrap();
        let changed = meetings::changed_at(c, &id).unwrap().unwrap();
        let read = meetings::recap(c, &id).unwrap().unwrap().read_at.unwrap();
        if changed[..19] == read[..19] {
            assert!(changed < read);
            return;
        }
    }
    panic!("the provider took more than a second on all three attempts");
}

#[tokio::test]
async fn three_hour_meeting_keeps_its_midpoint_decision() {
    let dir = std::env::temp_dir().join(format!(
        "nixarchy-meetingai-rolling-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    let db = Database::open(&path).unwrap();
    let id = meetings::start(db.connection(), "Long", "", "en", None, Default::default()).unwrap();
    let filler = "We went over the roadmap and the staffing plan in some detail. ".repeat(6);
    let mut total = 0;
    for i in 0..1080 {
        let text = if i == 540 {
            format!("{filler} DECISION-MIDPOINT is approved.")
        } else {
            filler.clone()
        };
        total += text.len();
        let t = f64::from(i) * 10.0;
        meetings::add_line(db.connection(), &id, "them", Some(0), &text, t, t + 9.0).unwrap();
    }
    assert!(total > 400_000);
    drop(db);

    let largest = Arc::new(Mutex::new(0));
    let mut config = ProviderConfig::new("custom-recap", "", "model");
    config.base_url = fake_provider(largest.clone());
    config.custom = Some(CustomProvider {
        auth: "none".into(),
        response_format: "text".into(),
        ..Default::default()
    });
    recap::generate(&path, &id, Some(config), "English", true)
        .await
        .unwrap();

    let db = Database::open(&path).unwrap();
    let recap = meetings::recap(db.connection(), &id).unwrap().unwrap();
    assert_eq!(recap.status, "ready");
    assert!(
        recap.content.contains("DECISION-MIDPOINT"),
        "{}",
        recap.content
    );
    assert!(meetings::chunks(db.connection(), &id).unwrap().len() >= 40);
    assert!(
        *largest.lock().unwrap() < 30_000,
        "no prompt holds the whole meeting"
    );
}

#[tokio::test]
async fn late_line_with_an_earlier_end_time_still_reaches_the_notes() {
    let dir =
        std::env::temp_dir().join(format!("nixarchy-meetingai-late-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    let db = Database::open(&path).unwrap();
    let id = meetings::start(db.connection(), "Late", "", "en", None, Default::default()).unwrap();
    meetings::add_line(
        db.connection(),
        &id,
        "them",
        Some(0),
        &"word ".repeat(2000),
        0.0,
        100.0,
    )
    .unwrap();
    let mut config = ProviderConfig::new("custom-recap", "", "model");
    config.base_url = fake_provider(Arc::new(Mutex::new(0)));
    config.custom = Some(CustomProvider {
        auth: "none".into(),
        response_format: "text".into(),
        ..Default::default()
    });
    assert!(
        recap::maybe_chunk(&path, &id, &config, "English")
            .await
            .unwrap()
    );
    // The other channel's line lands after the chunk but ends before the chunk's t1.
    meetings::add_line(
        db.connection(),
        &id,
        "me",
        None,
        "DECISION-LATE is approved",
        10.0,
        20.0,
    )
    .unwrap();
    recap::generate(&path, &id, Some(config), "English", true)
        .await
        .unwrap();
    let recap = meetings::recap(db.connection(), &id).unwrap().unwrap();
    assert!(recap.content.contains("DECISION-LATE"), "{}", recap.content);
    assert_eq!(meetings::chunks(db.connection(), &id).unwrap().len(), 2);
}
