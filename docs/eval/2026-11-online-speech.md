# Online speech engines for Norwegian: Part 1 (#78)

This is Part 1, dated 2026-11-05. It cost nothing and sent no audio to any provider.
Prices and terms change. Most provider pages carry no date. Each fact below links its source and was fetched on 2026-11-05.
Part 1 is public pages, the app's code, and one self-run server on p620, measured on 2026-11-05.

## Result

**Self-run NB-Whisper is the only option measured so far. It got 16.7% word errors on 20 NPSC calls, under #70's bar of 20%.**
It is free per minute and the audio stays local. The app needs no Whisper code for it.
No cloud engine was measured. No independent Norwegian accuracy comparison of any cloud engine was found. Vendor claims are left out.

The recommendation is provisional. The owner decides. Whether work calls may leave the machine stays the owner's call.

1. **For Norwegian on a machine that can run it: self-run NB-Whisper.**
   Measured at 16.7% (under #70's 20% bar), free per minute, audio stays local, no Whisper code in the app. It is the only one measured so far.
   Limits: NPSC training overlap, not yet tried in the real app, not measured beyond p620, and it needs 16 threads there.
   It uses the machine during calls, and would have to be kept running.
2. **If an online engine is wanted:** Groq turbo is the cheapest at about $1.12 a month, and needs no code.
   Its Norwegian accuracy is unmeasured and Norwegian is not named on its page.
   Deepgram is the only one listing Norwegian and working with the app today. It is free for about 10 months on credit, then about $18 a month, with the EU setting.
   Part 2 measures these two. It may not be needed if the owner is happy with option 1.

## Comparison

| Engine | Norwegian | Price | Works with the app today | Where audio goes |
|---|---|---|---|---|
| Self-run NB-Whisper small | Measured: 16.7% word errors | $0 per minute | Yes, as `openai-compatible-stt` | Stays on the machine |
| Groq Whisper Large V3 Turbo | Not named on the page; accuracy unmeasured | $0.04/h, 10 s minimum per request | Yes, no code needed | US; logs up to 30 days, zero retention available |
| Deepgram Nova-3 | Listed (`no`) | Streaming $0.0077/min; pre-recorded $0.0043/min | Yes (streaming) | Retained by default; opt-out per request; EU endpoint |
| OpenAI `gpt-transcribe` | Not named; no language list | $0.0045/min | A preset exists, but uses the deprecated `whisper-1` (see below) | Not used for training by default |
| Soniox | Listed (`no`) | Async $0.10/h; real-time $0.12/h | Likely needs new code | Terms not read |

Sources are in the per-provider sections below.

## Cost

Twenty hours of calls a month. "Batch" means only chunks with speech are sent. The ratios come from the app's real chunk rate, measured below.

| Engine | Billed | Month |
|---|---|---|
| Self-run NB-Whisper | $0 per minute. Uses 16 threads while running, on p620 | $0 |
| Groq turbo | $0.04/h with a 10 s minimum: 20 h × 1.4 = 28 h | **$1.12** |
| Soniox async | $0.10/h, 18 h (no minimum stated). Real-time is $0.12/h and may bill both channels. Likely needs new code. Unconfirmed | $1.80 |
| OpenAI `gpt-transcribe` | $0.0045/min = $0.27/h, 18 h (no minimum found) | **$4.86** |
| Deepgram streaming | What the app uses for Deepgram. Both channels, the whole call: 40 h × $0.462 | **$18.48** |

Deepgram's new accounts get $200 credit, no card, no expiry. At $18.48 a month that is about 10.8 months.

## The app's real chunk rate

Measured, and it contacts nothing: `accuracy_replay --fake x --no-filter --speed 8` on the same 20 two-channel calls.

- 201 chunks in 24.6 minutes of audio: **8.2 chunks a minute** (highest call 10.1).
- Chunk length: median 6.2 s, p95 12.0 s.
- Speech sent is 0.9 × the call's length.
- Billed with a 10 s minimum per request: 1.4 × the call's length.

The spec's "40 billable hours" and "chunk every 4 s" were too high for batch engines.
Real calls may differ. These are built, turn-taking calls.

## Self-run NB-Whisper (measured on p620, 2026-11-05)

**Setup.**
- `whisper-server` from nixpkgs `whisper-cpp` 1.9.5, started as: `whisper-server -m ggml-model.bin -t 16 -l no --host 127.0.0.1 --port 8391 --inference-path /v1/audio/transcriptions`.
- Model: `NbAiLab/nb-whisper-small`, file `ggml-model.bin` at revision e9bb5cb83c, 488 MB, Apache 2.0, not gated.
  `ct2/` and `ggml-model-q5_0.bin` are also published. Nothing needed converting. Source: https://huggingface.co/NbAiLab/nb-whisper-small
- It accepted the app's request with no code: `accuracy_replay --url http://127.0.0.1:8391 --model nb-whisper-small --language no`.

**Accuracy.**
20 NPSC calls, `~/.cache/meetingai-eval/no/sets-npsc20`, `--speed 8`, `tools/accuracy/score.py`, same references and the same filter as #70:
**16.7% word errors (534/3203)**. Me 16.4%, them 16.9%, code-switch 51.2%, 0 bleed, 0 invented, 0 phantom, short words 11/16, 0 invalid runs.

Beside #70, on the same references:

| Run | Word errors | Code-switch | Bleed | Short words |
|---|---|---|---|---|
| This run: whisper.cpp server, `ggml-model.bin` | 16.7% (534/3203) | 51.2% | 0 | 11/16 |
| #70: NB-Whisper small in process (sherpa-onnx ONNX export, Silero VAD) | 23.9% (767/3203) | 42.5% | 1 | 7/16 |
| #70: Parakeet | 85.1% | | | |

#70's NB-Whisper run is at `~/.cache/meetingai-eval/no/results/p620/npsc20b-nbsmall.txt`. #70's bar was at most 20%.

**Why it is better is not established.** The differences are a different runtime and export, and no Silero.
The cause is unknown. This document does not claim Silero is the cause.

**Can it keep up?** One call at 1x, 16 threads, machine load about 26.
- Chunk time: median 2.32 s, p95 3.81 s (9 requests). #70's bar was under 1.5 s / 3 s. NB-Whisper in process was 2.4 / 7.8.
- Speech to line: me 4.87 / 6.56 s, them 3.12 / 3.73 s.
- Server RSS 0.6 to 1.3 GB.

One call is a small sample. It was not measured on any other machine.

**Setting in the app.** Speech provider `openai-compatible-stt` (`src/ai/catalog/providers.rs:121`), with `speech_base_url` and `speech_model`.
Local-only allows it on a loopback address.
**Not yet tried in the real app, with real calls.** That is for the owner.

## Deepgram

- Nova-3 Norwegian `no`, monolingual and multilingual: https://developers.deepgram.com/docs/models-languages-overview
- Prices: streaming monolingual $0.0077/min, pre-recorded $0.0043/min, multilingual $0.0092 and $0.0052.
  $200 credit, no card, no expiry. No minimum, billed per second. A 10-minute stereo file is billed 20 minutes: https://deepgram.com/pricing
- Data, default: requests "participate in the Model Improvement Program (MIP) by default, and Deepgram retains them". Request audio is "Retained to improve our models".
  With `mip_opt_out=true`, it is "Retained only for the duration needed to process the request". Opt-out is per request.
  The page does not say whether opting out changes the price. Metadata is kept 90 days.
  The EU endpoint processes and stores "Within the EU; the specific country may change".
  Full in-region needs both the regional endpoint and the opt-out: https://developers.deepgram.com/trust-security/your-data
- Terms, last updated August 6, 2026, section 3.2: Deepgram may use Your Content to improve services, including "training and testing our Models".
  Section 3.3: API users can opt out per request: https://deepgram.com/terms
- **The app sends `mip_opt_out=true` only on the EU endpoint** (`src/speech/stt/transcription.rs:384`, `:825`).
  The owner's `deepgram_region` is `us`. The owner's engine today is `parakeet-local`, so nothing goes to Deepgram now.
  Advice: set `deepgram.region = "eu"` before any real use.

## Groq

- Whisper Large V3 Turbo $0.04/h, V3 $0.111/h. At least 10 s billed per request. Language is optional ISO-639-1.
  Norwegian is not named on the page: https://console.groq.com/docs/speech-to-text
- Limits, base Developer plan: 20 requests a minute, 2K a day, 7.2K audio seconds an hour, 28.8K a day: https://console.groq.com/docs/rate-limits.md
  At 8.2 chunks a minute a call fits the per-minute limit, with room for more. Whether it holds on a real, busier call is not known.
  **The daily request cap is the tight one:** 2K requests at 8.2 a minute is about 4.1 hours of calls a day. 28.8K audio seconds a day is about 8.9 hours at 0.9 speech.
  Groq says the limits for your account are in its settings, and higher ones may be available.
- Data: by default Groq does not retain inference data. Logs for reliability or abuse are kept up to 30 days. Zero retention is available to all customers in Data Controls.
  The page does not say whether inputs train models. Data is retained in Google Cloud in the US: https://console.groq.com/docs/your-data

## OpenAI

- `gpt-transcribe` is the recommended model. `whisper-1`, `gpt-4o-transcribe`, `gpt-4o-mini-transcribe` and `gpt-4o-transcribe-diarize` are deprecated and "will shut down on February 26, 2027".
  There is no language list for `gpt-transcribe`. Norwegian is not named. Endpoint `/v1/audio/transcriptions`, 25 MB: https://developers.openai.com/api/docs/guides/speech-to-text
- Prices: `gpt-transcribe` $0.0045/min, Whisper $0.006, `gpt-4o-transcribe` est. $0.006, mini est. $0.003: https://developers.openai.com/api/docs/pricing
- Data: API data is not used to train by default. For transcriptions, the retention table lists no abuse-monitoring retention and no application state, and it is zero-retention eligible: https://developers.openai.com/api/docs/guides/your-data
- **The app's OpenAI preset still uses `whisper-1`** (`src/ai/catalog/presets.rs:90`), which shuts down on that date. Note it as a follow-up.

## Soniox

- Async $0.10/h, real-time $0.12/h. The page mentions no minimum and no free tier: https://soniox.com/pricing
- Norwegian `no`: https://docs.soniox.com/stt/concepts/supported-languages
- It has its own REST (upload, then transcribe) and WebSocket API. It is not shown as OpenAI-compatible, so it likely needs code: https://docs.soniox.com/stt/api-reference
- Terms were not read. Only a shortlist reserve.

## Others

These come from a research pass and were not re-checked. Each is labelled "from the provider's page, not re-checked" or "third party". All probably need new code. None was checked.

- ElevenLabs Scribe v2 batch $0.22/h, "90+ languages", Norwegian not named. From the provider's page, not re-checked: https://elevenlabs.io/pricing/api
- Mistral Voxtral Mini Transcribe 3 $0.21/h. From the provider's page, not re-checked: https://docs.mistral.ai/inference/pricing
- Speechmatics Oak 1 batch $0.30/h, Norwegian `no`. From the provider's pages, not re-checked: https://www.speechmatics.com/pricing, https://docs.speechmatics.com/speech-to-text/languages
- AssemblyAI U2 $0.15/h, Norwegian on Universal-2 only. From the provider's pages, not re-checked: https://www.assemblyai.com/pricing, https://www.assemblyai.com/docs/pre-recorded-audio/supported-languages
- Google Chirp 3 `no-NO` is in the EU table, price not read. From the provider's page, not re-checked: https://docs.cloud.google.com/speech-to-text/v2/docs/speech-to-text-supported-languages
- Azure: not confirmed.
- Hugging Face: no provider hosts NB-Whisper: https://huggingface.co/docs/inference-providers/index

## Limits

- **NPSC overlap.** NB-Whisper's model card lists NPSC among its training data, though these calls are built from NPSC's test sittings.
  So this set may flatter it. It is also parliamentary speech, not a call.
- **No real microphone or network** in a run.
- **One call at 1x** for speed, on p620 only, at machine load about 26.
- **The chunk rate and the cost ratios** come from built, turn-taking calls. Real calls may differ.
- **No cloud engine was measured for accuracy.** Groq's limits are checked against the chunk rate on paper only.
- **Most provider pages have no date.** The Soniox terms were not read. The "Others" were not re-checked.
- **Not tried in the real app, with real calls.**

## Part 2

Part 2 would measure Deepgram Nova-3 and Groq turbo on the same public NPSC audio. It would need the owner's approval, a monthly limit, and its own plan update.
Part 2 needs sign-ups and sends audio to those providers, so Part 1 does not do it. It may not be needed if the owner is happy with self-run NB-Whisper.
The plan sets Part 2's limit at $5, with a tool that refuses non-NPSC audio. OpenAI joins only if Norwegian is confirmed on a current model. Soniox joins only if the others miss.
