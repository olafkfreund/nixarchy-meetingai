---
status: draft
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
author: olafkfreund
---

# Intent: our own identity and an MIT license

## Problem

The project started as a fork of [lennette](https://github.com/parazeeknova/lennette)
(AGPL-3.0, by Harsh Sahu and tutankhAman). It has since become a different
product (headless daemon, Quickshell UI, new providers, modes, MCP), but:

- **Licence:** `git blame` shows ~10,600 of ~27,500 current lines (~38 %)
  still authored by the original authors, in 52 files. Their code is
  AGPL-3.0, so the project cannot be relicensed MIT while it remains, and
  replacing `LICENSE` alone would misstate the rights in the code.
- **Branding:** `.github/README.md` is lennette's README (its logo loads from
  lennette's server), `src/assets/nixarchy-meetingai.png` is lennette's app
  icon renamed (installed by `nix/package.nix:52` and `justfile:57`), and
  `README.md`, `.cargo/config.toml` and the old retarget artifacts mention
  lennette.
- **Name:** the product is `nixarchy-meetingai`, but the GitHub repository
  is still `mixarchy-meetingai`.

Largest remaining original files (original lines / total):

| File | Lines |
|---|---|
| `src/speech/stt/transcription.rs` | 1012/1133 |
| `src/speech/session/listen.rs` | 594/597 |
| `src/ai/catalog/models.rs` | 582/761 |
| `tests/provider_flow.rs` | 538/580 |
| `src/ai/chat/session.rs` | 528/528 |
| `src/ai/chat/engine.rs` | 509/570 |
| `src/db/repos/meetings.rs` | 478/709 |
| `src/audio/capture/microphone.rs` | 437/439 |
| `src/core/models/settings.rs` | 433/681 |
| `src/audio/dsp/mod.rs`, `system_audio.rs`, `conversations.rs`, `vault.rs`, `credentials.rs`, `encryption.rs` | 300–420 each |
| 37 more files | under 300 each |

## Proposed outcome

- **No lennette branding:** `.github/README.md` and the old icon are
  removed; a new monochrome icon of our own replaces it (theme-friendly
  SVG); lennette is mentioned only in the history note below.
- **Repository renamed** to `nixarchy-meetingai` on GitHub (GitHub
  redirects the old URL); flake URLs and docs use the new name.
- **Every line written by us:** each of the 52 files is rewritten from its
  behaviour (the tests, our specs and the protocol), not from the original
  text, until `git blame` on the tree shows no lines by the original
  authors. Tests keep passing at every step.
- **Then MIT:** `LICENSE` becomes MIT (Olaf K. Freund), `Cargo.toml` and
  `nix/package.nix` metadata say MIT, and a short "History" note credits
  lennette as the starting point.

## Affected users and systems

- All of `src/`, `tests/`, `Cargo.toml`, `nix/`, `justfile`, `devenv.nix`,
  the docs, and the GitHub repository name.
- The p620 install (#2504 in `nixos_config`) uses the repo URL; it moves to
  the new name.

## Constraints

- **The licence changes last,** only when no original-author lines remain
  (checked by a script over `git blame`), never before.
- **Rewritten, not reformatted:** renaming variables or reordering code is
  still the original work. Each module is rewritten from its interface and
  tests by a coder that is told not to copy the old body.
- **Behaviour and data stay compatible:** the encrypted database, the vault
  and the socket protocol keep working for existing installs.
- **Legal caveat:** this is an engineering approach, not legal advice; an
  AI-assisted rewrite with the old code in the same repository may not count
  as a clean-room rewrite. The owner may want legal advice or the original
  authors' consent before publishing under MIT.
- Work runs module by module behind the existing test suite; the UI, the
  providers work and the p620 install continue meanwhile.

## Open questions

1. **Order:** branding removal and the repo rename now, the rewrite as its
   own long-running plan afterwards (proposed), or all together?
2. **Icon:** a simple monochrome glyph-style SVG drawn for us (proposed), or
   will you supply one?
3. **History note:** keep a one-line credit to lennette as the starting point
   in the README (proposed; it is honest and costs nothing), or none?
4. **Repo rename:** do it now with GitHub's redirect (proposed), or after the
   p620 install is working?
