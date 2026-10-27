---
status: draft
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
intent: intent/2026-10-27-0-more-providers.md
---

# Spec: subscription Gemini, OpenCode, Ollama Cloud and correct model lists

## Measured before design (p620, 2026-10-27, through the real daemon)

| Provider / model | First token | Total | Result |
|---|---|---|---|
| `codex-sub` | 3.8 s | 3.9 s | works, but **hangs forever when `codex` on PATH is a mise command wrapper** |
| `claude-sub` haiku | 1.7 s | 4.8 s | works (same mise risk: `claude` is also a mise wrapper here) |
| `ollama` qwen2.5:7b | 3.0 s | 3.2 s | works |
| `ollama` gpt-oss:120b-cloud (cloud via the local daemon) | 0.3 s | 0.5 s | works |
| `ollama` qwen3.8:27b (17 GB, cold) | — | 30 s | **fails**: the HTTP read timeout (`src/core/network.rs:10`, 30 s) fires before the model loads |
| Ollama Cloud direct (`https://ollama.com/api/chat`, bearer key) | — | 0.7 s | works; `/api/tags` and the OpenAI-compatible `/v1/models` list 14+ models |
| `agy -p` (Antigravity login) | ~39 s | ~40 s | works, too slow for live |
| `opencode run`, free model, cold | — | 8.2 s | works; ~4.4 s cold overhead, ~1.3 s attached to `opencode serve` |
| `models.list` | | | claude-sub: sonnet/opus/haiku; codex-sub: only `default`; gemini: error without a key; ollama: includes embedding/OCR models |

## Owner requests folded in

- Codex, local Ollama and Ollama Cloud must work.
- **Remove the Gemini API-key provider.** Gemini is reached by subscription
  only (Gemini CLI, agy, or OpenCode).
- **The model dropdown always shows the selected provider's models.**
- (The red recording indicator is a UI request; it goes into the UI quality
  plan, not here.)

## Design

### 1. Fixes to what exists

- **CLI spawn in its own session:** `src/ai/chat/cli.rs:309` uses
  `process_group(0)`. A mise wrapper in a background process group is
  stopped when it touches the terminal. Spawn with `setsid()` in
  `pre_exec` (new session, no controlling terminal; the pgid still equals
  the pid, so `GroupGuard`/`killpg` keep working). A test spawns a script
  that reads `/dev/tty` and must not hang.
- **Resolve the CLI once:** the daemon resolves `claude`, `codex`,
  `gemini`, `agy`, `opencode` on PATH at start and logs the resolved path
  (not args). The HM module sets the service PATH to the user's Nix
  profile, so the systemd service never depends on shell shims.
- **Local model load time:** chat requests to loopback Ollama use a
  300 s read timeout (per read, so a stream that keeps sending is never
  cut). Other HTTP calls keep 30 s. The NOW card shows "Loading model…"
  until the first token.
- **Ollama model list:** only models whose `/api/show` capabilities
  include `completion` (drops embedding and OCR-only models), cached per
  daemon run.
- **Codex model list:** `default` plus the models `codex` itself accepts
  (read from `codex` `--help`/config if it exposes them; otherwise a short
  maintained list in `src/ai/catalog/`), with `default` first.

### 2. Model dropdown correctness (`shell/`)

- Changing the provider calls `loadModels(newProvider)` with the value
  the user picked, not the value read back from settings (today the
  reload races the settings echo and shows the previous provider's
  list).
- Every `models.list` reply is tagged with the provider it was asked for;
  a reply for a provider that is no longer selected is dropped.
- A failed list clears the dropdown and shows the error under it, with
  the fix ("Run `opencode auth login`", "Set the Ollama Cloud key").
- The model is reset to the provider's default (first entry) when the
  provider changes, not to an empty string.

### 3. Providers after this change

