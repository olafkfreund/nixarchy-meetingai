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

**Likely cause, from reading the harness; not yet proven by a run.** `shell/tests/harness-grounded.qml` steps through
phases on a 500 ms timer (:32) and reads the citation chips exactly once, at phase 2, one second in (:36–41). If
the shell has not finished loading and drawing the answer by then, the chips read as empty. That is the failure
seen: `lineChip` and `notesChip` empty, and every later check true.

## Proposed outcome

- The phase passes repeatedly on a busy machine, and still fails if the chips are really wrong or missing.
- The test keeps checking the same things, with the same expected values.

## Affected users and systems

Only `shell/tests/harness-grounded.qml`, and whichever other harness phases have the same fixed-time read. No product code.

## Constraints

- **A real failure must still fail.** Waiting has a limit, inside the phase's `timeout 15`.
- **Later phases must keep their order and their own timing.** They depend on the steps before them.
- **Smallest change:** wait for this condition, not a general rewrite of the harness.

## Open questions

1. **Do other harnesses have the same pattern?** Recommended: fix `grounded` only, since it is the one seen failing, and file others if they fail.
