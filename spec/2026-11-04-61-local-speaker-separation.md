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
real chunker (`BatchChunks`) and the S4 labeller and prints the numbers the
intent asks for. Test audio stays outside the repo; the build scripts and
TSV formats are committed.

**Two test sets:**

1. **Synthetic conversations from LibriSpeech** (`test-clean`, as
   `voice_eval` uses):
   - 2–4 speakers, turns of 1–8 s;
   - gaps of 0.2–1.0 s, so a third are under the 0.6 s chunk close;
   - some 0.3 s overlaps (crosstalk).
2. **Real meeting audio from the AMI corpus** (CC BY 4.0): recorded meetings
   of 3–5 people, with word-level speaker times. It has the room acoustics,
   different microphones and natural turn-taking that read speech lacks.

Both are scored clean and after an Opus round trip at 16 kbps
(`ffmpeg`), which is what call audio on the system channel looks like.
Speakers are split into a **dev half** (used to sweep thresholds) and a
**held-out half** (the numbers the gate sees), so the thresholds are not
tuned on the figures that approve them.

**Truth, per chunk.** It comes from sample times, not "one turn is one chunk"
(a speaker's own 0.6 s pause also splits chunks).
- **Single-speaker:** one speaker has at least 90% of the chunk's voiced
  time.
- **Mixed:** anything else.

**Scored on emitted lines.** The harness feeds each chunk through the same
function `handle_chunk_result` uses to label a line, with a fake
transcriber returning fixed text. Failed and empty chunks are dropped
exactly as the batch path drops them, so the denominator is the lines a user
would see.

**Output, per set and condition (clean or call, by number of speakers):**
- **Errors.** Labelled single-speaker lines whose number maps to the wrong
  speaker, after the best one-to-one mapping. **Every labelled mixed line
  counts as an error.**
- **Coverage.** Single-speaker lines with at least 1.5 s voiced that got a
  number.
- **Mixed abstention.** Mixed lines left unlabelled. Mixed lines are also
  reported by voiced length (under 3 s, 3 s or more).
- **Cost, measured on p620 and on razer:**
  - embedding time per line;
  - CPU seconds per minute of audio;
  - added process RSS;
  - closure growth (zero if CAM++ suffices).
- **Latency, measured on the real batch path with Parakeet, the owner check
  and separation all active (a `--live` mode that replays the audio in real
  time):**
  - chunk end to line, for system and for mic lines;
  - owner-check time per line;

  all against the same run with separation off.

A threshold sweep on the dev half prints the error/coverage curve for
`JOIN`, `NEW` and `MIXED`.

**A dedicated pipeline is compared only if CAM++ misses the bar.** That is
sherpa-onnx offline diarization (a segmentation model plus embedding),
scored in the same harness. It adds a model to the closure (about 6 MB),
which the reuse-first constraint allows only on evidence.

### S2. The gate

The owner approves the measured numbers before S3–S7 are built. The
proposed bar, on the held-out half of AMI after the Opus round trip:

- errors: at most 1 labelled line in 20 (labelled mixed lines included);
- coverage: at least 4 in 5 single-speaker lines with 1.5 s or more
  voiced;
- mixed abstention: at least 3 in 4 mixed lines with 3 s or more voiced
  left unlabelled;
- latency: chunk-end-to-line, for system and mic lines, rises by under
  100 ms at the median; owner-check results are unchanged, and its time
  rises by under 100 ms;
- cost: under 100 MB added RSS on razer, and under 5 CPU-seconds per
  minute of audio.

LibriSpeech results are reported alongside as a cleaner reference, but do
not pass the gate on their own.

**If it passes:** the measured thresholds and the default setting go into
the plan.

**If it fails:** the plan stops, and the numbers are written into #61. A
dedicated-pipeline result from S1, if run, goes to the owner as the next
option.

### S3. Where labelling happens: inside the batch path, before the line leaves

`batch_task` (`src/speech/stt/transcription.rs` ~1279) holds each system
chunk's samples (`BatchChunk` ~387) while it is transcribed (up to
`MAX_IN_FLIGHT` 4 at once, in a `FuturesOrdered`, ~1147).

- **Embed once, alongside transcription.** For a channel-1 chunk, the
  embedding runs on a blocking worker joined with the transcription
  (`join`), inside the same `pending` future. The future therefore
  finishes after `max(transcription, embedding)`, not their sum.
  - The embedding is computed once per chunk; a transcription retry
    (~1208) reuses it.
  - Its result is dropped when the text is empty or the call fails.
- **Label before the line leaves.** `handle_chunk_result` (~1236) labels
  through the S4 labeller and sets `TranscriptLine.speaker` before
  `emit_lines`. So:
  - no relabel event is needed;
  - the stored row, live view, answers, `cite_window`, rolling notes and
    recap all see the final speaker;
  - nothing waits on `settle_checks`.
