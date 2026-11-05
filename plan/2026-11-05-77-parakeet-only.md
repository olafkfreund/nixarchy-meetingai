---
status: approved
issue: 77
spec: spec/2026-11-05-77-parakeet-only.md
coders:
  1: agy     # large mechanical removal across many files
  2: codex   # small tricky logic: language lifecycle, prompt override, migration
  3: coder   # the shell picker and smoke test, ordinary QML
  4: coder   # Home Manager options and docs, ordinary edits
reviewers:
  - codex    # read-only: session lifecycle and privacy paths
  - agy      # read-only: leftovers of the removal and docs
---

# Plan: Parakeet as the only local engine, with a per-meeting language

**Running order:** step 1 first, alone, because it edits `session.rs`. Then steps 2, 3 and 4 in parallel, since they touch
different files and share only the contract below. Claude runs every check, reviews each diff and commits; step 5 is Claude's.

## Decisions carried from the approved spec

- **Scope:** the local Whisper engines go: NB-Whisper, turbo, Silero VAD, the export tools, the engine-swapping code. Deepgram and
  OpenAI-compatible servers stay. The code is removed, not switched off.
- **Accepted trade-off:** no local Norwegian. A Norwegian call on Parakeet with no choice made is Swedish-looking text again. Choosing
  Norwegian is only offered on an engine that can do it, and nothing moves to the cloud by itself.
- **"English" means nothing chosen.** Choosing it clears the choice, and `en` is never sent to an engine because of it.
- **The language's lifecycle:**

  | Event | Result |
  |---|---|
  | `meeting.language.set` | set; `""` or `"en"` clears; saved to `meetings.language` at once if a meeting is open or just finished |
  | Meeting starts | written to `meetings.language`, replacing the hardcoded `"en"` |
  | Stop or finish | kept, for Play, follow-up questions and the recap |
  | Play, restore, resume | read back from the meeting's row |
  | New, with a finished or restored meeting open | cleared, in `reset_session` |
  | New, with no meeting open | kept, as a set-up interview keeps its candidate, so choose-then-record works |

- **Display:** the chosen language; otherwise a global `speech_language` if set; otherwise English.
- **Speech:** effective language = the meeting's, else global `speech_language`, worked out after deciding New or Play. Parakeet ignores it,
  but the guard refuses what it cannot do. Standalone dictation uses the global setting.
- **Answers:** prompts use the meeting's language if chosen, else the conversation's latched language. `conversation_response` is never changed
  by this. Rolling notes and recaps use the language of the meeting being summarised, by ID, else `response_language`. Prompts get the name
  ("Polish"), not the code.
- **Local-only:** agents' `state.get` redaction also nulls `meeting_language`. No change to which engine is allowed.
- **Migration:** see step 2.3. It runs beside `migrate_whisper`, before Nix settings are applied, so a Nix-set `nb` is applied again and refused at
  recording start.

## The contract between steps 2 and 3

- **Command:** `meeting.language.set {"language": "<code or empty>"}`, replying `{}` or an error. Not on the agent (MCP) allow-lists. An accepted change emits
  updated state, as the shell reads daemon state, not the reply.
- **`state.get` gains:**
  - `meeting_language`: chosen code, or `null`;
  - `meeting_language_label`: what to show, per Display above;
  - `languages`: `[{"code":"pl","name":"Polish"}, ...]` for the current engine, English first. The whole table for engines whose support cannot be checked;
  - `language_hint`: the one-line hint text, written by the daemon so the shell has no engine logic.
- **Hint text:** on Parakeet, "Parakeet detects the language itself. This sets the language of answers and notes, over the answer language setting. Norwegian needs Deepgram."
  On an engine whose support cannot be checked, the same first idea and "This engine's language support cannot be checked." Otherwise, "This sets the speech language, and the language of answers and notes, over the answer language setting."
- **The shell** maps `{code, name}` to `Select`'s `{value, label}`, with `en` as value `""`.

## Traps

