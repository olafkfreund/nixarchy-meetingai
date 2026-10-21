# Lennette

An ambient real-time meeting and discussion copilot.

Lennette is a native Rust/GPUI overlay with live speech transcription, streamed
answers, screen attachments and a local conversation/meeting dashboard.

## Build and install

On NixOS, run `devenv shell -- just install`. The installer embeds the runtime
library paths so app-menu launches also work outside a development shell.
For protected Hyprland screen sharing, follow the
[compositor and Home Manager setup](packaging/hyprland/README.md).

## Connect the services

Open the dashboard, then **Settings → AI & Keys**. Choose a service, click its
API key field, paste the key and save it. Keys are stored in the local encrypted
vault. The supported services are:

- **Deepgram** for speech transcription, using `nova-3` by default.
- **Merge Gateway** for AI answers, fixed to `deepseek/deepseek-v4.1-flash-1017`.
- **Google Gemini** as the other AI service, with an editable Gemini model ID.

Other providers and local CLI assistants are disabled. Older vault entries are
preserved encrypted but cannot be selected or used for requests.

Click Play to start microphone and system audio capture. Recording stays disabled
until a Deepgram key is configured. Explicit spoken questions and common question
phrases trigger an answer automatically; ordinary statements remain transcript
context. Questions that arrive during an answer are coalesced to the latest one.
The selected prompt and recent transcript steer the response. Stop releases the
input, flushes pending speech, and saves the meeting's end time. Speech/provider
failures appear in the overlay.

Type or paste a question to ask manually. The picture button attaches the focused
display to the next question. On Hyprland this uses `grim` on the current
compositor, and respects the private overlay capture rule.

The overlay opens as a corner dock and expands near the top center on hover.
The lower controls switch between the full composer, follow-ups only, and hidden.
**Ctrl+Shift+\\** shows/hides the overlay; **Ctrl+Shift+I** reveals and focuses
its input. The Home Manager module installs the other compositor shortcuts.

## Validation

Run `devenv shell -- xvfb-run -a just pre-push` for formatting, lint, type checks,
unit tests and local HTTP protocol flow tests. The provider flow tests cover WAV
upload, streaming text and images, authentication failures, empty/error streams,
cancellation, and persisted conversations/finished meetings for both AI services.
They use local endpoints and require no credentials.

For an explicitly requested live check, create a private JSON file outside the
repository with `deepgram` and `merge` key fields, then run:

```sh
devenv shell -- cargo run --example provider_verify -- /private/path/keys.json /path/to/question.wav
```

This saves those credentials in your local vault and sends the WAV to Deepgram,
then requests a DeepSeek answer through Merge Gateway. It prints the test
transcript/answer, never the keys, and saves test records under `target/`.

Provider protocols follow the official [Merge Gateway](https://docs.merge.dev/merge-gateway/api-overview/chat-completions/create),
[Gemini](https://ai.google.dev/gemini-api/docs/openai), and
[Deepgram](https://developers.deepgram.com/reference/speech-to-text/listen-pre-recorded) documentation.

Markdown answers stream directly into native text. Fenced and inline code use
Fira Code Nerd Font, with Syntect and bat's bundled grammars for syntax colors.
Unknown languages remain readable as plain text. The renderer handles headings,
links, nested lists, tables, quotes, emphasis, and incomplete fenced blocks while
an answer arrives. Screenshots use small removable previews; the full image is
sent to the model.

The chat list draws visible blocks with scroll position preserved as the active
answer grows. Completed documents and syntax states are reused, streamed parse
prefixes are replaced, thumbnail textures are released, and idle controls do not
request continuous animation frames. The live transcript view retains 500 recent
lines; the complete transcript remains in the meeting database.

Run `devenv shell -- cargo run --release --example markdown_bench` to measure local
Markdown/highlighting preparation independently of network/model latency. The
approach follows [Syntect's incremental state API](https://docs.rs/syntect/5.3.0/syntect/easy/struct.HighlightLines.html),
[bat's bundled syntax definitions](https://docs.rs/two-face/0.5.2+bat-0.26.1/two_face/),
and [pulldown-cmark's CommonMark parser](https://github.com/pulldown-cmark/pulldown-cmark).
