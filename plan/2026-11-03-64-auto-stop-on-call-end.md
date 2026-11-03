---
status: draft
issue: 64
spec: spec/2026-11-03-64-auto-stop-on-call-end.md
coders:
  1: coder   # fake-daemon flags + call-state.sh cases: mechanical test plumbing that follows existing helpers
  2: codex   # Service.qml: small, tricky state machine (ownership, takeover, guards) — codex's strength
reviewers:
  - codex    # independent read of the Service.qml diff against this plan
  - agy      # second independent read; it found the most transition bugs in the spec review
---

# Plan: Notes stop by themselves when the call ends

## Decisions carried over (approved spec)

- **Shell only.** All logic is in `shell/Service.qml`; the daemon does not
  change. `client.state.since` (the recording start, epoch ms) and
  `client.state.meeting` come from the daemon's `state`. `calls.since`
  (`callSince`, epoch ms) and `calls.writerGone` come from `CallState.qml`.
- **New Service properties:**
  - `ownerSince` (double, 0);
  - `pendingStop` (var, null; `{meeting}`);
  - `handledFor` (double, 0);
  - `keptFor` (double, 0);
  - `stopGraceMs` (int, 30000);
  - `readonly property int slackMs: 5000`;
  - a `Timer { id: stopTimer; interval: root.stopGraceMs }`.
- **`forCall()`:** `ownerSince > 0 && client.recording &&
  Number(client.state.since) >= ownerSince - slackMs`.
- **Entering `call`:**
  - If `pendingStop` is set, this is a takeover: stop the timer, set
    `pendingStop = null` and `handledFor = 0`, and keep `ownerSince`.
  - Else, if `keptFor === ownerSince && ownerSince > 0 && client.recording`,
    keep `ownerSince`.
  - Else set `ownerSince = calls.since || 1`.
- **Leaving `call`** (previous `call`, now `maybe` or `none`), when
  `handledFor !== ownerSince`, `!calls.writerGone`, `forCall()` and
  `keptFor !== ownerSince`:
  - set `handledFor = ownerSince` and
    `pendingStop = {meeting: String(client.state.meeting)}`;
  - restart `stopTimer`;
  - call `offer("The call ended. Notes stop in " + stopGraceMs/1000 +
    " s.", "keep", "Keep recording", ownerSince, pendingStop.meeting,
    stopGraceMs)`.
- **Today's `none` offer branch** (`calledKey !== 0 && client.recording`)
  additionally requires:
  - `!pendingStop` and `keptFor !== ownerSince`;
  - `handledFor !== ownerSince || calls.writerGone`.

  It sets `handledFor = ownerSince` and passes
  `String(client.state.meeting)`.
- **`writerGone` while a stop is pending** (checked in `onCallChanged` and
  in `onWriterGoneChanged`): stop the timer and set `pendingStop = null`.
  If recording, show the "Stop notes" offer once (`handledFor =
  ownerSince`).
- **The timer fires:** `record.stop` only when all of these hold, then set
  `pendingStop = null`:
  - `pendingStop` is set;
  - `client.recording`;
  - `String(client.state.meeting) === pendingStop.meeting`;
  - `calls.call !== "call"`;
  - `!calls.writerGone`.
- **`Connections { target: client; function onRecordingChanged() }`:**
  when recording becomes false, stop the timer and set `pendingStop =
  null`.
- **`act(what, key, meeting)`:**
  - `"start"`: unchanged.
  - `"stop"`: today's call condition, plus `client.recording` and
    `String(client.state.meeting) === meeting`.
  - `"keep"`: only if `pendingStop && pendingStop.meeting === meeting`.
    Then stop the timer, set `pendingStop = null` and `keptFor =
    ownerSince`.
- **`offer(body, action, label, key, meeting, timeoutMs)`:**
  - `then: action === "take" ? "start" : action`;
  - add `"-t", String(timeoutMs)` when `timeoutMs` is given;
  - the notifier gains `property string meeting`, and
    `onStreamFinished` calls `root.act(p.then, p.key, p.meeting)`.
- **Accepted limits:** a restart during the grace period, and a restart
  after a takeover, both fall back to the offer or to no action. No code
  is needed for them.
- **Header comment:** `Service.qml:6-8` describes the new rule instead of
  "It never stops a recording on its own".

## Steps

Test first: step 1 lands the tests, which fail against the current
`Service.qml`. Step 2 makes them pass.

### 1. Tests and fake support (coder)

Files: `shell/tests/fake-daemon.py`, `shell/tests/call-state.sh`,
`shell/tests/harness-call.qml`.

1. **`fake-daemon.py`:**
   - `state` (~61) gains `"meeting": "m-live"` when `live`.
   - New flag `--live-since=<ms-ago>`: when set, `state["since"] =
     now_ms - ms_ago`.
   - `record.start` (~583) sets `state["meeting"] = "m-%d" % n`, with a
     counter, and always casts `state`. Today it casts only `if call:`;
     keep that cast and add one when `not call`.
   - `record.stop` (~590) casts `state` after setting `recording = False`.
