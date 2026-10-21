#!/usr/bin/env bash
set -euo pipefail
package_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_dir=$(cd -- "$package_dir/../.." && pwd)
source_dir=${1:-"$repo_dir/ref/Hyprland"}
build_dir=${2:-"$repo_dir/target/hyprland-private"}
expected=c7216a9807a2683ce243f6ed14ae01e7e884c426
actual=$(git -C "$source_dir" rev-parse HEAD)
if [[ "$actual" != "$expected" ]]; then
    echo "Unsupported Hyprland revision: $actual (expected $expected)" >&2
    exit 1
fi
if git -C "$source_dir" apply --reverse --check "$package_dir/private-overlay.patch" 2>/dev/null; then
    echo 'Private overlay patch is already applied.'
else
    git -C "$source_dir" apply --check "$package_dir/private-overlay.patch"
    git -C "$source_dir" apply "$package_dir/private-overlay.patch"
fi
cmake -S "$source_dir" -B "$build_dir" \
    -DCMAKE_BUILD_TYPE="${BUILD_TYPE:-Release}" \
    -DCMAKE_CXX_FLAGS_DEBUG='-O0 -g0' \
    -DCMAKE_DISABLE_PRECOMPILE_HEADERS=ON \
    -DBUILT_WITH_NIX=ON -DNO_HYPRPM=ON -DNO_UWSM=ON \
    -DTRACY_ENABLE=OFF -DWITH_TESTS=OFF
cmake --build "$build_dir" --target Hyprland hyprctl -j "${BUILD_JOBS:-2}"
echo "Built $build_dir/Hyprland; your running compositor has not been replaced."
