---
status: approved
issue: 14
spec: spec/2026-10-28-14-transcript-accuracy.md
coders:
  1: codex     # signal gate: DSP maths with synthetic tests, Codex did the db layer well
  4: agy       # text filter: self-contained, small interface (agy did audio/dsp)
  6: codex     # Deepgram key file, EU region, OpenAI multipart wire: provider plumbing
  # 0, 2, 3, 5, 7 stay with the Claude coder (Sonnet): tooling, Nix/PipeWire,
  # the Parakeet spike and integration need p620 runs from this session
reviewers:     # every step, read-only, on top of the Opus review
  - codex      # codex review --base main, sandbox read-only
  - agy        # agy --mode plan, diff in the prompt
---

# Plan: transcripts that show only what was said, by whom

## Approved decisions (copied from the spec)

- **Audio path first, engine second.** Measure, then AEC, then a signal
  gate, then the text filter, then Parakeet.
- **Evaluation set:** `tools/accuracy/` (scripts in git, audio outside git,
  passed by path). Counts per run: bleed lines (remote words on the mic
  channel), lost owner words, invented lines on silence, latency. A
  synthetic signal-only version runs in CI.
- **AEC:** `nix/hm-module.nix` option `echoCancel.enable` (default true)
  writes `~/.config/pipewire/pipewire.conf.d/60-meetingai-echo-cancel.conf`
  loading `libpipewire-module-echo-cancel` with `monitor.mode = true` and a
  source named `meetingai-echo-cancel-source`. Mic capture moves to `parec`
  by source name (the system-audio path), echo-cancel source when present,
  else the default source; the device picker lists sources by name.
- **Signal gate** at the interleaved-frame boundary in
  `src/speech/session/listen.rs`: per 20 ms frame, lag 0–250 ms between
  system and mic envelopes; zero a mic frame only when strongly explained
  by louder system audio and no independent mic energy (double talk kept).
  Thresholds from the evaluation set; ships off until it loses no owner
  words, then on.
- **Text filter** (`src/speech/stt/filter.rs`): containment of a mic line's
  words in the combined system text within ±8 s; a mic line waits (≤ 3 s)
  for overlapping system chunks; the mic channel's own text is never the
  engine prompt. #12's 0.2 s gate and 60 s repeat filter stay.
- **Parakeet in-process:** sherpa-onnx's official Rust API over nixpkgs
  `sherpa-onnx` 1.13.8, Parakeet TDT 0.6B v3 INT8, one model per daemon,
  inference on a blocking worker, per-channel chunks from `BatchChunks`,
  channel and chunk times kept. Model pinned in `nix/parakeet-model.nix`
  (hash, CC-BY-4.0 attribution). **Spike first**; fallback: a local
  OpenAI-compatible service on `python3Packages.sherpa-onnx` reached
  through step 6's wire. Timestamps only if reliable.
- **Whisper removed:** provider `whisper-local`, the whisper user service,
  `nix/whisper-model.nix`, `whisper.*` options (→ `mkRemovedOptionModule`
  with a message); a saved `whisper-local` migrates once to
  `parakeet-local` with a notice.
- **Deepgram:** `deepgram.keyFile` → `NIXARCHY_MEETINGAI_DEEPGRAM_KEY_FILE`
  read at runtime (the `external_ollama_key` pattern), vault key wins;
  `deepgram.region = "eu"` → `api.eu.deepgram.com` and `mip_opt_out=true`.
- **OpenAI-compatible speech:** `SttWire::OpenAiMultipart`
  (`POST {base}/v1/audio/transcriptions`, model, optional key).
- Default engine `parakeet-local`; local-only interviews never go online.

## Repo traps

- **Rewrite #3 overlap:** `transcription.rs` and `listen.rs` (rewrite step
  10), `microphone.rs` (step 3) and `settings.rs` (step 7) are original
  code still to be rewritten. This plan lands first; edits there are ours
  and the later briefs describe the new behaviour. Step 4 of the rewrite
  (db) is not merged yet: do not touch `src/db/`.
- Shell tests run headless only; nothing here may open the owner's real
  microphone in a test. Real-audio checks are owner-run on p620.
