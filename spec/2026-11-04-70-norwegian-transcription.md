---
status: draft
issue: 70
intent: intent/2026-11-04-70-norwegian-transcription.md
---

# Spec: Calls in Norwegian are transcribed as Norwegian, locally

The approved intent asks for:
- Norwegian transcribed locally;
- no silent wrong language when a language is set;
- a speech-language picker, without ever replacing a provider the user
  chose;
- dictation included;
- no change for Parakeet's 25 languages;
- #14's guarantees kept: no invented text, no bleed.

**Measure first, then build:** S1–S2 pick the engine on evidence, and S3–S8
build only if the owner approves S2.

## Design

### S1. Measure the candidates (`tools/accuracy`, extended)

**Engines, all local and in-process through sherpa-onnx (the runtime
Parakeet and the speaker model already use):**

| Engine | Source | Notes |
|---|---|---|
| Parakeet v3 (baseline) | current | expected to fail Norwegian: shows the problem in numbers |
| **Whisper large-v3-turbo** | official sherpa-onnx export `csukuangfj/sherpa-onnx-whisper-turbo` (MIT), int8 | multilingual, includes `no`; no new inference dependency |
| **NB-Whisper** (`NbAiLab/nb-whisper-large` or `-medium`, Apache-2.0) | exported once with sherpa-onnx's own Whisper export script, int8 | trained on Norwegian; no official sherpa-onnx export exists, so the export is pinned (S7) |

**Test audio:**
- **Norwegian speech:** FLEURS `nb_no` test split (CC BY 4.0), about 350
  utterances with reference text.
- **#14's failure cases,** built with the existing `tools/accuracy`
  generators (`synthetic.py`, `libri_mix.py`) from FLEURS speech plus MUSAN
  noise (CC BY 4.0):
  - silence;
  - quiet room tone;
  - short non-speech chunks of 0.2–1.0 s (coughs, typing, chair);
  - remote-only speech;
  - double talk with delayed bleed;
  - Norwegian with English terms (FLEURS `nb_no` phrases mixed with English
    technical words, from a fixed list).
- **Everything goes through the real chunker** (`BatchChunks`) and
  `LineFilter`, as the live path does, so short noise chunks and prompts are
  tested as they really arrive.
- Test audio stays outside the repo, as for #14.

