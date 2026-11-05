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
  `parakeet.rs`, which exist only to swap engines. **`DECODE` stays:** Parakeet's preload and transcribe rely on it to
  decode one at a time (`parakeet.rs:113`).
- **Messages:** the "Norwegian Whisper" wording in `speech_not_ready`, `speech_guard` and the Local-only errors goes.
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
fresh install is marked silently. It runs after Nix settings are applied, and skips keys Nix locks: a locked
`settings.speechLanguage = "nb"` on Parakeet gets the normal refusal at recording start, which says what to do.

### S4. The meeting's language

**State.** `Inner` gains `meeting_language: Option<String>`, a lowercase base code. `None` means nothing was chosen.

| Event | What happens to the language |
|---|---|
| `meeting.language.set {language}`, a new command, not offered to agents over MCP | set; `""` or `"en"` clears it; written to `meetings.language` at once if a meeting is open or was just finished, so each accepted change persists |
| A meeting starts | written to `meetings.language`, in place of the hardcoded `"en"` (`src/daemon/session.rs:2776`). No schema change |
| Stop and finish | **kept.** The finished meeting stays open for Play, follow-up questions and its recap |
| Play, restore, resume | read back from that meeting's row |
| New, with a finished or restored meeting open | cleared, in `reset_session`. Auto-recorded calls are New, so they start with nothing chosen |
| New, with no meeting open | kept, the same way a set-up interview keeps its candidate (`src/daemon/session.rs:2733`). So choose, then record, works |

**`record_start` works out the effective language after deciding New or Play.** It currently resolves speech before its New reset
(`src/daemon/session.rs:2600` against `:2743`), which would apply the previous meeting's language to a new call.

**What the meeting shows.** The chosen language if there is one. Otherwise the global `speech_language` if set, so a user with French set for
a cloud engine does not see a false "English". Otherwise "English". Choosing English clears the meeting's choice, so it means "no override",
and never forces `en` on an engine. With no global setting, a default meeting therefore behaves exactly as today on every engine.

**Transcription.** At recording start, the effective language is the meeting's, if chosen, otherwise the global `speech_language`, as today. It
goes through `resolve_speech` and `speech_guard` as today:

- Parakeet ignores the value, but the guard still refuses a language it cannot do;
- Deepgram and OpenAI-compatible servers receive it.

Dictation during a meeting already reuses the recording's engine. **Standalone dictation has no meeting**, so it keeps using the global setting.

**Answers and notes.** A chosen language is an override at prompt time, and nothing else changes:

- **Prompts** (`system_prompt`, `src/daemon/session.rs:1653`, and the follow-ups that use it): the meeting's language if chosen, otherwise the
  conversation's latched language, as today. `conversation_response` is **not** changed, so clearing the choice restores it, and other saved
  conversations in the meeting keep theirs.
- **Rolling notes and recaps** (`:2526`, `:3202`): resolved by the ID of the meeting being summarised, from its row, so a regenerated older
  meeting uses its own language. Otherwise `response_language`.
- **Name, not code.** `response_instruction` is written for names ("Write in Norwegian"), so the helper passes the name from the table below.
- **Precedence is stated in the picker's hint**, so nobody wonders why `response_language` seems ignored.
- **#71.** This resolves its wrong-language answers when a language is chosen. With none chosen, behaviour stays as today, and #71 keeps that case and the
  question triggers.

**Languages offered.** `speech_guard`'s engine lists move into one function, `language_support(engine, model, code)` returning supported,
unsupported or unknown, used by the guard and the picker. `state.get` adds `languages: [{code, name}]` for the current engine, from one code-to-name table:
Parakeet gets its 25; Deepgram Nova-3 gets its list, including Norwegian; other engines get the whole table, and the existing "cannot be checked" notice
still applies, so an offered language there is not a verified one.

**Local-only.** `state.get`'s redaction for agents (`src/daemon/session.rs:5295`) also nulls `meeting_language`, as it does `meeting` and `candidate`. The list of
languages is about the engine, so it stays. Nothing here changes which engine a Local-only meeting may use.

**The shell.** A compact language `Select` sits in the meeting view, near the mode picker, with a one-line hint:

- on Parakeet: "Parakeet detects the language itself. This sets the language of answers and notes. Norwegian needs Deepgram.";
- on an engine that accepts a language, during recording: speech changes apply at the next start, using the existing notice's wording. On Parakeet there is no
  such notice, since speech does not change.

The Settings picker and hint added in #70 are removed. **`speech_language` stays a setting** for Nix and for cloud-engine users, without its own Settings
control, and `settings.search` still finds it.

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
- **English cannot override a global language.** With `speech_language` set to French on a cloud engine, choosing English means "no override", so French is
  still sent. Forcing English there is rare, and is done in the global setting.
- **The meeting view is tight on space,** and a new control could crowd it. The shell's geometry check runs in CI.

## Verification

- **Rust:** `resolve_speech` no longer returns `whisper-nb-local`; the migration table, including a Nix-locked key being skipped; every row of the state
  table, including a New call never using the previous meeting's language, and choose-then-record keeping the choice; a change during a meeting surviving
  restore; `en` and `""` clearing it; a language refused on Parakeet; Deepgram receiving the chosen code; prompts using the name; clearing restoring the
  conversation's own language; a recap of an older meeting using that meeting's language; Local-only redaction; and a default meeting producing today's
  prompt and engine config.
- **Shell:** `shell/tests/ci.sh`, with the smoke test choosing a language in the meeting view, the daemon logging `meeting.language.set`, the hint showing, and
  Norwegian absent on Parakeet.
- **`nix flake check`,** including `docs-match`, and an evaluation where a removed `whisperNb` option fails with its message.
- **Search:** no `whisper_nb`, `whisper-nb-local`, `whisperNb` or `LoadMode` left, outside the migration, its test, the removed-option entries and `docs/eval/`.
- **p620 after deploy:** the daemon runs, the closure no longer holds the NB-Whisper model, an English call works, and picking Polish gives Polish
  answers. The owner runs those last two.
