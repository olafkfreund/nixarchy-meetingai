-- nixarchy-meetingai local database.
--
-- Conversations, messages, live-session transcripts, the document library and
-- shared app settings. Triggers keep conversation timestamps in step with the
-- messages inside them, and cascades keep child rows from outliving their parent.
--
-- The five default listen modes are seeded from Rust (`db::seed`), not here, so
-- seeding stays idempotent and testable.

CREATE TABLE IF NOT EXISTS system_prompts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    prompt TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')) NOT NULL,
    updated_at TEXT DEFAULT (datetime('now')) NOT NULL,
    listen_slot INTEGER
);

CREATE INDEX IF NOT EXISTS idx_system_prompts_name ON system_prompts(name);

-- One prompt per listen slot: the tab order is the slot value.
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
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    attached_files TEXT,
    FOREIGN KEY (conversation_id) REFERENCES conversations (id) ON DELETE CASCADE
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
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL DEFAULT '',
    context TEXT NOT NULL DEFAULT 'auto',
    language TEXT NOT NULL DEFAULT 'en',
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at TEXT,
    duration_seconds INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS meeting_lines (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    who TEXT NOT NULL,
    speaker INTEGER,
    text TEXT NOT NULL,
    start_time REAL NOT NULL DEFAULT 0,
    end_time REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_meeting_lines_meeting ON meeting_lines (meeting_id);
CREATE INDEX IF NOT EXISTS idx_meeting_lines_time ON meeting_lines (meeting_id, start_time);
CREATE INDEX IF NOT EXISTS idx_meetings_started ON meetings (started_at DESC);

CREATE TABLE IF NOT EXISTS meeting_recaps (
    meeting_id TEXT PRIMARY KEY,
    content TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    error TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    mime_type TEXT NOT NULL DEFAULT '',
    size_bytes INTEGER NOT NULL DEFAULT 0,
    pages INTEGER NOT NULL DEFAULT 0,
    ocr_text TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_files_created ON files (created_at DESC);

CREATE TABLE IF NOT EXISTS prompt_files (
    prompt_id TEXT NOT NULL,
    file_id TEXT NOT NULL,
    PRIMARY KEY (prompt_id, file_id),
    FOREIGN KEY (file_id) REFERENCES files (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS saved_notes (
    kind TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (kind, ref_id)
);

CREATE TABLE IF NOT EXISTS meeting_speakers (
    meeting_id TEXT NOT NULL,
    speaker_key TEXT NOT NULL,
    name TEXT NOT NULL,
    PRIMARY KEY (meeting_id, speaker_key)
);

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS cached_summaries (
    kind TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    mode TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (kind, ref_id, mode)
);

CREATE INDEX IF NOT EXISTS idx_cached_summaries_ref
    ON cached_summaries (kind, ref_id);

CREATE TABLE IF NOT EXISTS context_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    personal_context TEXT NOT NULL DEFAULT '',
    meeting_context TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS conversation_state (
    conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
    context_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS meeting_conversations (
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    PRIMARY KEY (meeting_id, conversation_id)
);

-- Rolling notes: one summary per slice of final lines, consolidated on stop.
CREATE TABLE IF NOT EXISTS meeting_note_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    t0 REAL NOT NULL,
    t1 REAL NOT NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_note_chunks_meeting ON meeting_note_chunks (meeting_id);
