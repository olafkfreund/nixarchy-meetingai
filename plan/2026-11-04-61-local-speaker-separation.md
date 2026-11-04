---
status: approved
issue: 61
spec: spec/2026-11-04-61-local-speaker-separation.md
coders:
  1: codex   # speakers.rs (embed_windows, SpeakerLabeller, label_line) + per-call embedder mutex: exact logic
  2: codex   # dormant batch-path plumbing (SttHints, join, label before emit): subtle FuturesOrdered ordering
  3: agy     # dataset builders (LibriSpeech conversations, AMI subset, Opus round trip): large, mechanical scripts
  4: coder   # diarize_eval harness incl. --live mode: ordinary Rust, sizeable
  6: codex   # daemon wiring: labeller lifetime, batch-vs-stream offsets, MAX from meeting_lines; tricky
  7: coder   # combined notice, Settings toggle, fake daemon, MCP refusal test
reviewers:
  - codex    # read-only `codex review --base origin/main` on the integrated branch
  - agy      # read-only `agy --mode plan` review of the diff against this plan
---

# Plan: Tell remote speakers apart without sending audio anywhere

This plan implements the approved spec on `feat/61-local-speaker-separation`.
**Steps 1–4 build what the measurement needs. Step 5 is a hard stop for the
owner. Steps 6–8 are built only if the owner approves the numbers.**

Steps 1–2 add the labeller and the batch-path plumbing in a dormant form:
- `SttHints` defaults to no labeller, and the daemon does not set one until
  step 6, so production behaviour is unchanged;
- this is the only way to measure latency on the real batch path (spec S1
  `--live`) before the gate;
- the one change that is live at once is the per-call embedder lock, which
  also closes the owner check's existing concurrent use (spec S6);
- if the gate fails, steps 1–2 are reverted, except the lock.

## Decisions carried over from the approved spec

**Labeller (S4).** `SpeakerLabeller` is a pure, synchronous type in
`src/speech/speakers.rs`. It is fed vectors from the `Embedder` trait
(`src/speech/voice_check.rs` ~75).

- **Voiced samples:** 30 ms frames with `rms ≥ 0.005`, the same as
  `voice_check::speech`.
- **Windows, by voiced length:**
  - under 1.5 s: no label;
  - 1.5 s to under 3 s: the whole voiced audio is one vector, with no
    mixed check;
  - 3 s or more: 2–4 non-overlapping 1.5 s windows, spread evenly. If any
    pair's cosine is below `MIXED`, there is no label. Otherwise the vector
    is the normalised mean of the windows.
- **Assignment:**
  - no known speaker yet: create speaker `first`;
  - otherwise take the best cosine against each speaker's centroid, the
    normalised mean of a ring of their last 20 vectors;
  - at least `JOIN`: assign the speaker and push the vector to their ring;
  - below `NEW`, with fewer than `MAX_SPEAKERS` (8) known: create a new
    speaker;
  - else: no label.
- Numbers are absolute, starting at `first`. `raise_next(n)` makes the next
  new number at least `n`.
- `JOIN`, `NEW` and `MIXED` come from step 3's dev-half sweep.

**Measurement (S1).**
- **Sets:** LibriSpeech `test-clean` synthetic conversations (2–4
  speakers, turns of 1–8 s, gaps of 0.2–1.0 s, some 0.3 s overlaps) and an
  AMI subset (real meetings).
- **Conditions:** both clean and after an Opus round trip at 16 kbps.
- **Split:** speakers divide into a dev half and a held-out half.
- **Truth:** a line is single-speaker when one speaker has at least 90% of
  its voiced time, else mixed. It is taken from sample times.
- **Scoring:** on emitted lines, through the same labelling function the
  batch path uses, with a fake transcriber. Failed and empty lines are
  dropped.
- **Metrics:**
  - errors: wrong single-speaker labels, plus every labelled mixed line;
  - coverage: single-speaker lines with 1.5 s or more voiced that got a
    number;
  - mixed abstention, reported by voiced length;
  - cost on p620 and razer: embedding time per line, CPU-seconds per
    minute, added RSS, closure growth;
  - `--live` latency, with Parakeet, the owner check and separation all
    on: chunk end to line for system and mic lines, and owner-check time,
    each compared with separation off.

**Gate (S2), on the held-out half of AMI after the Opus round trip:**
- errors: at most 1 in 20;
- coverage: at least 4 in 5;
- mixed abstention: at least 3 in 4 for mixed lines with 3 s or more
  voiced;
- latency: median chunk-end-to-line rises by under 100 ms for system and
  mic lines; owner-check results unchanged, and its time up by under
  100 ms;
