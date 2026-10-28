#!/usr/bin/env bash
# Tap the audio of the recording that is running (what the transcriber receives) into a stereo
# 16 kHz WAV: left = mic, right = system. Start a recording first; the owner runs this knowingly.
# Usage: record.sh OUT.wav SECONDS   (Ctrl-C stops early)
set -euo pipefail
out=$(realpath -m "${1:?usage: record.sh OUT.wav SECONDS}")
secs=${2:?usage: record.sh OUT.wav SECONDS}
case $secs in '' | *[!0-9]*) echo "SECONDS must be a whole number" >&2; exit 2 ;; esac
ctl=${NIXARCHY_MEETINGAI:-nixarchy-meetingai}
trap '"$ctl" ctl record.tap "{\"stop\":true}" >/dev/null; exit 130' INT TERM
echo "Tapping the running recording for ${secs}s -> $out"
"$ctl" ctl record.tap "$(jq -n --arg p "$out" --argjson s "$secs" '{path:$p,seconds:$s}')"
sleep "$secs"
