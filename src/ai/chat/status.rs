//! Per-provider readiness for the Providers tab: ready, not installed, not logged in or no key,
//! with the command that fixes it. Login checks are quick local probes, never a model call.

use std::time::Duration;

use serde_json::{Value, json};
use tokio::process::Command;

use super::cli::on_path;

/// The command (or step) that signs in to `id`.
pub fn login_hint(id: &str) -> &'static str {
    match id {
        "claude-sub" => "claude",
        "agy-sub" => "agy",
        "codex-sub" => "codex login",
        "opencode" => "opencode auth login",
        "ollama-cloud" => "set OLLAMA_API_KEY in the service environment, or add the key here",
        _ => "ollama serve",
    }
}

/// Whether a login check output means signed in: claude prints `"loggedIn": false` and codex
/// `Not logged in`, whatever the exit code; otherwise exit 0 means signed in.
fn logged_in(output: &str, success: bool) -> bool {
    let text = output.to_lowercase();
    if text.contains("not logged in") || text.replace(' ', "").contains(r#""loggedin":false"#) {
        return false;
    }
    success
}

/// Run `program args` for a few seconds at most and read its login state.
async fn signed_in(program: &str, args: &[&str]) -> bool {
    let run = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(Duration::from_secs(5), run).await {
        Ok(Ok(o)) => logged_in(
            &format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ),
            o.status.success(),
        ),
        _ => false,
    }
}

fn home_file(relative: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(relative))
}

/// `opencode auth login` stores its credentials as a JSON object keyed by provider.
fn opencode_signed_in() -> bool {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| home_file(".local/share"));
    data.and_then(|d| std::fs::read_to_string(d.join("opencode/auth.json")).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| v.as_object().map(|o| !o.is_empty()))
        .unwrap_or(false)
}

/// One status row. `has_key` says a key is stored or set in the environment.
pub async fn status(id: &str, name: &str, has_key: bool) -> Value {
    let (cli, installed) = match id {
        "claude-sub" => (Some("claude"), on_path("claude")),
        "agy-sub" => (Some("agy"), on_path("agy")),
        "codex-sub" => (Some("codex"), on_path("codex")),
        "opencode" => (Some("opencode"), on_path("opencode")),
        _ => (None, true),
    };
    let state = if !installed {
        "not_installed"
    } else {
        match id {
            "claude-sub" => ready(
                signed_in("claude", &["auth", "status"]).await,
                "not_logged_in",
            ),
            "codex-sub" => ready(
                signed_in("codex", &["login", "status"]).await,
                "not_logged_in",
            ),
            "agy-sub" => ready(
                home_file(".gemini/oauth_creds.json").is_some_and(|p| p.exists()),
                "not_logged_in",
            ),
            "opencode" => ready(opencode_signed_in(), "not_logged_in"),
            "ollama-cloud" => ready(has_key, "no_key"),
            // Local Ollama: ready when something listens on its port.
            _ => ready(
                tokio::time::timeout(
                    Duration::from_secs(1),
                    tokio::net::TcpStream::connect(ollama_addr()),
                )
                .await
                .is_ok_and(|r| r.is_ok()),
                "not_installed",
            ),
        }
    };
    json!({"id": id, "name": name, "state": state, "login": login_hint(id), "cli": cli})
}

/// `host:port` of the local Ollama; the Nix module may move it.
fn ollama_addr() -> String {
    std::env::var("NIXARCHY_MEETINGAI_OLLAMA_URL")
        .ok()
        .and_then(|u| reqwest::Url::parse(&u).ok())
        .and_then(|u| Some(format!("{}:{}", u.host_str()?, u.port_or_known_default()?)))
        .unwrap_or_else(|| "127.0.0.1:11434".into())
}

fn ready(ok: bool, otherwise: &'static str) -> &'static str {
    if ok { "ready" } else { otherwise }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_provider_without_a_key_says_so_and_names_the_fix() {
        let row = status("ollama-cloud", "Ollama Cloud", false).await;
        assert_eq!(row["state"], "no_key");
        assert!(row["login"].as_str().unwrap().contains("OLLAMA_API_KEY"));
        assert_eq!(
            status("ollama-cloud", "Ollama Cloud", true).await["state"],
            "ready"
        );
    }

    #[test]
    fn not_logged_in_output_wins_over_the_exit_code() {
        assert!(!logged_in("Not logged in\n", true));
        assert!(!logged_in("{\n  \"loggedIn\": false,\n}", true));
        assert!(logged_in("Logged in using ChatGPT", true));
        assert!(logged_in("{\"loggedIn\": true}", true));
        assert!(!logged_in("", false));
    }

    #[test]
    fn every_provider_has_a_login_hint() {
        for p in crate::ai::providers::CHAT_PROVIDERS {
            assert!(!login_hint(p.id).is_empty());
        }
        assert_eq!(login_hint("codex-sub"), "codex login");
        assert_eq!(login_hint("opencode"), "opencode auth login");
    }
}
