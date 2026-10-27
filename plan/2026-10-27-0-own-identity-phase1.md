---
status: draft
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
spec: spec/2026-10-27-0-own-identity-mit.md
---

# Plan: our own identity (phase 1 of the identity and MIT spec)

Phase 2 (the rewrite and the licence switch) gets its own plan after this
one lands.

## Approved decisions (copied from the spec)

- Delete `.github/README.md` (lennette's README and remote logo) and
  `src/assets/nixarchy-meetingai.png` (lennette's icon).
- New monochrome icon `packaging/nixarchy-meetingai.svg`: one
  `currentColor` path (a speech bubble with a small dot), square viewBox;
  installed as `share/icons/hicolor/scalable/apps/dev.nixarchy.meetingai.svg`
  by `nix/package.nix` and the `justfile`; the desktop file names
  `dev.nixarchy.meetingai`; the plugin manifest uses it if it has an icon.
- Remove lennette mentions from `README.md` (except the History line),
  `.cargo/config.toml` and code comments; old retarget artifacts stay as
  history.
- README History line: "Started from lennette (AGPL-3.0) by parazeeknova;
  being rewritten. See Licence." Licence line: "AGPL-3.0 while original
  code remains; MIT once the rewrite is complete."
- Rename the GitHub repo `mixarchy-meetingai` → `nixarchy-meetingai` with
  `gh repo rename`; update `git remote`, the README snippet, the flake
  description, and #2504's input URL in `nixos_config`.

## Repo traps

- `nix/package.nix:52` and `justfile:57` install the old PNG; both change.
- The rename is outward-facing; the owner approved it in the intent.
- The #2504 worktree is `~/.config/nixos-2504`; its spec and plan name the
  old URL, so they get a deviation note there.

## Steps

1. **Icon and branding.** Draw the SVG; update `nix/package.nix`,
   `justfile`, `packaging/dev.nixarchy.meetingai.desktop`, the manifest;
   delete the two files; remove lennette mentions; History and Licence
   lines in README.
   → verify by `rg -i lennette -g '!intent/**' -g '!spec/**' -g '!plan/**'`
   showing only the History line, `nix build .#default` with the icon at
   the new path, and the SVG rendered once to a PNG to look at.
2. **Repository rename.** `gh repo rename nixarchy-meetingai`; `git remote
   set-url origin git@github.com:olafkfreund/nixarchy-meetingai.git`;
   README and flake text; note in #2504's spec/plan.
   → verify by `gh repo view olafkfreund/nixarchy-meetingai` and `git
   ls-remote origin`.

## Tests

`nix flake check`, `nix build .#default`, the `rg` check.

## Rollback

Revert step 1's commit; `gh repo rename mixarchy-meetingai` undoes step 2
(GitHub redirects either way).

## Implementation split

Small: the session model does both steps itself.

## Deviations log (recorded with the implementation commit)

None yet.
