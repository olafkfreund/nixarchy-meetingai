# shellcheck shell=bash
# Sourced by the shell test scripts after `work` is set. Starts a private headless
# sway (no window on the owner's desktop) and points WAYLAND_DISPLAY at it.
# MEETINGAI_ON_DESKTOP=1 uses the real session instead, for watching a run.
# Sets HEADLESS=1|0; in headless mode the only output is 1920x1080 at 0,0.
# The whole run also gets a private D-Bus session (dbus-run-session), so nothing a test starts can reach the
# owner's real session bus: the calling script starts over once inside it. The bus has no service directory,
# so it activates nothing (a portal started from it would inherit the owner's compositor).
if [ -z "${MEETINGAI_PRIVATE_BUS:-}" ]; then
    trap - EXIT
    chmod -R u+w "$work"; rm -rf "$work"
    bus_cfg=$(mktemp /tmp/nm.XXXX)
    printf '%s\n' '<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>' \
        '<policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>' > "$bus_cfg"
    MEETINGAI_PRIVATE_BUS=$bus_cfg exec dbus-run-session --config-file="$bus_cfg" -- bash "$0" "$@"
fi
rm -f "$MEETINGAI_PRIVATE_BUS"   # the bus has read its config
if [ "${MEETINGAI_ON_DESKTOP:-}" = 1 ]; then
    export WAYLAND_DISPLAY=${XDG_RUNTIME_DIR:?}/${WAYLAND_DISPLAY:-wayland-1}
    HEADLESS=0
else
    # sway from the nixpkgs this repo's flake.lock pins, not whatever the registry says today
    repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
    sway_bin=$(nix build --no-link --print-out-paths --inputs-from "$repo" nixpkgs#sway | head -1)/bin/sway
    mkdir -m 700 "$work/sway"
    printf 'output HEADLESS-1 resolution 1920x1080 position 0 0\n' > "$work/sway.cfg"
    env -u WAYLAND_DISPLAY -u DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u SWAYSOCK \
        XDG_RUNTIME_DIR="$work/sway" WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
        "$sway_bin" -c "$work/sway.cfg" > "$work/sway.log" 2>&1 &
    for _ in $(seq 100); do
        sock=$(find "$work/sway" -maxdepth 1 -name 'wayland-[0-9]*' -type s | head -1)
        [ -n "$sock" ] && break
        sleep 0.05
    done
    [ -n "$sock" ] || { cat "$work/sway.log" >&2; echo "FAIL: headless sway did not start" >&2; exit 1; }
    export WAYLAND_DISPLAY=$sock
    unset HYPRLAND_INSTANCE_SIGNATURE DISPLAY   # never talk to the owner's real compositor
    HEADLESS=1
fi
