---
status: draft
issue: 79
author: olafkfreund
---

# Intent: The grounded smoke test stops failing under load

## Problem

The `grounded` phase of `shell/tests/smoke.sh` fails intermittently, which fails `shell/tests/ci.sh`
with an error that has nothing to do with the change under test. It failed 3 of 3 runs on clean `main` and
2 of 3 on the #77 branch, on a busy p620, so people learn to ignore it, and a real failure could be missed.

**Cause, from reading the code; not yet confirmed by a run: two clocks race.** `shell/tests/fake-daemon.py` (`stream_grounded`, :462) sends the first
answer, with its chips, 0.3 s after it starts, then a second answer, with no chips, 1.0 s later. `shell/tests/harness-grounded.qml` reads the chips once at its own phase 2,
1.0 s after its timer starts (:32, :36). Nothing links the two. If the shell starts slowly, the read lands after the second answer. That matches the failure
exactly: `lineChip` and `notesChip` are empty because the second answer has none, while `bubbleStamp` and `markersHidden` stay true because the first answer is still on screen
as an archived row. An earlier "not loaded yet" theory is ruled out for the same reason: the answer text was already showing. So this is a test problem, not a product bug.

## Proposed outcome

- The phase passes 10 of 10 runs while the machine is busy, for example with a parallel `cargo` build, and still fails if the chips are really wrong or missing.
- The test keeps checking the same things, with the same expected values.

## Affected users and systems

`shell/tests/harness-grounded.qml` and `shell/tests/fake-daemon.py`'s grounded mode. No product code. The harness's later phases also read at fixed times (search results, jumps, rename), so
they may race too; the spec covers what the same fix reaches.

## Constraints

- **A real failure must still fail.** Waiting has a limit, inside the phase's `timeout 15`.
- **Later phases must keep their order and their own timing.** They depend on the steps before them.
- **Smallest change:** wait for this condition, not a general rewrite of the harness.

## Open questions

1. **Do other harnesses have the same pattern?** They step on fixed timers too, per review. Recommended: fix `grounded` only, since it is the only one seen failing, and file others if they fail.