- The evaluation audio is personal: never commit it, never send it to a
  cloud engine without the owner choosing that engine for the run.
- No secret in the Nix store: key files by path at runtime only.
- `git add -N` new files before `nix flake check` (flakes see tracked files).
- Coders do not commit; the lead commits each step with `SKIP_BUMP=1`.
- Merge PRs with merge commits (rewrite #3 counting rule).

## Steps

0. **Evaluation tooling** (`tools/accuracy/record.sh`, `score.py`,
   `synthetic.py`; `src/daemon/session.rs` only if a test input hook is
   needed). `record.sh` records a stereo WAV with the owner's consent
   (mic + monitor, `parec`); `score.py` replays a WAV through the batch path
   and prints the four numbers against a reference transcript file;
   `synthetic.py` makes signal-only fixtures for CI.
   → verify: `score.py` runs on the synthetic fixture; owner records the
   real set on p620 (owner action), baseline numbers in the PR.
1. **Signal gate** (`src/speech/session/listen.rs`, new
   `src/audio/echo_gate.rs`), setting `echo_gate` default off.
   → verify: unit tests on synthetic signals (pure echo at 0/120/250 ms
   lag zeroed, double talk kept, silence untouched); numbers on the real set.
2. **AEC source** (`nix/hm-module.nix`, `src/audio/capture/system_audio.rs`
   reused for the mic, `src/audio/capture/microphone.rs`,
   `src/audio/capture/devices.rs`).
   → verify: HM eval check file present/absent; on p620
   `pactl list sources short | grep meetingai-echo-cancel-source`; numbers
   on the real set; device change test with headphones.
3. **Parakeet spike** (`Cargo.toml`, a throwaway `examples/parakeet.rs`,
   `nix/parakeet-model.nix`). Build in the Nix sandbox and on p620; measure
   cold load, RTF, RAM, a 60-minute file.
   → verify: numbers in the PR; decision recorded in this plan's
   deviations log (in-process or fallback service).
4. **Text filter** (`src/speech/stt/filter.rs`, the prompt call in
   `src/speech/stt/transcription.rs`).
   → verify: tests for containment over combined system text, out-of-order
   arrival, no mic self-prompt; existing filter tests still pass.
5. **Parakeet provider** (`src/ai/catalog/providers.rs`,
   `src/speech/stt/transcription.rs`, `src/speech/stt/parakeet.rs`,
   `src/core/models/settings.rs`, `src/daemon/session.rs` readiness,
   `nix/hm-module.nix` model env, `flake.nix` checks/fixtures).
   → verify: two-channel and stop/flush tests with a tiny test model or a
   mocked recogniser; `nix flake check`; real set numbers vs whisper.
6. **Deepgram key file and region, OpenAI multipart wire**
   (`src/daemon/session.rs`, `src/ai/catalog/providers.rs`,
   `src/speech/stt/transcription.rs`, `nix/hm-module.nix`).
   → verify: key precedence test (vault > file), EU URL and `mip_opt_out`
   test, multipart request shape test against a local stub server;
   `nixos_config`: `deepgram.keyFile = config.age.secrets.api-deepgram.path`
   (separate nixos PR).
7. **Remove whisper; switch defaults; turn the gate on if earned**
   (`nix/hm-module.nix`, `nix/whisper-model.nix` deleted, `flake.nix`,
   `src/core/models/settings.rs`, `src/ai/catalog/presets.rs`, README,
   docs). Migration with notice; `whisper.*` → removed-option messages.
   → verify: migration test; `nix flake check`; README options regenerated;
   real set final numbers; deploy to p620 after asking on the bus and when
   no meeting is recording.

## Tests

`cargo test --workspace`, clippy `-D warnings`, `cargo fmt --check`,
`nix flake check`, `checks.shell` (headless), the synthetic fixture in CI,
and the owner-recorded evaluation set (before/after numbers per step in each
PR). One PR per step, merge commits.

## Rollback

Each step is its own PR and reverts alone. The gate and AEC have off
switches (`echo_gate`, `echoCancel.enable`). Until step 7 merges, whisper
stays available, so steps 3–6 never leave a host without a working local
engine.

## Implementation split

Claude coder (Sonnet): steps 0, 2, 3, 5, 7. Codex: steps 1 and 6. agy:
step 4. Each step reviewed read-only by Codex and agy (one at a time) and
by a fresh Opus agent given this plan and the diff; a review not obtained
is reported, not replaced.

## Deviations log

- Step 6: local-only interviews may use `openai-compatible-stt` only with a loopback base URL. `deepgram.region` is nullable in Home Manager; `null` leaves settings.json absent in a minimal configuration, while the daemon defaults to `us`.
- Step 2: a saved `cpal:` input device keeps CPAL and so runs without echo
  cancelling (the user's explicit choice is kept). The picker still lists and
  selects it, labelled "(no echo cancelling)", and Listen start emits one
  notice suggesting "Meeting AI echo-cancelled mic".
- Step 2: the parec mic falls back to `@DEFAULT_SOURCE@` when the chosen
  source delivers no samples within 500 ms. The drop-in module entry carries
  `flags = [ ifexists nofail ]`. `pactl list sources` has a 2 s timeout and
  dictation start opens the mic outside the daemon lock.
- **Step 2:** the module arguments were checked by loading the drop-in
  into an isolated PipeWire 1.6.9 with the stock config: it creates
  `meetingai-echo-cancel-source` and a sink it always makes, named
  `meetingai-echo-cancel-sink` so it is recognisable. Mic capture goes
  through `parec` for Pulse source names (echo-cancel source by default,
  else `@DEFAULT_SOURCE@`); explicit `cpal:` devices and legacy names keep
  CPAL; no Pulse server → CPAL. `Microphone::level()` is 0 on the parec
  path (no callers). A parec failure on the default path is a start error,
  not a CPAL fallback.
- **Step 0:** the replay is `examples/accuracy_replay.rs`, which runs
  `BatchChunks` → provider → `LineFilter` in sequence rather than the
  spawned live task (same components, latency measured per chunk); this
  made `BatchChunk`/`BatchChunks` `pub`. JSON lines also carry `latency`.
  `score.py` scores by words (bleed: > ½ of a mic line's words in `them:`
  text and < ½ in `me:`; invented: < ½ of its words anywhere in the
  reference); `[silence]` markers are parsed but unused. `score.py` calls
  `cargo run --example`, so it runs inside `devenv shell`.
- **Step 0 review fixes** (Codex, Opus): recording no longer uses two
  `parec` processes (channels were unaligned and the live mic does not go
  through parec): a socket-only command `record.tap {path, seconds}` /
  `{stop}` writes the exact interleaved frames the transcriber receives
  (after `route_audio`) to a 0600 WAV, only during a running recording and
  never for a local-only interview; not exposed over MCP. `record.sh`
  wraps it. `score.py`: lost owner words by counts (not set membership),
  bleed decided on words unique to one side, `[silence mm:ss-mm:ss]`
  spans for invented-on-silence, latency over every request, release
  build. The replay runs 4 requests in flight with prompts built ahead,
  like live, and uses the live retry helper. The synthetic fixtures feed
  step 1's Rust tests; no separate CI job.
- Step 1: the gate analyzes 1 ms envelopes in the existing 50 ms capture blocks rather than emitting 20 ms blocks. It holds one block for lookahead, adding 50 ms of gate-only output latency so a quiet owner onset can restore the preceding block. The gate remains off by default.
- **Step 1:** the gate analyses 1 ms envelopes inside the existing 50 ms
  capture blocks instead of emitting 20 ms blocks; it holds one block for
  lookahead (50 ms extra latency, gate on only) so a quiet owner onset can
  restore the block before it. Off by default.
- **Step 4:** echo needs a run of 3 consecutive words plus content-word
  containment ≥ 0.6 (stop words ignored), so short owner replies survive;
  mic lines are held until 3 s after they arrive (audio clock) and while
  an overlapping system chunk is open or in flight (15 s cap); `accept`
  returns released lines, `tick`/`flush` release the rest. The step 0
  replay now prints request and released-line records separately and
  flushes at the end; `score.py` reads them.
