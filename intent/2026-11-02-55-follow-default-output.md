---
status: draft
issue: 55
author: olafkfreund
---

# Intent: System audio follows the default output device

## Problem

When no output device is set in Settings, recording captures the monitor of
whatever the default output was **when recording started**
(`SystemAudio::start(None)`, `src/audio/capture/system_audio.rs:41`). If you
then switch output, for example when a Bluetooth headset connects mid-call:

- the old sink still exists: capture keeps listening to it, and the other side
  of the call goes unrecorded without any notice;
- the old sink disappears: `drop_failed_system`
  (`src/speech/session/listen.rs:393`) shows a notice and goes mic-only for the
  rest of the recording.

Either way the other side of the call is lost.

## Proposed outcome

- With no output device set, system capture moves to the new default output
  within a few seconds of a change, and the recording keeps both channels.
- A short notice row says which output it moved to.
- If the old sink vanishes and a default output exists, capture moves to it
  rather than going mic-only.
- With an output device chosen in Settings, nothing changes: capture stays on
  that device, as today.

## Affected users and systems

- `src/audio/capture/system_audio.rs`, `src/speech/session/listen.rs`.
- p620 (PipeWire with the Pulse shim, `parec` and `pactl`). Owner calls with
  Bluetooth headsets.

## Constraints

- Never restart PipeWire. Only our own `parec` child is stopped and started.
- Tests must not use the live microphone or real audio devices: fake `parec`
  and `pactl`, as the existing tests do.
- The few hundred milliseconds of audio around a switch may be lost; the
  recording must not stop.

## Open questions

1. How should a change be noticed: poll `pactl get-default-sink` every ~2 s
   from the existing pump, or run `pactl subscribe`? Proposal: poll. It's
   one short command and needs no second long-lived child.
2. Should the notice row appear on every switch? Proposal: yes, one row per
   switch, since it explains a gap in the other side's audio.
