---
status: approved
issue: 44
spec: spec/2026-10-30-44-meeting-export.md
coders:
  1: codex   # schema and repo functions: mechanical, follows the guarded-ALTER pattern
  2: codex   # small edits at three known call sites
  3: coder   # the exporter: path safety and file modes need a coder that can run the tests
  4: codex   # settings, Nix option and daemon wiring follow existing patterns
  5: agy     # QML: a card, a Settings row and a button, against the fake daemon
reviewers:
  - codex    # codex review --base main, read-only
  - agy      # --mode plan; a headless file-read denial counts as "not obtained"
---

# Plan: export each meeting to a folder as Markdown, with screenshots and documents

## Approved decisions (from the spec)

- From now on, screenshots and the original bytes of interview documents
  are saved in the encrypted database; older meetings have neither.
- Screenshot table `meeting_screenshots(id INTEGER PRIMARY KEY, meeting_id
  → meetings ON DELETE CASCADE, taken_at REAL, question TEXT, mime TEXT,
  data BLOB)`. Stored in `chat_send` against the meeting the turn is about
  (`meeting_id`, else `finishing_meeting`, else `resume_meeting`); no
  meeting means not stored; a failed store is logged, never blocks.
- `candidate_documents` gains `original BLOB` and `mime TEXT NOT NULL
  DEFAULT ''` (guarded `ALTER`). Rows without an original export their
  text as `<name>.md`. Size cap: the existing 10 MB `MAX_FILE_BYTES`.
- `meeting_exports(meeting_id PRIMARY KEY → meetings ON DELETE CASCADE,
  dir TEXT)` remembers each meeting's folder.
- Settings `export_folder` (string, default empty) and `export_meetings`
  (bool, default true); Nix option `exportFolder`. Nix-fixed means the
  dialog never shows and the Settings row is read-only.
- Folder `<YYYY-MM-DD> <title>` (start date, sanitised title, ≤ 120 bytes,
  empty title → `Meeting`); collision → ` (2)`, ` (3)` …; a renamed
  meeting's folder is renamed on re-export, user files move with it.
- `notes.md`: frontmatter (`title, date, start, end, duration, mode,
  candidate, local_only, meeting_id, exported`), then Summary (recap, else
  rolling notes), Action items, Decisions, Open questions, Screenshots
  (`![<time>](attachments/shot-NN.png)` + question), Documents (links into
  `documents/`), Transcript (`[hh:mm:ss] <name>: text`). Empty sections
  left out. Standard Markdown links, never `![[…]]`.
- The app owns `notes.md`, `attachments/`, `documents/` only; each written
  to a temporary sibling and renamed into place. Folders `0700`, files
  `0600`; every write checked to be inside the meeting folder.
- Never exported: voice tables, the provider vault, settings, audio.
- Runs in `finish_meeting_artifacts`'s `finalize` (every path, including
  `notes.regenerate`), off the async thread, under the existing notes
  lock. `export_meetings` off → nothing. Empty folder → emit
  `export.folder_needed {meeting}`, no error. Failure → one error
  "Could not export the meeting: …". Success → `export.done {meeting,
  path}`. On demand: `meetings.export {meeting}` → `{path}`; not on the
  agent allow-list.
- Overlay: first-time card ("Save meeting notes to a folder?", **Choose
  folder…** with `FolderDialog`, **Not now** → `export_meetings` false;
  shown once per session); Settings: "Export meetings" switch and "Export
  folder" row with **Change…**; Library notes view: **Export now**.
- Owner: `exportFolder = "/home/olafkfreund/Documents/Synechron/Meetings"`
  in a separate nixos PR after this ships.
- Profile documents are not exported (a meeting does not record its
  profile).

## Steps

Each step runs in its own worktree `w44-sN` off `feat/44-export` and is
merged back `--no-ff`; the lead runs the full check after each merge
(Codex and agy sandboxes cannot run devenv or cargo).

**Check** (every step):
`chmod -R u+w target; TMPDIR=/tmp devenv shell -- bash -c 'cargo fmt --check; cargo clippy --workspace --all-targets -q -- -D warnings; cargo test --workspace -q'`;
shell steps also `TMPDIR=/tmp devenv shell -- bash shell/tests/ci.sh`
(the final contrast step needs `OMARCHY_SHELL_TREE` until #31 lands; every
step before it must pass).

1. **Storage** (codex). `src/db/schema/schema.sql`: add
   `meeting_screenshots` and `meeting_exports` (with an index on
   `meeting_screenshots(meeting_id)`). `src/db/mod.rs` (~l. 60–90, the
   guarded `ALTER`s): add `original` and `mime` to `candidate_documents`
   the same guarded way. New `src/db/repos/screenshots.rs` (`add`, `list`
   for a meeting, ordered by `taken_at`) and `src/db/repos/exports.rs`
   (`get`, `set`); register in `src/db/repos/mod.rs`.
   `src/db/repos/candidates.rs:135` `add_document` takes
   `original: Option<&[u8]>, mime: &str`; `documents` (l. 155) returns
   them (a `Document` field each). Tests: round trips; cascade on meeting
   delete; the `ALTER` on a database created without the columns.
   → check. Traps: `SCHEMA` runs on every open, so only `CREATE … IF NOT
   EXISTS` there and column changes only as guarded `ALTER`s;
   never squash; `tests/fixtures/old-vault` must still open.

