---
status: draft
issue: 77
intent: intent/2026-11-05-77-parakeet-only.md
---

# Spec: Parakeet as the only local engine, with a per-meeting language

**Decisions from the intent (owner approved its recommendations):**

- "Everything else" means the local Whisper engines. Deepgram and OpenAI-compatible servers stay.
- A chosen language sets that meeting's answers, notes and recap, and refuses what the engine
  cannot do (option 2A).
- It is chosen before recording; only languages the current engine can do are offered.
- The code is removed, not switched off. #70's measurements stay on file.

## Design

### S1. Remove the local Whisper engines

- **Delete:** `src/speech/stt/whisper_nb.rs`, `nix/whisper-nb-model.nix`, `nix/whisper-turbo-model.nix`,
  `nix/silero-vad.nix`, `tools/whisper-export/`, `examples/whisper_spike.rs`.
- **`whisper-nb-local`** leaves `STT_PROVIDERS` and joins the retired IDs its catalogue test checks.
- **The two-engine machinery goes:** `LoadMode`, `set_load_mode`, `unload_inactive`, and the unload paths in
  `parakeet.rs`, which exist only to swap engines. `DECODE` stays if Parakeet's own decoding needs it, which the
  plan confirms from the code.
- **`examples/accuracy_replay.rs`** loses `--engine whisper-nb|turbo`, `--model-dir`, `--vad` and `--load-mode`.
- **`resolve_speech`** no longer routes. It keeps normalising Norwegian codes to `no`, since Deepgram and
  OpenAI-compatible servers still receive them.
- **`tools/accuracy/`'s Norwegian set builder stays.** It runs no engine and costs nothing to keep.

### S2. Home Manager and p620

- `whisperNb.enable`, `.model` and `.vad` become `lib.mkRemovedOptionModule`, with a message saying Parakeet has no
  Norwegian and that Deepgram can do it. The environment lines go.
- The assertion returns to its earlier form, and also rejects `settings.speechProvider = "whisper-nb-local"`.
- The README's generated options block is regenerated, or `docs-match` fails.
- **p620:** one nixos PR both bumps the meetingai input and removes `whisperNb.enable`, because either alone fails
  or leaves a dead option. It lands only after the meetingai PR merges.

### S3. Saved settings

A new one-time migration, `migrate_norwegian_engine`, next to `migrate_whisper`, with its own marker:

| Saved | Becomes | Notice |
|---|---|---|
| `speech_provider = whisper-nb-local` | `parakeet-local` | yes |
| `speech_language` of `nb`, `no` or `nn` (any case or region), with the default local engine | empty | yes |
| `speech_language` of those, with Deepgram or an OpenAI-compatible engine | unchanged | no |

The notice says the local Norwegian engine was removed, Parakeet does not transcribe Norwegian, and Deepgram can. A
fresh install is marked silently. **A language locked by Nix cannot be migrated:** `settings.speechLanguage = "nb"`
on Parakeet gets the normal refusal at recording start, which says what to do.

### S4. The meeting's language

**State.** `Inner` gains `meeting_language: Option<String>`, a lowercase base code. `None` means nothing was chosen,
which is what "English" shows. It is:

- set by a new command `meeting.language.set {language}`, where `""` or `"en"` clears it;
- written to the existing `meetings.language` column when a meeting starts, in place of the hardcoded `"en"`
  (`src/daemon/session.rs:2776`). No schema change is needed;
- read back from that column when a meeting is restored or resumed;
- cleared by New and by finishing a meeting, so it never reaches the next one;
- reported in `state.get`, so the shell shows it.

**Choosing English explicitly is the same as choosing nothing.** So a default meeting behaves exactly as today, for
every engine, and `en` is never sent to an engine because of this control.

**Transcription.** At recording start the effective language is the meeting's, if chosen, otherwise the global
`speech_language`, as today. It goes through `resolve_speech` and `speech_guard` as today:

- Parakeet ignores the value, but the guard still refuses a language it cannot do;
- Deepgram and OpenAI-compatible servers receive it.

A change during recording applies at the next start, using the existing notice (`settings_set`,
`src/daemon/session.rs:4015`). Dictation during a meeting already reuses the recording's engine. **Standalone dictation
has no meeting**, so it keeps using the global setting.

