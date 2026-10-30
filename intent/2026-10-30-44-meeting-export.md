---
status: approved
issue: 44
author: olafkfreund
---

# Intent: export each meeting to a folder as Markdown, with screenshots and documents

## Problem

Everything a meeting produces stays inside the encrypted database and is
visible only in the overlay. The owner keeps their working notes in an
Obsidian vault and wants every meeting there too, without copying by hand.
Today:

- There is no export at all: no command, no setting, no folder.
- Screenshots sent during a meeting are never saved. The image goes to the
  chat model and is dropped; only the question text is stored. A meeting's
  screenshots cannot be recovered afterwards.
- Interview documents (CV, job description) and profile documents are kept
  as extracted text only, truncated at 16,000 characters. The original
  file (the CV as a PDF) is not kept anywhere.

## Proposed outcome

- When a meeting finishes, and again whenever its notes are regenerated,
  the meeting is exported to `<export folder>/<YYYY-MM-DD> <meeting name>/`:
  - `notes.md`: the meeting summary, action items, decisions, open
    questions, and the full transcript with speaker names;
  - the meeting's screenshots as image files, shown inline in `notes.md`;
  - for an interview, the candidate's CV and other attached files as the
    original files (for files attached from now on), plus the job
    description.
- Re-exporting a meeting replaces that meeting's folder, even if the
  meeting was renamed in between; it never leaves a duplicate folder.
- Interviews are exported automatically, like every other meeting.
- The export folder is chosen in a dialog the first time it is needed,
  and can be changed later in Settings. For the owner it is
  `~/Documents/Synechron` (an Obsidian vault); a Nix option can fix it.
- From now on, screenshots and original attached files are saved with the
  meeting, encrypted in the database like everything else, so they can be
  exported (and re-exported) later.
- An "Export now" action in the meeting's notes view exports on demand.
  This is the only way to export older meetings; their screenshots and
  original files are not recoverable.

## Affected users and systems

- This repo: `src/daemon/session.rs` (meeting-end hook, screenshot
  capture, document attach, new commands), `src/db/schema/schema.sql`
  and `src/db/repos/*` (storing screenshots and original files),
  `src/core/models/settings.rs` and `src/daemon/nix_settings.rs` (the
  export-folder setting), `nix/hm-module.nix` (a Nix option),
  `shell/` (folder dialog, Settings field, "Export now"),
  `shell/tests/fake-daemon.py`, README.
- p620, and every host with the module. Nothing is written to disk until
  a folder is chosen.
- Open #31 and #32 branches touch `session.rs` and `Library.qml`; this
  work merges after or alongside them.

## Constraints

- **The export is plain text on disk, outside the encrypted database.**
  Files and folders are created readable by the owner only (0600/0700).
  The user chooses the folder; the app never picks a synced location
  by itself.
- Never export voice data: the enrolled voice sample, voice scores, or
  any audio. The transcript text is exported (the owner asked for it).
- Never export API keys, settings, or anything from the provider vault.
- Meeting names and document names are made safe as file names (no path
  separators, no `..`, no names reserved on other systems); an export
  can never write outside its meeting folder.
- Notes the owner edits by hand inside the exported folder may be
  overwritten by the next export of that meeting (see open question 3).
- Agents and MCP clients cannot change the export folder (it decides
  where plaintext lands), like the provider and voice settings.
- Shell tests stay headless.

## Open questions

1. **Local-only interviews.** A candidate marked local-only was promised
   their data never leaves this machine. An export folder is on this
   machine, but a vault may sync to the cloud. Proposed: export them
   like other interviews, and put `local_only: true` in the frontmatter.
   The alternative is to skip them unless "Export now" is used.
2. **Obsidian or plain Markdown links.** Proposed: standard Markdown
   links (`![](attachments/shot-01.png)`), which Obsidian and every other
   Markdown viewer show. The alternative is Obsidian-only `![[…]]` embeds.
3. **Hand edits.** Proposed: the app owns `notes.md` and rewrites it; it
   never touches any other file the user adds to the folder. The
   alternative is to stop updating a `notes.md` the user has edited.
4. **Size cap for stored originals.** Proposed: refuse to store an
   attached original above 25 MB (the text is still extracted, as today).
