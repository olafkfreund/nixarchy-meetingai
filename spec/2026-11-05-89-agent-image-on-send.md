---
status: approved
issue: 89
intent: intent/2026-11-05-89-agent-image-on-send.md
---

# Spec: An agent's question never captures the screen

## Design

`dispatch` already knows whether a request came through the MCP server (`agent: bool`, from `origin == Some("mcp")`, `src/daemon/session.rs:5420`). Pass that on to the send path, and use it at the five points where an image or its details enter a turn. No new request field, setting or error.

In `chat_send` (`:1777`), which gains `agent: bool` from its one caller (`:5624`):

1. **Empty question (`:1794`).** A waiting image only counts as something to send when not an agent, so an agent's empty question gets the normal `SendError::Empty`.
2. **Fresh capture (`:1815`).** `fresh` becomes `!agent && (args.screenshot || s.use_image_on_send)`. No capture and no capture delay for an agent.
3. **Waiting image (`:1838`).** An agent's send does not `take()` `pending_capture`. It stays waiting, and `state.capture` stays true for the owner.

In `begin_send` (`:1887`), which gains `agent: bool`:

4. **Attached images (`:2006`).** Images from `g.sources` are not added to an agent's turn. The check that the model supports images (`:1945`) is also skipped for an agent, since no image goes, so an attached image can't make an agent's question fail on a text-only model.

5. **Attached image details (`system_prompt`, `:1727`).** `append_prompt` sends every source's name and `"image_attached": true` to the model (`src/core/context.rs:80`), so an agent's turn would still learn an image's filename. `system_prompt` gains `agent: bool` and, for an agent, passes `append_prompt` only the sources with no image. `begin_send` also passes it to the follow-up suggestions (`spawn_follow_ups`), which an agent's reply collects (`src/mcp/mod.rs`).

`begin_send`'s other callers pass `false`: automatic answers (`:2409`) behave as today, and so does the test at `:9886`. The send from `chat_send` passes its own value.

**A test seam for the capture.** `chat_send` calls `grab_screen` directly (`:1823`), and tests must never run the real `grim`. Add a `#[cfg(test)]` capture field on `Daemon` that returns an image and counts its calls, in the style of `auto_capture`, used only by this call. It's needed to show both that an agent's send doesn't capture and that the owner's still does.

**What an agent's turn still gets:** the question, transcript, notes and text from attached documents, as today. Earlier images don't come back through history: the encoder takes images only from `TurnContext.images`, for the last message (`src/ai/chat/engine.rs:397`, `:423`), and never reads `Message.images`.

**README:** one line in "Ask your agents": an agent's question never takes or sends an image.

**Decisions from the intent:** an agent's question is answered without the image rather than refused. Local-only handling is unchanged, since an agent's `chat.send` is already refused for a Local-only interview.

## Alternatives rejected

- **Refuse an agent's question when the setting is on.** It would make MCP stop working for anyone with the setting on, over something the agent didn't ask for.
- **Strip it in the MCP server (`src/mcp/mod.rs`).** The capture happens in the daemon from a setting, so there's nothing to strip on that side.
- **Add a field to `ChatSend`.** The client would supply it, and `dispatch` already has the answer.
- **Remove images from `g.sources` for the turn.** It changes the owner's state to protect one turn. Leaving them out of that turn's list is simpler, and they stay for the owner.
- **Withhold text attachments from agents too.** The intent covers images. Text context is what an agent's question is for, and it's what the existing allow-list permits. Filed as a question for the owner if wanted, not done here.

## Risks

- **A missed caller.** `begin_send`'s new argument is required, so the compiler finds every caller.
- **An owner's flow changes by accident.** Existing tests cover the owner's waiting image and empty question, and they keep passing unchanged. Tests for the owner's capture and attached image are new; see Verification.
- **Other local clients** aren't covered, as the intent states.
- **The owner's setting stays on.** This fix makes that safe for MCP; it doesn't change the setting.

## Verification

- New tests, each through `handle("chat.send", ..., Some("mcp"))` against the existing `provider_server` (`:6733`), asserting on the request body the provider receives:
  - with `use_image_on_send` on and a non-zero `capture_delay_seconds`, the capture seam isn't called, the turn starts without waiting, and the request has no image;
  - with a waiting image, the request has no image, `state.capture` stays true, no screenshot row is stored, and the owner's next send still takes and sends it;
  - with an attached image, the request has no image, filename or `image_attached`, `g.sources` still holds it, and it works on a model without image support; a text attachment's content is still there;
  - an empty question with a waiting image returns the empty error.
- Matching tests with origin `None`: the owner's send still captures, takes the waiting image and includes the attached one.
- Each guard removed in turn makes a test fail.
- `cargo test --workspace`, clippy with `-D warnings`, `cargo fmt --check`, `nix flake check`.
- After deployment on p620, with the owner's setting still on, an MCP question stores no screenshot row and starts no `grim` (journal and database), using text that doesn't reveal anything.
