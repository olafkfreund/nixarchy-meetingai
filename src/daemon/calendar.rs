//! Optional calendar lookup: the event happening now that has a Meet, Teams or Zoom link names the
//! session. Read-only through the user's own `gog` login; a missing or logged-out `gog` is a quiet no-op.
//! Event titles are never logged.

use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

const LINKS: [&str; 4] = [
    "meet.google.com",
    "teams.microsoft.com",
    "teams.live.com",
    "zoom.us",
];
/// How far either side of now the lookup reaches.
const WINDOW_MS: i64 = 10 * 60_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub title: String,
    pub start: i64,
    pub end: i64,
}

impl Event {
    /// On now, or starting within the lookup window.
    pub fn current(&self, now: i64) -> bool {
        self.end > now && self.start <= now + WINDOW_MS
    }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// RFC 3339 with an offset (or Z) to epoch milliseconds.
fn parse_time(s: &str) -> Option<i64> {
    let (date, rest) = s.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, mo, da) = (d.next()??, d.next()??, d.next()??);
    let split = rest.find(['Z', '+', '-']).unwrap_or(rest.len());
    let (clock, zone) = rest.split_at(split);
    let mut t = clock.split(':');
    let h = t.next()?.parse::<i64>().ok()?;
    let mi = t.next()?.parse::<i64>().ok()?;
    let sec = t.next().map_or(Some(0.0), |p| p.parse::<f64>().ok())?;
    let offset = match zone.as_bytes().first() {
        Some(b'+') | Some(b'-') => {
            let (hh, mm) = zone[1..].split_once(':')?;
            let v = hh.parse::<i64>().ok()? * 60 + mm.parse::<i64>().ok()?;
            if zone.starts_with('-') { -v } else { v }
        }
        _ => 0,
    };
    let secs = days_from_civil(y, mo, da) * 86_400 + h * 3600 + mi * 60 - offset * 60;
    Some(secs * 1000 + (sec * 1000.0) as i64)
}

/// Epoch milliseconds as RFC 3339 UTC, for `gog --from/--to`.
fn format_time(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let s = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

fn moment(v: &Value) -> Option<i64> {
    parse_time(v.as_str().or_else(|| v["dateTime"].as_str())?)
}

/// The timed event with a conference link that is on now, else the next one starting within the
/// window. Back to back, the next one wins once the current one ends within 2 minutes.
/// All-day events never count.
pub fn pick(json: &str, now: i64) -> Option<Event> {
    let root: Value = serde_json::from_str(json).ok()?;
    let list = root
        .as_array()
        .or_else(|| root["events"].as_array())
        .or_else(|| root["items"].as_array())?;
    let all: Vec<Event> = list
        .iter()
        .filter_map(|e| {
            let (start, end) = (moment(&e["start"])?, moment(&e["end"])?);
            let text = [
                &e["hangoutLink"],
                &e["location"],
                &e["description"],
                &e["conferenceData"],
            ]
            .map(|v| v.to_string())
            .join(" ");
            let title = e["summary"].as_str()?.trim();
            let e = Event {
                title: title.chars().take(120).collect(),
                start,
                end,
            };
            (LINKS.iter().any(|l| text.contains(l)) && !title.is_empty() && e.current(now))
                .then_some(e)
        })
        .collect();
    let next = all.iter().filter(|e| e.start > now).min_by_key(|e| e.start);
    let cur = all
        .iter()
        .filter(|e| e.start <= now)
        .max_by_key(|e| e.start);
    match (cur, next) {
        (Some(c), Some(n)) if c.end - now <= 2 * 60_000 => Some(n),
        (Some(c), _) => Some(c),
        (None, n) => n,
    }
    .cloned()
}

/// `gog calendar events` stdout for now ± 10 minutes, or None when `gog` is missing, logged out or slow.
pub async fn fetch(program: &str, now: i64) -> Option<String> {
    let out = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(program)
            .args([
                "--readonly",
                "--no-input",
                "calendar",
                "events",
                "--all",
                "--json",
                "--results-only",
            ])
            .args(["--from", &format_time(now - WINDOW_MS)])
            .args(["--to", &format_time(now + WINDOW_MS)])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).ok())?
}

/// `calendar.json` (0600) in the daemon's runtime directory, for meeting-mode to use as a confirmer.
pub fn write(dir: &Path, event: Option<&Event>, now: i64) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let body = json!({
        "event": event.map(|e| json!({"title": e.title, "start": e.start, "end": e.end})),
        "updated": now,
    });
    let tmp = dir.join("calendar.json.tmp");
    let done = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| f.write_all(body.to_string().as_bytes()))
        .and_then(|()| std::fs::rename(&tmp, dir.join("calendar.json")));
    if done.is_err() {
        tracing::warn!("could not write calendar.json");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/gog-events.json");

    #[test]
    fn times_round_trip() {
        let t = parse_time("2026-10-27T10:00:00+01:00").unwrap();
        assert_eq!(format_time(t), "2026-10-27T09:00:00Z");
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn picks_the_linked_timed_event_now() {
        let now = parse_time("2026-10-27T10:05:00+01:00").unwrap();
        let e = pick(FIXTURE, now).unwrap();
        assert_eq!(e.title, "Sprint planning");
        // Lunch has no link, the all-day Zoom event is not timed.
        assert!(pick(FIXTURE, parse_time("2026-10-27T11:45:00+01:00").unwrap()).is_none());
        assert!(pick("not json", now).is_none());
    }

    #[test]
    fn back_to_back_events_hand_over_near_the_end() {
        let ev = |t: &str, a: i64, b: i64| {
            let f = |m: i64| format_time(m * 60_000).replace('Z', "+00:00");
            json!({"summary": t, "hangoutLink": "https://meet.google.com/x",
                   "start": {"dateTime": f(a)}, "end": {"dateTime": f(b)}})
        };
        let list = json!([ev("Second", 30, 60), ev("First", 0, 30)]).to_string();
        let m = 60_000;
        assert_eq!(pick(&list, 10 * m).unwrap().title, "First");
        assert_eq!(pick(&list, 29 * m).unwrap().title, "Second");
        assert_eq!(pick(&list, -5 * m).unwrap().title, "First");
    }

    #[tokio::test]
    async fn a_missing_gog_is_a_quiet_none() {
        assert!(fetch("/nonexistent/gog", 0).await.is_none());
    }

    #[test]
    fn the_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("nm-cal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, None, 1);
        let m = std::fs::metadata(dir.join("calendar.json")).unwrap();
        assert_eq!(m.permissions().mode() & 0o777, 0o600);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
