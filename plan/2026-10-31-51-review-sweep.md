---
status: approved
issue: 51
spec: spec/2026-10-31-51-review-sweep.md
coders:
  1: codex   # daemon privacy: session.rs gate logic, a well-scoped Rust edit
  2: codex   # daemon reliability: same file as step 1, same implementer keeps context
  3: codex   # server.rs concurrency: same team, follows steps 1-2
  4: agy     # transcription.rs: self-contained module with fakes for tests
  5: agy     # listen.rs, system_audio.rs, filter.rs: audio-side modules
  6: codex   # AI providers: cli.rs, catalog, opencode
  7: agy     # storage: export.rs, recap.rs, db/mod.rs
  8: agy     # shell/Client.qml + fake-daemon test
reviewers:
  - codex    # `codex review --base origin/main`, read-only, on the merged branch
---

# Plan: review sweep — privacy gates, recordings that stop early, export overwrites

Implements every finding in the approved spec, on branch `fix/51-review-sweep`
in worktree `/mnt/code/Source-home/GitHub/mixarchy-meetingai-review`, as one PR.
Line numbers are from main at f31e5c4 and may have drifted; find code by the
function names given.

## How it runs (owner asked for a dynamic workflow with agent teams)

- One Workflow run. Four teams work in parallel, each in its own git worktree
  (`isolation: 'worktree'`), branched from `fix/51-review-sweep`. The files the
  teams own don't overlap:

  | Team | Steps | Files it owns |
  | ---- | ----- | ------------- |
  | daemon | 1 → 2 → 3 | `src/daemon/session.rs`, `src/daemon/server.rs`, `src/mcp/mod.rs`, `tests/daemon_flow.rs` |
  | speech | 4 → 5 | `src/speech/stt/transcription.rs`, `src/speech/session/listen.rs`, `src/audio/capture/system_audio.rs`, `src/speech/stt/filter.rs` |
  | ai | 6 | `src/ai/chat/cli.rs`, `src/ai/chat/opencode.rs`, `src/ai/catalog/custom.rs`, `src/ai/catalog/providers.rs` |
  | storage+shell | 7 → 8 | `src/core/export.rs`, `src/ai/recap.rs`, `src/db/mod.rs`, `src/db/repos/exports.rs`, `shell/Client.qml`, `shell/tests/*` |

  A team that needs a change in a file it doesn't own reports it rather than
  making it, and the lead makes it in step 10.
- Each team lead is a Claude agent. It hands each step to the implementer
  named in `coders:` with `agy-implement` / `codex-implement`, following the
  `handoff` skill. It reviews the diff against its step, runs the step's
  check, and fixes small gaps itself. It returns its worktree path, the files
  it changed, the checks it ran and any deviations. It does **not** commit.
- If a handoff fails as the handoff skill defines it (non-zero exit, status
  not SUCCESS, denied actions, empty diff, HEAD moved), it retries once only
  on "connecting to sandbox server". Otherwise the team lead does the step
  itself and says so. That fallback goes in the PR, not here.
- Steps 9 and 10 belong to the session lead (Opus). It commits each team's
  work as one commit per step and merges the team worktrees into the branch.
  Then it runs the whole check, a fresh Opus review given only this plan and
  `git diff origin/main`, and the `codex review` listed in `reviewers:`.

## Steps

Each step is test first: write the test, show it fails on the unfixed code,
then fix. The step check is
`TMPDIR=/tmp devenv shell -- bash -c 'cargo fmt --check; cargo clippy --workspace --all-targets -q -- -D warnings; cargo test --workspace -q'`
unless the step says otherwise.

Traps that apply to every step:
- Agents and MCP must stay refused `voice.*`, `providers.*` and
  `meetings.export`.
- The voice sample never appears in events, logs or exports.
- No secrets in prompts or logs.
- No live mic, no PipeWire restart, no desktop.
- No API keys: subscription logins only, never
  `HUMANIZE_CODEX_BYPASS_SANDBOX`.
- No commits by implementers.

