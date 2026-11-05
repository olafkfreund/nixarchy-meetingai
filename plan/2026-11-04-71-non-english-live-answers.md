---
status: approved
issue: 71
spec: spec/2026-11-04-71-non-english-live-answers.md
coders:
  1: codex   # small tricky logic: the prompt rule and where the origin flows
  2: coder   # the shell sending the flag and its smoke check, ordinary edits
reviewers:
  - codex    # read-only: the prompt rule and precedence
  - agy      # read-only: paths that send fixed text
---

# Plan: Live answers on calls that are not in English

**Running order:** steps 1 and 2 in parallel, on separate files, sharing the one field below. Step 3 is Claude's. Claude runs checks and commits.

## Decisions carried from the approved spec

- **The shell and daemon mark the app's own English requests,** with no matching on text: `chat.send` gains `app_request: bool`, default false. Anything typed never sets it, including "What should I say?" typed by hand.
- **The rule:** when the request is the app's own, no meeting language is chosen, the conversation's language is Auto, and there is transcript text, the instruction reads "This request is a button in the app, not the user's language. Reply in the language of the recent meeting speech." A chosen language or a set `response_language` wins over it.
- **Which requests:** the "What should I say?" button; asking about a quoted line with nothing typed; a screenshot or attachment with nothing typed; generated follow-ups.
- **No language detection, no opener lists, no change to `is_question`.** The button's label stays in English. The filler list stays, for a separate cleanup.
- **Question detection:** counts from the owner's own meetings, on this machine, numbers only. 80% or more of opener lines carrying a `?` means no trigger change. Otherwise a separate spec with a real labelled set. Too little Polish means S1 ships, #71 closes, and a new issue is opened only when detection is seen failing.

## Traps

- **Line numbers** may have moved; find by function name.
- **Attached text chips** change the question string (`shell/Overlay.qml:234`), which is why the flag is carried and text is not matched.
- **Codex's sandbox cannot run nix or cargo.** Claude runs them; nothing is committed during a handoff.
- **`conversation_response`'s latched language** decides "Auto" for an ongoing conversation, so read it as `system_prompt` does.
- **Automatic answers** are not app requests: the question there is the remote speaker's own line.
- **The owner's meetings are private:** step 3's counts print numbers only, never text.

## Steps

### 1. The rule (codex) — `src/daemon/protocol.rs`, `src/daemon/session.rs`, `src/core/models/settings.rs`

1. `ChatSend` (`protocol.rs:57`) gains `#[serde(default)] app_request: bool`.
2. `chat_send`: true when it supplies the screenshot or attachment fallback text. Passes it to `begin_send`, and on to `system_prompt`. Every other `begin_send` caller passes false: `answer_pending_question`, which is automatic, and the existing test that calls it directly.
3. `system_prompt`: the rule above, after the existing language precedence is resolved (:1707). `settings.rs` holds the sentence, next to `response_instruction`.
4. `spawn_follow_ups` (:2187) calls `system_prompt` itself, and passes true, since its request is always the app's own.
5. **Tests:** an app request gets the rule; the same words without the flag do not; a chosen meeting language wins; a set `response_language` wins; no transcript means no rule; an automatic answer is unaffected; a default call's prompt differs only for app requests; follow-ups get the line. Existing `is_question` tests are untouched.

Check, by Claude: `cargo test --workspace`, clippy `-D warnings`, fmt.

### 2. The shell (coder) — `shell/Overlay.qml`, `shell/Client.qml`, `shell/tests/`

1. `Overlay.submit` (:231) sets `app_request` when it substitutes the quoted-line text and the user typed nothing; `onQuick` (:417) sets it. `Client.ask` (`shell/Client.qml:194`) passes it to `chat.send`, and saves it in `lastAsk`, so `retry()` resends it. A failed button request must not retry as if typed.
2. `fake-daemon.py` logs the flag; `smoke.sh` checks the button and an empty quoted-line request send it, typed text does not, and a retry of a button request still sends it.

Check, by Claude: `bash shell/tests/ci.sh`.

### 3. Evidence and ship (Claude)

1. **Counts, read-only, through `nixarchy-meetingai ctl` as an agent:** `session.list` gives the 40 most recent meetings and hides Local-only interviews, and `transcript.get` pages each. So this covers those 40 at most, and **deliberately excludes Local-only meetings.** Count final remote lines starting with an English opener, and how many have a `?`; then the same for Polish openers ("czy", "co", "jak", "dlaczego", "kiedy", "gdzie"). Numbers go in `docs/eval/2026-11-question-marks.md`, and no text does. The scope is stated in the result.
2. **Sample:** five Polish and five English made-up transcripts with "What should I say?" on the owner's usual chat provider, using the real prompt, reporting the language of each answer. Only those test sentences are sent.
3. Apply the decision rule and report. If it calls for a trigger change, stop for the owner: that is new work.
4. Reviews by a fresh Opus agent, Codex and agy, read-only; `nix flake check`; a PR linking the artifacts; merge after CI; a meetingai-only nixos bump and deploy to p620 when nothing is recording.

## Rollback

`git revert -m 1 <merge>`, and bump nixos.

## Deviations

(Recorded here during implementation, in the same commit as the code.)
