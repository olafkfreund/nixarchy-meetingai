#!/usr/bin/env bash
# Refuse to open Lennette unless the running compositor accepts capture exclusion.
set -euo pipefail
if [[ -z ${XDG_RUNTIME_DIR:-} || -z ${WAYLAND_DISPLAY:-} || -z ${HYPRLAND_INSTANCE_SIGNATURE:-} ]]; then
    echo 'Lennette private mode requires a running Hyprland Wayland session.' >&2
    exit 1
fi
# The rule probe and the application must target the same Wayland compositor.
if ! socket=$(hyprctl instances -j | jq -er --arg instance "$HYPRLAND_INSTANCE_SIGNATURE" \
    '.[] | select(.instance == $instance) | .wl_socket | select(type == "string" and length > 0)') || \
    [[ "$WAYLAND_DISPLAY" != "$socket" && "$WAYLAND_DISPLAY" != "${XDG_RUNTIME_DIR%/}/$socket" ]]; then
    echo 'Lennette was not opened: Wayland display and Hyprland instance do not match.' >&2
    exit 1
fi
rule='hl.window_rule({name="lennette-private-overlay",match={class=[[^(dev\.lennette\.overlay|lennette)$]]},float=true,pin=true,border_size=0,rounding=0,no_blur=true,no_shadow=true,private_overlay=true})'
rule+='; hl.window_rule({name="lennette-overlay-size",match={class=[[^dev\.lennette\.overlay$]],title="^lennette$"},size={720,620},center=true})'
rule+='; hl.window_rule({name="lennette-dashboard-size",match={class=[[^dev\.lennette\.overlay$]],title="^lennette dashboard$"},size={1100,760},center=true})'
if ! response=$(hyprctl eval "$rule" 2>&1) || [[ "$response" != 'ok' ]]; then
    echo "Lennette was not opened: the compositor could not enforce private_overlay: $response" >&2
    exit 1
fi
if ! plugins=$(hyprctl -j plugin list 2>&1) || \
    ! jq -e --argjson allowed "${LENNETTE_ALLOWED_PLUGIN_PATHS:-[]}" \
      'type == "array" and all(.[]; .privateCaptureCompatible == true and (.path as $path | $allowed | index($path) != null))' \
      <<< "$plugins" >/dev/null 2>&1; then
    echo 'Lennette was not opened: a loaded plugin is not approved for this private compositor build.' >&2
    exit 1
fi
if [[ ${1:-} == --check ]]; then
    echo 'The running compositor accepted the private_overlay rule.'
    exit 0
fi
binary=${LENNETTE_BINARY:-"$HOME/.local/bin/lennette"}
if [[ ! -x "$binary" ]]; then
    echo "Lennette executable is missing: $binary" >&2
    exit 1
fi
if ! help=$("$binary" --help) || [[ "$help" != *'LENNETTE_PRIVATE_OVERLAY=1'* ]]; then
    echo 'Lennette was not opened: rebuild the app with private-overlay tray suppression support.' >&2
    exit 1
fi
# GPUI chooses Wayland when WAYLAND_DISPLAY exists. Remove the X11 fallback and
# headless override so this launcher cannot accidentally create an XWayland app.
unset DISPLAY WAYLAND_SOCKET ZED_HEADLESS
export LENNETTE_PRIVATE_OVERLAY=1
exec "$binary" "$@"
