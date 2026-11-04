---
status: draft
issue: 62
author: olafkfreund
---

# Intent: Agents cannot restore a local-only interview

## Problem

Agents connected over MCP can call `session.restore` (it is on the agent
allowlist in `src/daemon/session.rs` ~4690). The local-only guard right below
it covers `transcript.get`, `notes.get`, `now.get`, `chat.send` and
`items.list`, but not `session.restore`. So an agent can restore a
local-only interview by its meeting id, or by the id of the chat linked to
that interview.

Later reads by the agent are refused, but the restore itself already has
effects the local-only promise forbids:

- It loads the interview's title, context, recap and up to 10 000 characters
  of transcript into the chat sources (`restored-meeting:<id>`), the same
  context the next answer is built from.
- It replays the interview transcript to the shell and sets the interview's
  candidate on the session.
- It resets whatever session the user had open.

The interview never leaves the machine through this path today, because
agent reads are refused afterwards. But an agent can put a local-only
interview in front of the user without the user asking for it, and the
guard's coverage depends on every later command remembering to check.

## Proposed outcome

1. An agent's `session.restore` of a local-only interview, by meeting id or
   by linked chat id, is refused with the same "This interview is
   local-only" error the other guarded commands return. Nothing changes:
   the user's open session, sources and shell stay as they were.
2. Restoring a local-only interview from the shell still works.
3. Agents can still restore other meetings and chats.

## Affected users and systems

- `src/daemon/session.rs`: the MCP guard in `handle`, and its tests.
- Agents using the MCP `session_restore` tool (`src/mcp/mod.rs` ~625): they
  get an error instead of a restore.
- Interviewers using local-only mode.

## Constraints

- Refuse before `reset_session`, so a refused call has no side effects.
- Keep the error text shared with the other guarded commands.
- No schema change. Deploy to p620 follows the usual rules (not while
  recording).

## Open questions

1. Should agents be able to restore a local-only interview at all?
   Recommended: no. The issue asks for this decision, and refusing matches
   the rule that agents cannot read a local-only interview.
