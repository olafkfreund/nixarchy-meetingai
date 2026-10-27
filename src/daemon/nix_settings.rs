//! Settings declared in Nix (`programs.nixarchy-meetingai.settings`).
//!
//! The Home Manager module writes `settings.json`; at start the daemon applies it.
//! Plain values are defaults, written once per (key, value); with `enforce` they are
//! written at every start and the app refuses to change them.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use rusqlite::OptionalExtension;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    ai::providers::{ChatProvider, SttProvider},
    core::models::settings::{AutoResponse, MeetingMode, ResponseLength, ResponsePace, Settings},
    db::{Database, Result, settings as store},
};

#[derive(Debug, Default, Deserialize)]
pub struct NixSettings {
    #[serde(default)]
    pub enforce: bool,
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

pub fn path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| {
        d.config_dir()
            .join("nixarchy-meetingai")
            .join("settings.json")
    })
}

/// `None` when there is no file (not a Nix install) or it cannot be read.
pub fn read(path: &Path) -> Option<NixSettings> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text)
        .inspect_err(|error| tracing::warn!(%error, "ignoring unreadable Nix settings file"))
        .ok()
}

/// The same rules as `settings.set` (a known key) plus a check on the typed values.
fn valid(key: &str, value: &str) -> bool {
    Settings::is_key(key)
        && match key {
            "meeting_mode" => MeetingMode::parse(value).is_some(),
            "response_pace" => ResponsePace::ALL.iter().any(|v| v.label() == value),
            "response_length" => ResponseLength::ALL.iter().any(|v| v.label() == value),
            "auto_response" => AutoResponse::ALL.iter().any(|v| v.as_str() == value),
            "smart_follow_ups" | "auto_record_on_call" | "reduce_motion" | "mcp_allow_record" => {
                matches!(value, "true" | "false")
            }
            "chat_provider" | "live_provider" => {
                ChatProvider::by_id(value).is_some() || value.starts_with("custom-")
            }
            "speech_provider" => SttProvider::by_id(value).is_some(),
            "text_scale" => value.parse::<u32>().is_ok_and(|n| (80..=160).contains(&n)),
            _ => true,
        }
}

