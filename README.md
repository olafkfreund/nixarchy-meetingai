# nixarchy-meetingai

A meeting copilot for [Omarchy](https://omarchy.org) on NixOS (Nixarchy). It
listens to your calls, writes a live transcript, answers questions while you
talk, suggests what to ask next, and turns each meeting into notes you can
search later. It runs on your own machine: transcription is local
(whisper.cpp), and answers come from the AI subscriptions you already have,
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
- A GPU helps: the speech-to-text build uses Vulkan.

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

Rebuild. This installs the binary, two user services
(`nixarchy-meetingai` and `nixarchy-meetingai-whisper`), the shell plugin, the
whisper model (large-v3-turbo, about 1.6 GB, downloaded once at build time),
the call-detection plugin and a keybinding fragment.

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
  whisper-local and every model call goes to a local Ollama on this machine;
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

### Fully offline

Transcription is always local. Choose Ollama for answers and notes and
nothing leaves the machine. Large local models can take a while to load the
first time; the answer card shows "Loading model…" meanwhile.

## Configuration

### Home Manager options

Everything is configured under `programs.nixarchy-meetingai`: the services
(autostart, log level, the whisper server's model, address and flags), the
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
    speechProvider = "whisper-local";
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

With the [nixarchy-meeting-mode](https://github.com/olafkfreund/nixarchy-meeting-mode)
plugin installed, a detected call offers "Take notes", or starts them by
itself when Auto-record calls is on.

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



Speech-to-text provider (` whisper-local ` or ` deepgram-stt `)\.



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



## programs\.nixarchy-meetingai\.whisper\.enable



Run the local whisper\.cpp server\. Turn off when using Deepgram only\.



*Type:*
boolean



*Default:*

```nix
true
```



## programs\.nixarchy-meetingai\.whisper\.package



whisper\.cpp build providing ` whisper-server `\.



*Type:*
package



*Default:*

```nix
pkgs.whisper-cpp-vulkan
```



## programs\.nixarchy-meetingai\.whisper\.extraArgs



Extra ` whisper-server ` flags\.



*Type:*
list of string



*Default:*

```nix
[ ]
```



## programs\.nixarchy-meetingai\.whisper\.host



Listen address of the whisper server (keep it on loopback)\. The daemon is told 127\.0\.0\.1
when this is ` 0.0.0.0 ` and ` ::1 ` when it is ` :: `; an IPv6 address is bracketed in its URL\.



*Type:*
string



*Default:*

```nix
"127.0.0.1"
```



## programs\.nixarchy-meetingai\.whisper\.model



A whisper\.cpp model package (one with ` passthru.modelName `)\.



*Type:*
package



*Default:*

```nix
(pkgs.callPackage ./whisper-model.nix { })."large-v3-turbo"
```



## programs\.nixarchy-meetingai\.whisper\.port



Port of the whisper server; the daemon is told the same address\.



*Type:*
16 bit unsigned integer; between 0 and 65535 (both inclusive)



*Default:*

```nix
8178
```



## programs\.nixarchy-meetingai\.whisper\.threads



Threads for the whisper server (` -t `); null leaves the default\.



*Type:*
null or (positive integer, meaning >0)



*Default:*

```nix
null
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
systemctl --user status nixarchy-meetingai nixarchy-meetingai-whisper
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
