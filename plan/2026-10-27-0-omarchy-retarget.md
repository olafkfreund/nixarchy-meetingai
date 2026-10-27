---
status: approved
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
spec: spec/2026-10-27-0-omarchy-retarget.md
reviewers:   # extra read-only reviews on top of the Opus review; the owner asked for both as team mates
  - codex
  - agy
---

# Plan: Retarget lennette to nixarchy-meetingai (Rust daemon + Quickshell UI)

## Approved decisions (copied from the spec, so no other file is needed)

- **Shape:** the Rust crate becomes a headless daemon, `nixarchy-meetingai`,
  run as a systemd user service. All GPUI UI is deleted. The UI is a
  Quickshell (omarchy-shell) plugin, `nixarchy.meetingai`, in `shell/`.
- **IPC:** a unix socket at
  `$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock` (directory `0700`),
  newline-delimited JSON.
  - A request is `{"id":n,"cmd":"…","args":{…}}`; its reply is
    `{"id":n,"ok":true,"data":…}` or `{"id":n,"ok":false,"error":"…"}`.
  - Events go to every client as `{"event":"…",…}`.
  - Lines are capped at 1 MiB, attachments are passed as paths, unknown
    commands are rejected, and keys never cross the socket (`has_key: bool`
    only).
- **Commands:** `session.new|restore|list|delete`, `quit`, `record.start|stop`,
  `record.toggle_mic|toggle_system`,
  `chat.send{text,attachments,screenshot,no_context}`, `chat.cancel`,
  `dictate.start|stop`, `screenshot.capture{region}`,
  `attachment.preview{path}`, `settings.get|set|search`,
  `profiles.list|save|delete`, `prompts.get|set{mode,kind}`, `providers.list`,
  `models.list{provider}` (cached), `notes.get{meeting}`,
  `notes.regenerate{meeting}`, `state.get`.
- **Events:** `state`, `transcript.partial|final{who,speaker,text,t0,t1}`,
  `answer.delta|done|error{turn}`, `suggestion{text}`,
  `notes.updated{meeting}`, `error{message,range?}`.
- **Providers:**
  - `claude-sub` spawns `claude -p --input-format stream-json --output-format
    stream-json --include-partial-messages --verbose --tools ""
    --strict-mcp-config --no-session-persistence --settings
    '{"disableAllHooks":true}' --system-prompt <ours> --model <m>`. Text comes
    from `stream_event`/`content_block_delta`/`delta.text`; the end is the
    `result` line.
  - `codex-sub` spawns `codex exec --json -c forced_login_method=chatgpt -s
    read-only --ephemeral --skip-git-repo-check -C <empty tmp> -m <m> [-i
    img]… -`. The answer is the `item.completed` with `item.type ==
    "agent_message"`, delivered as one delta.
  - Both run with `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` and
    `OPENAI_API_KEY_FILE` removed from the environment, in an empty temp dir,
    in their own process group (killed on cancel).
  - `gemini` keeps the existing API-key path.
  - `ollama` uses `http://127.0.0.1:11434/v1/chat/completions`, no key, with
    models from `/api/tags`.
  - Merge Gateway is removed. The native Anthropic wire sends `x-api-key`.
    `http://` is allowed only for the loopback hosts `localhost`,
    `127.0.0.1` and `::1`.
- **STT:** `whisper-local` (the new default) does a multipart `POST
  /inference` (`file`, `response_format=json`, `language`) to `whisper-server`
  at `127.0.0.1:8178`, through the existing batch adapter. A loopback failure
  is retried 3× with backoff, then emits `error` with the lost time range.
  Deepgram stays. The model defaults to `ggml-small.en`. Server build:
  `whisper-cpp-vulkan` (AMD RX 7900).
- **Notes:**
  - During recording, every 8k characters of new final lines, or every
    5 min, summarise that slice into `meeting_note_chunks(meeting_id, t0, t1,
    body)` and emit `notes.updated`.
  - On stop, summarise the leftover slice, then consolidate **all chunks**
    into Summary / Decisions / Action items.
  - Interview mode consolidates as question → answer → notable signals, with
    no verdict or score.
  - Recap prompts are editable. The 120k-character excerpt recap is removed.
- **Calls:** watch `$XDG_RUNTIME_DIR/meeting-mode.json`. When `active` turns
  true, notify "Take notes / Not now". The setting `auto_record_on_call`
  (default off) starts recording straight away instead. Recording is never
  stopped automatically when the call ends; the notification only offers to
  stop it.
- **Removed:** GPUI UI, screen-share hiding, Windows, tray, autostart,
  self-update, own shortcut code, `xcap` (use `grim` / `slurp`), `web/`,
  release tooling, Merge Gateway, meeting tone (sentiment), and the UI-only
  settings `theme`, `transparency`, `font_scale`, `show_borders`,
  `always_on_top`, `autostart`, `overlay_size`, `startup_privacy` and
  `hide_app_icon`.
- **Rename:** crate, binary, socket, data dir and app id become
  `nixarchy-meetingai` (Rust crate path `nixarchy_meetingai`).
  `ProjectDirs::from("dev","nixarchy","nixarchy-meetingai")`. No data
  migration (no lennette data exists on this machine).
- **License:** `Cargo.toml` `license` is set to match `LICENSE` (AGPL-3.0).
  README credits upstream lennette (parazeeknova).
- **Theming:** QML uses the shell's `qs.Commons` `Color` singleton. Markdown
  is rendered with `Text.MarkdownText`, with no syntax highlighting.

## Repo traps (apply to every step)

