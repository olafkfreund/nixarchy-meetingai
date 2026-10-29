---
status: approved
issue: 32
spec: spec/2026-10-29-32-sherpa2-voxlock.md
coders:
  1: coder     # Nix source override and a flake check: needs nix builds, which the agy and Codex sandboxes cannot run
  2: codex     # replay tool flag and score.py: small Rust + Python plumbing
  3: agy       # VoxLock example: self-contained Rust against one crate API
  4: codex     # Alkove measurement script: Python, isolated
  # 5 stays with the lead: owner recordings, runs and the report need p620 and the owner
reviewers:     # every code step, read-only, on top of the Opus review
  - codex      # codex review --base origin/main, sandbox read-only
  - agy        # agy --mode plan, files named in the prompt
---

# Plan: a matching sherpa-onnx library, and a measured choice of voice filter

## Approved decisions (copied from the spec)

- **Library:** `nix/sherpa-onnx.nix` overrides nixpkgs' `sherpa-onnx`
  (1.13.8) to 2.0.1 with `overrideAttrs` (new `src` hash; pre-fetched cmake
  deps updated if 2.0.1 changed them). `nix/sherpa-lib.nix` links it.
  onnxruntime stays nixpkgs' 1.27.1 if build and tests pass; otherwise
  override it to 1.28.2 (what upstream 2.0.1 pins). Fallback if the source
  build fails: upstream prebuilt `sherpa-onnx-v2.0.1-linux-x64-shared`
  with autoPatchelf. Either choice is logged in Deviations.
- **Guard:** flake check `sherpa-version-match` compares the `sherpa-onnx`
  version in `Cargo.lock` (`builtins.fromTOML`) with the library's
  `version`; pure evaluation. A negative test proves it fails on a mismatch.
- **No regression:** Parakeet lines and texts on
  `~/meetingai-eval/aec-parakeet-tap.wav` identical on old and new library.
- **Comparison:** no filter vs VoxLock (sherpa-onnx streaming denoiser,
  explicit owner sample, never learned from the input) vs Alkove Focus
  (PyPI `alkove`, pinned, throwaway venv in the scratchpad, never imported
  by the app). Models pinned by hash; VoxLock's in `nix/voxlock-model.nix`.
- **Recordings** (owner, ~5 min, `~/meetingai-eval`, 0600, out of git):
  owner sample 15 s; owner reads 6 lines, speakers silent; phone plays
  Piper test speech while owner silent then reading; speakers play
  `speech.wav` while owner reads.
- **Metrics** per recording and filter, via `score.py --parakeet`: foreign
  words on the mic channel, owner words lost, invented lines, delay, CPU
  time per second of audio on p620.
- **Report:** `tools/accuracy/reports/2026-10-voice-filters.md`.
- **Decision rule:** a filter is proposed in a follow-up only if it removes
  more foreign words than #31's check labels `room`, loses at most 1 in 50
  owner words, and uses under half a core on p620.

## Repo traps (all steps)

- Implementers never commit, deploy or restart; the lead commits
  (`SKIP_BUMP=1` on docs-only commits). Report changed files and deviations.
- agy: the worktree is read-only to its shell; edit with file tools, use
  commands only for checks.
- After Codex or agy ran in the worktree: `chmod -R u+w target` and remove
  `target/*/build/sherpa-onnx-sys-*` before the lead's check.
- Never run anything against the owner's live microphone or desktop;
  recordings are step 5 only, run by the lead with the owner.
- The step 10 rewrite files (`speech/stt/transcription.rs`,
  `speech/session/listen.rs`, `cli.rs`) are not edited by this plan.
- No API keys in argv or prompts; subscription logins only.
- Check: `chmod -R u+w target; TMPDIR=/tmp devenv shell -- bash -c 'cargo fmt --check; cargo clippy --workspace --all-targets -q -- -D warnings; cargo test --workspace -q'`

## Steps

1. **Library 2.0.1 and the guard** (`nix/sherpa-onnx.nix` new,
   `nix/sherpa-lib.nix`, `flake.nix` checks, `nix/package.nix` if it
   names the library).
   → verify: `nix build .#default`, `nix flake check` (incl.
   `parakeet-decode`, `sherpa-version-match`), the negative test, the
   check command.
   Traps: nixpkgs' package file is `pkgs/by-name/sh/sherpa-onnx/package.nix`
   (pre-fetched FetchContent deps listed there); `SHERPA_ONNX_LIB_DIR` is
   set in `flake.nix` and `nix/package.nix`.
2. **Parakeet in the replay tools** (`examples/accuracy_replay.rs`:
   `--parakeet` next to `--url`/`--fake`, exactly one of the three;
   `tools/accuracy/score.py`: passes it through; `tools/accuracy/README.md`).
   → verify: the check command; `score.py ... --fake` still works; a unit
   test for the "exactly one" rule.
3. **VoxLock filter example** (`examples/voice_filter.rs` new;
   `nix/voxlock-model.nix` new, finished by the lead with the hashes).
   Usage `voice_filter IN.wav SAMPLE.wav OUT.wav --model-dir DIR`: stereo
   16 kHz in, left channel through `OnlineSpeechDenoiser` with the VoxLock
   config (`voice_sample` = SAMPLE.wav) in 20 ms chunks, right unchanged,
   stereo out; prints `{"rtf": ...}`.
   → verify: the check command; a test on a generated WAV that the right
   channel is unchanged and the output length matches (no model needed:
   the denoiser call behind a small seam).
4. **Alkove measurement script** (`tools/accuracy/alkove_filter.py` new).
   Same usage and output as step 3, using the `alkove` Python package's
   file API with a print learned from SAMPLE.wav. Run with `python3 -I`.
   Pins the package version in a comment and in the README's run line
   (venv in the scratchpad, `pip install alkove==<ver> --require-hashes`).
   → verify: `python3 -I -m py_compile`; `--help` runs without the package
   installed (import inside `main`).
5. **Measure and decide** (lead, with the owner on p620):
   a. score the 29 Oct tap on the old library, keep the output;
   b. after step 1 is built: score again, compare (identical);
   c. owner recordings via `tools/accuracy/record.sh` + references;
   d. run steps 3 and 4 on each, score all three variants;
   e. write the report, commit it; open the follow-up issue only if the
      decision rule passes.
   → verify: the report has every row and the decision.
6. **Ship:** PR linking intent, spec, plan; CI green; merge commit (never
   squash). Deploy to p620 from committed main, announced on the agent bus,
   not while recording; then `state.get` speech ready on Parakeet and a
   short recording transcribes.

## Tests

- `nix flake check` green with `sherpa-version-match` and `parakeet-decode`.
- The check command green.
- Identical Parakeet output on the 29 Oct tap (step 5b).

## Rollback

Revert the merge commit: `nix/sherpa-lib.nix` links nixpkgs' library again;
the examples and scripts are standalone. Redeploy p620 from main.

## Deviations

None yet.
