#!/usr/bin/env bash
# The whole shell suite, headless: what the `shell` flake check and the CI fallback run.
# usage: ci.sh [OUT_DIR]   (crops land in OUT_DIR/<scenario>-<scale>). Needs the tools and
# variables of `nix develop .#shell`; the scripts it calls must never get MEETINGAI_ON_DESKTOP.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
out=${1:-${TMPDIR:-/tmp}/meetingai-ci}
unset MEETINGAI_ON_DESKTOP
[ -w "${HOME:-/nonexistent}" ] || export HOME=$(mktemp -d)
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-$(mktemp -d)}   # live-shots reads it before it makes its own
chmod 700 "$XDG_RUNTIME_DIR"
export MEETINGAI_GEOMETRY=1

step() { echo "=== $*"; }
step lint;       bash "$here/lint.sh"
step smoke;      bash "$here/smoke.sh"
step smoke-real; bash "$here/smoke-real.sh"
step keys;       bash "$here/keys.sh"
for scale in 100 150; do
    for scenario in live k8s team interviewer r5 first-run no-daemon error providers; do
        step "live-shots $scenario $scale%"
        SCENARIO=$scenario SCALE=$scale bash "$here/live-shots.sh" "$out/$scenario-$scale"
    done
done
step contrast
t=$(dirname "$OMARCHY_SHELL_TREE")/themes
python3 "$here/contrast.py" "$t/flexoki-light/colors.toml" "$t/catppuccin-latte/colors.toml" "$t/white/colors.toml"
echo "shell suite ok"
