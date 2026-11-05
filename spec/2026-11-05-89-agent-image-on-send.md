---
status: draft
issue: 89
intent: intent/2026-11-05-89-agent-image-on-send.md
---

# Spec: An agent's question never captures the screen

## Design

`dispatch` already knows whether a request came through the MCP server (`agent: bool`, from `origin == Some("mcp")`, `src/daemon/session.rs:5420`). Pass that on to the send path, and use it at the four points where an image enters a turn. No new request field, setting or error.

In `chat_send` (`:1777`), which gains `agent: bool` from its one caller (`:5624`):

1. **Empty question (`:1794`).** A waiting image only counts as something to send when not an agent, so an agent's empty question gets the normal `SendError::Empty`.
2. **Fresh capture (`:1815`).** `fresh` becomes `!agent && (args.screenshot || s.use_image_on_send)`. No capture and no capture delay for an agent.
3. **Waiting image (`:1838`).** An agent's send does not `take()` `pending_capture`. It stays waiting, and `state.capture` stays true for the owner.

In `begin_send` (`:1887`), which gains `agent: bool`:

4. **Attached images (`:2006`).** Images from `g.sources` are not added to an agent's turn. The check that the model supports images (`:1945`) is also skipped for an agent, since no image goes, so an attached image can't make an agent's question fail on a text-only model.

`begin_send`'s other callers pass `false`: the send from `chat_send` passes its own value, and automatic answers (`:2409`) behave as today.

**What an agent's turn still gets:** the question, transcript, notes and text from attached documents, as today. Earlier images don't come back through history, because a turn's images are attached only to its last message (`src/ai/chat/engine.rs:423`) and `g.conversation` holds text.

**README:** one line in "Ask your agents": an agent's question never takes or sends an image.

**Decisions from the intent:** an agent's question is answered without the image rather than refused. Local-only handling is unchanged, since an agent's `chat.send` is already refused for a Local-only interview.

## Alternatives rejected

- **Refuse an agent's question when the setting is on.** It would make MCP stop working for anyone with the setting on, over something the agent didn't ask for.
- **Strip it in the MCP server (`src/mcp/mod.rs`).** The capture happens in the daemon from a setting, so there's nothing to strip on that side.
- **Add a field to `ChatSend`.** The client would supply it, and `dispatch` already has the answer.
- **Remove images from `g.sources` for the turn.** It changes the owner's state to protect one turn. Leaving them out of that turn's list is simpler, and they stay for the owner.
- **Withhold text attachments from agents too.** The intent covers images. Text context is what an agent's question is for, and it's what the existing allow-list permits. Filed as a question for the owner if wanted, not done here.

## Risks

- **A prompt could mention an image the model doesn't get.** If `append_prompt` names attached images in the text sent to the model, an agent's answer could say it can't see the attachment. The plan checks this and fixes it only if it's so.
- **A missed caller.** `begin_send`'s new argument is required, so the compiler finds every caller.
- **An owner's flow changes by accident.** Existing tests for sending a waiting image and for the empty question cover this, and they keep passing unchanged.
- **Other local clients** aren't covered, as the intent states.
- **The owner's setting stays on.** This fix makes that safe for MCP; it doesn't change the setting.

## Verification

- New tests, each through `handle("chat.send", ..., Some("mcp"))`:
  - with `use_image_on_send` on, no capture is attempted and the turn has no images (a test seam, since tests must never run the real `grim`);
  - with a waiting image, the turn has no images, the image is still waiting, and no screenshot row is stored;
  - with an attached image, the turn has no images and `g.sources` still holds it, on a model without image support too;
  - an empty question with a waiting image returns the empty error.
- Matching tests with origin `None`: the owner's send still captures, takes the waiting image and includes the attached one.
- Each guard removed in turn makes a test fail.
- `cargo test --workspace`, clippy with `-D warnings`, `cargo fmt --check`, `nix flake check`.
- After deployment on p620, with the owner's setting still on, an MCP question stores no screenshot row and starts no `grim` (journal and database), using text that doesn't reveal anything.
