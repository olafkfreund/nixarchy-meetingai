---
status: draft
issue: 64
intent: intent/2026-11-03-64-auto-stop-on-call-end.md
---

# Spec: Notes stop by themselves when the call ends

## Design

All the logic lives in `shell/Service.qml`. Nothing changes in the daemon:
`record.stop` already finishes the notes, and `state.get` / `state` events
already carry `recording`, `since` (recording start, epoch ms) and
`meeting`. `CallState.qml` already exposes `call`, `since` (`callSince`,
epoch ms) and `writerGone`.

**Ownership.** A new `ownerSince` property holds the `callSince` of the
call that owns the notes.
- When the call reaches `call`, `ownerSince` is set to `calls.since`. The
  exception is a takeover (below), which keeps it.
- `forCall()` is true when all of these hold:
  - `client.recording`;
  - `Number(client.state.since) >= ownerSince - SLACK_MS`, where
    `SLACK_MS = 5000`, so notes started during a short `maybe` just before
    the call count;
  - `ownerSince > 0`.
- These are timestamps the daemon and the detector keep, so `forCall()`
  gives the same answer after a shell restart. A call that is already on
  when the shell starts sets `ownerSince` from the file, as `calledKey`
  does today.

**Leaving `call`.** In `onCallChanged`, when the previous state was `call`
and the new one is `maybe` or `none`:
- **`calls.writerGone`** (the state is unknown): if recording, show
  today's "Stop notes" offer. No timer.
- **`forCall()`**: set `pendingStop = { meeting: client.state.meeting }`
  and start `stopTimer` (`interval: stopGraceMs`, default 30 000). Show
  one notification:
  `notify-send -t <stopGraceMs> -A keep="Keep recording" … "The call ended. Notes stop in 30 s."`
- **Otherwise, while recording:** today's "Stop notes" offer. It is still
  shown only on `none`, as today.

**Returning to `call`.** When the state is back at `call` while
`pendingStop` is set, this is a takeover:
- stop `stopTimer` and clear `pendingStop`;
- keep `ownerSince`, so the notes are still the returning call's when that
  call ends, even if its `callSince` is new;
- `react()` stays as it is: it returns early while recording, so the
  returning call does not start a second recording.

**When the timer fires.** It sends `record.stop` only if all of these hold:
- `pendingStop` is set;
- `client.recording`;
- `client.state.meeting === pendingStop.meeting`;
- `calls.call !== "call"`.

It then clears `pendingStop`. A "Notes stopped" notification is not
needed: the bar already changes state.

**Cancelling.**
- `Connections { target: client; onRecordingChanged }`: when recording
  becomes false, clear `pendingStop` and stop the timer. That covers a
  manual stop during the grace period.
- **Keep recording:** the notification's action runs `act("keep", …)`,
  which clears `pendingStop` and stops the timer. The recording runs on,
  and nothing else is prompted for that call.

**Fixing today's "Stop notes" offer.** `act("stop", key)` checks the call
today, not the recording (`Service.qml:58-63`).
- The offer now carries the meeting: `offer(…, key, meeting)` stores it on
  the notifier.
- `act` stops only when `client.state.meeting === meeting`. The old call
  condition stays as well.

**Test seam.** `property int stopGraceMs: 30000` on `Service`.
`tests/harness-call.qml` sets it to `2000`, so the tests finish in seconds.
The notification text is built from `stopGraceMs / 1000`.

**Header comment.** It currently says "It never stops a recording on its
own"; that changes to describe the new rule.

## Alternatives rejected

- **A daemon-side timer.** The daemon does not read `meeting-mode.json`;
  only the shell does. Moving call detection into the daemon is a much
  bigger change.
- **An in-memory "started for this call" flag.** It is lost on a shell
  restart, and the order in which `client` and `CallState` start is racy
  (both reviewers). The timestamps hold up.
- **A setting `auto_stop_on_call_end`.** The approved intent chose always
  on: **Keep recording** covers the exceptions.
- **Stopping on the first `none`, without a grace period.** A reconnect
  would split the meeting into two recordings.

## Risks

- **A detector restart briefly reads as `none`** (a new `pid`, until
  `/proc` confirms it). With `writerGone` this shows today's offer, exactly
  as now, and never stops anything. The flash of an offer is existing
  behaviour.
- **A long reconnect** (over 30 s) stops the notes, as the intent accepted.
  Taking notes again starts a new meeting.
- **Clock:** `since` and `callSince` both come from the same host's clock.
  The 5 s slack covers the gap between the detector seeing the call and
  the user pressing record.
- **`notify-send -A` blocks** until it is closed or expires. `-t` makes it
  expire with the grace period, so no process is left behind.
- **Hosts:** p620 and razer run the shell. There is no daemon change, so
  nothing differs for MCP or agents.

## Verification

`shell/tests/call-state.sh` gains cases against `fake-daemon.py`, using
the existing `st`/`wr` helpers and `stopGraceMs = 2000`:

| Case | Steps | Expect |
|---|---|---|
| `autostop` | `--live`, with the recording `since` after `callSince`; `call` → `none`; wait 3 s | `record.stop` logged; a `-A keep=Keep recording` notification |
| `return` | as above, but `none` → `call` (new `callSince`) within 1 s, then wait 3 s | no `record.stop` |
| `maybe-end` | `call` → `maybe`; wait 3 s | `record.stop` |
| `writer-gone` | the recording is the call's; the file's `pid` is dead | no `record.stop`; the "Stop notes" offer |
| `hangup` (existing) | the recording predates the call (`since` = now − 754 s) | no `record.stop`; the "Stop notes" offer, as today |
| `keep` | `autostop`, with the notify stub answering `keep` | no `record.stop` |

Fake support needed:
- `fake-daemon.py` gains `--live-since=<ms>`, to place the recording start.
- The `notify-send` stub prints `$NOTIFY_ANSWER` when it is set, so
  `-A` actions can be exercised.

The manual-stop cancel is covered by code review. Its guard is the meeting
check, which the `writer-gone` and `hangup` cases also exercise.

Then the repo checks: `bash shell/tests/lint.sh`, `bash shell/tests/ci.sh`
and `nix build .#checks.x86_64-linux.shell`.

Manual check after deploy: a real call in Zoom or Meet with notes on, then
hang up. "Notes stop in 30 s" appears, and the bar shows the notes
finishing about 30 s later.
