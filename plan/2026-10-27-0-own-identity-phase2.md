---
status: draft
issue: 3
spec: spec/2026-10-27-0-own-identity-mit.md
reviewers: []
---

# Plan: rewrite the remaining original code, then MIT (phase 2)

Phase 1 (branding, icon, repo rename) is merged. This plan carries the
spec's phase 2 decisions.

## Approved decisions (copied from the spec)

- **Measure:** `tools/origin-lines.sh` counts lines whose `git blame`
  author is Harsh Sahu or tutankhAman, per file and in total, ignoring
  `intent/`, `spec/`, `plan/`, `docs/`. It runs in `nix flake check`.
- **Unit of work:** one module per step, leaves first (order below).
- **How:** the coder gets the module's public interface, the tests, our
  specs and the protocol, and writes a new body **without reading the old
  one**. The old file is deleted first in the worktree; only an interface
  stub remains. Done when the full suite passes and `origin-lines.sh`
  shows 0 for that module.
- **Compatibility:** on-disk formats (SQLCipher database, schema, vault
  file, settings keys) and the socket protocol stay byte-compatible; each
  data-touching step opens a database written by the old code.
- **Licence switch** only at total 0: `LICENSE` → MIT (Copyright Olaf K.
  Freund), `Cargo.toml` and `nix/package.nix` `license = MIT`, README
  licence line updated, History line kept.

## Size today (measured 2026-10-28)

10,560 lines of code in 63 files (13,212 counting `Cargo.lock`, `LICENSE`
and lock files, which are handled in step 11, not rewritten). Largest:
`speech/stt/transcription.rs` 1013, `speech/session/listen.rs` 594,
`ai/catalog/models.rs` 581, `tests/provider_flow.rs` 538,
`ai/chat/session.rs` 528, `ai/chat/engine.rs` 509.

## Repo traps

- In-file `#[cfg(test)]` tests are original code too: they go with the
  body. Before deleting a file the **lead** writes a behaviour list (one
  line per old test: input → expected) into the coder's brief; the coder
  writes new tests from that list. The lead does not paste old code.
- `tests/*.rs` integration tests are kept until step 10 as the safety net.
- `git blame -w` (whitespace-insensitive), so reformatting never counts as
  a rewrite (spec: renaming or reformatting is still the original work).
- Shell tests run headless only (`shell/tests/headless.sh`).
- Coders cannot commit; the lead commits each step with `SKIP_BUMP=1`.
- Database key file `<db>.key` and the plaintext → encrypted migration in
  `db/encryption.rs` must keep working on existing p620 data.

## Steps

0. **Tooling and fixture.** `tools/origin-lines.sh` (the measure above,
   `--total` prints one number, exit 0); `checks.origin-lines` in
   `flake.nix` prints the list (informational, never fails).
   `tests/fixtures/old-history/`: a database + `.key` + settings file
   written by today's binary (a small meeting, conversation, library doc,
   profile, custom provider) and `tests/old_data.rs` that opens it and
   reads each back. → verify `nix flake check`, `cargo test old_data`.
1. **Small core:** `core/security/files.rs`, `core/mod.rs`,
   `core/network.rs`, `core/composer.rs`, `core/context.rs`, all `mod.rs`,
   `lib.rs`, `main.rs`, `ai/recap.rs`. (~520)
2. **`audio/dsp`.** (~420)
3. **`audio/capture/*`** (devices, microphone, system audio). Extra:
   a real microphone and system-audio recording on p620 with the owner.
   (~1040)
4. **`db/encryption.rs`, `db/schema/*`, `db/mod.rs`.** `schema.sql` keeps
   every table, column, type and default (that is a format, not prose);
   statements are rewritten. Extra: `old_data` passes. (~610)
5. **`db/repos/*`** (settings, library, sessions, conversations,
   profiles, meetings). Extra: `old_data` passes. (~1410)
6. **`core/security/*`** (vault, credentials, provider import). Extra: the
   vault file on p620 reopens (copy, never the live one). (~790)
7. **`core/models/*`** (domain, settings). Settings keys unchanged.
   (~610)
8. **`ai/catalog/*`, `ai/titling.rs`** (models, providers, custom,
   presets). (~1230)
9. **`ai/chat/*`** (conversation, engine, session). (~1210)
10. **`speech/stt/transcription.rs`, `speech/session/listen.rs`,
    `cli.rs`.** Extra: a live whisper session on p620. (~1710)
11. **Tests and build files:** new `tests/provider_flow.rs`,
    `network_resilience.rs`, `live_speech.rs` written from behaviour
    lists; `devenv.nix`, `devenv.yaml`, `justfile`, `Cargo.toml`,
    `.gitignore`, `lefthook.yml`, `rust-toolchain.toml`, `.envrc`,
    `.gitattributes`, `packaging/*`, `scripts/pre-push-checks.sh`
    rewritten or deleted if unused; `Cargo.lock` and `devenv.lock`
    regenerated (`cargo generate-lockfile`, `devenv update`).
    (~1300)
12. **Licence switch** once `origin-lines.sh --total` is 0: `LICENSE`
    MIT, `Cargo.toml`/`nix/package.nix` `license`, README Licence line.
    Close #3.

Each step: its own branch off main and PR, the full suite (`cargo test
--workspace`, clippy `-D warnings`, `cargo fmt --check`, `checks.shell`),
`origin-lines.sh` 0 for the module, CI green, merged before the next step.
Steps 3, 6 and 10 are deployed to p620 and checked with the owner before
the next step starts.

## Implementation split

One `coder` (Sonnet) per step, briefed with the stub and behaviour list;
continued with `SendMessage` while steps share context. Each step is
reviewed by a fresh Opus agent given this plan and the diff, plus one
check the coder did not see: the reviewer greps the new code for
distinctive identifiers and comments from the old file (taken from `git
show main:<file>`) and flags copies.

## Tests

`cargo test --workspace`, `tests/old_data.rs`, `nix flake check`,
`origin-lines.sh --total` reaching 0, p620 checks at steps 3, 6, 10.

## Rollback

Each step is one PR: `git revert` it. The licence switch is its own
commit and reverts alone.

## Risks the owner should weigh before approving

- Legal (from the spec): an AI-assisted rewrite in the same repo may not
  count as clean-room. Not legal advice.
- Cheaper route: if Harsh Sahu and tutankhAman agree to relicense, steps
  0–11 are unnecessary.
- Size: 13 PRs, about 10.5k lines; audio, STT and the vault can regress.

## Deviations log

None yet.
