---
status: approved
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
spec: spec/2026-10-27-0-more-providers.md
reviewers:   # read-only, after the Opus review
  - codex
  - agy
---

# Plan: subscription Gemini, OpenCode, Ollama Cloud and correct model lists

Runs after the UI quality plan's QML steps, because steps 3 and 7 here
touch the same Overlay/Library model controls; steps 1–2 (daemon only)
can start in parallel with the UI work.

## Approved decisions (copied from the spec)

- **Spawn CLIs in their own session** (`setsid` in `pre_exec`) instead of
  `process_group(0)` (`src/ai/chat/cli.rs:309`), so mise-wrapped `claude`
  and `codex` don't hang; pgid still equals pid for `GroupGuard`/`killpg`.
  The daemon resolves each CLI on PATH at start and logs the path. The HM
  module sets the service PATH to the user's Nix profile.
- **Loopback Ollama chat** uses a 300 s per-read timeout (other HTTP stays
  30 s, `src/core/network.rs:10`); the NOW card shows "Loading model…"
  until the first token.
- **Ollama models:** only those whose `/api/show` capabilities include
  `completion`, cached per daemon run. **Codex models:** `default` first,
  then the models `codex` accepts (discovered if possible, else a short
  list in `src/ai/catalog/`).
- **Dropdown correctness:** load models with the provider the user picked;
  tag each `models.list` reply with its provider and drop stale replies;
  a failed list clears the dropdown and shows the error and the fix;
  changing provider resets the model to that provider's first entry.
- **Providers:** `claude-sub`, `codex-sub`, `gemini-sub` (Gemini CLI,
  stream-json, no tools/extensions/MCP/session save, keys unset, system
  prompt from a 0600 file; live only if measured first token ≤ 4 s),
  `agy-sub` (`--mode plan`, notes only), `opencode` (private `opencode
  serve --hostname 127.0.0.1 --port 0` in a temp dir, started on first use,
  stopped after 10 min idle, tools denied, models as `provider/model`),
  `ollama`, `ollama-cloud` (Ollama wire, `https://ollama.com`, bearer key
  from `OLLAMA_API_KEY` in the service environment or the vault, never
  local), presets (OpenRouter, Mistral, Groq, DeepSeek, Together, LM
  Studio, vLLM) as Custom provider templates.
- **Remove `gemini`** (API key): stored `gemini` provider settings migrate
  to `gemini-sub`; its vault key is deleted on first start with a notice.
- **Chains:** typed: claude-sub → gemini-sub → opencode → codex-sub →
  ollama-cloud → ollama (agy only when chosen). Live: claude-sub →
  gemini-sub (if fast) → opencode → ollama-cloud → ollama. Local-only:
  loopback ollama only.
- **`providers.status`** reports each provider as ready / not logged in /
  not installed / no key, with the login command; the Providers tab shows
  it.

## Repo traps

- cargo only via `devenv shell -- <cmd>`; rustfmt only touched files.
- Coders cannot commit; the lead commits each step.
- Never print or log `OLLAMA_API_KEY` or any vault value; tests use fake
  keys.
- Real-provider checks use `tools/provider-probe.py` against a throwaway
  daemon (`XDG_RUNTIME_DIR`/`XDG_DATA_HOME`/`XDG_CONFIG_HOME` in `mktemp
  -d /tmp/nm.XXXX`), never the owner's data.
- `ollama` must stay loopback-only for local-only interviews;
  `stays_local()` must return false for `ollama-cloud` and for `-cloud`
  model names.
- Shell tests: `devenv shell -- env -u SIZE bash shell/tests/<x>.sh`.

## Steps

1. **Spawn and timeouts.** `src/ai/chat/cli.rs`: `setsid` spawn; CLI path
   resolution and a startup log line. `src/core/network.rs`: a chat client
   with a 300 s read timeout used for loopback Ollama. "Loading model…"
   status in `now.get` / the NOW card until the first delta.
   → verify by a unit test spawning a `/dev/tty` reader that must exit,
   and `tools/provider-probe.py` with the mise wrappers on PATH: codex-sub,
   claude-sub, ollama qwen3.8:27b (cold) all answer.
2. **Model lists.** `src/ai/catalog/models.rs`: Ollama capability filter;
   codex model list; `models.list` replies carry `provider`.
   → verify by unit tests with fixture `/api/show` replies and the probe's
   `models.list` dump.
3. **Remove `gemini`, add `ollama-cloud` and presets.**
   `src/ai/catalog/providers.rs` and `custom.rs`; settings migration and
   vault key deletion; `ollama-cloud` config from env or vault; preset
   templates; `stays_local()` false for it.
   → verify by unit tests (migration, key deletion, never local) and the
   probe for ollama-cloud.
