#!/usr/bin/env bash
# Lines still authored by the original lennette authors (git blame -w), per file and in total.
# Usage: origin-lines.sh [--total] [--all] [path...]
# --all also counts Cargo.lock, devenv.lock and LICENSE (replaced, not rewritten, in step 11/12).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
total_only=0 all=0
while [[ ${1-} == --* ]]; do
  case $1 in --total) total_only=1 ;; --all) all=1 ;; *) echo "unknown flag $1" >&2; exit 2 ;; esac
  shift
done
total=0 rows=""
while IFS= read -r file; do
  [[ -f $file ]] || continue
  case $file in intent/* | spec/* | plan/* | docs/* | tests/fixtures/*) continue ;; esac
  if ((!all)); then case $file in Cargo.lock | devenv.lock | LICENSE) continue ;; esac; fi
  n=$(git blame -w --line-porcelain -- "$file" 2>/dev/null | grep -cE '^author (Harsh Sahu|tutankhAman)$' || true)
  ((n > 0)) || continue
  total=$((total + n))
  rows+="$n $file"$'\n'
done < <(git ls-files -- "$@")
if ((total_only)); then
  echo "$total"
else
  printf '%s' "$rows" | sort -rn
  echo "total $total"
fi
