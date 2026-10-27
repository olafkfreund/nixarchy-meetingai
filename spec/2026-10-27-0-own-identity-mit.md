---
status: approved
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
intent: intent/2026-10-27-0-own-identity-mit.md
---

# Spec: our own identity and an MIT license

## Decisions (from the approved intent)

1. Branding removal and the repo rename first; the rewrite is its own long
   plan afterwards.
2. A monochrome SVG icon drawn for us.
3. A one-line credit to lennette stays in the README.
4. The repository is renamed now; GitHub's redirect covers old URLs.

## Design

### Phase 1: identity (one plan, small)

- **Delete** `.github/README.md` (lennette's README and remote logo) and
  `src/assets/nixarchy-meetingai.png` (lennette's icon).
- **New icon** `packaging/nixarchy-meetingai.svg`: a monochrome glyph (a
  speech bubble with a small dot, single `currentColor` path, square
  viewBox), installed by `nix/package.nix` to
  `share/icons/hicolor/scalable/apps/dev.nixarchy.meetingai.svg` and by the
  `justfile`; `packaging/dev.nixarchy.meetingai.desktop` points at
  `dev.nixarchy.meetingai`. The plugin's manifest icon, if any, uses the
  same SVG.
- **Text:** remove lennette mentions from `README.md` (except the History
  line), `.cargo/config.toml` and any code comments; the old retarget
  intent/spec/plan stay as history.
- **History line** in README: "Started from lennette (AGPL-3.0) by
  parazeeknova; being rewritten. See Licence."
- **Repository rename** `mixarchy-meetingai` → `nixarchy-meetingai` with
  `gh repo rename`; update `git remote`, the README install snippet, the
  flake description, and #2504's input URL in `nixos_config`.
- **Licence status** in README: "AGPL-3.0 while original code remains; MIT
  once the rewrite is complete" (accurate at every point).

### Phase 2: rewrite (separate plan, many steps)

- **Measure:** `tools/origin-lines.sh` counts lines whose `git blame` author
  is Harsh Sahu or tutankhAman, per file and in total; it runs in CI (`nix
  flake check`) and prints the remaining list. It ignores `intent/`,
  `spec/`, `plan/`, `docs/`.
- **Unit of work:** one module (a file or a small group) per step, in
  dependency order from leaves up:
  1. `core/` small files (`security/files.rs`, `core/mod.rs`,
     `network.rs`, `composer.rs`, `context.rs`), `audio/dsp`
  2. `audio/capture/*` (devices, microphone, system audio)
  3. `db/` (encryption, schema seed, repos: settings, library, sessions,
     conversations, profiles, meetings)
  4. `core/security/*` (vault, credentials, provider import)
  5. `core/models/*` (domain, settings)
  6. `ai/catalog/*` (custom, presets, providers, models), `ai/titling.rs`
  7. `ai/chat/*` (conversation, engine, session)
  8. `speech/stt/transcription.rs`, `speech/session/listen.rs`
  9. `cli.rs`, `db/mod.rs`, tests (`provider_flow`, `network_resilience`,
     `live_speech`), `devenv.nix`, `justfile`, `Cargo.toml`
- **How each module is rewritten:** the coder gets the module's public
  interface (signatures, types), its tests, our specs and the protocol,
  and is told to write a new implementation without reading or copying the
  old body. The old file is deleted first in the worktree, so only the
  interface stub and the tests remain. The step is done when the full
  suite passes and `origin-lines.sh` shows zero lines for that module.
- **Compatibility:** the on-disk formats (SQLCipher database, schema,
  vault file, settings keys) and the socket protocol stay byte-compatible;
  each data-touching step adds a test that opens a database written by the
  old code (a fixture produced before the step).
- **Licence switch:** when `origin-lines.sh` reports 0 in total: `LICENSE`
  → MIT (Copyright Olaf K. Freund), `Cargo.toml` and `nix/package.nix`
  `license = MIT`, README licence line updated, History line kept.

## Alternatives rejected

- **Relabelling as MIT now:** misstates the licence of 38 % of the code.
- **Asking the original authors:** the owner chose the rewrite (it remains
  the faster route if they agree; the README can say so).
- **Rewriting by renaming or reformatting:** still the original work.

## Risks

- **Legal:** an AI-assisted rewrite in the same repository may not count as
  clean-room. Not legal advice; the owner may want advice or the authors'
  consent before publishing under MIT.
- **Regressions:** audio capture, STT and the vault are the riskiest
  modules; their steps get extra checks (a real microphone test on p620,
  a database written by the old code reopened by the new).
- **Size:** about 10,600 lines in 52 files; many steps. The UI, providers
  and install work continue meanwhile.

## Verification

- Phase 1: no "lennette" outside history artifacts (`rg -i lennette` check),
  the icon renders in the launcher, `gh repo view olafkfreund/nixarchy-meetingai`
  works, `nix flake check`.
- Phase 2: per step the full test suite, clippy and `origin-lines.sh` for
  the module; at the end `origin-lines.sh` total 0, then the licence
  switch commit.