2. **`harness-call.qml`:** `Service { stopGraceMs: 2000 }`.
3. **`call-state.sh`:**
   - **The `notify-send` stub:** when `NOTIFY_ANSWER` is set, print it.
     It keeps logging its arguments.
   - **A `user <cmd>` helper:** a `python3 -c` one-liner that connects to
     `$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock`, sends
     `{"id":1,"cmd":"<cmd>","args":{}}`, and reads one line. It plays the
     user as a second client.
   - **The `st` helper's 4th argument (`callSince`):** pass it explicitly
     in the new cases.
   - **`viamaybe` (~82):** use `callSince` = now instead of `5000`, on
     both writes (the same value).
   - **New cases**, following the existing `run`/`fail`/`no_record`
     style. Let `now=$(date +%s%3N)`.
     - `autostop`: `--live --live-since=2000`; base `st call $$ "" $((now-10000))`; go → `st none`, sleep 3.
       Assert:
       - `grep -c 'record.stop'` = 1;
       - the notify log has `-A keep=Keep recording` and `-t 2000`.
     - `maybe-end`: same base; go → `st maybe $$ "" <same callSince>`, sleep 3.
       Assert one `record.stop`.
     - `takeover`: `--live --live-since=30000`; base callSince `now-60000`; go → `none`, sleep 1, `st call $$ "" $(date +%s%3N)`, sleep 1, `none`, sleep 3.
       Assert:
       - exactly one `record.stop`;
       - it is logged after the second `none`: check that the log line
         follows a marker the test appends to the log via
         `echo MARK >> log` before the second `none`.
     - `keep`: as `autostop` with `NOTIFY_ANSWER=keep` exported for the
       run.
       Assert:
       - no `record.stop`;
       - no `-A stop=` offer.
     - `manualnew`: as `autostop`; go → `none`, sleep 0.5, `user record.stop`, `user record.start`, sleep 3.
       Assert exactly one `record.stop` (the user's).
     - `gone`: as `autostop`; go → `st none <dead-pid>`, sleep 3.
       Assert:
       - no `record.stop`;
       - a `-A stop=Stop notes` offer.

       For the dead pid: `sh -c 'echo $$'` returns a pid that has exited.
     - `gonepending`: as `autostop`; go → `st maybe`, sleep 0.5, `st maybe <dead-pid>`, sleep 3.
       Assert:
       - no `record.stop`;
       - a `-A stop=Stop notes` offer.
   - **Existing `hangup` and `viamaybe`:** keep their assertions.
4. **Check:** `TMPDIR=/tmp devenv shell -- bash shell/tests/call-state.sh`.
   The new cases **must fail** against the current `Service.qml` (no
   `stopGraceMs` property and no auto-stop). Record which ones fail.

Traps:
- `run()` gives `go` about 9 s: keep each `go` at 6 s or less.
- `qml_bad` fails on any QML warning.
- Never use bare `git stash`.

### 2. Service.qml (codex)

File: `shell/Service.qml` only.

Implement "Decisions carried over" exactly:
- properties, `forCall()`, `stopTimer`;
- `offer`/`act` (~59-73) and the notifier (~34-46);
- the `onCallChanged` changes (~100-117);
- `Connections` on `client.onRecordingChanged` and
  `calls.onWriterGoneChanged`;
- the header comment.

Keep `react()`, `startNotes` and the IPC handler unchanged.

**Check:** `TMPDIR=/tmp devenv shell -- bash -c 'bash shell/tests/lint.sh && bash shell/tests/call-state.sh'`.
All cases are green, the step 1 cases included.

Traps:
- Notifications never carry the call title. The existing test greps
  "Secret roadmap" out of the notify log.
- Interviewer mode is unchanged for starting.
- No new `qmllint` warnings.

### 3. Integrate, review, ship (Claude)

1. Run `bash shell/tests/ci.sh` and
   `nix build .#checks.x86_64-linux.shell -L`.
2. Read-only reviews in parallel, each checking the diff against this
   plan:
   - a fresh Opus agent (this plan path plus `git diff origin/main`);
   - `codex review --base origin/main -c sandbox_mode="read-only"`;
   - agy `--mode plan`.

   Verify each finding, then fix it or refute it in the PR.
3. Open a PR linking the intent, the spec and this plan, and comment
   `@codex review`. Merge with a merge commit after CI.
4. **Deploy to p620:**
   - read the bus and post to it;
   - check that no recording is running (`ctl --agent state.get`);
   - run `AGENT_BUS_ANNOUNCED=1 nhs p620 nixarchy-meetingai`. #60's
     deploy goes first, if it is still pending.
   - Afterwards: the Omarchy shell picks up the new `Service.qml` (a
     shell reload if `nhs` does not do it), and the journal is clean.
5. **Manual:** a short real call with notes on, then hang up. "Notes stop
   in 30 s" appears, and the notes finish. **Keep recording** keeps them
   running.

## Tests

- After step 1, the new `call-state.sh` cases fail on the current code.
- After step 2, `call-state.sh`, `lint.sh` and `ci.sh` are green, and so
  is the nix shell check.

## Rollback

- **Before merge:** drop the branch.
- **After merge:** `git revert -m 1 <merge>`, bump the nixos lock again,
  and switch.
- No data or schema is touched, so a revert is complete.

## Deviations

(Recorded here during implementation, in the same commit as the code.)
