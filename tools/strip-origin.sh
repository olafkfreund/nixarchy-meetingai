#!/usr/bin/env bash
# Remove the lines git blame credits to the original authors (#3, rewrite step "removal" commit).
# A file left with only blank lines is deleted. Usage: strip-origin.sh file...
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
for file in "$@"; do
  git blame -w --line-porcelain -- "$file" | awk '
    /^author / { a = substr($0, 8) }
    /^\t/ { if (a != "Harsh Sahu" && a != "tutankhAman") print substr($0, 2) }
  ' >"$file.stripped"
  if grep -q '[^[:space:]]' "$file.stripped"; then mv "$file.stripped" "$file"; else rm "$file.stripped"; git rm -q "$file"; fi
done
