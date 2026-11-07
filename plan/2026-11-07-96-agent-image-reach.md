---
status: draft
issue: 96
spec: spec/2026-11-07-96-agent-image-reach.md
coders:
  1: codex   # small, tricky state handling across three reset paths, with tests that must fail without it
  2: coder   # one README paragraph
reviewers:
  - codex    # read-only: any agent path that still drops the waiting screenshot, or an owner path that now keeps it
  - agy      # read-only: tests really fail without it; the shell still shows the screenshot
---

# Plan: Agents can't discard the owner's waiting screenshot

Branch `fix/96-agent-image-reach`, worktree `../meetingai-96`. Line numbers are from `origin/main` at `202995a`; find each place by its code.

## Decisions carried from the approved spec

- When an agent (`origin == Some("mcp")`, the `agent` flag `dispatch` already has) starts a new session or reopens a meeting or chat, the owner's waiting screenshot (`pending_capture`) is kept. Everything else is cleared as today.
- Attached images (`g.sources`) are not kept. They're files the owner picked and can attach again, and the shell doesn't show them, so kept ones would go unseen into the new session.
- The owner's own new session and restore keep clearing everything.
- The shell must still show the screenshot after the reset.
- Text about images: no code. The README says what agents can read.
- Accepted: a kept screenshot can be saved under the meeting an agent reopened (`src/daemon/session.rs:1971`), not the one open when it was taken.

## Steps

1. **Code and tests, `src/daemon/session.rs`.**
   - Next to `reset_session` (:4268), add a helper:
     ```rust
     /// An agent's reset keeps the owner's waiting screenshot: it exists nowhere else.
     fn reset_session_keeping_capture(&self, g: &mut Inner, agent: bool) {
         let kept = if agent { g.pending_capture.take() } else { None };
         self.reset_session(g);
         if kept.is_some() {
             g.pending_capture = kept;
         }
     }
     ```
   - Use it instead of `reset_session` at exactly three places:
     - `session_restore`'s chat branch (:4352);
     - its meeting branch (:4360);
     - the `session.new` handler (:6084).

     The other callers (:3309, :4573, :5465) stay as they are.
   - In the chat branch, add `self.emit_state(g);` after `self.restore_chat(g, id)?;`, before `return Ok(json!({"kind": "chat"}))` (:4355). The shell sets `capture` false on `session.reset` (`shell/Client.qml:329`), and only a `state` event shows it again. The meeting branch (:4421) and `session.new` (:6089) already emit state.
   - Test `agent_reset_keeps_waiting_screenshot`, beside `agent_leaves_waiting_image_and_its_meeting_row_for_owner` (:7418). For each of `session.new`, `session.restore` of a saved chat and `session.restore` of a saved meeting:
     - set `pending_capture` to a `CapturedImage`, call with `Some("mcp")`, then assert `state_value(...)["capture"] == true`;
     - for the chat restore, subscribe first and assert a `state` event with `capture: true` comes after `session.reset`.
     - The owner's `session.new` (origin `None`) clears it.

     Build the saved chat and meeting the way the existing restore tests do (around :7828 and :7887).

   → verify: the step check. Then remove the take-and-restore from the helper: a test must fail. Then remove the new `emit_state`: the chat event check must fail.
   Traps:
   - Never hold `self.inner.lock()` across an `.await`.
   - `session_restore` refuses an agent for a local-only interview before resetting (:4343); keep that order.
   - Codex's sandbox can't run nix or cargo: the lead runs the check.

2. **README**, "Ask your agents", after "An agent's question never takes or sends an image." (`README.md:373`), add:

   > An agent never gets your images. It can read the text around them: your questions, the answers and follow-up suggestions, which can name an image's file. That's what an agent can already read about anything else. Starting a new session or reopening a meeting from an agent keeps a screenshot that's waiting for your question.

   → verify: `nix flake check`.

**Step check:** `find target/debug target/release -maxdepth 2 -name 'lib*.so*' ! -perm -u+w -delete; nix develop -c bash -c 'cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check'`

## Tests

- Both removals in step 1 fail a test.
- The existing `agent_leaves_waiting_image_and_its_meeting_row_for_owner` passes.
- The step check, `bash shell/tests/ci.sh`, `bash shell/tests/smoke.sh` and `nix flake check` pass.

## Rollback

Revert the PR's merge commit. Agent resets go back to clearing the waiting screenshot. No data or setting to undo.

## Deploy

To p620 by the usual flake bump, after checking `ctl state.get | jq .data.recording` is false.
