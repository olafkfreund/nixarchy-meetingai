---
status: approved
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
   turn its `id`, `channel` (0 = me, 1 = them), `start`, `end` in float seconds, `text`. In the
   turn loop (:598) `t` is in samples, so divide by `RATE`.
2. **`--exclude-ids FILE`:** remove these sentence IDs **before** selection, inside `load_npsc`
   (:485) and the new loader. Filtering afterwards would not refill the selection, because
   `load_npsc` stops at a target duration (:543). Each build writes `used_ids.json`, listing only
   the IDs actually placed in that build's 20 measured calls, not the loader's whole pool, which
   would starve the second sample. If fewer than 20 calls can be built, exit non-zero with the
   count.
3. **LibriSpeech loader:** `load_librispeech(dir, seed)` for the extracted `test-clean` under
   `~/.cache/meetingai-eval/librispeech/LibriSpeech`, returning the items `build_calls` takes,
   with each item's `duration` read from the audio, since `build_calls` filters on it (:579), and
   words lowercased as `norm_text` does (:325). `--language en` selects it in `main` (:840), under
   the manifest key and directory `libri`, and skips the NPSC and FLEURS checks that exit today
   (:871–877).
4. **`--calls-only`:** build `npsc` or `libri` with `double_talk` (:891), `remote_only` (:895),
   silence, room-tone, bursts and short words, and skip FLEURS, dictation and code-switch, which
   this work does not use. **Every measured set uses `max_calls=20`.** Without it, `build_calls`
   would use all of LibriSpeech (about 250 calls), leaving nothing for sample 2. Only `build_calls`
   sets get turn data; the guard-only sets do not need it. `SHORT_WORDS` (:51) is Norwegian only,
   so on English `build_short_words` uses its existing 1.2 s clip fallback (:705), and the results
   say that this English short-word guard is weaker.
5. **`tools/accuracy/selfcheck.py`** (new, assert-based, no downloads): on a tiny synthetic
   pool, turn data matches the WAV length; a repeat build with the same seed gives identical
   bytes; `--exclude-ids` removes those IDs; too small a pool exits non-zero.

Check: `python3 -I tools/accuracy/selfcheck.py` passes, and `python3 -I tools/accuracy/no_mix.py
--help` lists the flags. Do not build real sets in this step.

### 2. Scoring (coder) — `tools/accuracy/score.py`

1. **Per channel.** `score_ext` (:298) adds `wer_me` and `wer_them` beside `wer` (:346), and
   the single and suite reports (:445, :487) print them. Overall stays the sum of the two
   channels' edit distances over all reference words, and suite totals use the same sum.
2. **Rename** `lost` (:318) to `missing_words` everywhere it is used, including `score()` (:421)
   and `tot_lost` (:495, :516), printed as "missing owner words (includes misheard words and lines
   judged as bleed)" at :457, and in the suite line at :539.
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
   first word. `score_suite` writes the per-call JSON with a new `--json-out`: each call's kept
   lines, and `edits_me`, `ref_me`, `edits_them` and `ref_them`, so suites sum correctly.
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
2. **`--no-filter` (run B).** Read the WAV into `BatchChunks`, with the same tail silence as
   `replay`, and call `transcribe_chunk` on each chunk in order, with an empty prompt. In-process
   engines ignore the prompt. Print every chunk as `{"kind":"chunk","channel","start","end"}`, and,
   only when the text is non-empty (`handle_chunk_result` also skips empty text), a
   `{"kind":"line","channel","start","end","released_at","text"}` record, in chunk-finish order.
   Offline modes print `released_at` as `null`. Chunk errors exit non-zero. No wall-clock pacing.
3. **`--filter-lines FILE` (run B+F).** Branch in `main` before `resolve_engine` and the preload
   calls (:244–262), so no WAV, engine or model is needed. Read a run B log. Step an audio clock in
   0.1 s ticks up to the last chunk's end **plus 4 s of tail**, so the 3 s hold still runs on the last
   turn, as the replay's tail does. At each tick, **first** `filter.tick(now, earliest, open_end)`, then
   `accept` every line whose chunk has closed by `now`, in finish order. Use `"kind":"chunk"` spans, not
   lines, for the arguments, because empty chunks have no line: `earliest` is the start of the system
   chunk with `start <= now < end`, and `open_end` is `now` while one is open. With zero decode time no
   closed system chunk is ever pending, which is why this is simpler than `earliest_system` and
   `open_only_system_end` (transcription.rs:1192). A chunk cut at 15 s closes at `start + 15 s`, not at
   its cut point, so take the close time from when `BatchChunks::push` returned it, and print that as
   `"closed"` on the chunk record. Then `flush`. Print released lines.

   **This is an approximation, and is labelled so in the results:** decode time is treated as zero,
   so a line arrives when its audio ends, while in production it arrives when decoding finishes.
   B → B+F is "what the filter removes with no decode delay". B+F → C is reported separately, and can
   come from scheduling, concurrency or decode order, so it is not automatically charged to the filter.
4. **`--turns FILE` (run A).** For each turn in the turn data, cut `start`..`end` from that channel,
   and send it whole through `transcribe`. Print a line record for each non-empty result, with the
   turn's channel, start and end.
5. **`whisper_spike.rs`:** optional `--stem-prefix` (default `turbo-`, empty for NB-Whisper) and
   `--threads` (default 4). Add `--turns FILE WAV` printing the same JSON line records as run A, no
   VAD, so `score.py` can read its output. It rejects stereo today (:40); in `--turns` mode it accepts
   the call's stereo WAV and cuts the turn's channel. The plain single-file mode stays as it is.
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

## Deviations

(Recorded here during implementation, in the same commit as the code.)
- Step 1 (coder; review by Claude):
  - `load_librispeech(root, exclude)` has no seed, because `build_calls` already shuffles by seed.
  - `--language en` without `--calls-only` is an error.
  - A default build also writes `*.turns.json` now; `manifest.json` and the printed summary do not change.
  - `--slice N` replaces the 20-call count, for smoke runs.
  - **For step 4:** the English build uses `<cache>/librispeech` as its base, so it needs
    `--musan-dir ~/.cache/meetingai-eval/no/musan`, and each sample needs its own `--out-dir`.
  - `selfcheck.py` does not run `--calls-only`, `--language en` or NPSC exclusion through `main`.
    Claude read those paths; step 4's builds are their first real run.
- Step 2 (coder; README wording by Claude):
  - A notice invalidates a run if it starts with "lost speech for" or contains "failed". Both the
    queue-drop and failed-chunk paths use the first form (`lost_speech_notice`).
  - `--from-logs` and `--json-out` work in suite mode only, and logs are read from
    `DIR/<set>/<call>.jsonl`.
  - Not exercised by a check: a live suite through `run_replay`, and the single-file report path.
- Step 3 (Codex; step check by Claude):
  - `whisper_spike.rs` and `accuracy_replay.rs` each have their own copy of `cut_turn`; examples
    cannot share code without a new module, and the plan keeps `src/` untouched.
  - Run B's chunk records carry `closed`, the audio time `BatchChunks::push` returned the chunk.
  - The clock stepping has unit checks; a real run first happens in step 4.
- Steps 4–6 (Claude):
  - #77 removed NB-Whisper, so only Parakeet on English was measured. The Norwegian pair, C at 8× and A-noVAD were not run. **This narrows the approved plan and awaits the owner's approval.**
  - Results are in `docs/eval/2026-11-call-pipeline.md`. Under the decision rule they point to stopping.