- cost: under 100 MB added RSS on razer, and under 5 CPU-seconds per
  minute.

If it fails, stop and write the numbers into #61.

**Not a concern:** `LiveTurns` splitting answer turns when the speaker
changes (`transcription.rs` ~98–134). Batch lines are all `speech_final`, so
each line already ends its turn. Do not add turn logic (spec review
record).

**Runtime (S3, S5–S7):**
- **Batch path:**
  - a channel-1 chunk's embedding is `join`ed with its transcription in
    the same `FuturesOrdered` future;
  - it is computed once per chunk, and a retry reuses it;
  - its result is dropped when the text is empty or the call fails;
  - `handle_chunk_result` labels the line before `emit_lines`.
- **How the labeller and embedder reach it:** they travel in `SttHints` on
  both connection paths (`ListenSession::start`,
  `LiveTranscriber::connect_config_with`).
- **Labeller lifetime:**
  - there is one labeller per meeting, in `Inner`;
  - it is created after `reset_session` when `new: true`;
  - it is reused on pause and resume;
  - restore starts a fresh one at `MAX(speaker) + 1`.
- **Offsets:**
  - `speaker_offset` is added to final and partial lines only when
    `g.speech_batch` is false;
  - at every `record.start` it is `MAX(speaker) + 1` from `meeting_lines`,
    and the labeller is raised to at least that.
- **Embedder sharing:**
  - one shared `VoiceCheck.embedder`;
  - `SherpaEmbedder::embed` locks a mutex per call;
  - loading no longer needs an enrolled owner voice or an unpaused owner
    check.
- **Load failure:** one combined notice, "The speaker model could not be
  loaded: the owner voice check and speaker separation are off."
- **Setting:** `speaker_separation` (bool), default from step 4, shown
  next to the speech engine.
  - It applies only on the batch path.
  - It is not in `MCP_SETTINGS`, and a test pins that.
  - Deepgram is unchanged.

## How it runs

- **Step check:**

  ```bash
  nix develop -c bash -c 'cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check'
  ```

  Steps 7–8 add `bash shell/tests/ci.sh`.
- One commit per step, `<type>(<area>): … (#61, plan step N)`. Deviations
  go under **Deviations** in the same commit.
- Each new test fails before its change.
- **Repo traps:**
  - Before any cargo build, delete the read-only sherpa library copies:
    `find target/debug -maxdepth 2 -name 'lib*.so*' ! -perm -u+w -delete`.
    `sherpa-onnx-sys`'s build script fails with EACCES after an
    environment change.
  - `Inner` is a `parking_lot` mutex; never hold it across `.await`.
  - `session.rs` is about 9 000 lines; anchors are approximate (`~`).
  - Handoffs: `agent-run … < /dev/null`.
- **Test data and results** live in `~/.cache/meetingai-eval/` and are never
  committed. They are about 0.4 GB of LibriSpeech `test-clean` and a few
  GB of the AMI subset. The scripts that fetch and build them are
  committed.

## Steps

### 1. `speakers.rs` and the per-call embedder lock (codex)

1. Add `src/speech/speakers.rs`, exported from `src/speech/mod.rs`:
   - **`pub fn embed_windows(embedder: &dyn Embedder, samples: &[f32]) ->
     Vec<Vec<f32>>`.** It takes a chunk's **raw** samples and selects
     voiced frames with `voice_check::speech`. Let `L` be the voiced
     length and `W` = 1.5 s (`WINDOW_SAMPLES`):
     - `L < W`: return `[]`;
     - `W ≤ L < 2W`: embed all voiced samples once, giving `[v]`;
     - `L ≥ 2W`: let `k = min(4, L / W)`, take windows at offsets
       `i·(L − W)/(k − 1)` for `i` in `0..k` (non-overlapping because
       `k·W ≤ L`), and embed each.
     - Drop windows the embedder rejects (`None`).
   - **`SpeakerLabeller`**, with `new(first)`, `raise_next(n)` and
     `decide(&mut self, vectors: &[Vec<f32>]) -> Option<i64>`, holding the
     rules above:
     - `[]`: no label;
     - one vector: no mixed check;
     - two or more vectors: the mixed check, then the normalised mean;
     - then the assignment rules.
   - **`pub fn label_line(labeller: &Mutex<SpeakerLabeller>, vectors:
     &[Vec<f32>]) -> Option<i64>`**: locks and calls `decide`. The harness
     and the batch path both use it.
   - **`JOIN`, `NEW` and `MIXED`** are `pub const`, provisionally `0.55`,
     `0.35` and `0.45`, replaced at step 5.
