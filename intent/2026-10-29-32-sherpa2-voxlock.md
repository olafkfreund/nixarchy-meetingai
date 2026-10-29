---
status: draft
issue: 32
author: olafkfreund
---

# Intent: a matching sherpa-onnx library, and a measured choice of voice filter

## Problem

- **Mismatched versions.** The Rust binding is sherpa-onnx 2.0.1, but the
  flake links the nixpkgs C library, version 1.13.8. Parakeet works today,
  but the binding's structures are written for 2.0; any use of a part that
  changed between the versions can crash or misread memory, with no error
  at build time.
- **VoxLock is out of reach.** VoxLock (Apache-2.0 code and weights) removes
  every voice but one person's from the mic audio, and sherpa-onnx 2.0
  supports it. The 1.13.8 library has no VoxLock.
- **No measured choice.** Two filters claim to keep only the owner's voice:
  VoxLock and Alkove Focus (Apache-2.0, 11 days old, tuned for headsets).
  Both publish their own numbers on other people's tests; neither is proven
  on a desk microphone like the owner's (Razer Seiren V2 Pro) next to
  speakers. A filter that cuts the owner's own words would be worse than
  none.

## Proposed outcome

- The flake builds the sherpa-onnx C library at the same version as the Rust
  binding, and a check fails the build if they differ again.
- Parakeet transcripts are unchanged by the upgrade: the evaluation set from
  #14 gives the same numbers before and after.
- A written comparison, in the repo, of three ways to treat the mic channel
  after echo cancellation: nothing, VoxLock, and Alkove Focus. On the same
  recordings it reports foreign words that got through, owner words that
  were lost, delay, and CPU use on p620.
- A decision from those numbers: one filter becomes an opt-in setting in a
  follow-up issue, or neither does.

## Affected users and systems

- This repo: `nix/` (the sherpa-onnx library build), `Cargo.toml` and
  `Cargo.lock` if the binding needs a matching change, `tools/accuracy/`
  (the comparison and its report), CI.
- Every host with the module: the sherpa-onnx library in the closure
  changes.
- The owner: about 5 minutes reading a short script aloud at the desk, once
  with the speakers silent and once while they play test speech.

## Constraints

- No filter ships in this task; only the library upgrade and the
  measurement. Using a filter in meetings is a separate issue.
- Alkove is used only for the measurement, outside the app's code and build
  (its crates are not published), in an isolated environment.
- Recordings of the owner stay out of git, in `~/meetingai-eval`, readable
  only by the owner; deleted on request.
- Models are fetched with fixed hashes; nothing is downloaded at run time.
- Deploy only from committed main, announced on the agent bus, never while
  recording.

## Open questions

1. **How to build the 2.0 library:** override the nixpkgs package in this
   flake now, or wait for nixpkgs? Recommended: override here now, and
   offer the update to nixpkgs as well.
2. **Another person in the room:** the owner's colleague, or test speech
   played from a second device that is not the computer's speakers (so echo
   cancellation cannot remove it)? Recommended: a second device, such as a
   phone, playing the same test speech.
3. **Scope of the follow-up:** decided after the numbers. If neither filter
   keeps the owner's words better than the voice check of #31, no filter is
   added.