- **Ordered results are the risk.** A slower system future holds back the
  mic results queued behind it, and the echo hold
  (`open_only_system_end`, `earliest_system`, ~1163–1188). S1's `--live`
  latency measures it, S2 gates it, and a test pins the echo hold's
  behaviour (Verification).
- Embedding a chunk that turns out empty wastes its CPU. S1's CPU figure
  includes it.

### S4. The labeller: online clustering of CAM++ embeddings

A pure, synchronous `SpeakerLabeller` in `src/speech/` decides; the
`Embedder` trait supplies vectors. It can be unit-tested on fixed vectors.

**Windows.** The chunk's voiced samples (30 ms frames with `rms ≥ 0.005`,
as `voice_check::speech` uses) are embedded in 1.5 s windows:

| Voiced length | Embedding | Mixed check |
|---|---|---|
| Under 1.5 s | none | no label |
| 1.5 s to under 3 s | one window over all of it, embedded whole (as `voice_check::score` does) | none possible: two windows would overlap too much to differ. The line is labelled if it passes `JOIN`/`NEW`; S1 reports this band separately, and the gate's mixed-abstention bar applies only from 3 s |
| 3 s or more | 2–4 non-overlapping 1.5 s windows, spread evenly | if any two windows' cosine is below `MIXED`, no label. Otherwise the vector is the normalised mean of the windows |

**Assign.**
- With no known speaker yet, a usable vector creates speaker `first`.
- Otherwise the best cosine against each known speaker's centroid decides.
  The centroid is the normalised mean of a **ring buffer of that
  speaker's last 20 assigned vectors**.
  - At least `JOIN`: that speaker is assigned, and the vector is pushed to
    their ring.
  - Below `NEW`, with fewer than `MAX_SPEAKERS` (8) known: a new speaker
    is created.
  - Otherwise: no label, and the line stays "Them".

**Numbers.** Speaker numbers are absolute within the meeting, starting at
`first` (S5).

`JOIN`, `NEW` and `MIXED` are constants set from S1's dev-half sweep.

### S5. State across pause, resume, provider switches and restore

**Plumbing.** The daemon starts speech in two places:
- `ListenSession::start(&config)` in `record_start` (~2403);
- `LiveTranscriber::connect_config_with(config, hints)` in
  `recording_task` (~2543), which reaches `connect_batch` (~843) for
  non-streaming engines.

The labeller and the embedder travel in `SttHints`, as two new optional
fields: `labeller: Option<Arc<Mutex<SpeakerLabeller>>>` and the shared
embedder.

**Lifetime.**
- One labeller per meeting, in `Inner`.
- For `record.start` with `new: true`, it is created **after**
  `reset_session` (~2437), so a new meeting never inherits the old one's
  voices.
- A pause and resume (`new: false`) hands over the same labeller, so the
  same people keep their numbers.

**Offsets: batch versus streaming.**
- `handle_live_event` adds `speaker_offset` to final **and** partial
  speaker numbers today (~2070, ~2097).
- From now on it adds it only when the current recording's engine is
  streaming, which `record_start` records as `g.speech_batch` from
  `config.speech_streaming()`. That covers Deepgram and any custom
  streaming provider; batch lines already carry absolute numbers.

**One numbering per meeting.**
- At every `record.start`, `speaker_offset` is computed as
  `MAX(speaker) + 1` from **`meeting_lines`**, not from `g.transcript`.
  The live buffer drops its oldest 100 lines at 500 (~2187), so it can
  miss a speaker.
- The labeller's next new number is raised to at least that value.
- So a switch from Deepgram to Parakeet in mid-meeting continues after
  Deepgram's numbers, and a switch back continues after the labeller's.

**Restore.** On `session_restore`, and when resuming after a daemon
restart, a fresh labeller starts at `first = MAX(speaker) + 1`. Earlier
voices get new numbers, and no voice prints are stored (intent question 4).
Centroids live in memory only and are dropped with the meeting session.

### S6. Sharing the model, and independence from the owner check

**One model.** The labeller uses the daemon's one `VoiceCheck.embedder`
(`Arc<dyn Embedder + Send + Sync>`, `session.rs` ~248), loaded lazily on a
blocking worker. The model is in RAM once.

**A lock per call.**
- Today the owner check already calls `embed()` from concurrent
  `spawn_blocking` tasks with no lock (~3029). Only loading is locked.
- sherpa-onnx's thread safety for concurrent `compute` calls is not
  established.
- So `SherpaEmbedder::embed` takes an internal mutex **per call**, not
  across a chunk's 2–4 windows, so neither user waits for the other's
  whole chunk.
