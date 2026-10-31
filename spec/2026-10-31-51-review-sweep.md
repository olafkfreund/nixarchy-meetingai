---
status: draft
issue: 51
intent: intent/2026-10-31-51-review-sweep.md
---

# Spec: review sweep — privacy gates, recordings that stop early, export overwrites

Line numbers are from main at f31e5c4. Finding ids follow the intent.

## Design

### 1. Privacy

- **A1 — the candidate survives `record.start {new:true}`**
  (`src/daemon/session.rs` `record_start` ~2029-2172, `reset_session`
  ~2962-2983):
  - Before `reset_session`, take `candidate`, `consent` and `local_only`.
    Put them back after the reset, then create the meeting row. The
    consent and local-only checks before it stay as they are.
  - `new` still ends the previous conversation; it no longer forgets who
    the interview is with.
  - Test: select a candidate, give consent, mark local-only, send
    `record.start {new:true}`. The meeting is linked to the candidate and
    `local_for(meeting)` is true.
- **A2 — MCP allowlist in the daemon** (`handle` ~4195):
  - Origin `mcp` may send only the commands `src/mcp/mod.rs` sends today:
    `chat.send`, `items.list`, `notes.get`, `notes.regenerate`, `now.get`,
    `record.start`, `record.stop`, `series.list`, `session.list`,
    `session.new`, `session.restore`, `settings.get`, `settings.set`,
    `state.get` and `transcript.get`.
  - `settings.set` from `mcp` is accepted only for the keys in
    `SETTINGS_ALLOWLIST`. That list moves from `src/mcp/mod.rs` into a
    shared constant both sides use.
  - `chat.send` from `mcp` is refused when it carries `attachments`
    (MCP never sends any).
  - The `agent` allowlist is unchanged.
  - Test: a table of refused commands (`settings.set api_key`,
    `settings.set mcp_allow_record`, `attachment.preview`, `record.tap`,
    `candidate.local_only`, `session.delete`, `quit`) and allowed ones.
  - The limit, which agy noted: a local process can leave out `origin`
    altogether. This narrows the MCP server's permissions; it isn't a
    boundary against other local programs. A comment by the gate says so.
- **A3 — `notes.regenerate` never exports**:
  - `finish_meeting_artifacts` gets an `export: bool`. Only
    `finish_recording` passes `true`.
  - `notes.regenerate` is refused for the meeting that is recording now.
  - Local-only meetings still regenerate, with the local model (decision 2).
  - Test: with an export folder set, regenerate writes no files.
- **A8 — `hide_interviews`** (~4292) also nulls `candidate` and `consent`.
  Test: extend the current hide test.
- **B1 — agy gets its prompt on stdin** (`src/ai/chat/cli.rs` `agy_args`
  ~145-161, call ~471):
  - Pass `--input-format stream-json` and write one NDJSON user message
    to stdin instead of `-p <prompt>`. The daemon already reads
    `--output-format stream-json`.
  - The message format is confirmed by a single probe with a harmless
    prompt, on the subscription login only.
  - If the probe shows stdin doesn't work, decision 3 applies: agy gets no
    profile or CV documents, and its status shows "prompt visible to local
    users".
  - Test: the argv test asserts that no argument contains the prompt text.
- **B3, B4 — no unencrypted remote endpoints**:
  - `src/ai/catalog/custom.rs` `validate` accepts `ws://` only for
    loopback hosts, using the `endpoint_allowed` rule.
  - `ProviderConfig::endpoint()` and `speech_usable_in`
    (`src/ai/catalog/providers.rs` ~170, ~219) also check
    `endpoint_allowed` when used, so a record already in the vault with a
    bad URL is treated as not configured.
  - Tests: `ws://remote` rejected, `ws://127.0.0.1` accepted; a stored bad
    URL means not usable.

### 2. Recording keeps going

