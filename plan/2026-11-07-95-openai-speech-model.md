---
status: draft
issue: 95
spec: spec/2026-11-07-95-openai-speech-model.md
coders:
  1: coder   # one-value change in two files plus an assert
  2: codex   # migration with matching rules and tests that must fail without them
  3: coder   # one README line
reviewers:
  - codex    # read-only: the migration's matching, and that nothing else is changed
  - agy      # read-only: tests really fail without it; the notice shows once
---

# Plan: The OpenAI speech preset keeps working after whisper-1 shuts down

Branch `fix/95-openai-speech-model`, worktree `../meetingai-95`. Line numbers are from `origin/main` at `202995a`; find each place by its code.

## Decisions carried from the approved spec

- New OpenAI speech records use `gpt-transcribe`.
- At every start, a migration moves `whisper-1` to `gpt-transcribe` only where it points at OpenAI's own host (`api.openai.com`), in saved speech records and in the `openai-compatible-stt` settings. Anything else is left alone, since self-run servers also use the name `whisper-1`.
- It runs with the existing start-up migrations, needs no marker, and returns one notice when it changed something: "OpenAI speech now uses gpt-transcribe: whisper-1 shuts down on February 26, 2027."
- A record that fails to save is logged with `tracing::warn!` and left as it was; the daemon still starts.
- Not tested live: no OpenAI key or call. The PR says so.
- Accepted risks:
  - setting `whisper-1` back by hand on OpenAI gets moved again at the next start;
  - an enforced Nix `speechModel = "whisper-1"` wins over the migration at every start, since Nix settings apply after `Daemon::new` (`src/daemon/mod.rs:41`).

## Steps

1. **The preset.**
   - `src/ai/catalog/presets.rs:90`: `model: "whisper-1"` becomes `model: "gpt-transcribe"`.
   - `shell/tests/fake-daemon.py:191`: the same change to the OpenAI speech preset's `"model"`.
   - In `drafts_are_distinct_and_ready_for_keys` (`presets.rs:160`), assert that the preset named `OpenAI` with `speech: true` drafts `model == "gpt-transcribe"`.

   → verify: the step check below, and `bash shell/tests/ci.sh`.

2. **The migration, `src/daemon/session.rs`.**
   - New `fn migrate_openai_speech(db: &Database, vault: &Vault) -> Option<String>`, next to `migrate_gemini` (:950).
   - For each `credentials::list(vault, Kind::Speech)` where `model == "whisper-1"` and `reqwest::Url::parse(&base_url)` has host `api.openai.com`: set `model = "gpt-transcribe"`, then `credentials::save`. On a save error, `tracing::warn!` and skip.
   - Settings, read with `crate::db::settings::get` as `migrate_whisper` does: if `speech_provider` is `openai-compatible-stt`, `speech_model` is `whisper-1`, and `speech_base_url`'s host is `api.openai.com`, set `speech_model` to `gpt-transcribe`.
   - Return the notice if either changed, else `None`.
   - Add it to the array in `Daemon::new` (:1183–1187), after `migrate_norwegian_engine`.
   - Tests, beside `saved_whisper_local_migrates_once_with_a_notice` (:9562), with `Database::open_in_memory()` and a `Vault::open` in a fresh temp directory (as at :8653):
     - an OpenAI-host record with `whisper-1` becomes `gpt-transcribe`, and the notice is returned;
     - a loopback `whisper-1` record, and an OpenAI record with another model, are unchanged;
     - the settings case is changed; with a loopback `speech_base_url`, it's unchanged;
     - a second run returns `None`.

   → verify: the step check. Then remove the host test, and separately the model test: a test must fail each time.
   Traps:
   - `credentials::list` drops records that aren't usable (`credentials.rs:200`). The preset's records are `custom-…` with `custom: Some`, so they pass. Build test records with `Preset::draft()`, not by hand.
   - Don't hold `self.inner` here: it runs before the daemon exists.
   - Codex's sandbox can't run nix or cargo: the lead runs the check.

3. **README**, after the OpenAI-compatible row of the speech table (`README.md:185`), one line: a `whisper-1` model pointing at OpenAI is moved to `gpt-transcribe` at start, because OpenAI shuts `whisper-1` down on February 26, 2027. A Nix-declared `settings.speechModel` must be changed in Nix.

   → verify: `nix flake check`.

**Step check:** `find target/debug target/release -maxdepth 2 -name 'lib*.so*' ! -perm -u+w -delete; nix develop -c bash -c 'cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check'`

## Tests

- Step 1's assert fails with the preset unchanged.
- Step 2's removals each fail a test.
- `rg whisper-1 src shell` finds only the migration and its tests.
- The step check, `bash shell/tests/ci.sh` and `nix flake check` pass.

## Rollback

Revert the PR's merge commit. Records already moved keep `gpt-transcribe`. To undo one, set its model back in Providers, after removing the migration.

## Deploy

p620 has no OpenAI speech record or setting, so nothing changes in use. It ships with the next routine meetingai bump.