| id | Route | Auth | Live answers | Notes / recaps |
|---|---|---|---|---|
| `claude-sub` | `claude -p` stream-json | Claude subscription | yes | yes |
| `codex-sub` | `codex exec --json` | ChatGPT subscription | no (no streaming) | yes |
| `gemini-sub` | `gemini -p --output-format stream-json` | Google sign-in | if measured ≤ 4 s first token | yes |
| `agy-sub` | `agy -p --output-format stream-json --mode plan` | Antigravity login | no | yes |
| `opencode` | private `opencode serve` on 127.0.0.1, HTTP API | whatever is logged in to OpenCode | yes | yes |
| `ollama` | local `/api/chat` | none (loopback) | yes | yes |
| `ollama-cloud` | `https://ollama.com/api/chat` | API key (`OLLAMA_API_KEY`) | yes | yes |
| presets | OpenAI-compatible Custom provider | API key | yes | yes |

- **Removed:** `gemini` (API key). A stored `chat_provider`/`live_provider`
  of `gemini` migrates to `gemini-sub`, and its stored key is deleted from
  the vault on first start (one-line notice).
- **`gemini-sub`:** argv mirrors `claude-sub` hardening: no tools, no
  extensions, no MCP, no session save, system prompt from a 0600 file,
  `GEMINI_API_KEY`/`GOOGLE_API_KEY` unset. Exact flags are confirmed from
  `gemini --help` when implementing. Installed by the HM module
  (`pkgs.gemini-cli`).
- **`agy-sub`:** `--mode plan`, keys unset, temp working directory, never
  in the live chain.
- **`opencode`:** the daemon starts `opencode serve --hostname 127.0.0.1
  --port 0` in a private temp directory on first use, reads the port from
  its output, talks to its HTTP API (create a session, send a message,
  stream events), and stops it after 10 min idle. Tools are disabled for
  the session (OpenCode's agent permissions set to deny). Models come from
  its provider list, shown as `provider/model`.
- **`ollama-cloud`:** the Ollama wire with base URL `https://ollama.com`
  and a bearer key. The key comes from `OLLAMA_API_KEY` in the service
  environment (the owner's agenix/sops secret, wired by the HM module as
  an `EnvironmentFile`) or from the vault. Never counts as local.
- **Presets** (Custom provider templates): OpenRouter, Mistral, Groq,
  DeepSeek, Together, LM Studio (loopback), vLLM (URL). Choosing one fills
  the URL and wire; the user adds a key.
- **Chains:** typed: claude-sub → gemini-sub → opencode → codex-sub →
  ollama-cloud → ollama (agy only when chosen). Live: claude-sub →
  gemini-sub (if fast) → opencode → ollama-cloud → ollama. Local-only
  interviews: loopback ollama only (unchanged).
- **Providers tab** shows each provider's status (ready, not logged in,
  not installed, no key) from a new `providers.status` command, with the
  exact login command to copy.

## Alternatives rejected

- **Keeping the Gemini API key:** the owner asked for subscription only.
- **agy on the live path:** ~40 s per call.
- **`opencode run` per message:** 4–5 s overhead per call; the server
  removes most of it.
- **Raising every HTTP timeout:** only local model loading needs it.
- **Dropping mise-wrapped CLIs:** users install CLIs with mise; the
  session fix makes them work.

## Risks

- **Gemini CLI speed is unknown** until measured; it may end up notes-only.
- **OpenCode's HTTP API** changes between versions; pin the tested version
  range and fail with a clear message outside it.
- **Codex model names** may not be discoverable; a maintained list can go
  stale.
- **Terms of service:** driving subscription CLIs from another program is
  the user's responsibility (README).

## Verification

- **Probe:** `tools/provider-probe.py` (the script used for the table
  above) runs the real daemon in a throwaway directory and asks each
  provider one question. It must show an answer for codex-sub,
  claude-sub, gemini-sub, opencode, ollama (7b and 27b cold), ollama-cloud,
  and agy-sub (notes path), with mise wrappers on PATH.
- **Unit tests:** setsid spawn with a `/dev/tty` reader; the gemini →
  gemini-sub migration and key deletion; Ollama capability filter; model
  reply tagging; chain order per mode; ollama-cloud never local.
- **UI:** switching providers in the fake daemon shows the right list
  every time (a scripted switch through all providers); an error state
  crop.
- **Unchanged checks:** cargo suite, clippy, lint, smoke, flake check.
