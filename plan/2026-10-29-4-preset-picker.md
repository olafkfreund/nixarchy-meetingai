---
status: approved
issue: 4
spec: spec/2026-10-29-4-preset-picker.md
coders:
  1: codex     # daemon commands and tests: protocol plumbing over existing presets/credentials
  2: agy       # overlay Providers tab: QML and the headless tests, against the fixed contract below
  # 3 (README) stays with the lead
reviewers:     # read-only, on top of the Opus review
  - codex      # codex review --base origin/main, sandbox read-only
  - agy        # agy --mode plan, files named in the prompt
---

# Plan: add a provider from a preset in the overlay

## Approved decisions (copied from the spec)

- Reuse `src/ai/catalog/presets.rs` (`PRESETS`, `Preset::draft()`) and the
  vault's custom credentials; `provider_config` already resolves a saved
  `custom-…` id, so no change to how a chosen provider is used.
- Presets for chat and speech; several providers from one preset allowed
  (ids unique, names free); "Test connection" only on an explicit click.
- Keys only in the vault; never in a reply, event, log or error. All new
  commands except `presets.list` are refused for `agent` and `mcp`
  origins ("not allowed for agents").
- Endpoints: `https://`, or `http://` on localhost (`endpoint_allowed`).
- Removing a provider in use resets that setting to its default with a
  notice; a Nix-fixed setting is never changed (remove refuses instead).

## Contract (both steps build against it)

Socket commands (requests are `{cmd, args}` as today; replies `{ok, data,
error}`):

| Command | Args | Data |
|---|---|---|
| `presets.list` | `{}` | `[{preset, name, kind: "chat"\|"speech", endpoint, model, needs_key, local}]` (`preset` = slug of the name: lower case, spaces → `-`; `local` = localhost endpoint; `needs_key` = `!local`) |
| `providers.add` | `{preset, name?, api_key?, model?, endpoint?}` | `{id}` |
| `providers.update` | `{id, name?, api_key?, model?, endpoint?}` (`api_key`: absent keep, `null` remove, string set) | `{}` |
| `providers.remove` | `{id}` | `{}` |
| `providers.test` | `{preset?, id?, api_key?, model?, endpoint?}` (exactly one of preset/id) | `{ok: bool, message: string}` |
| `providers.list` | `{}` | built-ins as today, plus custom rows `{id, name, kind, custom: true, model, endpoint, has_key, requires_key}` for chat and speech |

- `settings.set` accepts a `custom-…` id for `chat_provider`,
  `live_provider` (chat records) and `speech_provider` (speech records)
  when that record exists; otherwise its current error.
- Event: `providers.changed {}` after add, update and remove, so open
  overlays reload `providers.list`.
- Errors are plain sentences (shown to the user as-is).

## Repo traps (all steps)

- Implementers never commit, deploy or restart; the lead commits. Report
  changed files and deviations.
- Do not edit `src/speech/stt/transcription.rs`, `src/speech/session/listen.rs`,
  `src/cli.rs`, `src/audio/capture/*` (rewrite steps shipping now).
- agy: the worktree is read-only to its shell; edit with file tools, use
  commands only for checks.
- After Codex or agy ran: `chmod -R u+w target`, remove
  `target/*/build/sherpa-onnx-sys-*`.
- Never call a real provider in tests: local fake HTTP servers only (see
  `tests/provider_flow.rs` for the pattern). Shell tests headless only
  (`bash shell/tests/ci.sh`).
- Check: `chmod -R u+w target; TMPDIR=/tmp devenv shell -- bash -c 'cargo fmt --check; cargo clippy --workspace --all-targets -q -- -D warnings; cargo test --workspace -q'`

## Steps

1. **Daemon** (`src/daemon/protocol.rs`, `src/daemon/session.rs`,
   `src/ai/catalog/presets.rs` for a slug helper if needed): the commands
   and event of the contract; `providers.list` custom rows; `settings.set`
   accepting existing custom ids; remove-in-use fallback with a notice and
   refusal for Nix-fixed settings; `providers.test` (chat: one short
   non-streaming request, 15 s timeout; speech: 0.5 s silent WAV through the
   existing transcribe path, empty text = success; message scrubbed of the
   key; refused in local-only sessions).
   → verify: daemon tests for every contract row, origin refusal, the key
   never in replies/events (grep the serialized output for the test key),
   remove-in-use and Nix-fixed refusal, test ok/fail against a local fake
   server; the check command.