- `lefthook.yml` pre-commit runs `scripts/bump-version.sh` on **every
  commit**. Commit with `SKIP_BUMP=1` until step 2 removes the bump hook.
- Pre-commit runs `cargo clippy --workspace --all-targets --all-features -- -D
  warnings`, so every step must be clippy-clean. Run inside `devenv shell`
  (toolchain 1.99.1, runtime libraries via `LD_LIBRARY_PATH`).
- The lefthook `web` command runs `bun` in `web/`. Remove it in the same
  commit that deletes `web/`.
- `src/features/mod.rs` re-exports modules (`features::listen`,
  `features::chat_session`, …). Fix those paths when you delete or move a
  module.
- Each step leaves `cargo test` green. A step whose work needs more than one
  commit keeps the tree building after each one.

## Steps

1. **Rename and license.**
   - `Cargo.toml`: `name = "nixarchy-meetingai"`, `license = "AGPL-3.0-only"`
     (check the `LICENSE` header for "or later" and match it),
     `description`, `authors`.
   - Replace `lennette::` with `nixarchy_meetingai::` in `src/main.rs` and
     `tests/*.rs`.
   - Set `ProjectDirs::from("dev","nixarchy","nixarchy-meetingai")` in
     `src/app.rs:82`, `src/core/security/vault.rs:58` and `src/cli.rs:146`.
   - App id `dev.nixarchy.meetingai` in `src/app.rs:180,277,367` and
     `src/platform/shortcuts/wayland.rs:247`.
   - Rename `packaging/dev.lennette.overlay.desktop` and
     `LENNETTE_RUNTIME_LIBRARY_PATH` (rg it), and set `devenv.nix` `name`.
   - User-visible strings "lennette" become "nixarchy-meetingai".
   → verify by `cargo test && rg -il lennette src tests Cargo.toml devenv.nix`
   (expect no hits outside asset filenames, which go in step 8).
   Traps: SKIP_BUMP=1; `Cargo.lock` regenerates.

2. **Delete what is not ours (non-UI).**
   - Remove `web/`, `tools/` (and `members = ["tools"]` in `Cargo.toml`),
     `packaging/hyprland/`, `packaging/windows/`, `docs/windows.md`,
     `build.rs`, `.github/workflows/release.yml`, `scripts/*.ps1`,
     `scripts/bump-version.sh`, `scripts/install-binary.sh`,
     `tests/install-ps1.test.ps1` and `tests/install-sh.test.sh`.
   - Remove the `*_windows.rs` files: `src/audio/capture/system_audio_windows.rs`,
     `src/platform/shortcuts/{windows,control_windows}.rs` and
     `src/platform/system/tray_windows.rs`.
   - Remove every `#[cfg(windows)]` / `target_os = "windows"` branch. There
     are 13 files: `rg -l 'cfg\((not\()?windows|target_os = "windows"' src`.
     For `cfg(not(windows))`, keep the body and drop the attribute.
   - Remove the Windows `[target]` deps (`windows-sys`, `embed-resource`,
     the Windows `gpui` line).
   - Remove privacy mode: `--privacy`/`--normal` (`src/cli.rs:23-24,48`),
     the `privacy-mode`/`normal-mode` actions in `src/platform/shortcuts`,
     the settings `startup_privacy` and `hide_app_icon`
     (`src/core/models/settings.rs`: struct, defaults, keys, load, save,
     test fixture at ~638), and their uses in `src/ui/views/overlay.rs` and
     `src/ui/views/settings/{mod,search}.rs`. Delete only the code needed to
     compile; that UI goes away in step 8.
   - Remove the tray, startup and updates modules
     (`src/platform/system/{tray,startup,updates}.rs`, the `ksni` dep) and
     their call sites (`overlay.rs` `poll_tray` ~2070, settings).
   - Remove the `bump-version` and `web` commands from `lefthook.yml`, and
     the `just` recipes that call the deleted scripts or `web`.
   → verify by `cargo test`, `cargo tree -i ksni` and `cargo tree -i
   windows-sys` (both "not found"), and `rg -i
   'privacy|stealth|exclude.?from.?capture|cfg\(windows' src` (no hits).
   Traps: SKIP_BUMP=1 for this commit (the hook is still live until it
   lands). `src/platform/display/window.rs` keeps compiling without its
   Windows half; it is deleted in step 8.

3. **Provider catalog.**
   - `src/ai/catalog/providers.rs`: remove `merge-gateway` (line ~37) and
     add `ollama` (OpenAI wire, `requires_key` false, endpoint as above).
     Replace the test `only_current_providers_are_available` (~212) so it
     asserts `["gemini","ollama","claude-sub","codex-sub"]`; the CLI entries
     are added in step 4, so assert the step-3 subset here and extend it in
     step 4.
   - `src/ai/catalog/models.rs:17`: remove Merge. For Ollama, discover models
     via `GET /api/tags` (`models[].name`).
   - `src/ai/chat/engine.rs:279`: for `ChatWire::Anthropic` without a
     custom auth, send `x-api-key: <key>` instead of `bearer_auth`. Add a
     test that asserts the header.
   - `src/core/security/credentials.rs:87`: accept `http` only when
     `url.host_str()` is `localhost`, `127.0.0.1` or `[::1]`. Apply the same
     rule to bundled endpoint overrides (`providers.rs` ~123). Add tests for
     a loopback and a non-loopback URL.
   - Remove Merge from the settings UI and `tests/provider_flow.rs`.
   → verify by `cargo test` and `rg -i merge src tests` (no provider hits).

