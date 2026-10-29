---
status: approved
issue: 3
spec: spec/2026-10-27-0-own-identity-mit.md
coders:        # owner asked 2026-10-28 for agy and codex to share the coding
  2: agy       # audio/dsp: self-contained maths, small interface
  4: codex     # db encryption/schema: SQL and SQLCipher, codex reads repos well
  5: codex     # db repos: same domain as step 4
  7: agy       # core/models: plain data types and serde
  8: codex     # ai/catalog: provider/model lists, many small functions
  9: agy       # ai/chat: engine and session over the catalog
  # 0, 1, 3, 6, 10, 11, 12 stay with coder (Sonnet): tooling, audio
  # capture, the vault and STT need the p620 checks run from this session
reviewers:     # every step, read-only, on top of the Opus review
  - codex      # codex review --base main, sandbox read-only
  - agy        # agy --mode plan with the allowlist settings overlay
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

## Owner decisions after approval (2026-10-28, asked and answered)

- **Dead code is deleted, not rewritten:** `core/composer.rs` and the
  `audio/dsp` items used only by their own tests (`Segmenter`,
  `VadConfig`, `FlushReason`, `Segment`, `downmix`, `process`,
  `is_passthrough`). An exception to "interface unchanged" for items
  nothing outside their file calls.
- **New default prompt texts:** the default listen-slot prompts, the
  response style and the older upgrade texts are the original authors'
  prose; we write new ones. Prompts a user edited are kept; unedited old
  defaults are recognised by SHA-256 digest and switch to the new text.
  No old prompt text stays in the code.

## Merging so the count is honest

The old file is removed in its own commit, and the new file added in a
later commit; the PR is merged with a merge commit (never squash), so
`git blame` does not match identical lines (blank lines, `}`, `pub mod
x;`, schema columns) back to the old file. This makes the count easy to
game, so the reviewer's check for copied identifiers and comments, and
`--copies`, stay the real guard.

## Briefs (lesson from steps 2 and 4)

Briefs give the interface, the formats and behaviour in our own words.
They never carry the old file's statements, test input values or the
order of its steps: steps 2 and 4 got old test constants and the old
`schema.sql` statements through the brief, and the code came back with
them. Table statements in `schema.sql` are rewritten (only index and
trigger SQL is token-fixed); `tests/old_data.rs` compares columns,
foreign keys and unique keys, so the layout is free.

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

Each step: its own worktree and branch off main and PR, the full suite
(`cargo test --workspace`, clippy `-D warnings`, `cargo fmt --check`,
`checks.shell`), `origin-lines.sh` 0 for the module, CI green.

**Three teams in parallel** (owner, 2026-10-28). Every step keeps its
module's public interface unchanged, so modules can be rewritten at the
same time without breaking each other. Each team works its lane in order:

| Team | Lane (in order) |
|---|---|
| Claude (`coder`) | 0, then 1, 3, 6, 10 |
| Codex | 4, 5, 8 |
| agy | 2, 7, 9 |

- Step 0 merges first; the lanes start after it.
- Step 11 starts when steps 1–10 are merged; step 12 last.
- Merges are serialised by the lead: rebase on main, full suite, CI
  green, merge one PR at a time. A conflict in a shared file (`mod.rs`,
  `Cargo.toml`) is resolved by the lead in the later PR.
- Each step is reviewed by the two teams that did not write it (read-only)
  plus the Opus review.
- Steps 3, 6 and 10 are deployed to p620 and checked with the owner before
  they merge; the other lanes do not wait for that.

## Implementation split

Per step the implementer is named in `coders:` above (agy and codex via
the `handoff` skill in a worktree; the clean-room brief and the forbidden
`git show`/`git log -p` of old files apply to them too). Reviews: codex
and agy per `reviewers:`, one at a time, plus the Opus review below; a
review that is not obtained is reported, not retried with another model.

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

- **Step 0:** `git blame` needs `.git`, which the Nix sandbox source lacks,
  so the measure runs as a separate CI job `origin-lines` (`fetch-depth:
  0`, output in the job summary) instead of `checks.origin-lines`. The
  script also skips `tests/fixtures/`. Its count is 11,099 lines (13,212
  with `--all`); the 10,560 above was a hand estimate. The fixture
  database is 296 KB (SQLCipher page size), above the 200 KB aim. Step 0
  was done by the lead after the `coder` produced no output in an hour.
