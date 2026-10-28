#!/usr/bin/env bash
# Record a stereo 16 kHz WAV: left = default mic, right = default sink monitor (system audio).
# Usage: record.sh OUT.wav SECONDS   -- the owner runs this knowingly; tests never do.
set -euo pipefail
out=${1:?usage: record.sh OUT.wav SECONDS}
secs=${2:?usage: record.sh OUT.wav SECONDS}
case $secs in '' | *[!0-9]*) echo "SECONDS must be a whole number" >&2; exit 2 ;; esac
mic=$(pactl get-default-source)
mon=$(pactl get-default-sink).monitor
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
echo "Recording ${secs}s: left (me) = $mic, right (them) = $mon -> $out"
fmt=(--format=s16le --rate=16000 --channels=1)
timeout "$secs" parec "${fmt[@]}" --device="$mic" --file-format=wav "$dir/l.wav" || true &
timeout "$secs" parec "${fmt[@]}" --device="$mon" --file-format=wav "$dir/r.wav" || true &
wait
# Merge the two mono captures; trims to the shorter one.
python3 -I - "$dir/l.wav" "$dir/r.wav" "$out" <<'PY'
import sys, wave
l, r, out = (wave.open(sys.argv[1]), wave.open(sys.argv[2]), wave.open(sys.argv[3], "wb"))
n = min(l.getnframes(), r.getnframes())
a, b = l.readframes(n), r.readframes(n)
out.setnchannels(2); out.setsampwidth(2); out.setframerate(16000)
out.writeframes(b"".join(a[i:i+2] + b[i:i+2] for i in range(0, len(a), 2)))
print(f"wrote {n / 16000:.1f}s")
PY