2. **Overlay** (`shell/Library.qml` Providers tab, `shell/Client.qml`,
   `shell/tests/fake-daemon.py`, `shell/tests/smoke.sh`): "Your providers"
   rows (name, Chat/Speech, model, endpoint host, key set) with **Use for
   chat** / **Use for speech** (by kind; sets the setting unless
   Nix-locked), **Edit**, **Remove** (existing confirm dialog); **Add
   provider** → preset list grouped Chat / Speech, local ones marked "on
   this computer" → form (name, endpoint, model, API key as a password
   field hidden for local presets, **Test connection** with its message,
   **Save**); Edit reuses the form with an empty key field ("leave empty to
   keep the saved key"); reload on `providers.changed`.
   → verify: `bash shell/tests/ci.sh`; the smoke test drives add → test →
   save → use for chat → remove against the fake daemon.
3. **README** (lead): Providers section: adding a provider from a preset,
   where the key lives, that Test connection sends one request.
   → verify: `docs-match` flake check.
4. **Review and ship** (lead): Opus review against this plan, Codex and agy
   read-only reviews; fixes; PR linking intent, spec, plan; CI; merge
   commit; deploy p620 from committed main, announced on the agent bus,
   not while recording.

Steps 1 and 2 run in parallel (separate worktrees) against the contract.

## Tests

The check command, `bash shell/tests/ci.sh`, `nix flake check`.

## Rollback

Revert the merge commit. Custom providers saved meanwhile stay in the
vault and keep working when chosen (resolution is unchanged); only the
overlay loses the add/edit screens.

## Deviations

- Step 1: OpenAI and Groq have both a chat and a speech preset with the
  same name, so the speech slugs end in `-speech` (`openai-speech`,
  `groq-speech`).
- Step 1: `providers.update` with `api_key: null` is refused for a
  provider that needs a key ("remove the provider instead"); the vault
  keeps its rule that a keyed custom provider has a key. Keyless (local)
  providers can drop a key.
- Step 1: the speech test sends 0.5 s of silence; the custom wire reports
  a blank transcript as "empty text", which the test counts as a working
  connection (only that error).
- Review fix: custom providers were never routed. `plan_chain` only knew
  `CHAT_PROVIDERS`, so a `chat_provider` or `live_provider` of `custom-…`
  was skipped (also for custom ids set through Nix before this PR). The chosen
  custom id now leads the chain with the chosen model, ahead of the built-in
  fallbacks, when its vault record is usable; local-only still filters it by
  `stays_local`. `speech_config` also allows a custom speech id in a
  local-only session when its endpoint is loopback.
- Review fix: `provider_config` and `speech_config` ignore the `chat_model` /
  `speech_model` setting for a custom provider; it uses its own saved model.
  The overlay model picker shows that model instead of calling `models.list`.
- Review fix, auth: a preset keeps the auth its draft gives (Anthropic:
  `header` with `x-api-key`; others `bearer`). Only a localhost `http://`
  endpoint switches to `none`; an https endpoint restores the preset's auth.
  `providers.add`, `providers.update` (when the endpoint changes) and
  `providers.test` use this one rule. Keys are trimmed once on input.
  `providers.test` takes `name` and answers `{ok: false, message}` for a
  missing key or a bad endpoint instead of an RPC error.
- Review fix: `settings.set` with a `custom-…` id that has no record fails
  with "This provider no longer exists."; the removal notice carries
  `kind: "providers"` (shown as a banner).
- "Use for chat" sets only `chat_provider`; `live_provider` stays (said in the
  README). Origin gate: the `cmd != "presets.list"` test was redundant and is gone.
- Re-review: a streaming custom speech record sends audio to its
  `stream_url`, which `stays_local` does not check, so local-only sessions
  never use a streaming custom speech record (tested). The provider form is
  cleared when the overlay closes. `fit_auth` sends no key to any localhost
  `http://` endpoint, so a local proxy that needs a key (e.g. LiteLLM on
  localhost) is not supported from presets; moving an endpoint back to
  https restores the preset's auth, not a hand-edited one.
