---
status: draft
issue: 4
author: olafkfreund
---

# Intent: add a provider from a preset in the overlay

## Problem

The app knows 13 provider presets (OpenAI, Anthropic, Groq, OpenRouter,
Mistral, DeepSeek, Together, LM Studio, vLLM and others: endpoint, model,
wire format, image support), but nothing outside the presets file uses
them. In the overlay, the Providers tab shows the built-in providers and
their login status only:

- There is no way to add a custom provider (an endpoint with a key) from
  the overlay. The daemon has no command to list presets or to add one;
  a custom provider only arrives through the Nix settings
  (`custom-...` ids) or by hand.
- So trying another model provider means editing Nix config and
  rebuilding, for something that is a user choice, not a system setting.

## Proposed outcome

- In the Providers tab, "Add provider" opens a list of the presets. Picking
  one fills the endpoint, model and format; the user adds the API key (if
  the preset needs one) and saves.
- The saved provider appears in the chat and live-answer provider lists
  like the built-in ones, and can be edited or removed there.
- Local presets (LM Studio, vLLM, Ollama-style endpoints) work without a
  key and with a localhost URL.
- The key goes into the encrypted vault, as keys do today; it is never
  shown again or sent back over the socket.

## Affected users and systems

- This repo: `src/ai/catalog/presets.rs` and `custom.rs` (presets and
  custom-provider records), `src/daemon/session.rs` and `protocol.rs`
  (new socket commands), `src/core/security/credentials.rs` (the vault
  record), `shell/Library.qml` / `Overlay.qml` (the Providers tab),
  `shell/tests/fake-daemon.py`, README.
- Every host with the module; nothing changes until a user adds one.
- Rewrite #3: `core/security` is already rewritten (step 6, merged);
  nothing here touches the step 3/10/11 files.

## Constraints

- Keys only in the vault, never in settings, logs, events or replies;
  agents and MCP clients cannot add, change or read providers.
- Only `https://` endpoints, or `http://` on localhost (the existing
  endpoint rule).
- A provider fixed by the Nix settings stays read-only in the overlay
  (shown, not editable), like other Nix-fixed settings.
- No new network calls when the list opens; a "test connection" button
  is optional (open question 2).
- Shell tests stay headless.

## Open questions

1. **Scope:** presets for chat only, or speech too (the presets file has
   a speech flag; today the speech presets are few)? Recommended: both,
   since the record is the same; the list shows which kind each is.
2. **Test connection:** a button that sends one tiny request with the
   key before saving? Recommended: yes, optional, so a wrong key is found
   at once rather than in a meeting.
3. **Several of the same preset** (e.g. two OpenRouter keys)? Recommended:
   allowed, each with its own name.