4. **CLI subscription providers.**
   - Add `ChatWire::Cli` and two catalog entries (`claude-sub` "Claude
     (subscription)", `codex-sub` "ChatGPT / Codex (subscription)";
     `requires_key` false; `supports_images` true).
   - New `src/ai/chat/cli.rs`:
     - Build the argv exactly as in the decisions.
     - Spawn with `tokio::process::Command`: `env_remove` the three key
       variables, `current_dir(tempdir)`, `process_group(0)`, stdin piped.
     - Claude: write one stream-json user message
       (`{"type":"user","message":{"role":"user","content":[…text,
       image base64 blocks…]}}`), then close stdin. The history is
       flattened into the system prompt as a quoted transcript.
     - Codex: write the prompt text to stdin and pass images as `-i <tmp
       file>`.
     - Parse stdout lines into deltas. A non-zero exit, or no `result` /
       `agent_message`, becomes `ChatError` carrying the stderr tail (≤2 KB).
     - Cancel: `killpg` the group (`libc` is already a dep).
   - `engine.rs` `stream_chat_inner` dispatches `ChatWire::Cli` to `cli.rs`.
   - Model lists are static: claude `sonnet`, `opus`, `haiku`; codex the
     CLI default plus free text.
   - On first use, run `claude --version` / `codex --version`; a missing
     binary becomes a clear error.
   - Tests: parser unit tests against fixtures
     `tests/fixtures/claude-stream.ndjson` and `codex-exec.ndjson`, recorded
     with the commands in the decisions (prompt "Say: hi there"); and a
     cancel test using a fake script that sleeps.
   → verify by `cargo test cli` and one manual `cargo run -- ctl` call later
   (step 7).
   Traps: never pass `--dangerously-*` flags; codex sandbox stays
   `read-only`.

5. **whisper.cpp STT.**
   - `src/ai/catalog/providers.rs`: add an `SttWire::WhisperServer` entry
     `whisper-local` (endpoint `http://127.0.0.1:8178/inference`, no key) and
     make it the default `speech_provider` in `settings.rs`.
   - `src/speech/stt/transcription.rs`:
     - `connect_config` (~474) routes `whisper-local` to `connect_batch`
       (~519).
     - `transcribe_samples` gains a multipart branch (`reqwest` `multipart`
       feature; WAV from `audio::dsp::encode_wav`) that reads JSON `text`.
     - In `connect_batch`, a request error for a loopback provider retries
       3× (250 ms, 1 s, 3 s) before `LiveEvent::Failed`. The failure message
       includes the chunk's `start..end`.
   - Test: a mock `/inference` server (the pattern in
     `tests/network_resilience.rs`) returns text; another mock fails twice
     and then succeeds (expect one line, no failure); a third always fails
     (expect `Failed` carrying the range).
   → verify by `cargo test transcription network_resilience`.

