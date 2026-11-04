---
status: draft
issue: 73
spec: spec/2026-11-04-73-call-pipeline-word-loss.md
coders:
  1: coder   # no_mix.py: ordinary additions to a Python builder
  2: coder   # score.py: ordinary additions to a Python scorer
  3: codex   # accuracy_replay.rs: small but tricky driving of the chunker and filter clocks
reviewers:
  - codex    # read-only: the code path and clock handling
  - agy      # read-only: tool and scorer correctness
---

# Plan: Measure the call pipeline's word loss, then decide

Implements the measurement and the decision gate. **No file under `src/` changes in
this plan.** A fix, if the numbers call for one, needs its own approved spec section and
plan steps. It is not covered here.

## Decisions carried from the approved spec

- **Pairs:** Parakeet on English, NB-Whisper small on Norwegian. No claim about others.
- **Runs, on the same turns of the same calls:**
  - **A, whole turns:** each turn's exact audio cut from the call, sent whole through
    `transcribe`.
  - **B, chunks:** the call through `BatchChunks` and `transcribe_chunk`, no filter;
    decoded chunks saved.
  - **B+F, chunks filtered:** B's saved lines through `LineFilter` offline, so the filter is
    the only changed step.
  - **C, production:** the call through `LiveTranscriber` at 1×.
  - **A-noVAD:** NB-Whisper only, A's turns on the same model with no VAD.
  - **C at 8×:** Norwegian only, compared with C at 1×, to explain #70's `--speed 8` figures.
- **Two samples per pair, sharing no sentence ID.** The build fails instead of reusing one.
- **Decision rule, fixed now; the owner sees all numbers and decides:**
  - one cause costs ≥ 2 points on the channel it affects, worse in most calls, in both
    samples → design a fix (S4);
  - no single cause, but A → C costs ≥ 2 points in both samples → the owner decides whether
    smaller fixes are worth it;
  - A → C under 2 points, or the samples disagree → correct the record and stop (S5).
  - "Artefact" is used only for a gap the runs show is not from the pipeline.
- **Guards for any later fix, against today's build:** bleed lines and microphone remote
  words on double-talk and remote-only sets; invented lines on silence, room-tone and burst
  sets; phantom phrases anywhere; short words kept; speech-to-line per channel, median and
  p95 within +0.1 s; NB-Whisper chunk decode median within +0.1 s; the gain holds in both
  samples.
- **Real call audio:** only if the owner provides it, consented, through `record.sh`, runs B
  and C. Otherwise the result states that only built calls were tested.

## Rules that must hold

- **No engine or model runs on 2026-11-04.** Steps 1–3 only build and run unit checks. Steps 4
  onward start on 2026-11-05 at the earliest.
- **Audio stays out of git and off cloud engines.** Sets live in `~/.cache/meetingai-eval/`.
- **Run Python with `-I`** on anything reading downloaded data. Extracted archives stay in the
  cache directory, and scripts stay in `tools/accuracy/`.
- **Keep what works.** `no_mix.py`'s default output and `score.py`'s existing reports keep their
  shape, except `lost` becoming `missing words`.

## Traps

- **sherpa-onnx build:** before cargo, run
  `find target/debug target/release -maxdepth 2 -name 'lib*.so*' ! -perm -u+w -delete`.
- **Codex and agy sandboxes cannot run nix or cargo.** Claude runs every check.
- **`tools/accuracy/__pycache__/` has a tracked `.pyc`.** Do not add more; `-I` still writes
  caches, so unstage them before committing.
- **`LineFilter::accept` uses `clock.max(line.end)` as arrival time.** Feeding saved lines
  needs ticks on the audio clock, or it behaves like the 8× case.
- **`score.py` and `no_mix.py` use word lists at their top.** `no_mix.py` lines 1–320 are data;
  grep before assuming a word such as "noise" is code.

## Steps

### 1. Test sets (coder) — `tools/accuracy/no_mix.py`

1. **Turn data.** `build_calls` (:557) writes `<name>.turns.json` beside each call: for each
   turn its `id`, `channel` (0 = me, 1 = them), `start`, `end`, `text`. In the turn loop (:598)
   it is known where each sentence is placed.
2. **`--exclude-ids FILE`:** remove these sentence IDs **before** selection, inside `load_npsc`
   (:485) and the new loader. Filtering afterwards would not refill the selection, because
   `load_npsc` stops at a target duration (:543). Each build writes `used_ids.json`. If fewer
   than 20 calls can be built, exit non-zero with the count.
