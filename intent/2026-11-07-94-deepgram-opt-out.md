---
status: draft
issue: 94
author: olafkfreund
---

# Intent: Deepgram never keeps call audio to improve its models

## Problem

Deepgram keeps request audio for its Model Improvement Program unless each request opts out. Its own page (https://developers.deepgram.com/trust-security/your-data, read 2026-11-05) says request audio is "Retained to improve our models" by default, and with `mip_opt_out=true` it is "Retained only for the duration needed to process the request". Its terms (section 3.2, updated August 6, 2026) let it use content for "training and testing our Models".

The app sends `mip_opt_out=true` only on the EU endpoint:

- batch: `deepgram_batch_url`, `src/speech/stt/transcription.rs:384`, only when the host is `api.eu.deepgram.com`;
- live: `live_url`, `src/speech/stt/transcription.rs:825`, only when `eu`.

`deepgram_region` defaults to `us` (`src/core/models/settings.rs:259`). So a user who picks Deepgram and doesn't change the region sends their calls, including other people's voices, without the opt-out, and by Deepgram's terms they may be kept to improve its models. The README tells them to choose `eu` to avoid it (`README.md:184`, `:202`).

This came from #14's spec, which paired the opt-out with the EU endpoint for in-region handling (`spec/2026-10-28-14-transcript-accuracy.md:100`). Nothing there decided that US requests should be kept.

**Today:** p620 uses `parakeet-local`, so nothing goes to Deepgram. Its `deepgram_region` is `us`.

## Proposed outcome

- Every request the built-in Deepgram engine (`deepgram-stt`) sends, batch or live, on either endpoint, carries `mip_opt_out=true`.
- The EU setting still chooses where audio is processed, and nothing else.
- The README no longer says only EU opts out.

## Affected users and systems

Anyone using `deepgram-stt`. `src/speech/stt/transcription.rs`, its tests, and `README.md` (including the generated option text for `deepgram.region`, if it mentions the opt-out). No setting, database or shell change.

## Constraints

- No Deepgram call is made to check this, and no sign-up: the tests check the URLs the app builds.
- A test must fail without the change, for both batch and live on the US endpoint.
- **Custom speech records are out of scope.** A custom record sends to the URL its owner typed, through `transcribe_custom` and its own `stream_url` (`src/speech/stt/transcription.rs:169`, `:865`), not through the two functions above, and there's no Deepgram preset. The README says such a record sends what its owner configures, so to opt out they add `mip_opt_out=true` themselves.

## Open questions

- **Price.** Deepgram's page doesn't say whether opting out changes the price. The pricing page lists one price, with no mention of the program. I propose going ahead without asking Deepgram, and stating that in the PR.