6. **Rolling notes; remove meeting tone.**
   - New migration in `src/db/schema`: table `meeting_note_chunks(id,
     meeting_id, t0, t1, body, created_at)` with an index on `meeting_id`.
     Repo functions go in `src/db/repos/meetings.rs`.
   - `src/ai/recap.rs`:
     - `summarise_slice(lines) -> body` uses a decisions / actions / open
       questions / key quotes prompt.
     - `consolidate(chunks, mode)` uses the General or Interview prompt;
       prompts are read from settings keys `recap_prompt_general` and
       `recap_prompt_interview`, with the current text as default.
     - `generate` = summarise any lines after the last chunk's `t1`, then
       consolidate. Remove the use of `transcript_excerpt(…,120_000)`, and
       the function itself if nothing else uses it (rg).
   - A trigger function `maybe_chunk(meeting)` (characters since the last
     chunk ≥ 8000 or ≥ 5 min) is called by the daemon in step 7.
   - Delete `src/ai/sentiment.rs`, `src/db/repos/sentiment*`, the sentiment
     schema (leave old tables in existing DBs alone; don't drop them), and
     `src/ui/views/dashboard/meeting_tone.rs` plus its nav entry.
   - Test: build a 3-hour synthetic meeting (≈ 400k characters) with a
     marker decision "DECISION-MIDPOINT" at the midpoint. Use a fake
     provider that echoes any `DECISION-*` it sees. Assert the consolidated
     recap contains it.
   → verify by `cargo test recap`.

7. **Daemon + IPC (port the orchestration out of `overlay.rs`).**
   - New `src/daemon/{mod.rs,protocol.rs,session.rs,server.rs}`.
   - `protocol.rs`: serde types for every command and event in the
     decisions.
   - `server.rs`:
     - Create the `0700` dir, bind the socket (remove a stale one), and
       accept many clients.
     - Per client: a `BufReader` line loop with a 1 MiB cap (oversize closes
       the client).
     - Events go out through a `tokio::sync::broadcast`.
   - `session.rs` owns `ListenSession`, `ChatSession`, `Conversation`,
     `Database` and `Vault`. Port the logic of these `src/ui/views/overlay.rs`
     functions, keeping their behavior and tests:
     - `prepare_recording_resume` 876, `spawn_transcription` 888,
       `completed_turn` 997, `handle_live_event` 1022, `push_transcript` 1099
     - `create_meeting` 1166, `begin_send` 1335, `interrupt_response` 1524
     - `automatic_speech_active` 1595, `answer_pending_question` 1606
       (pacing Fast / Balanced / Relaxed), `selected_system_prompt` 1655
     - `handle_session_event` 1856, `cancel_follow_ups` 2697 /
       `visible_follow_ups` 2897
     - `generate_conversation_title` 2784, `generate_title` 2812,
       `finish_session_title` 2867, `finish_meeting_artifacts` 2875 (calls
       step 6 `generate`)
     - `refresh_configuration` 2999, `speech_config` 3064,
       `selected_chat_config` 3093, `select_profile` / `apply_profile`
       3754-3771, `load_model_catalog` 3833 (cache)
     - dictation 3900-3971, `effective_prompt_id` 4166
   - Call `recap::maybe_chunk` after each final line, and emit
     `notes.updated`.
   - Screenshots: `screenshot.capture{region:false}` runs `grim -`;
     `region:true` runs `grim -g "$(slurp)" -`. The PNG becomes the same
     attachment type `attach_capture` (1793) produced.
   - `src/main.rs`: no args runs the daemon (single instance via the
     existing `cli::InstanceLock`). `ctl <cmd> [json]` connects, sends one
     request, and prints the reply. Retire the old `--toggle-*` flags and
     `control.rs` (replaced by the socket).
   - Tests in `tests/daemon_flow.rs`:
     - Start the server on a temp `XDG_RUNTIME_DIR` with fake STT and AI
       (reuse the mocks from steps 4-5).
     - Drive `session.new → record.start → (fake final lines) → chat.send →
       answer.delta+ → answer.done → record.stop → notes.updated`.
     - An oversize line closes the client.
     - `settings.get` never contains a key.
   → verify by `cargo test daemon_flow`, then manually run `cargo run &` and
   `cargo run -- ctl state.get`.
   Traps: GPUI still compiles in this step. Leave `overlay.rs` alone; its
   copies of the logic die in step 8.

8. **Delete the GPUI UI.**
   - Remove `src/ui/`, `src/app.rs` (move `Services` construction into
     `daemon::session` if anything still uses it), and
     `src/platform/display/{window,capture,screen}.rs`.
   - Remove `src/platform/system/gpu.rs`, and
     `src/platform/shortcuts/{wayland,wayland_stub,hyprland,bindings,commands,control}.rs`,
     keeping anything `ctl` needs.
   - Remove `src/assets/` except the icon PNG used by the `.desktop` file
     (rename it to `nixarchy-meetingai.png`), `src/features/mod.rs`
     re-exports of deleted modules, and `pub mod ui; pub mod app;` in
     `src/lib.rs`.
   - Remove the deps `gpui`, `lucide-icons`, `two-face`, `pulldown-cmark`,
     `raw-window-handle` and `xcap`.
   - Remove the UI-only settings listed in the decisions (struct, defaults,
     keys, load, save, tests).
   - `tests/provider_flow.rs:531-535` uses `ui::markdown`: delete that
     assertion block and keep the rest of the test.
   - Trim `packaging/runtime-libraries.nix` to what cpal (alsa/pipewire) and
     SQLCipher need.
   → verify by `cargo test`; by `cargo tree -i gpui`, `cargo tree -i xcap`,
   `cargo tree -i two-face` and `cargo tree -i ksni` (all "not found"); by
   `rg -i 'lennette|merge-gateway|privacy|stealth' src tests` (no hits); and
   by `cargo build --release` (note the binary size in the PR).

9. **Quickshell plugin `shell/`.**
   - `shell/manifest.json`: id `nixarchy.meetingai`, `kinds [service,
     overlay, bar-widget]`, entry points as below. Copy the structure of
     `~/.config/omarchy/plugins/nixarchy.meeting-mode/manifest.json`.
   - `Service.qml`:
     - One `Quickshell.Io` `Socket` with `SplitParser{splitMarker:"\n"}`
       (pattern: `~/.config/omarchy/plugins/ronnie.worldradio/Panel.qml:1502`)
       and reconnect backoff of 0.5 s → 5 s.
     - `send(cmd,args)` returns a promise-like callback keyed by `id`.
     - The model holds state, transcript (ListModel, capped at 500 lines)
       and answers.
   - `BarWidget.qml` (pattern: `meeting-mode/BarWidget.qml`): a dot and
     timer while recording. Click opens a menu with New / Restore / Quit;
     Quit asks for confirmation inline.
   - `Overlay.qml`: the transcript list (mic and system visually distinct,
     speaker label), the answer stream with `Text.MarkdownText`, follow-up
     suggestions, and selectors (provider/model with a filter field,
     General/Interview, pacing, "No context").
   - `Composer.qml`:
     - Grows to 5 lines, then scrolls; Enter sends, Shift+Enter adds a line.
     - Code or pastes over 2000 characters become square chips with a
       scrollable preview popup.
     - Sent references can be reopened from the question bubble via
       `attachment.preview`.
     - The file picker is `QtQuick.Dialogs` `FileDialog`.
   - `Library.qml`: tabs Restore (chats + meetings, with their notes),
     Profiles, Prompts (answer, follow-up, recap General/Interview),
     Providers (has-key state, set key, model list), Settings. One search
     field calls `settings.search` and filters across tabs.
   - Colors come only from `qs.Commons` `Color`. Respect the
     `reduce_motion` setting (animations off when true).
   - Smoke test: `shell/tests/fake-daemon.py` (stdlib only; answers
     `state.get` and streams a fake transcript and answer) and
     `shell/tests/smoke.sh`. The script starts the fake, runs `quickshell -p
     shell/tests/harness.qml` (loads Service + Overlay) for 5 s, and fails on
     any `qml:` error line on stderr.
   → verify by `qmllint shell/*.qml` (clean) and `shell/tests/smoke.sh`
   (exit 0).
   Traps: Quickshell 0.3.2 API. Check each `Quickshell.Io` type in the
   installed `quickshell-io.qmltypes` before use. Keep heavy work in the
   daemon.
   **Look and feel (owner, 2026-10-27): flat, the same as other Omarchy
   plugins.**
   - Build only from the shell's `qs.Ui` components:
     - `Dropdown`; `SearchableDropdown` for the model picker
     - `TextField`, `FilterInput`
     - `Button`, `PanelActionButton`, `ToggleSwitch`, `ButtonGroup`
     - `PanelSectionHeader`, `PanelSeparator`, `PanelToolTip`
     - `PopupCard` for the bar menu
     - `ConfirmDialog` for Quit
   - Do not use QtQuick.Controls `ComboBox`, `Button`, `TextField`, `Menu`
     or `Popup`.
   - Group content with section headers and separators, not bordered or
     nested boxes. The only framed surface is the overlay window itself.
   - Radius is always `Style.cornerRadius`; border width is
     `Style.normalBorderWidth`. No hard-coded `1`, no shadows, no
     gradients, no hard-coded colors.
   - Hover and focus fills come from `Style.controlFill` /
     `Style.controlBorder`; colors from `Commons.Color` roles (`popups.*`,
     `menu.*`, `accent`).
   - Copy the composition of a first-party panel, e.g.
     `<omarchy-tree>/shell/panels/audio/Panel.qml`.
   Deviation (coder D): each entry point (Service, Overlay, BarWidget) has
   its own `Client.qml` socket, because plugin entry points cannot share one
   instance, so the daemon must accept many clients (it already does,
   step 7). The protocol details D assumed are binding for step 7:
   - `state` is flat: `{recording, since(ms), mic, system}`.
   - `settings.get` returns a flat key→value object; `settings.search`
     returns `[{key, label, value}]`.
   - `session.list` returns `[{id, kind:"meeting"|"chat", title}]`.
   - `prompts.get` / `notes.get` return `{text}`.
   - `providers.list` returns `[{id, name, requires_key, has_key}]`;
     `models.list` returns `[string]`.
   - Setting a key is `settings.set{key:"api_key", provider, value}`; the
     daemon stores it in the vault and never echoes it.
   - Setting keys `chat_model` and `meeting_mode` are new.

10. **Call offer and auto-record.**
    - `Service.qml`: a `FileView` on `$XDG_RUNTIME_DIR/meeting-mode.json`
      (`watchChanges`, plus a 5 s reload timer, as meeting-mode does).
    - When `active` goes false → true while not recording: if setting
      `auto_record_on_call`, send `record.start` and notify "Taking notes";
      otherwise run `notify-send -a nixarchy-meetingai -A take=Take\ notes -A
      no=Not\ now`, and on `take` send `record.start`.
    - When it goes true → false while recording: notify with a "Stop notes"
      action. Never stop on its own.
    - Add the `auto_record_on_call` bool (default false) to `settings.rs`
      and the Library Settings tab.
    → verify with the smoke harness: write `{"active":true}` to the fake
    runtime dir and assert `record.start` reaches the fake daemon only when
    the setting is on.

11. **Nix packaging and docs.**
    - `flake.nix` at the repo root:
      - `packages.default`: `rustPlatform.buildRustPackage`, with
        `runtime-libraries.nix` for the RPATH, and the `.desktop` and icon.
      - `packages.whisper-model-small-en` (and `-small`): fixed-output
        derivations, copying the approach of
        `/mnt/code/Source-home/GitHub/nixarchy-voice/nix/whisper-model.nix`.
      - `homeManagerModules.default`, with options
        `programs.nixarchy-meetingai.{enable, package, whisper.model,
        whisper.package (default pkgs.whisper-cpp-vulkan), whisper.port
        (8178)}`:
        - user services `nixarchy-meetingai.service` (the daemon) and
          `nixarchy-meetingai-whisper.service` (`whisper-server -m <model>
          --host 127.0.0.1 --port <port>`)
        - `xdg.configFile."omarchy/plugins/nixarchy.meetingai".source =
          ./shell`
        - Hyprland binds: overlay toggle via the omarchy-shell IPC for the
          plugin; record, mic, system and screenshot via
          `nixarchy-meetingai ctl …`. Use the bind style the nixarchy skill
          documents.
    - `devenv.nix`: drop `rust-overlay` items that only GPUI needed (none
      expected; check the build).
    - `README.md`: what it is, install (flake input + HM module), the
      providers and how each signs in, offline mode, keybinds, and credit to
      upstream lennette (parazeeknova) under AGPL-3.0.
    → verify by `nix flake check`, `nix build .#default`, and `nixfmt
    --check flake.nix nix/*.nix`.

12. **End-to-end on this machine (session model, not the coder).**
    - Enable the HM module and switch.
    - Run a real call with mic + system audio: whisper lines are labelled,
      a Claude-sub answer streams, a Codex-sub answer arrives, and Ollama
      answers with the network off.
    - Stop: the notes cover the whole call. Restore → resume appends to the
      same meeting.
    - Run the meeting-mode offer both ways.
    - Measure whisper lag on `small.en` (write it in the PR). If it is over
      3 s per utterance, propose `base.en` as the default in the PR (a plan
      deviation → update this file).
    → verify by checking that each item in the spec's Verification §1-7 is
    ticked in the PR description.

## Tests

- `devenv shell -- cargo test --workspace` is green after every step.
- `devenv shell -- cargo clippy --workspace --all-targets -- -D warnings` is
  clean.
- `qmllint shell/*.qml` is clean, and `shell/tests/smoke.sh` exits 0.
- `nix flake check` and `nix build .#default` succeed.
- Guard greps (step 8) return nothing.

## Rollback

All work is on branch `refactor/omarchy-quickshell-rewrite`; `main` is
untouched until the PR merges. Each step is its own commit, so `git revert
<sha>` undoes one step. Before step 8, the GPUI app still builds and runs
from any commit. After merge, `git revert -m 1 <merge>` restores lennette.
There is no data migration, so there is nothing to undo on disk. Disabling
`programs.nixarchy-meetingai.enable` removes the services, plugin and binds.

## Implementation split (managed policy)

Changed at the owner's request on 2026-10-27 ("use dynamic workflows with
max 5 agents in a team"), replacing the single-coder split. All implementers
are `coder` (Sonnet) agents run by a Workflow, at most 5 at once:

- **Wave 1 (in order, main tree):** step 1, then step 2.
- **Wave 2 (in parallel, one git worktree each, merged by the session
  model):**
  - A: steps 3 + 4, plus the `whisper-local` catalog entry from step 5
  - B: step 5 without the catalog entry
  - C: step 6
  - D: steps 9 + 10 QML, built against the protocol in Approved decisions
    and a fake daemon
  - E: step 11 Nix and README

  The Rust setting `auto_record_on_call` from step 10 goes to C, so that
  `settings.rs` is edited in fewer places.
- **Wave 3 (in order):** step 7, then step 8.
- Step 12 and merge-conflict resolution stay with the session model.
- Review after wave 3: a fresh Opus agent gets only this path and `git diff
  main...`, plus Codex `review --base main` and agy, per `reviewers:`.

## Deviations log (recorded by the session model)

- **Worktree base (wave 2):** the workflow's automatic worktrees were
  created from `main` (c873851), not from the task branch, so coders A, B
  and C made no changes. From then on the lead creates worktrees from the
  task branch. Coders cannot commit (guard hook), so the lead commits all
  handed-back work.
- **Step 5:** coder B, not A, owns the `whisper-local` catalog entry. B
  needed it to compile its tests.
- **Step 4:** no separate `--version` probe; a missing binary already fails
  at spawn with a clear `ChatError::Cli`.
  - The Codex model list is `["default"]`, meaning no `-m` flag; any other
    id is passed to `-m`.
  - The Claude fixture has its `system` lines stripped (they contain local
    paths).
  - `ChatProvider::requires_key` is now a field.
- **Step 6:** notes coverage is tracked by time (`end_time > last t1`), so
  a late line with an earlier end time would be missed. **Fixed in wave 3:**
  coverage is tracked by meeting line id.
  - Short meetings (under 8k characters) consolidate the raw transcript
    directly.
  - `transcript_excerpt` stays until step 8, because the UI still uses it.
- **Step 9:**
  - The model picker lost free-typed model ids; this is restored in
    wave 3.
  - The multi-line composer and prompt editor remain QtQuick `TextArea`
    (`qs.Ui` has no multi-line field), drawn with the same flat
    `BorderSurface` as `qs.Ui` `TextField`.
  - `Pill.qml` is deleted in favour of `qs.Ui` `Button`.
- **Step 11:**
  - Binds are a generated `~/.config/hypr/meetingai-binds.lua` that the
    user loads from `bindings.lua`.
  - The flake output is `homeManagerModules` (`nix flake check` warns that
    the name is non-standard).
  - `nix build` fails until step 8 removes GPUI (egl/gbm).

## Wave 3 (owner: "max 5 agents")

In parallel:
- **(a)** step 7, the daemon, in the main tree
- **(b)** the Quickshell UI fixes from the screenshot review, in their own
  worktree:
  - an opaque surface (`Commons.Color.popups.background`)
  - remove the duplicate auto-record row; right-align toggles in a
    label/control grid
  - transcript and answer in one virtualized ListView with no gaps or
    clipping
  - the model trigger shows the current model; free-typed ids accepted
  - a properly run `qmllint` (with `-I` paths)
  - smoke-load `BarWidget` with its `PopupCard` menu
- **(c)** notes coverage by line id, in its own worktree.

Step 8 follows once these are merged.
- **Step 7 (wave 3):**
  - `platform/shortcuts/{mod,control,commands}.rs` and `features/mod.rs`
    stay until step 8, because the GPUI app still uses them.
  - Rolling notes always consolidate on stop; the `auto_recap` setting is
    no longer read.
  - Interval auto-screenshot (`tick_auto_capture`) is not ported. Send-time
    screenshots work, controlled by `use_image_on_send`.
  - After stop, `record.start` is refused until the final flush ends
    ("Finishing the previous transcript").
  - `providers.list` returns chat providers only. Speech keys are set with
    `settings.set{key:"api_key",provider:"deepgram-stt"}`.
  - Socket paths over 108 bytes fail (`SUN_LEN`); the normal
    `/run/user/<uid>` is fine.
- **Wave 3b:**
  - Answers and errors are rows in the client's single ListModel (role
    `who` = mic, system, answer or error).
  - The model picker is a `SearchableDropdown` plus a `TextField` for free
    ids (`SearchableDropdown` cannot expose its typed text).
  - The bar menu loads in the smoke test but has **not been seen
    rendered**; check it in step 12.
