# nixarchy-meetingai

A meeting copilot for [Omarchy](https://omarchy.org) on NixOS (Nixarchy). It
listens to your calls, writes a live transcript, answers questions while you
talk, suggests what to ask next, and turns each meeting into notes you can
search later. It runs on your own machine: transcription is local
(Parakeet), and answers come from the AI subscriptions you already have,
or from local models.

- **Daemon:** `nixarchy-meetingai`, a Rust user service that records,
  transcribes, talks to the AI providers and keeps an encrypted local database.
- **UI:** the Omarchy shell plugin `nixarchy.meetingai` (an overlay and a bar
  widget), built with Quickshell and following your Omarchy theme.
- **Agents:** an MCP server, so Claude Code, Codex or agy can read and drive it.


## Getting started

### 1. Requirements

- NixOS with Nixarchy (Omarchy shell on Hyprland) and Home Manager.
- PipeWire (for the microphone and the call audio).
- At least one way to get answers (see [Providers](#providers)): a Claude or
  ChatGPT subscription CLI, Antigravity, OpenCode, an Ollama Cloud key, or a
  local [Ollama](https://ollama.com).
- About 1.1 GB of free RAM while Parakeet speech-to-text is loaded (CPU only, no GPU needed).

### 2. Add the flake

```nix
{
  inputs.nixarchy-meetingai = {
    url = "github:olafkfreund/nixarchy-meetingai/refactor/omarchy-quickshell-rewrite";
    inputs.nixpkgs.follows = "nixpkgs";
    # Optional: only if you already have this input, so both share one copy.
    inputs.nixarchy-meeting-mode.follows = "nixarchy-meeting-mode";
  };
}
```

Then, in your Home Manager configuration:

```nix
{ inputs, ... }:
{
  imports = [ inputs.nixarchy-meetingai.homeManagerModules.default ];
  programs.nixarchy-meetingai.enable = true;
}
```

Rebuild. This installs the binary, the user service
`nixarchy-meetingai`, the shell plugin, the Parakeet speech model (about
640 MB, downloaded once at build time), the call-detection plugin and a keybinding fragment.

### 3. Two one-time steps

Nixarchy installs plugins but leaves turning them on to you, and your
`bindings.lua` is yours to edit:

```bash
omarchy plugin enable nixarchy.meetingai --section right
```

Add this line to `~/.config/hypr/bindings.lua`:

```lua
pcall(require, "hypr.meetingai-binds")
```

### 4. Sign in to a provider

Sign in once with the CLI you want to use (details in [Providers](#providers)):

```bash
claude            # Claude subscription
codex login       # ChatGPT subscription
agy               # Antigravity (Google subscription)
opencode auth login
```

### 5. First run

Press `SUPER + ALT + M` to open the overlay. If something is missing (the
service is not running, no provider is ready, speech-to-text is down), a
**Finish setting up** card says what to do next. Then:

1. Pick a mode in the header (see [Modes](#modes)).
2. Press the record button (or `SUPER + ALT + R`). A red dot and **REC**
   timer show in the header and on the bar while recording.
3. Ask anything in the composer at the bottom, or let the copilot answer the
   questions the other side asks.
4. Stop (`SUPER + ALT + SHIFT + R`). The notes are written when you stop.

## Modes

Pick one from the mode menu in the header.

| Mode | For | What you get |
| --- | --- | --- |
| **Assist** | any call | answers to the questions asked, suggestions, notes |
| **Interview me** | you are the candidate | short talking points for each question you are asked |
| **Team** | recurring team meetings | actions, decisions and questions pulled out live, linked across a series |
| **Interview someone** | you are the interviewer | a question bank from the CV, follow-up probes, evidence notes |

### Team meetings

Each team meeting belongs to a **series** (for example "Weekly platform
sync"); choose or create one before you record.

- **Items:** actions, decisions and questions appear as you talk, as
  drafts, and are settled when you stop. Edit, mark done, drop, or unlink.
- **Last time:** the previous meeting of the series, its recap and the items
  still open.
- **Earlier task?** When the talk returns to an open item from an earlier
  meeting, a callout offers Discussed, Done or Not related (at most one every
  three minutes).
- **Search:** recaps are searchable together with titles and transcripts.

### Interviews you run

The copilot helps you ask; it never judges the candidate.

- **Before:** in Settings, Candidate, create the candidate, attach the CV and
  the job description, and generate a question bank. Every question quotes
  the CV or job text it came from; questions whose quote is not in the
  documents are dropped. Edit, reorder, add and delete freely.
- **During:** a question you say aloud is marked asked; mark questions
  covered or skipped and keep an evidence note for each. After each answer
  you get one to three follow-up probes that quote what the candidate said.
- **After:** the recap lists each question, what the candidate said and the
  gaps to verify.
- **Limits:** no scores, ratings, rankings, hire or no-hire advice, and no
  guesses about emotion, personality, confidence or honesty. The prompts
  forbid them, and a filter withholds any answer, title, note or generated
  question that contains one (quotes of the candidate's own words excepted).
- **Consent:** recording in this mode always asks you to confirm that the
  candidate has been told. Agents cannot give that consent.
- **Local-only:** switch it on per candidate. Speech-to-text then stays on
  Parakeet (or an OpenAI-compatible server on this machine) and every model call goes to a local Ollama on this machine;
  anything else is refused, and agents cannot read that interview.
- **Deletion:** "Delete candidate…" removes the candidate, their documents,
  questions, the interview's transcript, notes and chats, in one step.

You are responsible for using this lawfully: tell candidates, keep their data
only as long as you need it, and make hiring decisions yourself.

## Providers

| Provider | How to sign in | Live answers |
| --- | --- | --- |
| Claude (subscription) | run `claude` once and log in | yes |
| Antigravity (subscription) | run `agy` once and sign in with Google | typed questions and notes |
| OpenCode | `opencode auth login`; uses any model OpenCode can reach | yes |
| ChatGPT / Codex (subscription) | `codex login`, choose ChatGPT | typed questions and notes |
| Ollama Cloud | an API key (see [Configuration](#configuration)) | yes |
| Ollama (local) | none; uses `127.0.0.1:11434` | yes |
| Custom | presets for OpenRouter, Mistral, Groq, DeepSeek, Together, LM Studio and vLLM, plus your key | yes |

- **Settings, Providers** shows which providers are ready and the command
  to fix the ones that are not.
- **Fallback:** if the chosen provider fails, typed questions try Claude,
  Antigravity, OpenCode, Codex, Ollama Cloud, then Ollama. Live answers skip
  the slower Antigravity and Codex.
- The subscription CLIs run without tools, MCP servers or saved sessions; the
  daemon only asks them for text. Keys live in the encrypted local vault and
  never cross the daemon's socket.
- Using a subscription CLI from another program is subject to that
  service's terms and limits.

### Speech-to-text

| Provider | Where it runs | Notes |
| --- | --- | --- |
| Parakeet (default, `parakeet-local`) | in the daemon, on the CPU | NVIDIA Parakeet TDT 0.6B v3; about 1.1 GB RAM while loaded; 25 European languages, detected automatically |
| Deepgram (`deepgram-stt`) | cloud | a key in Providers, or an agenix file: `deepgram.keyFile = config.age.secrets.api-deepgram.path;` (a key in Providers wins); `deepgram.region = "eu"` uses `api.eu.deepgram.com` and opts out of model improvement |
| OpenAI-compatible (`openai-compatible-stt`) | your server | any server with `/v1/audio/transcriptions` (a whisper server, vLLM, Groq, OpenAI): `settings.speechProvider = "openai-compatible-stt"`, `settings.speechBaseUrl` and `settings.speechModel` (required). whisper.cpp's `whisper-server` works when started with `--inference-path /v1/audio/transcriptions` |

whisper.cpp was removed in favour of Parakeet; the `whisper.*` options now
fail with a message saying so. A saved `whisper-local` choice becomes
`parakeet-local` once, with a notice. For Deepgram or OpenAI-compatible setups
only, set `parakeet.enable = false` to leave the model out of the closure.

### Fully offline

With Parakeet, transcription is local. Choose Ollama for answers and notes and
nothing leaves the machine. Large local models can take a while to load the
first time; the answer card shows "Loading model…" meanwhile.

## Configuration

### Home Manager options

Everything is configured under `programs.nixarchy-meetingai`: the services
(autostart, log level, the Parakeet model), the
Ollama Cloud key and the local Ollama address, the app's settings (as defaults,
or locked with `settings.enforce`), the plugin and call detection, and the
keybindings. The full list is in [Nix options](#nix-options).

Example with an agenix secret and a few settings:

```nix
programs.nixarchy-meetingai = {
  enable = true;
  ollamaCloud.keyFile = "/run/agenix/api-ollama";
  settings = {
    chatProvider = "ollama-cloud";
    chatModel = "gpt-oss:120b";
    speechProvider = "parakeet-local";
  };
};
```

Settings are defaults: applied once, and what you change in the app wins
afterwards (a value you change in Nix is applied once more). With
`settings.enforce = true` they are written at every start and the app shows
them locked with "Set in Nix". Profiles and prompts stay in the app.

### In the app (Settings)

- **Providers and models:** the provider and model for typed questions and
  for live answers, separately.
- **Pace:** how quickly the copilot answers what the other side asks.
- **Smart follow-ups:** suggested next questions, on or off.
- **Speech:** speech-to-text provider and model, microphone and output device.
- **Text size** and **Reduce motion**.
- **Auto-record calls:** start notes by itself when a call is detected
  (never in Interview someone mode).
- **Let AI agents start recording:** off by default.
- **Prompts:** every prompt per mode, editable, with a reset.
- **Profiles:** your role, context and documents, reused across meetings.

### Keybindings

| Action | Key |
| --- | --- |
| Overlay | `SUPER + ALT + M` |
| Start recording | `SUPER + ALT + R` |
| Stop recording | `SUPER + ALT + SHIFT + R` |
| Toggle microphone | `SUPER + ALT + SHIFT + M` |
| Toggle system audio | `SUPER + ALT + S` |
| Region screenshot | `SUPER + ALT + SHIFT + S` |

Inside the overlay: `1`, `2`, `3` send a suggestion; `Escape` closes a
dialog, then a drawer, then the overlay; everything is reachable with `Tab`.

### Calls

The [nixarchy-meeting-mode](https://github.com/olafkfreund/nixarchy-meeting-mode) plugin is the only
detector; the Home Manager option `callDetection.enable` (on by default) installs it. meetingai reads its
state file `$XDG_RUNTIME_DIR/meeting-mode.json` and reacts to the call state, never to whether Meeting
Mode is switched on (switching it on by hand is not a call).

- **Maybe** is a browser holding the microphone that nothing has confirmed yet. The bar shows a dim dot and
  nothing else.
- **Call** is a call app holding the microphone, or a browser call that a meeting window title, a screen
  share or one minute of continuous microphone confirms. The bar shows an outlined dot and a **Take notes?**
  chip with the app's glyph, and a notification with a "Take notes" button. Click the chip to start (it
  names the session after the call window), right-click to dismiss it for this call. The record hotkey and
  `omarchy-shell meetingai offer` do the same. Auto-record calls
  (`settings.autoRecordOnCall`) starts the notes at once instead, except in interviewer mode, where
  consent stays manual and the offer is shown.
- **Call ends** while recording: a "Stop notes" offer. The notes are never stopped by themselves, because a
  call can drop and resume.
- **No detector:** if the state file is still missing 30 seconds after start, Settings says "Call
  detection is off: enable the meeting-mode plugin" with the command. A state file whose writer has
  exited is ignored.

**Calendar (optional, off by default).** With `settings.calendarEnabled` the daemon runs
`gog calendar events` (read-only, with your own `gog` login) for now plus or minus ten minutes, every five
minutes while idle and whenever the call state changes. An event with a Meet, Teams or Zoom link names the
session and is written to `$XDG_RUNTIME_DIR/nixarchy-meetingai/calendar.json` (mode 0600) for meeting-mode to
use as a confirmer. A missing or logged-out `gog` is silently skipped.

**Privacy.** Window and event titles stay in memory, the two runtime files and the session name. They are
never written to logs or notifications (log lines and notifications name only the app).

The old meeting-mode `onStart` and `onEnd` hooks that called `ctl record.start` are no longer needed: remove
them.

## Ask your agents

`programs.nixarchy-meetingai.skills.enable` (default on) installs a `meetingai`
skill for Claude Code, Codex and agy. Ask "show me my latest meeting notes",
"what did we agree about X" or "search my meetings for Y"; the agent reads them
read-only through `nixarchy-meetingai ctl --agent`, which the app enforces:
it refuses every command except reads and never shows local-only interviews. What it reads goes to that agent's
own provider. The skill links into `~/.claude/skills`, `~/.agents/skills` and
`~/.gemini/antigravity/skills`; an existing file at one of those paths makes
activation fail, so move it first or set `skills.enable = false`.

The agent flag is chosen by the caller: any program running as you can reach
the daemon without it. It keeps an agent that follows the skill read-only and
away from local-only interviews; it is not a sandbox for software you do not
trust.

## Nix options

Generated by `nix build .#docs` (a flake check keeps it current).

<!-- nix-options:begin -->
## programs\.nixarchy-meetingai\.enable



Whether to enable the nixarchy-meetingai daemon, its Quickshell plugin and local speech-to-text\.



*Type:*
boolean



*Default:*

```nix
false
```



*Example:*

```nix
true
```



## programs\.nixarchy-meetingai\.package



The nixarchy-meetingai package\.



*Type:*
package



*Default:*

```nix
nixarchy-meetingai.packages.${system}.default
```



## programs\.nixarchy-meetingai\.autostart

Start the services with the graphical session\.



*Type:*
boolean



*Default:*

```nix
true
```



## programs\.nixarchy-meetingai\.callDetection\.enable



Also install the nixarchy-meeting-mode plugin (call detection) through ` programs.nixarchy.plugins `,
unless you already declare a ` nixarchy.meeting-mode ` entry there\. Needs nixarchy; ignored without it\.



*Type:*
boolean



*Default:*

```nix
true
```



## programs\.nixarchy-meetingai\.deepgram\.keyFile



Path to a Deepgram key file, read by the daemon at runtime\. A vault key takes precedence\.



*Type:*
null or string



*Default:*

```nix
null
```



*Example:*

```nix
"/run/agenix/deepgram"
```



## programs\.nixarchy-meetingai\.deepgram\.region



Deepgram API region; EU requests opt out of model improvement\.



*Type:*
null or one of “us”, “eu”



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.echoCancel\.enable



Load PipeWire’s WebRTC echo canceller with the default sink’s monitor as the reference and
expose the result as the ` meetingai-echo-cancel-source ` source, which the daemon records as the
microphone when present\. The system default source is not changed\.
It takes effect after PipeWire restarts or at the next login
(` systemctl --user restart pipewire pipewire-pulse wireplumber `)\.



*Type:*
boolean



*Default:*

```nix
true
```



## programs\.nixarchy-meetingai\.extraEnvironment



Extra environment for the daemon service\. Never put secrets here: it lands in the Nix store\.



*Type:*
attribute set of string



*Default:*

```nix
{ }
```



*Example:*

```nix
{
  HTTPS_PROXY = "http://proxy:3128";
}
```



## programs\.nixarchy-meetingai\.keybindings



Hyprland keys, written to ` ~/.config/hypr/meetingai-binds.lua `\.
Load it from bindings\.lua with ` pcall(require, "hypr.meetingai-binds") `\.
Set an entry to null to drop it\.



*Type:*
attribute set of (null or string)



*Default:*

```nix
{
  mic = "SUPER + ALT + SHIFT + M";
  overlay = "SUPER + ALT + M";
  record = "SUPER + ALT + R";
  screenshot = "SUPER + ALT + SHIFT + S";
  stop = "SUPER + ALT + SHIFT + R";
  system = "SUPER + ALT + S";
}
```



## programs\.nixarchy-meetingai\.logLevel



Daemon log level (` RUST_LOG ` for the daemon’s own crate)\.



*Type:*
one of “error”, “warn”, “info”, “debug”, “trace”



*Default:*

```nix
"info"
```



## programs\.nixarchy-meetingai\.ollama\.url



Address of the Ollama server, ` https://... ` or ` http:// ` on localhost, 127\.0\.0\.1 or \[::1]
(anything else is refused by an assertion, since the app would hide the provider)\. It wins
over a base URL stored in the app\. Only a loopback address counts as local for
local-only interviews\.



*Type:*
string



*Default:*

```nix
"http://127.0.0.1:11434"
```



## programs\.nixarchy-meetingai\.ollamaCloud\.environmentFile



File with ` OLLAMA_API_KEY=... `, read by the daemon service (systemd ` EnvironmentFile `)\.
Give a path outside the Nix store; the key is never copied into the store\.
Without it, add the key in the Library, Providers tab\.



*Type:*
null or string



*Default:*

```nix
null
```



*Example:*

```nix
"/run/secrets/ollama-cloud.env"
```



## programs\.nixarchy-meetingai\.ollamaCloud\.keyFile



File holding only the Ollama Cloud key (for example an agenix secret)\. The daemon reads
this path itself (` NIXARCHY_MEETINGAI_OLLAMA_KEY_FILE `), so the key never enters the Nix
store; a missing file just means no key (logged once at warn)\. It wins over ` environmentFile `
and the key added in the app; do not set both options\.



*Type:*
null or string



*Default:*

```nix
null
```



*Example:*

```nix
"/run/agenix/ollama-cloud"
```



## programs\.nixarchy-meetingai\.parakeet\.enable



Give the daemon the local Parakeet speech model (640 MB in the closure)\. Turn off only for Deepgram or OpenAI-compatible setups; the daemon then reports Parakeet as not ready\.



*Type:*
boolean



*Default:*

```nix
true
```



## programs\.nixarchy-meetingai\.parakeet\.model



The Parakeet speech model for ` speechProvider = "parakeet-local" `: a directory with ` encoder.int8.onnx `, ` decoder.int8.onnx `, ` joiner.int8.onnx ` and ` tokens.txt ` (NVIDIA Parakeet TDT 0\.6B v3, CC-BY-4\.0)\. The daemon gets it as ` NIXARCHY_MEETINGAI_PARAKEET_DIR `\. Once Parakeet has been used the model stays loaded (about 1\.1 GB RAM) while the daemon runs\.



*Type:*
package



*Default:*

```nix
pkgs.callPackage ./nix/parakeet-model.nix { }
```



## programs\.nixarchy-meetingai\.plugin\.enable



Install the Quickshell plugin: through ` programs.nixarchy.plugins ` when nixarchy is imported,
else into ` ~/.config/omarchy/plugins `\. Enabling it in the shell and placing its bar
widget stay a one-time step (` omarchy plugin enable nixarchy.meetingai `)\.



*Type:*
boolean



*Default:*

```nix
true
```



## programs\.nixarchy-meetingai\.settings\.autoRecordOnCall



Start recording when a call begins\.



*Type:*
null or boolean



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.autoResponse



Which utterances get an automatic answer\.



*Type:*
null or one of “off”, “questions”, “pauses”



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.calendarEnabled



Name sessions from the calendar (read-only, through ` gog `; off by default)\.



*Type:*
null or boolean



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.chatModel



Model for chat answers\.



*Type:*
null or string



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.chatProvider



Provider for chat answers\.



*Type:*
null or string



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.enforce



Without this the values above are defaults: applied once, and the app wins afterwards
(a changed Nix value is applied once more)\. With it they are written at every start and
the app shows them locked with “Set in Nix”\.

A default that you remove and later add back with the same value is not applied again
(the app remembers a hash of each value it has applied)\. To force it, change the value
or set this option\.



*Type:*
boolean



*Default:*

```nix
false
```



## programs\.nixarchy-meetingai\.settings\.extra



Settings without a typed option, by key, as strings\. Never a secret (` api_key ` is refused)\.



*Type:*
attribute set of string



*Default:*

```nix
{ }
```



*Example:*

```nix
{
  capture_delay_seconds = "3";
}
```



## programs\.nixarchy-meetingai\.settings\.liveModel



Model for automatic live answers\.



*Type:*
null or string



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.liveProvider



Provider for automatic live answers\.



*Type:*
null or string



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.mcpAllowRecord



Let MCP clients start recording\.



*Type:*
null or boolean



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.meetingMode



Meeting mode\.



*Type:*
null or one of “general”, “interview”, “team_meeting”, “interviewer”



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.reduceMotion



Reduce motion\.



*Type:*
null or boolean



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.responseLanguage



Answer language (` Auto ` follows the question)\.



*Type:*
null or string



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.responseLength



Answer length\.



*Type:*
null or one of “Short”, “Medium”, “Auto”



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.responsePace



How soon automatic answers start\.



*Type:*
null or one of “Fast”, “Balanced”, “Relaxed”



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.smartFollowUps



Suggest follow-up questions\.



*Type:*
null or boolean



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.speechBaseUrl



Base URL for OpenAI-compatible speech; must be set for that provider\.



*Type:*
null or string



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.speechLanguage



Speech language code; empty detects it\.



*Type:*
null or string



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.speechModel



Speech-to-text model\.



*Type:*
null or string



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.speechProvider



Speech-to-text provider (` parakeet-local ` (the default), ` deepgram-stt `, or ` openai-compatible-stt `)\.



*Type:*
null or string



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.settings\.textScale



Overlay text size in percent\.



*Type:*
null or integer between 80 and 160 (both inclusive)



*Default:*

```nix
null
```



## programs\.nixarchy-meetingai\.skills\.enable



Link the ` meetingai ` agent skill into ` ~/.claude/skills `, ` ~/.agents/skills ` (Codex)
and ` ~/.gemini/antigravity/skills ` (agy), so they can read your meetings through ` ctl `\.
Existing files at those paths make activation fail\.



*Type:*
boolean



*Default:*

```nix
true
```



## programs\.nixarchy-meetingai\.speakerModel



The speaker-embedding model (3D-Speaker CAM++, Apache-2\.0, 26 MB) used to tell the owner’s voice from others in the room\. The daemon gets the file path as ` MEETINGAI_SPEAKER_MODEL `; it is only loaded once a voice sample exists\.



*Type:*
package



*Default:*

```nix
pkgs.callPackage ./nix/speaker-model.nix { }
```


<!-- nix-options:end -->

## Use from AI agents

`nixarchy-meetingai mcp` is an MCP server over stdio; the daemon must be
running. Register it once per client:

```bash
claude mcp add --scope user meetingai -- nixarchy-meetingai mcp
codex mcp add meetingai -- nixarchy-meetingai mcp
agy mcp add meetingai nixarchy-meetingai mcp
```

Tools: `meeting_status`, `transcript_get`, `current_answer`, `ask`,
`meetings_list`, `meeting_notes`, `meeting_items`, `series_list`,
`notes_regenerate`, `session_new`, `session_restore`, `record_start`,
`record_stop`, `settings_set`. Resources: `meeting://current/transcript`,
`meeting://{id}/transcript`, `meeting://{id}/notes`.

- `record_start` is refused unless you turn on "Let AI agents start
  recording"; agents cannot change that setting, the API key, or give
  interview consent.
- Local-only interviews are invisible to agents.
- The overlay shows a quiet notice when an agent records or asks something.
- Whatever an agent reads goes to that agent's own provider.

The same commands work from a shell: `nixarchy-meetingai ctl state.get`.

## Privacy and data

- Audio is transcribed locally and not stored; transcripts, notes and keys are
  kept in an encrypted database in your data directory.
- Text goes to a cloud provider only when you choose one; local-only
  interviews never do.
- Delete a meeting from Settings, Restore; delete an interview's data with
  "Delete candidate…".

## Troubleshooting

```bash
systemctl --user status nixarchy-meetingai
journalctl --user -u nixarchy-meetingai -e
nixarchy-meetingai ctl providers.status
```

- **Overlay says the daemon is not running:** start the service with the
  first command above.
- **A provider is "not logged in":** run its sign-in command from the table.
- **No bar widget:** run the `omarchy plugin enable` step again.
- **Keys do nothing:** check the `pcall` line in `bindings.lua`.

## Development

```bash
devenv shell -- cargo test --workspace
devenv shell -- bash shell/tests/smoke.sh
nix build .#default
```

The shell tests run in a private, invisible sway session, so they never open
windows on your desktop; set `MEETINGAI_ON_DESKTOP=1` to watch one.
`tools/provider-probe.py` asks every provider one question through a
throwaway daemon. Design notes live in `intent/`, `spec/` and `plan/`.

### Continuous integration

`.github/workflows/ci.yml` runs on every push and pull request, with three
jobs that need only Nix:

| Job | Runs |
|---|---|
| `rust` | `cargo test --workspace` (with `CI=1`), clippy with `-D warnings`, rustfmt check, in `nix develop` |
| `nix` | `nix flake check -L`: the Home Manager module checks, the package and plugin builds, and the README options match |
| `shell` | `nix build .#checks.x86_64-linux.shell -L`: lint, smoke, smoke-real, keys, every `live-shots.sh` scenario at 100 % and 150 % with the geometry check, and contrast, in a headless sway |

Run the same things locally with `nix develop -c cargo test --workspace`,
`nix develop .#shell -c bash shell/tests/ci.sh` and `nix flake check`.
Requiring these checks on `main` is a follow-up once CI has been green for
two weeks.

## License and history

AGPL-3.0 while original code remains; MIT once the rewrite is complete (see
`LICENSE`).

Started from lennette (AGPL-3.0) by parazeeknova; being rewritten.
