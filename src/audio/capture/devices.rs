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
pub fn enumerate() -> Devices {
    #[cfg(windows)]
    let result = enumerate_windows();
    #[cfg(not(windows))]
    let result = enumerate_linux();
    match result {
        Ok(devices) => devices,
        Err(error) => Devices {
            error: Some(error),
            ..Devices::default()
        },
    }
}

#[cfg(not(windows))]
fn pactl(args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("pactl")
        .args(args)
        .output()
        .map_err(|error| format!("pactl is unavailable: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("pactl failed: {}", stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(not(windows))]
fn enumerate_linux() -> Result<Devices, String> {
    // Use the same backend and identifiers as Microphone::start. Pulse source
    // names cannot be passed to CPAL's ALSA device selector.
    let inputs = crate::audio::microphone::input_devices()
        .into_iter()
        .map(|d| Device {
            id: d.id,
            name: d.name,
            is_default: d.is_default,
        })
        .collect();
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

#[cfg(windows)]
fn enumerate_windows() -> Result<Devices, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let inputs = crate::audio::microphone::input_devices()
        .into_iter()
        .map(|d| Device {
            id: d.id,
            name: d.name,
            is_default: d.is_default,
        })
        .collect();
    let default = host
        .default_output_device()
        .and_then(|device| device.id().ok());
    let devices = match host.output_devices() {
        Ok(devices) => devices,
        Err(error) => {
            return Ok(Devices {
                inputs,
                outputs: Vec::new(),
                error: Some(error.to_string()),
            });
        }
    };
    let outputs = devices
        .filter_map(|device| {
            let name = device.description().ok()?.name().to_owned();
            let id = device.id().ok()?;
            Some(Device {
                id: format!("cpal:id:{id}"),
                is_default: default.as_ref() == Some(&id),
                name,
            })
        })
        .collect();
    Ok(Devices {
        inputs,
        outputs,
        error: None,
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