- **sherpa-onnx build:** before cargo, `find target/debug target/release -maxdepth 2 -name 'lib*.so*' ! -perm -u+w -delete`.
- **Codex and agy sandboxes cannot run nix or cargo.** Claude runs every check.
- **`DECODE` stays.** Parakeet's `preload` and `transcribe` rely on it (`parakeet.rs:113`).
- **`agent-run` treats a moved `HEAD` as a failure,** so nothing is committed while a handoff runs.
- **`docs-match`:** after any option change, regenerate the README block between the `nix-options` markers from `nix build .#docs`.
- **`record_start`'s order must not change.** The language, Local-only and consent checks run before `reset_session` (:2743), and capture starts before it too (:2709). Moving
  the reset first would wipe an open meeting on a refused start; moving the checks later would capture before checking. Compute the value instead: step 2.4.
- **`tools/accuracy/score.py`:** #73's open branch edits it heavily, so leave it alone. It reads `load_mode` with a default, so its removal is harmless.

## Steps

### 1. Remove the engines (agy, alone)

1. **Delete:** `src/speech/stt/whisper_nb.rs`, `examples/whisper_spike.rs`, `nix/whisper-nb-model.nix`, `nix/whisper-turbo-model.nix`,
   `nix/silero-vad.nix`, `tools/whisper-export/`.
2. **`src/speech/stt/mod.rs`:** remove `pub mod whisper_nb` (:6), `LoadMode`, `LOAD_MODE`, `set_load_mode`, `unload_inactive` (:14–29). Keep `DECODE`.
3. **`src/speech/stt/parakeet.rs`:** remove the `unload_inactive` branches (:114, :185) and `unload_locked` and `unload` (:122–138) if nothing else calls them, and
   the test-only `loaded` (:203) if no test is left using it, or clippy fails on dead code.
4. **`src/speech/stt/transcription.rs`:** remove the "Norwegian Whisper" name (:426), the `whisper-nb-local` dispatch (:441), and the tests using it
   (:1787, `load_modes_keep_or_unload_the_inactive_engine` :1804). Keep a test that the prompt is ignored in-process on Parakeet.
5. **`src/ai/catalog/providers.rs`:** remove the entry (:121) and readiness (:238); the catalogue test expects 3 and lists `whisper-nb-local` as retired.
6. **`src/daemon/session.rs`:** `speech_not_ready` (:649); routing in `resolve_speech` (:684), keeping the `no` normalisation for other engines; `local_speech` (:713);
   `speech_guard` (:758, :825, whose message now names only Deepgram or an OpenAI-compatible engine); `preload_speech` (:843); Local-only messages (:1205, :2622);
   the id checks at :2691, :3497, :3519; and the tests at :6657–:6780, adjusted so `nb` on the default engine is refused on Parakeet.
7. **`examples/accuracy_replay.rs`:** remove the `whisper-nb` and `turbo` engines, `--model-dir`, `--vad`, `--load-mode` and their tests; keep every other mode working. The
   summary drops `load_mode`.

Leave `norwegian_whisper_is_not_migrated` (`session.rs:8185`) for step 2, which replaces it. `flake.nix` is step 4's.

Check: the step check below, and `git grep -n -i -E 'whisper_nb|whisper-nb-local|LoadMode'` under `src` and `examples` returns only the retired-ID test and that one.

### 2. The meeting's language (codex) — `src/daemon/session.rs`, `src/core/models/settings.rs`, `src/db/repos/meetings.rs`

1. **`language_support(engine, model, code)`:** moves the lists from `speech_guard` (:717) into one function returning supported, unsupported or unknown; the guard behaves as today.
   A single code-to-name table covers every code in those lists, about 55, with common English names. A test checks that every listed code has a name. `response_instruction`
   already takes a name, so it does not change.
2. **State and command:** `Inner.meeting_language`; the command and `state_value` (:1178) fields in the contract; `meetings::set_language` beside `reopen`. The agents' redaction at :5295 also nulls `meeting_language`.
   **Storage:** nothing chosen is stored as `en`, and reading normalises `en` and `""` to `None` everywhere, because every existing row says `en`.
