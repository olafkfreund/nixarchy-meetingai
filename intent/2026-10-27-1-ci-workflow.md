---
status: approved
issue: 1
author: olafkfreund
---

# Intent: a GitHub Actions workflow that builds and tests everything

## Problem

Nothing checks a push or a pull request. Every check runs only on the
owner's machine: `cargo test`, clippy, rustfmt, the shell suite (lint,
smoke, smoke-real, keys, geometry, contrast), and `nix flake check`. Now
that issues are enabled and work is tracked there, changes need the same
checks on GitHub before they merge.

Two things make CI harder than "run the tests":

- **The shell tests need the Omarchy shell tree.** Every script defaults
  `OMARCHY_SHELL_TREE` to a store path that exists only on the owner's
  machine (`/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/shell`).
  CI has to build that tree itself, from the public `olafkfreund/nixarchy`
  flake.
- **The shell tests need Wayland.** They already run in a private headless
  sway (`shell/tests/headless.sh`), which works without a display, so they
  can run on a runner.

## Proposed outcome

- A workflow `.github/workflows/ci.yml` on every push to any branch and
  every pull request, with jobs that can run in parallel:
  - **Rust:** `cargo test --workspace`, clippy `-D warnings`, rustfmt check;
  - **Nix:** `nix flake check` (which also builds the package and plugin)
    and the module checks;
  - **Shell:** lint, smoke, keys, the geometry check over the screenshot
    scenarios, and contrast, headless, with screenshots kept as a
    downloadable artifact when a check fails;
  - optionally **smoke-real** (the real daemon against the shell).
- A Nix binary cache for the runners, so builds don't start from zero each
  time.
- Results shown on each pull request; `main` can then require them.

## Affected users and systems

- The repository (`.github/workflows/`), the shell test scripts (to take the
  Omarchy tree from the flake instead of a fixed store path), possibly
  `flake.nix` (a test input or a `checks` entry for the shell suite).
- No change to the app itself.

## Constraints

- No secrets in CI: no provider keys, no subscription logins. Tests use the
  fake daemon and fake providers, as they already do.
- The whisper model (1.6 GB) is not downloaded in CI; nothing in the test
  suite needs it.
- Known flaky test #5 (host audio devices) must not make CI flaky: skip it
  on CI until it is rewritten, or fix it first.
- Free GitHub-hosted runners only, unless the owner chooses otherwise.

## Decisions (owner approved the proposals)

1. Free cache: `DeterminateSystems/magic-nix-cache` (GitHub's cache).
2. The shell suite runs as a `nix flake check` entry, identical locally and
   in CI.
3. `main` requires the CI checks before merging, once CI is stable.
4. Flaky test #5 is skipped on CI until it is rewritten.