2. In `src/speech/voice_check.rs`, make `speech`, `normalise` and
   `WINDOW_SAMPLES` `pub(crate)` (`cosine` is already `pub`). Do not copy
   them.
3. **`SherpaEmbedder`** (`voice_check.rs` ~79) wraps its extractor in a
   `Mutex`, locked inside each `embed()` call only.
4. **Tests:**
   - `embed_windows` with a fake `Embedder` and synthetic samples: each
     voiced-length band, and the window count and offsets at 3.0 s, 4.5 s,
     6.0 s and 10 s;
   - `decide` on fixed vectors: the first speaker, join, a new speaker,
     unsure, mixed, `MAX_SPEAKERS`, the 20-vector ring, `first` and
     `raise_next`;
   - the existing `voice_check` tests still pass.

Check: the step check.

### 2. Dormant batch-path plumbing (codex)

In `src/speech/stt/transcription.rs`:

1. **`SttHints`** (~381) gains `speakers: Option<SpeakerHints>`, where
   `SpeakerHints { labeller: Arc<Mutex<SpeakerLabeller>>, embedder:
   Arc<dyn Embedder + Send + Sync> }`.
   - Implement `Debug` for `SpeakerHints` by hand (printing `"…"`), so
     `SttHints` keeps `#[derive(Debug, Clone, Default)]`.
   - `Default` is `None`, so every existing caller (~1574, ~1713, …) is
     unchanged.
2. **`queue_chunks`** (~1199): for a channel-1 chunk when `speakers` is set,
   the pending future becomes
   `join(transcribe, spawn_blocking(embed_windows(embedder, samples)))`.
   - The vectors stay with the request, so a retry (~1208) does not embed
     again.
   - With `speakers` unset, the future is exactly as today.
3. **`handle_chunk_result`** (~1236): on non-empty `Ok` text for channel 1
   with vectors, set `speaker = label_line(&labeller, &vectors)` before
   `filter.accept` and `emit_lines`. Mic lines are never labelled.
4. **Tests, with a fake transcriber (as the existing batch tests use) and a
   fake embedder:**
   - with `speakers`, system lines are labelled and mic lines are not;
   - without it, nothing changes;
   - an empty or failed result gets no label;
   - a retry embeds once;
   - a slow embedding delays the next mic line by at most
     `max(transcribe, embed)`;
   - the echo hold (`open_only_system_end`, `earliest_system`) releases mic
     lines as before.

Check: the step check.

### 3. Test sets (agy)

Add `tools/diarize/` (data in `~/.cache/meetingai-eval/`, never committed),
using the standard-library Python plus `ffmpeg`, `curl` and `unzip`, which
are in the dev shell. Do not add `soundfile`: decode with `ffmpeg` and write
with `wave`.

1. **`librispeech_convs.py`** builds the synthetic conversations described
   above.
   - Seeded, so it is reproducible.
   - Writes 16 kHz mono WAVs plus truth `speaker<TAB>start<TAB>end` per
     voiced turn.
   - Speakers are split `dev`/`test` by a fixed list, with no speaker in
     both halves.
2. **`ami_subset.sh`** downloads a fixed list of AMI meetings from the
   Edinburgh mirror (`Mix-Headset.wav`, about 35–70 MB each), plus the
   manual annotations zip (`ami_public_manual_1.6.2.zip`, about 22 MB).
   - `ami_words.py` turns `words/<MEETING>.<A-D>.words.xml` into the same
     truth TSV (`xml.etree`).
   - The split is **by scenario**, for example all of `ES2002*` in `dev`
     and `ES2003*` in `test`, because the same people appear in every
     meeting of a scenario.
   - About 8 meetings in total, under 1 GB.
3. **`opus_roundtrip.sh`**: `ffmpeg` to Opus at 16 kbps and back to
   16 kHz WAV.
4. **`README.md`**: the commands, licences (LibriSpeech and AMI, both CC BY
   4.0), the cache layout and the expected sizes.

Check: run on a slice of 2 conversations per speaker count and 2 AMI
meetings (one `dev`, one `test`), and inspect the TSVs.

### 4. `examples/diarize_eval.rs` (coder)

