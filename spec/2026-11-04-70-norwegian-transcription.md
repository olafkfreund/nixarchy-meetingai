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

**Measure first, then build:** S0 checks feasibility, S1–S2 pick the engine
on evidence, and S3–S8 build only if the owner approves S2.

## Design

### S0. Spike: can this sherpa-onnx do Whisper in-process?

Before anything else, a throwaway example checks that `sherpa-onnx = 2.0.1`
(`Cargo.toml` ~44) exposes the offline Whisper model config next to the
transducer config `parakeet.rs` uses (~70–81). It must load the official
turbo export and decode one Norwegian WAV with `language = "no"`.

If the binding lacks Whisper, the spec returns to the owner. The fallback is
a loopback whisper server on the OpenAI-compatible path, which #14 had
dropped. It is not chosen here.

### S1. Measure the candidates

**Engines, all local and in-process (sherpa-onnx):**

| Engine | Source | Notes |
|---|---|---|
| Parakeet v3 (baseline) | current | expected to fail Norwegian: shows the problem in numbers |
| **Whisper large-v3-turbo** | official sherpa-onnx export `csukuangfj/sherpa-onnx-whisper-turbo` (MIT), int8 | multilingual, includes `no` |
| **NB-Whisper** (`NbAiLab/nb-whisper-large` or `-medium`, Apache-2.0) | exported with sherpa-onnx's Whisper export script at a pinned NB-Whisper revision and a pinned sherpa-onnx tool version | trained on Norwegian |

**Test audio (kept outside the repo, like #14's):**
- **Conversational Norwegian** for the gate: **NPSC** (Norwegian
  Parliamentary Speech Corpus, `NbAiLab/NPSC`, CC0). It is spontaneous and
  semi-spontaneous speech, with reference text.
- **Owner's own recordings, if supplied:** a short private set of real
  Norwegian calls with corrected reference text, as #14's private set
  was. They are scored as a separate condition.
- **Read speech** as a reference: FLEURS `nb_no` (CC BY 4.0).
- **#14's failure cases,** built from NPSC speech plus MUSAN noise (CC BY
  4.0):
  - silence;
  - quiet room tone;
  - non-speech bursts of 0.2–1.5 s (coughs, typing, chair);
  - remote-only speech;
  - double talk with delayed bleed;
  - **short words** ("ja", "nei", "takk", "greit");
  - short dictations of 1–3 s and long dictations of 40–60 s.
- **Code-switching:** NPSC segments that already contain English words,
  selected by matching reference text against an English word list. They
  are not spliced audio.

**Tool changes (`tools/accuracy/`, new files beside the existing ones):**
- `no_mix.py`: builds the sets above, in the same truth format
  `libri_mix.py` writes, and parameterised instead of hard-coded to
  LibriSpeech.
- `score.py` gains:
  - word error rate (sequence edit distance, NPSC-style normalisation);
  - invented lines per hour;
  - phantom phrases anywhere in a line;
  - median and 95th-percentile chunk time;
  - peak RSS;
  - an `--engine` choice beside `--parakeet`.
- **Replay paces audio at 1× wall-clock** through the real `BatchChunks` and
  `LineFilter`, so the bleed filter's 3 s hold and the single decode mutex
  are exercised as in a live call.

**Metrics, per engine, set and host (p620 and razer):**
- word error rate;
- invented lines per hour, and phantom phrases;
- bleed lines and lost owner words;
- short words kept;
- dictation success (1–3 s and 40–60 s);
- median and p95 chunk time;
- peak RSS with Parakeet also loaded, plus the speaker model, and with the
  inactive engine unloaded (S7).

### S2. The gate

The owner approves the measured numbers and the chosen engine. The proposed
bar, **on NPSC conversational speech**, plus the owner's set if supplied:

- **word error rate:** at most 20% on NPSC, reported separately for the
  owner's set (FLEURS is reported alongside, not gated);
- **invented text:** at most 1 invented line per hour of silence and
  non-speech, and **no** phantom phrase anywhere;
- **bleed and lost owner words:** no worse than Parakeet on the same
  double-talk set (#14's numbers);
- **short words:** at least 9 in 10 kept;
- **dictation:** short and 40–60 s dictations succeed;
- **latency:** median chunk time under 1.5 s and p95 under 3 s on razer.
  The 3 s matters because of the bleed hold;
- **RAM:** peak under 4 GB on razer, in whichever loading mode S7 picks.
  Unloading the inactive engine is measured here, before approval, not added
  later.

Among the engines that pass, the lower NPSC error rate wins. If neither
passes, the plan stops, the numbers go into #70, and the owner decides.

### S3. A local Norwegian engine, `whisper-nb-local`

A new in-process provider using sherpa-onnx's offline Whisper config (per
S0), next to `parakeet.rs`:
- `language = "no"`, `task = "transcribe"`;
- loaded lazily and kept loaded, unless S2 chose unloading;
- long audio is cut with Parakeet's `split_points`/`decode_pieces`
  (`parakeet.rs` ~119–151), so 40–60 s dictations fit Whisper's 30 s
  window;
