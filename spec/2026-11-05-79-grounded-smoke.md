---
status: draft
issue: 79
intent: intent/2026-11-05-79-grounded-smoke.md
---

# Spec: The grounded smoke test stops racing

## Design

Replace the race between two clocks with one explicit handshake, in the test only.

- **`shell/tests/fake-daemon.py`, `stream_grounded` (:462):** after the first answer, wait for a new test-only command, `test.next`, instead of `time.sleep(1.0)`. Wait at most 10 s, then send the
  second answer anyway, so a broken harness fails on its checks and does not hang.
- **`shell/tests/harness-grounded.qml` (:32–46):** the timer does not move past phase 1 until `nowSources` shows its chips, or until about 8 s has passed, when it continues and the existing check
  fails as it should. After reading them at phase 2, the harness sends `test.next`.
- **Later phases keep their order and timing.** Their fixed-time reads are not changed.
- **The `timeout 15` (`smoke.sh:135`)** grows only by what the wait can add, to 25 s, so a slow start still finishes.

## Alternatives rejected

- **A longer sleep in the daemon, or an earlier read.** It moves the race; a busier machine still loses it.
- **Reading the chips repeatedly until they appear.** The chips can already be gone if the second answer arrives first, so the harness would wait for something that never returns.
- **A rewrite of all harnesses into condition waits.** Only `grounded` has been seen failing.

## Risks

- **Later phases may race too.** They read at fixed times, and this does not touch them. If one fails after this, it gets the same treatment.
- **`--grounded` is shared with nothing else.** Only `harness-grounded.qml` starts the daemon with it (`smoke.sh:132`), so other harnesses are unaffected. The plan confirms this with a search.

## Verification

- `smoke.sh`'s grounded phase passes 10 of 10 runs during a parallel `cargo build`.
- It still fails when a chip's expected text is deliberately changed, proving the check is live; that edit is not committed.
- `bash shell/tests/ci.sh` passes.
