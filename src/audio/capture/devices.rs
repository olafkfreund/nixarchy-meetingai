//! Audio device enumeration.
//!
//! On Linux this shells out to `pactl`, which covers PulseAudio and
//! PipeWire-Pulse. When it is unavailable — or on another platform — the lists are
//! empty and the UI says so, rather than inventing devices that do not exist.

use serde::Deserialize;

/// One input or output device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Detected devices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Devices {
    pub inputs: Vec<Device>,
    pub outputs: Vec<Device>,
    /// Why enumeration failed, when it did.
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct PactlSource {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    monitor_of_sink: Option<String>,
}

#[derive(Deserialize)]
struct PactlSink {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

/// Parse `pactl --format=json list sources`.
///
/// Monitor sources are excluded from microphones: they carry a sink's output, not
/// a microphone, and including them would let a user "record" their speakers.
pub fn parse_sources(json: &str) -> Result<Vec<Device>, String> {
    let sources: Vec<PactlSource> =
        serde_json::from_str(json).map_err(|error| format!("unexpected pactl output: {error}"))?;
    Ok(sources
        .into_iter()
        .filter(|source| source.monitor_of_sink.is_none())
        .map(|source| Device {
            name: source
                .description
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| source.name.clone()),
            id: source.name,
            is_default: false,
        })
        .collect())
}

/// Microphones by Pulse source name: the echo-cancelled source first and marked
/// default when present, else the server's default source.
pub fn pulse_inputs(json: &str, default_source: Option<&str>) -> Result<Vec<Device>, String> {
    use crate::audio::microphone::ECHO_CANCEL_SOURCE;
    let mut devices = parse_sources(json)?;
    match devices.iter().position(|d| d.id == ECHO_CANCEL_SOURCE) {
        Some(index) => {
            devices.swap(0, index);
            mark_default(&mut devices, Some(ECHO_CANCEL_SOURCE));
        }
        None => mark_default(&mut devices, default_source),
    }
    Ok(devices)
}

/// Keep a saved device visible and selected even when it is not a Pulse source
/// (an explicit `cpal:` choice runs without echo cancelling).
pub fn keep_selected(devices: &mut Vec<Device>, selected: Option<&str>) {
    let Some(id) = selected
        .map(str::trim)
        .filter(|id| !id.is_empty() && *id != "default")
    else {
        return;
    };
    if !devices.iter().any(|d| d.id == id) {
        let name = id.strip_prefix("cpal:").unwrap_or(id);
        devices.push(Device {
            id: id.to_owned(),
            name: format!("{name} (no echo cancelling)"),
            is_default: false,
        });
    }
    for device in devices.iter_mut() {
        device.is_default = device.id == id;
    }
}

/// Parse `pactl --format=json list sinks`.
pub fn parse_sinks(json: &str) -> Result<Vec<Device>, String> {
    let sinks: Vec<PactlSink> =
        serde_json::from_str(json).map_err(|error| format!("unexpected pactl output: {error}"))?;
    Ok(sinks
        .into_iter()
        .map(|sink| Device {
            name: sink
                .description
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| sink.name.clone()),
            id: sink.name,
            is_default: false,
        })
        .collect())
}

/// Mark the device matching `default_name` as default.
pub fn mark_default(devices: &mut [Device], default_name: Option<&str>) {
    let Some(default_name) = default_name.map(str::trim).filter(|name| !name.is_empty()) else {
        if let Some(first) = devices.first_mut() {
            first.is_default = true;
        }
        return;
    };
    for device in devices.iter_mut() {
        device.is_default = device.id == default_name;
    }
}

