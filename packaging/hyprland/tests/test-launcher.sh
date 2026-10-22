#!/usr/bin/env bash
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
printf '#!%s\n' "$BASH" > "$work/hyprctl"
cat >> "$work/hyprctl" <<'STUB'
if [[ $1 == instances ]]; then
    printf '%s\n' '[{"instance":"test","wl_socket":"wayland-test"}]'
    exit 0
fi
if [[ $1 == -j ]]; then
    printf '%s\n' "${FAKE_PLUGINS:-[]}"
    exit 0
fi
printf '%s\n' "$FAKE_RESPONSE"
exit "${FAKE_STATUS:-0}"
STUB
printf '#!%s\n' "$BASH" > "$work/lennette"
cat >> "$work/lennette" <<'STUB'
if [[ ${1:-} == --help ]]; then
    printf '%s\n' "${FAKE_SUPPORT:-LENNETTE_PRIVATE_OVERLAY=1}"
    exit 0
fi
[[ -z ${DISPLAY:-} && -z ${WAYLAND_SOCKET:-} && -z ${ZED_HEADLESS:-} ]]
[[ ${LENNETTE_PRIVATE_OVERLAY:-} == 1 ]]
printf '%s\n' "$*" > "$LAUNCHED_FILE"
STUB
chmod +x "$work/hyprctl" "$work/lennette"
export PATH="$work:$PATH" XDG_RUNTIME_DIR="$work" WAYLAND_DISPLAY=wayland-test HYPRLAND_INSTANCE_SIGNATURE=test
export LENNETTE_BINARY="$work/lennette" LAUNCHED_FILE="$work/launched"
for FAKE_RESPONSE in 'error: unknown field private_overlay' '' 'ok but unverified'; do
    export FAKE_RESPONSE
    if bash "$here/../launch.sh" 2>/dev/null; then echo 'unsafe launch accepted' >&2; exit 1; fi
    [[ ! -e "$LAUNCHED_FILE" ]]
done
export FAKE_RESPONSE=ok FAKE_STATUS=1
if bash "$here/../launch.sh" 2>/dev/null; then echo 'failed IPC accepted' >&2; exit 1; fi
export FAKE_STATUS=0 DISPLAY=:999 WAYLAND_SOCKET=99 ZED_HEADLESS=1
export FAKE_PLUGINS='[{"name":"overview"}]'
if bash "$here/../launch.sh" 2>/dev/null; then echo 'render plugin accepted' >&2; exit 1; fi
[[ ! -e "$LAUNCHED_FILE" ]]
export FAKE_PLUGINS=invalid
if bash "$here/../launch.sh" 2>/dev/null; then echo 'invalid plugin status accepted' >&2; exit 1; fi
export FAKE_PLUGINS='[]' WAYLAND_DISPLAY=other-compositor
if bash "$here/../launch.sh" 2>/dev/null; then echo 'mismatched compositor accepted' >&2; exit 1; fi
[[ ! -e "$LAUNCHED_FILE" ]]
export WAYLAND_DISPLAY=wayland-test
export FAKE_SUPPORT=unsupported
if bash "$here/../launch.sh" 2>/dev/null; then echo 'old app without tray suppression accepted' >&2; exit 1; fi
[[ ! -e "$LAUNCHED_FILE" ]]
unset FAKE_SUPPORT
export LENNETTE_ALLOWED_PLUGIN_PATHS='["/nix/store/test-plugin/lib/liboverview.so"]'
export FAKE_PLUGINS='[{"name":"overview","path":"/nix/store/test-plugin/lib/liboverview.so","privateCaptureCompatible":true}]'
bash "$here/../launch.sh" --toggle-window
[[ $(cat "$LAUNCHED_FILE") == --toggle-window ]]
rm "$LAUNCHED_FILE"
export FAKE_PLUGINS='[{"name":"overview","path":"/nix/store/other-plugin/lib/liboverview.so","privateCaptureCompatible":true}]'
if bash "$here/../launch.sh" 2>/dev/null; then echo 'unapproved plugin path accepted' >&2; exit 1; fi
export FAKE_PLUGINS='[{"name":"overview","path":"/nix/store/test-plugin/lib/liboverview.so"}]'
if bash "$here/../launch.sh" 2>/dev/null; then echo 'old compositor plugin status accepted' >&2; exit 1; fi
[[ ! -e "$LAUNCHED_FILE" ]]
unset LENNETTE_ALLOWED_PLUGIN_PATHS
export FAKE_PLUGINS='[]'
bash "$here/../launch.sh" --toggle-window
[[ $(cat "$LAUNCHED_FILE") == --toggle-window ]]
rm "$LAUNCHED_FILE"
bash "$here/../launch.sh" --check >/dev/null
[[ ! -e "$LAUNCHED_FILE" ]]
unset WAYLAND_DISPLAY
if bash "$here/../launch.sh" 2>/dev/null; then echo 'X11 launch accepted' >&2; exit 1; fi
echo 'PASS: launcher fails closed and preserves control arguments.'
