---
status: draft
issue: 78
author: olafkfreund
---

# Intent: Choose a low-cost online engine for languages Parakeet lacks

## Problem

#77 made Parakeet the only local engine. It has no Norwegian: a Norwegian call comes out as Swedish-looking text at 85% word errors. The only route left is Deepgram, in the cloud, which the owner asked to look past: "the best online (and almost free) engine for other languages" (2026-11-05). Nobody has compared engines, and no online engine has been measured on these languages.

Some things are already known, all unverified or partial:
- OpenAI-compatible hosted engines need no new code (`openai-compatible-stt`), so trying several is cheap.
- Third-party pages suggest Deepgram Nova-3 costs about $0.35–0.67 per hour, perhaps double if billed per channel.
- The app only sends Deepgram's model-improvement opt-out when `deepgram.region = "eu"`.

## Proposed outcome

- A short, sourced comparison, and a recommendation the owner can act on: which engine to use for Norwegian, what it costs for a typical month, and where the audio goes.
- Accuracy measured on the same NPSC Norwegian calls as #70, so it compares directly with the removed NB-Whisper's 23.9% and Parakeet's 85.1%.
- Using the winner takes a setting, ideally with no code.

## Affected users and systems

Anyone with calls Parakeet can't do. Docs and `docs/eval/` first. Code only if the best engine needs a new protocol.

## Constraints

- **Privacy first.** Only public test audio (NPSC) goes to any provider during evaluation, never the owner's meetings. Provider terms on retention and training are read, not assumed, and each result says where audio goes.
- **Nothing goes to the cloud by itself.** Engines stay a deliberate choice, refused in Local-only.
- **The owner approves spending and accounts.** No sign-ups or paid calls without it, and a measured run has a stated cost limit.
- **Work calls may not be allowed to leave the machine.** This is for the owner to judge, and a cloud result does not settle it.

## Open questions

1. **What does "almost free" mean?** Recommended: name a monthly limit, for example for 20 hours of calls. Pay-per-minute prices only matter against this.
2. **Which providers?** Recommended: start from hosted Whisper-style APIs and Deepgram, checking Norwegian support and terms first; the spec names the shortlist from current sources.
3. **Should this happen now?** The owner said this was for later. Recommended: finish the desk research now, which costs nothing, then stop before any paid test.