- **no prompt:** the in-process path receives neither the glossary nor
  `filter.prompt`.
  - The OpenAI-compatible path keeps its prompt. It is the user's engine,
    and the intent leaves it out of scope.
- `SttWire::InProcess` dispatches by provider id (`transcription.rs` ~172),
  not straight to Parakeet.

**The non-speech guard, a real voice detector:**
- Chunks for this engine pass through sherpa-onnx's **Silero VAD** (about
  2 MB, pinned like the other models).
- Only the speech segments it finds are decoded. A chunk with no speech
  segment is not sent.
- No minimum length beyond the VAD's own, so "ja" and "nei" survive (S2
  gates this).
- **Phantom phrases are removed anywhere in a line**, not only when they are
  the whole line. They come from a fixed list measured in S1, and a line
  left empty is dropped.

### S4. Choosing the engine

- **Settings** gain a speech-language picker next to the speech engine
  (`shell/Overlay.qml`), with Auto plus the languages the active engine
  supports.
  - Existing values keep working: empty means Auto, and Nix `speechLanguage`
    still sets it.
  - The Auto hint: "Speaking Norwegian? Choose it here: the default engine
    does not know Norwegian."
- **The resolved engine.** One function,
  `resolve_speech(settings) -> ResolvedSpeech { id, config, language }`,
  decides the engine. **Everything downstream uses the resolved id, not the
  stored `speech_provider`:**
  - `speech_config` (~1193);
  - preload in `record_start` (~2485) and `dictate_start` (~3290);
  - `speech_not_ready` (~643) and `state_value` (~985);
  - so a missing Norwegian model says so, rather than "not ready: ." for
    Parakeet.
- **Rules:**
  - **Default engine:** `parakeet-local` with `speech_language` in {`nb`,
    `no`, `nn`} resolves to `whisper-nb-local`.
  - **A provider the user chose** is never replaced; S6 applies.
  - **Any other language** with `parakeet-local` stays on Parakeet.
- **Dictation** uses `resolve_speech`, so it follows the same rules.
- **Mid-call behaviour:** the engine and language are fixed per recording.
  Changing the setting during a recording takes effect at the next
  `record.start`, and a notice says so.
- **Mixed languages:** with a Norwegian engine, English terms are
  transcribed as best that engine can, and S1 reports the code-switch error
  rate. An English-only call with `nb` set is transcribed by the Norwegian
  engine, and the picker hint says to use Auto for English.
- **Auto (no language set)** stays on Parakeet, with **no automatic
  switching in this issue**. Telling Norwegian from Swedish or Danish on the
  first utterance is the risky case both reviews named, so Auto detection
  gets its own issue.

### S5. Language codes, normalised once

`speech_language_for(engine, setting)` runs **where the provider config is
built**, inside `resolve_speech`. Every path then reads the normalised
`config.language`, including Deepgram batch and live (`live_url` ~808),
OpenAI-compatible, custom raw, JSON, multipart and streaming (~865).

- Empty or "Auto" (any case) → unset.
- `nb`/`no`/`nn` → `no` for OpenAI-compatible, Deepgram and
  `whisper-nb-local`; unchanged for custom.
- Every other language passes through unchanged.
- Deepgram live keeps its existing rule that `en` means unset (~808).

The custom `multipart-wav` body gains `language` when one is set (~308).

### S6. No silent wrong language

**Known support per engine:**

| Engine | Support |
|---|---|
| Parakeet | 25 languages (constant, from the model card) |
| `whisper-nb-local` | `nb`/`no`/`nn` |
| Deepgram | a constant list for its models, including `no` |
| OpenAI-compatible and custom | **unknown**: depends on the server and model |

**At `record.start` and `dictate.start`, before capture:**
- **Known unsupported:** the start is refused, and the message **names an
  engine that can** do the language. For example: "The default engine can't
  transcribe Norwegian. Install the Norwegian engine
  (`whisperNb.enable`), or switch to Deepgram." The intent asks for it to be
  offered.
- **Unknown (OpenAI-compatible, custom):** recording starts, the language is
  sent, and a one-time notice says the app can't check that this server
  supports it.
- **Automatic recording** (`auto_record_on_call`): a known-unsupported
  language does not start recording. The call-offer notice says why and how
  to fix it. That is the same rule, with no capture. A wrong-language
  transcript would be the silent failure the intent forbids.

### S7. Packaging, local-only and budget

