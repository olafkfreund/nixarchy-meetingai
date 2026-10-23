#!/usr/bin/env bash
# Installer/launcher integration tests use isolated homes and local releases.
set -euo pipefail
source_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
test_home="$work/user's home"
mkdir -p "$test_home"
export LENNETTE_TEST_LAUNCH_LOG="$work/launched"

make_release() {
    local version=$1
    mkdir -p "$work/pkg/lennette-linux-x86_64" "$work/releases/download/$version"
    cat >"$work/pkg/lennette-linux-x86_64/lennette" <<'FAKE'
#!/bin/sh
if [ "${1:-}" = --version ]; then echo 'lennette test'; exit 0; fi
printf '%s\n' "$@" >"$LENNETTE_TEST_LAUNCH_LOG"
FAKE
    chmod +x "$work/pkg/lennette-linux-x86_64/lennette"
    cp "$source_dir/src/assets/lennette_app_icon.png" "$work/pkg/lennette-linux-x86_64/lennette_app_icon.png"
    tar -C "$work/pkg" -czf "$work/releases/download/$version/lennette-linux-x86_64.tar.gz" lennette-linux-x86_64
    (cd "$work/releases/download/$version" && sha256sum lennette-linux-x86_64.tar.gz >SHA256SUMS-linux-x86_64.txt)
}
install_here() {
    env -u XDG_DATA_HOME -u XDG_BIN_HOME -u LENNETTE_HOME \
        HOME="$test_home" SHELL=/bin/bash DISPLAY=:test \
        LENNETTE_RELEASES_URL="$work/releases" LENNETTE_INSTALL_URL="$source_dir/web/public/install" \
        bash "$source_dir/web/public/install" "$@" </dev/null
}
run_launcher() {
    DISPLAY=:test "$test_home/.local/bin/lenny" "$@"
}
check_version() {
    local output
    output=$(run_launcher --version) || return
    [[ $output == *'lennette test'* ]]
}
check() {
    local what=$1; shift
    if ! "$@"; then echo "FAIL: $what" >&2; exit 1; fi
    echo "ok: $what"
}

make_release v0.0.0-test
printf '%064d  lennette-linux-x86_64.tar.gz\n' 0 >"$work/releases/download/v0.0.0-test/SHA256SUMS-linux-x86_64.txt"
if install_here --no-launch --no-modify-path >"$work/tamper.log" 2>&1; then echo 'FAIL: tamper accepted'; exit 1; fi
check 'tampered download is rejected' grep -q 'checksum mismatch' "$work/tamper.log"
check 'failed install leaves no launcher' test ! -e "$test_home/.local/bin/lenny"

make_release v0.0.0-test
install_here --no-launch --no-modify-path >"$work/install.log"
check 'lenny is executable' test -x "$test_home/.local/bin/lenny"
check 'the branded desktop launcher is installed' grep -q 'Icon=dev.lennette.overlay' "$test_home/.local/share/applications/dev.lennette.overlay.desktop"
check 'desktop icon matches the shipped artwork' cmp "$source_dir/src/assets/lennette_app_icon.png" "$test_home/.local/share/icons/hicolor/512x512/apps/dev.lennette.overlay.png"
check 'lennette remains a compatibility alias' test -e "$test_home/.local/bin/lennette"
check '--no-launch does not open the app' test ! -e "$LENNETTE_TEST_LAUNCH_LOG"
check '--no-modify-path preserves startup files' test ! -e "$test_home/.bashrc"
check 'app flags work from a path containing spaces and apostrophes' check_version
run_launcher 'argument with spaces' "apostrophe's argument" >/dev/null
check 'arguments are passed without shell evaluation' test "$(cat "$LENNETTE_TEST_LAUNCH_LOG")" = "$(printf '%s\n' 'argument with spaces' "apostrophe's argument")"
rm "$LENNETTE_TEST_LAUNCH_LOG"
install_here >"$work/again.log"
check 'setup launches immediately by default' test -f "$LENNETTE_TEST_LAUNCH_LOG"
install_here --no-launch >>"$work/again.log"
check 'repeated setup reuses the release' grep -q 'Already installed' "$work/again.log"
check 'PATH is added exactly once' test "$(grep -c 'lennette: user binaries on PATH' "$test_home/.bashrc")" = 1
# The PATH entry is shell code, so actually source it with this unusual home.
# shellcheck disable=SC2016
check 'persisted PATH handles the home directory correctly' env TEST_BIN="$test_home/.local/bin" bash -c 'source "$1"; [[ $PATH == "$TEST_BIN:"* ]]' _ "$test_home/.bashrc"

mv "$work/releases" "$work/offline"
check 'normal launches do not need the network' check_version
check 'doctor works offline in a desktop session' run_launcher doctor
check 'help works offline' run_launcher --help
mv "$work/offline" "$work/releases"

make_release v0.0.1-test
printf '%064d  lennette-linux-x86_64.tar.gz\n' 0 >"$work/releases/download/v0.0.1-test/SHA256SUMS-linux-x86_64.txt"
if run_launcher update >"$work/update-failed.log" 2>&1; then echo 'FAIL: tampered upgrade accepted'; exit 1; fi
check 'failed upgrade keeps the active launcher' grep -q 'v0.0.0-test' "$test_home/.local/bin/lenny"
check 'failed upgrade keeps the app runnable' check_version

make_release v0.0.1-test
rm "$LENNETTE_TEST_LAUNCH_LOG"
run_launcher update >"$work/update.log"
check 'lenny update selects the latest release' grep -q 'v0.0.1-test' "$test_home/.local/bin/lenny"
check 'updates do not launch unexpectedly' test ! -e "$LENNETTE_TEST_LAUNCH_LOG"
check 'old releases remain available until uninstall' test -d "$test_home/.local/share/lennette/versions/v0.0.0-test"
run_launcher update --version v0.0.0-test >"$work/pin.log"
check 'a pinned version can be selected' grep -q 'v0.0.0-test' "$test_home/.local/bin/lenny"

rm -rf "$test_home/.local/share/lennette/versions"
check 'missing payload is repaired and the new release runs' check_version
check 'repair rewrites the active version path' grep -q 'v0.0.1-test' "$test_home/.local/bin/lenny"
if env -u DISPLAY -u WAYLAND_DISPLAY "$test_home/.local/bin/lenny" doctor >"$work/headless.log" 2>&1; then echo 'FAIL: headless doctor succeeded'; exit 1; fi
check 'doctor explains a missing desktop session' grep -q 'No desktop session' "$work/headless.log"
if install_here --version '../../escape' >"$work/tag.log" 2>&1; then echo 'FAIL: unsafe tag accepted'; exit 1; fi

mkdir -p "$test_home/.config/lennette"
printf 'keep me' >"$test_home/.config/lennette/settings"
run_launcher uninstall
check 'uninstall removes lenny' test ! -e "$test_home/.local/bin/lenny"
check 'uninstall removes its compatibility alias' test ! -L "$test_home/.local/bin/lennette"
check 'uninstall removes all release payloads' test ! -e "$test_home/.local/share/lennette/versions"
check 'uninstall removes the desktop entry' test ! -e "$test_home/.local/share/applications/dev.lennette.overlay.desktop"
check 'uninstall removes its desktop icon' test ! -e "$test_home/.local/share/icons/hicolor/512x512/apps/dev.lennette.overlay.png"
check 'uninstall keeps settings' test -f "$test_home/.config/lennette/settings"
echo 'Linux installer: all checks passed'
