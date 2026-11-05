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

- A fresh install offers one local engine, Parakeet, and no local Whisper engine: no model
  in the closure, no `whisperNb` options, no routing to it.
- The language shows English by default, and can be changed for one meeting from the
  meeting itself, without going to Settings and without changing the next meeting.
- A language the engine in use cannot do is refused with a clear reason, never written as a
  neighbouring language. On Parakeet, that includes Norwegian.
- English calls work as they do today, at today's speed.

What the chosen language does on Parakeet is open question 2.

## Affected users and systems

- **Removed:** `src/speech/stt/whisper_nb.rs`, `whisper-nb-local` in
  `src/ai/catalog/providers.rs`, routing in `resolve_speech`
  (`src/daemon/session.rs:663`), `nix/whisper-nb-model.nix`, `nix/silero-vad.nix`,
  `nix/whisper-turbo-model.nix`, `tools/whisper-export/`, the `whisperNb.*` options, and the
  README's Norwegian section.
- **Changed:** the picker (`shell/Overlay.qml`), and the language's storage. `meetings.language`
  exists but is always written as `"en"` (`src/daemon/session.rs:2776`), and nothing found so far
  uses it.
- **p620:** `hosts/common/nixos/omarchy-meetingai.nix` in nixos sets `whisperNb.enable`, so it must
  come off in step with the bump, or that build fails. p620 is the only host importing it.
- **Other issues:** #74 (NB-Whisper delay) would have no subject. #73's plan measures NB-Whisper on
  Norwegian, so that pair would drop. #71's Norwegian trigger work shrinks, while Polish and others
  remain. #70 is closed as done, and this reverses its result, so it needs a note.

## Constraints

- **Saved settings must keep working.** A saved `whisper-nb-local` or `nb` must not break the daemon.
  There is a pattern for this: `whisper-local` became `parakeet-local` once, with a notice.
- **Removed Nix options fail with a message saying why,** as the `whisper.*` options do today.
- **Local-only stays intact.** No choice here may send audio off the machine without the user having
  chosen a cloud engine. Choosing "Norwegian" must never quietly route to the cloud.
- **#14's guarantees hold:** no new bleed or invented lines. Removing an engine should not affect them.
- **A meeting's language can't change the audio already transcribed.** A mid-meeting change applies
  from then on. Speech settings today wait for the next start
  (`speech_setting_change_waits_for_next_start`), so the spec must settle this.
- **Old transcripts and exports are untouched.**
- **No measurements are needed** unless the owner picks an option that changes an engine.

## Open questions

1. **What does "everything else" cover?** Recommended: the local Whisper engines only (NB-Whisper,
   turbo, the export tools). Keep Deepgram and OpenAI-compatible servers. They are already opt-in,
   have tests, p620 already has a Deepgram key, and Deepgram's Nova-3 accepts Norwegian, so they are
   the only remaining route to Norwegian. The alternative, Parakeet alone, is simplest and removes
   more code, but makes Norwegian calls impossible, and removes cloud speaker separation too.
2. **What should the language do on Parakeet, given it cannot steer the engine?** Recommended: use it
   for what it can affect. It sets the language answers are written in during that meeting, and it
   refuses a language Parakeet cannot do. This also settles #71's wrong-language answers without
   detection. The alternative is a language control that visibly does nothing on Parakeet, which
   would be misleading.
3. **What happens when someone picks Norwegian?** Recommended: on Parakeet, refuse with a reason that
   names the option ("Parakeet can't transcribe Norwegian. Choose Deepgram in Settings."). The user
   switches on purpose, which keeps Local-only and privacy intact. The alternative of leaving Norwegian
   out of the list hides the reason from the user.
4. **Remove or switch off?** Recommended: remove, as the owner asked. Git history and the release asset
   keep it recoverable, and it is days old. Switching it off in nixos alone (one line) would keep every
   option open at no cost, so it is the cheap alternative if Norwegian might be needed again soon.
5. **Keep #70's measurement?** Recommended: keep `docs/eval/2026-11-norwegian-stt.md`, with a note that
   the engine was later removed, since it is the evidence for any later attempt.
