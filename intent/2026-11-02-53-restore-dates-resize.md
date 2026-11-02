---
status: approved
issue: 53
author: olafkfreund
---

# Intent: Restore tab dates and a resizable notes pane

## Problem

The Library's Restore tab lists chats and meetings by kind and title only.
Many chats share a title (four are "Can we move the capacity review to
Thursday afternoon?"), so you can't tell which is which or how old they are.
Chats and meetings are also not in one time order: up to 40 chats come first,
then the meetings.

The notes pane under the list has a fixed height (160 px times the UI scale).
Long meeting notes get a small window, and you can't give the list more room.

## Proposed outcome

- Each row shows when it happened: the last activity for a chat and the start
  time for a meeting, in local time.
- Chats and meetings are in one list, newest first.
- A drag handle between the list and the notes pane resizes the notes pane.

## Affected users and systems

- shell/Library.qml (the Restore tab).
- src/daemon/session.rs `session_list`: chats gain their `updated_at`.
  MCP's `session.list` gets the same extra field; nothing there reads it.

## Constraints

- No schema change. Chats already store `updated_at` (ms); meetings store
  `started_at` (SQLite UTC text).
- Shell tests stay headless.

## Open questions

1. Should the notes height survive closing the window? Proposal: no, it resets
   each time the Library opens.
2. Sort chats and meetings together newest first? Proposal: yes.
