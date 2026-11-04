---
status: draft
issue: 61
spec: spec/2026-11-04-61-local-speaker-separation.md
coders:
  1: codex   # SpeakerLabeller: pure, small, threshold logic that must be exact
  2: agy     # dataset builders (LibriSpeech conversations, AMI subset, Opus round trip): large, mechanical scripts
  3: coder   # diarize_eval harness incl. --live mode: ordinary Rust, sizeable
  5: codex   # batch path: embed alongside transcription inside FuturesOrdered; subtle ordering
  6: codex   # daemon state: labeller lifetime, batch-vs-stream offsets, MAX from meeting_lines; tricky
  7: coder   # embedder per-call mutex, combined notice, setting, Settings toggle, fake daemon
reviewers:
  - codex    # read-only `codex review --base origin/main` on the integrated branch
  - agy      # read-only `agy --mode plan` review of the diff against this plan
---

# Plan: Tell remote speakers apart without sending audio anywhere

This plan implements the approved spec on `feat/61-local-speaker-separation`.
**Steps 1–3 measure. Step 4 is a hard stop for the owner. Steps 5–8 are
built only if the owner approves the numbers at step 4.**

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

### 1. `SpeakerLabeller` (codex)

1. Add `src/speech/speakers.rs`, exported from `src/speech/mod.rs`, with
   the labeller described above:
   - `SpeakerLabeller::new(first)`;
   - `label(&mut self, embedder, voiced_samples) -> Option<i64>`;
   - `raise_next(n)`;
   - `JOIN`, `NEW` and `MIXED` as `pub const` with provisional values
     (`0.55`, `0.35`, `0.45`), replaced at step 4.
   - The vector maths sits behind a `fn decide(&mut self, vectors:
     &[Vec<f32>]) -> Option<i64>`, so tests can feed fixed vectors.
2. Reuse `voice_check`'s voiced-frame selection and `cosine`/`normalise`.
   Make them `pub(crate)`; do not copy them.
3. Unit tests on fixed vectors:
   - the first speaker;
   - join;
   - a new speaker;
   - unsure gives no label;
   - each voiced-length band;
   - mixed at 3 s or more gives no label;
   - `MAX_SPEAKERS`;
   - the 20-vector ring;
   - `first` and `raise_next`.

Check: the step check.

### 2. Test sets (agy)

Add `tools/diarize/` with:

1. `librispeech_convs.py`: builds the synthetic conversations described
   above.
   - Seeded, so it is reproducible.
   - Writes 16 kHz mono WAVs plus a truth TSV
     (`speaker<TAB>start<TAB>end` per voiced turn).
   - Splits speakers into `dev` and `test` by a fixed list.
2. `ami_subset.sh`: downloads a fixed list of AMI meetings (Mix-Headset
   WAV and the word-level XML), converts them to 16 kHz mono, and writes
   the same truth TSV from the word times.
   - The meeting list is split into `dev` and `test` by meeting.
   - Uses `curl` and `ffmpeg` only.
3. `opus_roundtrip.sh`: `ffmpeg` to Opus at 16 kbps and back to 16 kHz WAV,
   for every file.
4. A `README.md` with the commands, the licences (LibriSpeech CC BY 4.0,
   AMI CC BY 4.0) and the cache layout.

Python uses only the standard library plus `soundfile`, which is in the dev
shell, or add it there.

Check: run the scripts on a small slice (2 conversations, 1 AMI meeting)
and inspect the TSVs. No cargo change.

### 3. `examples/diarize_eval.rs` (coder)

1. **Offline mode:**
   `diarize_eval MODEL SET_DIR [--condition clean|call] [--half dev|test] [--sweep]`.
   - Runs the real `BatchChunks` (`src/speech/stt/transcription.rs`) on
     each conversation.
   - Labels each chunk through the function `handle_chunk_result` will
     use. Expose it as `pub fn label_line(...)` in `speakers.rs`, so step 5
     and the harness share it.
   - Scores against the truth TSV with the rules above.
   - Prints one TSV row per condition and a summary.
   - `--sweep` prints the error/coverage curve over a grid of `JOIN`, `NEW`
     and `MIXED`.
   - Also prints embedding ms per line, CPU-seconds per minute
     (`getrusage`) and peak RSS delta.
2. **`--live` mode:**
   - Replays one call-condition AMI meeting in real time through
     `LiveTranscriber` (Parakeet, both channels: the AMI audio on channel 1
     and a LibriSpeech owner track on channel 0).
   - Runs the owner check on channel 0.
   - Records chunk-end-to-line for both channels and owner-check time,
     with separation off and then on.
   - Needs Parakeet available (`MEETINGAI_PARAKEET` or the configured
     model dir). It skips with a message otherwise.
3. Run both on p620 and razer. Write the outputs to
   `~/.cache/meetingai-eval/results/<host>/`.

Check: the step check (the example builds and clippy is clean), plus one
offline run on the step 2 slice.

