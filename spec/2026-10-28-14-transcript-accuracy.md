---
status: approved
issue: 14
intent: intent/2026-10-28-14-transcript-accuracy.md
---

# Spec: transcripts that show only what was said, by whom

Inputs: the approved intent; read-only design reviews by agy (plan mode,
model not reported) and Codex (gpt-6-sol), both checked against the code by
the lead. Where they disagreed (how to run Parakeet) the choice is below.

## Design

Order matters: **fix the audio path first, then the engine.** A better
recogniser still transcribes speaker bleed faithfully as "you" if the mic
hears the speakers (Codex).

### 1. Measure first: an evaluation recording

- `tools/accuracy/` holds a short stereo evaluation set made on p620 with
  the owner's consent: remote speech through the speakers only, owner
  speech, double talk, silence, a device change. Kept out of git (it is
  personal audio); its location is passed by path.
- `tools/accuracy/score.py` replays it through the daemon's batch path
  (`ctl` + a fake capture input) and counts: remote words on the mic channel
  ("bleed lines"), owner words lost, invented lines on silence, latency.
- Every later step reports these numbers before and after. A synthetic
  version (generated tones/noise, no speech) runs in CI for the signal
  logic only.

### 2. Echo-cancelled mic source (PipeWire WebRTC AEC)

- `nix/hm-module.nix`: `echoCancel.enable` (default `true`) writes
  `~/.config/pipewire/pipewire.conf.d/60-meetingai-echo-cancel.conf` loading
  `libpipewire-module-echo-cancel` with `monitor.mode = true` (the default
  sink's monitor is the reference, so call apps need no routing) and a
  virtual source named `meetingai-echo-cancel-source`.
- Mic capture moves to the same `parec` path system audio already uses
  (`src/audio/capture/system_audio.rs`), recording a source **by name**:
  the echo-cancel source when present, else the default source. cpal/ALSA
  cannot choose a PipeWire source by name (agy). The device picker keeps
  working (it lists sources by name).
- Verified on p620: remote-only speech disappears from the mic channel,
  owner speech during playback survives, and a sink change (headphones in)
  keeps working.

### 3. Signal-level bleed suppression (fallback when AEC is off or partial)

- At the interleaved-frame boundary (`src/speech/session/listen.rs`, where
  mic and system are paired), per 20 ms frame: estimate the lag (0–250 ms)
  between the system envelope and the mic envelope; zero a mic frame only
  when it is strongly explained by louder system audio (normalised
  correlation and level ratio above thresholds) **and** there is no
  independent mic energy (double talk is kept).
- Thresholds are set from the evaluation set, off by default until it
  improves the numbers without losing owner words; then on.

### 4. Text filter as last guard (`src/speech/stt/filter.rs`)

- Compare a mic line with the **combined** system text overlapping it
  within ±8 s, using containment (share of the mic line's words found in
  the system text), not the current symmetric 80 % within 2 s.
- Hold a mic line until system chunks overlapping it have been transcribed
  (bounded wait, 3 s), so arrival order no longer decides.
- The mic channel's own text is no longer used as the engine's prompt
  (the "Amen" loop). Today's 0.2 s minimum-sound gate and 60 s repeat
  filter (#12) stay.

### 5. Parakeet replaces whisper (in-process)

- **Runtime:** sherpa-onnx's official Rust API over the nixpkgs
  `sherpa-onnx` (1.13.8) library, offline transducer, Parakeet TDT 0.6B v3
  INT8 bundle. One model loaded once in the daemon, inference on a blocking
  worker thread (never the Tokio capture thread), per-channel chunks from
  the existing batch path (`BatchChunks`), channel identity and chunk
  times preserved.
- **Model:** fetched by Nix (`nix/parakeet-model.nix`, pinned archive and
  hash, CC-BY-4.0 attribution in README); path passed by environment
  variable, like the whisper model today.
- **Spike first** (plan step): build and load in the Nix sandbox and on
  p620; measure cold load, real-time factor, RAM, a 60-minute run. If the
  in-process build fails or crashes, fall back to agy's design: a local
  OpenAI-compatible HTTP service (`python3Packages.sherpa-onnx`) reached
  through the connection in 6. The plan records which one shipped.
- **Removal:** `whisper-local`, the whisper user service, its model
  derivation and `whisper.*` options are removed. `whisper.*` options turn
  into `mkRemovedOptionModule` with a message; a saved `whisper-local`
  setting migrates to `parakeet-local` once, with a notice.
- Word timestamps only if the export returns them reliably; otherwise
  chunk start/end as today.

### 6. Online and other engines

- **Deepgram key file:** `programs.nixarchy-meetingai.deepgram.keyFile`
  sets `NIXARCHY_MEETINGAI_DEEPGRAM_KEY_FILE`, read at runtime like the
  Ollama key (`external_ollama_key` pattern in `session.rs`); the vault
  key still wins. p620 uses `config.age.secrets.api-deepgram.path`.
- **Deepgram region:** an `eu` option uses `api.eu.deepgram.com` and
  `mip_opt_out=true` (Codex: both are needed for in-region handling).
- **OpenAI-compatible speech connection:** `SttWire::OpenAiMultipart`
  (`POST /v1/audio/transcriptions`, model, optional key) with base URL and
  model in settings, so OpenAI `gpt-4o-transcribe`, Groq, or any local
  server (whisper or Parakeet) work without per-engine code.
- Default engine: `parakeet-local`. Local-only interviews never use an
  online engine (unchanged rule).

## Alternatives rejected

- **Text matching alone:** cannot recover speech the two channels segment
  or recognise differently (both reviewers).
- **Muting the mic whenever system audio plays:** destroys double talk.
- **Inferring the call app's mute:** apps mute their own outgoing stream,
  not the OS source; a guessed signal gives false confidence (Codex).
- **Parakeet as a separate HTTP service first:** another service and
  response contract to maintain; kept as the fallback (Codex over agy).
- **`sherpa-rs`:** deprecated by its author; `parakeet-rs` uses a
  different model layout; one known-compatible export only.
- **Keeping whisper:** owner decision (intent, answer 1).
- **Cloud engine as default:** privacy and cost (intent, answer 3).

## Risks

- **AEC routing:** a call playing to a sink other than the default may not
  reach the reference; mitigated by the signal fallback (3) and the device
  change test.
- **Erasing real owner speech:** the signal gate's main risk; it ships off
  until the evaluation set shows no lost owner words.
- **CPU load on p620 and CPU-only hosts:** two channels of Parakeet on CPU;
  the spike measures it; four-request back-pressure stays.
- **Native packaging:** sherpa-onnx linking in the Nix build; fallback in 5.
- **Rewrite #3:** `transcription.rs`, `listen.rs`, `microphone.rs` are
  rewrite steps 3 and 10; this change lands first and the briefs describe
  the new behaviour.
- **Personal audio:** the evaluation set stays outside git.

## Verification

- Evaluation set numbers before/after each step (bleed lines, lost owner
  words, invented lines, latency), recorded in the PR.
- Unit tests: echo gate on synthetic signals (pure echo zeroed, double talk
  kept, lag 0–250 ms); filter containment and ordering; no self-prompt on
  the mic channel; Deepgram key-file precedence; OpenAI multipart request
  shape; migration `whisper-local` → `parakeet-local`; mute zeroes the mic
  channel end to end.
- `nix flake check` (HM eval: echo-cancel file present/absent, removed
  whisper options give the message, deepgram keyFile env).
- p620: the AEC source exists (`pactl list sources short`), a real call
  with speakers shows no bleed lines, CPU and RAM within the spike's
  measured budget.
