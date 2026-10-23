#!/usr/bin/env bash
# Preserve the existing hook/just entry point; version handling lives in Rust.
set -euo pipefail
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"
exec cargo xtask bump "$@"
