---
status: draft
issue: 77
author: olafkfreund
---

# Intent: Parakeet as the only local speech engine, English by default, language chosen per meeting

## Problem

The owner wants a simpler speech setup. #70 added a second local engine, NB-Whisper,
and a global speech-language setting, and that has costs:

- Norwegian lines arrive more slowly (2.4 s median, 7.8 s p95 per chunk, against 0.55
  and 1.95 s on Parakeet), and it uses about 2.1 GB of RAM once loaded.
- It adds an engine, a model, Nix options and routing to maintain.
- The language is a global setting that must be remembered before each call, while it
  only matters for an occasional meeting.

The owner's direction: drop the other engines, keep Parakeet, default to English, choose
a language by hand in a meeting when needed, and improve language handling later in other
ways.

**Two facts limit what that can deliver, and the owner should decide with them in view:**

1. **Choosing a language does not change what Parakeet transcribes.** Parakeet detects
   the language itself and takes no language input: `transcribe_in_process`
   (`src/speech/stt/transcription.rs:415`) passes none, and neither NVIDIA's model listing
   nor the sherpa-onnx example shows such an option. So "English" on Parakeet behaves
   exactly like today's Auto, and it cannot stop Parakeet misdetecting a call. Whether the
   model can be constrained some other way has not been checked in the source.
2. **Parakeet has no Norwegian.** It covers 25 languages, including English and Polish.
   A Norwegian call on Parakeet came out as Swedish-looking text at 85.1% word errors, the
   bug #70 fixed. If NB-Whisper goes, Norwegian can only be transcribed by a cloud or
   self-run engine, or not at all.

## Proposed outcome

These hold whichever way the open questions go:

- A fresh install has one local engine, Parakeet, and no local Whisper engine: no model in
  the closure, no `whisperNb` options, no routing to it.
- The meeting shows English as its language, and another can be chosen from the meeting
  itself, without going to Settings and without changing the next meeting.
- A language the engine in use cannot do is refused with a clear reason, never written as a
  neighbouring language. On Parakeet, that includes Norwegian.
- English calls work as they do today, at today's speed.

**"English" here means nothing was chosen, not "force English".** Saving `en` as the global
setting would change behaviour for other engines: Deepgram's batch path and OpenAI-compatible
servers would start receiving `language=en` on every call, where today they get none. On
Parakeet, a Polish call under "English" still transcribes as Polish, since it detects by itself.

## Affected users and systems

- **Removed:** `src/speech/stt/whisper_nb.rs`, `whisper-nb-local` in
  `src/ai/catalog/providers.rs`, routing in `resolve_speech`
  (`src/daemon/session.rs:663`), `nix/whisper-nb-model.nix`, `nix/silero-vad.nix`,
  `nix/whisper-turbo-model.nix`, `tools/whisper-export/`, and the `whisperNb.*` options.
- **Also reached by the removal:** `LoadMode` and the unload path in `parakeet.rs`, which only
  exist for two engines; the dispatch and tests in `transcription.rs`; readiness in
  `providers.rs`, and its catalogue test; `speech_not_ready`, `local_speech`, `speech_guard`,
  `preload_speech` and Local-only messages in `session.rs`; the environment and assertion in
  `nix/hm-module.nix`; `examples/accuracy_replay.rs`; and several README places: requirements,
  Local-only, the provider table, Fully offline, the Norwegian section and the generated options.
- **Changed:** the picker, which sits in Settings (`shell/Overlay.qml:590`), not the meeting, and offers
  only Auto, English and Norwegian today. `meetings.language` is read back into `MeetingRecord`
  and always written as `"en"` (`src/daemon/session.rs:2776`); nothing found uses it for speech or
  answers, so it can hold the choice if it needs storing.
- **p620:** `hosts/common/nixos/omarchy-meetingai.nix` in nixos sets `whisperNb.enable`, so it must
  come off in step with the bump, or that build fails. p620 is the only host importing it.
