---
status: approved
issue: 32
intent: intent/2026-10-29-32-sherpa2-voxlock.md
---

# Spec: a matching sherpa-onnx library, and a measured choice of voice filter

Inputs: the approved intent and its decisions (override in this flake now,
second voice from a phone, filter only in a follow-up); facts checked on
2026-10-29: nixpkgs ships sherpa-onnx 1.13.8 and onnxruntime 1.27.1; the
crate is 2.0.1; upstream v2.0.1 (2026-10-26) pins onnxruntime 1.28.2; the
2.0.1 crate exposes VoxLock as `OfflineSpeechDenoiserVoxLockModelConfig`
for both `OfflineSpeechDenoiser` and the streaming `OnlineSpeechDenoiser`.

## Design

### 1. The library at the crate's version

- New `nix/sherpa-onnx.nix`: nixpkgs' `sherpa-onnx` with
  `overrideAttrs` to version 2.0.1 (new `src` hash; the pre-fetched cmake
  dependencies updated if 2.0.1 changed them). `nix/sherpa-lib.nix` links
  it instead of `pkgs.sherpa-onnx`.
- onnxruntime stays nixpkgs' 1.27.1 if the 2.0.1 build and tests pass with
  it; otherwise the same file overrides onnxruntime to 1.28.2. Recorded in
  the plan as a deviation either way.
- A flake check `sherpa-version-match` reads the `sherpa-onnx` version from
  `Cargo.lock` (`builtins.fromTOML`) and fails if it differs from the
  library's version. Pure evaluation, no build.
- The existing `parakeet-decode` check runs against the new library.
- After it merges, the same version bump is offered to nixpkgs as a PR from
  the owner's fork (not blocking).

### 2. Parakeet unchanged

- `examples/accuracy_replay.rs` and `tools/accuracy/score.py` gain
  `--parakeet` (in-process Parakeet through `speech::stt::parakeet`, model
  path from the module's environment) next to `--url` and `--fake`.
- The 29 October tap (`~/meetingai-eval/aec-parakeet-tap.wav`) and the
  `speech.wav` reference are scored on the old and the new library; lines
  and texts must be identical.

### 3. The comparison

- `examples/voice_filter.rs`: reads a stereo 16 kHz tap WAV, runs VoxLock
  over the left (mic) channel with sherpa-onnx's streaming denoiser in
  20 ms chunks, given an explicit owner voice sample WAV (never learned
  from the input), and writes a stereo WAV with the filtered left channel
  and the right unchanged. Prints its time per second of audio.
- `tools/accuracy/alkove_filter.py`: the same job with Alkove Focus and a
  print learned from the same owner sample, run in a throwaway virtual
  environment in the session scratchpad with the `alkove` PyPI package
  pinned by version and hash. It is a measurement script only: nothing in
  the app or its build imports Alkove.
- Models: VoxLock `enroll.onnx` and `chunk_int8.onnx` from
  `arbazsiddiqui/voxlock` on Hugging Face, pinned by hash in
  `nix/voxlock-model.nix` (so the follow-up can reuse it); Alkove's models
  through its own sha256-checked download, into the scratchpad.
- Recordings (owner, about 5 minutes, at the desk, mic through the echo
  canceller, kept in `~/meetingai-eval`, 0600, out of git):
  1. owner sample: 15 s reading a sentence, nothing else playing;
  2. owner reads a 6-line script, speakers silent;
  3. the phone plays test speech (Piper voice, different lines) while the
     owner is silent, then while the owner reads;
  4. the speakers play `speech.wav` while the owner reads (double talk).
  References written from the scripts, in the existing `ref` format.
- Each recording is scored three times (no filter, VoxLock, Alkove) with
  `score.py --parakeet`: foreign words on the mic channel, owner words
  lost, invented lines; plus delay and CPU time per second of audio on
  p620.
- Report `tools/accuracy/reports/2026-10-voice-filters.md`: the numbers,
  the versions and hashes, what each filter did wrong, and the decision.

### 4. The decision rule

A filter is proposed in a follow-up issue only if, on these recordings, it
removes more foreign words than #31's voice check would label `room`, AND
loses no more than 1 in 50 of the owner's words, AND uses under half a core
on p620. Otherwise the report says no filter, and why.

## Alternatives rejected

- **Prebuilt upstream binaries (`sherpa-onnx-v2.0.1-linux-x64-shared`)**:
  quickest, but a binary blob patched with autoPatchelf instead of a source
  build; kept only as the fallback if the source override fails, recorded
  in the plan.
- **Waiting for nixpkgs:** decided against in the intent.
- **The `ort` crate for VoxLock:** a second copy of onnxruntime in the
  process.
- **Alkove's Rust crates as a git dependency:** unpublished (`publish =
  false`), preview API; for a measurement the Python package is enough and
  stays out of the app.
- **Synthetic mixes only:** they say nothing about the owner's desk mic and
  room, which is the open question.

## Risks

- **2.0 build changes:** new cmake dependencies or flags in 2.0.x break the
  override; the fallback above, and the build runs in CI before p620.
- **onnxruntime 1.27 vs 1.28:** possible API or model-opset mismatch; the
  `parakeet-decode` check catches the Parakeet side, the comparison the
  VoxLock side.
- **Behaviour change in Parakeet decoding between 1.13.8 and 2.0.1:**
  caught by the identical-lines check on the 29 October tap.
- **Recording burden:** about 5 minutes of the owner's time; the phone's
  speaker is a weaker room voice than a person, noted in the report.
- **Deploy:** the library change reaches every host; deployed only from
  committed main, announced on the agent bus, not while recording.

## Verification

- `nix flake check` (including `sherpa-version-match` and
  `parakeet-decode`), `cargo fmt`, `clippy -D warnings`, `cargo test
  --workspace`.
- A negative test of the version check: a deliberately wrong version in a
  test fixture fails it.
- Identical Parakeet lines on the 29 October tap, old vs new library.
- The report committed with all three rows for every recording, and the
  decision stated.
- After deploy on p620: the daemon reports speech ready on Parakeet and a
  short recording transcribes.