2. **Save screenshots and originals** (codex, after 1).
   `src/core/context.rs:16` `Attachment` gains `bytes: Vec<u8>` and
   `mime: String` (filled in `import`, l. 116, from what it already
   reads; empty for in-memory sources). `src/daemon/session.rs:1278`
   (`chat_send`, where `pending_capture` is taken): if an image was
   taken and a meeting applies, decode the base64 and
   `screenshots::add(meeting, taken_at = seconds since the meeting
   started (0 for a finished meeting), question, mime, bytes)`; log on
   error. `candidate_attach` (`session.rs`, after the `"candidate.attach"`
   arm, l. 3909) passes `Some(&item.bytes), &item.mime`. Tests: a
   screenshot sent while recording is stored with that meeting and its
   question; one sent with no meeting is not stored; an attached PDF's
   original round-trips byte for byte. → check. Traps: never log image
   bytes or document text; the store happens outside any `await` while
   holding the lock (`parking_lot` guard).

3. **The exporter** (coder, parallel with 1–2; uses only plain structs,
   not the repos). New `src/core/export.rs`, `pub mod export;` in
   `src/core/mod.rs`. Input structs mirror the repo records it needs
   (meeting, recap text, items, lines with speaker names, screenshots,
   documents, candidate, `local_only`). `pub fn render(..) -> String`,
   `pub fn safe_name(&str) -> String`,
   `pub fn write(root: &Path, previous: Option<&Path>, export: &Export)
   -> io::Result<PathBuf>` doing the folder naming, rename on
   re-export, collision suffix, temp-then-rename for `notes.md`,
   `attachments/`, `documents/`, modes `0700`/`0600`, and the
   inside-folder check. Unit tests listed in the spec's Verification
   (render fixture, sanitising cases, traversal refused, rename keeps a
   user file, ` (2)` collision, modes, no `![[`). → check.
   Traps: `std::os::unix::fs::PermissionsExt` for modes, and set them on
   create, not after; `rename` across the same filesystem only (temp
   siblings guarantee it); cut names on a char boundary.

4. **Settings, Nix and daemon wiring** (codex, after 1–3).
   `src/core/models/settings.rs`: `export_folder`, `export_meetings` in
   the struct, defaults (~l. 245), parsing (~l. 342) and `pairs`
   (l. 408, the array length goes to 38). `nix/hm-module.nix`:
   `exportFolder = nullable str "…"` (~l. 244) mapped to
   `export_folder` (~l. 55). `src/daemon/protocol.rs`: reuse
   `MeetingArg` (l. 152). `src/daemon/session.rs`:
   a `fn export_meeting(&self, meeting) -> Result<PathBuf, String>` that
   reads everything under the lock, then runs `export::write` in
   `spawn_blocking`, stores the folder with `exports::set`; call it from
   `finalize` (l. 2317) honouring `export_meetings` and the empty-folder
   event; a `"meetings.export"` arm next to `"notes.regenerate"`
   (l. 4010); emit `export.done` / `export.folder_needed`. Tests from the
   spec: finish with a folder writes it; no folder → event, no files;
   `export_meetings` false → nothing; agent call refused; an enrolled
   voice fixture leaves no voice data in the output. → check.
   Traps: never hold the `parking_lot` guard across `.await`; the export
   must not run before `recap::generate` finishes; Nix-locked settings
   go through `nix_settings.rs` unchanged (the key check is
   `Settings::is_key`).

5. **Overlay** (agy, parallel with 4 against the fake daemon).
   `shell/tests/fake-daemon.py`: answer `meetings.export` with a path and
   `settings.set` for the two keys; a flag `--export-ask` that emits
   `export.folder_needed` after connect. `shell/Library.qml`: in the
   Settings tab (~l. 822) an "Export meetings" switch and an "Export
   folder" row with **Change…** (`FolderDialog`, `QtQuick.Dialogs` is
   already imported, l. 7; see the `FileDialog` at l. 377 for the path
   decoding); in the notes row (~l. 257) an **Export now** button showing
   "Exported to …" or the error. `shell/Overlay.qml` (or where other
   event cards live): the first-time card. `shell/tests/smoke.sh`: cases
   for the card (both answers), the Settings row, Export now.
   → check incl. `ci.sh`. Traps: tests headless only, never the real
   desktop; agy's sandbox is read-only for commands, so edit with its
   file tools and leave running tests to the lead.

6. **Docs** (lead). README: an "Exporting meetings" section (where, what,
   plain files, the Nix option). → check.

7. **Ship** (lead). Merge main in; full check; Opus review of the diff
   against this plan, plus the `reviewers:`; fix; PR linking intent,
   spec and plan, naming who did each step; CI green; merge commit (never
   squash).

8. **Deploy and live check** (lead, after announcing on the agent bus).
   nixos PR in a worktree setting `exportFolder` for p620; merge; deploy
   from committed main. Live: one short meeting with a screenshot, one
   test interview with a PDF CV; check the folders in
   `~/Documents/Synechron/Meetings`, file modes with `stat`, and that
   Obsidian shows the images; then delete the test meetings and their
   folders. Not while the owner is recording.

## Tests

- Steps 1–4: `cargo test --workspace` (new unit and daemon tests listed
  per step); expected all pass, no ignored new tests.
- Step 5: `bash shell/tests/ci.sh` passes up to the contrast step.
- Step 7: CI green on the PR.
- Step 8: the live check above.

## Rollback

- Code: revert the merge commit on main. The new tables and columns stay
  in existing databases and are ignored by older code (`CREATE … IF NOT
  EXISTS` and extra columns are harmless).
- p620: revert the nixos PR (drops `exportFolder`), or set
  `export_meetings` off in Settings. Exported folders are the user's
  files and are never deleted by a rollback.