1. **Daemon privacy (A1, A2, A3, A8)**, in `src/daemon/session.rs` and
   `src/mcp/mod.rs`.
   - **A1:** in `record_start`, take `candidate`, `consent` and the
     `local_only` flag before `if new { self.reset_session(g) }`, and put
     them back right after, before the meeting row is created and before
     the candidate link (`if let Some(candidate) = &g.candidate`). Test:
     select a candidate, give consent, set local-only, send
     `record.start {new:true}`; the meeting is linked to the candidate and
     `local_for(meeting)` is true.
   - **A2:** in `handle`, origin `mcp` gets an allowlist. It may send
     exactly `chat.send`, `items.list`, `notes.get`, `notes.regenerate`,
     `now.get`, `record.start`, `record.stop`, `series.list`,
     `session.list`, `session.new`, `session.restore`, `settings.get`,
     `settings.set`, `state.get` and `transcript.get`.
     - `settings.set` from `mcp` is accepted only for keys in a shared
       `pub const MCP_SETTINGS` (moved from `SETTINGS_ALLOWLIST` in
       `src/mcp/mod.rs`; both sides use it).
     - `chat.send` from `mcp` that carries `attachments` is refused.
     - Add a one-line comment at the gate: this narrows the MCP server's
       permissions; it isn't a boundary against other local processes,
       which can leave out `origin`.
     - Leave the `agent` allowlist unchanged.
     - Test: a table of refused commands (`settings.set api_key`,
       `settings.set mcp_allow_record`, `attachment.preview`, `record.tap`,
       `candidate.local_only`, `session.delete`, `quit`) and allowed ones.
   - **A3:**
     - `finish_meeting_artifacts` takes `export: bool`. Only
       `finish_recording` passes `true`, and `notes.regenerate` passes
       `false`.
     - `notes.regenerate` is refused, with "That meeting is still
       recording.", when its id is the meeting being recorded.
     - Local-only meetings still regenerate, with the local model.
     - Test: with an export folder set and `export_meetings` true,
       regenerate writes no files.
   - **A8:** `hide_interviews` for `state.get` also nulls `candidate` and
     `consent`. Extend the existing hide test.
   - Check: the step check.
   - Traps: the existing test `export_never_contains_the_voice_sample` must
     still pass.
2. **Daemon reliability (A5, A6, A7, A9, C5, C8 daemon side)**, in
   `src/daemon/session.rs`.
   - **A5:** a drop guard clears `g.recording` on every error return of
     `record_start`, including the `JoinError` from `spawn_blocking`.
     Test: an injected failing start, then a second `record.start` isn't
     "Already recording."
   - **A6:** `session_restore` finds the meeting by id with a direct query.
     Add `meetings::get(c, id)` in `src/db/repos/meetings.rs` if it's
     missing. That is the only file outside the team's set it may touch,
     and only to add this function.
   - **A7:** a drop guard resets `chunking`, and the chunk error is logged
     with `tracing::warn!`.
   - **A9, C8:** on push failure and STT disconnect in `recording_task`,
     call `session.take().map(|mut s| s.stop())` the way the normal close
     does. Push `Closed` reports nothing itself; the `Failed` event carries
     the one message.
   - **C5:** `set_system_audio` runs inside `tokio::task::spawn_blocking`.
     Move the `ListenSession` into the closure and get it back from the
     join, so access stays in order.
   - Check: the step check.
3. **Concurrent slow requests (A4)**, in `src/daemon/server.rs`.
   - Per connection, `chat.send`, `voice.enroll`, `questions.generate`,
     `providers.test`, `dictate.stop`, `models.list` and `notes.regenerate`
     run in a spawned task and reply through `out` by `id`. All other
     commands stay in order on the connection loop.
   - Test (in `tests/daemon_flow.rs`): a `chat.send` against a slow fake
     provider, then `record.stop` on the same socket gets its reply first.
   - Check: the step check, then `bash shell/tests/ci.sh`.
4. **Speech requests (C1, C4, C7)**, in `src/speech/stt/transcription.rs`.
   - **C1:** in `batch_task`, a chunk error retries once, then emits a
     "lost speech for A–B s" notice, using the existing notice or
     error-line event, and continues.
     - `NotConfigured` and `MissingKey` stay fatal.
     - Also fatal: no successful chunk for 60 s while errors keep coming.
     - Test: a fake engine that fails one chunk in three gives the other
       lines, one notice, and no `Failed`.
   - **C4:** `queue_chunks` keeps ready chunks in a local `VecDeque` and
     feeds `pending` as slots free up, like the tail loop. Cap: 120 s of
     queued audio; over it, the oldest chunk becomes a lost-speech notice.
     Test: a slow fake engine with 10 chunks gives every line in order.
   - **C7:** within 2 s of `LONGEST`, cut at the quietest 100 ms window of
     that tail, with a small local helper. Don't use
     `parakeet::split_points`: it's private and assumes 30 s. Test: a tone
     with a gap at 14 s is cut at the gap.
   - Check: the step check.
5. **Capture (C2, C3, C6, C10)**.
   - **C2:** in `src/speech/session/listen.rs`, a system-capture failure
     (`dropped() > 0 || !is_running()`) takes the
     `set_system_audio(false)` path with the mic-only notice. Only a mic
     failure calls `abort_captures`. Test: a fake system source that stops
     gives a notice, and mic frames keep coming.
   - **C6:** same file. When the queue would overflow, or when it lags,
     drop or pad one frame, count it, and send at most one notice per
     minute, instead of aborting. Test: a feed running 1% fast for
     simulated minutes never aborts.
   - **C3:** in `src/audio/capture/system_audio.rs`, with no source chosen,
     parec gets `--device @DEFAULT_MONITOR@`. The `pactl` lookup stays,
     only for the display name.
     - Probe it once with
       `timeout 1 parec --device @DEFAULT_MONITOR@ --format s16le --raw | head -c 1 | wc -c`.
       That reads system output only, never the mic.
     - If it doesn't print 1, keep the old behaviour and report C3 as not
       done.
     - Unit test: the argv builder.
   - **C10:** in `src/speech/stt/filter.rs`, a held mic line is released
     when the open system chunk's current end passes `line.end + margin`,
     or when the chunk closes, not only at `HOLD_CAP`. Test: continuous
     system speech plus one mic line comes out within the chunk length.
   - Check: the step check.
