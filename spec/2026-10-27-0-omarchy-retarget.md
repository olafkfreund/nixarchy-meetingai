---
status: draft
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
intent: intent/2026-10-27-0-omarchy-retarget.md
---

# Spec: Retarget lennette to nixarchy-meetingai (Rust daemon + Quickshell UI)

## Decisions taken from the owner (2026-10-27)

| Question | Answer |
|---|---|
| UI | **Rewrite in Quickshell.** GPUI is removed. Rust becomes a headless daemon. |
| Claude / OpenAI | **Subscriptions**, through the `claude` and `codex` CLIs. |
| Gemini | Add it (see Design §4 for the path). |
| Local STT | **whisper.cpp for the live transcript and the final notes.** Works fully offline. |
| Calls | **Offer** to record when a call starts; a setting turns on **auto-start**. |
| Name | **nixarchy-meetingai** |

## Design

```mermaid
flowchart LR
  subgraph QS[omarchy-shell (Quickshell)]
    BW[bar widget: pill] --> SV[service: socket + state]
    OV[overlay: conversation] --> SV
    ST[overlay: settings / library] --> SV
  end
  SV <-- NDJSON over unix socket --> D[nixarchy-meetingai daemon]
  D --> A[audio: cpal mic + system]
  A --> W[whisper-server :loopback]
  A -.optional.-> DG[Deepgram]
  D --> P[claude CLI / codex CLI / Gemini API / Ollama]
  D --> DB[(SQLCipher history)]
  MM[meeting-mode state.json] --> SV
```

### 1. Split: the Rust daemon owns all logic; QML only renders

- Keep `src/{audio,speech,ai,db,core}` almost unchanged. They hold the hard
  parts: capture, DSP, the transcription sessions, the chat engine,
  encryption and the vault.
- Delete `src/ui/` (18.7k lines), `src/app.rs`, and the GPUI dependency.
  Today `src/ui/views/overlay.rs:1943` mixes session orchestration with
  rendering. That orchestration moves into a new `src/daemon/` (session state
  machine + IPC). It is ported, not rewritten: the same calls into
  `speech::session`, `ai::chat` and `db`.
- `src/main.rs` runs the daemon. `nixarchy-meetingai ctl <command>` is a thin
  client for Hyprland binds and scripts. It replaces the flags in
  `src/cli.rs:17-24`.
- Lifecycle: a systemd **user** service, `nixarchy-meetingai.service`. It
  replaces `src/platform/system/startup.rs` autostart and the tray.

### 2. IPC protocol

- Socket `$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock`, in a `0700`
  directory. It replaces the one-way control socket in
  `src/platform/shortcuts/control.rs`. The QML client uses `Quickshell.Io`
  `Socket` + `SplitParser{splitMarker:"\n"}`, the pattern
  `ronnie.worldradio/Panel.qml:1502` uses to talk to mpv.
- Newline-delimited JSON. A request is `{"id":n,"cmd":"…","args":{…}}`; its
  reply is `{"id":n,"ok":true,"data":…}` or `{"id":n,"ok":false,"error":"…"}`.
  Unsolicited events are `{"event":"…",…}` and are sent to every client.
- Commands (one per user-visible feature):
  `session.new|restore|list|delete`, `quit`,
  `record.start|stop`, `record.toggle_mic|toggle_system`,
  `chat.send{text, attachments:[path], screenshot, no_context}`, `chat.cancel`,
  `screenshot.capture{region}`, `attachment.preview{path}`,
  `settings.get|set|search`, `profiles.list|save|delete`,
  `prompts.get|set{mode, kind}`, `providers.list`, `models.list{provider}`
  (cached), `notes.get{meeting}`, `notes.regenerate{meeting}`, `state.get`.
- Events: `state` (recording, provider, mode, mic/system levels),
  `transcript.partial|final{who, speaker, text, t0, t1}`,
  `answer.delta|done|error{turn}`, `suggestion{text}`,
  `notes.updated{meeting}`, `call.detected{app}`.
- Trust boundary: any local process running as this user can reach the
  socket. The socket is not a security boundary against the same user, so the
  daemon only validates input: each line is capped at 1 MiB, attachments are
  passed as paths (read by the daemon, with the existing size limits), and
  unknown commands are rejected. Secrets never travel over the socket:
  `settings.get` returns `has_key: true` and never the key itself.

### 3. Quickshell plugin `nixarchy.meetingai`

