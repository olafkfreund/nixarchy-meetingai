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
    url = "github:olafkfreund/mixarchy-meetingai/refactor/omarchy-quickshell-rewrite";
    inputs.nixpkgs.follows = "nixpkgs";
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
whisper model (large-v3-turbo, about 1.6 GB, downloaded once at build time)
and a keybinding fragment.

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

| Option | Default | What it does |
| --- | --- | --- |
| `programs.nixarchy-meetingai.enable` | `false` | install and run everything |
| `programs.nixarchy-meetingai.package` | this flake's package | the daemon |
| `programs.nixarchy-meetingai.whisper.model` | large-v3-turbo | whisper.cpp model package |
| `programs.nixarchy-meetingai.whisper.package` | `pkgs.whisper-cpp-vulkan` | whisper.cpp build |
| `programs.nixarchy-meetingai.ollamaCloud.keyFile` | `null` | a file holding the bare Ollama Cloud key (for example an agenix secret); passed to the daemon as a systemd credential, never copied into the Nix store |
| `programs.nixarchy-meetingai.ollamaCloud.environmentFile` | `null` | alternatively, a file with `OLLAMA_API_KEY=...` |
| `programs.nixarchy-meetingai.keybindings` | see below | Hyprland keys; set one to `null` to drop it |

Example with an agenix secret:

```nix
programs.nixarchy-meetingai = {
  enable = true;
  ollamaCloud.keyFile = "/run/agenix/api-ollama";
};
```

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

## License and history

AGPL-3.0 while original code remains; MIT once the rewrite is complete (see
`LICENSE`).

Started from lennette (AGPL-3.0) by parazeeknova; being rewritten.
