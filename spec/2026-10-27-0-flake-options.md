---
status: approved
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
intent: intent/2026-10-27-0-flake-options.md
---

# Spec: a complete flake: every install and configuration option in Nix

## Decisions (from the approved intent)

1. Nix settings are **defaults** applied once; `settings.enforce = true`
   re-applies them at every start and the app shows them locked.
2. Profiles and prompts stay in the app.
3. Home Manager only, no NixOS module.
4. Lands before the p620 install and replaces its step 1.

## Design

### Flake outputs (`flake.nix`)

| Output | Content |
|---|---|
| `packages.<sys>.default` / `.nixarchy-meetingai` | the daemon |
| `packages.<sys>.plugin` | the shell plugin folder (`shell/` minus `tests/`) |
| `packages.<sys>.whisper-model-<name>` | every model `nix/whisper-model.nix` defines, including `large-v3-turbo` |
| `overlays.default` | `nixarchy-meetingai`, `nixarchy-meetingai-plugin`, `whisper-models.<name>` |
| `homeManagerModules.default` (and `.nixarchy-meetingai`) | the module |
| `checks.<sys>` | module evaluations (below) and the package build |

### Home Manager options (`nix/hm-module.nix`, prefix `programs.nixarchy-meetingai`)

**Service**

| Option | Type, default | Effect |
|---|---|---|
| `enable` | bool, `false` | everything below |
| `package` | package, flake's | daemon |
| `autostart` | bool, `true` | `WantedBy=graphical-session.target` for both services |
| `logLevel` | enum `error warn info debug trace`, `"info"` | `RUST_LOG` |
| `extraEnvironment` | attrs of str, `{}` | extra service environment |

**Speech-to-text**

| Option | Type, default | Effect |
|---|---|---|
| `whisper.enable` | bool, `true` | the local whisper service; off for Deepgram-only users |
| `whisper.model` | package, large-v3-turbo | model file |
| `whisper.package` | package, `whisper-cpp-vulkan` | server build |
| `whisper.host` | str, `"127.0.0.1"` | listen address (loopback recommended) |
| `whisper.port` | port, `8178` | port; passed to the daemon so it calls the same address |
| `whisper.threads` | null or int, `null` | `-t` |
| `whisper.extraArgs` | list of str, `[]` | extra server flags |

**Providers**

| Option | Type, default | Effect |
|---|---|---|
| `ollamaCloud.keyFile` | null or str, `null` | bare key file (e.g. agenix); `LoadCredential=ollama:<path>`, daemon reads `$CREDENTIALS_DIRECTORY/ollama` first |
| `ollamaCloud.environmentFile` | null or str, `null` | `OLLAMA_API_KEY=...` file, as today |
| `ollama.url` | str, `"http://127.0.0.1:11434"` | local Ollama address (loopback only counts as local) |

**App settings** (`settings.*`, all `null` = leave to the app)

| Option | Setting key | Type |
|---|---|---|
| `settings.meetingMode` | `meeting_mode` | enum `general interview team_meeting interviewer` |
| `settings.chatProvider` / `chatModel` | `chat_provider` / `chat_model` | str |
| `settings.liveProvider` / `liveModel` | `live_provider` / `live_model` | str |
| `settings.responsePace` | `response_pace` | enum (the app's values) |
| `settings.responseLength` | `response_length` | enum |
| `settings.responseLanguage` | `response_language` | str |
| `settings.autoResponse` | `auto_response` | enum |
| `settings.smartFollowUps` | `smart_follow_ups` | bool |
| `settings.autoRecordOnCall` | `auto_record_on_call` | bool |
| `settings.speechProvider` / `speechModel` / `speechLanguage` | `speech_*` | str |
| `settings.textScale` | `text_scale` | int 80–200 |
| `settings.reduceMotion` | `reduce_motion` | bool |
| `settings.mcpAllowRecord` | `mcp_allow_record` | bool |
| `settings.extra` | any key | attrs of str, for keys without a typed option |
| `settings.enforce` | — | bool, `false` |

Enum values are taken from the Rust types at build time (a small generated
JSON committed with a test that it matches), so they can't drift.

**Shell**

| Option | Type, default | Effect |
|---|---|---|
| `plugin.enable` | bool, `true` | install the plugin: through `programs.nixarchy.plugins."nixarchy.meetingai".src` when that option exists, else `~/.config/omarchy/plugins` as today |
| `keybindings` | as today | Hyprland fragment |
| `callDetection.enable` | bool, `true` | adds the meeting-mode plugin to `programs.nixarchy.plugins` unless an entry with that id exists |

### How settings reach the daemon

- The module writes `$XDG_CONFIG_HOME/nixarchy-meetingai/settings.json`:
  `{"enforce": bool, "values": {"key": "value", ...}}` (strings, as the
  settings table stores them). No secrets are ever in it (`api_key` is
  rejected by an assertion).
- At start the daemon reads it and validates every key with the same rules
  as `settings.set` (unknown key or bad value: a warning, then skipped).
- **Defaults:** a key is written to the database only if the database has
  no value for it yet, recorded in a `settings_seeded` table (key, value
  hash), so a later change in the app wins and a changed Nix value is
  applied once more.
- **Enforced:** every listed key is written at every start; `settings.set`
  from the UI or MCP for those keys is refused ("Set in Nix"); `settings`
  replies carry `locked: [keys]` and the Settings controls show them
  disabled with "Set in Nix".

### Checks (`nix flake check`)

- the module with nothing but `enable`, and with every option set;
- assertions: `whisper.enable = false` with `settings.speechProvider =
  "whisper-local"`; `api_key` in `settings.extra`; both `ollamaCloud`
  key options set;
- the generated `settings.json` for the full example matches a fixture;
- a Rust test feeds that fixture to the loader (defaults and enforce).

### Docs

`README.md` gets a "Nix options" section generated by
`nix build .#docs` (`nixosOptionsDoc` over the module) and committed, with a
check that the committed file matches.

## Alternatives rejected

- **Writing the settings database from Nix:** it is encrypted and owned by
  the daemon.
- **Environment variables per setting:** no locking, and too many.
- **A NixOS module:** everything runs as the user.
- **Profiles and prompts from Nix:** deferred by the owner.

## Risks

- **Enum drift** between Rust and Nix: guarded by the generated JSON test.
- **Seeding surprises:** a value changed in the app and then changed in Nix
  is overwritten once (by design, "a changed Nix value is applied once
  more"); documented.
- **Plugin path change** for existing installs: the old
  `~/.config/omarchy/plugins/nixarchy.meetingai` link is replaced by
  nixarchy's; Home Manager handles the switch.

## Verification

- `nix flake check` (all checks above) and `nix build .#default .#plugin
  .#whisper-model-large-v3-turbo`.
- `cargo test` for the settings loader (defaults, enforce, invalid keys,
  `locked` in replies, refusal in `settings.set`).
- Headless shell test: a locked setting shows disabled with "Set in Nix".
- On p620 (#2504): the install uses `ollamaCloud.keyFile` and a couple of
  `settings`, and the app starts with them.