Lives in this repo under `shell/` and is installed into
`~/.config/omarchy/plugins/nixarchy.meetingai/` by the Home Manager module.
Its manifest follows `nixarchy.meeting-mode/manifest.json`:
`kinds: [service, overlay, bar-widget]`.

| File | Replaces | Does |
|---|---|---|
| `Service.qml` | tray + `overlay.rs` state | the one socket connection, reconnect with backoff, the state model, watches meeting-mode's `$XDG_RUNTIME_DIR/meeting-mode.json` |
| `BarWidget.qml` | the pill | recording dot + timer; click opens a menu with New / Restore / Quit; Quit asks for confirmation inside the menu |
| `Overlay.qml` | `overlay.rs`, `composer.rs`, `session_menu.rs`, `menus.rs` | live transcript (mic and system shown separately), answers, model picker with filter, mode, pacing, "No context" |
| `Composer.qml` | `composer.rs`, `text_input.rs`, `editor.rs` | grows to 5 lines, then scrolls; Enter sends, Shift+Enter adds a line; code blocks and pastes over 2k chars become square attachment chips with scrollable previews, reopened from the question bubble |
| `Library.qml` | `dashboard/*`, `settings/*` | Restore (chats + meetings), notes, profiles, prompts, providers, settings; one search box across all tabs |

- **Theming:** use the shell's own `qs.Commons` `Color` singleton, the way
  `meeting-mode/BarWidget.qml` does. It already reads
  `~/.local/state/omarchy/current/theme/colors.toml` and follows theme
  changes. No palette of our own; `src/ui/theme/tokens.rs` is deleted.
- **Markdown:** `Text { textFormat: Text.MarkdownText }`. No syntax
  highlighting (`two-face`, `pulldown-cmark` and `syntax.rs` are dropped).
- **Motion:** honour the shell's reduce-motion setting where the shell
  exposes it; otherwise use our `reduce_motion` setting.
- **Keybinds:** the Home Manager module adds Hyprland binds that call
  `omarchy-shell` IPC to toggle the overlay, and `nixarchy-meetingai ctl` for
  recording, mic, system and screenshot.
- **Calls:** when the meeting-mode state turns `active`, the service shows a
  notification with actions "Take notes / Not now". If the setting
  `auto_record_on_call` is on, it calls `record.start` straight away and
  notifies afterwards. The setting defaults to off. When the call ends,
  recording is **not** stopped automatically; the notification only offers
  to stop it.

### 4. AI providers

Today `src/ai/catalog/providers.rs:28` ships Gemini + Merge Gateway; the
engine already has an Anthropic wire. New catalog:

| id | Path | Auth | Streaming | Images |
|---|---|---|---|---|
| `claude-sub` | spawn `claude -p --output-format stream-json --include-partial-messages --verbose --tools "" --strict-mcp-config --no-session-persistence --settings '{"disableAllHooks":true}' --system-prompt <ours> --model <m>`; history and images go in on stdin as `--input-format stream-json` | Claude subscription (CLI login) | yes: `stream_event` → `content_block_delta` (verified) | yes, as base64 blocks |
| `codex-sub` | spawn `codex exec --json -c forced_login_method=chatgpt -s read-only --ephemeral --skip-git-repo-check -C <empty tmp> -m <m> [-i img]… -` | ChatGPT subscription | **no**: the whole answer arrives in one `item.completed` (verified) | yes, `-i` |
| `gemini` | existing OpenAI-compatible endpoint | API key (existing vault) | yes | yes |
| `ollama` | `http://127.0.0.1:11434/v1/chat/completions`; models from `/api/tags` | none | yes | if the model supports it |

- A new `ChatWire::Cli` path in `src/ai/chat/engine.rs` spawns the process
  with a scrubbed environment: `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` are
  removed so the subscription login is used. It runs in an empty temp
  directory with stdin closed after the prompt. `chat.cancel` kills the
  process group.
- The CLI path sends its own prompt text (`--system-prompt`), so the user's
  CLAUDE.md, hooks and MCP servers are kept out. This was verified: with
  hooks off, the stream holds only `init`, deltas and `result`.
- The model list for the CLIs is a short built-in list (aliases such as
  `sonnet`, `opus`, `haiku`; the Codex default). The user can type any other
  model id.
- Remove Merge Gateway (`providers.rs:37`, `models.rs:17`, settings, tests).
- Remove the `ollama` "retired" test (`providers.rs:219`). Ollama is allowed
  again, as a loopback endpoint only.
- Fix the bearer bug: the native Anthropic wire sends `x-api-key`
  (`engine.rs:279`). It stays for custom providers.