- That also closes the owner check's existing concurrent use.
- S1's `--live` run measures the added owner-check time against S2's bar.

**Loading.**
- `Daemon::embedder()` no longer requires an enrolled owner voice or an
  unpaused owner check. Those conditions stay on the owner check (~3010,
  ~3015).
- A load failure sets the shared `failed` flag, as today. The one notice
  names both features: "The speaker model could not be loaded: the owner
  voice check and speaker separation are off." This replaces today's
  owner-only message (~2979), so there is never a second, conflicting
  notice.
- Lines then stay "Them", and nothing else changes.

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
  `speaker_separation` is not added to it, and a test pins the refusal.

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
- **Embedding only after transcription succeeds.** It saves CPU on empty
  chunks, but adds the embedding time to every system line and to the mic
  lines ordered behind it. Joining both keeps the delay at their maximum.
- **A second embedder instance for the labeller.** No contention, but the
  model would be in RAM twice, against the intent's constraint.

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

- **S1/S2:** the harness output for each set and condition, the dev-half
  sweep and the held-out numbers, on p620 and razer. It is attached to #61
  and approved by the owner before S3–S7.
- **`cargo test --workspace`:**
  - **`SpeakerLabeller` on fixed vectors:**
    - the first speaker;
    - join;
    - a new speaker;
    - unsure gives no label;
    - each voiced-length band (under 1.5 s, 1.5–3 s, 3 s or more);
    - a mixed chunk of 3 s or more gives no label;
    - `MAX_SPEAKERS`;
    - the 20-vector ring;
    - absolute numbering from `first`.
  - **The batch path with a fake embedder and transcriber:**
    - system lines carry the label, and mic lines never do;
    - empty or failed transcription gets no label;
    - a retry reuses the embedding;
    - a slow embedding delays a following mic line only up to
      `max(transcription, embedding)`;
    - the echo hold releases mic lines as before.
  - **Daemon:**
    - pause and resume keep numbers, with no double offset on final and
      partial lines;
    - a switch from Deepgram to batch and back mid-meeting continues the
      numbering;
    - `speaker_offset` comes from `meeting_lines` after more than 500
      lines;
    - restore starts at `MAX + 1`;
    - a `new: true` recording gets a fresh labeller;
    - the setting turns labelling off;
    - a missing model gives the one combined notice, and lines stay "Them";
    - it works with no enrolled owner voice and with the owner check
      paused;
    - a local-only interview on Parakeet is labelled;
    - MCP `settings.set speaker_separation` is refused.
- `cargo clippy … -D warnings`, `cargo fmt --check`,
  `bash shell/tests/ci.sh`, `nix flake check`.
- **After deploy, with the owner:** a real 3-person call on Parakeet. Lines
  are numbered, names stick across a pause, and the owner check behaves as
  before.

## Review record (draft)

| Finding | From | Verdict |
|---|---|---|
| The gate lacks a real-call condition and p620/razer cost, CPU and latency figures | Codex, agy | confirmed → AMI real meetings, a held-out half, `--live` latency, costs on both hosts |
| Majority-speaker truth rewards labelling mixed chunks; coverage unattainable | Codex, agy | confirmed → 90% single-speaker truth; labelled mixed lines count as errors; coverage over single-speaker lines |
| Thresholds tuned on the gate's own figures | agy | confirmed → dev/held-out speaker split |
| `FuturesOrdered` and the echo hold can delay mic lines | Codex, agy | confirmed → embedding joined with transcription; latency gate; echo-hold test |
| Score lines, not chunks; retries, empties, `LineFilter` | Codex | confirmed → scored on emitted lines through the same function; embed once per chunk |
| The offset rule must be batch versus streaming, including partials | Codex, agy | confirmed → `g.speech_batch`; final and partial |
| A provider switch mid-meeting; the 500-line buffer understates `MAX` | agy | confirmed → `MAX` from `meeting_lines`; the labeller's next number raised |
| The labeller is not passed via `connect_batch`; `reset_session` order | agy | confirmed → via `SttHints` on both paths; created after `reset_session` |
| Embedder concurrency: today's calls are unlocked; a lock across windows would block | agy, Codex | confirmed → a per-call mutex inside `SherpaEmbedder` |
| A hardcoded owner-only load-failure notice and a shared `failed` flag | agy | confirmed → one combined notice |
| 1.5–3 s chunks cannot be checked for mixing; the first speaker; the centroid cap | agy | confirmed → length bands; the first vector creates `first`; a 20-vector ring |
| Missing tests: local-only, provider switch, owner check paused or unenrolled, MCP refusal | agy | confirmed → Verification |
| `LiveTurns` splits on speaker change | agy | refuted: batch lines are all `speech_final`, so behaviour is unchanged (agy's own conclusion) |

