#!/usr/bin/env bash
# Encode 16 kHz WAVs to Opus at 16 kbps (voip) and decode back to 16 kHz mono 16-bit PCM WAV (#61 step 3).
#
# Usage:
#   bash tools/diarize/opus_roundtrip.sh IN.wav OUT.wav
#   bash tools/diarize/opus_roundtrip.sh SET_DIR
#
# In directory mode, walks `<SET_DIR>/<dev|test>/clean/**/*.wav` (or `<SET_DIR>/clean/**/*.wav`)
# and writes the round-tripped WAVs + copied `.tsv` truth files to the sibling `call/` tree.
set -euo pipefail

if ! command -v ffmpeg >/dev/null 2>&1; then
    echo "opus_roundtrip.sh: ffmpeg is required on PATH" >&2
    exit 1
fi

roundtrip_one() {
    local in_wav="$1"
    local out_wav="$2"
    local tmp_opus
    mkdir -p "$(dirname "$out_wav")"
    tmp_opus="$(mktemp "${TMPDIR:-/tmp}/opus_rt.XXXXXX.opus")"
    trap 'rm -f "$tmp_opus"' RETURN

    ffmpeg -nostdin -v error -y -i "$in_wav" \
        -ac 1 -ar 16000 -c:a libopus -b:a 16k -vbr on -application voip \
        "$tmp_opus"
    ffmpeg -nostdin -v error -y -i "$tmp_opus" \
        -ac 1 -ar 16000 -c:a pcm_s16le \
        "$out_wav"

    local in_tsv="${in_wav%.wav}.tsv"
    local out_tsv="${out_wav%.wav}.tsv"
    if [[ -f "$in_tsv" && "$in_tsv" != "$out_tsv" ]]; then
        cp "$in_tsv" "$out_tsv"
    fi
}

if [[ $# -eq 2 && ( "$1" == *.wav || ! -d "$1" ) ]]; then
    roundtrip_one "$1" "$2"
    exit 0
fi

if [[ $# -eq 1 && -d "$1" ]]; then
    set_dir="${1%/}"
    shopt -s globstar nullglob
    count=0
    for wav in "$set_dir"/**/clean/**/*.wav; do
        [[ -f "$wav" ]] || continue
        rel="${wav#"$set_dir/"}"
        out_rel="${rel/clean\//call\/}"
        if [[ "$out_rel" == "$rel" ]]; then
            out_wav="$set_dir/call/$(basename "$wav")"
        else
            out_wav="$set_dir/$out_rel"
        fi
        roundtrip_one "$wav" "$out_wav"
        count=$((count + 1))
    done
    echo "round-tripped $count WAV files under $set_dir"
    exit 0
fi

echo "usage: opus_roundtrip.sh IN.wav OUT.wav | opus_roundtrip.sh SET_DIR" >&2
exit 2
