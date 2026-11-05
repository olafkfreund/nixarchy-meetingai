---
status: approved
issue: 78
intent: intent/2026-11-05-78-online-speech.md
---

# Spec: Choose a low-cost online engine for languages Parakeet lacks

## Design

The result is a sourced comparison and a recommendation in `docs/eval/2026-11-online-speech.md`, with no new protocol code. It has two parts, with a stop for the owner between them.

**Part 1, free, no accounts:** finish the desk comparison, and check that a self-run NB-Whisper server works with the app.

**Part 2, only with the owner's approval:** measure the shortlist on the same 20 NPSC calls as #70 (21 minutes, through the call pipeline), against NB-Whisper's 23.9% and Parakeet's 85.1%. Only NPSC audio goes to a provider, and the tool refuses anything else. It needs sign-ups, so it's named here and not started.

### What we know so far

All from the provider's own page, fetched 2026-11-05, unless marked. Most pages have no date.

| Engine | Norwegian | Price per audio hour | Minimum billed | Works with the app | Where audio goes |
|---|---|---|---|---|---|
| **Deepgram Nova-3** | Listed, `no` | Streaming $0.46, batch $0.26 (monolingual) | None; per second | Yes, built in | US, or EU with `deepgram.region = "eu"`. See below. |
| **Self-run NB-Whisper** | Built for it | Free | n/a | A candidate: Part 1 must show a server that loads the model and accepts the app's request | This machine |
| **Groq Whisper turbo** | Whisper's list; not named on the page | $0.04 | **10 s per request** | Yes, existing preset | US; terms to read |
| **OpenAI** | `whisper-1` by reference; not listed for newer models | Third-party figures only | Unknown | Yes, existing preset | Terms to read |
| **Soniox** | Listed, `no` | Async $0.10, real-time $0.12 | Not stated | Not confirmed; may need a new client | Terms to read |

ElevenLabs, Mistral, Speechmatics, AssemblyAI, Google and Azure aren't OpenAI-compatible as far as found. The app has configurable JSON, multipart and raw WAV requests, so whether each needs code depends on its API, which wasn't checked. None is cheaper than Soniox on the prices found, so each stays in the document's table, not on the shortlist. Hugging Face has no hosted NB-Whisper.

**Cost for 20 hours of calls a month,** assuming each of the 2 channels is billed for the whole call, so 40 billable hours. That fits Deepgram streaming, which sends both channels throughout (`src/speech/stt/transcription.rs:821`). Batch engines are sent only chunks with speech, so for them it's an upper limit, before any minimum per request.

| Engine | Monthly |
|---|---|
| Deepgram streaming | 40 x $0.462 = **$18.48** |
| Deepgram batch | 40 x $0.258 = $10.32 |
| Groq turbo | At most 40 x $0.04 = $1.60, or about **$4.00** if every chunk is about 4 s and billed as 10 s |
| Soniox real-time | 40 x $0.12 = $4.80, plus a client to write and maintain |
| Self-run NB-Whisper | $0 per minute, and it uses the machine during calls |

**Deepgram's $200 credit:** its pricing page says new accounts get $200, with no card and no expiry. At $18.48 a month that's about 10 months. Whether a credit runs out is for the owner to weigh against "almost free".

### What Part 1 must settle

