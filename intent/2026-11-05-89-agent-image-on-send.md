---
status: draft
issue: 89
author: olafkfreund
---

# Intent: An agent's question never captures the screen

## Problem

The app means agents not to take screenshots. The MCP allow-list refuses an agent's `chat.send` when it passes `screenshot` or `attachments` (`src/daemon/session.rs:5337`). Three paths get past that intent, because `chat_send` doesn't know who is asking (`src/daemon/session.rs:1777`):

- **A fresh capture.** `let fresh = args.screenshot || s.use_image_on_send;` (`:1815`). With `use_image_on_send` on, an agent's question grabs the whole screen and sends it to the model. The agent can then ask "what's on my screen?" and read the answer. The image is also stored with the meeting, if one is open.
- **A waiting image.** If the owner captured a screenshot and hasn't sent it yet, `pending_capture` is taken by the next send (`:1838`), including an agent's. The owner's image goes with the agent's question, is stored under that question, and the owner's own question no longer has it. An agent's empty question also passes while an image waits (`:1794`), and becomes "Describe what this screenshot shows".
- **An attached image.** An image the owner attached stays in `g.sources` for later turns, and `begin_send` adds every image there to the model request (`:1812`, `:2005`), including an agent's.

**How much this matters today:** `use_image_on_send` is off by default, and agents can't change it (`MCP_SETTINGS`, `src/mcp/mod.rs:36`), but **it is on for the owner's p620 today** (`settings.get`, 2026-11-05). It isn't offered in the shell (no match in `shell/*.qml`). So any agent question there captures the screen now. The other two paths need no setting: only an agent asking while an image is waiting or attached. The notice "An agent asked: ..." appears only after the call returns, so it doesn't warn in time.

The paths come from reading the code on `main`, confirmed by Codex and agy reading it too. They haven't been reproduced.

## Proposed outcome

- A question from an agent never takes a screenshot and never sends an image of any kind, whatever the settings: not a fresh capture, a waiting one or an attached one.
- An image the owner captured or attached stays available for the owner's next question.
- An agent's empty question is refused as empty, even while an image waits.
- The owner's own questions behave as they do today, including `use_image_on_send`.

## Affected users and systems

Anyone who uses MCP agents with the app. `src/daemon/session.rs` (`chat_send`, `begin_send`, `dispatch`). No database, Nix option or shell change is expected. Agents' own text questions keep working.

## Constraints

- **Use the origin check that exists; add no new request field.** `dispatch` already receives `origin == Some("mcp")` (`:5420`). The origin is itself sent by the client (`src/mcp/mod.rs:90`), so this holds for the MCP server, not for an arbitrary local process.
- **Don't fail an agent's question because of a setting it can't see.** With `use_image_on_send` on, its question is answered as text.
- **Prove it with a test that fails without the fix,** for both paths.

## Limits this does not change

Any process running as the owner can connect to the daemon's socket and leave out `origin`, and it can read the database and export folder (`:5314` notes the first part). This closes the path for clients that identify as agents through MCP, which is the boundary the allow-list already draws. It isn't a sandbox against local processes.

## Open questions

1. **Refuse or answer without the image?** Recommended: answer without it. The agent didn't ask for an image, and a refusal would make MCP stop working for anyone with the setting on.
2. **Should a Local-only check apply here too?** Recommended: no change. An agent's `chat.send` is already refused for a Local-only interview.
