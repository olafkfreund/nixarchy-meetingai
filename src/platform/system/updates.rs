//! Read-only update discovery from this project's public GitHub releases.
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
}

fn version(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.trim().trim_start_matches('v');
    // Main builds are published as v0.18.10-build.17.9512829; this is
    // nixarchy-meetingai's stable release convention, not a beta channel.
    let value = if let Some((version, build)) = value.split_once("-build.") {
        let (number, commit) = build.split_once('.')?;
        number.parse::<u64>().ok()?;
        if commit.len() < 7 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        version
    } else {
        value
    };
    let mut parts = value.split('.');
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
}

pub fn newer(current: &str, available: &str) -> Result<bool, String> {
    Ok(
        version(available).ok_or("GitHub returned an unrecognized release version.")?
            > version(current).ok_or("The installed version is not recognized.")?,
    )
}

pub async fn latest() -> Result<Release, String> {
    crate::core::install_tls_provider();
    let response = reqwest::Client::new()
        .get("https://api.github.com/repos/parazeeknova/nixarchy-meetingai/releases/latest")
        .header(
            "User-Agent",
            concat!("nixarchy-meetingai/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|_| "Could not reach GitHub. Check your connection and try again.")?;
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            404 => "No public release is available yet.".into(),
            403 | 429 => "GitHub's update limit was reached. Try again later.".into(),
            code => format!("GitHub returned HTTP {code}. Try again later."),
        });
    }
    let bytes = crate::core::network::read_body(response, 1024 * 1024)
        .await
        .map_err(|_| "GitHub returned an unreadable release.")?;
    let release: Release =
        serde_json::from_slice(&bytes).map_err(|_| "GitHub returned an unreadable release.")?;
    if release.draft || release.prerelease {
        return Err("No stable public release is available.".into());
    }
    Ok(release)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compares_versions_numerically_and_rejects_ambiguous_tags() {
        assert!(newer("0.18.9", "v0.18.10").unwrap());
        assert!(!newer("0.18.11", "v0.18.11").unwrap());
        assert!(!newer("0.18.11", "v0.17.99").unwrap());
        assert!(newer("0.18.11", "latest").is_err());
        assert!(newer("0.18.11", "v0.19.0-beta").is_err());
        assert!(!newer("0.18.11", "v0.18.10-build.17.9512829").unwrap());
        assert!(newer("0.18.11", "v0.18.12-build.18.00f918d").unwrap());
    }
}
