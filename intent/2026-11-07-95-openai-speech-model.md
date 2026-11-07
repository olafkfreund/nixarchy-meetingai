---
status: draft
issue: 95
author: olafkfreund
---

# Intent: The OpenAI speech preset keeps working after whisper-1 shuts down

## Problem

The OpenAI speech preset uses `whisper-1` (`src/ai/catalog/presets.rs:90`). OpenAI says `whisper-1`, `gpt-4o-transcribe`, `gpt-4o-mini-transcribe` and `gpt-4o-transcribe-diarize` "will shut down on February 26, 2027", and recommends `gpt-transcribe` (https://developers.openai.com/api/docs/guides/speech-to-text, read 2026-11-05). After that date, anyone using the preset gets no transcript.

A preset is a template: choosing it saves a provider record with the model copied in (`Preset::draft`, `src/ai/catalog/presets.rs:114`). So changing the preset fixes new records only. A record someone already saved keeps `whisper-1`.

The shell's test daemon copies the preset too (`shell/tests/fake-daemon.py:191`).

**Today:** p620 uses `parakeet-local` with no OpenAI speech record, so it's unaffected.

## Proposed outcome

- New OpenAI speech records use a model OpenAI still supports past February 26, 2027.
- A record saved with `whisper-1` either keeps working after that date or says clearly why it stopped and what to change. Which one is for the spec.

## Affected users and systems

Anyone using the OpenAI speech preset. `src/ai/catalog/presets.rs`, possibly where saved records are loaded, `shell/tests/fake-daemon.py`, the README if it names the model.

## Constraints

- No OpenAI calls and no key without the owner's approval. So it can't be proven here that `gpt-transcribe` accepts the app's request (multipart WAV, a `prompt` from the profile's glossary, `language`, the `text` response). The spec says how that's handled and the PR says it wasn't tested live.
- Don't rewrite a user's saved record without saying so.
- Prices on OpenAI's page: `gpt-transcribe` $0.0045/min, Whisper $0.006/min.

## Open questions

1. **Change saved `whisper-1` records automatically, or only tell the user?** I'd change them when loaded, show a one-time notice, and leave records with any other model alone.
2. **Test with a real key?** I'd leave it untested and note that in the PR, unless you want to supply a key for one call on public test audio.
3. **Do it now or nearer the date?** I'd do it now: it's small, and February is close.