- **Wave 3c:** coverage is tracked by `meeting_lines` rowid (arrival
  order), with the column `meeting_note_chunks.last_line` added by a
  guarded `ALTER`.
- **Step 8 additions:** also remove settings nothing reads once GPUI is
  gone: `auto_recap`, `auto_screenshot`, `screenshot_interval`, and any
  other setting with no reader outside `src/ui` (list them in the PR).

## Review fixes (Opus fresh review, Codex gpt-6-sol, agy; each verified by the session model)

Shared protocol additions (daemon and client must agree):
- event `settings{settings:{…flat key→value…}}`, sent to every client
  after any successful `settings.set`
- event `session.reset{}`, sent before `session.new` / `session.restore`
  replay; the client clears its rows
- event `history{rows:[{who:"question"|"answer", text}]}`, sent after a
  chat restore (bounded to the restored messages)
- requests on one connection run in order; events still stream

Fixes:
1. **(critical, agy)** An empty whisper result (a cough or a tap) became
   "no transcript in the response" and stopped the recording. It is now
   treated as silence and skipped. Only a chunk that fails all its retries
   emits `error` with the lost time range.
2. **(P1)** A `settings.set` change is broadcast to every client (the
   Service's stale `auto_record_on_call` could record without asking).
3. **(P1)** `quit` and SIGTERM wait for finalization: track
   `finishing_meeting`; `record.start` and `session.new|restore` are
   refused until it is done.
4. **(P1)** CLI providers clean up on drop: a guard kills the process
   group and removes the temp dir (`0700`, files `0600`).
5. **(major)** `claude-sub` passes the system prompt with
   `--system-prompt-file` in that temp dir, and earlier history goes in the
   stdin user message, never in argv (E2BIG at 128 KiB, `/proc` exposure).
   Model ids starting with `-` are rejected.
6. **(major)** The client shows `{ok:false}` replies centrally as an error
   row and resets `answering`.
7. **(major)** Changing provider resets `chat_model` to "" (the provider
   default).
8. **(major)** Daemon SQLite connections set `busy_timeout` (5 s).
9. `session.new` / `session.restore` emit `session.reset`; a chat restore
   emits `history`.
10. Chunking and `generate` are serialized per meeting (no duplicate
    chunks); a `generate` failure emits `error`.
11. The HM `whisper.port` option is removed; the daemon's endpoint is fixed
    at 8178 (`shortcut: fixed port, add an option only if a clash appears`).
12. HM installs `shell/` without `tests/`.
13. "Set key" with an empty field no longer deletes the key: the UI
    requires text, and the daemon removes a key only on `value: null`.
14. Licenses agree on AGPL-3.0-or-later: manifest and Cargo.toml (nix
    `agpl3Plus` already matches).
15. The dead `cli::control_socket` is removed.

Verified, no change:
- **systemd PATH:** the user manager already imports the session PATH
  (mise wrappers included), so `claude` and `codex` resolve. Re-check in
  step 12.
- **ToggleSwitch double-toggle:** refuted; the owner flips `checked` on
  `toggled()` by design.
- **Overlay bind:** `omarchy-shell shell toggle nixarchy.meetingai '{}'`
  is a valid form.

Logged deviation (step 11): `nix/package.nix` uses `buildInputs` plus a
PATH wrapper (`grim`, `slurp`, `libnotify`) instead of
`runtime-libraries.nix` for the RPATH.

## Owner requests, UI round 2 (2026-10-27)

1. **Rounder corners.** Our own surfaces (the overlay card, answer cards,
   suggestion chips, the composer) use `Math.max(Style.cornerRadius, 12)`
   (Hyprland rounding here is 6). Where a `qs.Ui` component exposes a
   radius, set it the same way; otherwise leave it, and note it.
2. **Settings out of the main view.** The default overlay is the
   conversation:
   - a slim header: Record/Stop, the timer, mic and system levels, a
     General/Interview switch, and a "Settings" tab button
   - the transcript and answers
   - the composer

   Provider, model, pace and "No context" move into a Settings tab next to
   Restore, Profiles, Prompts and Providers.
3. **Answers stand out.** Each answer is its own block: a left accent
   stripe (`Commons.Color.accent`), a small "Suggested answer" label, the
   markdown body, and `onLinkActivated` → `Qt.openUrlExternally` so links
   are clickable.
4. **Recording indicators in the overlay.** A timer since `state.since`,
   plus mic and system level dots. New daemon event: `levels{mic, system}`
   (0..1 RMS), throttled to about 5 Hz, sent only while recording.
5. **Theme audit.** No hard-coded colors, fonts or sizes in `shell/*.qml`
   (tests excepted): everything comes from `Commons.Color`, `Style.font`
   and `Style.spacing`, so theme switches apply live.

## Owner requests, round 3 (2026-10-27)

1. **Frontier models by default; local on demand.**
   - The `chat_provider` default is `claude-sub` with model `sonnet`.
   - The fallback order when the chosen provider is unusable is
     `claude-sub` → `codex-sub` → `gemini` (needs a key) → `ollama`.
     `CHAT_PROVIDERS` and the daemon's fallback follow this order.
   - Ollama stays selectable in Settings and is never picked automatically
     while a frontier provider is usable.
   - Measured warm time to first token on this machine: claude haiku
     1.8–2.2 s, sonnet 3.0 s, codex about 8.7 s (no streaming), ollama
     qwen2.5:7b 0.04 s.
2. **Who said what, at a glance.** Theme tints only (the shell exposes
   `foreground`, `background`, `accent`, `urgent`, `muted` and surface
   roles):
   - **meeting** (system audio): left-aligned bubble, speaker name in bold,
     background `Qt.alpha(foreground, 0.06)`
   - **you** (mic): right-aligned bubble, background
     `Qt.alpha(accent, 0.18)`, "You" label
   - **answers**: the existing striped card, full width
   - **partials**: the same bubble, dimmed and in italics

   Bubbles use the round-2 radius (max of `Style.cornerRadius` and 12)
   and are at most 80% of the width.
3. **Suggestions you can read at a glance.** A pinned strip above the
   composer, visible without scrolling:
   - each suggestion is a full-width row with a number badge **1/2/3** in
     the accent color, the text at body size or larger (not monospace
     caption), and a hover fill from `Style.controlFill`
   - click, or press 1/2/3 when the composer is empty, to send it as a
     question
   - the strip clears when a new answer starts; at most 3 rows

### Round 3, final (replaces items 1 to 3 above; reviewed by Codex gpt-6-sol, agy, and web research; approved by the owner 2026-10-27)

Shared protocol additions:
- event `answer.started{turn, question, automatic, provider, model}`;
  `answer.delta` and `answer.done` also carry `provider` and `model`
- typed questions appear through `answer.started.question`; the client
  shows them as a "You · Asked" bubble
- `suggestion` events carry only the generated follow-ups, at most 3
  (`FIXED_FOLLOW_UPS` are no longer emitted)

Daemon and providers:
- **D1** Typed questions use `chat_provider`/`chat_model` (default
  `claude-sub`/`sonnet`). Automatic live answers use `live_provider`/
  `live_model` (default `claude-sub`/`haiku`).
- **D2** Real fallback when the chosen provider is unusable or fails to
  spawn:
  - typed: claude-sub → codex-sub → gemini (key) → ollama
  - live: claude-sub → gemini (key) → ollama, never codex (no streaming)
  - `claude`/`codex` usability checks that the binary is on PATH
  - the effective provider and model are reported in `answer.started`
  - an explicitly chosen ollama is kept
- **D3** `settings.search` hides keys managed elsewhere: chat_provider,
  chat_model, live_provider, live_model, response_pace, meeting_mode,
  recap_prompt_*, smart_follow_ups_prompt, personal_context,
  meeting_context.

UI (`shell/`):
- **U1** No scrim while recording. Clicks outside the card pass through
  (input mask = card). Click-outside-to-close works only when not
  recording.
- **U2** Layout from top to bottom:
  - the header: Stop/Record, timer, Mic/System level dots (click to
    mute), General/Interview, ⚙
  - a compact transcript, bottom-anchored, with a "Transcript" toggle to
    collapse it
  - a pinned **NOW** panel: who asked, the question, the status
    (Listening / Writing / Ready), and the answer at a larger size than
    the transcript, with the provider/model in small type
  - an **ASK NEXT** strip of up to 3 rows with [1]/[2]/[3] badges at body
    size or larger
  - the composer: `+`, 📷 Screenshot, and a "What should I say?" quick
    action
- **U3** Bubbles: flat and borderless, radius max(`Style.cornerRadius`,
  12), at most 80% of the width.
  - meeting: left, `Qt.alpha(foreground, 0.10)`, label "Them" or the
    speaker's name
  - you: right, `Qt.alpha(accent, 0.22)`, label "You · Spoke" /
    "You · Asked"
  - text at full `popups.text` brightness; partials in italics only, not
    dimmed
- **U4** Keys 1/2/3 send a suggestion only while the overlay has focus,
  the composer is empty and the Library is closed.
- **U5** Reduce motion: no scroll animations; streamed text is still
  appended, without a cursor or animations.
- **U6** Contrast check: compute the ratio of `popups.text` against each
  tinted fill for the current theme and one light theme (`flexoki-light`
  or `catppuccin-latte`); at least 4.5:1, or raise the tint (logged in
  `shell/tests`).
- The step-9 rule is amended: flat, borderless tinted bubbles and cards
  are allowed; nested bordered boxes are still not.

Transcript quality:
- **T1** A benchmark script `tools/whisper-bench.sh`:
  - runs whisper-server with Vulkan on this GPU for `small.en`,
    `medium.en` and `large-v3-turbo` against a public reference set (the
    LibriSpeech test sample on the HF hub, with known text)
  - reports WER and the real-time factor
  - commits the results to `docs/whisper-bench.md`

  The default model is the most accurate one with RTF ≤ 0.3 and p95
  utterance latency ≤ 1.5 s; the Nix default is updated to match.
- **T2** whisper-server requests send `prompt` = the last ~200 characters
  of the same channel's final text, plus names and terms taken from the
  profile/meeting context (≤ 200 characters), and `temperature=0`.
- **T3** Hallucination filter: drop a result that is empty or punctuation
  only, a known silence phrase ("thank you.", "thanks for watching", "you",
  "[blank_audio]", "(silence)" and so on, case-insensitive, whole line),
  or an exact repeat of the previous line on the same channel within 10 s.
  Unit tests cover each case.
- **T4** Echo dedupe: drop a mic line whose normalized text overlaps a
  system line by ≥ 80% token similarity and whose time overlaps within
  ±2 s. Unit-tested.
- Known limit: whisper has no diarization, so all remote speakers are
  "Them" unless Deepgram is chosen (shortcut, to be revisited with a local
  diarizer).

### Owner request: monochrome icons (2026-10-27)

No colored icons or emoji anywhere in the UI.
- Icons are plain text labels (Settings, Screenshot).
- Indicators use the theme text color, with brightness standing in for
  color: the level dots, the outlined 1/2/3 badges, and the bar pill's
  recording dot (formerly `urgent` red).
- The accent tint stays only on surfaces (the "you" bubbles, the answer
  stripe) and on links.
- Error text keeps `urgent`.

### Owner request: more space (2026-10-27)

- Bubbles and cards are padded 16 × 12 px, with 14 px between rows.
- Line height is 1.3×.
- Bubble and earlier-answer text goes up from `bodySmall` to `body`.
- Suggestion rows use the same vertical padding.
- The transcript pins to its exact bottom (`contentY`, not the estimated
  `positionViewAtEnd`) whenever content, an item, or the list itself
  changes height, because the NOW panel resizes the list.
