# Question marks and answer language on non-English calls (#71)

Measured on p620, 2026-11-05.

## How often remote questions carry a `?`

From the owner's 36 most recent meetings visible to agents, so **Local-only interviews are excluded**: 3,320 remote lines of two or more words, counted locally. Numbers only; no text left the machine.

| | Lines | With a `?` |
|---|---|---|
| All remote lines | 3,320 | 399 (12%) |
| Starting with an English opener | 133 | 61 (46%) |
| Starting with a Polish opener | 0 | — |

- **46% is a floor, not a rate.** Lines opening with "when" or "how" are not all questions, so the true share on real questions is higher, by an unknown amount.
- English does not depend on the `?`, as `is_question` also takes these openers.
- **Polish barely appears:** one line has a Polish letter, and none opens with a Polish question word. So Polish detection cannot be judged from this history.

**Under the approved rule:** too little Polish means S1 ships, #71 closes, and question detection gets a new issue only when seen failing.

## Does the app's own request get the right language?

"What should I say?" with made-up transcripts, five questions or statements per language, using a simplified version of the real prompt. The language was judged by a simple check for Polish letters and words.

| Model | Polish transcript, no rule | Polish transcript, with the rule | English transcript, either way |
|---|---|---|---|
| Claude Sonnet (the owner's usual) | 5 of 5 Polish | 5 of 5 Polish | 0 of 5 Polish |
| `qwen2.5:3b` (small, local) | 0 of 5 Polish | 1 of 5 Polish | 0 of 5 Polish |
| `gemma4:e4b` | invalid: empty replies | invalid: empty replies | invalid |

## Reading

- **The bug is model-dependent:** it does not appear on Sonnet, and does on the small local model.
- **The rule helps only a little there,** and does no harm in these runs: English stayed English.
- **Choosing a language in the meeting (#77) is the stronger fix,** as it says "Write in Polish" outright.

## Limits

Ten made-up transcripts, a simplified prompt, and one person's meetings. This is a rough signal, not a measurement.
