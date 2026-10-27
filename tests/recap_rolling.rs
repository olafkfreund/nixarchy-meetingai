//! A 3-hour meeting is recapped from rolling chunks, not one giant prompt.
use nixarchy_meetingai::ai::{catalog::custom::CustomProvider, providers::ProviderConfig, recap};
use nixarchy_meetingai::db::{Database, meetings};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// Fake provider: answers every request with each `DECISION-*` token it sees.
fn fake_provider(largest: Arc<Mutex<usize>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/test", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for socket in listener.incoming().flatten() {
            let largest = largest.clone();
            std::thread::spawn(move || answer(socket, largest));
        }
    });
    url
}

fn answer(mut socket: TcpStream, largest: Arc<Mutex<usize>>) {
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
    write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
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
    recap::generate(&path, &id, Some(config), "English")
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
    recap::generate(&path, &id, Some(config), "English")
        .await
        .unwrap();
    let recap = meetings::recap(db.connection(), &id).unwrap().unwrap();
    assert!(recap.content.contains("DECISION-LATE"), "{}", recap.content);
    assert_eq!(meetings::chunks(db.connection(), &id).unwrap().len(), 2);
}
