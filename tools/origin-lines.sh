#!/usr/bin/env bash
# Lines still authored by the original lennette authors (git blame -w), per file and in total.
# Usage: origin-lines.sh [--total] [--all] [path...]
# --all also counts Cargo.lock, devenv.lock and LICENSE (replaced, not rewritten, in step 11/12).
# --copies also follows lines moved or copied from other files (git blame -M -C -C), so pasted
# original code is still counted.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
total_only=0 all=0 follow=()
while [[ ${1-} == --* ]]; do
  case $1 in --total) total_only=1 ;; --all) all=1 ;; --copies) follow=(-M -C -C) ;; *) echo "unknown flag $1" >&2; exit 2 ;; esac
  shift
done
total=0 rows=""
while IFS= read -r -d '' file; do
  [[ -f $file ]] || continue
  case $file in intent/* | spec/* | plan/* | docs/* | tests/fixtures/*) continue ;; esac
  if ((!all)); then case $file in Cargo.lock | devenv.lock | LICENSE) continue ;; esac; fi
  # A failed blame must stop the script, never read as 0 lines left.
  blame=$(git blame -w "${follow[@]}" --line-porcelain -- "$file")
  n=$(grep -cE '^author (Harsh Sahu|tutankhAman)$' <<<"$blame" || true)
  ((n > 0)) || continue
  total=$((total + n))
  rows+="$n $file"$'\n'
done < <(git -c core.quotePath=false ls-files -z -- "$@")
if ((total_only)); then
  echo "$total"
else
  printf '%s' "$rows" | sort -rn
  echo "total $total"
fi
