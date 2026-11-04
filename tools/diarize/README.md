# Local speaker separation evaluation sets (#61)

Scripts live in git; **test audio and results live in `~/.cache/meetingai-eval/`
and are never committed.** Override the cache root with `MEETINGAI_EVAL_CACHE`.

All scripts use only Python's standard library (`wave`, `xml.etree`, `array`,
`random`, `subprocess`) plus `ffmpeg`, `curl` and `unzip`. Audio is decoded
with `ffmpeg` and written with Python's `wave` module (`soundfile` is not used).

## Licences

- **LibriSpeech ASR corpus (`test-clean`)** — Vassil Panayotov, Guoguo Chen,
  Daniel Povey and Sanjeev Khudanpur, licensed under **CC BY 4.0**
  (`https://www.openslr.org/12/`).
- **AMI Meeting Corpus** — IDIAP / University of Edinburgh and partners,
  licensed under **CC BY 4.0** (`https://groups.inf.ed.ac.uk/ami/corpus/`).

## Cache layout (`~/.cache/meetingai-eval/`)

```text
~/.cache/meetingai-eval/
├── librispeech/
│   ├── test-clean.tar.gz                          (~330 MB archive)
│   ├── LibriSpeech/test-clean/<spk>/<ch>/*.flac   (~350 MB extracted)
│   └── convs/
│       └── <dev|test>/
│           ├── clean/<2|3|4>spk/<conv_id>.{wav,tsv}
│           ├── call/<2|3|4>spk/<conv_id>.{wav,tsv}
│           └── truth/<2|3|4>spk/<conv_id>.tsv
├── ami-hf/
│   ├── raw/<validation|test>-0000N-of-0000M.parquet   (~0.25–0.3 GB each)
│   └── <dev|test>/
│       ├── clean/4spk/<MEETING>.{wav,tsv}
│       ├── call/4spk/<MEETING>.{wav,tsv}
│       └── truth/4spk/<MEETING>.tsv
└── results/
    └── <host>/
```

### Truth TSV format

Every `.tsv` file alongside a conversation or meeting WAV has one line per
voiced turn, sorted by start time:

```text
speaker<TAB>start<TAB>end
```

where `start` and `end` are floating-point seconds from the start of the WAV.

### Expected sizes

- **LibriSpeech `test-clean`:** ~0.35 GB archive (`test-clean.tar.gz`, MD5
  `32fa31d27d2e1cad72775fee3f4849a9`), ~0.25 GB for 60 generated conversations
  (clean + Opus call condition).
- **AMI (Hugging Face `sdm`):** one validation and one test parquet (~0.55 GB),
  holding several meetings each; a rebuilt meeting is ~35–70 MB clean.

## Commands

### 1. LibriSpeech synthetic conversations

Fetch `test-clean` once if not already cached:

```bash
mkdir -p ~/.cache/meetingai-eval/librispeech
curl -fL -o ~/.cache/meetingai-eval/librispeech/test-clean.tar.gz \
    https://www.openslr.org/resources/12/test-clean.tar.gz
tar -xzf ~/.cache/meetingai-eval/librispeech/test-clean.tar.gz \
    -C ~/.cache/meetingai-eval/librispeech
```

Build the seeded 2-, 3- and 4-speaker conversations (turns 1–8 s, gaps
0.2–1.0 s, 0.3 s overlaps, disjoint fixed speaker lists for `dev` and `test`):

```bash
python3 -I tools/diarize/librispeech_convs.py \
    ~/.cache/meetingai-eval/librispeech/LibriSpeech/test-clean \
    ~/.cache/meetingai-eval/librispeech/convs
```

For a quick smoke slice (2 conversations per speaker count):

```bash
python3 -I tools/diarize/librispeech_convs.py \
    ~/.cache/meetingai-eval/librispeech/LibriSpeech/test-clean \
    ~/.cache/meetingai-eval/librispeech/convs \
    --per-count 2
```

### 2. AMI meetings (Hugging Face copy)

The Edinburgh mirror answers most whole-file downloads with HTTP 403 (rate-limited; only
some byte ranges get through), so the meetings come from the Hugging Face copy,
[`edinburghcstr/ami`](https://huggingface.co/datasets/edinburghcstr/ami) (CC BY 4.0),
config `sdm` (single distant microphone: every voice on one channel, like a call's
system audio). Each meeting is rebuilt on one track from its segments at `begin_time`.
The halves follow AMI's official partition, which keeps speakers apart: `validation` is
`dev`, `test` is held out.

```bash
nix shell --impure --expr 'with import <nixpkgs> {}; python3.withPackages (p: [ p.pyarrow ])' \
    -c python3 -I tools/diarize/ami_hf.py --meetings 4
```

`--meetings 1` gives the smoke slice (one `dev`, one `test` meeting).

### 3. Opus 16 kbps round trip (`call` condition)

Encode every `clean/` WAV to Opus at 16 kbps (`libopus`, VoIP application) and
decode back to 16 kHz mono WAV under `call/`:

```bash
bash tools/diarize/opus_roundtrip.sh ~/.cache/meetingai-eval/librispeech/convs
bash tools/diarize/opus_roundtrip.sh ~/.cache/meetingai-eval/ami-hf
```

Or on a single file:

```bash
bash tools/diarize/opus_roundtrip.sh IN.wav OUT.wav
```
