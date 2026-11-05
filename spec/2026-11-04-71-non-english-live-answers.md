---
status: draft
issue: 71
intent: intent/2026-11-04-71-non-english-live-answers.md
---

# Spec: Live answers on calls that are not in English

**What changed since the intent:** #77 removed local Norwegian, and added a language chosen per meeting, which sets answers and notes when chosen. So this covers what remains: **answers when no language is chosen, and detecting spoken questions in other languages.** Norwegian now only matters on Deepgram. The intent's recommendations stand: measure before writing word lists, keep the button label in English, and leave the filler list for a separate cleanup.

## Design

### S1. Fixed-text requests answer in the call's language

Some requests are the app's own English text: "What should I say?", asking about a quoted line with nothing typed, a screenshot or attachment with nothing typed, and generated follow-ups. With Auto, "the language of the user's latest question" points at that text.

- **`system_prompt`:** when no meeting language is chosen, the conversation's language is Auto, the meeting has transcript text, and the question is one of those fixed requests, the instruction becomes: "This request is a button in the app, not the user's language. Reply in the language of the recent meeting speech."
- **Follow-ups:** the same rule, for the questions generated after an answer.
- **A question the user really typed is untouched,** even if its text matches, as the rule keys on the same words: a typed "What should I say?" on a Polish call gets a Polish answer, which is what's wanted.
- **The fixed texts live in one list,** shared by the daemon. The shell's texts must match; a test checks them against the QML.
- **No language detection is added,** so the model does the judging from the transcript.

### S2. Measure question detection first

`is_question` accepts a `?` in any language, on two or more words, or an English opener. The size of the gap is the `?` rate, which is unknown.

- **Measure, on Parakeet, per language:** English and Polish, from FLEURS' test sets, whose raw text has question marks that label real questions. A small set of about 100 questions and 100 statements per language, built with `tools/accuracy/`. Measure the engine's `?` rate on questions, and on statements, through `transcribe`.
- **Report:** detection and false alarms for `is_question` as it stands.

### S3. Decision rule, set before the numbers exist

The owner sees the numbers and decides; the proposal is:

| Polish result | Next |
|---|---|
| Detection within 10 points of English, and false alarms at 5% or less | No trigger change. #71 closes with S1 |
| Lower | Add a Polish opener list, in the same shape as English, with its lead-in words, and measure again on the same sets. It stays only if detection comes within 10 points and false alarms stay at 5% or less, including on the English set |

Any other language is added only when the owner asks, by the same rule. Norwegian on Deepgram is not measured here, since nothing has been measured on Deepgram, and #78 covers that.

## Alternatives rejected

- **A language detection library,** which is a new dependency for what the model and a chosen language already cover.
- **Asking a model whether each line is a question:** it adds delay to the live path, and sends call text to a provider on every line, which the intent forbids.
- **Word lists for all 25 languages:** a lot of unverified grammar.
- **Translating the button:** a separate job.

## Risks

- **The model may still reply in English** despite the instruction. S1's check samples real models.
- **FLEURS is read speech,** not conversation, so real `?` rates may differ. This is stated in the result.
- **Mixed calls:** "the language of the recent meeting speech" is ambiguous when people mix languages. Choosing a language in the meeting settles it.
- **Openers can misfire:** Polish "co" or "jak" may begin statements, which is why false alarms are a bar.

## Verification

- **Rust:** fixed requests get the rule, a typed non-fixed question does not; a chosen language or a set `response_language` wins over it; no transcript means no rule; a default English call's prompt differs only for those requests. `is_question` tests keep passing.
- **Sample:** five Polish and five English calls' worth of transcript, with "What should I say?" on the owner's usual chat provider, with its results reported. Cost is minimal, and it sends only those test sentences.
- **The measurement** in S2, in `docs/eval/`.
