---
status: draft
issue: 1
intent: intent/2026-10-27-1-ci-workflow.md
---

# Spec: a GitHub Actions workflow that builds and tests everything

## Decisions (from the approved intent)

1. Free cache: `DeterminateSystems/magic-nix-cache`.
2. The shell suite runs as a `nix flake check` entry.
3. `main` requires the checks once CI is stable.
4. Flaky test #5 is skipped on CI until it is rewritten.

## Design

### `.github/workflows/ci.yml`

- **Triggers:** `push` (all branches), `pull_request`, `workflow_dispatch`.
  `concurrency: ci-${{ github.ref }}` with `cancel-in-progress: true`.
- **Permissions:** `contents: read` only; no secrets are referenced.
- **Setup in every job:** `actions/checkout@v4`,
  `DeterminateSystems/nix-installer-action`,
  `DeterminateSystems/magic-nix-cache-action` (pinned by commit SHA).
- **Jobs (in parallel):**

| Job | Runs |
|---|---|
| `rust` | `nix develop --command cargo test --workspace --no-fail-fast` with `CI=1`, then clippy `-D warnings` and `cargo fmt --all -- --check` scoped to our files |
| `nix` | `nix flake check -L` (module checks, package and plugin builds, docs-match) |
| `shell` | `nix build .#checks.x86_64-linux.shell -L` (below); on failure uploads `shell-artifacts` (crops, `qs.err`, logs) with `actions/upload-artifact` |

- `nix develop` uses a flake `devShells.default` added for CI (the same
  toolchain as `devenv.nix`: rustc/cargo/clippy/rustfmt, pkg-config,
  sqlcipher, alsa/pipewire headers), so CI does not need devenv.

### The shell suite as a flake check

- New input `nixarchy` (`github:olafkfreund/nixarchy`, `follows` nixpkgs)
  used only for its `packages.<sys>.omarchy` (the shell tree); scripts take
  `OMARCHY_SHELL_TREE=<that>/shell` instead of the fixed store path (the
  fixed path stays the local default).
- `checks.<sys>.shell`: a derivation that runs `lint.sh`, `smoke.sh`,
  `keys.sh`, `live-shots.sh` with `MEETINGAI_GEOMETRY=1` for every scenario
  at 100 % and 150 %, and `contrast.py`, inside `headless.sh` (headless sway
  with the pixman renderer, private D-Bus), with fonts from
  `fonts.packages` (JetBrainsMono Nerd Font) via `FONTCONFIG_FILE`.
  `smoke-real.sh` builds the daemon and runs it too.
- **If the Nix sandbox blocks sway or quickshell** (no `/dev/shm`, seat or
  font issues), the check falls back to a non-sandboxed CI step
  (`nix develop .#shell --command bash shell/tests/ci.sh`) and the
  deviation is recorded; the local `nix flake check` then keeps only the
  lint part.
- Crops land in `$out` so a failed check can be inspected.

### Flaky test #5

`#[cfg_attr(ci, ignore)]`-style skip: the test checks `std::env::var("CI")`
and returns early with a message when set, until #5 rewrites it.

### Branch protection (after two green weeks)

`gh api` sets `main` to require `rust`, `nix`, `shell`; not done in this
plan's first PR (recorded as a follow-up issue).

## Alternatives rejected

- **devenv in CI:** slower to set up and adds its own cache; a flake dev
  shell gives the same tools.
- **Cachix:** costs money; the free GitHub cache is enough to start.
- **Self-hosted runner on p620:** would let CI reach the owner's desktop and
  secrets.

## Risks

- **Headless Wayland in the Nix sandbox** may not work; the fallback above
  keeps CI useful.
- **Runner time and cache size:** the Rust build and quickshell closure are
  large; the cache has a 10 GB limit per repository.
- **Fonts** differ from the owner's desktop, so geometry numbers may shift;
  the checks compare alignment within a crop, not pixel positions.

## Verification

- The workflow runs green on a PR from this plan's branch, with all three
  jobs; a deliberately misaligned control and a failing unit test each turn
  the run red (tried on a throwaway branch, then dropped).
- `nix flake check` still passes locally.