3. **LibriSpeech loader:** `load_librispeech(dir, seed)` for the extracted `test-clean` under
   `~/.cache/meetingai-eval/librispeech/LibriSpeech`, returning the items `build_calls` takes,
   and using `--language en` to select it in `main` (:840). Words lowercased as `norm_text`
   does (:325).
4. **`--calls-only`:** build `npsc` or the English set with `double_talk` (:891), `remote_only`
   (:895), silence, room-tone, bursts and short words, and skip FLEURS, dictation and
   code-switch, which this work does not use.
5. **`tools/accuracy/selfcheck.py`** (new, assert-based, no downloads): on a tiny synthetic
   pool, turn data matches the WAV length; a repeat build with the same seed gives identical
   bytes; `--exclude-ids` removes those IDs; too small a pool exits non-zero.

Check: `python3 -I tools/accuracy/selfcheck.py` passes, and `python3 -I tools/accuracy/no_mix.py
--help` lists the flags. Do not build real sets in this step.

### 2. Scoring (coder) — `tools/accuracy/score.py`

1. **Per channel.** `score_ext` (:298) adds `wer_me` and `wer_them` beside `wer` (:346), and
   the single and suite reports (:445, :487) print them. Overall stays the sum of the two
   channels' edit distances over all reference words, and suite totals use the same sum.
2. **Rename** `lost` (:318) to `missing_words`, printed as "missing owner words (includes
   misheard words and lines judged as bleed)" at :457, and in the suite line at :539.
3. **Speech to line.** From `released_at` minus `end` on `"kind": "line"` records, median and p95
   per channel, in the report, for paced runs only: C at 1×. It is not meaningful for B, B+F, A
   or C at 8×, and is printed as "n/a" there. Per-chunk decode time (:374) stays.
4. **Notices.** Counts `"kind": "notice"` records, and marks the run invalid if any is a lost
   speech or failed chunk notice.
5. **`tools/accuracy/compare_runs.py`** (new): takes two suites' per-call JSON and prints each
   run's score per channel and overall, the difference, and in how many calls it was worse. For
   B and B+F it lists each removed line with its text and channel, matched on identical
   timestamps. The rule is inferred where the public API allows: an exact repeat, a silence
   phrase, otherwise "echo or loop". It also reports seconds of turn audio covered by no
   `"kind":"chunk"` span, labelled as an upper bound because it includes silence before the
   first word. `score_suite` writes the per-call JSON with a new `--json-out`.
6. **`--from-logs DIR`:** `score_suite` (:487) reads each call's saved JSONL from `DIR` instead of launching a
   replay. Today `--replay-log` (:546) only writes logs, and suite mode passes the same flags to every call, which
   cannot give B+F each call's own B log, or run `whisper_spike`.
7. **`selfcheck.py` additions:** per-channel error counts add up to the overall count; a misheard
   word appears in `missing_words`; a notice invalidates a run; speech-to-line per channel on
   hand-written records; `--from-logs` scores a hand-written log.

Check: `python3 -I tools/accuracy/selfcheck.py` passes.

### 3. Replay modes (codex) — `examples/accuracy_replay.rs`, `examples/whisper_spike.rs`

1. **Notices.** In the event match (:397–410), `LiveEvent::Notice` prints
   `{"kind":"notice","text":...}` instead of falling through `Some(_)`.
2. **`--no-filter` (run B).** After `live` setup is skipped, read the WAV into `BatchChunks`, with
   the same tail silence as `replay`, and call `transcribe_chunk` on each chunk in order, with an
   empty prompt. In-process engines ignore the prompt. Print each as a
   `{"kind":"line","channel","start","end","released_at","text"}` record, in chunk-finish order,
   and print chunk spans as `{"kind":"chunk",...}`. Chunk errors exit non-zero. No wall-clock
   pacing.
