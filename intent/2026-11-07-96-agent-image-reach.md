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

**2. An agent can learn about images from text.** Found by the #89 review, traced in code, not reproduced. Text about the owner's images reaches agents by at least four routes:
- **Follow-up suggestions.** They're generated with every source's name in the prompt, images included (`spawn_follow_ups`, `src/daemon/session.rs:2336`; `append_prompt`, `src/core/context.rs:73`). This holds for every later owner turn while the image stays attached, not just the turn with it. Agents get suggestions from `now.get` (`:5833`) and in MCP `ask`'s reply (`ask_daemon`, `src/mcp/mod.rs:144`).
- **The latest answer.** MCP `current_answer` and `meeting_status` return `now.get`'s last question and answer (`src/mcp/mod.rs:478`, `:410`), so an answer describing the image can be read as is.
- **History.** An answer describing the image stays in the conversation, which an agent's later question sends as history.

So an agent can't get the image, but it can learn what the owner said about it, and what the model said it showed. #89 decided agents keep document text; it didn't decide this.

## Proposed outcome

- An agent's `session_new` or `session_restore` leaves a waiting screenshot and attached images for the owner, or is refused while one waits. Which one is for the spec.
- For part 2, whatever the owner decides below: written down in the README, with a test if the code changes.

## Affected users and systems

Anyone using MCP agents. `src/daemon/session.rs` (`reset_session`, `session_restore`, the `session.new` handler, maybe `spawn_follow_ups`), possibly `src/mcp/mod.rs` tool descriptions. No database or shell change expected.

## Constraints

- Use the existing `origin == Some("mcp")` check, as #89 did. Not a sandbox against local processes.
- The owner's own new session or restore keeps clearing everything, as today.
- A test that fails without the fix, for each change.

## Open questions

1. **Part 1: refuse, or keep?** I'd keep the waiting screenshot and attached images across an agent's reset, and clear the rest as usual. An agent can see that a screenshot waits (`capture` in `state.get`, `src/daemon/session.rs:1349`), but refusing would still block a normal action over the owner's half-finished question.
2. **Part 2: may agents see text about the owner's images?** Options:
   - **a.** Accept it, and say so in the README. Nothing changes in code.
   - **b.** Hide the routes from agents while an image is attached or was in the latest turn: suggestions, the latest answer in `now.get`, and image turns in an agent's history.
   - **c.** Only hide suggestions.

   I'd pick **a**. Blocking every route means hiding the owner's own answers from agents they chose to connect, and a partial block (**c**) looks safe without being safe, since `current_answer` still returns the answer. The images themselves stay out of reach, which #89 already ensures.
