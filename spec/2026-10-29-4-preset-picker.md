---
status: approved
issue: 4
intent: intent/2026-10-29-4-preset-picker.md
---

# Spec: add a provider from a preset in the overlay

Inputs: the approved intent and its decisions (chat and speech presets,
optional "Test connection", several providers from one preset allowed);
the code on main checked by the lead on 2026-10-29.

## What already works (no change)

- `src/ai/catalog/presets.rs`: 13 presets (`name`, `speech`, `endpoint`,
  `model`, `wire`, `images`) and `Preset::draft()`, which builds a
  `Credential` with id `custom-<uuid>`, a `CustomProvider`, the preset's
  model and endpoint, and an empty key. Its test proves every draft
  validates once a key is set. Nothing outside the file uses it today.
- `credentials::{save, load, list, remove}` keep custom records in the
  vault (`usable()` accepts `custom-` ids with a `custom` member).
- `provider_config(id, ..)` in `src/daemon/session.rs` resolves any saved
  credential, so a `custom-…` id set as `chat_provider` or `live_provider`
  already works end to end.

## Design

### 1. Socket commands (`src/daemon/protocol.rs`, `src/daemon/session.rs`)

All are refused for `agent` and `mcp` origins ("not allowed for agents"),
except `presets.list`, which holds no user data. No reply, event, log or
error ever contains a key.

- `presets.list` → `[{ "preset": <slug>, "name", "kind": "chat"|"speech",
  "endpoint", "model", "needs_key": bool, "local": bool }]`. `slug` is the
  lower-case name with spaces as `-`; `local` = the endpoint is on
  localhost; `needs_key` = not local.
- `providers.add {preset, name?, api_key?, model?, endpoint?}` →
  `{ "id": "custom-…" }`. Starts from `Preset::draft()`; `name` defaults to
  the preset's name (several from one preset allowed: names need not be
  unique, ids are); `model`/`endpoint` override the preset's when given
  (endpoint must pass `endpoint_allowed`); the key goes into the vault
  record; `Credential::validate()` must pass before saving.
- `providers.update {id, name?, api_key?, model?, endpoint?}`: only
  `custom-` ids. `api_key` absent = keep, `null` = remove, string = set.
  Same validation.
- `providers.remove {id}`: only `custom-` ids. If the id is the current
  `chat_provider`, `live_provider` or `speech_provider`, that setting falls
  back to its default and a notice says so. A setting fixed by Nix is never
  changed: then `providers.remove` refuses with the reason.
- `providers.test {preset | id, api_key?, model?, endpoint?}` →
  `{ "ok": bool, "message": String }`. Builds the config the same way as
  add/update without saving, then: chat, one short non-streaming request
  ("Reply with OK", small max tokens, 15 s timeout); speech, a 0.5 s
  silent WAV through the existing transcribe path (an empty transcript is
  success). `message` is the provider's error text with the key removed
  (existing scrubbing), or "Connected." Never sent without an explicit
  click; local-only sessions never call it.
- `providers.list` keeps its shape for built-ins and adds the user's custom
  chat providers: `{id, name, requires_key, has_key, custom: true, model,
  endpoint}`. Speech: a new `speech.providers` reply, or the existing speech
  provider list gains the custom speech records in the same way (whichever
  the overlay already reads; the plan names it after checking).

### 2. Overlay (`shell/Library.qml` Providers tab, `shell/Client.qml`)

- Under the built-in providers: **Your providers** — one row per custom
  provider (name, Chat/Speech, model, host of the endpoint, key set or
  not) with Edit and Remove (Remove asks through the existing confirm
  dialog).
- **Add provider** opens the preset list, grouped Chat / Speech, local
  presets marked "on this computer". Picking one opens a form: name
  (pre-filled), endpoint and model (pre-filled, editable), API key
  (password field, hidden when the preset is local), **Test connection**
  (shows the message), **Save**. Edit opens the same form with an empty
  key field ("leave empty to keep the saved key").
- Saved providers appear in the chat / live-answer provider pickers by
  their name (the pickers already list `providers.list`).
- `shell/tests/fake-daemon.py` implements the commands; the headless smoke
  test drives add → test → save → appears in the picker → remove.

### 3. Docs

README Providers section: adding a provider from a preset, where the key
lives, that tests send one request to the provider.

## Alternatives rejected

- **Keep Nix-only custom providers:** a user choice should not need a
  rebuild; the intent asks for the overlay.
- **A free-form "custom provider" form without presets:** the
  `CustomProvider` fields (wire, auth header, response path) are easy to get
  wrong; presets fill them. A fully free form can come later on the same
  commands.
- **Testing automatically on save:** would send the key out without an
  explicit action; kept as a button.

## Risks

- **Removing a provider in use:** handled by the fallback and notice; Nix
  fixed settings are never changed.
- **Test connection sends the key to that provider:** the purpose of the
  button; only on click, never in local-only sessions, endpoint rule
  applies.
- **Preset data ages** (a model id is renamed upstream): the model field
  is editable, and Test connection shows the provider's error.
- **No host change:** nothing changes until a user adds a provider.

## Verification

- Unit tests (daemon, fake HTTP servers as in `tests/provider_flow.rs`):
  `presets.list` shape; add/update/remove round trip; validation errors
  (bad endpoint, missing key for a remote preset); agent/mcp refusal; the
  key never in any reply or event; remove of the selected provider falls
  back with a notice and refuses for a Nix-fixed setting; `providers.test`
  ok and failing against a local fake server, error text scrubbed;
  `providers.list` includes the custom provider.
- Headless shell smoke test of the add → test → save → select → remove flow.
- `cargo fmt`, `clippy -D warnings`, `cargo test --workspace`,
  `bash shell/tests/ci.sh`, `nix flake check`.