4. **`gemini-sub` and `agy-sub`.** CLI adapters in `src/ai/chat/cli.rs`
   with fixture-based stream parsers (record one real stream each into
   `tests/fixtures/`); install the Gemini CLI in `nix/hm-module.nix`; ask
   the owner to sign in once (`! gemini`); measure first-token time with
   the probe and set whether `gemini-sub` is in the live chain (recorded
   in the deviations log).
   → verify by parser unit tests and the probe.
5. **`opencode`.** `src/ai/chat/opencode.rs`: server lifecycle (start,
   port discovery, idle stop, cleanup on daemon exit), session create,
   message send, event stream to deltas, tools denied; model list from its
   provider API.
   → verify by unit tests against a tiny fake HTTP server and the probe
   (owner fixes the OpenCode ChatGPT login if needed).
6. **Chains and status.** `src/daemon/session.rs` `plan_chain`/`chain`
   (`:420`, `:672`) for the new order; `providers.status`.
   → verify by unit tests for each mode and local-only.
7. **Shell.** `shell/Overlay.qml` (and Library Providers tab): load models
   with the picked provider, drop stale replies, error + fix under the
   dropdown, reset to the first model; provider status rows with the copy
   command; preset picker; "Loading model…".
   → verify by a fake-daemon scripted switch through every provider
   (`SCENARIO=providers`), geometry check, crops.
8. **Nix and docs.** HM module: service PATH, `gemini-cli` package,
   optional `ollamaCloud.environmentFile` for the key; README providers
   table with login commands.
   → verify by `nix flake check` and an HM eval.
9. **Review:** Opus, then Codex and agy; final probe run of every
   provider with the owner present for any login.

## Tests

- `devenv shell -- cargo test --workspace --no-fail-fast`, clippy
  `-D warnings`, rustfmt on touched files.
- `tools/provider-probe.py` answers for every provider (mise wrappers on
  PATH).
- lint, smoke, smoke-real, geometry check, `nix flake check`.

## Rollback

Each step is its own commit. The `gemini` migration is one-way for the
setting; the deleted key can be re-entered as an `ollama-cloud`-style
custom provider if ever needed.

## Implementation split

One `coder` does steps 1–6 and 8 (Rust and Nix) in order; the UI coder
does step 7 after the UI quality plan finishes. The lead commits each
step, runs the probe, and does step 9.

## Deviations log (recorded with the implementation commit)

None yet.

## Owner change (2026-10-27): Gemini through agy, not the Gemini CLI

- The owner chose **agy** (Antigravity CLI, Google subscription) as the
  Gemini route. `gemini-sub` (Gemini CLI) is dropped; the HM module does
  not install `gemini-cli`.
- **Why it was slow:** agy starts every MCP server in the user's
  `~/.gemini` config on each call (14 here, including `go run …@latest`
  and `npx -y`). With a private config that has no MCP servers, a full
  answer takes 6.8–8.1 s instead of ~40 s (measured, gemini-3.8-flash-low).
- **`agy-sub`:** the daemon runs agy with a private `HOME` overlay in a
  0700 temp dir: `.gemini/*` and `.gemini/config/*` symlinked from the
  real home **except** `config/mcp_config*` and `settings.json*`, which are
  replaced by an empty MCP config and `{}`; `.config`, `.local`, `.cache`
  symlinked. This also guarantees no tools reach the call. Flags: `--mode
  plan --output-format stream-json --print-timeout`, keys unset, model from
  `agy models`.
- **Chains:** `agy-sub` takes `gemini-sub`'s place in the typed chain
  (claude-sub → agy-sub → opencode → codex-sub → ollama-cloud → ollama).
  It joins the live chain only if its measured first token is ≤ 4 s;
  otherwise it serves typed questions, notes, recaps and question banks.
- Plan step 4 becomes: the agy adapter with the private-HOME overlay, a
  recorded stream fixture and parser tests, `agy models` discovery, and the
  probe measurement that decides the live chain.
- **Owner refinement:** the `gemini` entry is replaced in one change by
  `agy-sub`, shown as "Antigravity (subscription)", with no key field
  (status from install/login). Stored `gemini` settings migrate straight
  to `agy-sub`. This unit runs right after step 1, ahead of steps 2–3.
- **Step 1 (correction):** the codex/claude "hang" measured before the
  spec was mostly a probe artifact: the probe pointed `XDG_DATA_HOME` /
  `XDG_CONFIG_HOME` at empty temp dirs, so the mise wrappers tried to
  install the tools there. The real user service keeps the real XDG paths.
  `setsid` stays as a cheap guard against a terminal stall. The probe now
  passes `MISE_DATA_DIR`/`MISE_CONFIG_DIR` through. The 300 s client is
  chosen per provider (loopback ollama only); `now.get` reports `loading`
  by provider id, so a non-loopback custom `base_url` isn't excluded there
  (the timeout itself is exact). The `setsid` test can't reproduce a
  controlling terminal under the test runner.