3. **`--filter-lines FILE` (run B+F).** Read a run B log. Step an audio clock in 0.1 s ticks up to
   the last chunk's end. At each tick, `accept` every line whose `end` has passed, in finish order,
   and call `filter.tick(now, earliest, open_end)`, computing the two arguments as the batch task
   does (`earliest_system`, `open_only_system_end`, transcription.rs:1192): `earliest` is the first
   system chunk that has begun with its line not yet accepted, and `open_end` is set only while a
   system chunk is open and no closed one is still pending. Then `flush`. Print released lines. No
   engine is loaded.

   **This is an approximation, and is labelled so in the results:** decode time is treated as zero,
   so a line arrives when its audio ends, while in production it arrives when decoding finishes.
   B → B+F is "what the filter removes with no decode delay". B+F → C is reported separately, and can
   come from scheduling, concurrency or decode order, so it is not automatically charged to the filter.
4. **`--turns FILE` (run A).** For each turn in the turn data, cut `start`..`end` from that channel,
   and send it whole through `transcribe`. Print a line record for each.
5. **`whisper_spike.rs`:** optional `--stem-prefix` (default `turbo-`, empty for NB-Whisper) and
   `--threads` (default 4). Add `--turns FILE WAV` printing the same JSON line records as run A, no
   VAD, so `score.py` can read its output.
6. **Unit checks** in the example, with no model: the clock stepping on hand-written lines holds a
   microphone line 3 s, then releases it; an echo of a system line is removed; the same turn cut
   twice is identical.

Check, by Claude: the step check below. Do not load a real model.

```bash
find target/debug target/release -maxdepth 2 -name 'lib*.so*' ! -perm -u+w -delete
nix develop -c bash -c 'cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check'
```

`git diff origin/main -- src` must be empty.

### 4. Build sets and sanity checks (Claude, from 2026-11-05)

1. Build Norwegian and English samples 1 and 2, with the second using `--exclude-ids` from the first.
   If either fails on supply, stop and tell the owner.
2. Confirm the Norwegian sample 1 has the #70 call shape, with about 3,200 reference words.
3. Sanity: identical audio for a repeated seed; no shared sentence IDs; B+F against C on a Parakeet
   English sample; C at 1× on Parakeet English reproduces itself within 1 point.

Stop and report if a sanity check fails.

### 5. Measure (Claude)

All on p620, with `NIXARCHY_MEETINGAI_WHISPER_NB_THREADS=16` for NB-Whisper as in #70. Check the bus and
that no recording is active before heavy runs. Nothing else loads the machine during C at 1×. Logs stay in
`~/.cache/meetingai-eval/results/p620/73/`.

| Run | Pairs and samples | How |
|---|---|---|
| A | both, both samples | `accuracy_replay CALL.wav --engine E --turns CALL.turns.json` |
| B | both, both samples | `accuracy_replay CALL.wav --engine E --no-filter > CALL.b.jsonl` |
| B+F | both, both samples | `accuracy_replay --filter-lines CALL.b.jsonl` |
| C at 1× | both, both samples | `accuracy_replay CALL.wav --engine E` (existing path) |
| C at 8× | Norwegian, sample 1 | the same, with `--speed 8` (existing flag) |
| A-noVAD | Norwegian, both samples | `whisper_spike MODEL_DIR --stem-prefix "" --threads 16 --turns CALL.turns.json CALL.wav` |
| Guard baselines | both pairs, today's build, 1× | `score.py` on double-talk, remote-only, silence, room-tone, bursts and short-word sets |

All modes are logged first, then scored against each call's reference with step 2's `--from-logs`. The guard runs and C at 1× together save
the baselines for any later fix: bleed lines, microphone remote words, invented lines, phantom phrases, short
words, speech-to-line per channel, and NB-Whisper chunk decode time.

### 6. Results and gate (Claude, then STOP)

Write `docs/eval/2026-11-call-pipeline.md`: every run per channel and sample, per-call direction, each
cause's cost, removed lines, the 8× against 1× result, the VAD figure, notices, and the limits. Apply the
decision rule and recommend. **Stop for the owner's decision.** No fix without a new approved spec section
and plan steps.

### 7. If it stops here (Claude)

For the S5 outcome: correct `docs/eval/2026-11-norwegian-stt.md`, then PR, merge after CI, and close #73 with
the figures. The tools land in the same PR. No deploy, since nothing under `src/` changes.

## Tests

- `python3 -I tools/accuracy/selfcheck.py` passes after steps 1 and 2.
- The step check above passes after step 3, with an empty `src/` diff.
- `nix flake check` passes before the PR.
- Reviews before the PR: a fresh Opus agent, `codex review`, and agy, all read-only.

## Rollback

Nothing deployed changes. `git revert` the merge; sets and logs in the cache can be deleted.
