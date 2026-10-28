---
status: approved
issue: 14
author: olafkfreund
---

# Intent: transcripts that show only what was said, by whom

## Problem

Meeting transcripts misstate who said what and add things nobody said, and
the notes and recaps are built from them:

- **Speaker bleed.** In one meeting on p620 today, 319 of 500 lines were on
  the owner's (mic) channel, and many were the other participants heard
  through the speakers ("So, Olaf, none of this is in the cloud"). Muting in
  Teams or Meet does not stop the OS microphone this app records. The echo
  filter only drops a mic line that nearly equals a call line ending within
  2 s; the two channels are cut and transcribed separately, so most bleed
  passes (agy's review found the short-fragment, ordering and similarity
  causes in `src/speech/stt/filter.rs` and `transcription.rs`).
- **Invented text.** Whisper turns quiet audio into stock phrases ("Amen",
  "Thank you for watching") and, fed its own last words as a prompt, repeats
  them. Today's fixes (#12) reduce this but do not remove the cause.
- **Engines.** Whisper is the only local engine. Deepgram works online but
  its key can only be typed into the app; it cannot come from agenix, so
  other hosts cannot use the key now stored there (nixos#2530).

## Proposed outcome

- **Parakeet replaces whisper** as the local speech engine (NVIDIA Parakeet
  TDT 0.6B v3, CC-BY-4.0, CPU, 25 European languages), installed and
  configured by the flake with no manual model download. Whisper is removed
  from the defaults (see open question 1).
- **No speaker bleed:** with speakers on and no headphones, the other side's
  words do not appear as the owner's lines. Removed at the audio level
  before transcription (echo cancellation and a check against the call
  audio at the same moment), with the text filter as a last guard.
- **No invented text:** silence and noise never become lines; the mic
  channel's own text is not fed back to the engine.
- **Mute is honoured:** when the owner mutes in the app, nothing is
  transcribed on the mic channel (already true; verified by a test).
- **Online engines:** Deepgram's key can come from a file
  (`/run/agenix/api-deepgram`) through the Home Manager module, and the app
  gains an "OpenAI-compatible" speech connection, so OpenAI's API, Groq or a
  local Parakeet server work without per-engine code.
- **Measured:** a replayable test recording of a call with speaker bleed
  and silence gives the before/after numbers (lines on the wrong channel,
  invented lines) for whisper, Parakeet and Deepgram.

## Affected users and systems

- This repo: `src/speech/stt/*` (engines, chunking, filter),
  `src/audio/capture/*` (mic source), `src/ai/catalog/providers.rs`,
  `src/daemon/session.rs` (key file), `nix/hm-module.nix` (Parakeet service,
  echo-cancel source option, Deepgram key file), README.
- p620 and any host with the module: the local speech service changes from
  whisper-server to Parakeet; a user-level PipeWire echo-cancel source.
- `nixos_config`: `programs.nixarchy-meetingai.deepgram.keyFile =
  config.age.secrets.api-deepgram.path` after nixos#2530.
- Rewrite #3: `transcription.rs` and `listen.rs` are step 10; removing the
  whisper path shrinks that step. The brief for step 10 is written after
  this lands.

## Constraints

- Audio stays local unless the owner picks an online engine; local-only
  interviews never use one.
- No secret in the Nix store: key files are read at runtime by path.
- No regression for existing settings: a user on `whisper-local` is moved
  to Parakeet on upgrade with a one-time notice.
- The owner's real desktop and microphone are not used by tests; shell
  tests stay headless.
- Works on AMD (p620 RX 7900 XT) and on CPU-only hosts; no CUDA required.

## Open questions

1. **Whisper:** remove it entirely (less code, smaller rewrite step 10) or
   keep it as an optional engine? Recommended: remove from defaults and the
   flake's local service; keep the generic "OpenAI-compatible" connection,
   which can still point at a whisper server if anyone wants one.
2. **Echo cancellation on the system:** add PipeWire's WebRTC echo-cancel
   source through this module (user-level, opt-out), or leave it to the
   host config? Recommended: in this module, on by default.
3. **Default engine when a Deepgram key is present:** Deepgram online, or
   Parakeet local with Deepgram as an opt-in? Recommended: Parakeet local by
   default (privacy, no cost), Deepgram one switch away.

## Answers (owner approved the intent with the recommendations, 2026-10-28)

1. Whisper is removed; the OpenAI-compatible connection can reach any
   whisper server someone runs themselves.
2. PipeWire WebRTC echo-cancel in this module, on by default.
3. Parakeet local is the default engine; Deepgram one switch away.
