---
status: draft
issue: 64
intent: intent/2026-11-03-64-auto-stop-on-call-end.md
---

# Spec: Notes stop by themselves when the call ends

## Design

All the logic lives in `shell/Service.qml`. Nothing changes in the daemon:
`record.stop` already finishes the notes, and the daemon's `state` already
carries `recording`, `since` (recording start, epoch ms) and `meeting`.
`Client.state` merges every `state` event, so Service's `client.state.since`
and `client.state.meeting` are current. `CallState.qml` already exposes
`call`, `since` (`callSince`, epoch ms) and `writerGone`.

### State in `Service`

| Property | Meaning |
|---|---|
| `ownerSince` (double) | The `callSince` of the call that owns the notes. 0 when no call does. |
| `pendingStop` (var) | `{ meeting }` while the grace timer runs, else `null`. |
| `handledFor` (double) | The `ownerSince` whose end was already handled (timer started, kept, or offered). Later `maybe` / `none` events of the same chain do nothing. |
| `keptFor` (double) | The `ownerSince` the user chose **Keep recording** for. That chain is never stopped automatically or prompted again. |
| `stopGraceMs` (int, 30 000) | The grace period. `tests/harness-call.qml` sets 2000. |

`forCall()` is true when all of these hold:
- `ownerSince > 0`;
- `client.recording`;
- `Number(client.state.since) >= ownerSince - SLACK_MS`, where
  `SLACK_MS = 5000`.

The slack is a deliberate trade-off. It admits notes started during a
short `maybe` just before the call reached `call`, so they count as the
call's. The cost: a room recording started less than 5 s before a call
also counts as the call's.

### Transitions (`onCallChanged`, with `was` = the previous `call`)

1. **Entering `call`:**
   - With `pendingStop` set, this is a **takeover**. Stop the timer and
     clear `pendingStop` and `handledFor`. Keep `ownerSince`, so the notes
     stay owned even under a new `callSince`.
   - Otherwise, with `keptFor === ownerSince` and a recording still
     running, the user chose to keep the notes. Keep `ownerSince`.
   - Otherwise set `ownerSince = calls.since || 1`. A call already on at
     startup takes this branch too, as `calledKey` does today.
   - `react()` is unchanged. It returns early while recording, so a
     returning call never starts a second recording.
2. **Leaving `call`** (`was === "call"`, now `maybe` or `none`), when
   `handledFor !== ownerSince`:
   - **`calls.writerGone`:** the state is unknown. Do nothing here. The
     `none` branch below shows today's offer.
   - **`forCall()`, and `keptFor !== ownerSince`:**
     - set `handledFor = ownerSince` and
       `pendingStop = { meeting: client.state.meeting }`;
     - start `stopTimer`;
     - notify through `offer("The call ended. Notes stop in N s.", "keep",
       "Keep recording", …, meeting, stopGraceMs)`.
3. **`none`, today's offer branch** (`calledKey !== 0 && client.recording`).
   It now also requires:
   - no pending stop;
   - `handledFor !== ownerSince`, unless the writer has gone;
   - `keptFor !== ownerSince`.

   It sets `handledFor = ownerSince` and passes `client.state.meeting` to
   the offer. Non-owned notes, and a gone writer, still get "Stop notes"
   exactly as today, through `maybe` as well (`viamaybe`).
4. **`writerGone` while a stop is pending** (any change where
   `calls.writerGone` is true and `pendingStop` is set): cancel the
   pending stop. Then show the "Stop notes" offer once, because the state
   is unknown.

### Stopping and cancelling

- **`stopTimer` fires:** send `record.stop` only if all of these hold, then
  clear `pendingStop`:
  - `pendingStop` is set;
  - `client.recording`;
  - `client.state.meeting === pendingStop.meeting`;
  - `calls.call !== "call"`;
  - `!calls.writerGone`.
- **`Connections { target: client; onRecordingChanged }`:** when recording
  becomes false, stop the timer and clear `pendingStop`. That covers a
  manual stop during the grace period.
- **`act(what, key, meeting)`:**
  - `"stop"`: today's call condition, plus `client.recording` and
    `client.state.meeting === meeting`.
  - `"keep"`: only if `pendingStop` is set and `pendingStop.meeting ===
    meeting`. Then stop the timer, clear `pendingStop`, and set
    `keptFor = ownerSince`.
  - `"start"`: unchanged.
- **`offer(body, action, label, key, meeting, timeoutMs)`:**
  - `then` becomes the action name itself (`take` → `start`, `stop`,
    `keep`).
  - `-t timeoutMs` is added when it is given, so the notification and its
    blocking `notify-send -A` process expire with the grace period.
  - The notifier stores `meeting`.

### Accepted limits (the safe direction)

Each of these falls back to today's "Stop notes" offer, or to no action.
None of them stops a recording wrongly.
- **A shell restart during the 30 s grace period:** the timer is in
  memory. Once the call is over the detector writes `callSince: 0`, so the
  restarted shell cannot rebuild ownership. The notes keep running.