- **C1 — one failed speech request is not fatal**
  (`src/speech/stt/transcription.rs` `batch_task` ~1156, 1218):
  - A chunk error retries once. If it fails again, it becomes an error row
    such as "lost speech for 2400.0–2409.3 s" (`LiveEvent::Notice` or the
    existing error-line event), and the loop goes on.
  - `NotConfigured` and `MissingKey` stay fatal.
  - Test: a fake engine that fails one chunk out of three gives two lines,
    one notice, and no `Failed`.
- **C4 — back-pressure instead of "fell behind"** (`queue_chunks` ~1108):
  - Ready chunks wait in a local `VecDeque` and enter `pending` as slots
    free up, the same way the tail loop at ~1187 works.
  - The cap is 120 s of queued audio. Over the cap, the oldest chunk
    becomes a "lost speech" notice instead of ending the recording.
  - Test: a slow fake engine with 10 chunks gives every line, in order.
- **C2 — losing system audio keeps the mic**
  (`src/speech/session/listen.rs` ~340-348):
  - A system-capture failure takes the existing
    `set_system_audio(false)` path, with a mic-only notice.
  - Only a mic failure calls `abort_captures`.
  - Test: a fake system source that stops gives a notice, and the mic
    frames keep coming.
- **C5 — device switch off the async worker** (session.rs ~2253):
  - `set_system_audio` runs in `spawn_blocking`. The session is moved into
    the closure and handed back, so access to it stays in order.
- **A5 — the recording slot is always freed** (session.rs ~2076-2114): a
  small drop guard clears `g.recording` on every error return, including
  a `JoinError`. Test: inject a failing start, then `record.start` works.
- **A9 + C8 — clean exit when the stream ends** (session.rs ~2267-2304):
  - Push failure and disconnect paths call `session.stop()`, the same as
    the normal close.
  - Push `Closed` reports nothing itself; the `Failed` event carries the
    one message.

### 3. Export keeps the user's changes

- **B2** (`src/core/export.rs` `fill`/`swap_dir`/`rm` ~287-375):
  - `meeting_exports` gets a nullable `notes_sha256` column, added with
    the existing add-column-if-missing pattern in `src/db/mod.rs`, so no
    data is lost.
  - On export: if `notes.md` exists and its hash isn't the stored one, the
    new notes are written to `notes (updated).md` and the user's file is
    left alone.
  - `screenshots/` and `documents/` only add or replace files whose names
    we generate. Other files are never deleted, and a directory is removed
    only if it is empty.
  - Tests: edit `notes.md`, re-export, and the edit survives next to the
    new file; a user file in `screenshots/` survives.

### 4. UI responsiveness

- **A4 — concurrent requests per connection**
  (`src/daemon/server.rs` ~78-99):
  - Each request is handled in its own task, and replies go through the
    existing `out` channel matched by `id`.
  - To keep ordering for commands that change the same state, such as
    `record.start` followed by `record.stop`, the state-changing commands
    of one connection run in order. Only the commands that wait a long
    time run concurrently: `chat.send`, `voice.enroll`,
    `questions.generate`, `providers.test`, `dictate.stop`, `models.list`
    and `notes.regenerate`.
  - `chat.cancel`, `record.stop` and `capture.clear` no longer wait behind
    them.
  - Test: start a slow `chat.send` against the fake provider, then
    `record.stop` on the same socket answers first.
- **C9** (`shell/Client.qml` ~190, 237, 298): while `now.turn` is -1 or -2
  and the user's question is waiting for `answer.started`, deltas are
  ignored. Test: a fake-daemon step in `shell/tests`.
- **C10** (`src/speech/stt/filter.rs` ~216): a held mic line is released
  when the open system chunk's current end passes `line.end + margin`, or
  when the chunk ends, not only at `HOLD_CAP`. Test: continuous system
  speech plus one mic line comes out within the chunk length, not 15 s
  later.

### 5. Smaller faults

- **A6:** `session_restore` looks up the meeting by id with a direct query
  (`meetings::get`, added if missing).