1. **Offline mode:**
   `diarize_eval MODEL SET_DIR --condition clean|call --half dev|test [--sweep]`.
   - Chunk each conversation with the real `BatchChunks`.
   - For each chunk, run `embed_windows` and `label_line`, then **the real
     `LineFilter::accept`**, with a fake transcriber that returns fixed
     text. Score only the lines that come out, so the denominator is what a
     user sees.
   - **Truth** comes from sample times: single-speaker when one speaker has
     at least 90% of the voiced time, else mixed.
   - **Errors** are taken after the **best one-to-one mapping** of
     labeller numbers to true speakers per conversation (exhaustive: at
     most 8 numbers by 5 speakers). Every labelled mixed line counts as an
     error.
   - **Output:** one TSV row per set × condition × **number of speakers**,
     with errors, coverage, and mixed abstention split by voiced length
     (under 3 s, 3 s or more), plus embedding ms per line,
     CPU-seconds per minute (`getrusage`) and peak RSS delta.
   - **`--sweep`** prints the error/coverage curve over a grid of `JOIN`,
     `NEW` and `MIXED`.
2. **`--live` mode:**
   - Replays a **5-minute slice** of one call-condition AMI meeting in real
     time through `LiveTranscriber::connect_config_with`, with Parakeet
     located by `parakeet::model_dir()` (`NIXARCHY_MEETINGAI_PARAKEET_DIR`)
     and preloaded with `parakeet::preload`.
   - Channel 1 is the AMI audio; channel 0 is a LibriSpeech owner track
     checked by `voice_check::check` with the same embedder.
   - Run once with `SttHints.speakers` unset and once with it set.
   - Records, for both channels, chunk end to line, and owner-check time
     per line.
   - Missing Parakeet or model: **exit non-zero** with the reason.
3. Run on p620 and razer. Write the outputs to
   `~/.cache/meetingai-eval/results/<host>/`.

Check: the step check (the example builds, clippy is clean), plus one
offline run on the step 3 slice.

### 5. Gate: the owner approves the numbers (Claude, then STOP)

1. **Claude runs and collects:**
   - the dev-half sweep: pick `JOIN`, `NEW` and `MIXED` at the error bar
     with the most coverage;
   - the held-out offline numbers for both sets, conditions and speaker
     counts;
   - `--live` on p620 and razer.
2. **Writes `docs/eval/2026-11-diarization.md`** with:
   - every bar against its measured value;
   - the chosen thresholds;
   - the proposed default for `speaker_separation`: on only if every bar
     passes.

   Posts the same table on #61.
3. **If any bar fails:**
   - commit the eval doc;
   - revert steps 1–2's labeller and plumbing, but keep the per-call lock;
   - mark steps 6–8 "not built" here;
   - stop. The owner decides about the dedicated pipeline (spec S1).
4. **If all pass:**
   - commit the measured constants into `speakers.rs`;
   - **stop for the owner's explicit approval** of the numbers and the
     default, recorded as `docs(eval): approve diarization numbers (#61)`.

### 6. Daemon wiring (codex)

In `src/daemon/session.rs` and `src/core/models/settings.rs`:

1. **Setting:** `Settings` gains `speaker_separation: bool`, with the
   step 5 default. It is not in `MCP_SETTINGS` (`src/mcp/mod.rs`).
2. **`Inner`** gains `labeller: Option<Arc<Mutex<SpeakerLabeller>>>` and
   `speech_batch: bool`.
3. **`record_start`:**
   - **Where.** All of it goes in the **second** lock block, after
     `reset_session` (~2437, run for `new: true`) and after the meeting
     `id` is bound (~2481). The first block (~2341) runs before the reset
     and may still hold the old meeting.
   - **`speech_batch`** = `!speech_streaming()` of the configured speech
     provider, taken from settings, so it is also set when `capture` is
     false (unit tests).
   - **`speaker_offset`** = `MAX(speaker) + 1` from `meeting_lines` for that
     id, replacing the `g.transcript` scan at ~2341.
   - **Labeller:** reuse it on resume; create it at
     `first = speaker_offset` when there is none; then
     `raise_next(speaker_offset)`.
4. **`fn speaker_hints(&self) -> Option<SpeakerHints>`:**
   - returns `None` unless `speaker_separation` and `speech_batch` are both
     set;
   - takes the labeller from `Inner`;
   - loads the embedder through `self.embedder()` **on a blocking worker**,
     never under `inner`.

   `recording_task` calls it before `connect_config_with` (~2543), the only
   place the daemon connects speech: `ListenSession::start` (~2403) opens
   audio only. Unit tests call `speaker_hints()` directly after `use_fake`.
   Calling it before `use_fake` would mark the model failed, so
   `record.start` alone never loads the model when `capture` is false.
5. **`handle_live_event`** (~2070 final, ~2097 partial): add
   `speaker_offset` only when `!g.speech_batch`.
6. **`session_restore`** (~3383) and `reset_session` (~3285) drop the
   labeller.
