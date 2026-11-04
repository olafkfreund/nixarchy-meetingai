---
status: approved
issue: 62
author: olafkfreund
---

# Intent: Restoring a session cannot break a local-only interview

## Problem

Issue #62 found that agents (MCP) can call `session.restore` on a local-only
interview. The Codex and agy reviews of the first draft found a wider
problem: restoring a session can switch local-only off, for the user as well
as for agents. Three gaps:

1. **A restored prep chat loses its candidate (user and agents).**
   - A chat started for a candidate before recording is linked through
     `candidate_conversations` (`begin_send`, `session.rs` ~1650).
   - `restore_chat` (`session.rs` ~3459) looks up the candidate only
     through `meeting_conversations`. So restoring that prep chat sets no
     candidate, and `local_only` stays false after `reset_session`.
   - The next `chat.send` (from the user, or from an agent, since the
     local-only guard now passes) can go to a cloud model with the prep
     chat's history and saved sources, such as the candidate's CV.
   - This breaks the promise that a local-only interview never leaves the
     machine.

2. **Agents can restore a local-only interview.** `session.restore` is on the
   MCP allowlist (`session.rs` ~4690) but not in the local-only guard
   (~4730). A restore can be by meeting id, by a linked chat id, or by a
   prep chat id; the MCP tool passes any of these as `meeting`. It:
   - resets the user's session;
   - selects the chat's saved profile;
   - loads the chat's saved sources and history;
   - for a meeting id, loads the interview's title, context (1 000
     characters), recap (3 000) and transcript excerpt (10 000) into the chat
     sources, and replays up to 500 lines to the shell.

   Later agent reads are refused. But the agent has put the interview on the
   user's screen and into the answer context without being asked.

3. **Agents can disturb an open local-only interview.** While a local-only
   interview is open but not recording, an agent can call `session.new` or
   `session.restore` with another id, which clears it. `notes.regenerate`
   with a local-only meeting id re-titles and re-summarises the interview
   (on a local model, but uninvited). Neither is guarded.

## Proposed outcome

1. Restoring any chat linked to a local-only candidate, whether through a
   meeting or as a prep chat, brings the candidate back. Local-only is then
   on, for the user and for agents alike.
2. From agents (MCP and `--agent`), `session.restore` is refused with the
   shared "This interview is local-only" error when the id is a local-only
   interview's meeting or any chat linked to it. `candidates::local_only_ids`
   already lists exactly these ids. A refused call changes nothing.
3. From agents, `session.new` and `session.restore` are refused while a
   local-only interview is the open session, and `notes.regenerate` is
   refused for a local-only meeting.
4. The user can still do all of these from the shell. Agents can still
   restore, create and regenerate ordinary sessions.

## Affected users and systems

- `src/daemon/session.rs`: `restore_chat`, the agent guard in `handle`, and
  their tests. Possibly `src/db/repos/candidates.rs` for a "candidate of
  this chat" lookup.
- Agents using the MCP tools `session_restore`, `session_new` and
  `notes_regenerate` (`src/mcp/mod.rs`): they get an error in these cases.
- Interviewers using local-only mode.

## Constraints

- Refuse before `reset_session`, so a refused call has no side effects.
- Reuse the existing error text and `local_only_ids`. No schema change.
- The fix to gap 1 must not change restore for chats that have no
  candidate.
- Deploy to p620 follows the usual rules (not while recording).

## Out of scope

- `record.stop` from agents during a local-only interview. Stopping ends
  capture and sends nothing anywhere; `mcp_allow_record` deliberately
  gates only starting.
- `settings.set meeting_mode` from agents during a live interview. It
  changes behaviour (the question cover) but not where data goes. If wanted,
  it gets its own issue.

## Open questions

1. Should agents be able to restore a local-only interview at all?
   Recommended: no (outcome 2). It matches the rule that agents cannot read
   one.
2. Should agents be refused from replacing an open local-only interview
   (outcome 3)? Recommended: yes. The interview is the user's working
   session, and an agent switching away from it is the same intrusion as
   restoring into it.
