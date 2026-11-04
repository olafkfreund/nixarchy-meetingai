---
status: approved
issue: 62
spec: spec/2026-11-04-62-mcp-restore-local-only.md
coders:
  1: codex   # the candidate lookup decides whether local-only is on: small, privacy-critical SQL
  2: codex   # the agent guard must sit under the same lock as the reset: small, tricky ordering
reviewers:
  - codex    # read-only `codex review --base origin/main`
  - agy      # read-only `agy --mode plan` of the diff against this plan
---

# Plan: Restoring a session cannot break a local-only interview

This plan implements the approved spec on `fix/62-mcp-restore-local-only`.

## Decisions carried over from the approved spec

- **S1.** `candidates::for_conversation(c, conversation) -> Result<Option<String>>`
  returns the candidate linked to a chat through `meeting_conversations` (via
  the candidate's `meeting_id`) or through `candidate_conversations`. When
  both link it, a local-only candidate wins:

  ```sql
  SELECT c.id FROM candidates c
  WHERE c.meeting_id IN
          (SELECT meeting_id FROM meeting_conversations WHERE conversation_id = ?1)
     OR c.id IN
          (SELECT candidate_id FROM candidate_conversations WHERE conversation_id = ?1)
  ORDER BY c.local_only DESC
  LIMIT 1
  ```

  `restore_chat` uses it in place of its inline `meeting_conversations`
  query. This applies on every path, the shell included.
- **S2.** These checks apply only to calls from MCP, and run under the same
  `inner` lock as the change, after `require_idle` and before
  `reset_session`:
  - `session.restore {id}` is refused if `id` is in
    `candidates::local_only_ids`;
  - `session.restore` and `session.new` are refused if `self.local_now(&g)`;
  - `notes.regenerate {meeting}` is refused if `candidates::for_meeting` is
    local-only.

  All return `"This interview is local-only"`. The `--agent` allowlist
  already excludes these commands and stays unchanged.
- **S3.** Tests:
  1. a prep chat restored from the shell is local-only;
  2. MCP restore of the meeting, its linked chat and the prep chat is
     refused, with the state unchanged;
  3. MCP `session.new` and restore of an ordinary meeting are refused while
     a local-only interview is open;
  4. MCP `notes.regenerate` of a local-only meeting is refused;
  5. ordinary sessions succeed from MCP and from the shell;
  6. the shell keeps `session.new`, local-only restore and
     `notes.regenerate`;
  7. a cross-linked chat restores as local-only.

## How it runs

- **Step check:**

  ```bash
  cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check
  ```

- One commit per step, `fix(…): … (#62, plan step N)`.
- Each new test fails before its change.
- Traps:
  - `Inner` is a `parking_lot` mutex; never hold it across `.await`;
  - `notes.regenerate` spawns its work, so its check must come before
    `tokio::spawn`.

## Steps

### 1. A restored chat brings its candidate back (codex)

1. `src/db/repos/candidates.rs`: add `for_conversation` next to
   `for_meeting` (~128), with the SQL above. Add a repo test covering:
   - a prep chat only;
   - a meeting chat only;
   - a cross-link to an ordinary and a local-only candidate, where
     local-only wins;
   - an unlinked chat, which gives `None`.
2. `src/daemon/session.rs` `restore_chat` (~3459–3474): replace the inline
   query and `for_meeting` with
   `candidates::for_conversation(c, id).map_err(|e| e.to_string())?`, then
   call `set_candidate` as now.
   - A database error fails the restore. It must never become `None`,
     because `set_candidate(None)` switches local-only off.
   - `for_conversation` uses `.optional()?`, like `for_meeting`, so "not
     linked" is `Ok(None)`.
3. Two daemon tests.
   - **S3.1.** Create a local-only candidate and a prep chat linked only
     through `candidate_conversations`. Start a new session and restore the
     chat with `handle(..., None)`.
   - **S3.7.** Link a chat to an ordinary candidate's meeting
     (`meeting_conversations`) and to a local-only candidate
     (`candidate_conversations`), then restore it the same way.

   For both, assert:
   - `state.get` shows the local-only candidate;
   - `d.local_only.load(Ordering::Relaxed)` is true;
   - `d.chain(&settings, false, d.local_now(&d.inner.lock()))` holds only
     `ollama`.

   Do **not** copy the literal `true` from
   `interviewer_consent_local_only_and_mcp_refusal` (~7380). It would pass
   even if restore never set local-only.

Check: the step check. The daemon test fails on `main`.

### 2. The agent guard under the lock (codex)

1. `session.rs` `dispatch` (~4832) gains `agent: bool`.
   - `handle` passes `origin == Some("mcp")` at ~4787.
   - The early non-agent call at ~4659 passes `false`.
2. `session.restore` (~4916) passes `agent` into `session_restore(id,
   agent)` (~3353). After `require_idle`, before `conversations::
   require_conversation` and `reset_session`, when `agent` is set, refuse
   if `local_only_ids` contains `id` or `self.local_now(g)`. A
   `local_only_ids` error is propagated (`map_err(|e| e.to_string())?`, as
   `session_list` does at ~3519), never treated as "not local-only".
3. `session.new` (~4843): after `require_idle` and before `known_series`
   and `candidates::get`, when `agent` is set, refuse if `self.local_now(&g)`.
4. `notes.regenerate` (~5042): at the **top** of its existing lock block,
   before the "still recording" check, when `agent` is set, refuse if
   `candidates::for_meeting(...).map_err(|e| e.to_string())?` gives a
   local-only candidate. A live local-only interview then gets the
   local-only error, not the recording one.
5. Tests S3.2–S3.6, in the style of
   `interviewer_consent_local_only_and_mcp_refusal`.
   - Calls use `handle(cmd, args, Some("mcp"))` and `None` for the shell.
   - For refusals, compare `state.get` before and after.
   - **S3.5** covers each command from both origins:
     - `session.restore` of an ordinary meeting;
     - `session.restore` of an ordinary chat;
     - `session.new`;
     - `notes.regenerate` of an ordinary meeting.
   - A `notes.regenerate` that succeeds spawns `finish_meeting_artifacts`.
     Wait for it as `regenerating_notes_never_exports_and_waits_for_the_recording`
     (~8178) does: wait for `items.updated`, then
     `drop(d.notes.lock().await)`. Or use a meeting with no lines.

Check: the step check. The tests fail on `main`.

### 3. Review and ship (Claude)

1. Read-only reviews in parallel:
   - a fresh Opus agent, given this plan path and `git diff origin/main`;
   - `codex review --base origin/main -c sandbox_mode="read-only"`;
   - `agy --mode plan`.

   Fix the confirmed findings and re-run the step check and
   `nix flake check -L --no-build`.
2. Manual check on p620, as in the spec's verification: restore a
   local-only prep chat and see the candidate in the state; MCP
   `session_restore` of that chat is refused.
3. Open a PR linking intent, spec and plan, and merge with a merge commit
   after CI.
4. Deploy with the usual rules: announce on the bus, bump the nixos lock by
   PR, switch from committed main, **not while recording**.

## Tests

- The step check is green after each step.
- `nix flake check -L --no-build` is green.

## Rollback

`git revert -m 1 <merge>`, revert the nixos lock bump, then switch. There is
no schema change.

## Review record (draft plan)

| Finding | From | Verdict |
|---|---|---|
| `.ok().flatten()` turns a database error into "no candidate" and switches local-only off | Codex, agy | confirmed → propagate errors in steps 1 and 2 |
| Step 1.3 says S3.7 but describes only the prep chat | Codex, agy | confirmed → separate S3.7 daemon test |
| Copying the `chain(…, true)` assertion proves nothing | agy | confirmed → use `local_now`, plus the flag |
| The regenerate check after the recording check returns the wrong error | agy | confirmed → check first |
| S3.5 is vague; a successful regenerate leaves a task running | agy | confirmed → spelled out, plus how to wait |

agy ran without the `herdr-run` wrapper. Earlier runs through it hung.

## Deviations

(Recorded here during implementation, in the same commit as the code.)