3. **`migrate_norwegian_engine(&Database)`,** beside `migrate_whisper` (:940), added to the list at :1072, with its marker `norwegian_engine_notice_shown`. Provider first, then language, so a saved
   `whisper-nb-local` with `nb` ends on Parakeet with no language:
   - `speech_provider = whisper-nb-local` → `parakeet-local`;
   - `speech_language` of `nb`, `no` or `nn`, any case or region, with a now empty or `parakeet-local` provider → empty;
   - the same with Deepgram or OpenAI-compatible → unchanged, no notice;
   - the notice says the local Norwegian engine was removed, Parakeet does not transcribe Norwegian, and Deepgram can. A fresh install is marked silently.
4. **Lifecycle,** per the table above. In `record_start` (:2573), **nothing moves**: before `resolve_speech` (:2600), compute `chosen = if new && g.resume_meeting.is_some() { None } else { g.meeting_language.clone() }` and use it for
   the guards and engine config, without resetting anything. Write `chosen` to the column at :2776. `keep` at :2736 also keeps the language when no meeting is open; `reset_session` (:3622) clears it, and `finish_recording` (:3021) does not;
   `session_restore` (:3690) and Play read it from the row.
5. **Speech:** a helper applies the effective language to a settings copy before `resolve_speech` and `speech_config`, in `record_start` and the readiness state. Standalone `dictate_start` (:3479) stays on the global setting.
6. **Answers:** `system_prompt` (:1653) uses the meeting's language name if chosen, without touching `conversation_response` at :1884 or :3829; `spawn_chunk` (:2526) and `finish_meeting_artifacts` (:3202) resolve by meeting ID from the row.
7. **During recording:** `meeting.language.set` emits the existing "applies at the next recording start" notice only if the engine accepts a language.
8. **Tests:**
   - the migration: each row, the combined provider-and-language case, a fresh install silent, a Nix-set `nb` refused at recording start, replacing `norwegian_whisper_is_not_migrated`;
   - each lifecycle row, including a New call never using the previous meeting's language, a refused New leaving the open meeting intact, and choose-then-record keeping the choice;
   - a change during a meeting surviving restore; `en` and `""` clearing it; old rows reading as nothing chosen;
   - a language refused on Parakeet, and Deepgram receiving the chosen code;
   - prompts using the name; clearing restoring the conversation's own language; a recap of an older meeting using that meeting's language;
   - Local-only redaction; `settings.search` still finding `speech_language`;
   - a default meeting producing today's prompt and engine config.

Check: the step check.

### 3. The picker (coder) — `shell/`

1. **Remove** the Settings picker and hint from `shell/Overlay.qml` (:589–601, :666).
2. **Add** a compact `Select`, `objectName: "meetingLanguage"`, in the meeting view in `shell/Overlay.qml`, inside `convo` (:393), above `Composer`, not in `Header.qml`: the header is one row, shown in the library too, with its width already tight. It reads
   `state.languages`, shows `meeting_language_label`, and sends the contract's command.
3. **Hint,** `objectName: "meetingLanguageHint"`, shows `state.language_hint` as is.
4. **`shell/tests/fake-daemon.py`:** the command, logged so the smoke test can see it, and the state fields. **`shell/tests/smoke.sh`** (:637–642, :743, :747): the settings phase no longer looks for the removed picker, and the meeting-view phase (outside the library, as `find` needs
   visible items) chooses Polish, sees `meeting.language.set` in the log, the hint, and no Norwegian on Parakeet.

Check: `bash shell/tests/ci.sh`. `check-geometry` must stay at 0 problems.

### 4. Home Manager and docs (coder) — `nix/hm-module.nix`, `README.md`, `tools/accuracy/README.md`, `docs/eval/`