- **Deepgram's handling of audio.** Its model improvement program is voluntary, but its page doesn't say whether a pay-as-you-go account is in by default, how long stored data is kept, or whether opting out changes the price. Opted-out requests are kept "only for the duration necessary". **The app sends `mip_opt_out=true` only on the EU endpoint** (`src/speech/stt/transcription.rs:384`, `:825`), so a US setup doesn't opt out. Part 1 reads the terms, and the recommendation says plainly what to set. The EU setting already gives the opt-out with no code.
- **Groq's limits.** The research found 20 requests a minute and 7,200 audio seconds an hour on the base plan, unconfirmed by me. Two channels at a chunk every 4 s is 30 requests a minute, over that limit. Part 1 confirms the figures and the app's real chunk rate, since if the chunks are longer, the limit and the 10 s minimum may not matter.
- **Terms** for every shortlisted engine: retention, training, opt-out and region, from the provider's own terms, not summaries.
- **A self-run server.** Does a maintained server (speaches, or whisper.cpp's) accept the app's multipart request at `/v1/audio/transcriptions` and the language it sends, and is there a speech preset or setting that reaches it? Is a CTranslate2 or ggml NB-Whisper available that gives the same quality? #70 used its own ONNX export, and the model card lists a ggml conversion. Can it keep up with two channels on p620? This runs on natural load, with public audio.
- **Accuracy claims.** No independent Norwegian comparison of these engines was found. Vendor claims are reported as vendor claims, or left out.

### Part 2, when approved

- **Shortlist:** Deepgram Nova-3 and Groq Whisper turbo, both on free credit or a free tier if possible, plus the self-run server. OpenAI is added only if Part 1 confirms Norwegian on a current model. Soniox is measured only if the others all miss, since it needs code.
- **Cost limit:** $5 in total. The set is 21 minutes on 2 channels, so 42 billable minutes per engine: about $0.32 on Deepgram streaming, and far less on Groq.
- **Through the real path:** the app's own engine code, so that chunking, language and errors are what a real call would see. The replay tool can't do this yet (it has only `--engine parakeet`, `--url` and `--fake`). It gains what it needs, with keys only in the environment for that run. It's test code; the app itself doesn't change.
- **Public audio only:** before any network request, the tool refuses a file that isn't in the NPSC set's manifest, and a self-check proves it.
- **Comparing with #70:** the same 20 calls, references, filter and scoring. #70 ran NB-Whisper in-process with Silero at `--speed 8`, and a live cloud engine runs at real speed through different chunking. So 23.9% is a close comparison, not an exact one, and the result says so. The self-run server gets the same treatment as the cloud engines.
- **Reported per engine:** word errors, delay, failures or rate-limit errors, real cost, and where audio went.

### The recommendation

It names one engine for Norwegian and says what to set. It's based on word errors first (is it at or better than NB-Whisper's 23.9%?), then monthly cost at 20 hours, then where audio goes. Whether work calls may leave the machine stays the owner's call, so it also names the best option that keeps audio local.

## Alternatives rejected

- **Choosing from desk research alone.** No independent Norwegian numbers exist.
- **Adding a new client now.** It costs code to maintain, before anything shows it's needed.
- **Bringing NB-Whisper back into the app.** #77 removed it on purpose. A separate server keeps audio local with no Whisper code in the app.
- **Hugging Face hosted NB-Whisper.** No provider offers it.
- **Sending the opt-out everywhere now.** It may be right, but the price effect isn't known. Until Part 1 answers, anyone using Deepgram for real calls should set `deepgram.region = "eu"`, which already opts out. A code change, if any, would be its own small issue.

## Risks

- **Prices and terms change.** The document dates every fact, and the owner should re-check before relying on them.
- **21 minutes is a small sample,** and parliamentary speech isn't a call. It compares directly with #70's numbers, with the same limits.
- **Free tiers may have training or retention terms** that differ from paid ones. Part 1 checks; only public audio is sent either way.
- **Keys.** Any key is the owner's, kept out of the repo and logs. No sign-up is made on the owner's behalf without approval.

## Verification

- Every number in the document has a source URL and date, and third-party figures are labelled.
- Part 1 ends with a stop: the owner sees the document and decides on Part 2, including a monthly limit.
- If Part 2 runs: a self-check on the replay additions including the public-audio refusal, the same scoring as #70 (`tools/accuracy/score.py`), and a recorded cost against the $5 limit.
- `cargo test --workspace`, clippy and `cargo fmt --check` if any example code changes.
