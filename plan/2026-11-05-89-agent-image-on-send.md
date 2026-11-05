---
status: approved
issue: 89
spec: spec/2026-11-05-89-agent-image-on-send.md
coders:
  1: codex   # small, tricky logic across the send path, with tests that must really fail without it
reviewers:
  - codex    # read-only: any image path left for an agent
  - agy      # read-only: the owner's send is unchanged, and the tests
---

# Plan: An agent's question never captures the screen

**Before step 1:** merge `main` into this branch once #84 (PR #90) has merged, because it changes `src/daemon/session.rs` heavily. Line numbers below are from `6d48810`, so find each place by its code.

## Decisions carried from the approved spec

- A request is from an agent when `origin == Some("mcp")`. `dispatch` already receives this as `agent: bool` (`src/daemon/session.rs:5420`, `:5471`). No new request field, setting or error.
- An agent's question is answered without any image, never refused because of one.
- It never takes a fresh capture, never takes the owner's waiting image, never sends an attached image or its name.
- The owner's waiting and attached images stay available for the owner.
- An agent's empty question is refused as empty, even while an image waits.
- An agent still gets the question, transcript, notes and text from attached documents (owner's decision, 2026-11-05).
- The owner's own sends and automatic answers don't change. Local-only handling doesn't change.
- This covers the MCP server. Other local processes can omit `origin`, as the intent states.

## Traps

- Tests must never run the real `grim`.
- The owner's setting has no effect on this path in tests: the seam never falls through to `grab_screen`.
- Turns run on the test's single-threaded runtime: wait for the provider's request with `spawn_blocking`, as at `:6754`.
- Before `cargo`: `find target/debug target/release -maxdepth 2 -name 'lib*.so*' ! -perm -u+w -delete`.
- Codex can't run `cargo` or `nix` in its sandbox. Claude runs the checks and commits.
- Start quickshell only through `shell/tests/`. No wildcards under `/tmp`. No artificial CPU load.

## Steps

### 1. Daemon and README (codex) — `src/daemon/session.rs`, `README.md`

1. `chat_send` (`:1777`) gains `agent: bool`; its caller (`:5624`) passes it.
2. `image_only` (`:1794`): `imported.is_empty() && !agent && (args.screenshot || pending)`. So an agent's empty question is always the empty error.
3. `fresh` (`:1815`): `!agent && (args.screenshot || s.use_image_on_send)`.
4. The take (`:1838`): an agent's send uses `None` and leaves `pending_capture` and its state alone.
5. `begin_send` (`:1887`) gains `agent: bool`. Callers: `chat_send` passes its value; automatic answers (`:2409`) and the test (`:9886`) pass `false`.
6. In `begin_send`: skip the image-support check (`:1945`) for an agent, and don't add `g.sources` images (`:2006`) for an agent.
7. `Inner` gains `turn_from_agent: bool`, which `begin_send` sets before it builds the prompt, on every turn. `system_prompt` (`:1643`) reads it, and when it's true passes `append_prompt` (`:1727`) a list of only the sources with no image (`.filter(|s| s.image.is_none()).cloned()`, no change to `append_prompt`). Follow-up suggestions run later, from `finish_turn` to `spawn_follow_ups` (`:2076`, `:2181`, `:2233`), and they read the same field. This replaces the spec's `system_prompt` argument: an argument would have to pass through `finish_turn` and its test callers, and ten test calls to `system_prompt`, for the same result.
8. `chat_send`'s capture (`:1823`) goes through a seam. Add to `Daemon`, under `#[cfg(test)]`: `chat_capture: Mutex<Option<Box<dyn Fn() -> Result<CapturedImage, String> + Send + Sync>>>`, `None` by default. In tests the call uses it, and `None` returns an error, so `grim` can't run. The test's closure counts its calls. Only this call uses it.
9. `README.md`, "Ask your agents": add "An agent's question never takes or sends an image."
10. **Tests**, using `provider_server` (`:6733`) and asserting on the request body:
    - Agent, `use_image_on_send` on, `capture_delay_seconds` at 30, longer than `provider_server`'s 5 s read timeout (`:6404`), so waiting would fail the test: the seam isn't called, and no image is in the request.
    - Agent, waiting image, with a meeting open (rows are written only then, `:1842`): no image in the request, `state.capture` stays true, the row count doesn't change, then the owner's send takes and sends it and adds a row.
    - Agent, attached image and a text attachment: no image, filename or `image_attached` in the request; the text is there; `g.sources` still has the image; it works on a model without image support.
    - Agent, empty question, with a waiting image, and calling `chat_send` directly with `screenshot: true`: the empty error both times.
    - Follow-ups: with `turn_from_agent` true, the prompt `spawn_follow_ups` builds has no image filename or `image_attached`; with it false, it has them.
    - Owner, `use_image_on_send` on: the seam is called once and the image is sent.
    - Owner, attached image: it's sent.

Check: `nix develop -c bash -c 'cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check'`

### 2. Ship (Claude)

Remove each guard in turn and confirm a test fails. `nix flake check`. Read-only reviews by a fresh Opus agent, Codex and agy. A PR closing #89, saying Codex did step 1. Merge after CI. A meetingai-only nixos bump, and deploy to p620 when nothing is recording, following the usual procedure. On p620, with the owner's setting still on, ask one harmless question through MCP, and confirm no screenshot row is added and no `grim` ran (database and journal).

## Tests

The step 1 check, `nix flake check`, and the removal check in step 2, all passing. The p620 check shows no new row.

## Rollback

Revert the merge and bump the nixos pin back. No schema or setting changes.

## Deviations

(Recorded here during implementation, in the same commit as the code.)
