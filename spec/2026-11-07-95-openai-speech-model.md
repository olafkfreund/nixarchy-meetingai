---
status: approved
issue: 95
intent: intent/2026-11-07-95-openai-speech-model.md
---

# Spec: The OpenAI speech preset keeps working after whisper-1 shuts down

## Design

**1. The preset.** `src/ai/catalog/presets.rs:90`: `model: "whisper-1"` becomes `model: "gpt-transcribe"`. New records made from it get the new model, by way of `Preset::draft` (:114). Same in the shell's test daemon, `shell/tests/fake-daemon.py:191`.

**2. A start-up migration, the pattern the daemon already uses.** A new `migrate_openai_speech(db, vault) -> Option<String>` in `src/daemon/session.rs`, next to `migrate_gemini` (:950), added to the list in `Daemon::new` (:1183). It runs at every start and needs no marker, because once it has run, nothing still matches.
- **Saved records:** for each `credentials::list(vault, Kind::Speech)` (`src/core/security/credentials.rs:230`) with `model == "whisper-1"` and a `base_url` whose host is `api.openai.com`, set `model = "gpt-transcribe"` and `credentials::save` it. Matching on the host, not the full URL, also covers a record whose path was edited.
- **Settings:** if `speech_provider == "openai-compatible-stt"`, `speech_model == "whisper-1"` and `speech_base_url`'s host is `api.openai.com`, set `speech_model` to `gpt-transcribe`.
- Anything pointing anywhere else is left alone: self-run servers also use the name `whisper-1`.
- If anything changed, return one notice: "OpenAI speech now uses gpt-transcribe: whisper-1 shuts down on February 26, 2027." The daemon already joins start-up notices and shows them once (`startup_notice`, :1183).
- A record that fails to save is logged with `tracing::warn!` and left as it was. That doesn't stop the daemon starting, and the next start tries again.

**3. Docs.** The README names no OpenAI speech model today (`rg whisper-1 README.md` finds nothing). Add one line under the speech engines (`README.md:185`), saying a `whisper-1` setting pointing at OpenAI is moved to `gpt-transcribe` at start, and why.

**Not tested live**, as decided: no OpenAI key or call. The request shape doesn't change. The preset sets the endpoint, the multipart wire and `/text` as the response path (`Preset::draft`, :116), and `transcribe_custom` (`src/speech/stt/transcription.rs:277`) builds and sends the request. OpenAI's guide documents `/v1/audio/transcriptions` for `gpt-transcribe`. The PR says so.

## Alternatives rejected

- **Only change the preset.** Saved records would stop working in February, with an error the user has to work out.
- **Map `whisper-1` to `gpt-transcribe` at request time, not in storage.** The stored model and the one sent would differ, so the Providers screen would show the wrong thing.
- **A one-time marker setting, like `migrate_whisper`.** It isn't needed: the match is the marker.
- **Wait until nearer February.** It's small, `gpt-transcribe` is cheaper ($0.0045 against $0.006 a minute), and the owner decided to do it now.

## Risks

- **`gpt-transcribe` might treat `prompt` or `language` differently, or reject one of them.** That's unverified: no call is made. If it fails, the user gets the error. Setting `whisper-1` back by hand doesn't stick: the next start moves it again. Until February, they can use a self-run server or another OpenAI model. It's in the PR, and it's the price of fixing this before the shutdown, as decided.
- **Nix-declared settings:** Nix settings are applied after `Daemon::new` (`src/daemon/mod.rs:41`), so after this migration.
  - A plain Nix default is written once per value. On the first start after it's declared, it can put `whisper-1` back until the next start migrates it again.
  - With `enforce`, `whisper-1` comes back at every start, the migration never holds, and the notice repeats.

  That's left as is: the user chose to force the value. It's an unlikely setup, and the README line tells Nix users to change `settings.speechModel` themselves.
- **Hosts:** p620 has no OpenAI speech record or setting (it uses `parakeet-local`), so nothing changes there.

## Verification

- Unit tests in `session.rs`, using `Database::open_in_memory()` and a temporary vault:
  - an OpenAI-host record with `whisper-1` becomes `gpt-transcribe`, and the notice is returned;
  - a loopback record with `whisper-1`, and an OpenAI record with another model, are unchanged;
  - the settings case is changed, and a self-run base URL is left alone;
  - a second run returns `None`.
  - The preset test (`drafts_are_distinct_and_ready_for_keys`, `presets.rs:160`) still passes, and a new assert checks that the OpenAI speech preset's draft has `model == "gpt-transcribe"`. That assert fails if the preset isn't changed.
  - `rg whisper-1 src shell` then finds only the migration and its tests.
- Each test fails with the migration's matching removed.
- The step check (`cargo test`, `clippy -D warnings`, `fmt --check` in `nix develop`), `bash shell/tests/ci.sh`, and `nix flake check`.
