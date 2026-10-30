---
status: approved
issue: 44
intent: intent/2026-10-30-44-meeting-export.md
---

# Spec: export each meeting to a folder as Markdown, with screenshots and documents

The intent was approved with its proposed answers to the open questions:
local-only interviews are exported and marked `local_only: true`; links are
standard Markdown; the app owns only what it writes; originals have a size
cap. The existing import already refuses files over 10 MB
(`MAX_FILE_BYTES`, `src/core/context.rs`), so that is the cap and no second
limit is added.

## Design

### 1. What is saved from now on (encrypted, in the database)

- **Screenshots.** New table `meeting_screenshots` (`id INTEGER PRIMARY
  KEY`, `meeting_id` → `meetings(id) ON DELETE CASCADE`, `taken_at` (seconds
  into the meeting, like line times), `question TEXT`, `mime TEXT`,
  `data BLOB`). In `chat_send` (`src/daemon/session.rs`, where
  `pending_capture` is taken before `begin_send`) the image is also stored
  against the meeting that chat turn is about, the same meeting the turn
  uses for context (`meeting_id`, else `finishing_meeting`, else
  `resume_meeting`). A screenshot with no meeting is not stored. A failed
  store is logged and never blocks the question.
- **Original files for interview documents.** `candidate_documents` gains
  `original BLOB` and `mime TEXT NOT NULL DEFAULT ''` through a guarded
  `ALTER TABLE` in `src/db/mod.rs`, the existing pattern. `context::import`
  already reads the whole file (≤ 10 MB); it returns the bytes and mime
  alongside the text, and `candidate_attach` stores them. Rows attached
  before this change have no original and export their text as `.md`.
- Deleting a meeting or candidate deletes these rows (cascade), as today.

### 2. The export folder setting

- New settings `export_folder` (string, default empty) and
  `export_meetings` (bool, default true) in `src/core/models/settings.rs`
  (`pairs`, defaults, parsing), so `settings.set` and the Nix settings
  accept them. `nix/hm-module.nix` gets `exportFolder` (nullable string),
  mapped like the other typed options. When Nix fixes it, the dialog never
  shows and the Settings field is read-only, as for other locked settings.
- Agents and MCP clients already cannot call `settings.set` or anything
  else outside their read-only allow-list (`session.rs`, the `origin ==
  Some("agent")` check). The new `meetings.export` command is not added to
  that list.

### 3. The exporter: `src/core/export.rs` (new)

A pure part and a writing part, so the format is unit-tested without a
daemon.

- `render(meeting, recap, items, lines, speakers, screenshots, documents,
  candidate) -> String` builds `notes.md`:
  - YAML frontmatter: `title`, `date`, `start`, `end`, `duration`, `mode`
    (general/team/interviewer), `candidate` (interviews), `local_only`
    (interviews), `meeting_id`, `exported`.
  - `# <title>`, then `## Summary` (the recap; if no recap is ready, the
    rolling notes that `notes.get` would show), `## Action items`,
    `## Decisions`, `## Open questions` (from `meeting_items`, owner and
    due where set), `## Screenshots` (each as
    `![<time>](attachments/shot-NN.png)` with its time and question),
    `## Documents` (links to `documents/<name>`), and `## Transcript`:
    every line with `[hh:mm:ss] <speaker name or channel>: text`, using the
    names in `meeting_speakers`.
  - Empty sections are left out.
- `write(root, meeting, files) -> PathBuf` writes the meeting:
  - Folder name `<YYYY-MM-DD> <title>`, from the meeting's start date and
    title, made safe as a file name: path separators, control characters
    and `<>:"\|?*` replaced, leading dots and trailing dots and spaces
    trimmed, Windows reserved names (`CON`, `NUL`, …) suffixed, length cut
    to 120 bytes on a char boundary. An empty title becomes `Meeting`.
    Document names get the same treatment.
  - New table `meeting_exports(meeting_id PRIMARY KEY → meetings ON
    DELETE CASCADE, dir TEXT)` remembers where each meeting went. On
    re-export, if the stored folder still exists under a different name
    (the meeting was renamed), it is renamed to the new name, so the
    user's own files in it move with it. If the new name is taken by
    another folder, ` (2)`, ` (3)` … is appended.
  - The app owns `notes.md`, `attachments/` and `documents/` only.
    `notes.md` is written to a temporary file in the same folder and
    renamed over the old one. `attachments/` and `documents/` are
    rebuilt the same way (written into a temporary sibling, then swapped
    in). Nothing else in the folder is touched.
  - The root folder is created if missing. New folders are `0700`, new
    files `0600`. Every path written is checked to be inside the meeting
    folder before writing.
- Never read or written by the exporter: the voice tables, the provider
  vault, settings, audio.

### 4. When it runs

