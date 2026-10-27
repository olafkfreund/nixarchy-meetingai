---
status: approved
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
author: olafkfreund
reviews: local measurements on p620, 2026-10-27 (below)
---

# Intent: Gemini by subscription, OpenCode for everyone else, more models

## Problem

- **Gemini needs an API key.** The owner pays for Google's subscription
  (Antigravity) and wants Gemini to use it, like Claude and ChatGPT do.
- **Users without Claude, ChatGPT or Antigravity** have only API keys or
  local Ollama. Many already use OpenCode, which logs in to ChatGPT and
  GitHub Copilot and reaches 75+ providers.
- **Other models** (OpenRouter, Mistral, Groq, DeepSeek, LM Studio, vLLM)
  work today only by typing a Custom provider (`src/ai/catalog/custom.rs`)
  by hand.

Measured on p620 (2026-10-27):

| Route | Result |
|---|---|
| `agy -p --output-format stream-json` (Antigravity login), gemini-3.8-flash-low | ~40 s per answer, 33 s of it before the prompt is accepted, on every call; a long-lived `--input-format stream-json` process gave no output in a 200 s test |
| Gemini CLI (`gemini -p`, Google sign-in) | not installed; not measured |
| `opencode run`, free model, cold | 8.2 s, correct answer; a second free run was throttled (90 s timeout) |
| `opencode run` overhead | ~4.4 s cold, ~1.3 s attached to `opencode serve` |
| OpenCode accounts here | ChatGPT login returns 401; OpenCode Zen has no credit |

## Proposed outcome

- **`gemini-sub`:** Gemini through the Gemini CLI with Google sign-in, no
  key, streamed. Installed by the Home Manager module. Used for live
  answers if its measured first-token time is close to Claude's CLI;
  otherwise for notes only.
- **`agy-sub`:** Gemini (and the other Antigravity models) through agy,
  for slow jobs only: notes, recaps, question banks. Never live.
- **`opencode`:** one provider that drives a private `opencode serve`
  (loopback, started on demand, kept warm) and lists every model OpenCode
  can reach. Works with whatever the user logged in to in OpenCode.
- **Presets** for common API endpoints (OpenRouter, Mistral, Groq,
  DeepSeek, Together, LM Studio, vLLM), so "other models" is a pick from
  a list plus a key.
- **Fallback order** for typed questions: Claude → Gemini CLI → OpenCode
  → Codex → agy (notes only) → Ollama. Live answers skip Codex and agy.
- **Codex, local Ollama and Ollama Cloud must work** (owner): each is
  verified end to end on this machine before the new providers are built,
  and kept working. Ollama Cloud (`ollama.com`, API key from the owner's
  agenix/sops secrets) is a first-class provider next to local Ollama,
  with its models listed.
- **Setup is visible:** Providers shows which logins work, with the login
  command for each (`claude`, `gemini`, `opencode auth login`, `agy`).

## Affected users and systems

- Users with a Google subscription, OpenCode users, users with only API
  keys.
- `src/ai/catalog/**`, `src/ai/chat/**` (CLI adapters), `src/daemon/session.rs`
  (chain, usable checks), `shell/Library.qml` (Providers), `nix/hm-module.nix`
  (install the Gemini CLI), README.

## Constraints

- Subscription logins only for the `-sub` providers: keys are unset in
  their environment, as for `claude-sub` and `codex-sub`.
- No tools, no file access, no MCP, no session persistence for any CLI
  route (as today for Claude and Codex).
- `opencode serve` listens on loopback only, in a private temp directory.
- Local-only interviews stay loopback Ollama + whisper; none of these
  routes count as local.
- Using these CLIs from another program must stay within each service's
  terms; the README says the user is responsible for their plan's limits.

## Decisions (owner approved the proposals)

1. **Order:** after the UI quality plan and before call detection.
2. **Gemini CLI on the live path:** decided from the measurement in the
   plan.
3. **OpenCode server lifetime:** start on first use, stop after 10 min
   idle.
