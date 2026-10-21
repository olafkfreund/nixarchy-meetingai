#!/usr/bin/env bash
# Automated semantic version bumper for Lennette.
# Usage:
#   scripts/bump-version.sh [patch|minor|major|auto] [commit_msg_file_or_string]
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
CARGO_TOML="$REPO_ROOT/Cargo.toml"

if [ ! -f "$CARGO_TOML" ]; then
    echo "Error: Cargo.toml not found at $CARGO_TOML" >&2
    exit 1
fi

MODE="${1:-auto}"
MSG_SRC="${2:-}"

python3 - << PYEOF
import os
import re
import sys
import subprocess

cargo_toml_path = "$CARGO_TOML"
mode = "$MODE"
msg_src = """$MSG_SRC"""

msg = ""
if msg_src.strip():
    if os.path.isfile(msg_src.strip()):
        try:
            with open(msg_src.strip(), "r", encoding="utf-8", errors="ignore") as f:
                msg = f.read()
        except Exception:
            msg = ""
    else:
        msg = msg_src

if not msg and os.path.isdir("/proc"):
    pid = os.getppid()
    while pid > 1:
        try:
            with open(f"/proc/{pid}/cmdline", "rb") as f:
                cmd = f.read().split(b"\x00")
            for i, arg in enumerate(cmd):
                if arg in (b"-m", b"--message") and i + 1 < len(cmd):
                    msg = cmd[i+1].decode(errors="ignore")
                    break
                elif arg.startswith(b"--message="):
                    msg = arg.split(b"=", 1)[1].decode(errors="ignore")
                    break
            if msg:
                break
            with open(f"/proc/{pid}/stat", "r") as f:
                pid = int(f.read().split()[3])
        except Exception:
            break

bump_type = "patch"
if mode in ("major", "minor", "patch"):
    bump_type = mode
elif mode == "auto":
    if re.search(r"BREAKING[ -]CHANGE|^[a-zA-Z0-9_]+!:", msg, re.IGNORECASE | re.MULTILINE):
        bump_type = "major"
    elif re.search(r"^feat(\([a-zA-Z0-9_-]+\))?:", msg, re.IGNORECASE | re.MULTILINE):
        bump_type = "minor"
    else:
        bump_type = "patch"

with open(cargo_toml_path, "r", encoding="utf-8") as f:
    lines = f.readlines()

current_version = None
inside_pkg = False
new_lines = []
updated = False

for line in lines:
    stripped = line.strip()
    if stripped.startswith("[package]"):
        inside_pkg = True
    elif stripped.startswith("[") and not stripped.startswith("[["):
        inside_pkg = False

    if inside_pkg and not current_version and stripped.startswith("version ="):
        m = re.search(r'version\s*=\s*"([^"]+)"', stripped)
        if m:
            current_version = m.group(1)

if not current_version:
    print(f"Error: Could not extract package version from {cargo_toml_path}", file=sys.stderr)
    sys.exit(1)

parts = current_version.split("-")[0].split(".")
major = int(parts[0]) if len(parts) > 0 else 0
minor = int(parts[1]) if len(parts) > 1 else 0
patch = int(parts[2]) if len(parts) > 2 else 0

if bump_type == "major":
    major += 1
    minor = 0
    patch = 0
elif bump_type == "minor":
    minor += 1
    patch = 0
else:
    patch += 1

new_version = f"{major}.{minor}.{patch}"

inside_pkg = False
for line in lines:
    stripped = line.strip()
    if stripped.startswith("[package]"):
        inside_pkg = True
    elif stripped.startswith("[") and not stripped.startswith("[["):
        inside_pkg = False

    if inside_pkg and not updated and stripped.startswith("version ="):
        new_lines.append(f'version = "{new_version}"\n')
        updated = True
    else:
        new_lines.append(line)

with open(cargo_toml_path, "w", encoding="utf-8") as f:
    f.writelines(new_lines)

print(f"lennette version bumped: {current_version} -> {new_version} ({bump_type})")
PYEOF

# Sync lockfile if cargo is present
if command -v cargo >/dev/null 2>&1; then
    cargo check --quiet >/dev/null 2>&1 || true
fi

# Stage changes if inside a git repository
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    git add "$CARGO_TOML"
    if [ -f "$REPO_ROOT/Cargo.lock" ]; then
        git add "$REPO_ROOT/Cargo.lock"
    fi
fi
