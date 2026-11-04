---
status: draft
issue: 70
spec: spec/2026-11-04-70-norwegian-transcription.md
coders:
  1: codex   # S0 spike: small, decides feasibility; exact sherpa-onnx API use
  2: codex   # whisper_nb engine (dormant) + VAD + phantom filter + split reuse: subtle
  3: agy     # tools/accuracy: no_mix.py builders, score.py metrics, 1x replay: large, mechanical Python
  4: coder   # Nix packaging: model, VAD, export tooling: ordinary, self-contained
  6: codex   # resolve_speech, codes, S6 guard, dictation, local-only: tricky routing
  7: coder   # Home Manager options, Settings picker, fake daemon + smoke
reviewers:
  - codex    # read-only `codex review --base origin/main` on the integrated branch
  - agy      # read-only `agy --mode plan` review of the diff against this plan
---

# Plan: Calls in Norwegian are transcribed as Norwegian, locally

This plan implements the approved spec on `fix/70-norwegian-transcription`.

- **Step 1 is a feasibility spike.** If it fails, stop and return to the
  owner.
- **Steps 2–4 build what the measurement needs.** The engine is dormant: no
  routing reaches it.
- **Step 5 is a hard stop for the owner.**
- **Steps 6–8 are built only if the owner approves the numbers.** If the
  gate fails, steps 2 and 4 are reverted.

## Decisions carried over from the approved spec

**Engines, all in-process through sherpa-onnx `2.0.1`:**
- Parakeet v3 is the baseline;
- Whisper large-v3-turbo comes from the official export
  `csukuangfj/sherpa-onnx-whisper-turbo` (MIT, int8);
- NB-Whisper (`NbAiLab/nb-whisper-large` or `-medium`, Apache-2.0) is
  exported with sherpa-onnx's Whisper export script, at a pinned NB-Whisper
  revision and a pinned export-tool version.

**The new engine, `whisper-nb-local`:**
- `language = "no"`, `task = "transcribe"`;
- **no prompt** (neither the glossary nor `filter.prompt`);
- long audio cut with `parakeet::split_points`/`decode_pieces` (~119–151);
- `SttWire::InProcess` dispatches by provider id (`transcription.rs`
  ~172);
- a Silero VAD (sherpa-onnx, pinned) decodes only speech segments, and a
  chunk with none is not sent;
- phantom phrases from a fixed list are removed anywhere in a line, and a
  line left empty is dropped.

**Measurement:**
- **Sets:** NPSC (`NbAiLab/NPSC`, CC0) is the gate set; the owner's private
  calls are an optional extra condition; FLEURS `nb_no` (CC BY 4.0) is
  reported only. Data stays in `~/.cache/meetingai-eval/no/`, never in git.
- **#14's failure cases,** from NPSC plus MUSAN (CC BY 4.0) noise: silence,
  room tone, non-speech bursts of 0.2–1.5 s, remote-only speech, double talk
  with delayed bleed, short words ("ja", "nei", "takk", "greit"), dictations
  of 1–3 s and 40–60 s, and code-switching taken from NPSC segments that
  already contain English.
- **Replay** runs at 1× wall-clock through the real `BatchChunks` and
  `LineFilter`.
- **Metrics:**
  - word error rate (edit distance);
  - invented lines per hour;
  - phantom phrases anywhere in a line;
  - bleed and lost owner words;
  - short words kept;
  - dictation success;
  - median and p95 chunk time;
  - peak RSS with Parakeet and the speaker model loaded, both resident and
    with the inactive engine unloaded;
  - all on p620 and razer.

**Gate:**
- NPSC word error rate ≤ 20%;
- at most 1 invented line per hour, and no phantom phrase;
- bleed and lost owner words no worse than Parakeet;
- short words kept ≥ 9 in 10;
- short and 40–60 s dictation succeed;
- on razer: median under 1.5 s and p95 under 3 s; peak RSS under 4 GB in
  the chosen loading mode.

Of the engines that pass, the lower NPSC error rate wins.

**Routing (built after the gate):**
- One `resolve_speech(settings) -> ResolvedSpeech { id, config, language }`
  is used by `speech_config`, preload in `record_start`/`dictate_start`,
  `speech_not_ready` and `state_value`.
- `parakeet-local` with `nb`/`no`/`nn` resolves to `whisper-nb-local`. A
  provider the user chose is never replaced.
- Auto stays on Parakeet, with no automatic switching.
- Engine and language are fixed per recording; a change applies at the next
  start, with a notice.

**Codes:**
- normalised once, in `resolve_speech`;
- empty or "Auto" → unset;
- `nb`/`no`/`nn` → `no` for OpenAI-compatible, Deepgram and
  `whisper-nb-local`; unchanged for custom;