- **Automatically:** at the end of `finish_meeting_artifacts`
  (`src/daemon/session.rs:2301`), in its `finalize` step, so it runs on
  every path: with a model, without one, and on `notes.regenerate`, which
  calls the same function. It runs off the async thread
  (`spawn_blocking`), after the recap and items are final.
  - If `export_meetings` is off, nothing happens.
  - If `export_folder` is empty, the daemon emits
    `export.folder_needed {meeting}` and stops; it is not an error.
  - Errors are shown once as "Could not export the meeting: …" and
    never undo the recap.
  - Success emits `export.done {meeting, path}`.
- **On demand:** new command `meetings.export {meeting}` does the same
  and replies `{path}`. With no folder set it replies with an error
  telling the shell to ask for one.

### 5. The overlay (`shell/`)

- **First-time dialog:** on `export.folder_needed`, the overlay shows a
  small card: "Save meeting notes to a folder?" with **Choose folder…**
  (a `FolderDialog` from `QtQuick.Dialogs`, already imported in
  `Library.qml`) and **Not now**. Choosing sets `export_folder` and
  exports that meeting. **Not now** sets `export_meetings` false; it can
  be turned on again in Settings. The card is not shown again in the
  same session after either answer.
- **Settings tab:** an "Export meetings" switch (`export_meetings`) and an
  "Export folder" row showing the path with **Change…** (the same
  `FolderDialog`). Read-only when Nix fixes it.
- **Notes view (Library):** an **Export now** button for the selected
  meeting, calling `meetings.export`; it shows "Exported to <path>" or
  the error. Without a folder it opens the folder dialog first.
- `shell/tests/fake-daemon.py` answers `meetings.export` and can emit
  `export.folder_needed`.

### 6. For the owner on p620

`~/.config/nixos` sets `exportFolder = "/home/olafkfreund/Documents/
Synechron/Meetings"` through the module (a separate nixos PR after this
ships), so the dialog never shows there. A `Meetings/` subfolder keeps
the exports out of the vault's root; the owner can say otherwise at plan
approval.

## Alternatives rejected

- **Obsidian `![[…]]` embeds:** only Obsidian shows them. Standard links
  work in Obsidian and every other viewer.
- **Saving screenshots and originals as files on disk instead of in the
  database:** plaintext at rest before the user ever chose to export, and
  outside the encrypted store that owns meeting data.
- **Storing the original's file path instead of its bytes:** the file can
  move or change; the export would silently lose it or show a different
  version.
- **Exporting profile documents:** a profile is not tied to a meeting; the
  database does not record which profile a meeting used, so the export
  cannot say which documents belonged to it. Interview documents are tied
  to the meeting through the candidate. Profile documents can be a
  follow-up.
- **A separate export script or an MCP tool:** agents cannot read
  local-only interviews; an outside script would need the database key.
  The daemon already holds the data and the key.
- **Skipping local-only interviews:** the intent decided to export them,
  marked in the frontmatter.
- **Manual export only:** the owner asked for it to happen after every
  meeting.

## Risks

- **Plaintext on disk:** the export is the one place meeting data leaves
  the encrypted database. If the chosen folder syncs (the Synechron vault
  may), transcripts and CVs sync with it. The dialog says the notes are
  saved as plain files; the folder is the user's choice.
- **Database size:** screenshots (typically 0.3–2 MB each) and originals
  (≤ 10 MB) grow the encrypted DB. Deleting a meeting removes them.
- **Rename races:** a meeting renamed while an export runs: exports for
  one meeting run one at a time (under the existing notes lock), so the
  next export renames the folder.
- **Folder gone or unwritable** (an unmounted drive): the export fails
  with a message; the meeting itself is unaffected.
- **Merge conflicts** with the open #31 and #32 branches in `session.rs`
  and `Library.qml`; whichever lands second merges main in.
- p620 only for the live check; other hosts are unchanged until a folder
  is chosen.

## Verification

- Unit tests (`src/core/export.rs`): the rendered `notes.md` for a fixture
  meeting (frontmatter, sections, speaker names, screenshot links, empty
  sections left out); file-name sanitising (separators, `..`, reserved
  names, long names, empty title); writes stay inside the folder;
  re-export after a rename moves the folder and keeps a user's extra
  file; name collision adds ` (2)`; modes are `0700`/`0600`.
- Daemon tests (`session.rs`): a screenshot sent during a meeting is
  stored with it; `candidate.attach` keeps the original; finishing a
  meeting with a folder set writes the folder; with no folder it emits
  `export.folder_needed` and writes nothing; `export_meetings = false`
  writes nothing; `meetings.export` is refused for agents; the export
  never contains voice data (a fixture with an enrolled voice; grep the
  output).
- DB test: the guarded `ALTER` runs on an old database (the
  `tests/fixtures/old-vault` style).
- Shell tests (headless): the first-time card, the Settings row and
  **Export now**, against the fake daemon.
- Full check: `cargo fmt --check`, `clippy -D warnings`, `cargo test`,
  `bash shell/tests/ci.sh`, and the Nix build.
- Live on p620 after deploy (from committed main): one short meeting with
  a screenshot and one test interview with a PDF CV; open the exported
  folders in Obsidian; check modes with `stat`; then delete the test
  meetings and their export folders.