- `credentials.rs:87`: `http://` is allowed only when the host is
  `localhost`, `127.0.0.1` or `::1`; any other plain-HTTP endpoint is
  rejected.

### 5. Speech: whisper.cpp first, offline

- Add `SttWire::WhisperServer`: a multipart `POST /inference` (`file`,
  `response_format=json`, `language`) to `whisper-server`, which runs as its
  own systemd user service on `127.0.0.1:8178`, with a GPU-backed build (this
  machine has an AMD RX 7900; use whisper.cpp's Vulkan build from nixpkgs).
- **Live:** reuse the existing batch adapter
  (`src/speech/stt/transcription.rs:519` `connect_batch`). It already keeps
  mic and system apart, closes an utterance on silence, and splits at 15 s.
  Only `transcribe_samples` gains the multipart branch. This gives
  near-live, per-utterance lines, not word-by-word partials.
- **Final notes:** the same transcribed lines are the source. There is no
  second pass in this change (see Rejected).
- Model: `ggml-small.en` by default (multilingual `small` when the language
  is not English), fetched as a fixed Nix derivation the way
  `nixarchy-voice/nix/whisper-model.nix` does.
- Deepgram stays as an optional cloud provider. It is existing, tested code;
  the default switches to `whisper-local`.
- Fall behind or disconnect: today recording stops (`transcription.rs:681`).
  For the loopback server, retry 3 times with backoff before stopping, and
  emit `error` with the time range that was lost.

### 6. Notes you can trust

- The recap is no longer cut to one excerpt. Replace the single
  `transcript_excerpt(…, 120_000)` call (`recap.rs:17`,
  `meetings.rs:259`) with rolling notes:
  - **During recording:** every 8k characters of new final lines (or 5 min),
    summarise that slice into a dated section (`## 10:04–10:12`) of
    decisions, actions, open questions and key quotes. Store it in a new
    table, `meeting_note_chunks(meeting_id, t0, t1, body)`, and emit
    `notes.updated`.
  - **On stop:** take the leftover slice, then consolidate all chunk
    summaries (never the raw transcript) into Summary / Decisions / Action
    items. Every chunk is always included, so nothing in the middle is
    dropped.
- Interview mode uses its own consolidation prompt: question → answer →
  notable signals, with no verdict or score. Recap prompts are editable,
  like the answer and follow-up prompts.
- The transcript stays the source: every note section links to its time
  range.

### 7. Removals (each removed fully: code, settings keys, CLI, tests, deps)

| Remove | Where |
|---|---|
| GPUI UI | `src/ui/`, `src/app.rs`, `src/platform/display/window.rs`, `src/platform/system/gpu.rs`; deps `gpui`, `lucide-icons`, `two-face`, `pulldown-cmark`, `raw-window-handle` |
| Screen-share hiding | `packaging/hyprland/`, `tools/src/hyprland/`, `privacy-mode` / `normal-mode` (`cli.rs:23-24`), `startup_privacy`, `hide_app_icon` |
| Windows | every `#[cfg(windows)]` branch (13 files), `*_windows.rs` (4), `packaging/windows/`, `docs/windows.md`, `scripts/*windows*.ps1`, `scripts/build-windows-installer.ps1`, `tests/install-ps1.test.ps1`, `build.rs`; deps `windows-sys`, `embed-resource` |
| Tray / autostart / self-update | `src/platform/system/{tray,startup,updates}.rs`; dep `ksni` |
| Own shortcuts | `src/platform/shortcuts/{wayland,hyprland,bindings}.rs` (Hyprland binds move to the HM module) |
| Screen capture lib | `src/platform/display/{capture,screen}.rs` → `grim` / `grim -g "$(slurp)"`; dep `xcap` |
| Web + release | `web/`, `tools/` (release/bump), `.github/workflows/release.yml`, `scripts/{bump-version,install-binary}.sh`, `tests/install-sh.test.sh` |
| Merge Gateway | see §4 |
| Meeting tone | `src/ai/sentiment.rs`, `db/repos/sentiment`, `dashboard/meeting_tone.rs`. It is not in the owner's feature list. Say so at review if it should stay. |
| UI-only settings | `theme`, `transparency`, `font_scale`, `show_borders`, `always_on_top`, `autostart`, `overlay_size` (Omarchy provides these) |

Kept: `src/vendor/xattr`, `packaging/runtime-libraries.nix` (trimmed to
what audio and SQLCipher need), `devenv.nix`, `lefthook.yml`.

### 8. Rename and packaging

- Crate, binary, socket, data dir and app id: `nixarchy-meetingai`.
  `ProjectDirs::from("dev","lennette","lennette")` (`app.rs:82`,
  `vault.rs:58`, `cli.rs:146`) becomes
  `ProjectDirs::from("dev","nixarchy","nixarchy-meetingai")`.
- **No data migration:** there is no lennette data on this machine
  (`~/.local/share/lennette` is absent; checked 2026-10-27).
- A `flake.nix` at the repo root exports `packages.default` (the daemon),
  `packages.whisper-model-*`, and
  `homeManagerModules.default` (`programs.nixarchy-meetingai.enable`: user
  services for the daemon and whisper-server, the plugin files, and the
  Hyprland binds).
- License: set `Cargo.toml` `license` to match `LICENSE` (AGPL-3.0). Keep a
  credit line for upstream lennette (parazeeknova) in a new `README.md`.

## Alternatives rejected

- **Keep GPUI, add a Quickshell widget.** This was the recommendation; the
  owner chose the full Quickshell UI.
- **QML-only app with no Rust daemon.** Audio capture, SQLCipher and
  streaming parsers do not exist in QML, so they would be rewritten in shell
  scripts. That throws away the most tested code.
- **Embedding Rust into QML as a QML module (cxx-qt).** It has to be built
  against the shell's exact Qt version, and a crash would take down the whole
  desktop shell. A separate process keeps that from happening.
- **API keys for Claude and OpenAI.** The owner asked for subscriptions.
  The API-key path is kept only as a custom provider.
- **Gemini through `agy` (Google subscription).** agy is an agent with
  browser and `run_command` tools, and no flag to turn them off was found;
  that is too much reach for a meeting assistant. Use the Gemini API key
  path now; revisit if a tool-less agy mode appears.
- **`whisper-stream` (word-by-word live partials).** It owns the microphone
  itself, so the mic and system channel split (and our DSP) would be lost.
- **A second, higher-quality whisper pass over saved audio for final
  notes.** That needs audio stored on disk (it isn't today), which has
  privacy and disk-space costs. Later, if the live lines prove too weak.
- **faster-whisper.** It adds a Python runtime; `nixarchy-voice` already
  uses whisper.cpp.

## Risks

- **This is a rewrite of every screen.** The QML is about 3–5k new lines with
  no test harness today. Mitigation: the daemon gets the tests (protocol,
  session, notes, providers); the QML is checked with `qmllint` plus a smoke
  test that loads the plugin in a Quickshell instance against a fake daemon.
- **A crash in the plugin can disturb the shell.** Quickshell plugins share
  one process with the bar. Keep the QML free of heavy work: the transcript
  list virtualizes (ListView), and the overlay holds at most 500 lines in
  memory.
- **The CLI providers are slow to start.** Each `claude -p` spawn costs
  about 1–3 s to start up before the first token. Codex shows the answer only
  when it is complete. Accept this for subscriptions; Ollama/Gemini remain
  the fast path. Flags can change between CLI releases, so the daemon checks
  `--version` and fails with a clear message.
- **The whisper.cpp model may not keep up on the GPU.** If inference falls
  behind, live lines lag; the retry policy and `error` event surface it.
  Measure with `small.en` on the RX 7900 before choosing a default.
- **Subscription terms.** Driving `claude` and `codex` non-interactively for
  personal use is the documented print mode. It is not a shared service.

## Verification

1. `cargo build --release` and `cargo test` pass with no `gpui`, `ksni`,
   `windows-sys`, `xcap` or `two-face` in `cargo tree`.
2. `rg -i 'privacy|stealth|exclude.?from.?capture|merge-gateway|lennette|cfg\(windows'`
   returns only the upstream credit in README.
3. Protocol tests: a scripted client drives `session.new → record.start →
   chat.send → answer.delta* → answer.done → record.stop → notes.updated`
   against fake STT and fake AI providers.
4. Notes test: a 3-hour synthetic transcript with a decision placed at the
   midpoint; the consolidated notes contain it.
5. Provider tests: the CLI stream parsers are tested against recorded
   `claude` and `codex` output; Ollama is tested with a local mock; an
   `http://` non-loopback endpoint is rejected.
6. `qmllint shell/*.qml` is clean; the plugin loads in omarchy-shell and
   connects; a live run on this machine works end to end: mic + system, a
   whisper transcript, a Claude answer, notes after stop, Restore and append.
7. Call flow: toggling meeting-mode's state file shows the offer; with
   `auto_record_on_call` on, recording starts.
