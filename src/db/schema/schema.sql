-- nixarchy-meetingai local database.
CREATE TABLE IF NOT EXISTS system_prompts (
  id INTEGER PRIMARY KEY AUTOINCREMENT
, name TEXT NOT NULL
, prompt TEXT NOT NULL
, created_at TEXT DEFAULT (datetime('now')) NOT NULL
, updated_at TEXT DEFAULT (datetime('now')) NOT NULL
, listen_slot INTEGER
);

CREATE INDEX IF NOT EXISTS idx_system_prompts_name ON system_prompts(name);

CREATE UNIQUE INDEX IF NOT EXISTS idx_system_prompts_listen_slot_unique
    ON system_prompts(listen_slot)
    WHERE listen_slot IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS update_system_prompts_timestamp
AFTER UPDATE ON system_prompts
FOR EACH ROW
WHEN OLD.updated_at = NEW.updated_at
BEGIN
    UPDATE system_prompts
    SET updated_at = datetime('now')
    WHERE id = NEW.id;
END;

CREATE TABLE IF NOT EXISTS conversations (
  id TEXT
, title TEXT NOT NULL
, created_at INTEGER NOT NULL
, updated_at INTEGER NOT NULL
, PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS messages (
  id TEXT
, conversation_id TEXT NOT NULL
, role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system'))
, content TEXT NOT NULL
, timestamp INTEGER NOT NULL
, attached_files TEXT
, PRIMARY KEY (id)
, FOREIGN KEY (conversation_id) REFERENCES conversations (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_conversations_updated_at ON conversations (updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_messages_conversation_id ON messages (conversation_id);
CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages (timestamp);
CREATE INDEX IF NOT EXISTS idx_messages_conversation_timestamp
    ON messages (conversation_id, timestamp ASC);
CREATE INDEX IF NOT EXISTS idx_messages_conversation_role
    ON messages (conversation_id, role, timestamp ASC);

CREATE TRIGGER IF NOT EXISTS update_conversation_timestamp_on_message_insert
AFTER INSERT ON messages
FOR EACH ROW
BEGIN
    UPDATE conversations
    SET updated_at = NEW.timestamp
    WHERE id = NEW.conversation_id;
END;

CREATE TRIGGER IF NOT EXISTS update_conversation_timestamp_on_message_update
AFTER UPDATE ON messages
FOR EACH ROW
BEGIN
    UPDATE conversations
    SET updated_at = NEW.timestamp
    WHERE id = NEW.conversation_id;
END;

CREATE TABLE IF NOT EXISTS meetings (
  id TEXT
, title TEXT NOT NULL DEFAULT ''
, context TEXT NOT NULL DEFAULT 'auto'
, language TEXT NOT NULL DEFAULT 'en'
, started_at TEXT NOT NULL DEFAULT (datetime('now'))
, ended_at TEXT
, duration_seconds INTEGER NOT NULL DEFAULT 0
, PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS meeting_lines (
  id TEXT
, meeting_id TEXT NOT NULL
, who TEXT NOT NULL
, speaker INTEGER
, text TEXT NOT NULL
, start_time REAL NOT NULL DEFAULT 0
, end_time REAL NOT NULL DEFAULT 0
, created_at TEXT NOT NULL DEFAULT (datetime('now'))
, PRIMARY KEY (id)
, FOREIGN KEY (meeting_id) REFERENCES meetings (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_meeting_lines_meeting ON meeting_lines (meeting_id);
CREATE INDEX IF NOT EXISTS idx_meeting_lines_time ON meeting_lines (meeting_id, start_time);
CREATE INDEX IF NOT EXISTS idx_meetings_started ON meetings (started_at DESC);

CREATE TABLE IF NOT EXISTS meeting_recaps (
  meeting_id TEXT
, content TEXT NOT NULL DEFAULT ''
, status TEXT NOT NULL DEFAULT 'pending'
, error TEXT NOT NULL DEFAULT ''
, updated_at TEXT NOT NULL DEFAULT (datetime('now'))
, PRIMARY KEY (meeting_id)
, FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS files (
  id TEXT
, name TEXT NOT NULL
, mime_type TEXT NOT NULL DEFAULT ''
, size_bytes INTEGER NOT NULL DEFAULT 0
, pages INTEGER NOT NULL DEFAULT 0
, ocr_text TEXT NOT NULL DEFAULT ''
, created_at TEXT NOT NULL DEFAULT (datetime('now'))
, updated_at TEXT NOT NULL DEFAULT (datetime('now'))
, PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_files_created ON files (created_at DESC);

CREATE TABLE IF NOT EXISTS prompt_files (
  prompt_id TEXT NOT NULL
, file_id TEXT NOT NULL
, PRIMARY KEY (prompt_id, file_id)
, FOREIGN KEY (file_id) REFERENCES files (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS saved_notes (
  kind TEXT NOT NULL
, ref_id TEXT NOT NULL
, content TEXT NOT NULL DEFAULT ''
, updated_at TEXT NOT NULL DEFAULT (datetime('now'))
, PRIMARY KEY (kind, ref_id)
);

CREATE TABLE IF NOT EXISTS meeting_speakers (
  meeting_id TEXT NOT NULL
, speaker_key TEXT NOT NULL
, name TEXT NOT NULL
, PRIMARY KEY (meeting_id, speaker_key)
);

CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT
, value TEXT NOT NULL
, updated_at TEXT NOT NULL DEFAULT (datetime('now'))
, PRIMARY KEY (key)
);

CREATE TABLE IF NOT EXISTS voice_sample (
  id INTEGER PRIMARY KEY CHECK (id = 1)
, embedding BLOB NOT NULL
, model TEXT NOT NULL
, created INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cached_summaries (
  kind TEXT NOT NULL
, ref_id TEXT NOT NULL
, mode TEXT NOT NULL
, title TEXT NOT NULL
, content TEXT NOT NULL DEFAULT ''
, updated_at TEXT NOT NULL DEFAULT (datetime('now'))
, PRIMARY KEY (kind, ref_id, mode)
);

CREATE INDEX IF NOT EXISTS idx_cached_summaries_ref
    ON cached_summaries (kind, ref_id);

CREATE TABLE IF NOT EXISTS context_profiles (
  id TEXT
, name TEXT NOT NULL
, personal_context TEXT NOT NULL DEFAULT ''
, meeting_context TEXT NOT NULL DEFAULT ''
, updated_at TEXT NOT NULL DEFAULT (datetime('now'))
, PRIMARY KEY (id)
);

-- Reference text (CV, job description, runbooks) attached to a profile.
CREATE TABLE IF NOT EXISTS profile_documents (
  id INTEGER PRIMARY KEY AUTOINCREMENT
, profile TEXT NOT NULL
, name TEXT NOT NULL
, text TEXT NOT NULL
, created_at TEXT NOT NULL DEFAULT (datetime('now'))
, FOREIGN KEY (profile) REFERENCES context_profiles(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_profile_documents_profile ON profile_documents (profile);

CREATE TABLE IF NOT EXISTS conversation_state (
  conversation_id TEXT
, context_json TEXT NOT NULL
, PRIMARY KEY (conversation_id)
, FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS meeting_conversations (
  meeting_id TEXT NOT NULL
, conversation_id TEXT NOT NULL
, PRIMARY KEY (meeting_id, conversation_id)
, FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
, FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

-- Rolling notes: one summary per slice of final lines, consolidated on stop.
CREATE TABLE IF NOT EXISTS meeting_note_chunks (
  id INTEGER PRIMARY KEY AUTOINCREMENT
, meeting_id TEXT NOT NULL
, t0 REAL NOT NULL
, t1 REAL NOT NULL
, body TEXT NOT NULL
, last_line INTEGER NOT NULL DEFAULT 0
, created_at TEXT NOT NULL DEFAULT (datetime('now'))
, FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_note_chunks_meeting ON meeting_note_chunks (meeting_id);

-- Team meetings: hand-picked series, extracted items and recap search.
-- meetings.series_id is added by a guarded ALTER in db/mod.rs.
CREATE TABLE IF NOT EXISTS series (
  id TEXT
, name TEXT NOT NULL UNIQUE
, created_at TEXT NOT NULL DEFAULT (datetime('now'))
, PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS meeting_items (
  id TEXT
, meeting_id TEXT NOT NULL
, series_id TEXT
, kind TEXT NOT NULL CHECK (kind IN ('action','decision','question'))
, text TEXT NOT NULL
, owner TEXT
, due TEXT
, status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','done','dropped'))
, source_line INTEGER
, origin_item TEXT
, draft INTEGER NOT NULL DEFAULT 1
, created_at TEXT NOT NULL DEFAULT (datetime('now'))
, PRIMARY KEY (id)
, FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_meeting_items_meeting ON meeting_items (meeting_id);
CREATE INDEX IF NOT EXISTS idx_meeting_items_series ON meeting_items (series_id, status);

CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(meeting_id UNINDEXED, body);

-- Interviewer mode: one candidate per interview, never shared with profiles or other meetings.
CREATE TABLE IF NOT EXISTS candidates (
  id TEXT
, meeting_id TEXT
, name TEXT NOT NULL DEFAULT ''
, local_only INTEGER NOT NULL DEFAULT 0
, created_at TEXT NOT NULL DEFAULT (datetime('now'))
, PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS candidate_documents (
  id TEXT
, candidate_id TEXT NOT NULL
, kind TEXT NOT NULL CHECK (kind IN ('cv','job'))
, name TEXT NOT NULL
, text TEXT NOT NULL
, PRIMARY KEY (id)
, FOREIGN KEY (candidate_id) REFERENCES candidates(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS interview_questions (
  id TEXT
, candidate_id TEXT NOT NULL
, text TEXT NOT NULL
, source TEXT NOT NULL
, competency TEXT
, status TEXT NOT NULL DEFAULT 'planned' CHECK (status IN ('planned','asked','covered','skipped'))
, note TEXT
, position INTEGER NOT NULL
, PRIMARY KEY (id)
, FOREIGN KEY (candidate_id) REFERENCES candidates(id) ON DELETE CASCADE
);
-- A prep chat started before the meeting row exists hangs off the candidate instead.
CREATE TABLE IF NOT EXISTS candidate_conversations (
  candidate_id TEXT NOT NULL
, conversation_id TEXT NOT NULL
, PRIMARY KEY (candidate_id, conversation_id)
, FOREIGN KEY (candidate_id) REFERENCES candidates(id) ON DELETE CASCADE
, FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_interview_questions_candidate ON interview_questions (candidate_id, position);
