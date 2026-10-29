//! Lists Pulse and PipeWire input sources through `pactl`.

/// A selectable input source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Read microphone sources from the JSON source list returned by `pactl`.
pub fn parse_sources(json: &str) -> Result<Vec<Device>, String> {
    let entries: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|error| format!("invalid source list: {error}"))?;
    entries
        .into_iter()
        .map(|entry| {
            let name = entry
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "source entry lacks a string name".to_owned())?;
            let monitor = optional_text(&entry, "monitor_of_sink")?;
            let label = optional_text(&entry, "description")?
                .filter(|description| !description.is_empty())
                .unwrap_or(name);
            Ok((monitor.is_none()).then(|| Device {
                id: name.to_owned(),
                name: label.to_owned(),
                is_default: false,
            }))
        })
        .collect::<Result<Vec<_>, String>>()
        .map(|entries| entries.into_iter().flatten().collect())
}

fn optional_text<'a>(entry: &'a serde_json::Value, key: &str) -> Result<Option<&'a str>, String> {
    match entry.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(format!("source field {key} must be text or null")),
    }
}

/// Run `pactl`, killed after two seconds so a wedged server cannot hang callers.
pub(crate) fn pactl(args: &[&str]) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new("pactl")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not run pactl: {error}"))?;
    let mut stdout = child.stdout.take().ok_or("pactl gave no output")?;
    let (tx, rx) = std::sync::mpsc::channel();
    // Read on a thread so a full pipe cannot block the child before the deadline.
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(text) => match child.wait() {
            Ok(status) if status.success() => Ok(text),
            _ => Err("pactl failed".into()),
        },
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err("pactl timed out".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_fields_and_monitors() {
        let input = r#"[{"name":"cedar.in","description":"Cedar Mic","extra":4},{"name":"river.monitor","monitor_of_sink":"river","description":"River"},{"name":"birch.in","description":"Birch Mic"}]"#;
        assert_eq!(
            parse_sources(input).unwrap(),
            vec![
                Device {
                    id: "cedar.in".into(),
                    name: "Cedar Mic".into(),
                    is_default: false
                },
                Device {
                    id: "birch.in".into(),
                    name: "Birch Mic".into(),
                    is_default: false
                },
            ]
        );
    }

    #[test]
    fn missing_labels_and_null_monitor() {
        let input = r#"[{"name":"larch.in","monitor_of_sink":null},{"name":"elm.in","description":""},{"name":"oak.monitor","monitor_source":"oak"}]"#;
        let found = parse_sources(input).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            ["larch.in", "elm.in", "oak.monitor"]
        );
    }

    #[test]
    fn invalid_shape() {
        assert!(parse_sources("bad").is_err());
        assert!(parse_sources("{}").is_err());
        assert!(parse_sources("[{}]").is_err());
        assert!(parse_sources(r#"[{"name":"cedar","description":3}]"#).is_err());
        assert!(parse_sources(r#"[{"name":"cedar","monitor_of_sink":true}]"#).is_err());
        assert!(parse_sources("[]").unwrap().is_empty());
    }
}
