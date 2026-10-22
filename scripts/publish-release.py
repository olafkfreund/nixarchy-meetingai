#!/usr/bin/env python3
"""Publish a complete, checksummed main build. Reruns reuse its existing tag."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib

BANNER = "https://swr2.itssingularity.com/lennette/github_release_banner_lennette.png"
ASSETS = (
    "lennette-linux-x86_64.tar.gz",
    "lennette-windows-x86_64.exe",
    "lennette-windows-x86_64.zip",
    "SHA256SUMS-linux-x86_64.txt",
    "SHA256SUMS-windows-x86_64.txt",
)


def gh(*args):
    result = subprocess.run(["gh", *args], text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(f"gh {args[0]} failed: {result.stderr.strip()}")
    return result.stdout


def release_body(repo, tag, sha, notes):
    base = f"https://github.com/{repo}/releases/download/{tag}"
    return f"""![Lennette]({BANNER})

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

{notes.strip()}
"""


def validate_assets(directory):
    actual = {path.name for path in directory.iterdir() if path.is_file()}
    if actual != set(ASSETS):
        raise RuntimeError(f"Unexpected release assets: missing={set(ASSETS) - actual}, extra={actual - set(ASSETS)}")
    verified = set()
    for sums in directory.glob("SHA256SUMS-*.txt"):
        for line in sums.read_text().splitlines():
            digest, name = line.split(maxsplit=1)
            name = name.lstrip("*")
            if name not in ASSETS or name.startswith("SHA256SUMS-"):
                raise RuntimeError(f"Unexpected checksum entry: {name}")
            path = directory / name
            with path.open("rb") as source:
                actual_digest = hashlib.file_digest(source, "sha256").hexdigest()
            if actual_digest != digest:
                raise RuntimeError(f"Checksum mismatch: {name}")
            verified.add(name)
    if verified != {name for name in ASSETS if not name.startswith("SHA256SUMS-")}:
        raise RuntimeError("Checksum coverage is incomplete")


def main():
    if os.environ.get("GITHUB_REF") != "refs/heads/main":
        raise RuntimeError("Releases may only be published from main")
    repo = os.environ["GITHUB_REPOSITORY"]
    sha = os.environ["GITHUB_SHA"]
    run = os.environ["GITHUB_RUN_NUMBER"]
    version = tomllib.loads(Path("Cargo.toml").read_text())["package"]["version"]
    tag = f"v{version}-build.{run}.{sha[:7]}"
    validate_assets(Path("dist"))
    existing = subprocess.run(["gh", "release", "view", tag, "--repo", repo,
        "--json", "isDraft,targetCommitish,assets"], text=True, capture_output=True)
    if existing.returncode == 0:
        release = json.loads(existing.stdout)
        if release["targetCommitish"] != sha:
            raise RuntimeError("Existing release targets a different commit")
        if not release["isDraft"]:
            if {asset["name"] for asset in release["assets"]} != set(ASSETS):
                raise RuntimeError("Published release is missing expected assets")
            print(f"Release {tag} is already published; preserving its assets.")
            return
    notes = json.loads(gh("api", f"repos/{repo}/releases/generate-notes",
        "-f", f"tag_name={tag}", "-f", f"target_commitish={sha}"))["body"]
    with tempfile.NamedTemporaryFile(mode="w", suffix=".md", encoding="utf-8") as body:
        body.write(release_body(repo, tag, sha, notes))
        body.flush()
        if existing.returncode != 0:
            gh("release", "create", tag, "--repo", repo, "--target", sha, "--draft",
                "--title", f"Lennette {version} · build {run}", "--notes-file", body.name)
        else:
            gh("release", "edit", tag, "--repo", repo, "--notes-file", body.name)
        gh("release", "upload", tag, "--repo", repo, "--clobber", *[str(Path("dist") / name) for name in ASSETS])
        uploaded = json.loads(gh("release", "view", tag, "--repo", repo, "--json", "assets"))
        if {asset["name"] for asset in uploaded["assets"]} != set(ASSETS):
            raise RuntimeError("Upload incomplete; leaving release as a draft")
        gh("release", "edit", tag, "--repo", repo, "--draft=false", "--latest")
    print(f"Published https://github.com/{repo}/releases/tag/{tag}")


if __name__ == "__main__":
    main()
