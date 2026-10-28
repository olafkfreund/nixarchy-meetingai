-- nixarchy-meetingai local database.
-- Reference text (CV, job description, runbooks) attached to a profile.
CREATE TABLE IF NOT EXISTS profile_documents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    profile TEXT NOT NULL REFERENCES context_profiles(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    text TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_profile_documents_profile ON profile_documents (profile);

-- Rolling notes: one summary per slice of final lines, consolidated on stop.
CREATE TABLE IF NOT EXISTS meeting_note_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    t0 REAL NOT NULL,
    t1 REAL NOT NULL,
    body TEXT NOT NULL,
    last_line INTEGER NOT NULL DEFAULT 0, -- highest meeting_lines rowid this chunk covers
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
CREATE INDEX IF NOT EXISTS idx_note_chunks_meeting ON meeting_note_chunks (meeting_id);

-- Team meetings: hand-picked series, extracted items and recap search.
-- meetings.series_id is added by a guarded ALTER in db/mod.rs.
CREATE TABLE IF NOT EXISTS series (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS meeting_items (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    series_id TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('action','decision','question')),
    text TEXT NOT NULL,
    owner TEXT,
    due TEXT,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','done','dropped')),
    source_line INTEGER,
    origin_item TEXT,
    draft INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_meeting_items_meeting ON meeting_items (meeting_id);
CREATE INDEX IF NOT EXISTS idx_meeting_items_series ON meeting_items (series_id, status);

CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(meeting_id UNINDEXED, body);

-- Interviewer mode: one candidate per interview, never shared with profiles or other meetings.
CREATE TABLE IF NOT EXISTS candidates (
    id TEXT PRIMARY KEY,
    meeting_id TEXT,
    name TEXT NOT NULL DEFAULT '',
    local_only INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS candidate_documents (
    id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL REFERENCES candidates(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('cv','job')),
    name TEXT NOT NULL,
    text TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS interview_questions (
    id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL REFERENCES candidates(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    source TEXT NOT NULL,
    competency TEXT,
    status TEXT NOT NULL DEFAULT 'planned' CHECK (status IN ('planned','asked','covered','skipped')),
    note TEXT,
    position INTEGER NOT NULL
);
-- A prep chat started before the meeting row exists hangs off the candidate instead.
CREATE TABLE IF NOT EXISTS candidate_conversations (
    candidate_id TEXT NOT NULL REFERENCES candidates(id) ON DELETE CASCADE,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    PRIMARY KEY (candidate_id, conversation_id)
);
CREATE INDEX IF NOT EXISTS idx_interview_questions_candidate ON interview_questions (candidate_id, position);