**Metrics, per engine (scored with `tools/accuracy/score.py`):**
- word error rate on Norwegian;
- **invented lines:** lines from silence or non-speech, per hour;
- **known phantom phrases** ("Teksting av", "Undertekster", "Takk for at du
  så på", "Amen");
- bleed lines and lost owner words in double talk;
- median and 95th-percentile time per chunk, on p620 and razer;
- **peak RSS with Parakeet also loaded**, plus the speaker model, on p620
  and razer.

### S2. The gate

The owner approves the measured numbers and the chosen engine. The proposed
bar on FLEURS `nb_no` with #14's failure cases:

- word error rate: **at most 15%** (FLEURS-style read speech; Parakeet's
  Danish is 18%);
- invented lines from silence and non-speech: **at most 1 per hour**, and no
  phantom phrase;
- bleed and lost owner words: **no worse than Parakeet** on the same
  double-talk set (#14's numbers);
- median chunk time: **under 1.5 s** on razer;
- peak RSS with Parakeet loaded: **under 4 GB** on razer.

Among the engines that pass, the lower error rate wins. If neither passes,
the plan stops, the numbers go into #70, and the owner decides (a smaller
model, or Deepgram for Norwegian non-local calls only).

### S3. A local Norwegian engine, `whisper-nb-local`

A new in-process provider using sherpa-onnx `OfflineWhisperModelConfig`, the
same way `src/speech/stt/parakeet.rs` uses the transducer config:
- `language = "no"`, `task = "transcribe"`;
- loaded lazily on first use and kept loaded, as Parakeet is;
- **no prompt at all**: sherpa-onnx Whisper takes none, so the glossary
  and `filter.prompt` never reach it. That removes #14's self-prompt cause on
  both channels.

**The non-speech guard,** for this engine only:
- a chunk is sent only if it has at least 0.5 s of voiced audio (30 ms
  frames with `rms ≥ 0.005`, the `voice_check::speech` rule);
- the result is dropped if it is only a phantom phrase from a fixed,
  tested list.

S1's invented-line count is measured **with** this guard, and the gate
applies to it.

### S4. Choosing the engine

- **Settings** gain a speech-language picker next to the speech engine
  (`shell/Overlay.qml`), with Auto plus the languages the engines support.
  Existing values keep working: an empty value means Auto, and the Nix
  `speechLanguage` still sets it.
- **Routing, in `speech_config`** (`src/daemon/session.rs` ~1193):
  - **Default engine:** `speech_provider = parakeet-local` with
    `speech_language` in {`nb`, `no`, `nn`} uses `whisper-nb-local`. Both are
    local, so it is the same class of engine.
  - **A provider the user chose** (Deepgram, OpenAI-compatible, custom) is
    **never replaced**. It gets the language, mapped to its code (S5). If it
    cannot take it, S6 applies.
  - **Any other language** with `parakeet-local` stays on Parakeet.
- **Dictation** (`dictate.start`/`dictate.stop`, ~3271/~3362) uses the same
  `speech_config`, so it follows the same routing.
- **Auto (no language set)** stays on Parakeet, with **no automatic
  switching in this issue.**
  - Parakeet has no language detector.
  - Telling Norwegian from Swedish or Danish acoustically on the first
    utterance is the risky case both reviews named.
  - The Settings picker shows the hint "Speaking Norwegian? Choose it here:
    the default engine does not know Norwegian."
  - Auto detection can follow as its own issue, with its own measurement.

### S5. Language codes, normalised once

One function, `speech_language_for(engine, setting) -> Option<String>`,
used by every path:

| Engine | Code sent |
|---|---|
| empty or "Auto" (any case) | nothing |
| OpenAI-compatible | `no` |
| Deepgram | `no` |
| custom raw/JSON | the setting |
| `whisper-nb-local` | `no` |
| Parakeet | n/a |

The custom `multipart-wav` body gains a `language` field when one is set
(today it drops it, `transcription.rs` ~308).

### S6. No silent wrong language

A guard at `record.start` and `dictate.start`, before capture: if
`speech_language` is set and the active engine cannot do it, the start is
refused with a message naming the problem and the fix. For example: "The
default engine can't transcribe Icelandic. Choose another speech engine, or
set the language to Auto."
- Parakeet's 25 languages are a constant taken from the model card.
- `whisper-nb-local` supports `nb`, `no` and `nn`.
- Cloud and custom engines are assumed able to take a language, because
  they receive it.

### S7. Packaging, local-only and budget

- **Packaging.** A new `nix/whisper-nb-model.nix` fetches the chosen export
  with a pinned hash, like `nix/parakeet-model.nix`. For NB-Whisper:
  - the one-time export is published as a release asset of this repo;
  - the export script and the command used are committed under
    `tools/whisper-export/`, so it can be reproduced.
- **Local-only.**
  - `stays_local` (~628) and the local-only check in `speech_config`
    (~1199) admit `whisper-nb-local` explicitly.
  - `migrate_whisper` (~744) rewrites only the old `whisper-local` id and
    never the new one.
  - Tests pin all three.
- **Budget.**
  - Both models stay loaded once used, as Parakeet does today.
  - If S1 shows peak RSS on razer over the gate, the inactive engine is
    dropped when the other loads.
  - That is a measured switch, decided at S2, not built speculatively.

### S8. Answer language and scope

- `speech_language` does not change `response_language`; answers and notes
  keep their own setting.
- Out of scope, as in the intent:
  - English-only live triggers (#71);
  - the `"en"` meeting language column.

## Alternatives rejected

- **A loopback whisper server (whisper.cpp / faster-whisper) on the
  OpenAI-compatible path.** It works today without code, but needs a second
  service and runtime, and #14 removed whisper.cpp for its invented text and
  self-prompting. The in-process sherpa-onnx route reuses the runtime and
  takes no prompt.
- **Auto language detection in this issue.** Norwegian, Swedish and Danish
  are acoustically close, and a wrong switch rewrites a live call. It is
  deferred to its own issue with its own measurement.
- **Replacing Parakeet with Whisper for everything.** That reverts #14's gains
  for 25 languages.
- **Deepgram as the fix.** It is cloud, so not for local-only interviews. It
  stays the user's choice, and gets the right code (S5).

## Risks

- **Invented text in Norwegian.** Whisper-family risk, measured with the S3
  guard against the S2 bar. The guard's phrase list could hide a real
  "Takk": it only drops lines that are **only** a listed phrase.
- **Model size.** large-v3-turbo int8 is about 0.8 GB, and NB-Whisper large
  int8 about 1.6 GB. Measured on razer, with the unload switch if needed.
- **Export reproducibility (NB-Whisper).** The pinned hash plus the
  committed export script and command. If the export cannot be made to match
  sherpa-onnx's format, the gate falls back to large-v3-turbo.
- **Refusing to record (S6)** surprises someone with an exotic setting. The
  message says exactly how to fix it, and Auto always works.

## Verification

- **S1/S2:** the eval doc `docs/eval/2026-11-norwegian-stt.md` with every bar
  against its measured value, per engine and host, approved by the owner.
- **`cargo test --workspace`:**
  - routing: the default engine plus `nb`/`no`/`nn` goes to
    `whisper-nb-local`, a chosen provider is never replaced, and other
    languages stay on Parakeet;
  - code mapping for every engine, including "Auto" in any case being
    dropped;
  - the guard: an unsupported set language is refused at `record.start`
    and `dictate.start` before capture;
  - local-only: `whisper-nb-local` is admitted, cloud is still refused, and
    `migrate_whisper` leaves the new id alone;
  - the non-speech guard: under 0.5 s voiced is not sent, and a lone
    phantom phrase is dropped;
  - custom multipart sends `language`.
- `bash shell/tests/ci.sh` (the picker and its hint in the fake daemon and
  smoke), `nix flake check`.
- **After deploy, with the owner:** a real Norwegian call and a Norwegian
  dictation, transcribed as Norwegian; then a quiet stretch with typing,
  which must not produce text.