- **Model.** `nix/whisper-nb-model.nix` fetches the chosen export with a
  pinned hash.
  - For NB-Whisper, the export is a release asset of this repo, with the
    NB-Whisper revision, the sherpa-onnx export-tool version and the exact
    command committed under `tools/whisper-export/`. The hash checks the
    bytes; the pinned inputs make it reproducible.
  - Apache-2.0 allows redistribution, and the asset carries its licence.
  - Silero VAD: `nix/silero-vad.nix`, pinned.
- **Home Manager** (`nix/hm-module.nix`):
  - new options `whisperNb.enable` (default **off**) and `whisperNb.model`;
  - when enabled, the daemon gets `NIXARCHY_MEETINGAI_WHISPER_NB_DIR` (and
    the VAD path), mirroring `parakeet.enable` (~170–181);
  - the 0.8–1.6 GB model is in the closure **only when enabled**;
  - readiness: `whisper_nb::not_ready_reason()`, like Parakeet's.
- **Local-only.**
  - The allowlist that matters is `speech_config`'s (~1199). It admits
    `whisper-nb-local` explicitly. `stays_local` (~628) already treats
    in-process engines as local.
  - `migrate_whisper` (~744) rewrites only the old `whisper-local` id.
  - Tests pin all three.
- **Budget.** The loading mode (both resident, or unload the inactive engine
  on switch) is chosen at S2 from measured numbers.

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

- **S0:** the spike compiles, loads the turbo export and decodes one
  Norwegian WAV.
- **S1/S2:** `docs/eval/2026-11-norwegian-stt.md` with every bar against its
  measured value, per engine, set and host, approved by the owner.
- **`cargo test --workspace`:**
  - **`resolve_speech`:**
    - the default engine plus `nb`/`no`/`nn` gives `whisper-nb-local`;
    - a chosen provider is never replaced;
    - other languages stay on Parakeet;
    - preload, `speech_not_ready` and `state_value` follow the resolved
      engine, and a missing Norwegian model gives its own message.
  - **Codes:** for every engine and path, including Deepgram live `en`,
    custom streaming, and "Auto" in any case.
  - **S6:**
    - known unsupported is refused before capture, naming an engine that
      can;
    - unknown engines start with one notice;
    - auto-record with a known-unsupported language does not capture, and
      shows the notice.
  - **Local-only:** `whisper-nb-local` is admitted, cloud is refused, and
    `migrate_whisper` leaves the new id alone.
  - **Guard:**
    - the VAD drops a non-speech chunk;
    - "ja"/"nei"-length speech survives;
    - a phantom phrase is removed at the start, the middle and the end of a
      line.
  - **Dictation:** 1–3 s and 40–60 s with `nb`, split under 30 s.
  - **Settings:** a change during a recording applies at the next start,
    with a notice.
  - **Custom multipart** sends `language`.
- `bash shell/tests/ci.sh` (the picker, hints and notices in the fake daemon
  and smoke), `nix flake check`.
- **After deploy, with the owner:**
  - a real Norwegian call and a Norwegian dictation, transcribed as
    Norwegian;
  - a quiet stretch with typing, which must produce no text;
  - "ja"/"nei" answers, which must be kept.

## Review record (draft)

| Finding | From | Verdict |
|---|---|---|
| The gate lacks conversational Norwegian before the default changes | Codex, agy | confirmed → NPSC (CC0) is gated; owner set optional; FLEURS reported only |
| Routing breaks readiness, preload (dictation preloads Parakeet), and in-process dispatch | agy, Codex | confirmed → `resolve_speech` used everywhere; dispatch by id |
| The energy guard passes noise and drops "ja"/"nei" and short dictation; phantom phrase only as a whole line | agy, Codex | confirmed → Silero VAD; phrases removed anywhere; short-word and dictation bars |
| `tools/accuracy` lacks WER, p95, RSS, Norwegian builders; unpaced replay misses the bleed hold | agy, Codex | confirmed → `no_mix.py`, `score.py` extensions, 1× pacing |
| Language normalisation misses live/streaming paths; the Deepgram live `en` rule | agy | confirmed → normalised at config build; `en` rule kept |
| S6 assumed cloud/custom support; must offer an engine; auto-record | Codex, agy | confirmed → known/unknown table; message names an engine; auto-record rule |
| Mid-call change, mixed languages, dictation over 30 s | agy | confirmed → fixed per recording; code-switch reported; `split_points` reused |
| Home Manager option, env var, closure, readiness; export reproducibility | Codex, agy | confirmed → `whisperNb.*`, off by default; pinned export inputs |
| RAM fallback conflicted with the gate | Codex | confirmed → unload measured before S2 |
| sherpa-onnx 2.0.1 Whisper API not verified | Codex, agy | confirmed → S0 spike first |
| `stays_local` vs the `speech_config` allowlist | agy | confirmed → the allowlist is the gate |

