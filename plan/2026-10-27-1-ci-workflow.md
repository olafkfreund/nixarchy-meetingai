---
status: approved
issue: 1
spec: spec/2026-10-27-1-ci-workflow.md
---

# Plan: a GitHub Actions workflow that builds and tests everything

## Approved decisions (copied from the spec)

- `.github/workflows/ci.yml`: triggers `push` (all branches),
  `pull_request`, `workflow_dispatch`; `concurrency: ci-${{ github.ref }}`,
  `cancel-in-progress: true`; `permissions: contents: read`; no secrets.
  Every job: `actions/checkout`, `DeterminateSystems/nix-installer-action`,
  `DeterminateSystems/magic-nix-cache-action`, all pinned by commit SHA.
- Jobs: `rust` (`nix develop --command` cargo test with `CI=1`, clippy
  `-D warnings`, rustfmt check on our files), `nix` (`nix flake check -L`),
  `shell` (`nix build .#checks.x86_64-linux.shell -L`, artifacts uploaded on
  failure).
- `devShells.default` in `flake.nix` with the `devenv.nix` toolchain
  (rustc, cargo, clippy, rustfmt, pkg-config, sqlcipher, alsa/pipewire
  headers, libclang, the env vars `CPATH`, `BINDGEN_EXTRA_CLANG_ARGS`,
  `LIBCLANG_PATH`, `PKG_CONFIG_PATH`, `SSL_CERT_FILE`).
- New input `nixarchy` (follows nixpkgs) for `packages.<sys>.omarchy`;
  scripts take `OMARCHY_SHELL_TREE` (the fixed local path stays default).
- `checks.<sys>.shell` runs lint, smoke, smoke-real, keys,
  `live-shots.sh` (`MEETINGAI_GEOMETRY=1`, every scenario, 100 % and 150 %)
  and contrast inside `headless.sh`, with JetBrainsMono Nerd Font via
  `FONTCONFIG_FILE`; crops in `$out`. **Fallback** if the sandbox blocks
  sway/quickshell: a non-sandboxed CI step `nix develop .#shell --command
  bash shell/tests/ci.sh`, recorded as a deviation.
- Flaky test #5 returns early when `CI` is set.
- Branch protection is a follow-up issue after two green weeks.

## Repo traps

- `nix flake check` sees only tracked files: `git add -N` new ones.
- Shell scripts run headless only; never `MEETINGAI_ON_DESKTOP`.
- `headless.sh` re-execs inside `dbus-run-session`; inside a Nix build
  there is no session bus to escape from, but the re-exec must still work.
- Pin every action by SHA (look up the current release commit with `gh api
  repos/<owner>/<repo>/git/ref/tags/<tag>`).
- Coders cannot commit; the lead commits and pushes.

## Steps

1. **Dev shell and inputs.** `flake.nix`: `devShells.default` and
   `devShells.shell` (quickshell, sway, grim, python3+PIL, the font, dbus),
   the `nixarchy` input; scripts honour `OMARCHY_SHELL_TREE`.
   → verify by `nix develop --command cargo test --workspace` and `nix
   develop .#shell --command bash shell/tests/lint.sh` locally.
2. **Flaky test skip** (#5): early return on `CI`.
   → verify by `CI=1 cargo test system_audio`.
3. **`checks.shell`**: the derivation and font config; try it locally with
   `nix build .#checks.x86_64-linux.shell -L`. If the sandbox blocks it,
   implement the fallback (`shell/tests/ci.sh`, `devShells.shell`) and
   record the deviation.
   → verify by a green local build (or green fallback script).
4. **Workflow** `.github/workflows/ci.yml` with the three jobs.
   → verify by `actionlint` (in the dev shell) and a push of this branch
   showing all three jobs green (`gh run watch`).
5. **Red checks:** on a throwaway branch, one misaligned control and one
   failing unit test each turn the run red; delete the branch.
   → verify by `gh run list` showing the two red runs.
6. **README** "Development" section: what CI runs; open the follow-up
   issue for branch protection.

## Tests

`nix flake check`, `nix develop --command cargo test --workspace`, the
GitHub run (green on the branch, red on the throwaway).

## Rollback

Delete `.github/workflows/ci.yml`; the dev shells and the `nixarchy` input
are additive.

## Implementation split

One `coder` does steps 1–3 and 6 in a worktree; the lead does steps 4–5
(pushing and watching runs) and commits each step.

## Deviations log (recorded with the implementation commit)

None yet.
- **Steps 1–4:** the shell suite runs inside the Nix sandbox (no fallback
  needed) with `QT_QUICK_BACKEND=software`, `SSL_CERT_FILE` and `curl`;
  `shell/tests/ci.sh` is the entry point. `devShells.default` uses
  nixpkgs' Rust 1.99.0 (not rust-overlay 1.99.1; `rust-toolchain.toml` is
  ignored there) and builds the vendored SQLCipher with perl/gnumake; it
  sets `TMPDIR=/tmp` so test socket paths stay under 108 bytes. `headless.sh`
  honours `SWAY_BIN`, `smoke-real.sh` honours `MEETINGAI_BIN`, `contrast.py`
  reads themes from `OMARCHY_SHELL_TREE`. **#5 fixed instead of skipped:**
  the host-dependent `monitor_sources().len() <= 8` assert is removed (it
  tested the machine, not the parser); no test needs a CI skip.
  `cargo fmt --all` reformatted `src/db/mod.rs` so the full `cargo fmt
  --check` can run in CI. The `nix` job evaluates with `--no-build` and
  builds every check except `shell` (own job); the `shell` job keeps failed
  build dirs (`--keep-failed`) for the artifact upload. Actions pinned:
  checkout v7.0.1, nix-installer v23, magic-nix-cache v15,
  upload-artifact v7.0.2.