/// Enumerate devices on this machine.
/// `selected` is the saved input device; it stays in the list even when it is not a Pulse source.
pub fn enumerate(selected: Option<&str>) -> Devices {
    let result = enumerate_linux(selected);
    match result {
        Ok(devices) => devices,
        Err(error) => Devices {
            error: Some(error),
            ..Devices::default()
        },
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
        .map_err(|error| format!("pactl is unavailable: {error}"))?;
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

fn enumerate_linux(selected: Option<&str>) -> Result<Devices, String> {
    // Same ids as Microphone::start: Pulse source names, or `cpal:` ids when
    // there is no Pulse server.
    let default_source = pactl(&["get-default-source"])
        .ok()
        .and_then(|text| text.lines().next().map(str::to_owned));
    let mut inputs = pactl(&["--format=json", "list", "sources"])
        .and_then(|text| pulse_inputs(&text, default_source.as_deref()))
        .unwrap_or_else(|_| {
            crate::audio::microphone::input_devices()
                .into_iter()
                .map(|d| Device {
                    id: d.id,
                    name: d.name,
                    is_default: d.is_default,
                })
                .collect()
        });
    keep_selected(&mut inputs, selected);
    let outputs_result =
        pactl(&["--format=json", "list", "sinks"]).and_then(|text| parse_sinks(&text));
    let error = outputs_result.as_ref().err().cloned();
    let mut outputs = outputs_result.unwrap_or_default();

    // `get-default-*` prints the name plus a description on the next line.
    let default_output = pactl(&["get-default-sink"])
        .ok()
        .and_then(|text| text.lines().next().map(str::to_owned));

    mark_default(&mut outputs, default_output.as_deref());

    Ok(Devices {
        inputs,
        outputs,
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCES: &str = r#"[
      {"name":"alsa_input.pci-0000_00_1f.3.analog-stereo","description":"Built-in Audio Analog Stereo","monitor_of_sink":null},
      {"name":"alsa_output.pci-0000_00_1f.3.analog-stereo.monitor","description":"Monitor of Built-in Audio","monitor_of_sink":"alsa_output.pci-0000_00_1f.3.analog-stereo"}
    ]"#;

    const SINKS: &str = r#"[
      {"name":"alsa_output.pci-0000_00_1f.3.analog-stereo","description":"Built-in Audio Analog Stereo"}
    ]"#;

    #[test]
    fn monitor_sources_are_excluded_from_microphones() {
        let devices = parse_sources(SOURCES).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "alsa_input.pci-0000_00_1f.3.analog-stereo");
        assert_eq!(devices[0].name, "Built-in Audio Analog Stereo");
    }

    #[test]
    fn echo_cancel_source_is_listed_first_and_default() {
        let json = r#"[
          {"name":"alsa_input.pci","description":"Built-in","monitor_of_sink":null},
          {"name":"alsa_output.pci.monitor","description":"Monitor","monitor_of_sink":"alsa_output.pci"},
          {"name":"meetingai-echo-cancel-source","description":"Meeting AI echo-cancelled mic","monitor_of_sink":null}
        ]"#;
        let devices = pulse_inputs(json, Some("alsa_input.pci")).unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, "meetingai-echo-cancel-source");
        assert_eq!(devices[0].name, "Meeting AI echo-cancelled mic");
        assert!(devices[0].is_default && !devices[1].is_default);
    }

    #[test]
    fn default_source_is_marked_without_echo_cancel() {
        let devices =
            pulse_inputs(SOURCES, Some("alsa_input.pci-0000_00_1f.3.analog-stereo")).unwrap();
        assert!(devices[0].is_default);
    }

    #[test]
    fn saved_cpal_device_stays_listed_and_selected() {
        let mut devices = pulse_inputs(SOURCES, None).unwrap();
        keep_selected(&mut devices, Some("cpal:id:alsa:usb"));
        assert_eq!(devices.len(), 2);
        let saved = devices.iter().find(|d| d.id == "cpal:id:alsa:usb").unwrap();
        assert!(saved.is_default && saved.name.contains("no echo cancelling"));
        assert_eq!(devices.iter().filter(|d| d.is_default).count(), 1);
    }

    #[test]
    fn sinks_parse_with_descriptions() {
        let devices = parse_sinks(SINKS).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Built-in Audio Analog Stereo");
    }

    #[test]
    fn missing_description_falls_back_to_name() {
        let json = r#"[{"name":"some.source"}]"#;
        assert_eq!(parse_sources(json).unwrap()[0].name, "some.source");
    }

    #[test]
    fn malformed_output_is_an_error_not_a_panic() {
        assert!(parse_sources("Connection failure").is_err());
        assert!(parse_sinks("{not json").is_err());
    }

    #[test]
    fn default_is_marked_by_name() {
        let mut devices = vec![
            Device {
                id: "a".into(),
                name: "A".into(),
                is_default: false,
            },
            Device {
                id: "b".into(),
                name: "B".into(),
                is_default: false,
            },
        ];
        mark_default(&mut devices, Some("b"));
        assert!(!devices[0].is_default);
        assert!(devices[1].is_default);
    }

    #[test]
    fn unknown_default_falls_back_to_first() {
        let mut devices = vec![Device {
            id: "a".into(),
            name: "A".into(),
            is_default: false,
        }];
        mark_default(&mut devices, Some("missing"));
        // No match: nothing is marked, and the UI shows no default.
        assert!(!devices[0].is_default);
        mark_default(&mut devices, None);
        assert!(devices[0].is_default);
    }

    #[test]
    fn empty_output_is_empty_not_an_error() {
        assert!(parse_sources("[]").unwrap().is_empty());
    }
}
