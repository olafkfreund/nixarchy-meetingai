---
name: meetingai
description: Answer questions about the user's recorded meetings from nixarchy-meetingai - recent meetings, the latest meeting notes, action items, "what did we agree", transcripts, standups, meeting series and "last time", and searching past meetings. Use when the user asks about a meeting, its notes, decisions or transcript.
---

# Meeting notes from nixarchy-meetingai

Use only `nixarchy-meetingai ctl --agent <command> '<json>'`. The result is JSON with
the answer in `.data` (`ok: false` carries `.error`); read it with `jq`, for
example `nixarchy-meetingai ctl --agent session.list | jq '.data'`.

Build the JSON with `jq` so quotes and apostrophes in words or ids cannot
break the shell quoting, for example:
`nixarchy-meetingai ctl --agent meetings.search "$(jq -nc --arg q "Jim's budget" '{query:$q,limit:10}')"`.
The same goes for ids: `"$(jq -nc --arg m "<id>" '{meeting:$m}')"`. Always check
`.ok` before reading `.data`; on `ok: false` report `.error` to the user.

Commands (the quoted `'{...}'` forms below are shorthand for that pattern when
the value is an id or plain word):

- **Recent meetings** (newest first; entries have `id`, `title`, `started_at`):
  `nixarchy-meetingai ctl --agent session.list | jq '[.data[] | select(.kind=="meeting")]'`
- **Latest meeting notes:** take the first meeting above, then
  `nixarchy-meetingai ctl --agent notes.get '{"meeting":"<id>"}' | jq -r '.data.text'`
- **Search meetings** (titles, transcript and notes; returns `id`, `title`, `started_at`). The query matches an exact word or phrase; search one word at a time if several words find nothing:
  `nixarchy-meetingai ctl --agent meetings.search '{"query":"<words>","limit":10}' | jq '.data'`
- **Action items:** `nixarchy-meetingai ctl --agent items.list '{"meeting":"<id>"}' | jq '.data'`
- **Series and what happened last time:** `nixarchy-meetingai ctl --agent series.list | jq '.data'`,
  then `nixarchy-meetingai ctl --agent last_time.get '{"series":"<id>"}' | jq '.data'`
- **Transcript:** `nixarchy-meetingai ctl --agent transcript.get '{"meeting":"<id>","limit":200}' | jq '.data'`;
  for more, repeat with `"after_line"` set to the last `line` number returned.
- **The meeting happening now:** `nixarchy-meetingai ctl --agent state.get`,
  `nixarchy-meetingai ctl --agent now.get`, `nixarchy-meetingai ctl --agent transcript.get '{}'`

## Rules

- Read only, and the app enforces it: `--agent` accepts only these read
  commands and hides local-only interviews. Never run `record.*`, `session.delete`, `settings.set`,
  `items.update` or `items.delete`, and do not change anything for the user.
- Quote notes and transcript as written; do not paraphrase decisions or names.
- If `ctl` cannot connect, say the daemon is not running and suggest
  `systemctl --user start nixarchy-meetingai`.
- What you read is sent to your own model provider; tell the user if they ask
  where their meeting data goes.