### 4. Gate: the owner approves the numbers (Claude, then STOP)

1. **Claude runs and collects:**
   - the step 3 sweep on the dev half: pick `JOIN`, `NEW` and `MIXED` at
     the error bar with the most coverage;
   - the offline held-out numbers for both sets and conditions;
   - `--live` on p620 and razer.
2. **Writes `docs/eval/2026-11-diarization.md`** with:
   - the table of every bar against its measured value;
   - the chosen thresholds;
   - the proposed default for `speaker_separation`: on only if every bar
     passes.

   Posts the same table as a comment on #61.
3. **If any bar fails:** commit the eval doc, set this plan's steps 5–8 to
   "not built", and stop. The owner decides about the dedicated pipeline
   (spec S1).
4. **If all pass:** commit the measured constants into `speakers.rs`, and
   **stop for the owner's explicit approval of the numbers and the
   default**, recorded as a commit
   `docs(eval): approve diarization numbers (#61)`.

### 5. Batch path (codex)

In `src/speech/stt/transcription.rs`:

1. `SttHints` (~382) gains `labeller: Option<Arc<Mutex<SpeakerLabeller>>>`
   and `embedder: Option<Arc<dyn Embedder + Send + Sync>>`.
2. In `queue_chunks` (~1199), for channel 1 with both hints set:
   - the pending future becomes `join(transcribe, spawn_blocking(embed
     voiced windows))`;
   - the embedding is kept with the request, so a retry (~1208) reuses it.
3. `handle_chunk_result` (~1236): on non-empty `Ok` text for channel 1,
   `speaker = label_line(...)` with the precomputed vectors. Mic lines are
   never labelled.
4. **Tests, with a fake transcriber and a fake embedder:**
   - system lines are labelled and mic lines are not;
   - an empty or failed result gets no label;
   - a retry reuses the embedding;
   - a slow embedding delays the next mic line by at most
     `max(transcribe, embed)`;
   - the echo hold releases as before.

Check: the step check.

### 6. Daemon state (codex)

In `src/daemon/session.rs`:

1. **New fields:** `Inner` gains `labeller: Option<Arc<Mutex<SpeakerLabeller>>>`
   and `speech_batch: bool`.
2. **`record_start`** (~2289–2440):
   - set `speech_batch` from `!config.speech_streaming()`;
   - compute `speaker_offset` as `MAX(speaker) + 1` from `meeting_lines`
     for the meeting (replacing the `g.transcript` scan, ~2341);
   - when `new: true`, create the labeller after `reset_session` (~2437);
     otherwise reuse it, or create it at `first = speaker_offset` if none
     exists;
   - call `raise_next(speaker_offset)`;
   - put it and the embedder (step 7) in the `SttHints` for both
     connection paths, only when `speaker_separation` is on and the path is
     batch.
3. **`handle_live_event`** (~2070, ~2097): add `speaker_offset` to final and
   partial numbers only when `!g.speech_batch`.
4. **`session_restore`** (~3383) and `reset_session`: drop the labeller.
   The next `record.start` creates a fresh one at `MAX + 1`.
5. **Tests:**
   - pause and resume keep numbers, with no double offset on final and
     partial lines;
   - a switch from Deepgram to batch and back mid-meeting continues the
     numbering;
   - over 500 lines, `MAX` comes from `meeting_lines`;
   - restore starts at `MAX + 1`;
   - `new: true` gets a fresh labeller;
   - a local-only interview on Parakeet is labelled.

Check: the step check.

### 7. Embedder, notice, setting and shell (coder)

1. **`SherpaEmbedder`** (`voice_check.rs` ~79) wraps the extractor with a
   `Mutex`, locked per `embed()` call.
2. **`Daemon::embedder()`** (~2955):
   - load without the owner-enrollment and paused conditions;
   - those stay in the owner check (~3010, ~3015);
   - the load-failure notice becomes the combined message.
3. **The `speaker_separation` setting:**
   - in settings, with the step 4 default;
   - not in `MCP_SETTINGS` (`src/mcp/mod.rs`);
   - a test that MCP `settings.set` for it is refused;
   - a test that it turns labelling off.
4. **Shell:**
   - a Settings toggle next to the speech engine, using the existing toggle
     component and copy style;
   - `shell/tests/fake-daemon.py` answers the setting;
   - a smoke check that the toggle sends `settings.set`.
5. **Tests:**
   - with no enrolled owner voice and the owner check paused, the labeller
     still gets an embedder;
   - a missing model gives the one combined notice, and lines stay "Them".

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
- Step 4's numbers are approved by the owner before step 5 starts.

## Rollback

- **Before merge:** drop the branch.
- **After merge:** `git revert -m 1 <merge>`, revert the nixos lock bump,
  and switch.
- **Live switch-off:** `speaker_separation` turns it off without a deploy.
- **No database change.** Speaker numbers are the existing column, and
  older builds read them as today.

## Deviations

(Recorded here during implementation, in the same commit as the code.)
