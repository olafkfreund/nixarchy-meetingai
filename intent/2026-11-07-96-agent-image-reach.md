---
status: draft
issue: 96
author: olafkfreund
---

# Intent: Agents can't discard the owner's images or read what was said about them

## Problem

#89 stopped agents from taking or receiving images. Its final review found two things it left alone.

**1. An agent can discard the owner's waiting screenshot and attachments.** MCP offers `session_new` and `session_restore` (`src/mcp/mod.rs:642`, `:654`), and the agent allow-list lets both through (`src/daemon/session.rs:5895`). Both reset the session (`reset_session`, `src/daemon/session.rs:4268`), which sets `pending_capture = None` and clears `sources`. So a screenshot the owner took for their next question, or a file they attached, disappears with no notice. Both commands are refused while recording (`require_idle`), so this happens between meetings.

Starting a new session or reopening a meeting is a legitimate thing for an agent to do, and both clear far more than images: the conversation, the transcript view, the meeting. Only the images can't be got back: a waiting screenshot isn't stored anywhere else.

**2. An agent can learn about images from text.** From the #89 review, not reproduced:
- After the owner asks about an image, follow-up suggestions are generated with the image's source list in the prompt (`spawn_follow_ups`, `src/daemon/session.rs:2336`, calling `system_prompt` as a non-agent turn). A suggestion can name the file, and `state.get` returns suggestions to agents (`:5833`).
- An answer describing the image stays in the conversation, which an agent's later question sends as history.

So an agent can't get the image, but it can learn what the owner said about it, and what the model said it showed. #89 decided agents keep document text; it didn't decide this.

## Proposed outcome

- An agent's `session_new` or `session_restore` leaves a waiting screenshot and attached images for the owner, or is refused while one waits. Which one is for the spec.
- For part 2, whatever the owner decides below, with a test showing it.

## Affected users and systems

Anyone using MCP agents. `src/daemon/session.rs` (`reset_session`, `session_restore`, the `session.new` handler, maybe `spawn_follow_ups`), possibly `src/mcp/mod.rs` tool descriptions. No database or shell change expected.

## Constraints

- Use the existing `origin == Some("mcp")` check, as #89 did. Not a sandbox against local processes.
- The owner's own new session or restore keeps clearing everything, as today.
- A test that fails without the fix, for each change.

## Open questions

1. **Part 1: refuse, or keep?** I'd keep the waiting screenshot and attached images across an agent's reset, and clear the rest as usual. Refusing would block agents from a normal action because of something they can't see.
2. **Part 2: may agents see text about the owner's images?** Options:
   - **a.** Accept it, and say so in the README. Nothing changes in code.
   - **b.** Keep image names out of agents' view: follow-up suggestions aren't shown to agents after a turn with an image.
   - **c.** Also leave turns that carried an image out of an agent's history.

   I'd pick **b**: it's small and closes the leak an agent can read directly. **c** removes the owner's own answers from the agent's context, which would make follow-up questions about the same topic worse, and the owner can already choose not to give an agent access.