- **Other issues:** #74 (NB-Whisper delay) would have no subject. #73's plan measures NB-Whisper on
  Norwegian, so that pair would drop. #71's Norwegian trigger work shrinks, while Polish and others
  remain. #70 is closed as done, and this reverses its result, so it needs a note.

## Constraints

- **Saved settings must keep working.** A saved `speech_language` of `nb`, `no` or `nn` would otherwise
  refuse every recording and dictation once routing is gone, and a saved `whisper-nb-local` would have no
  engine. Both need a one-time migration with a notice, like `whisper-local` (`migrate_whisper`,
  `src/daemon/session.rs:949`), but with their own marker, since `whisper_notice_shown` is already set
  on existing databases.
- **Removed Nix options fail with a message saying why,** as the `whisper.*` options do today.
- **Local-only stays intact.** No choice here may send audio off the machine without the user having
  chosen a cloud engine. Choosing "Norwegian" must never quietly route to the cloud.
- **#14's guarantees hold:** no new bleed or invented lines. Removing an engine should not affect them.
- **A language can't change audio already transcribed.** Speech settings are read once at recording start;
  a change during recording applies at the next start (`settings_set`, `src/daemon/session.rs:4015`).
- **Two sources of truth are not acceptable.** `response_language` already sets the answer language,
  and it is stored per conversation and used for live answers, rolling notes and recaps. If the
  meeting's language affects answers, which one wins must be stated.
- **Standalone dictation and resumed meetings need a defined language,** since neither starts a new meeting.
- **Old transcripts and exports are untouched.**
- **No measurements are needed** unless the owner picks an option that changes an engine.

## Open questions

1. **What does "everything else" cover?** The title and outcome assume this answer, and change with it.
   Recommended: the local Whisper engines only (NB-Whisper, turbo, the export tools). Keep Deepgram and
   OpenAI-compatible servers. They are already opt-in, have tests, p620 already has a Deepgram key, and
   Deepgram's Nova-3 accepts Norwegian and separates speakers, so they are the only remaining route to both.
   The alternative, Parakeet alone, is simplest and removes more code, but makes Norwegian calls
   impossible and loses speaker separation.
2. **What should the meeting's language change, given Parakeet ignores it?** This goes beyond what the owner
   literally asked, and it is the owner's call:
   - **A (recommended): answers and notes for that meeting, plus the refusal.** A chosen language overrides
     `response_language` for that meeting's answers, notes and recap; nothing chosen leaves
     `response_language` in charge. It does something real on Parakeet and settles #71's wrong-language
     answers, without detection. It costs some extra code.
   - **B: only engines that accept a language, plus the refusal.** Smallest, and most literal. On Parakeet it
     would do almost nothing, so its label has to say Parakeet detects by itself.
   - **C: no per-meeting control yet.** Only remove the engine, and leave the Settings picker with honest
     wording, then design the control with the "other ways" of improving this.
3. **When can it be chosen?** Recommended: before recording starts, in the meeting. This fits how speech
   settings work today. Under 2A, answers can follow a later change from the next answer on. Restarting
   transcription mid-recording is more work, and has no effect on Parakeet.
4. **Which languages are offered?** Recommended: English plus what the engine in use can do, which is
   Parakeet's 25 on Parakeet. Norwegian then appears only with an engine that can do it, and otherwise a
   hint says Norwegian needs Deepgram. Nothing switches to the cloud by itself.
5. **Remove or switch off?** Recommended: remove, as the owner asked. Git history and the release asset keep it
   recoverable. Switching it off in nixos alone is one line, but simplifies nothing else, so it only suits
   keeping Norwegian an easy option for a while.
6. **Keep #70's measurement?** Recommended: keep `docs/eval/2026-11-norwegian-stt.md`, with a note that the engine
   was later removed, since it is the evidence for any later attempt.
