---
status: draft
issue: 96
intent: intent/2026-11-07-96-agent-image-reach.md
---

# Spec: Agents can't discard the owner's images or read what was said about them

## Design

### Part 1: an agent's reset keeps the waiting screenshot

**Narrower than the approved intent, for the owner to confirm:** only the waiting screenshot (`pending_capture`) is kept, not attached images.
- Attached images in `g.sources` aren't lost by a reset: they're saved with their conversation, and `restore_chat` adds them back when it's reopened (`src/daemon/session.rs:4472`).
- Carrying them through an agent's `session.new` would put the old session's images into a new, unrelated one, unseen, and send them with the owner's next question.
- The waiting screenshot is the only image stored nowhere else.

**Code, `src/daemon/session.rs`:**
- A small helper next to `reset_session` (:4268):
  `fn reset_session_keeping_capture(&self, g: &mut Inner, agent: bool)`
  It takes `pending_capture` when `agent`, calls `reset_session`, and puts it back.
  `reset_session` itself, and its other three callers (:3309, :4573, :5465), are unchanged.
- It's used at the three resets an agent can reach:
  - `session.new` (:6084);
  - `session_restore`'s chat branch (:4352);
  - `session_restore`'s meeting branch (:4360).
- **The shell must still show it.** On `session.reset` the shell sets `capture` to false (`shell/Client.qml:329`), and only a following `state` event turns it back on.
  - `session.new` (:6089) and the meeting branch (:4421) already emit state.
  - The chat branch returns at :4355 without doing so. Add `self.emit_state(g)` there.

### Part 2: document it, no code

As decided (option **a**). In the README's "Ask your agents" section, after "An agent's question never takes or sends an image." (`README.md:373`), add:

> An agent never gets your images. It can read the text around them: your questions, the answers and follow-up suggestions, which can name an image's file. That's what an agent can already read about anything else.

## Alternatives rejected

- **Refuse an agent's `session.new`/`session.restore` while a screenshot waits.** It blocks a normal action over the owner's unfinished question. The owner decided to keep the screenshot instead.
- **Also keep attached images.** For the reasons in Part 1.
- **Add an `agent` parameter to `reset_session`.** That changes six callers to fix three; a helper used at the three is smaller.
- **Hide text about images from agents (options b and c).** Not chosen by the owner.

## Risks

- **An agent's reset followed by the owner's question sends the kept screenshot** with that question, in the new session. That's what the owner captured it for, and the shell shows that one waits. The screenshot is stored with whatever meeting is open when it's sent, as today.
- **A meeting or chat restored by an agent now has a waiting screenshot from before.** Same as above: it waits for the owner, visibly.
- **Hosts:** p620, at the next deploy. No database, setting or Nix change.

## Verification

- **New test** `agent_reset_keeps_waiting_screenshot`: set `pending_capture`, then call `session.new` with `Some("mcp")`; `capture` is still true. Repeat for `session.restore` of a saved chat and of a saved meeting.
  - For the chat restore, subscribe to events and check a `state` event with `capture: true` follows the `session.reset`.
  - The owner's own `session.new` (origin `None`) still clears it.
  - The test fails with the helper's take-and-restore removed, and the chat branch's event check fails with the new `emit_state` removed.
- The existing `agent_leaves_waiting_image_and_its_meeting_row_for_owner` (:7418) still passes.
- The step check (`cargo test`, `clippy -D warnings`, `fmt --check` in `nix develop`), `bash shell/tests/ci.sh`, and `nix flake check`.
