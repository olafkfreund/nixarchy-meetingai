---
status: approved
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
author: olafkfreund
---

# Intent: a complete flake: every install and configuration option in Nix

## Problem

The owner wants to install and configure meetingai entirely from Nix. Today
the flake and its Home Manager module cover only part of that:

- **Packages:** `flake.nix` exports the daemon and two whisper models
  (`small.en`, `small`). The model the module actually uses by default,
  large-v3-turbo, is not exported, the shell plugin is not a package, and
  the overlay only adds the daemon.
- **Module options:** six (`enable`, `package`, `whisper.model`,
  `whisper.package`, `ollamaCloud.environmentFile`, `keybindings`). The
  whisper port is hard-coded (`hm-module.nix`, "shortcut: fixed port"),
  whisper cannot be turned off for users of a cloud speech provider, the
  daemon cannot get extra environment or a log level, and the planned
  `ollamaCloud.keyFile` does not exist yet.
- **App settings:** everything the user picks in Settings (providers and
  models, mode, pace, follow-ups, auto-record on calls, speech provider and
  model, text size, reduce motion, agent recording consent, profiles,
  prompts) lives only in the encrypted database. None of it can be set from
  Nix, so a new machine starts from defaults and must be clicked through.
- **Plugin install** writes `~/.config/omarchy/plugins/nixarchy.meetingai`
  directly instead of going through `programs.nixarchy.plugins`, which
  validates plugins at build time.
- **Call detection** depends on the meeting-mode plugin, which the module
  does not mention.

## Proposed outcome

- **Packages and overlay:** `packages.<system>` exposes the daemon, the
  shell plugin and every supported whisper model (including
  large-v3-turbo); `overlays.default` adds them all.
- **One Home Manager module** with typed, documented options for:
  - the service (enable, package, start with the session or not, log level,
    extra environment);
  - local speech-to-text (enable, model, package, host and port, threads,
    extra arguments);
  - providers (Ollama Cloud key file or environment file, local Ollama
    address);
  - **app settings** (`programs.nixarchy-meetingai.settings`): the common
    ones as typed options (mode, chat and live provider/model, pace,
    follow-ups, auto-record on calls, speech provider/model, text size,
    reduce motion, agent recording), applied to the daemon from a file the
    module writes;
  - the plugin (through `programs.nixarchy.plugins` when nixarchy is
    present) and its keybindings;
  - call detection (adds the meeting-mode plugin unless it is already
    there).
- **`nix flake check`** evaluates the module with every option set and fails
  on a wrong combination (for example local-only speech with whisper turned
  off).
- **README** documents every option, generated from the module where
  practical so it can't drift.

## Affected users and systems

- Anyone installing meetingai with Nix; the owner's p620 install (#2504 in
  `nixos_config` waits for the key-file option and the plugin
  registration, which this work delivers).
- `flake.nix`, `nix/hm-module.nix`, `nix/package.nix`,
  `nix/whisper-model.nix`, the daemon's settings loading
  (`src/core/models/settings.rs`, `src/daemon/`), README.

## Constraints

- No secrets in the Nix store: keys only by file path.
- Settings set in the UI must not be silently thrown away.
- The plugin's bar placement and `bindings.lua` stay one-time user steps:
  nixarchy and the owner's config keep those user-owned.
- Existing installs keep working with no option set.

## Decisions (owner approved the proposals)

1. Nix settings are **defaults** applied once (still changeable in the
   app), with an opt-in `settings.enforce = true` that re-applies them at
   every start and shows them locked.
2. Profiles and prompts stay in the app for now.
3. No NixOS module; Home Manager only.
4. This lands before the p620 install and replaces its step 1.
