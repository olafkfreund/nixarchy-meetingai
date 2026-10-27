# nixarchy-meetingai

An ambient meeting copilot for [Omarchy](https://omarchy.org) on NixOS
(Nixarchy). A headless Rust daemon records and transcribes a call, answers
questions about it, and writes rolling notes. The UI is a Quickshell
(omarchy-shell) plugin, `nixarchy.meetingai`.

Derived from [lennette](https://github.com/parazeeknova/lennette) by
parazeeknova. Licensed under AGPL-3.0, see `LICENSE`.

## Install

Add the flake and the Home Manager module:

```nix
{
  inputs.nixarchy-meetingai.url = "github:olafkfreund/mixarchy-meetingai";

  # in your Home Manager configuration
  imports = [ inputs.nixarchy-meetingai.homeManagerModules.default ];
  programs.nixarchy-meetingai.enable = true;
}
```

This installs the `nixarchy-meetingai` binary, two user services
(`nixarchy-meetingai` and `nixarchy-meetingai-whisper`), the Quickshell plugin
and a Hyprland key fragment.

Options:

| Option | Default |
| --- | --- |
| `programs.nixarchy-meetingai.package` | this flake's package |
| `programs.nixarchy-meetingai.whisper.model` | `whisper-model-small-en` |
| `programs.nixarchy-meetingai.whisper.package` | `pkgs.whisper-cpp-vulkan` |
| `programs.nixarchy-meetingai.whisper.port` | `8178` |
| `programs.nixarchy-meetingai.keybindings` | see below |

## Providers

| Provider | Sign in |
| --- | --- |
| Claude (subscription) | run `claude` once and log in; the daemon spawns the CLI |
| ChatGPT / Codex (subscription) | run `codex login` and choose ChatGPT; the daemon spawns the CLI |
| Antigravity (subscription) | run `agy` once and sign in with your Google account; the daemon spawns the CLI |
| Ollama | none; uses `http://127.0.0.1:11434` |

API keys live in the encrypted local vault and never cross the daemon socket.

## Offline mode

Speech-to-text is `whisper-server` on `127.0.0.1:8178` with the
`ggml-small.en` model, built into the Nix store, so transcription needs no
network. Pair it with an Ollama model for answers and notes with no network
at all. Deepgram stays available as a cloud alternative.

## Keybindings

The module writes `~/.config/hypr/meetingai-binds.lua`. Load it once from your
`bindings.lua`:

```lua
pcall(require, "hypr.meetingai-binds")
```

| Action | Key |
| --- | --- |
| Overlay | `SUPER + ALT + M` |
| Start recording | `SUPER + ALT + R` |
| Stop recording | `SUPER + ALT + SHIFT + R` |
| Toggle microphone | `SUPER + ALT + SHIFT + M` |
| Toggle system audio | `SUPER + ALT + S` |
| Region screenshot | `SUPER + ALT + SHIFT + S` |

Override with `programs.nixarchy-meetingai.keybindings.<name> = "…"`, or set
one to `null` to drop it. The same actions are available from a shell:
`nixarchy-meetingai ctl state.get`.

## Team meetings

Pick "Team" as the meeting mode to track work across meetings. Each team
meeting belongs to a **series**; choose one (or create one) before you record.

- **Items.** While you talk, the copilot pulls out actions, decisions and
  questions. They start as drafts and are settled when you stop. Edit, mark
  done or delete them from the overlay.
- **Last time.** The previous meeting in the series, its recap and the items
  still open, are one step away before you start.
- **Earlier task?** When the talk returns to an open item from an earlier
  meeting, a callout offers Discussed, Done or Not related (at most one every
  three minutes).
- **Search.** The final recap is searchable with the meeting titles.

Agents read items with `meeting_items` and `series_list` (below).

## Interviews you run

Pick "Interviewer" as the meeting mode when you are the one asking the
questions. The copilot helps you ask; it never judges the candidate.

- **Before the call.** Create a candidate, attach their CV and the job
  description, and generate a question bank of 8 to 12 questions. Each one
  quotes the CV or job text it came from; questions whose quote is not in the
  documents are dropped. Edit, reorder, add and delete freely.
- **During the call.** A question you say aloud is marked asked; you can tick
  any question as covered or skipped and keep an evidence note per question.
  After each candidate answer you get one to three follow-up probes that quote
  what they said.
- **After the call.** The recap lists each question, what the candidate said
  (quotes) and gaps to verify. It is evidence only.
- **Limits.** No scores, ratings, rankings, hire or no-hire advice, and no
  guesses about emotion, personality, confidence or honesty. The prompts
  forbid them and a filter rejects any answer or recap that contains one,
  retries once, then withholds it. The filter is strict, so a harmless word
  such as "hire date" can cost a retry.
- **Consent.** Tell the candidate the interview is recorded and transcribed.
  The first recording of each interview asks you to confirm that. Agents
  cannot start an interview recording.
- **Local-only.** Turn it on per interview (recommended). Speech-to-text must
  then be whisper-local and every model call goes to ollama; anything else is
  refused, and agents cannot read that interview's transcript or notes.
- **Deletion.** Candidate data is kept until you delete it. "Delete candidate
  data" removes the candidate, documents, questions, transcript, notes and
  chat of that interview. Candidate data is never added to your profiles or to
  other meetings.

You are responsible for using this lawfully where you are: telling candidates,
keeping the data only as long as you need it, and making hiring decisions
yourself.

## Use from AI agents

`nixarchy-meetingai mcp` is an MCP server over stdio. It talks to the running
daemon, so `nixarchy-meetingai.service` must be up. Register it once per client
(nothing in the module edits their config files):

```sh
claude mcp add --scope user meetingai -- nixarchy-meetingai mcp
codex mcp add meetingai -- nixarchy-meetingai mcp
agy mcp add meetingai nixarchy-meetingai mcp
```

Remove it with `claude mcp remove meetingai` (and the equivalents for Codex
and agy).

Tools:

- `meeting_status`: state plus the current question and answer
- `transcript_get`: transcript lines of a meeting (`meeting`, `after_line`, `limit` up to 500)
- `current_answer`: the copilot's current answer
- `ask`: ask the copilot a question (up to 8k characters) and wait for the answer
- `meetings_list`: list past meetings (`query`, `limit`)
- `meeting_notes`: the notes of a meeting
- `meeting_items`: the actions, decisions and questions of a `meeting` or `series` (`status`)
- `series_list`: list the team-meeting series
- `notes_regenerate`: regenerate the notes of a meeting
- `session_new`: start a new session
- `session_restore`: restore a past meeting
- `record_start`: start recording (needs the consent setting below)
- `record_stop`: stop recording
- `settings_set`: change `meeting_mode`, `response_pace`, `chat_provider`,
  `chat_model`, `live_provider`, `live_model` or `text_scale`; `api_key` and
  `mcp_allow_record` are always refused

Resources:

- `meeting://current/transcript`: the last 200 lines, as markdown
- `meeting://{id}/transcript`
- `meeting://{id}/notes`

`resources/list` returns the current meeting and the 20 most recent.

**Consent.** `record_start` is refused unless you turn on "Let AI agents start
recording" in Settings (`mcp_allow_record`, off by default). Agents cannot
change this setting.

**Privacy.** Anything an agent reads, including transcripts and notes, goes to
that agent's provider. The overlay shows a quiet notice when an agent starts
or stops recording or asks the copilot a question. The daemon logs only the
command name of agent calls, never the arguments.

## Development

```sh
devenv shell -- cargo test --workspace
nix build .#default
```