- other languages pass through;
- Deepgram live keeps `en` as unset;
- custom multipart gains `language`.

**Wrong-language guard (S6):**

| Engine | Known support |
|---|---|
| Parakeet | its 25 languages |
| `whisper-nb-local` | `nb`/`no`/`nn` |
| Deepgram | a constant list, including `no` |
| OpenAI-compatible and custom | unknown |

- Known unsupported: refused before capture, naming an engine that can.
- Unknown: starts, with one notice.
- `auto_record_on_call` with a known-unsupported language: no capture, and
  the call-offer notice explains.

**Packaging:**
- `nix/whisper-nb-model.nix` and `nix/silero-vad.nix`, pinned hashes.
- An NB-Whisper export is a release asset of this repo, with
  `tools/whisper-export/` holding its pinned inputs and command.
- Home Manager: `whisperNb.enable` (default **off**) and `whisperNb.model`
  → `NIXARCHY_MEETINGAI_WHISPER_NB_DIR`, plus the VAD path. The model is in
  the closure only when enabled. Readiness comes from
  `whisper_nb::not_ready_reason()`.
- Local-only: `speech_config`'s allowlist (~1199) admits `whisper-nb-local`;
  `migrate_whisper` (~744) rewrites only `whisper-local`.

**Out of scope:** Auto language detection (a later issue), English-only live
triggers (#71), and the `"en"` meeting language column.

## How it runs

- **Step check:**

  ```bash
  nix develop -c bash -c 'cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check'
  ```

  Step 7 adds `bash shell/tests/ci.sh`.
- One commit per step, `<type>(<area>): … (#70, plan step N)`. Deviations
  are recorded under **Deviations** in the same commit.
- Each new test fails before its change.
- **Traps:**
  - before any cargo build, run
    `find target/debug target/release -maxdepth 2 -name 'lib*.so*' ! -perm -u+w -delete`
    (the `sherpa-onnx-sys` EACCES trap);
  - `Inner` is a `parking_lot` mutex, never held across `.await`;
  - handoffs use `agent-run … < /dev/null`;
  - Python runs with `-I`; pyarrow comes from
    `python3.withPackages (p: [ p.pyarrow ])`;
  - nothing is added to the dev shell.

## Steps

### 1. S0 spike (codex)

- Add `examples/whisper_spike.rs`: build sherpa-onnx's offline Whisper
  recognizer next to how `parakeet.rs` (~70–81) builds the transducer, load
  the turbo export from a path argument, and decode one WAV with
  `language = "no"`.
- Report the exact config type and field names.

Check: the example builds, clippy is clean, and on p620 it decodes one FLEURS
`nb_no` clip to Norwegian text. Claude downloads the turbo export to the
cache.

**If the binding has no Whisper support, stop and return to the owner.**

### 2. The dormant engine (codex)

1. **`src/speech/stt/whisper_nb.rs`:**
   - load lazily from `NIXARCHY_MEETINGAI_WHISPER_NB_DIR`;
   - `not_ready_reason()`, `preload()` and `transcribe(samples) -> String`,
     mirroring `parakeet.rs`;
   - the decode mutex shared the same way;
   - split with `parakeet::split_points`/`decode_pieces`, made `pub(crate)`;
   - the VAD from `NIXARCHY_MEETINGAI_SILERO_VAD`;
   - the phantom-phrase filter as a pure function with a fixed list.
2. **`SttWire::InProcess`** dispatches on the provider id: `parakeet-local`
   → Parakeet, `whisper-nb-local` → `whisper_nb`. Register
   `whisper-nb-local` as an in-process speech provider that no setting or
   route selects yet.
3. **Tests:**
   - the phantom filter removes a phrase at the start, middle and end;
   - VAD and splitting with a fake decoder: no speech → nothing sent; 0.3 s
     of speech kept; 45 s split under 30 s;
   - dispatch by id;
   - the Parakeet path is unchanged.

Check: the step check.

### 3. Measurement tools (agy)

Under `tools/accuracy/`, beside the existing files:
1. **`no_mix.py`** builds every set in the decisions above from NPSC,
   FLEURS `nb_no` and MUSAN, in `libri_mix.py`'s truth format, with seeds.
   It also builds the owner set from a folder if one is given.
2. **`score.py`** gains:
   - `--engine parakeet|whisper-nb|turbo`;
   - word error rate (edit distance, NPSC normalisation);
   - invented lines per hour;
   - phantom phrases anywhere;
   - short words kept;
   - dictation success;
   - median and p95;
   - peak RSS (`/proc/self/status` VmHWM).
3. **1× pacing** in the replay (`examples/accuracy_replay.rs`, or the
   driver `score.py` uses): feed PCM at wall-clock speed.
4. **README:** licences (NPSC CC0, FLEURS CC BY 4.0, MUSAN CC BY 4.0),
   sizes and commands.

Check: `python3 -I -m py_compile` on the new scripts, and a small slice
built and scored with Parakeet.

### 4. Packaging (coder)

1. `nix/whisper-turbo-model.nix` and `nix/silero-vad.nix`, with pinned
   hashes, following `nix/parakeet-model.nix`.
2. **`tools/whisper-export/`:** the script, the pinned NB-Whisper revision,
   the export-tool version and the exact command to export NB-Whisper to
   sherpa-onnx int8. **Claude runs it once** and publishes the result as a
   release asset of this repo, then adds `nix/whisper-nb-model.nix` pinned to
   it.
3. Each model's licence travels with it.

Check: `nix build` of each model derivation, plus `nix flake check -L
--no-build`.

### 5. Gate: the owner approves the numbers (Claude, then STOP)

1. Build the sets (step 3), and run every engine on p620 and razer, both
   resident and with the inactive engine unloaded.
2. Write `docs/eval/2026-11-norwegian-stt.md` with every bar against its
   measured value, the chosen engine and the chosen loading mode. Post the
   table on #70.
3. **If no engine passes:**
   - commit the eval doc;
   - revert steps 2 and 4;
   - mark steps 6–8 "not built";
   - stop for the owner.
4. **If one passes:** stop for the owner's explicit approval, recorded as
   `docs(eval): approve Norwegian STT numbers (#70)`.

### 6. Routing, codes, guard and local-only (codex)

In `src/daemon/session.rs`, `src/speech/stt/transcription.rs` and
`src/core/models/settings.rs`:
1. **`resolve_speech`**, with the normalised language. Replace every use of
   the stored provider for engine decisions with it:
   - `speech_config` (~1193);
   - preload in `record_start` (~2485) and `dictate_start` (~3290);
   - `speech_not_ready` (~643) and `state_value` (~985).
2. **Codes** for every path, including Deepgram live (~808), custom
   streaming (~865) and multipart (~308).
3. **The S6 guard** at `record.start` and `dictate.start`, before capture,
   with the known-support table and the auto-record rule.
4. **Local-only:** `speech_config`'s allowlist admits `whisper-nb-local`;
   `migrate_whisper` is untouched for the new id.
5. **Tests:** every routing, code, guard, local-only, dictation and
   setting-change case listed in the spec's Verification.

Check: the step check.

### 7. Home Manager, Settings and shell (coder)

1. **`nix/hm-module.nix`:**
   - `whisperNb.enable` (default off) and `whisperNb.model` (the step 5
     engine);
   - the daemon environment gets the model and VAD dirs when enabled,
     mirroring `parakeet.enable` (~170–181);
   - the assertion text covers it.
2. **`shell/Overlay.qml`:**
   - a speech-language picker next to the speech engine, with Auto plus the
     active engine's languages;
   - the Auto hint;
   - the guard and "unknown support" notices shown as the existing notices
     are.
3. **`shell/tests/fake-daemon.py` and the smoke test:** the picker sends
   `settings.set speech_language`; the hint and the refusal notice are
   shown.

Check: the step check plus `bash shell/tests/ci.sh`.

### 8. Integrate, review, ship (Claude)

1. Merge `main`. Run the step check, `ci.sh`, `nix flake check -L
   --no-build` and `nix build .#checks.x86_64-linux.shell -L`.
2. **Reviews, read-only and in parallel:**
   - a fresh Opus agent, given this plan and `git diff origin/main`;
   - `codex review --base origin/main -c sandbox_mode="read-only"`;
   - agy `--mode plan`.

   Verify each finding, fix the confirmed ones, and re-run the checks.
3. **PR:**
   - link intent, spec, plan and the eval doc;
   - list who did each step;
   - merge with a merge commit after CI.
4. **Deploy p620** the meetingai-only way:
   - enable `whisperNb` in the nixos config by the owner's choice (a
     separate nixos PR);
   - deploy from a clean `origin/main` worktree;
   - announce on the bus;
   - not while recording.
5. **The owner's live checks:**
   - a Norwegian call and a Norwegian dictation;
   - a quiet stretch with typing;
   - "ja"/"nei" answers.

## Tests

- The step check is green after every step, and `ci.sh` and `nix flake
  check` are green at step 8.
- Every new test fails before its change.
- Step 5's numbers are approved by the owner before step 6.

## Rollback

- **Before merge:** drop the branch.
- **After merge:** `git revert -m 1 <merge>`, revert the nixos change, and
  switch.
- **Live switch-off:** set `speech_language` back to Auto (Parakeet), or
  `whisperNb.enable = false`.
- **No database change.**

## Deviations

(Recorded here during implementation, in the same commit as the code.)
