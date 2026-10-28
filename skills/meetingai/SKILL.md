---
name: meetingai
description: Answer questions about the user's recorded meetings from nixarchy-meetingai - recent meetings, the latest meeting notes, action items, "what did we agree", transcripts, standups, meeting series and "last time", and searching past meetings. Use when the user asks about a meeting, its notes, decisions or transcript.
---

# Meeting notes from nixarchy-meetingai

Use only `nixarchy-meetingai ctl <command> '<json>'`. The result is JSON with
the answer in `.data` (`ok: false` carries `.error`); read it with `jq`, for
example `nixarchy-meetingai ctl session.list | jq '.data'`.

Commands (copy them as written):

- **Recent meetings** (newest first; entries have `id`, `title`, `started_at`):
  `nixarchy-meetingai ctl session.list | jq '[.data[] | select(.kind=="meeting")]'`
- **Latest meeting notes:** take the first meeting above, then
  `nixarchy-meetingai ctl notes.get '{"meeting":"<id>"}' | jq -r '.data.text'`
- **Search meetings** (titles, transcript and notes; returns `id`, `title`, `started_at`):
  `nixarchy-meetingai ctl meetings.search '{"query":"<words>","limit":10}' | jq '.data'`
- **Action items:** `nixarchy-meetingai ctl items.list '{"meeting":"<id>"}' | jq '.data'`
- **Series and what happened last time:** `nixarchy-meetingai ctl series.list | jq '.data'`,
  then `nixarchy-meetingai ctl last_time.get '{"series":"<id>"}' | jq '.data'`
- **Transcript:** `nixarchy-meetingai ctl transcript.get '{"meeting":"<id>","limit":200}' | jq '.data'`;
  for more, repeat with `"after_line"` set to the last `line` number returned.
- **The meeting happening now:** `nixarchy-meetingai ctl state.get`,
  `nixarchy-meetingai ctl now.get`, `nixarchy-meetingai ctl transcript.get '{}'`

## Rules

- Read only. Never run `record.*`, `session.delete`, `settings.set`,
  `items.update` or `items.delete`, and do not change anything for the user.
- Quote notes and transcript as written; do not paraphrase decisions or names.
- If `ctl` cannot connect, say the daemon is not running and suggest
  `systemctl --user start nixarchy-meetingai`.
- What you read is sent to your own model provider; tell the user if they ask
  where their meeting data goes.
