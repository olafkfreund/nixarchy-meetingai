use crate::audio::system_audio::SystemAudio;
/// The echo-cancelled virtual source written by the Home Manager module.
const ECHO_CANCEL_SOURCE: &str = "meetingai-echo-cancel-source";

/// Which capture path serves a requested input device.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Backend {
    /// CPAL, with the requested device (None = host default).
    Cpal(Option<String>),
    /// `parec` on a Pulse/PipeWire source name.
    Pulse(String),
}

/// Pick the capture path. `sources` are the non-monitor Pulse source names.
///
/// An explicit `cpal:` device keeps CPAL. Otherwise the echo-cancelled source is
/// the default when present, then the server's default source. With no Pulse
/// server (no sources) everything stays on CPAL, as before.
fn choose_backend(requested: Option<&str>, sources: &[String]) -> Backend {
    let requested = requested
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != "default");
    match requested {
        Some(name) if name.starts_with("cpal:") => Backend::Cpal(Some(name.to_owned())),
        // A bare name that is not a Pulse source is a legacy CPAL device name.
        Some(name) if sources.iter().any(|source| source == name) => {
            Backend::Pulse(name.to_owned())
        }
        Some(name) => Backend::Cpal(Some(name.to_owned())),
        None if sources.is_empty() => Backend::Cpal(None),
        None if sources.iter().any(|source| source == ECHO_CANCEL_SOURCE) => {
            Backend::Pulse(ECHO_CANCEL_SOURCE.to_owned())
        }
        None => Backend::Pulse("@DEFAULT_SOURCE@".to_owned()),
    }
}

/// How long a Pulse source may stay silent-of-data before falling back.
const FIRST_SAMPLES_WAIT: Duration = Duration::from_millis(500);

/// Open `source`; if it delivers nothing within `wait` (present but not driven),
/// reopen on `@DEFAULT_SOURCE@`. Returns the handle and the source really used.
fn open_with_fallback<T>(
    source: &str,
    open: impl Fn(&str) -> Result<T, AudioError>,
    has_samples: impl Fn(&T) -> bool,
    wait: Duration,
) -> Result<(T, String), AudioError> {
    const DEFAULT: &str = "@DEFAULT_SOURCE@";
    let first = open(source)?;
    let deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < deadline {
        if has_samples(&first) {
            return Ok((first, source.to_owned()));
        }
        thread::sleep(Duration::from_millis(20));
    }
    if has_samples(&first) || source == DEFAULT {
        return Ok((first, source.to_owned()));
    }
    tracing::warn!(
        source,
        "microphone source delivered no audio; using the default source"
    );
    drop(first);
    Ok((open(DEFAULT)?, DEFAULT.to_owned()))
}

/// Non-monitor Pulse source names, empty when there is no server.
fn pulse_source_names() -> Vec<String> {
    super::devices::pactl(&["--format=json", "list", "sources"])
        .ok()
        .and_then(|text| super::devices::parse_sources(&text).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|device| device.id)
        .collect()
}

#[cfg(test)] mod tests { use super::*;
    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn echo_cancel_source_is_the_default_when_present() {
        let sources = names(&["alsa_input.pci", ECHO_CANCEL_SOURCE]);
        assert_eq!(
            choose_backend(None, &sources),
            Backend::Pulse(ECHO_CANCEL_SOURCE.into())
        );
        assert_eq!(
            choose_backend(Some("default"), &sources),
            Backend::Pulse(ECHO_CANCEL_SOURCE.into())
        );
    }

    #[test]
    fn default_source_is_used_without_echo_cancel() {
        assert_eq!(
            choose_backend(None, &names(&["alsa_input.pci"])),
            Backend::Pulse("@DEFAULT_SOURCE@".into())
        );
    }

    #[test]
    fn no_pulse_server_stays_on_cpal() {
        assert_eq!(choose_backend(None, &[]), Backend::Cpal(None));
    }

    #[test]
    fn explicit_cpal_device_keeps_cpal() {
        let sources = names(&[ECHO_CANCEL_SOURCE]);
        assert_eq!(
            choose_backend(Some("cpal:id:alsa:x"), &sources),
            Backend::Cpal(Some("cpal:id:alsa:x".into()))
        );
    }

    #[test]
    fn named_source_is_pulse_and_unknown_bare_name_is_legacy_cpal() {
        let sources = names(&["alsa_input.usb", ECHO_CANCEL_SOURCE]);
        assert_eq!(
            choose_backend(Some("alsa_input.usb"), &sources),
            Backend::Pulse("alsa_input.usb".into())
        );
        assert_eq!(
            choose_backend(Some("Old Device"), &sources),
            Backend::Cpal(Some("Old Device".into()))
        );
    }

    #[test]
    fn silent_source_falls_back_to_the_default_source() {
        use std::cell::RefCell;
        let opened = RefCell::new(Vec::new());
        let (handle, used) = open_with_fallback(
            ECHO_CANCEL_SOURCE,
            |name| {
                opened.borrow_mut().push(name.to_owned());
                Ok(name.to_owned())
            },
            |name: &String| name == "@DEFAULT_SOURCE@",
            Duration::from_millis(60),
        )
        .unwrap();
        assert_eq!(
            (handle.as_str(), used.as_str()),
            ("@DEFAULT_SOURCE@", "@DEFAULT_SOURCE@")
        );
        assert_eq!(*opened.borrow(), [ECHO_CANCEL_SOURCE, "@DEFAULT_SOURCE@"]);
    }

    #[test]
    fn live_source_is_kept() {
        let (_, used) = open_with_fallback(
            ECHO_CANCEL_SOURCE,
            |name| Ok(name.to_owned()),
            |_: &String| true,
            Duration::from_millis(60),
        )
        .unwrap();
        assert_eq!(used, ECHO_CANCEL_SOURCE);
    } }
