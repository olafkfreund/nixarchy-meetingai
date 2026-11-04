---
status: draft
issue: 61
intent: intent/2026-11-04-61-local-speaker-separation.md
---

# Spec: Tell remote speakers apart without sending audio anywhere

The approved intent asks for local speaker numbers on remote lines from the
batch path (Parakeet, and loopback OpenAI-compatible servers). They must be:
- measured, and approved by the owner, before they ship;
- live;
- stable within a session, including pause and resume;
- never guessed when unsure;
- independent of the owner voice check.

The design has two halves:
- **S1–S2:** a measurement harness and a decision gate;
- **S3–S7:** the runtime labeller.

The runtime half is built only if S2's gate passes.

## Design

### S1. Measure first: `examples/diarize_eval.rs`

An offline harness, in the style of `examples/voice_eval.rs`, that runs the
real chunker and the S4 labeller on labelled conversations and prints the
numbers the intent asks for.

**Input.** A TSV of conversations, each a list of
`speaker<TAB>wav<TAB>gap_seconds` turns. A small script builds them from
LibriSpeech `test-clean` (the set `voice_eval` already uses):
- 2, 3 and 4 speakers;
- turns of 1–8 s;
- gaps of 0.2–1.0 s, so about a third are under the 0.6 s chunk close
  (hand-offs within a chunk);
- some 0.3 s overlaps (crosstalk).

Each conversation is also passed through Opus at 16 kbps and back with
`ffmpeg`, which is what call audio on the system channel looks like. Both
the clean and the "call" versions are scored.

The test audio stays outside the repo, as for `voice_eval`. The script and
the TSV format are committed.

**Processing.** Chunk with the real `BatchChunks` (`QUIET_CLOSE` 0.6 s,
`LONGEST` 15 s), label each chunk with S4, and compare against the
majority-speaker truth of the chunk.

**Output, per condition (clean or call, by number of speakers):**
- **errors:** labelled chunks whose speaker does not match truth, after the
  best one-to-one mapping of numbers to speakers;
- **coverage:** chunks of 1.5 s or longer that got a number;
- **mixed-chunk handling:** how many truly mixed chunks were left
  unlabelled;
- **cost:** embedding time per chunk, and the process RSS added by the
  labeller.

A threshold sweep (S4's `JOIN` and `NEW`) prints the error/coverage curve.

**A dedicated pipeline is compared only if CAM++ misses the bar.** That is
sherpa-onnx offline diarization (a segmentation model plus embedding),
scored in the same harness. It adds a model to the closure (about 6 MB), so
the reuse-first constraint only allows it on evidence.

### S2. The gate

The owner approves the measured numbers before S3–S7 are built. The
proposed bar on the Opus "call" condition, 2–4 speakers:
- at most 1 labelled chunk in 20 wrong;
- at least 4 in 5 chunks of 1.5 s or longer labelled;
- embedding time under 100 ms per chunk on razer;
- added RSS under 100 MB.

**If it passes:** the measured thresholds and the default setting go into
the plan.

**If it fails:** the plan stops here, and the result is written into #61
with the numbers. Any dedicated-pipeline result from S1 then goes to the
owner as the next option.

### S3. Where labelling happens: inside the batch path, before the line leaves

`batch_task` (`src/speech/stt/transcription.rs` ~1279) already holds each
system chunk's samples (`BatchChunk` ~387) while it is transcribed (up to
`MAX_IN_FLIGHT` 4 at once).

- For a channel-1 chunk, the speaker embedding is computed on a blocking
  worker **in parallel with transcription**, from the same samples.
- `handle_chunk_result` (~1247) sets `TranscriptLine.speaker` before
  `emit_lines`.

Labelling before the line is sent means:
- no relabel event is needed;
- the stored row, live view, answers, `cite_window`, rolling notes and
  recap all see the final speaker;
- nothing waits on `settle_checks`.

Transcription takes hundreds of milliseconds per chunk and the embedding
tens, so the line is not delayed. S1 measures this.

A chunk whose transcription fails or is empty is not labelled.

### S4. The labeller: online clustering of CAM++ embeddings

A pure, synchronous `SpeakerLabeller` in `src/speech/` decides; the
`Embedder` trait supplies vectors. It can be unit-tested on fixed vectors.

**Embed.** The chunk's voiced samples (`rms ≥ 0.005`, as `voice_check`
uses) are embedded in 1.5 s windows, up to 4, spread over the chunk.
- With fewer than 1.5 s voiced, there is no label.
- If two windows' cosine is below `MIXED` (measured), the chunk is
  treated as mixed and gets no label.
- Otherwise the windows' normalised mean is the chunk's vector.

**Assign.**
- The best cosine against each known speaker's centroid (a running mean of
  normalised vectors, capped at the last 20 assignments) decides.
- If it is at least `JOIN`, that speaker is assigned and the centroid
  updated.
- If it is below `NEW`, and fewer than `MAX_SPEAKERS` (8) are known, a new
  speaker is created.