- **A7:** a drop guard resets `chunking`; errors are logged with
  `tracing::warn!`.
- **B5:** `generate` keeps any error after `begin_recap` and always calls
  `finish_recap` with it. A failure of `finish_recap` itself is logged.
- **B6:** docs only. Add a note in the README security section that old
  blocks of a pre-encryption database may stay on disk, and to use disk
  encryption (decision 4).
- **B7:** `recap::db()` uses a new `Database::open_existing` that unlocks
  the database without `prepare`. The daemon has already prepared it at
  startup.
- **B8:** stderr keeps draining after the 1 MiB cap and keeps the last
  2 KiB for the error message.
- **B9:** when the opencode start times out, `kill_group(child.id())` and
  `child.wait()` run.
- **C3:** with no source chosen, parec uses `@DEFAULT_MONITOR@`. The
  `pactl` lookup stays, only for the display name. The implementer checks
  on p620 that parec accepts `@DEFAULT_MONITOR@`, with
  `parec --device @DEFAULT_MONITOR@ --format s16le -n probe | head -c 1`
  and a 1 s timeout. That only reads system output, never the mic, and
  doesn't restart PipeWire. If parec refuses it, the old behaviour stays
  and C3 is reported as not done.
- **C6:** when the queue would overflow, drop or pad one frame, count it,
  and send one notice per minute, instead of aborting.
- **C7:** when an utterance is within 2 s of `LONGEST`, cut it at the
  quietest 100 ms window in that tail. Write a small local helper;
  `parakeet::split_points` is private and assumes a 30 s limit, as Codex
  noted.

## Alternatives rejected

- **A2:** authenticate MCP with a token, or give it a separate socket. That
  is more machinery than the threat needs: any local process can already
  connect to the socket anyway.
- **A4:** make every request concurrent. That risks reordering, for
  example `record.stop` overtaking `record.start`, so only the slow
  commands run concurrently.
- **B2:** never overwrite, always write a new file. Then exports nobody
  edited pile up copies.
- **B6:** zero-fill the old file. That isn't reliable on SSD or
  copy-on-write storage, so decision 4 applies.
- **C1:** unlimited retries. A dead endpoint would then never let the
  recording finish.

## Risks

- **A4** changes the order in which the shell receives replies, if the
  shell relies on it. `shell/tests/ci.sh` and `fake-daemon.py` cover the
  overlay, and the shell suite runs.
- **A2** can refuse something an existing MCP client depends on. The list
  is taken from `src/mcp/mod.rs` itself, and `tests/` gets a test for each
  tool.
- **B1:** if the stream-json input format differs between agy versions,
  agy chat breaks. The probe and a unit test for the message format cover
  this, and the fallback in decision 3 stays available.
- **B2** adds a database column. It is additive and nullable, and an old
  database gets it on open. The tests use the old-history fixture.
- **C1, C4:** "lost speech" notices replace hard stops. That is wanted,
  but a broken engine now produces many notices instead of one stop. If
  every chunk fails for 60 s, the recording stops as before.
- **C3** needs live PipeWire behaviour, and that is checked only with the
  read-only probe.
- Affected host: p620 only (deployed). Deploy after merge, announced on
  the bus, and not while recording.

## Verification

- `TMPDIR=/tmp devenv shell -- bash -c 'cargo fmt --check; cargo clippy
  --workspace --all-targets -q -- -D warnings; cargo test --workspace -q'`
  is green.
- `bash shell/tests/ci.sh` is green, and `nix flake check` is green.
- Every finding with a test listed above has one. It is written first and
  shown to fail on main.
- A fresh Opus review compares the diff with `plan/`, and Codex runs
  `review --base main`, read-only.
- After deploy on p620, the daemon is active, there are no errors in the
  journal since the restart, and `ctl --agent` and MCP-origin refusals are
  checked with `ctl`.
- Not verified live: a real Bluetooth dropout (C2) and long-recording
  drift (C6). Both are covered only by fakes.
