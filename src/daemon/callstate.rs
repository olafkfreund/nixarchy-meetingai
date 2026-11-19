use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct CallReading {
    pub present: bool,
    pub schema: u64,
    pub call: String,
    pub since: f64,
    pub by: String,
    pub window: String,
    pub window_state: String,
    pub window_class: String,
    pub showing: bool,
    pub window_seq: u64,
}

impl Default for CallReading {
    fn default() -> Self {
        Self {
            present: false,
            schema: 1,
            call: "none".into(),
            since: 0.0,
            by: String::new(),
            window: String::new(),
            window_state: "none".into(),
            window_class: String::new(),
            showing: false,
            window_seq: 0,
        }
    }
}

pub fn pid(bytes: &[u8]) -> Option<u32> {
    // Probed in every state: a dead writer must read as not present even while idle.
    let raw = serde_json::from_slice::<Value>(bytes).ok()?;
    raw.get("pid")?
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
}

pub fn parse(bytes: &[u8], now: i64, probe: impl Fn(u32) -> bool) -> CallReading {
    let Ok(raw) = serde_json::from_slice::<Value>(bytes) else {
        return CallReading::default();
    };
    let Some(raw) = raw.as_object() else {
        return CallReading::default();
    };
    let mut reading = CallReading {
        present: true,
        schema: raw.get("schema").and_then(Value::as_u64).unwrap_or(1),
        ..CallReading::default()
    };
    let pid = raw.get("pid").and_then(Value::as_u64).unwrap_or(0);
    let writer_gone = pid > 0 && u32::try_from(pid).map_or(true, |pid| !probe(pid));
    let updated = raw.get("updated").and_then(Value::as_i64).unwrap_or(0);
    if writer_gone || (pid == 0 && now.saturating_sub(updated) > 600_000) {
        // Unknown, not "no call": nothing may stop on it.
        reading.present = false;
        return reading;
    }
    reading.call = match raw.get("call").and_then(Value::as_str) {
        Some("call") => "call",
        Some("maybe") => "maybe",
        _ => "none",
    }
    .into();
    if reading.call != "none" {
        reading.since = raw.get("callSince").and_then(Value::as_f64).unwrap_or(0.0);
    }
    reading.by = raw
        .get("callBy")
        .and_then(Value::as_str)
        .filter(|by| matches!(*by, "native" | "webapp" | "title" | "fallback"))
        .unwrap_or("")
        .into();
    reading.window = raw
        .get("callWindow")
        .and_then(Value::as_str)
        .unwrap_or("")
        .into();
    reading.window_state = raw
        .get("callWindowState")
        .and_then(Value::as_str)
        .filter(|state| matches!(*state, "none" | "bound" | "ambiguous"))
        .unwrap_or("none")
        .into();
    reading.window_class = raw
        .get("callWindowClass")
        .and_then(Value::as_str)
        .unwrap_or("")
        .into();
    reading.showing = raw
        .get("callWindowShowing")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    reading.window_seq = raw
        .get("callWindowSeq")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    reading
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    #[test]
    fn callstate_schemas_and_fields() {
        let old = br#"{"updated":1700000000000,"pid":7,"call":"maybe","callSince":12,"callBy":"fallback"}"#;
        let r = parse(old, NOW, |_| true);
        assert!(r.present);
        assert_eq!(
            (r.schema, r.call.as_str(), r.since, r.by.as_str()),
            (1, "maybe", 12.0, "fallback")
        );
        let new = br#"{"schema":2,"updated":1700000000000,"pid":8,"call":"call","callSince":13,"callBy":"native","callWindow":"0x123","callWindowState":"bound","callWindowClass":"Zoom","callWindowShowing":true,"callWindowSeq":4}"#;
        let r = parse(new, NOW, |_| true);
        assert_eq!(
            (
                r.schema,
                r.call.as_str(),
                r.window.as_str(),
                r.window_state.as_str(),
                r.window_class.as_str(),
                r.showing,
                r.window_seq
            ),
            (2, "call", "0x123", "bound", "Zoom", true, 4)
        );
        assert_eq!(r.by, "native");
    }

    #[test]
    fn callstate_writer_liveness_with_unchanged_file() {
        let bytes = br#"{"updated":1700000000000,"pid":7,"call":"call"}"#;
        assert_eq!(pid(bytes), Some(7));
        assert_eq!(parse(bytes, NOW, |pid| pid == 7).call, "call");
        assert_eq!(parse(bytes, NOW, |_| false).call, "none");
        assert!(!parse(bytes, NOW, |_| false).present);
        let idle = br#"{"updated":1700000000000,"pid":7,"call":"none"}"#;
        assert_eq!(pid(idle), Some(7));
        assert!(parse(idle, NOW, |pid| pid == 7).present);
        assert!(!parse(idle, NOW, |_| false).present);
        let new_pid = br#"{"updated":1700000000000,"pid":8,"call":"call"}"#;
        assert_eq!(parse(new_pid, NOW, |pid| pid == 7).call, "none");
        assert_eq!(parse(new_pid, NOW, |pid| pid == 8).call, "call");
    }

    #[test]
    fn callstate_pidless_ages_without_file_change() {
        let bytes = br#"{"updated":1700000000000,"call":"call"}"#;
        assert_eq!(parse(bytes, NOW + 600_000, |_| false).call, "call");
        assert_eq!(parse(bytes, NOW + 600_001, |_| false).call, "none");
        assert!(!parse(bytes, NOW + 600_001, |_| false).present);
    }

    #[test]
    fn callstate_garbage_and_titles() {
        assert!(!parse(b"garbage", NOW, |_| true).present);
        assert!(!parse(b"[]", NOW, |_| true).present);
        let bytes = br#"{"updated":1700000000000,"call":"call","callTitle":"Secret roadmap","callApp":"Secret application","callBy":"Secret meeting title"}"#;
        let debug = format!("{:?}", parse(bytes, NOW, |_| true));
        assert!(!debug.contains("Secret"));
    }
}