- **A shell or detector restart after a takeover:** the restarted shell
  sees only the new `callSince`. The older notes then fall outside the
  slack, and the end of the call shows the offer instead of stopping.

The header comment ("It never stops a recording on its own") is replaced by
a description of these rules.

## Alternatives rejected

- **A daemon-side timer.** Only the shell reads `meeting-mode.json`.
  Moving call detection into the daemon is a much bigger change.
- **An in-memory "started for this call" flag.** It is lost on a restart,
  and the order in which `client` and `CallState` start is racy (both
  reviewers). The timestamps hold up.
- **A setting.** The approved intent chose always on, with **Keep
  recording**.
- **Stopping on the first `none`, without a grace period.** A reconnect
  would split the meeting into two recordings.
- **Persisting `pendingStop` to a file** to survive a restart during the
  grace period. That adds state to clean up, for a 30 s window. Accepted
  as a limit instead.

## Risks

- **A detector restart flashes `none` with `writerGone`.** It shows
  today's offer, as now, and never stops anything.
- **A long reconnect** (over 30 s) stops the notes, as the intent
  accepted.
- **`notify-send -A … -t`:** with mako, an expired notification closes and
  `notify-send` exits. Choosing an action from an expired notification is
  impossible, and a stale "Keep" is guarded by the meeting check.
- **Hosts:** p620 and razer run the shell. There is no daemon change.

## Verification

**Fake support** (`shell/tests/fake-daemon.py`):
- `state` gains `"meeting": "m-live"` when live. `record.start` sets a new
  `meeting` (`m-<n>`).
- `record.start` and `record.stop` both broadcast `state`.
- `--live-since=<ms-ago>` places the recording start; the default stays
  754 s ago.

**Test helpers** (`shell/tests/call-state.sh`):
- The `notify-send` stub prints `$NOTIFY_ANSWER` when it is set, so an
  `-A` action can be chosen.
- A `user <cmd>` helper sends `record.stop` or `record.start` to the fake
  socket as a second client, playing the user.
- `harness-call.qml` sets `stopGraceMs: 2000`.
- Existing fixture fix: `viamaybe` uses `callSince` = now (it was `5000`,
  which would make the 754 s-old recording count as the call's).

**Cases** (each fits the 9 s `go` window):

| Case | Setup → steps | Expect |
|---|---|---|
| `autostop` | `--live --live-since=2000`, call `callSince` = now − 10 s; `call` → `none`; wait 3 s | one `record.stop`; a `-A keep=Keep recording` notification with `-t 2000` |
| `maybe-end` | as `autostop`; `call` → `maybe`, wait 3 s | `record.stop` |
| `takeover` | as `autostop`, call `callSince` = now − 60 s, and a recording since 30 s; `call` → `none`, 1 s, `call` (`callSince` = now), 1 s, `none`, wait 3 s | exactly one `record.stop`, after the second end |
| `keep` | `autostop` with `NOTIFY_ANSWER=keep` | no `record.stop`; no further offer |
| `manual-new` | `autostop`; after 0.5 s, `user record.stop`, then `user record.start` | `record.stop` exactly once (the user's); the new meeting keeps recording |
| `writer-gone` | `autostop`, but `call` → `none` with a dead `pid` | no `record.stop`; "Stop notes" offer |
| `gone-pending` | `autostop`; `call` → `maybe`, 0.5 s, the same file with a dead `pid` | no `record.stop`; "Stop notes" offer |
| `hangup` (existing) | the recording predates the call | no `record.stop`; "Stop notes" offer |
| `viamaybe` (existing, fixed) | the same, through `maybe` | no `record.stop`; "Stop notes" offer |

Then: `bash shell/tests/lint.sh`, `bash shell/tests/ci.sh` and
`nix build .#checks.x86_64-linux.shell`.

**Manual check after deploy:** a real call with notes on, then hang up.
"Notes stop in 30 s" appears, and the bar shows the notes finishing about
30 s later. **Keep recording** leaves them running.

## Review record (draft spec)

| Finding | From | Verdict |
|---|---|---|
| The timer could stop the notes after the writer went during the grace period | Codex, agy | confirmed → the timer guard checks `!writerGone`; transition 4 |
| **Keep recording**, then `maybe` → `none`, prompted again | Codex, agy | confirmed → `keptFor` and `handledFor` |
| The tests did not prove takeover, writer-gone-while-pending, or a replacement recording; the fake has no `meeting` | Codex, agy | confirmed → `takeover`, `gone-pending`, `manual-new`; fake support |
| A restart during the grace period, or after a takeover, loses state | Codex, agy | confirmed; the failure is safe → accepted limits |
| The slack rationale was backwards | Codex | confirmed → reworded as a trade-off |
| The existing `viamaybe` fixture (`callSince` 5000) would count as owned | agy | confirmed → fixture fix |
| A stale **Keep** or **Stop** could act on a newer meeting; `offer()` maps every non-take action to stop | agy | confirmed → `act` guards; `then` is the action |
| Are `client.state.since` / `meeting` available? | Codex (could not verify) | verified: `Client.state` merges `state` events; the daemon sends both (`ctl state.get`) |