1. **`nix/hm-module.nix`:** remove the environment (:82–85) and options (:187–205); add `whisperNb` `enable`, `model`, `vad` to the `mkRemovedOptionModule` pattern at :133, with a message saying Parakeet has no Norwegian and Deepgram can do it; the assertion at :376 drops `whisperNb`, and :372 also rejects `whisper-nb-local`.
2. **`README.md`:** requirements (:27), Local-only (:140), provider table (:185), Fully offline (:244), Settings. The Norwegian section becomes a short "Languages": Parakeet detects by itself, what the meeting language does, that a Norwegian call on Parakeet comes out wrong, and that Deepgram can do it but sends audio to the cloud, with `deepgram.region = "eu"` advised, since only that region opts out of model improvement. Regenerate the options block.
3. **`tools/accuracy/README.md`:** remove the Whisper engine and model instructions (:28–37, :58–60, :86–90, :115–134). Keep the NPSC set builder's instructions.
4. **`docs/eval/2026-11-norwegian-stt.md`:** a note at the top that the engine was removed by #77, keeping the results.
5. **`flake.nix`:** the removed-options check (:327) also covers a `whisperNb` option, and :328's provider check covers `whisper-nb-local`.

Check: `nix flake check`, by Claude.

### 5. Integrate, review, ship (Claude)

1. Full step check, `bash shell/tests/ci.sh`, `nix flake check`, the spec's search for leftovers, and an evaluation showing a `whisperNb` option fails with its message.
2. Reviews, read-only: a fresh Opus agent, `codex review --base origin/main`, and agy. Fix what holds up.
3. PR linking intent, spec and plan, naming who did each step; merge after CI.
4. **p620:** check the bus and that nothing is recording. One nixos PR bumps only the meetingai input and removes `whisperNb.enable`. Test build, CI, merge, then deploy from a clean worktree at `origin/main`. Verify the daemon, that the closure has no NB-Whisper model, and that a saved setting survives.
5. **Issues:** note on #70 that its engine was removed; close #74 as not planned; note on #71 what is resolved. #73's plan names NB-Whisper, so its amendment goes to the owner for approval. It is not changed here.
6. The owner's live check: an English call as today, and a chosen Polish meeting with Polish answers.

## Step check

```bash
find target/debug target/release -maxdepth 2 -name 'lib*.so*' ! -perm -u+w -delete
nix develop -c bash -c 'cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check'
```

## Deviation from the spec's wording

The spec places the control "near the mode picker". Reviews showed the header cannot hold it, so it goes in the meeting view above the composer. The behaviour is unchanged.

## Rollback

- **Before merge:** drop the branch.
- **After merge:** `git revert -m 1 <merge>`, restore `whisperNb.enable` in nixos, and switch. A migrated setting is not restored; the user picks Norwegian again.

## Deviations

(Recorded here during implementation, in the same commit as the code.)
- Step 1 (agy; deletion and checks by Claude):
  - agy's tools cannot delete files, so it emptied them; Claude removed them with `git rm`.
  - `nix/hm-module.nix` and `flake.nix` still reference the deleted model files until step 4, so `nix flake check` fails between these commits.
  - `speech_guard`'s refusal now always names Deepgram or an OpenAI-compatible engine, with no Norwegian branch.
- Step 2 (Codex; compile fixes by Claude):
  - Nothing chosen is stored as `""`, not `en`. Reading normalises both to none, so the rule holds for old rows.
  - `MeetingRecord.language` is now `Option<String>`; `tests/old_data.rs` is updated. Nothing in the shell or MCP reads it.
  - `record_start`'s second block uses `g.meeting_language` after the New handling, not `chosen`, which is out of scope there and could
    miss a change made during the blocking open.
  - `session.new` also keeps the language if no meeting is open.
  - Claude tightened a test's wait, and reworded the hint for engines whose support cannot be checked.
- Step 4 (coder):
  - `tools/accuracy/README.md` also renames `replay-turbo.jsonl` to `replay-parakeet.jsonl` in its examples.
- Step 3 (coder; a first coder stalled with no edits and was replaced):
  - The picker is a `Column` holding the `Select` and the hint, above `Composer`; `convo.below` accounts for its height.
  - `smoke.sh`'s export-flow timeout goes from 16 s to 20 s, for the new phases.
