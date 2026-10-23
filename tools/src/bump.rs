//! Semver updates preserve manifest formatting and synchronise the workspace lock.
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use std::{fs, path::Path, process::Command};
use toml_edit::{DocumentMut, value};

fn next_version(version: &str, mode: &str, message: &str) -> Result<String> {
    let numbers: Vec<u64> = version
        .split('-')
        .next()
        .unwrap()
        .split('.')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        numbers.len() == 3,
        "Expected a three-part version: {version}"
    );
    let (mut major, mut minor, mut patch) = (numbers[0], numbers[1], numbers[2]);
    let kind = match mode {
        "major" | "minor" | "patch" => mode,
        "auto"
            if Regex::new(r"(?im)BREAKING[ -]CHANGE|^[a-z0-9_]+(?:\([a-z0-9_-]+\))?!:")?
                .is_match(message) =>
        {
            "major"
        }
        "auto" if Regex::new(r"(?im)^feat(?:\([a-z0-9_-]+\))?:")?.is_match(message) => "minor",
        "auto" => "patch",
        _ => bail!("Unknown bump mode: {mode}"),
    };
    match kind {
        "major" => {
            major = major.checked_add(1).context("Major version overflow")?;
            minor = 0;
            patch = 0;
        }
        "minor" => {
            minor = minor.checked_add(1).context("Minor version overflow")?;
            patch = 0;
        }
        _ => patch = patch.checked_add(1).context("Patch version overflow")?,
    }
    Ok(format!("{major}.{minor}.{patch}"))
}

fn update(root: &Path, mode: &str, message: &str) -> Result<(String, String)> {
    let manifest_path = root.join("Cargo.toml");
    let lock_path = root.join("Cargo.lock");
    let manifest_text = fs::read_to_string(&manifest_path)?;
    let lock_text = fs::read_to_string(&lock_path)?;
    let mut manifest: DocumentMut = manifest_text.parse()?;
    let mut lock: DocumentMut = lock_text.parse()?;
    let old = manifest["package"]["version"]
        .as_str()
        .context("Missing app version")?
        .to_owned();
    let new = next_version(&old, mode, message)?;
    let package = lock["package"]
        .as_array_of_tables_mut()
        .context("Missing lock packages")?
        .iter_mut()
        .find(|p| p["name"].as_str() == Some("lennette") && !p.contains_key("source"))
        .context("App is missing from Cargo.lock")?;
    ensure!(
        package["version"].as_str() == Some(old.as_str()),
        "Manifest/lock app versions disagree; refusing to bump"
    );
    package["version"] = value(&new);
    manifest["package"]["version"] = value(&new);
    // Both documents are parsed and validated before either file is changed.
    fs::write(&lock_path, lock.to_string())?;
    if let Err(error) = fs::write(&manifest_path, manifest.to_string()) {
        fs::write(&lock_path, lock_text).context("Cannot roll back lockfile")?;
        return Err(error.into());
    }
    Ok((old, new))
}

pub fn run(root: &Path, args: &[String]) -> Result<()> {
    ensure!(
        args.len() <= 2,
        "Usage: cargo xtask bump [patch|minor|major|auto] [message or message-file]"
    );
    let mode = args.first().map(String::as_str).unwrap_or("auto");
    let message = match args.get(1) {
        Some(input) if Path::new(input).is_file() => fs::read_to_string(input)?,
        Some(input) => input.clone(),
        None => String::new(),
    };
    let (old, new) = update(root, mode, &message)?;
    let status = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .current_dir(root)
        .args(["add", "--", "Cargo.toml", "Cargo.lock"])
        .status()?;
    ensure!(
        status.success(),
        "Version updated, but git could not stage the manifests"
    );
    println!("lennette version bumped: {old} -> {new}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_file_bump_stages_both_manifests_in_an_isolated_repository() {
        let temp = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env_remove("GIT_INDEX_FILE")
                .args(["init", "--quiet"])
                .current_dir(temp.path())
                .status()
                .unwrap()
                .success()
        );
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"lennette\"\nversion = \"0.10.0\"\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("Cargo.lock"),
            "version = 4\n[[package]]\nname = \"lennette\"\nversion = \"0.10.0\"\n",
        )
        .unwrap();
        let message = temp.path().join("commit message.txt");
        fs::write(&message, "feat(profiles): add context").unwrap();
        run(
            temp.path(),
            &["auto".into(), message.to_string_lossy().into_owned()],
        )
        .unwrap();
        for file in ["Cargo.toml", "Cargo.lock"] {
            let output = Command::new("git")
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env_remove("GIT_INDEX_FILE")
                .args(["show", &format!(":{file}")])
                .current_dir(temp.path())
                .output()
                .unwrap();
            assert!(output.status.success());
            assert!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .contains("version = \"0.11.0\"")
            );
        }
    }
    #[test]
    fn semver_and_conventional_commits() {
        for (mode, message, expected) in [
            ("patch", "", "1.2.4"),
            ("minor", "", "1.3.0"),
            ("major", "", "2.0.0"),
            ("auto", "feat(ui): add profiles", "1.3.0"),
            ("auto", "fix: bug", "1.2.4"),
            ("auto", "feat(ui)!: changed\nbody", "2.0.0"),
            ("auto", "fix: x\nBREAKING CHANGE: y", "2.0.0"),
        ] {
            assert_eq!(next_version("1.2.3", mode, message).unwrap(), expected);
        }
        assert!(next_version("1.2", "patch", "").is_err());
        assert!(next_version("1.2.3", "typo", "").is_err());
        assert!(next_version("1.2.18446744073709551615", "patch", "").is_err());
    }
    #[test]
    fn updates_only_app_version_and_preserves_comments_and_other_packages() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("Cargo.toml"),
            "# keep\n[package]\nname = \"lennette\"\nversion = \"0.10.0\"\n",
        )
        .unwrap();
        fs::write(temp.path().join("Cargo.lock"), "version = 4\n[[package]]\nname = \"lennette\"\nversion = \"0.10.0\"\n[[package]]\nname = \"lennette-tools\"\nversion = \"0.1.0\"\n").unwrap();
        assert_eq!(update(temp.path(), "minor", "").unwrap().1, "0.11.0");
        let manifest = fs::read_to_string(temp.path().join("Cargo.toml")).unwrap();
        assert!(manifest.starts_with("# keep\n"));
        let lock = fs::read_to_string(temp.path().join("Cargo.lock")).unwrap();
        assert!(lock.contains("name = \"lennette-tools\"\nversion = \"0.1.0\""));
        // A stale lockfile fails before touching either document.
        fs::write(
            temp.path().join("Cargo.toml"),
            manifest.replace("0.11.0", "0.12.0"),
        )
        .unwrap();
        let before = fs::read(temp.path().join("Cargo.toml")).unwrap();
        assert!(update(temp.path(), "patch", "").is_err());
        assert_eq!(fs::read(temp.path().join("Cargo.toml")).unwrap(), before);
        assert_eq!(
            fs::read_to_string(temp.path().join("Cargo.lock")).unwrap(),
            lock
        );
    }
}