7. **Tests, through `inject_live` and `speaker_hints`:**
   - pause and resume keep numbers, with no double offset on final and
     partial lines;
   - a switch from Deepgram to batch and back mid-meeting continues the
     numbering;
   - after more than 500 lines, `MAX` comes from `meeting_lines`;
   - restore starts at `MAX + 1`;
   - `new: true` gets a fresh labeller;
   - the setting off gives `None`;
   - a local-only interview with a batch provider gets hints;
   - the existing tests that call `record.start` before `use_fake` still
     pass.

Check: the step check.

### 7. Notice, Settings and shell (coder)

1. **`Daemon::embedder()`** (~2955): the owner-enrollment and paused
   conditions already live only in the owner check (~3010, ~3015). The one
   change is the load-failure message (~2978–2981), which becomes the
   combined one.
2. **Tests:**
   - with no enrolled owner voice and the owner check paused,
     `speaker_hints()` still gets an embedder;
   - a missing model gives one combined notice, and `speaker_hints()` is
     `None`;
   - MCP `settings.set speaker_separation` is refused.
3. **Shell:**
   - a toggle in `shell/Overlay.qml`'s Settings, next to the speech engine,
     using the existing toggle component and copy style, shown only for
     batch engines;
   - `shell/tests/fake-daemon.py` answers the setting;
   - a smoke check that the toggle sends `settings.set`.

Check: the step check plus `bash shell/tests/ci.sh`.

### 8. Integrate, review, ship (Claude)

1. Merge `main`. Run the step check, `ci.sh`, `nix flake check -L
   --no-build` and `nix build .#checks.x86_64-linux.shell -L`.
2. **Reviews, read-only and in parallel:**
   - a fresh Opus agent, given only this plan path and
     `git diff origin/main`;
   - `codex review --base origin/main -c sandbox_mode="read-only"`;
   - agy `--mode plan`.

   Verify each finding, fix the confirmed ones, and re-run the checks.
3. **PR:**
   - link intent, spec, plan and the eval doc;
   - list who did each step and any fallbacks;
   - merge with a merge commit after CI.
4. **Deploy p620** the meetingai-only way: lock bump in a nixos worktree,
   PR, deploy from `main`, bus announcement, not while recording.
5. **The owner's live check:** a real 3-person call on Parakeet.

## Tests

- The step check is green after every step, and `ci.sh` and `nix flake
  check` are green at step 8.
- Every new test fails before its change.
- Step 5's numbers are approved by the owner before step 6 starts.

## Rollback

- **Before merge:** drop the branch.
- **After merge:** `git revert -m 1 <merge>`, revert the nixos lock bump,
  and switch.
- **Live switch-off:** `speaker_separation` turns it off without a deploy.
- **No database change.** Speaker numbers are the existing column, and
  older builds read them as today.

## Review record (draft plan)

| Finding | From | Verdict |
|---|---|---|
| The `--live` gate needs the batch plumbing and the lock, which came after the gate | Codex, agy | confirmed → steps 1–2 build them dormant before the gate; reverted if it fails |
| Step 6 needed step 7's setting and embedder | Codex | confirmed → the setting moves to step 6; `speaker_hints()` for tests |
| `record_start`: the first lock block runs before `reset_session` and may hold the old meeting; eager embedder load breaks tests | agy | confirmed → second block; load on a blocking worker in `speaker_hints()` |
| `label_line` and precomputed windows split inconsistently; no window formula | agy | confirmed → `embed_windows`, `decide` and `label_line` all in step 1, with an exact formula |
| The harness must score lines after `LineFilter` | Codex | confirmed → step 4 runs the real `LineFilter::accept` |
| `SttHints` derives `Debug`; `Embedder` is not `Debug` | Codex | confirmed → `SpeakerHints` with a hand-written `Debug` |
| The `LiveTurns` conclusion was missing | Codex | confirmed → in the decisions |
| `speech_batch` would be unset when `capture` is false | agy | confirmed → taken from settings |
| Missing: best one-to-one mapping, per-speaker-count output, AMI split by scenario | agy | confirmed → steps 3–4 |
| AMI words come as an XML zip; `soundfile`; Parakeet env var; `--live` length; a missing Parakeet should fail | agy | confirmed → `unzip`/`xml.etree`, `ffmpeg` + `wave`, `parakeet::model_dir`, a 5-minute slice, exit non-zero |
| Visibility of helpers; step 7 file paths; `embedder()` already free of owner conditions | agy | confirmed → steps 1 and 7 |

## Deviations

(Recorded here during implementation, in the same commit as the code.)
