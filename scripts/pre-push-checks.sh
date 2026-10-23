#!/usr/bin/env bash
# Git pre-push checks, run through lefthook: style, types and full test suite.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "======================================================"
echo " Running pre-push checks (style, typecheck, and tests)"
echo "======================================================"

if command -v devenv >/dev/null 2>&1 && [ -f "$REPO_ROOT/devenv.nix" ]; then
    devenv shell -- just pre-push
else
    just pre-push
fi

echo "All pre-push checks passed successfully!"
