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
- **Step 3 decision: in-process.** The official `sherpa-onnx` crate
  2.0.1 with `features = ["shared"]` and `SHERPA_ONNX_LIB_DIR` pointing at
  a `symlinkJoin` of nixpkgs `sherpa-onnx` 1.13.8 and `onnxruntime` links
  without any build-time download (the default `static` feature downloads
  and fails in the sandbox). On p620 (CPU, 4 threads): load 2.3–3.7 s,
  1.1 GB idle, RTF 0.08 on 30 s chunks, peak 1.7 GB over a 60-minute file;
  one-pass decode of very long audio fails, so 30 s chunks are required;
  token timestamps come back on an 80 ms grid. The fallback service was
  not needed. Step 5 adds a flake check running a tiny decode (crate 2.0.1
  against library 1.13.8 may break on a nixpkgs bump) and updates the
  package's cargo hash. Not measured: cold-disk load, two channels at
  once, real meeting audio (step 5 / evaluation set).
- **Step 5:** sherpa-onnx is a normal dependency (spike feature and
  example removed); the sandboxed `nix build` and a `parakeet-decode`
  flake check (real model, the archive's `test_wavs/en.wav`, 2.9 s incl.
  load) prove the no-download build. `nix/parakeet-model.nix` keeps
  `test_wavs` (~0.7 MB) for that check. The model env var is always set,
  so every HM closure carries the 640 MB model until step 7 makes it
  conditional on the provider. Default provider stays `whisper-local`
  until step 7; chunks are WAV-encoded and decoded again for the
  in-process wire (cheap, left as is). Not measured yet: daemon RSS with
  two channels on the real model.
- **Step 7:** whisper removed (provider, `SttWire::WhisperServer`, the
  loopback retry that only served it and its three mock-server tests, the
  user service, `whisper.*` options as `mkRemovedOptionModule`,
  `nix/whisper-model.nix`, `whisper-model-*` packages, `docs/whisper-bench.md`,
  `tools/whisper-bench.sh`, the SetupCard whisper command). Default
  `speech_provider` is `parakeet-local`; `migrate_whisper` rewrites a saved
  `whisper-local` at daemon start and joins its notice to the startup notice.
  New `parakeet.enable` (default true) gates `NIXARCHY_MEETINGAI_PARAKEET_DIR`;
  without it the daemon's not-ready hint says so. **Pending decision:**
  `echo_gate` stays off by default; the evaluation set is not recorded, so
  the gate is not turned on. **Depends on step 6:** this branch was cut from
  step 5, so `openai-compatible-stt`, `deepgram.keyFile` and `deepgram.region`
  do not exist here; the README speech section and the removed-option message
  name them, and the README options block was generated without them. After
  step 6 merges, regenerate the block (`nix build .#docs`). Also not
  measured here: real-set final numbers and the p620 deploy.
