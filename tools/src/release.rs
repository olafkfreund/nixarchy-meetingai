//! Publish a checksummed main build; tests use an in-process fake, never GitHub.
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};

const BANNER: &str = "https://swr2.itssingularity.com/lennette/github_release_banner_lennette.png";
const ASSETS: [&str; 5] = [
    "lennette-linux-x86_64.tar.gz",
    "lennette-windows-x86_64.exe",
    "lennette-windows-x86_64.zip",
    "SHA256SUMS-linux-x86_64.txt",
    "SHA256SUMS-windows-x86_64.txt",
];

pub(crate) fn hash(path: &Path) -> Result<String> {
    let mut source = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn validate_assets(directory: &Path) -> Result<()> {
    let actual = fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            ensure!(
                entry.file_type()?.is_file(),
                "Unexpected release directory: {}",
                entry.path().display()
            );
            Ok(entry.file_name().to_string_lossy().into_owned())
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let expected: BTreeSet<_> = ASSETS.iter().map(|s| s.to_string()).collect();
    ensure!(
        actual == expected,
        "Unexpected release assets: missing={:?}, extra={:?}",
        expected.difference(&actual),
        actual.difference(&expected)
    );
    let mut verified = BTreeSet::new();
    for sums in &ASSETS[3..] {
        for line in fs::read_to_string(directory.join(sums))?.lines() {
            let (digest, name) = line
                .split_once(char::is_whitespace)
                .context("Malformed checksum line")?;
            let name = name
                .trim_start()
                .strip_prefix('*')
                .unwrap_or(name.trim_start());
            ensure!(
                ASSETS[..3].contains(&name),
                "Unexpected checksum entry: {name}"
            );
            ensure!(
                verified.insert(name.to_owned()),
                "Duplicate checksum entry: {name}"
            );
            ensure!(
                hash(&directory.join(name))? == digest,
                "Checksum mismatch: {name}"
            );
        }
    }
    ensure!(verified.len() == 3, "Checksum coverage is incomplete");
    Ok(())
}

struct Config {
    repo: String,
    sha: String,
    run: String,
    version: String,
    branch: String,
}
impl Config {
    fn from_env(root: &Path) -> Result<Self> {
        let manifest: toml_edit::DocumentMut =
            fs::read_to_string(root.join("Cargo.toml"))?.parse()?;
        Ok(Self {
            repo: std::env::var("GITHUB_REPOSITORY")?,
            sha: std::env::var("GITHUB_SHA")?,
            run: std::env::var("GITHUB_RUN_NUMBER")?,
            branch: std::env::var("GITHUB_REF")?,
            version: manifest["package"]["version"]
                .as_str()
                .context("Missing app version")?
                .into(),
        })
    }
    fn tag(&self) -> Result<String> {
        ensure!(
            self.branch == "refs/heads/main",
            "Releases may only be published from main"
        );
        ensure!(
            self.sha.len() == 40 && self.sha.bytes().all(|b| b.is_ascii_hexdigit()),
            "Expected a full commit SHA"
        );
        ensure!(
            !self.run.is_empty() && self.run.bytes().all(|b| b.is_ascii_digit()),
            "Invalid build number"
        );
        Ok(format!(
            "v{}-build.{}.{}",
            self.version,
            self.run,
            &self.sha[..7]
        ))
    }
}

fn release_body(config: &Config, tag: &str, notes: &str) -> String {
    let Config { repo, sha, .. } = config;
    let base = format!("https://github.com/{repo}/releases/download/{tag}");
    format!(
        r#"![Lennette]({BANNER})

Native meeting and interview copilot for Linux and Windows.

| Platform | Download |
| --- | --- |
| Linux x86_64 | [Archive]({base}/lennette-linux-x86_64.tar.gz) |
| Windows x86_64 | [Portable EXE]({base}/lennette-windows-x86_64.exe) · [ZIP]({base}/lennette-windows-x86_64.zip) |

Extract the Linux archive and run `./lennette`, or run the Windows EXE directly.
Add chat and speech provider keys in **Settings → AI & Keys**.
See the [README](https://github.com/{repo}/blob/{sha}/.github/README.md) for dependencies,
features, shortcuts, and the optional patched Hyprland setup.

Windows privately launches with the tray off and excludes its overlay/dashboard from
full-desktop capture paths that honor Windows display affinity (Windows 10 2004+ / Windows 11).
Linux capture protection requires the patched Hyprland session and private launcher.
Capture exclusion is not a guarantee of universal undetectability; verify your actual share mode.
SHA-256 checksum files accompany the downloads. Commit: `{sha}`.

{}
"#,
        notes.trim()
    )
}

struct Response {
    ok: bool,
    stdout: String,
    stderr: String,
}
trait Github {
    fn call(&mut self, args: &[&str], input: Option<&str>) -> Result<Response>;
}
struct Gh;
impl Github for Gh {
    fn call(&mut self, args: &[&str], input: Option<&str>) -> Result<Response> {
        let mut child = Command::new("gh")
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Start GitHub CLI")?;
        if let Some(input) = input {
            child
                .stdin
                .take()
                .context("Missing gh stdin")?
                .write_all(input.as_bytes())?;
        }
        let output = child.wait_with_output()?;
        Ok(Response {
            ok: output.status.success(),
            stdout: String::from_utf8(output.stdout)?,
            stderr: String::from_utf8_lossy(&output.stderr).into(),
        })
    }
}
fn checked(github: &mut impl Github, args: &[&str], input: Option<&str>) -> Result<String> {
    let response = github.call(args, input)?;
    ensure!(
        response.ok,
        "gh {} failed: {}",
        args[0],
        response.stderr.trim()
    );
    Ok(response.stdout)
}
#[derive(Deserialize)]
struct Asset {
    name: String,
}
#[derive(Deserialize)]
struct Release {
    draft: bool,
    target_commitish: String,
    assets: Vec<Asset>,
}
fn asset_names(assets: &[Asset]) -> BTreeSet<&str> {
    assets.iter().map(|asset| asset.name.as_str()).collect()
}

fn publish(root: &Path, config: &Config, github: &mut impl Github) -> Result<String> {
    let tag = config.tag()?;
    let directory = root.join("dist");
    validate_assets(&directory)?;
    let repo = &config.repo;
    let endpoint = format!("repos/{repo}/releases/tags/{tag}");
    let probe = github.call(&["api", &endpoint], None)?;
    // `gh api` exits 0 even for a 404; the status only appears in the body and
    // on stderr. Only a genuine 404 means "create it" — anything else (auth,
    // rate limit, network) must fail rather than silently publish.
    let status = serde_json::from_str::<serde_json::Value>(&probe.stdout)
        .ok()
        .and_then(|v| v.get("status").cloned());
    let not_found = status
        .as_ref()
        .is_some_and(|s| s == "404" || s == &serde_json::json!(404))
        || probe.stderr.contains("HTTP 404");
    ensure!(
        not_found || probe.ok,
        "Cannot inspect existing release: {}",
        probe.stderr.trim()
    );
    let missing = not_found;

    let exists = if missing {
        false
    } else {
        let existing: Release = serde_json::from_str(&probe.stdout).with_context(|| {
            format!(
                "Unexpected response inspecting release: {}",
                probe.stdout.trim()
            )
        })?;
        ensure!(
            existing.target_commitish == config.sha,
            "Existing release targets a different commit"
        );
        if !existing.draft {
            ensure!(
                asset_names(&existing.assets) == ASSETS.into_iter().collect(),
                "Published release is missing expected assets"
            );
            return Ok(format!(
                "Release {tag} is already published; preserving its assets."
            ));
        }
        true
    };
    let generated = checked(
        github,
        &[
            "api",
            &format!("repos/{repo}/releases/generate-notes"),
            "-f",
            &format!("tag_name={tag}"),
            "-f",
            &format!("target_commitish={}", config.sha),
        ],
        None,
    )?;
    let generated: serde_json::Value = serde_json::from_str(&generated)?;
    let body = release_body(
        config,
        &tag,
        generated["body"]
            .as_str()
            .context("Missing generated notes")?,
    );
    if exists {
        checked(
            github,
            &["release", "edit", &tag, "--repo", repo, "--notes-file", "-"],
            Some(&body),
        )?;
    } else {
        checked(
            github,
            &[
                "release",
                "create",
                &tag,
                "--repo",
                repo,
                "--target",
                &config.sha,
                "--draft",
                "--title",
                &format!("Lennette {} · build {}", config.version, config.run),
                "--notes-file",
                "-",
            ],
            Some(&body),
        )?;
    }
    let paths: Vec<String> = ASSETS
        .iter()
        .map(|n| directory.join(n).to_string_lossy().into_owned())
        .collect();
    let mut args = vec!["release", "upload", &tag, "--repo", repo, "--clobber"];
    args.extend(paths.iter().map(String::as_str));
    checked(github, &args, None)?;
    let uploaded = checked(github, &["api", &endpoint], None)?;
    let uploaded: Release = serde_json::from_str(&uploaded)?;
    ensure!(
        uploaded.draft
            && uploaded.target_commitish == config.sha
            && asset_names(&uploaded.assets) == ASSETS.into_iter().collect(),
        "Upload incomplete; leaving release as a draft"
    );
    checked(
        github,
        &[
            "release",
            "edit",
            &tag,
            "--repo",
            repo,
            "--draft=false",
            "--latest",
        ],
        None,
    )?;
    Ok(format!(
        "Published https://github.com/{repo}/releases/tag/{tag}"
    ))
}

pub fn run(root: &Path, args: &[String]) -> Result<()> {
    if !args.is_empty() {
        bail!("Usage: cargo xtask release (requires GitHub Actions environment and gh)");
    }
    println!("{}", publish(root, &Config::from_env(root)?, &mut Gh)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct Fake {
        calls: Vec<Vec<String>>,
        inputs: Vec<String>,
        existing: Option<serde_json::Value>,
        incomplete: bool,
        denied: bool,
        fail_upload: bool,
    }
    impl Github for Fake {
        fn call(&mut self, args: &[&str], input: Option<&str>) -> Result<Response> {
            self.calls
                .push(args.iter().map(|s| s.to_string()).collect());
            if let Some(input) = input {
                self.inputs.push(input.into());
            }
            let mut response = Response {
                ok: true,
                stdout: String::new(),
                stderr: String::new(),
            };
            if args[0] == "api" && args[1].contains("/tags/") {
                if self.calls.len() == 1 {
                    if let Some(existing) = &self.existing {
                        response.stdout = existing.to_string();
                    } else if self.denied {
                        // A real 403 makes gh exit non-zero.
                        response.ok = false;
                        response.stdout = json!({"status": "403"}).to_string();
                        response.stderr = "API unavailable".into();
                    } else {
                        // Real gh exits 0 on a 404 and prints the error body to
                        // stdout, which is what broke publishing previously.
                        response.ok = true;
                        response.stdout =
                            json!({"message": "Not Found", "status": "404"}).to_string();
                        response.stderr = "gh: Not Found (HTTP 404)".into();
                    }
                } else {
                    response.stdout = json!({"draft": true, "target_commitish": "a".repeat(40), "assets": if self.incomplete { vec![] } else { ASSETS.iter().map(|n| json!({"name":n})).collect() }}).to_string();
                }
            } else if args[0] == "api" {
                response.stdout = json!({"body":"Generated changes"}).to_string();
            } else if args[1] == "upload" && self.fail_upload {
                response.ok = false;
                response.stderr = "Upload failed".into();
            }
            Ok(response)
        }
    }
    fn fixture() -> (tempfile::TempDir, Config, Fake) {
        let temp = tempfile::tempdir().unwrap();
        let dist = temp.path().join("dist");
        fs::create_dir(&dist).unwrap();
        for name in &ASSETS[..3] {
            fs::write(dist.join(name), name).unwrap();
        }
        for (manifest, names) in [(ASSETS[3], &ASSETS[..1]), (ASSETS[4], &ASSETS[1..3])] {
            fs::write(
                dist.join(manifest),
                names
                    .iter()
                    .map(|name| format!("{}  {name}\n", hash(&dist.join(name)).unwrap()))
                    .collect::<String>(),
            )
            .unwrap();
        }
        (
            temp,
            Config {
                repo: "example/lennette".into(),
                sha: "a".repeat(40),
                run: "42".into(),
                version: "0.10.0".into(),
                branch: "refs/heads/main".into(),
            },
            Fake {
                calls: vec![],
                inputs: vec![],
                existing: None,
                incomplete: false,
                denied: false,
                fail_upload: false,
            },
        )
    }
    #[test]
    fn publishes_only_after_verified_upload_with_exact_banner_and_links() {
        let (temp, config, mut github) = fixture();
        publish(temp.path(), &config, &mut github).unwrap();
        assert!(github.calls[2].contains(&"--draft".into()));
        assert_eq!(
            &github.calls.last().unwrap()[5..],
            ["--draft=false", "--latest"]
        );
        let body = &github.inputs[0];
        assert!(body.starts_with(&format!("![Lennette]({BANNER})")));
        assert!(body.contains(&format!("/blob/{}/.github/README.md", config.sha)));
        for name in &ASSETS[..3] {
            assert!(body.contains(&format!(
                "/releases/download/v0.10.0-build.42.aaaaaaa/{name}"
            )));
        }
    }
    #[test]
    fn invalid_artifacts_block_all_remote_calls() {
        for kind in [
            "damaged",
            "missing",
            "extra",
            "coverage",
            "traversal",
            "duplicate",
        ] {
            let (temp, config, mut github) = fixture();
            let dist = temp.path().join("dist");
            match kind {
                "damaged" => fs::write(dist.join(ASSETS[1]), b"damaged").unwrap(),
                "missing" => fs::remove_file(dist.join(ASSETS[0])).unwrap(),
                "extra" => fs::write(dist.join("extra"), b"").unwrap(),
                "coverage" => fs::write(dist.join(ASSETS[4]), b"").unwrap(),
                "traversal" => fs::write(dist.join(ASSETS[4]), "abc  ../outside\n").unwrap(),
                _ => fs::write(
                    dist.join(ASSETS[4]),
                    fs::read(dist.join(ASSETS[3])).unwrap(),
                )
                .unwrap(),
            }
            assert!(
                publish(temp.path(), &config, &mut github).is_err(),
                "{kind}"
            );
            assert!(github.calls.is_empty());
        }
    }
    #[test]
    fn only_main_can_publish_and_auth_failures_do_not_create_releases() {
        let (temp, mut config, mut github) = fixture();
        config.branch = "refs/heads/feature".into();
        assert!(publish(temp.path(), &config, &mut github).is_err());
        assert!(github.calls.is_empty());
        config.branch = "refs/heads/main".into();
        github.denied = true;
        assert!(publish(temp.path(), &config, &mut github).is_err());
        assert_eq!(github.calls.len(), 1);
    }
    #[test]
    fn published_reruns_preserve_assets_and_mismatched_commits_fail() {
        let (temp, config, mut github) = fixture();
        github.existing = Some(
            json!({"draft":false,"target_commitish":config.sha,"assets":ASSETS.iter().map(|n|json!({"name":n})).collect::<Vec<_>>()}),
        );
        assert!(
            publish(temp.path(), &config, &mut github)
                .unwrap()
                .contains("already published")
        );
        assert_eq!(github.calls.len(), 1);
        github.calls.clear();
        github.existing.as_mut().unwrap()["target_commitish"] = json!("b".repeat(40));
        assert!(publish(temp.path(), &config, &mut github).is_err());
        assert_eq!(github.calls.len(), 1);
    }
    #[test]
    fn partial_uploads_stay_draft_and_draft_reruns_resume() {
        for fail_request in [false, true] {
            let (temp, config, mut github) = fixture();
            github.incomplete = true;
            github.fail_upload = fail_request;
            assert!(publish(temp.path(), &config, &mut github).is_err());
            assert!(
                !github
                    .calls
                    .iter()
                    .flatten()
                    .any(|arg| arg == "--draft=false")
            );
        }
        let (temp, config, mut github) = fixture();
        github.existing = Some(json!({"draft":true,"target_commitish":config.sha,"assets":[]}));
        publish(temp.path(), &config, &mut github).unwrap();
        assert_eq!(github.calls[2][1], "edit");
    }
}
