---
status: approved
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
- phantom phrases from a fixed list are removed **anywhere** in a line, and
  a line left empty is dropped. The spec's Risks section still says "only
  whole-line"; the plan follows S3 and the review record;
- the VAD adds **no minimum length** of its own beyond its speech segments,
  so "ja" and "nei" survive;
- the OpenAI-compatible path keeps its prompt: it is the user's engine.

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

**Rules that must hold:**
- any language other than `nb`/`no`/`nn` with `parakeet-local` stays on
  Parakeet;
- "Auto" is matched in any case, and an empty value means Auto, including
  values set through Nix `speechLanguage`;
- `speech_language` never changes `response_language`;
- code-switch error rate is reported (S1), and the picker hint for an
  English call with `nb` set says to use Auto.

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
   - load from `NIXARCHY_MEETINGAI_WHISPER_NB_DIR` (the model folder: the
     turbo export or the NB-Whisper export, same layout);
   - `not_ready_reason()`, `preload()`, `unload()` and
     `transcribe(samples) -> String`, mirroring `parakeet.rs`.
2. **One decode lock and loading modes:**
   - Both in-process engines decode under **one global decode mutex** (move
     Parakeet's into a shared `stt::DECODE`), so decodes serialise as they
     do live.
   - Add `unload()` to Parakeet (drop the recognizer).
   - Add `LoadMode { Resident, UnloadInactive }`: when one engine loads
     under `UnloadInactive`, the other is unloaded. The gate picks the mode;
     until then the default is `Resident`, which is today's behaviour.
3. **Splitting:** use `parakeet::split_points`/`decode_pieces`, made
   `pub(crate)`.
4. **VAD:** from `NIXARCHY_MEETINGAI_SILERO_VAD`. The phantom-phrase filter
   is a pure function with a fixed list.
5. **Dispatch and registration:**
   - `SttWire::InProcess` dispatches on the provider id: `parakeet-local` →
     Parakeet, `whisper-nb-local` → `whisper_nb`.
   - `transcribe_with_prompt` ignores the prompt for in-process engines
     (~164–175).
   - Register `whisper-nb-local` in the STT provider catalogue next to
     `parakeet-local`, as in-process. No setting or route selects it yet.
6. **Tests:**
   - the phantom filter removes a phrase at the start, middle and end;
   - VAD and splitting with a fake decoder: no speech → nothing sent; 0.3 s
     of speech kept; 45 s split under 30 s;
   - dispatch by id;
   - the prompt is ignored in-process;
   - `UnloadInactive` unloads the other engine, and `Resident` keeps both;
   - the Parakeet path is unchanged.

Check: the step check.

### 3. Measurement tools (agy)

1. **`tools/accuracy/no_mix.py`** builds every set in the decisions above,
   in `libri_mix.py`'s truth format, with seeds:
   - **NPSC:** the test split from `NbAiLab/NPSC` on Hugging Face (parquet,
     CC0), about 2 h, selected by a fixed seed;
   - **FLEURS `nb_no` test** (`google/fleurs`);
   - **MUSAN** noise subset (OpenSLR 17), only the `noise/` part, about
     1 GB;
   - the owner set from a folder, if one is given.

   Download commands and sizes go in the README.
2. **`examples/accuracy_replay.rs`:**
   - `--engine parakeet|whisper-nb|turbo` plus `--model-dir`. It builds the
     provider config by id (`parakeet-local`, or `whisper-nb-local` with the
     given folder; turbo is `whisper-nb-local` pointed at the turbo export)
     and sets the VAD env var. No settings routing is involved.
   - **Live pacing:** it drives the real
     `LiveTranscriber::connect_config_with` with PCM at 1× wall-clock, as
     `examples/diarize_eval.rs --live` does. Input, decode results and
     `filter.tick` interleave exactly as in a call, so the 3 s bleed hold is
     exercised.
   - `--load-mode resident|unload` selects the step 2 mode.
   - **Peak RSS** is measured in this Rust process (`VmHWM`), after loading
     Parakeet and the speaker model too.
   - **Dictation** goes through `transcription::transcribe(&config, wav,
     None)`, the dictation path, not `BatchChunks`.
3. **`score.py`** reads the replay's output and adds:
   - word error rate (edit distance, NPSC normalisation);
   - invented lines per hour;
   - phantom phrases anywhere;
   - short words kept;
   - dictation success;
   - median and p95;
   - code-switch error rate;
   - the RSS reported by the replay.
4. **README:** licences (NPSC CC0, FLEURS CC BY 4.0, MUSAN CC BY 4.0), sizes
   and commands.

Check:
- `python3 -I -m py_compile` on the scripts;
- a small slice scored with **Parakeet and with the turbo export**, both
  through the paced replay;
- a held mic line is released about 3 s after its chunk, shown in the
  replay log;
- RSS is reported from the replay process.

### 4. Packaging and the NB-Whisper export (coder)

1. `nix/whisper-turbo-model.nix` and `nix/silero-vad.nix`, with pinned
   hashes, following `nix/parakeet-model.nix`.
2. **`tools/whisper-export/`:** the script, the pinned NB-Whisper revision,
   the export-tool version and the exact command to export NB-Whisper to
   sherpa-onnx int8.
   - **Claude runs it once.** It succeeds only if the step 1 spike decodes a
     FLEURS `nb_no` clip with the result.
   - On success, Claude publishes it as a release asset of this repo and
     adds `nix/whisper-nb-model.nix` pinned to it.
   - **If the export fails,** NB-Whisper drops out and the gate compares
     turbo against Parakeet only. That is recorded as a deviation, as the
     spec allows.
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
5. **Tests:**
   - `resolve_speech`:
     - the default engine plus `nb`/`no`/`nn` gives `whisper-nb-local`;
     - a chosen provider is never replaced;
     - other languages stay on Parakeet;
     - "Auto" in any case and empty are both Auto;
   - preload, `speech_not_ready` and `state_value` follow the resolved
     engine, and a missing Norwegian model gives its own message;
   - codes for every engine and path, including Deepgram live `en`, custom
     streaming and multipart `language`;
   - the guard:
     - known unsupported is refused before capture, naming an engine that
       can;
     - unknown engines start with one notice;
     - auto-record with a known-unsupported language does not capture, and
       shows the notice;
   - local-only: `whisper-nb-local` is admitted, cloud is refused, and
     `migrate_whisper` leaves the new id alone;
   - dictation of 1–3 s and 40–60 s with `nb`;
   - a setting change during a recording applies at the next start, with a
     notice;
   - `speech_language` leaves `response_language` unchanged.

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

## Review record (draft plan)

| Finding | From | Verdict |
|---|---|---|
| No unload mechanism to measure; RSS read from the Python scorer | Codex, agy | confirmed → step 2 `unload()` and `LoadMode`; replay measures VmHWM |
| The replay cannot select the Whisper engines or the VAD | Codex, agy | confirmed → `--engine`/`--model-dir` by provider id |
| 1× pacing alone does not exercise the bleed hold | Codex | confirmed → drive the real `LiveTranscriber`; check the 3 s release |
| Dictation is not measured through its own path | agy | confirmed → `transcription::transcribe` |
| The decode mutex is ambiguous | agy | confirmed → one global `stt::DECODE` |
| Prompt suppression and provider registration anchors | agy | confirmed → step 2.5 |
| Decisions delegated to the spec; missing rules | Codex, agy | confirmed → "Rules that must hold"; step 6 tests listed inline |
| No export success check or fallback; no NPSC/MUSAN acquisition | Codex | confirmed → step 4 check and turbo fallback; step 3 sources and sizes |
| The spec's Risks contradicts S3 on phantom phrases | Codex | confirmed → the plan follows S3 and the review record (anywhere) |
| A pre-gate smoke check for both candidates | Codex | confirmed → step 3 check |

## Deviations

(Recorded here during implementation, in the same commit as the code.)

- Step 1 (Codex; build and decode by Claude):
  - **sherpa-onnx 2.0.1 supports Whisper in-process**, via
    `OfflineRecognizerConfig.model_config.whisper: OfflineWhisperModelConfig`
    (`encoder`, `decoder`, `language`, `task`) plus `model_config.tokens` and
    `num_threads`.
  - Claude decoded FLEURS `nb_no` test clip `10009208613112238545` with the
    official turbo int8 export. The result is correct Norwegian, with minor
    spelling slips ("løveflok", "tilfelles"). The run took 12.2 s including
    the model load, at 2.0 GB peak RSS on p620.
  - FLEURS WAVs are not 16-bit PCM. The measurement converts with `ffmpeg
    -c:a pcm_s16le`.
- Step 2 (Codex; checks by Claude):
  - `whisper-nb-local` is registered in `STT_PROVIDERS` but reaches no
    picker: only a transcription test iterates the list, and routing comes in
    step 6.
  - The dispatch and loading-mode tests were added after the implementation,
    because Codex's sandbox could not run the suite.
  - Claude ran the step check: 457 lib tests.
  - The first real Whisper+VAD run happens in step 3's paced replay.
- Step 4 (coder; one fix by Claude):
  - `nix/whisper-turbo-model.nix` (`sha256-sRrLvNZg…`, MIT) and
    `nix/silero-vad.nix` (`sha256-niRJ4Qh0…`, MIT), both from the k2-fsa
    sherpa-onnx `asr-models` release, and both build.
  - They are callable like `parakeet-model.nix`, not flake `packages`
    outputs; step 7 wires them.
  - `tools/whisper-export/` pins `nb-whisper-large` at revision `8c6249f`
    and sherpa-onnx at `v1.12.40` (`d795ccd`).
  - **Claude corrected the export base from large-v2 to large-v3:**
    NB-Whisper large's `preprocessor_config.json` has `feature_size: 128`,
    which means large-v3. The export itself is not run yet (step 5).
- Step 3 (agy, `agent-run` reported "empty response"; the work was in the
  tree):
  - It adds `no_mix.py`, extends `score.py` and the README, and rewrites
    `examples/accuracy_replay.rs` for paced live replay by engine.
  - agy also edited `src/speech/stt/`, outside its brief. Claude kept
    `malloc_trim(0)` after an engine unloads: glibc does not return freed
    memory otherwise, so the unload mode could not show a saving.
  - Claude made agy's per-chunk latency recorder **opt-in**. As written it
    pushed on every daemon chunk with no reader: a slow leak. Now only the
    replay switches it on, and a test pins that the daemon never records.
  - Claude fixed one clippy `while_let_loop`, and ran the step check: 458
    lib tests.
  - The functional check (paced replay with Parakeet and turbo, the 3 s
    hold, RSS) runs in step 5, so it does not share the CPU with the
    NB-Whisper export.
