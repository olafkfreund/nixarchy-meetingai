---
status: draft
issue: 78
spec: spec/2026-11-05-78-online-speech.md
coders:
  1: coder   # writes the document from the sources Claude reads; Claude keeps the web reading and judgement
reviewers:
  - codex    # read-only: claims against the sources and code
  - agy      # read-only: the same, a second view
---

# Plan: Choose a low-cost online engine for languages Parakeet lacks

This plan covers **Part 1 only: free, no accounts, no provider contacted with audio.** It ends with a stop. Part 2 needs the owner's approval and its own plan update.

## Decisions carried from the approved spec

- The result is `docs/eval/2026-11-online-speech.md`: a sourced comparison and, after Part 1, a provisional recommendation. No app code changes.
- No sign-ups and no paid calls. Only public audio is used, and in Part 1 it stays on this machine.
- Costs are compared at 20 hours of calls a month, with 2 channels billed for a whole call as an upper limit for batch engines.
- Part 1 settles: Deepgram's handling of audio, Groq's limits against the app's real chunk rate, the terms of each shortlisted engine, whether a self-run NB-Whisper server works with the app, and which accuracy claims are only vendor claims.
- Shortlist for Part 2: Deepgram Nova-3, Groq Whisper turbo, the self-run server, with OpenAI only if Norwegian is confirmed on a current model, and Soniox only if the others miss, since it may need code. Part 2's limit is $5, with a tool that refuses non-NPSC audio.
- The recommendation weighs word errors first, then monthly cost at 20 hours, then where audio goes, and names the best option that keeps audio local. Whether work calls may leave the machine stays the owner's call.

**New since the spec:** NB-Whisper's model card lists NPSC among its training data. So its 23.9% and the self-run server's result may flatter it on this set. The document says so, and it adds this to the comparison's limits.

## Traps

- **Only public NPSC audio,** from `~/.cache/meetingai-eval/no/sets-npsc20/`, and in Part 1 only to a server on `127.0.0.1`.
- No keys, sign-ups or provider API calls. Reading public web pages is all that's allowed.
- Date every fact and link its source. Label third-party figures, and report vendor claims as vendor claims.
- **No artificial CPU load on p620.** Check the agent bus and other sessions' CI first, and run one thing at a time.
- Downloaded models and tools are untrusted: keep each in its own new directory under `~/.cache/meetingai-eval/`, and don't run anything from inside a download. Never `torch.load(weights_only=False)`.
- Bind the server to `127.0.0.1`, and stop it when done.
- No app code changes, so no bump or deploy.

## Steps

### 1. The document (Claude reads, coder writes) — `docs/eval/2026-11-online-speech.md`

Claude reads the primary sources and hands the coder notes with URLs and dates. The coder writes the document from those notes, and adds nothing else.

1. **Deepgram:** terms and privacy pages: default for pay-as-you-go in the improvement program, retention, whether opting out changes the price, and EU endpoint pricing.
2. **Groq:** rate limits page and data page, and the app's real chunk rate, from saved `chunk` records in the NPSC replay runs under `~/.cache/meetingai-eval/no/results/`: chunks a minute and their length. Compare with the 20 requests a minute and 7,200 audio seconds an hour, and work out the real cost with the 10 s minimum.
3. **OpenAI:** current status of `whisper-1`, `gpt-4o-transcribe` and `gpt-transcribe`, Norwegian, prices if readable, and data terms.
4. **Soniox:** terms, and its API's shape, enough to say whether it needs code.
5. The rest of the table from the spec, dated, with what wasn't confirmed marked as such.

### 2. Self-run server (Claude)

1. `nix shell nixpkgs#whisper-cpp`: confirm it has a server, and that it serves `/v1/audio/transcriptions`. If not, try `speaches`, kept in its own directory, and record what it took.
2. A ggml `nb-whisper-small` from `NbAiLab` on Hugging Face, the size #70 used, if one is published. If it isn't, record the conversion needed, and don't run it without asking: this is the point to stop if it's more than a download.
3. Run it on `127.0.0.1`, and one NPSC call through `accuracy_replay --url` at 1x, using the model name and language the app would send.
4. If that works, the 20 calls once, scored with `tools/accuracy/score.py`, at natural load, recording word errors, delay, and CPU use while it runs. It's a free number, but its setup differs from #70's (a different export and no Silero), so report it beside 23.9%, without replacing it.

### 3. Finish Part 1 (Claude)

Add step 2's result to the document, and a provisional recommendation. Read-only reviews by Codex and agy, and a fresh Opus agent on the document against its sources. A PR with the document only, saying the coder wrote step 1, not closing #78. **Then stop:** bring the owner the result, and ask for a monthly limit and whether to run Part 2.

## Tests

No code changes. The document has a source and date on each number, and its cost arithmetic is recomputed when reviewed. `nix flake check` passes.

## Rollback

Revert the PR. Remove the downloaded server and model from the cache.

## Deviations

(Recorded here during implementation, in the same commit as the code.)
