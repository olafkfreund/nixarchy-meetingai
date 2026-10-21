#!/usr/bin/env bash
# Install atomically, with Nix runtime libraries available outside the dev shell.
set -euo pipefail
source_binary=${1:?usage: install-binary.sh SOURCE DESTINATION}
destination=${2:?usage: install-binary.sh SOURCE DESTINATION}
directory=$(dirname -- "$destination")
mkdir -p -- "$directory"
temporary=$(mktemp "$directory/.lennette-install.XXXXXXXX")
trap 'rm -f -- "$temporary"' EXIT
install -m 755 -- "$source_binary" "$temporary"
if [[ -n ${LENNETTE_RUNTIME_LIBRARY_PATH:-} ]]; then
    patchelf --add-rpath "$LENNETTE_RUNTIME_LIBRARY_PATH" "$temporary"
fi
mv -f -- "$temporary" "$destination"
