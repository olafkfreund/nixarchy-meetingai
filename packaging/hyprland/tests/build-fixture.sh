#!/usr/bin/env bash
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
out=${1:-"$here/../../../target/hyprland-test-client"}
mkdir -p "$out"
protocols=$(pkg-config --variable=pkgdatadir wayland-protocols)
wayland-scanner client-header "$protocols/stable/xdg-shell/xdg-shell.xml" "$out/xdg-shell-client-protocol.h"
wayland-scanner private-code "$protocols/stable/xdg-shell/xdg-shell.xml" "$out/xdg-shell-protocol.c"
# Split pkg-config output into an array; stop before compiling on lookup errors.
client_flags=$(pkg-config --cflags --libs wayland-client)
read -r -a client_flags_array <<< "$client_flags"
cc -std=c11 -Wall -Wextra -Werror -I"$out" "$here/window.c" "$out/xdg-shell-protocol.c" \
    "${client_flags_array[@]}" -o "$out/window"
repo=$(cd -- "$here/../../.." && pwd)
hyprland_source=${HYPRLAND_SOURCE:-"$repo/ref/Hyprland"}
for protocol in \
    "$hyprland_source/protocols/wlr-screencopy-unstable-v1.xml" \
    "$protocols/unstable/linux-dmabuf/linux-dmabuf-unstable-v1.xml" \
    "$protocols/staging/ext-image-capture-source/ext-image-capture-source-v1.xml" \
    "$protocols/staging/ext-image-copy-capture/ext-image-copy-capture-v1.xml" \
    "$protocols/staging/ext-foreign-toplevel-list/ext-foreign-toplevel-list-v1.xml"; do
    stem=$(basename "$protocol" .xml)
    wayland-scanner client-header "$protocol" "$out/$stem-client-protocol.h"
    wayland-scanner private-code "$protocol" "$out/$stem-protocol.c"
done
keyboard_protocol="$hyprland_source/protocols/virtual-keyboard-unstable-v1.xml"
wayland-scanner client-header "$keyboard_protocol" "$out/virtual-keyboard-unstable-v1-client-protocol.h"
wayland-scanner private-code "$keyboard_protocol" "$out/virtual-keyboard-unstable-v1-protocol.c"
keyboard_flags=$(pkg-config --cflags --libs wayland-client xkbcommon)
read -r -a keyboard_flags_array <<< "$keyboard_flags"
cc -std=c11 -Wall -Wextra -Werror -I"$out" "$here/keyboard.c" \
    "$out/virtual-keyboard-unstable-v1-protocol.c" "${keyboard_flags_array[@]}" -o "$out/keyboard"
capture_flags=$(pkg-config --cflags --libs wayland-client gbm)
read -r -a capture_flags_array <<< "$capture_flags"
cc -std=c11 -Wall -Wextra -Werror -I"$out" "$here/capture.c" \
    "$out/wlr-screencopy-unstable-v1-protocol.c" \
    "$out/linux-dmabuf-unstable-v1-protocol.c" \
    "$out/ext-image-capture-source-v1-protocol.c" \
    "$out/ext-image-copy-capture-v1-protocol.c" \
    "$out/ext-foreign-toplevel-list-v1-protocol.c" \
    "${capture_flags_array[@]}" -o "$out/capture"
