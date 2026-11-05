---
status: approved
issue: 79
spec: spec/2026-11-05-79-grounded-smoke.md
coders:
  1: codex   # small tricky logic: thread signalling and harness timing
reviewers:
  - codex    # read-only: the race and the handshake
  - agy      # read-only: timing and the check still being live
---

# Plan: The grounded smoke test stops racing

**Cause:** `fake-daemon.py` replaces the first answer, with chips, by a second, without chips, 1 s later, while `harness-grounded.qml` reads the chips once on its own clock. On a slow start the read comes after the second answer.

**Fix:** a handshake, in test files only. No product code.

## Steps

### 1. The handshake (codex) — `shell/tests/fake-daemon.py`, `shell/tests/harness-grounded.qml`, `shell/tests/smoke.sh`

1. `fake-daemon.py`: a module-level `threading.Event`, `NEXT_TURN`, created before streaming. `stream_grounded` (:462) replaces `time.sleep(1.0)` (:474) with `NEXT_TURN.wait(12)`.
2. In the command handler, after the reply is sent (`self.send(...)`, :859), `if cmd == "test.next": NEXT_TURN.set()`, so two threads never write at once. It is only reachable in grounded mode's harness; other harnesses never send it.
3. `harness-grounded.qml`: in the timer (:32), before `root.phase++`, while `root.phase === 1` and `waited < 16` ticks (about 8 s), stay put unless `sourceChip0` and `sourceChip1` exist with text and `bubbleLabel` contains `12:04`. At phase 2, after reading, call `root.client(overlay).send("test.next", {})`.
4. `smoke.sh:135`: `timeout 15` becomes `timeout 25`.

Traps: `root.find` only returns visible items. The later phases keep their order and timing. `--grounded` is used only at `smoke.sh:132`; confirm with `git grep -- --grounded`.

Check, by Claude, as Codex's sandbox cannot run it:

```bash
for i in $(seq 1 10); do bash shell/tests/smoke.sh >/tmp/s.log 2>&1 && echo ok || { echo FAIL; grep -m1 FAIL /tmp/s.log; }; done
```

while a parallel `cargo build` loads the machine: 10 of 10. Then change `12:04 · Speaker 2` in the expected value by hand, see it fail, and revert. Then `bash shell/tests/ci.sh`.

### 2. Ship (Claude)

Reviews, read-only: Codex and agy. A PR linking intent, spec and plan, closing #79; merge after CI. No deploy, since nothing the app runs changes.

## Rollback

`git revert` the merge.

## Deviations

(Recorded here during implementation, in the same commit as the code.)
