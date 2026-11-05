# Question marks and answer language on non-English calls (#71)

Measured on p620, 2026-11-05.

## How often remote questions carry a `?`

From the owner's 36 most recent meetings visible to agents, so **Local-only interviews are excluded**: 3,320 remote lines of two or more words, counted locally. Numbers only; no text left the machine.

| | Lines | With a `?` |
|---|---|---|
| All remote lines | 3,320 | 399 (12%) |
| Starting with an English opener | 133 | 61 (46%) |
| Starting with a Polish opener | 0 | — |

- **46% is rough.** Lines opening with "when" or "how" are not all questions, which pulls it down, and a `?` can belong to a later sentence on the line, which pushes it up.
- English does not depend on the `?`, as `is_question` also takes these openers.
- **Polish barely appears** in these meetings: one line has a Polish letter, and none opens with a Polish question word. Older meetings were not seen, as the list is capped at 40.

**Against the approved rule:** 46% is **below the 80% line**, which points to a separate spec for question detection. Polish has too little data to measure, which points to shipping S1 and opening an issue only when detection is seen failing. Both apply, and the owner decides. A missed question produces no error, so "seen failing" would be noticed only as an answer that never came.

## Does the app's own request get the right language?

"What should I say?" with made-up transcripts, five questions or statements per language, using a simplified version of the real prompt. The language was judged by a simple check for Polish letters and words.

| Model | Polish transcript, no rule | Polish transcript, with the rule | English transcript, either way |
|---|---|---|---|
| Claude Sonnet (the owner's usual) | 5 of 5 Polish | 5 of 5 Polish | 0 of 5 Polish |
| `qwen2.5:3b` (small, local) | 0 of 5 Polish | 2 of 5 Polish | not rerun: 0 of 5 Polish with an earlier form of the rule |
| `gemma4:e4b` (local) | 5 of 5 Polish | 4 of 5 Polish | not rerun |

The local models ran with the rule replacing the Auto sentence, as shipped. The Sonnet row used an earlier form, with the rule added beside that sentence. An earlier `gemma4:e4b` run with the same transcripts returned empty replies, and is discarded.

## Reading

- **On the owner's usual model the rule changed nothing:** 5 of 5 either way. It has no measured benefit there.
- **Only the smallest local model showed the problem,** going from 0 to 2 of 5 with the rule. On the other two, 5 to 5 and 5 to 4 are no different. So the benefit is small and unproven, on a handful of transcripts.
- English stayed English with the rule in the runs that tested it.
- **A chosen language was not tested.** Choosing one says "Write in Polish" outright, and should be stronger, but that is reasoning, not a result.

## Limits

Ten made-up transcripts, a simplified prompt sent directly through `claude -p` and Ollama, not the daemon, a crude language check, and one person's meetings. This is a rough signal, not a measurement.