**Answers and notes.** A single helper gives the answer language: the meeting's language if chosen, otherwise
`response_language`. It replaces the direct reads at `src/daemon/session.rs:1655`, `:1887`, `:2526` and `:3202`, so
answers, follow-ups, rolling notes and recaps all agree. `response_language` stays the setting for everything else.

- **The conversation's latched value.** `conversation_response` keeps the language from a conversation's first turn. Setting the
  meeting language also updates that value, so answers change from the next one on. Earlier answers are not rewritten.
- **Precedence is stated in the picker's hint**, so nobody wonders why `response_language` seems ignored.
- **#71.** This resolves its wrong-language answers when a language is chosen. With none chosen, behaviour stays as it is
  today, and #71 keeps that case and the question triggers.

**Languages offered.** `speech_guard`'s engine lists move into one function, `language_support(engine, model, code)`,
used by the guard and the picker. `state.get` adds `languages: [{code, name}]` for the current engine, from one code-to-name
table. Parakeet gets its 25; Deepgram Nova-3 gets its list, including Norwegian; other engines, whose support cannot be checked, get
the whole table.

**The shell.** A compact language `Select` sits in the meeting view, near the mode picker, with a one-line hint:

- on Parakeet: "Parakeet detects the language itself. This sets the language of answers and notes. Norwegian needs Deepgram.";
- while recording: speech changes apply at the next start.

The Settings picker and hint added in #70 are removed. **`speech_language` stays a setting** for Nix and for cloud-engine users, without
its own Settings control, and `settings.search` still finds it.

## Alternatives rejected

- **Set `speech_language` to `en` globally.** It would send `language=en` to Deepgram's batch path and OpenAI-compatible servers on
  every call, and change nothing on Parakeet.
- **Parakeet alone, removing cloud engines too.** The owner took the recommendation to keep them; it would remove the only route to
  Norwegian and to speaker separation.
- **Only switching off `whisperNb` in nixos.** Keeps every cost of the code. The owner chose removal.
- **Routing Norwegian to Deepgram automatically.** It would send audio to the cloud without the user choosing it.
- **Forcing a language on Parakeet.** No option for it is documented; this is unproven, and belongs with the later "other ways".
- **A new meeting-language table.** The column already exists and is unused.
- **Translating the transcript into the chosen language.** Not asked for.

## Risks

- **Norwegian is lost locally.** A Norwegian call on Parakeet without a chosen language still comes out as Swedish-looking text,
  which was #70's complaint. Nothing here detects it, because the control is only used if someone chooses. Choosing Norwegian on Parakeet is not
  offered. This is the owner's accepted trade-off, and the README says so.
- **Answers in a chosen language while the transcript is in another.** If Parakeet misdetects a call, answers are still in the chosen language.
- **Merge conflict with #73's branch,** which edits `accuracy_replay.rs` and removes nothing. #73's plan also measures NB-Whisper, and needs an
  amended plan and owner approval after this lands.
- **Removing `LoadMode` or `DECODE`** touches decoding locks, and the wrong removal could allow concurrent decoding. Existing tests and a
  check of Parakeet's own lock cover this.
- **`meetings.language` on older rows** is always `en`, which reads as nothing chosen: the correct result.
- **The meeting view is tight on space,** and a new control could crowd it. The shell's geometry check runs in CI.

## Verification

- **Rust:** `resolve_speech` no longer returns `whisper-nb-local`; the migration table above; set, start, restore, New and finish for
  `meeting_language`; `en` and `""` clear it; an effective language refused on Parakeet; Deepgram receives the chosen code; the answer-language helper
  in all four places; a default meeting produces today's prompt and engine config.
- **Shell:** `shell/tests/ci.sh`, with the smoke test choosing a language in the meeting view, the daemon logging `meeting.language.set`, the hint showing, and
  Norwegian absent on Parakeet.
- **`nix flake check`,** including `docs-match`, and an evaluation where a removed `whisperNb` option fails with its message.
- **Search:** no `whisper_nb`, `whisper-nb-local`, `whisperNb` or `LoadMode` left, outside the migration, its test, the removed-option entries and `docs/eval/`.
- **p620 after deploy:** the daemon runs, the closure no longer holds the NB-Whisper model, an English call works, and picking Polish gives Polish
  answers. The owner runs those last two.