- Otherwise there is no label: the line stays "Them".

**Numbers.** Speaker numbers are absolute within the meeting, starting at
the `first` number the daemon gives (see S5).

`JOIN`, `NEW` and `MIXED` are constants set from S1's sweep.

### S5. State across pause, resume and restore

- The labeller lives in the daemon (`Inner`) per meeting, behind
  `Arc<Mutex<_>>`, and is handed to `connect_batch`. A pause or resume
  (`record.start` with `new: false`) reuses it, so the same people keep
  their numbers.
- Batch lines already carry absolute numbers, so `speaker_offset` is **not**
  added to them. It stays only for providers that number their own
  speakers from 0 per connection, which today means Deepgram
  (`session.rs` ~2070).
- On `session_restore`, or a new meeting, a fresh labeller starts at
  `first = MAX(speaker) + 1` from `meeting_lines` (as `speaker_offset` is
  computed today, ~3394). Earlier voices get new numbers, and no voice
  prints are stored (intent question 4).
- Centroids exist only in memory and are dropped with the meeting
  session.

### S6. Sharing the model, and independence from the owner check

- The labeller uses the daemon's one `VoiceCheck.embedder`
  (`Arc<dyn Embedder + Send + Sync>`, `session.rs` ~248), loaded lazily on a
  blocking worker as today. The model is in RAM once.
- Loading no longer requires an enrolled owner voice or an unpaused owner
  check. Those conditions stay on the owner check itself (~3010, ~3015).
- If the model is missing or fails to load, lines stay "Them". A one-time
  notice says speaker separation is off, and nothing else changes.
- `SherpaEmbedder` calls are serialised through one mutex, so the two
  users cannot run the model concurrently. S1 confirms the owner check's
  `me`/`room` results and timing are unchanged with both active.

### S7. Setting, scope and agents

- A new setting, `speaker_separation` (bool), shows in Settings next to the
  speech engine. Its default comes from S2.
- It applies only to engines on the batch path. Deepgram keeps its own
  numbers.
- Local-only interviews may use it: it is local by construction.
- **Wire format.** Remote lines carry a numbered `speaker` and
  `speaker_key`, exactly as Deepgram lines do. Phase A naming, export,
  notes and MCP read paths need no change.
- **Agents.** `settings.set` from MCP stays limited to `MCP_SETTINGS`;
  `speaker_separation` is not added to it.

## Alternatives rejected

- **Labelling after the line, with a relabel event.** Answers and notes
  would race the label, and it needs a new event carrying line id, number
  and key. Labelling inside the batch path avoids both.
- **Re-segmenting audio at speaker changes (splitting a chunk into several
  lines).** Parakeet returns no word times, so text cannot be split by
  speaker. Mixed chunks stay unlabelled instead, and S1 measures how often
  that happens.
- **Storing voice prints to recognise people across meetings.** That would
  be new stored biometric data (intent question 4).
- **A dedicated diarization model up front.** It costs closure size and RAM
  without evidence that the existing model is not enough. It is measured
  only if CAM++ misses the bar (S1).
- **Per-connection numbering with `speaker_offset`.** It renumbers the same
  people on every pause and resume.

## Risks

- **Compressed call audio.** Codec artefacts may pull voices together. S1's
  Opus condition measures this before anything ships. The gate can say no.
- **Fast turn-taking.** Hand-offs under 0.6 s make mixed chunks, which stay
  "Them" and lower coverage. Real calls may be worse than the synthetic set.
  The owner's live check after deploy is the last word.
- **Embedder contention.** One mutex for two users: a long owner check could
  delay a label by tens of milliseconds. Neither path waits for the other.
- **Drifting centroids.** Capping a centroid at its last 20 assignments
  keeps one long talker from absorbing others. `MAX_SPEAKERS` bounds memory.

## Verification

- **S1/S2:** the harness output for each condition and the threshold sweep,
  attached to #61 and approved by the owner before S3–S7.
- **`cargo test --workspace`:**
  - **`SpeakerLabeller` on fixed vectors:**
    - join;
    - new speaker;
    - unsure gives no label;
    - a mixed chunk gives no label;
    - `MAX_SPEAKERS`;
    - the centroid cap;
    - absolute numbering from `first`.
  - **The batch path with a fake embedder:**
    - system lines carry the label, and mic lines never do;
    - a failed or empty transcription gets no label.
  - **Daemon:**
    - pause and resume keep numbers, with no double offset;
    - restore starts at `MAX + 1`;
    - Deepgram still gets `speaker_offset`;
    - the setting turns labelling off;
    - a missing model leaves lines "Them" with one notice;
    - it works without an enrolled owner voice.
- `cargo clippy … -D warnings`, `cargo fmt --check`,
  `bash shell/tests/ci.sh`, `nix flake check`.
- **After deploy, with the owner:** a real 3-person call on Parakeet. Lines
  are numbered, names stick across a pause, and the owner check behaves as
  before.
