---
status: draft
issue: 89
author: olafkfreund
---

# Intent: An agent's question never captures the screen

## Problem

The app means agents not to take screenshots. The MCP allow-list refuses an agent's `chat.send` when it passes `screenshot` or `attachments` (`src/daemon/session.rs:5337`). Two paths get past that intent, because `chat_send` doesn't know who is asking (`src/daemon/session.rs:1777`):

- **A fresh capture.** `let fresh = args.screenshot || s.use_image_on_send;` (`:1815`). With `use_image_on_send` on, an agent's question grabs the whole screen and sends it to the model. The agent can then ask "what's on my screen?" and read the answer. The image is also stored with the meeting.
- **A waiting image.** If the owner captured a screenshot and hasn't sent it yet, `pending_capture` is taken by the next send (`:1837`), including an agent's. The owner's image goes with the agent's question, and the owner's own question no longer has it.

**How much this matters today:** `use_image_on_send` is off by default, and agents can't turn it on (`MCP_SETTINGS`, `src/mcp/mod.rs:36`). `use_image_on_send` isn't in the shell's settings as far as a search of `shell/*.qml` shows, so it would be set through Nix or `settings.set`. Whether the owner has it on hasn't been checked. A waiting image has no setting: it only needs an agent to ask at the wrong moment. The notice "An agent asked: ..." appears only after the call returns, so it doesn't warn in time.

All of this comes from reading the code on `main`. It hasn't been reproduced.

## Proposed outcome

- A question from an agent never takes a screenshot and never sends one, whatever the settings.
- An image the owner captured stays waiting for the owner's next question.
- The owner's own questions behave as they do today, including `use_image_on_send`.

## Affected users and systems

Anyone who uses MCP agents with the app. `src/daemon/session.rs` (`chat_send`, `dispatch`) and `src/daemon/protocol.rs` if a field is needed. No database, Nix option or shell change is expected. Agents' own text questions keep working.

## Constraints

- **The origin must come from the daemon, not the request.** `ChatSend` comes from the client, so an agent must not be able to claim it isn't one. `dispatch` already receives whether the origin is MCP.
- **Don't fail an agent's question because of a setting it can't see.** With `use_image_on_send` on, its question is answered as text.
- **Prove it with a test that fails without the fix,** for both paths.

## Limits this does not change

Any process running as the owner can connect to the daemon's socket and leave out `origin`, and it can read the database and export folder (the comment at `:5310` says so). This closes the path for clients that identify as agents through MCP, which is the boundary the allow-list already draws. It isn't a sandbox against local processes.

## Open questions

1. **Refuse or answer without the image?** Recommended: answer without it. The agent didn't ask for an image, and a refusal would make MCP stop working for anyone with the setting on.
2. **Should a Local-only check apply here too?** Recommended: no change. An agent's `chat.send` is already refused for a Local-only interview.
