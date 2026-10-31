---
status: draft
issue: 51
author: olafkfreund
---

# Intent: review sweep — privacy gates, recordings that stop early, export overwrites

## Problem

A code review by Fable over the whole repository, checked read-only by
Codex (gpt-6-sol) and agy (model not reported), found faults in five
groups. Codex confirmed 22 of 28 findings and partly confirmed 6; agy
confirmed 26 and partly confirmed 2 (A9, C10). Neither refuted any. Finding ids
(A = daemon/MCP, B = AI/core/db, C = speech/audio/shell) are kept so the spec
and plan can refer to them.

1. **Privacy promises are broken.**
   - A1: when recording starts during a call, the selected candidate is
     cleared. That covers a call starting recording automatically, and also
     the user pressing record while a call is on (agy: `shell/Client.qml`
     `startRecording` also sends `new: true`). The interview is then saved as an
     ordinary meeting: its notes go to cloud models and agents can see it.
   - A2: the daemon checks agent requests against an allowlist, but checks
     MCP requests only against a short denylist. Through MCP, an agent can
     change settings (including its own recording permission and the API
     key), read any file through attachments, switch off local-only, and
     delete meetings. Codex rates this high. agy rates it low to medium:
     any local process can connect to the socket and leave out `origin`
     altogether, so this is least privilege for the MCP server, not a wall
     against local processes.
   - A3: `notes.regenerate` writes an export even though agents are
     refused `meetings.export`.
   - A8: `state.get` shows agents the id of a local-only candidate.
   - B1: agy receives the whole prompt, CV documents included, as a
     command-line argument that any local user can read.
   - B3, B4: speech audio and API keys can go unencrypted (`ws://`,
     `http://`) to a remote host.
2. **Recordings stop by themselves.**
   - C1: one failed speech request ends the recording.
   - C4: a speech engine that falls behind for a moment ends the recording.
   - C2: system audio dropping out (headset reconnect, HDMI unplug) ends
     the microphone recording too.
   - C5: switching system audio while PipeWire is slow can make the
     microphone lose data.
   - A5: a panic while capture starts leaves the daemon saying "Already
     recording" until it is restarted.
3. **The user's own files are overwritten.**
   - B2: every re-export replaces `notes.md` and deletes files the user put
     in the export's `screenshots/` and `documents/` folders.
4. **The UI waits or shows the wrong thing.**
   - A4: requests on one connection run in order, so Stop and Cancel wait
     behind a slow request (screenshot delay, question generation).
   - C9: after a reconnect, an old answer can replace the question the user
     just asked.
   - C10: the user's own lines can appear up to 30 seconds late.
   - C8: a single disconnect is reported twice.
5. **Smaller faults.**
   - A6: meetings older than the 500 most recent cannot be restored.
   - A7, B5: rolling notes, or a recap, can get stuck, with no log line.
   - A9: the last audio is lost when the speech stream fails.
   - B6: after the one-time conversion to an encrypted database, the old
     unencrypted copy can still be recovered from free disk space.
   - B7: the database is fully reopened for every recap call.
   - B8: a CLI provider's errors beyond 1 MiB stop being read.
   - B9: a failed opencode start leaves helper processes behind.
   - C3: the system audio doesn't follow the default output when it
     changes.
   - C6: drift between the sound card's clock and the system clock can
     stop very long recordings.
   - C7: speech is cut at exactly 15 s, which can split a word.

## Proposed outcome

- **Privacy:** agents and MCP can reach only what they are meant to. An
  interview stays an interview whatever started the recording. Meeting
  content never appears in a process's command-line arguments and never
  leaves the machine unencrypted.
- **Recording:** a recording stops only when the user stops it, or when the
  microphone itself fails. A failed speech request or lost system audio
  shows a notice, and recording carries on.
- **Export:** never destroys a change the user made.
- **UI:** Stop and Cancel take effect at once.
- Every fix lands with a test that fails before the fix, where a headless
  test is possible.

## Affected users and systems

- The daemon (`src/daemon`), MCP server (`src/mcp`), speech and audio
  (`src/speech`, `src/audio`), AI providers (`src/ai`), export and database
  (`src/core`, `src/db`), and the shell (`shell/Client.qml`).
- p620, where it is deployed.
- Agents and MCP clients that use the daemon (narrower permissions).

## Constraints

- Agents and MCP stay refused voice.*, providers.* and meetings.export. The
  voice sample never appears in events, logs or exports.
- No database migration that can lose data. Exports already written stay
  readable.
- Tests are headless only: no live microphone, no PipeWire restart, no
  desktop.
- The MCP tools that exist today keep working.
- Deploy only from committed main, after an agent-bus announcement, and not
  while recording.

## Open questions

1. **A2:** which commands should MCP be allowed? My proposal is exactly
   what its tools use today, with `settings.set` limited to the MCP
   settings allowlist.
2. **A3:** should `notes.regenerate` on a local-only meeting be refused, or
   allowed with the local model (as it does today)? My proposal is to allow
   it, but never export from it.
3. **B1:** if agy has no way to read the prompt from stdin or a file,
   should agy be refused meeting content, or be run with a warning in the
   provider status? My proposal is to refuse profile and CV documents and
   show the warning.
4. **B6:** zero-filling the old file isn't a reliable erase on SSD or
   copy-on-write storage, as Codex noted. Should we document the remaining
   risk instead of adding code? My proposal is to document it only.
5. **Scope:** fix all 28 in one PR, or split them into a privacy PR (groups
   1 and 3) and a reliability PR (groups 2, 4 and 5)? My proposal is one
   branch and one PR, since you asked for one sweep.
