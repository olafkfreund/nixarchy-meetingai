---
status: approved
issue: 62
intent: intent/2026-11-04-62-mcp-restore-local-only.md
---

# Spec: Restoring a session cannot break a local-only interview

## Design

### S1. A restored chat brings its candidate back (intent outcome 1)

`src/db/repos/candidates.rs` gains `for_conversation(c, conversation)`. It
returns the candidate linked to a chat by either route. If both routes
link the chat, and to different candidates, a local-only candidate wins, so
the lookup can never switch local-only off:

```sql
SELECT c.id FROM candidates c
WHERE c.meeting_id IN
        (SELECT meeting_id FROM meeting_conversations WHERE conversation_id = ?1)
   OR c.id IN
        (SELECT candidate_id FROM candidate_conversations WHERE conversation_id = ?1)
ORDER BY c.local_only DESC
LIMIT 1
```

`restore_chat` (`src/daemon/session.rs` ~3459) replaces its inline
`meeting_conversations` lookup with this call and passes the id to
`set_candidate` as before. `set_candidate` then sets `local_only` from the
candidate. A chat with no candidate gets `None`, as today.

This is the only change on the user's path. The user may still restore a
local-only prep chat; it now comes back local-only.

### S2. The agent guard (intent outcomes 2 and 3)

Only MCP reaches these commands: the `--agent` allowlist (`session.rs`
~4671) has no `session.restore`, `session.new` or `notes.regenerate`.

The check must run under the same `inner` lock as the change it guards. A
check in `handle` releases the lock before `dispatch` takes it again, which
leaves a window for another request to change the open session. So
`handle` passes `for_agent = origin == Some("mcp")` into these three
commands. Each one checks right after it takes the lock (after
`require_idle`, before `reset_session`):

- `session.restore {id}`: refused if `id` is in
  `candidates::local_only_ids` (local-only meetings, their linked chats and
  prep chats).
- `session.restore` and `session.new`: refused if `self.local_now(&g)`, so
  an open local-only interview cannot be replaced.
  - `local_now` covers a live, finishing or restored local-only meeting.
  - It also covers an idle prep session through the `local_only` flag that
    `set_candidate` sets.
- `notes.regenerate {meeting}`: refused if that meeting's candidate is
  local-only (`candidates::for_meeting`).

All three return the shared `"This interview is local-only"`. A refused call
returns before any state change.

### S3. Tests (in `session.rs` tests)

1. A prep chat for a local-only candidate is linked through
   `candidate_conversations` only. Restoring it from the shell (no origin)
   sets the candidate, `local_only` is true, and the chain is local only.
   This test fails before S1.
2. From MCP, `session.restore` is refused, and the session is unchanged
   (`state.get` before and after), for:
   - the meeting id;
   - the linked chat id;
   - the prep chat id.
3. With a local-only interview open (idle), MCP `session.new` and
   `session.restore` of an ordinary meeting are refused.
4. MCP `notes.regenerate` of a local-only meeting is refused.
5. The same calls succeed for an ordinary meeting and chat, from MCP and from
   the shell.
6. From the shell, all of these still work:
   - `session.new` while a local-only interview is open;
   - restoring a local-only meeting;
   - `notes.regenerate` of a local-only meeting.
7. A chat linked to an ordinary candidate through one route and a local-only
   candidate through the other restores as local-only.

## Alternatives rejected

- **Hiding the error by pretending the id does not exist.** It would avoid
  confirming that a local-only interview exists. But `session.list` already
  hides those ids from agents, so an agent can only hold one by guessing or
  from an earlier leak. The shared message matches the other guarded
  commands.
- **Refusing `session.restore` from MCP altogether.** Agents legitimately
  restore ordinary meetings (`src/mcp/mod.rs` `session_restore`).
- **Fixing gap 1 by refusing `chat.send` when a restored chat has a
  local-only candidate.** It treats the symptom: the chain, consent and the
  assessment guard all hang off the candidate, so restoring the candidate is
  the root fix.

## Risks

- **Agents lose `session.new` while a local-only interview is open.** This is
  intended. The user ends the interview session first.
- **A chat linked by both routes to different candidates.** Nothing in the
  repositories prevents it. The lookup prefers the local-only candidate, and
  S3 test 7 covers it.

## Review record

| Finding | From | Verdict |
|---|---|---|
| `UNION … LIMIT 1` can pick an ordinary candidate and switch local-only off | Codex | confirmed → local-only wins (`ORDER BY local_only DESC`), test 7 |
| A guard in `handle` drops the lock before `dispatch` resets the session | Codex | confirmed → checks inside the locked command, via `for_agent` |
| Tests miss the shell paths the intent keeps | Codex | confirmed → test 6 |

## Verification

- `cargo test --workspace`, including the S3 tests (test 1 fails before S1).
- `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `nix flake check -L --no-build`.
- Manual run on p620: create a local-only candidate, chat before recording,
  start a new session, and restore the prep chat. The state shows the
  candidate, and an MCP `session_restore` with that chat id is refused.