fn hash(value: &str) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Write the settings; returns the keys the app must treat as locked.
pub fn apply(db: &Database, nix: &NixSettings) -> Result<Vec<String>> {
    let c = db.connection();
    c.execute(
        "CREATE TABLE IF NOT EXISTS settings_seeded (key TEXT PRIMARY KEY, hash TEXT NOT NULL)",
        [],
    )?;
    let mut locked = Vec::new();
    for (key, value) in &nix.values {
        if !valid(key, value) {
            tracing::warn!(key, "skipping invalid Nix setting");
            continue;
        }
        if nix.enforce {
            store::set(c, key, value)?;
            locked.push(key.clone());
            continue;
        }
        let now = hash(value);
        let seeded: Option<String> = c
            .query_row(
                "SELECT hash FROM settings_seeded WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        if seeded.as_deref() == Some(&now) {
            continue;
        }
        // A value already in the app wins over a first default; a changed Nix value is applied once more.
        if seeded.is_some() || store::get(c, key)?.is_none() {
            store::set(c, key, value)?;
        }
        c.execute(
            "INSERT OR REPLACE INTO settings_seeded (key, hash) VALUES (?1, ?2)",
            [key, &now],
        )?;
    }
    Ok(locked)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nix(enforce: bool, values: &[(&str, &str)]) -> NixSettings {
        NixSettings {
            enforce,
            values: values
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
        }
    }

    fn get(db: &Database, key: &str) -> Option<String> {
        store::get(db.connection(), key).unwrap()
    }

    #[test]
    fn defaults_apply_once_and_the_app_wins_afterwards() {
        let db = Database::open_in_memory().unwrap();
        let n = nix(false, &[("chat_model", "opus")]);
        assert!(apply(&db, &n).unwrap().is_empty());
        assert_eq!(get(&db, "chat_model").as_deref(), Some("opus"));
        store::set(db.connection(), "chat_model", "haiku").unwrap();
        apply(&db, &n).unwrap();
        assert_eq!(get(&db, "chat_model").as_deref(), Some("haiku"));
    }

    #[test]
    fn a_value_already_in_the_app_beats_a_first_default() {
        let db = Database::open_in_memory().unwrap();
        store::set(db.connection(), "chat_model", "haiku").unwrap();
        apply(&db, &nix(false, &[("chat_model", "opus")])).unwrap();
        assert_eq!(get(&db, "chat_model").as_deref(), Some("haiku"));
    }

    #[test]
    fn a_changed_nix_value_is_applied_once_more() {
        let db = Database::open_in_memory().unwrap();
        apply(&db, &nix(false, &[("chat_model", "opus")])).unwrap();
        store::set(db.connection(), "chat_model", "haiku").unwrap();
        apply(&db, &nix(false, &[("chat_model", "sonnet")])).unwrap();
        assert_eq!(get(&db, "chat_model").as_deref(), Some("sonnet"));
        store::set(db.connection(), "chat_model", "haiku").unwrap();
        apply(&db, &nix(false, &[("chat_model", "sonnet")])).unwrap();
        assert_eq!(get(&db, "chat_model").as_deref(), Some("haiku"));
    }

    #[test]
    fn enforce_rewrites_every_start_and_locks() {
        let db = Database::open_in_memory().unwrap();
        let n = nix(true, &[("text_scale", "120")]);
        assert_eq!(apply(&db, &n).unwrap(), ["text_scale"]);
        store::set(db.connection(), "text_scale", "90").unwrap();
        apply(&db, &n).unwrap();
        assert_eq!(get(&db, "text_scale").as_deref(), Some("120"));
    }

    #[test]
    fn invalid_keys_and_values_are_skipped() {
        let db = Database::open_in_memory().unwrap();
        let n = nix(
            true,
            &[
                ("api_key", "fake-key"),
                ("no_such_setting", "x"),
                ("meeting_mode", "party"),
                ("text_scale", "500"),
                ("chat_provider", "nope"),
                ("live_provider", "custom-abc"),
                ("speech_provider", "nope"),
                ("reduce_motion", "true"),
            ],
        );
        assert_eq!(apply(&db, &n).unwrap(), ["live_provider", "reduce_motion"]);
        assert_eq!(get(&db, "chat_provider"), None);
        assert_eq!(get(&db, "api_key"), None);
        assert_eq!(get(&db, "meeting_mode"), None);
    }

    #[test]
    fn the_enum_file_matches_the_rust_types() {
        let from_types = serde_json::json!({
            "meetingMode": ["general", "interview", "team_meeting", "interviewer"],
            "responsePace": ResponsePace::ALL.map(ResponsePace::label),
            "responseLength": ResponseLength::ALL.map(ResponseLength::label),
            "autoResponse": AutoResponse::ALL.map(AutoResponse::as_str),
        });
        let file: serde_json::Value =
            serde_json::from_str(include_str!("../../nix/settings-enums.json")).unwrap();
        assert_eq!(file, from_types);
        for m in file["meetingMode"].as_array().unwrap() {
            assert!(MeetingMode::parse(m.as_str().unwrap()).is_some());
        }
    }

    #[test]
    fn the_full_example_fixture_loads() {
        let nix: NixSettings =
            serde_json::from_str(include_str!("../../tests/fixtures/nix-settings.json")).unwrap();
        assert!(nix.enforce);
        let db = Database::open_in_memory().unwrap();
        let locked = apply(&db, &nix).unwrap();
        assert_eq!(locked.len(), nix.values.len(), "every fixture key is valid");
        assert_eq!(get(&db, "meeting_mode").as_deref(), Some("interviewer"));
        assert_eq!(get(&db, "response_pace").as_deref(), Some("Fast"));
    }
}
