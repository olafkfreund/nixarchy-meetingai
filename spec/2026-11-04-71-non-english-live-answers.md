---
status: draft
issue: 71
intent: intent/2026-11-04-71-non-english-live-answers.md
---

# Spec: Live answers on calls that are not in English

**What changed since the intent:** #77 removed local Norwegian, and added a language chosen per meeting, which sets answers and notes when chosen. So this covers what remains: **answers when no language is chosen, and detecting spoken questions in other languages.** Norwegian now only matters on Deepgram. The intent's recommendations stand: measure before writing word lists, keep the button label in English, and leave the filler list for a separate cleanup.

## Design

### S1. The app's own requests answer in the call's language

Some requests are the app's own English text: "What should I say?", asking about a quoted line with nothing typed, a screenshot or attachment with nothing typed, and generated follow-ups. With Auto, "the language of the user's latest question" points at that text.

- **The origin is carried, not guessed.** Matching text cannot tell a button from the same words typed, and attached text chips change the string. So `chat.send` (`src/daemon/protocol.rs:57`) gains `app_request: true`, set by the shell where it supplies the text (`shell/Overlay.qml:234`, `:417`). `chat_send` sets it itself for its screenshot and attachment fallbacks.
- **`system_prompt`:** when `app_request` is set, no meeting language is chosen, the conversation's language is Auto, and there is transcript text, the instruction becomes: "This request is a button in the app, not the user's language. Reply in the language of the recent meeting speech."
- **Follow-ups:** `spawn_follow_ups` builds its own English request, so it gets the same line when the same conditions hold.
- **Anything typed is untouched,** including "What should I say?" typed by hand, which follows the normal rule.
- **No language detection is added;** the model judges from the transcript.

### S2. Question detection: find evidence first

`is_question` accepts a `?` in any language, on two or more words, or an English opener. The size of the gap is the engine's `?` rate on spoken questions, and that is unknown.

**The planned test set does not exist.** FLEURS' test sets hold one distinct question sentence in Polish, and one in English, checked by downloading them. The proposal instead:

- **English baseline, from the owner's own meetings, on this machine:** the share of final remote lines starting with an English opener that also contain a `?`. It runs locally, and reports counts only, with no text leaving the machine.
- **Polish:** counted the same way from the owner's own Polish meetings, if any. The owner says whether they exist. If there are too few, Polish waits until there are.
- This is a rough signal, not a measurement: it can only see lines opening with a known English opener, so it says how Parakeet punctuates, not how many questions are missed.

### S3. Decision rule

The owner sees the counts and decides. The proposal:

| What the counts show | Next |
|---|---|
| Spoken questions usually get a `?` (80% or more) | No trigger change. #71 closes with S1 |
| They usually don't | A separate spec, with a real labelled question set, for adding openers per language |
| Too little Polish to count | S1 ships and #71 closes; question detection goes to a new issue, opened when it is seen failing |

## Alternatives rejected

- **FLEURS as a question set:** it has almost no questions.
- **Matching the request's text:** cannot tell a button from typing, and breaks with attached text.
- **A language detection library,** a new dependency for what the model and a chosen language already cover.
- **Asking a model whether each line is a question:** it adds delay to the live path, and sends call text to a provider on every line, which the intent forbids.
- **Word lists for all 25 languages:** a lot of unverified grammar.
- **Translating the button:** a separate job.

## Risks

- **The model may still reply in English** despite the instruction. S1's check samples real models.
- **The counts are a rough signal,** from one person's meetings, and only see English-style openers.
- **Mixed calls:** "the language of the recent meeting speech" is ambiguous when people mix languages. Choosing a language in the meeting settles it.

## Verification

- **Rust:** `app_request` gets the rule; the same words typed do not; a chosen language or a set `response_language` wins over it; no transcript means no rule; a default call's prompt differs only for those requests; follow-ups get it too. `is_question` tests keep passing.
- **Shell:** the button and the empty quoted-line request send `app_request`, and a typed one does not.
- **Sample:** five Polish and five English transcripts, with "What should I say?" on the owner's usual chat provider, with results reported. It sends only those test sentences.
- **The counts** in S2, in `docs/eval/`, as numbers only.