6. **AI providers (B1, B3, B4, B8, B9)**.
   - **B1:** in `src/ai/chat/cli.rs`, agy gets `--input-format stream-json`
     and one NDJSON user message on stdin, replacing `-p <prompt>`.
     - Confirm the message format first with one probe:
       `printf '%s\n' '<message>' | env -u GEMINI_API_KEY -u GOOGLE_API_KEY agy --mode plan --output-format stream-json --input-format stream-json`
       with the prompt "reply with the word ok".
     - If no format works, apply the fallback: agy gets no profile or CV
       documents, its status says "prompt visible to local users", and the
       deviation is reported.
     - Test: the argv test asserts that no argument contains the prompt
       text.
   - **B8:** same file. stderr keeps draining after 1 MiB, and the last
     2 KiB are kept for the error message.
   - **B3:** in `src/ai/catalog/custom.rs` `validate`, `ws://` is accepted
     only for loopback hosts, using the `endpoint_allowed` rule.
   - **B4:** in `src/ai/catalog/providers.rs`, `endpoint()` and
     `speech_usable_in` also require `endpoint_allowed`. Tests:
     `ws://remote` rejected, `ws://127.0.0.1` accepted; a stored bad URL
     isn't usable.
   - **B9:** in `src/ai/chat/opencode.rs`, when the port isn't seen within
     30 s, call `kill_group(child.id())` and then `child.wait()`.
   - Check: the step check.
   - Traps: argv must never include anything with "dangerously". Keep the
     existing private-HOME allowlist for agy.
7. **Storage (B2, B5, B7)**.
   - **B2:** `meeting_exports` gets a nullable `notes_sha256 TEXT` column,
     added in `src/db/mod.rs` with the existing add-column-if-missing
     pattern (`pragma_table_info`).
     - `src/core/export.rs`: if `notes.md` exists and its sha256 isn't the
       stored one, write `notes (updated).md` and leave `notes.md` alone.
       Store the hash of whatever file was written.
     - `screenshots/` and `documents/` only add or replace files whose
       names we generate. Never delete other files, and remove a
       directory only if it's empty.
     - Tests: edit `notes.md`, re-export, and the edit survives beside
       `notes (updated).md`; a user file in `screenshots/` survives; the
       old-history fixture opens and gets the column.
   - **B5:** in `src/ai/recap.rs`, any error after `begin_recap` is passed
     to `finish_recap`. A failure of `finish_recap` itself is logged.
     Test: a forced question-bank error leaves the status failed, not
     pending.
   - **B7:** add `Database::open_existing` in `src/db/mod.rs`. It unlocks
     through `encryption::open` without `prepare`, and `recap::db()` uses
     it. Test: it opens a prepared DB and reads.
   - Check: the step check.
   - Traps: no migration that can lose data; exports already written stay
     readable.
8. **Shell (C9)**, in `shell/Client.qml`.
   - While `now.turn` is -1 or -2 and the user's question is waiting for
     `answer.started`, `answer.delta` events are ignored.
   - Test: add a step to `shell/tests/fake-daemon.py` and the smoke test
     covering reconnect during a stream, then a new ask.
   - Check: `bash shell/tests/ci.sh`, headless.
9. **Docs (B6)**, by the session lead. Add a note to the security section
   of `README.md`: after the one-time conversion to an encrypted database,
   old blocks of the unencrypted file may stay on disk, so use disk
   encryption. Check: none.
10. **Integrate and verify**, by the session lead.
    - Commit each team's worktree as one commit per step, citing the step.
      Merge the commits into `fix/51-review-sweep`, and record any
      deviation in this plan in the same commit.
    - Make the cross-team edits teams reported.
    - Run the step check, `bash shell/tests/ci.sh` and `nix flake check`.
    - Then a fresh Opus review given only this plan path and
      `git diff origin/main`, and
      `codex review --base origin/main -c sandbox_mode="read-only"`. Fix
      what they confirm, and re-run the checks.
    - Open the PR linking intent, spec and plan, and saying which steps
      Codex, agy or a team lead did. Merge with a merge commit after CI.
    - Deploy to p620 per the repo rules: bus announcement, nixos worktree
      plus PR for the lock bump, switch from committed main, not while
      recording.
    - After deploy: the daemon is active, the journal is clean since the
      restart, and the MCP-origin refusals are checked with `ctl`.

## Tests

- The step check is green after every step and after integration.
- `bash shell/tests/ci.sh` and `nix flake check` are green.
- Every new test failed before its fix. Teams report this.

## Rollback

- Before merge: drop the branch.
- After merge: `git revert -m 1 <merge>` in this repo, and a revert of the
  nixos lock bump, then a switch from committed main.
- The `notes_sha256` column is nullable and ignored by older builds, so a
  rollback leaves the database usable.
