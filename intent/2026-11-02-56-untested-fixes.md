---
status: approved
issue: 56
author: olafkfreund
---

# Intent: Tests for the untested #51 fixes (A7, A9/C8, C5)

## Problem

The #51 review sweep fixed three failure paths without adding tests. A later
change could undo any of them unnoticed:

- **A7:** a chunking task that panics or ends early must not leave the
  daemon stuck in "chunking". A drop guard resets the state and logs a
  warning (`src/daemon/session.rs`).
- **A9/C8:** when the speech stream closes, the daemon must recognise it by
  the shared `transcription::STREAM_ENDED` constant, not by copied text. If
  the two drift apart, a closed stream looks like an error.
- **C5:** `set_system_audio` runs in `spawn_blocking`. If that task fails
  (a JoinError), the daemon stops the recording loop cleanly (`stop` set,
  `continue`) instead of panicking or hanging.

## Proposed outcome

One test per fix, each of which fails if its fix is reverted. Behaviour does
not change.

## Affected users and systems

Tests only: `src/daemon/session.rs` (test module), and
`src/speech/stt/transcription.rs` if A9/C8's test fits better there. Small
`#[cfg(test)]` hooks may be added where a failure can't be triggered
otherwise, like the existing `fail_open` flag.

## Constraints

- No real audio, no live microphone, no network.
- Any test hook is `#[cfg(test)]` only, so it never reaches the release build.
- The tests must not be timing-flaky. Poll with a deadline; no fixed sleeps.

## Open questions

None.