- **Step 0 review fixes** (Codex, agy, Opus): the synthetic fixture key
  is committed (`.gitignore` exception). `origin-lines.sh` stops on a
  failed blame instead of counting 0, reads `ls-files -z`, and has
  `--copies` (`git blame -M -C -C`, so pasted original code still counts;
  CI prints both, 11,497 today). `old_data.rs` also checks transcript
  lines, the notes search index, that a fresh database has the same
  schema (columns, indexes, triggers) as the old one, that `Settings`
  keys are unchanged (a renamed key fails), and that an old plaintext
  database (`history-plain.db`) is encrypted and kept. "Settings file"
  in step 0 means the settings rows in the database, which is where the
  app keeps them.
- **Step 5** (Codex): the first version followed the old bodies'
  structure in several functions and reused old test values, although
  Codex's command log shows it never read the old files (the original
  project is public, so model recall is the likely source; the brief
  also gave `migrate_legacy`'s steps in order, against the Briefs rule).
  Codex rewrote those functions with a different structure from their
  names only. The kept "ours" tests for note chunks and profile documents
  were replaced by new tests; `format_lines` has its own test again.
  Accepted differences: new marker and error wording, a 16,000/16,000
  head/tail split in `recent_messages`, `sessions::save`,
  `delete_prompt` and `index_notes` in one transaction, rowid
  tie-breaks, `untitled` matching the `Session · ` prefix exactly.
  `--copies` leaves 4 boilerplate lines in `conversations.rs`.
- **Step 7** (agy): the dead-code rule is read as "items nothing outside
  their file calls", so `Mode`, `OverlaySize`, `Role::is_user`,
  `AutoResponse::label`, `RESPONSE_LANGUAGES`, `load_from_path` and
  `save_changed` are deleted. The follow-up prompt gets new text; both old
  defaults are recognised by SHA-256. Only the fixture path is tested: a
  test for the short old default would put its old text back in the code.
  Review fixes by the lead: two `load` helpers and one test rewritten so
  they no longer match the old bodies.
- **Step 9** (agy, then a fresh Claude `coder`): agy's version kept the
  old `recent_history` loop shape and old statement shapes in the
  Anthropic SSE arm, error redaction and the size check. A fresh `coder`
  rewrote `window_history` (two passes), Anthropic event parsing (serde
  types), redaction (`Redactor`), the size check (`fits`), request
  building (`RequestParts`) and the stream loop (`StreamReader`).
  `shorten_middle` keeps the old 1/3 head, 2/3 tail. Accepted
  differences: blank earlier messages and empty deltas are skipped;
  Anthropic skips empty system parts; `ChatError::Request` has no
  `From<reqwest::Error>`; new log and error wording (the matched
  substrings stay). Dead items deleted: `Conversation` draft/submit/
  revision/eviction methods and `is_empty`, `ChatSession::{rename,
  delete}`; `build_body`, `parse_sse_line`, `ChatEvent` made private.
  `--copies` leaves 31 lines in `cli.rs`/`opencode.rs`, outside this step.
  Re-review (ready): also changed, all harmless: `fail` treats a
  whitespace-only partial answer as no answer; the shortening notice is
  4 characters longer; redaction also replaces the trimmed key; a
  trailing `\r` is stripped from a final unterminated stream line.
- **Step 8** (Codex, then a fresh Claude `coder`): Codex's version again
  matched old statements and step order (model recall; its log shows no
  access to the old files). Per the handoff fallback, a fresh `coder`
  rewrote titling cleanup and the title call, model discovery
  (pagination, status and body errors, id collection, auth headers, the
  overall limit), `is_usable`'s custom branch, `speech_usable_in` and the
  preset response path, from the brief. Behaviour kept from the old code:
  a page over 2,000 ids keeps the first 2,000; the 10,000 cap applies
  only when another page follows; a custom config without a `custom-` id
  falls back to the catalogue; an unknown auth value sends the key as is.
  Accepted differences: the title instruction is in the user message as
  the brief says; new error wording; an unencodable key fails the refresh
  before sending. The cache key is pinned by a test. Dead items deleted:
  `models::label`, `models::is_known_provider`,
  `titling::name_untitled_meetings`, `Preset::docs`, which leaves
  `db::meetings::untitled` without a caller.
  Second re-review: `CustomProvider::header`, the Anthropic text join,
  `models_endpoint`, id extraction, title prompt building and the test
  server were reworded again. Also changed: title cleanup strips a
  trailing quote after a full stop and cuts on characters; the title
  prompt uses a `<transcript>` tag; ids are de-duplicated across pages
  before the 10,000 cap; a non-array `data` falls back to `models`; an
  Anthropic reply without `content` reports "no content blocks". The
  `--copies` count is a lower bound: it cannot see code deleted in the
  removal commit and retyped later, so the reviewer's text comparison
  is the real check.
