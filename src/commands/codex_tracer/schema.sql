-- Codex Tracer SQLite Schema
-- Version: 1.0.0

-- Main events table: stores detailed Codex usage events
CREATE TABLE IF NOT EXISTS codex_tracer_events (
    -- Primary Keys
    record_id TEXT PRIMARY KEY,

    -- Session/Thread Metadata
    session_id TEXT NOT NULL,
    thread_name TEXT,
    session_updated_at TEXT,
    thread_key TEXT,
    thread_call_index INTEGER,
    previous_record_id TEXT,
    next_record_id TEXT,
    thread_source TEXT,

    -- Event Metadata
    event_timestamp TEXT NOT NULL,
    source_file TEXT NOT NULL,
    line_number INTEGER NOT NULL,
    turn_id TEXT,
    turn_timestamp TEXT,
    cwd TEXT,
    current_date TEXT,
    timezone TEXT,
    is_archived INTEGER NOT NULL DEFAULT 0,

    -- Model Information
    model TEXT,
    effort TEXT,
    model_context_window INTEGER,

    -- Call Origin Classification
    call_initiator TEXT,
    call_initiator_reason TEXT,
    call_initiator_confidence TEXT,

    -- Subagent Information
    subagent_type TEXT,
    agent_role TEXT,
    agent_nickname TEXT,
    parent_session_id TEXT,
    parent_thread_name TEXT,
    parent_session_updated_at TEXT,

    -- Token Accounting (Fine-grained)
    input_tokens INTEGER NOT NULL,
    cached_input_tokens INTEGER NOT NULL,
    uncached_input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    reasoning_output_tokens INTEGER NOT NULL,
    total_tokens INTEGER NOT NULL,

    -- Cumulative Token Accounting
    cumulative_input_tokens INTEGER NOT NULL,
    cumulative_cached_input_tokens INTEGER NOT NULL,
    cumulative_output_tokens INTEGER NOT NULL,
    cumulative_reasoning_output_tokens INTEGER NOT NULL,
    cumulative_total_tokens INTEGER NOT NULL,

    -- Computed Fields
    cache_ratio REAL NOT NULL,
    reasoning_output_ratio REAL NOT NULL,
    context_window_percent REAL NOT NULL
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_session_id ON codex_tracer_events(session_id);
CREATE INDEX IF NOT EXISTS idx_thread_key ON codex_tracer_events(thread_key);
CREATE INDEX IF NOT EXISTS idx_event_timestamp ON codex_tracer_events(event_timestamp);
CREATE INDEX IF NOT EXISTS idx_model ON codex_tracer_events(model);
CREATE INDEX IF NOT EXISTS idx_is_archived ON codex_tracer_events(is_archived);

-- Thread summaries table: aggregated statistics per thread
CREATE TABLE IF NOT EXISTS thread_summaries (
    thread_key TEXT PRIMARY KEY,
    first_record_id TEXT,
    last_record_id TEXT,
    call_count INTEGER NOT NULL,
    total_tokens_sum INTEGER NOT NULL,
    estimated_cost_sum REAL
);

-- Source files table: tracks parsing progress for incremental updates
CREATE TABLE IF NOT EXISTS source_files (
    file_path TEXT PRIMARY KEY,
    last_parsed_at TEXT NOT NULL,
    last_byte_offset INTEGER NOT NULL,
    last_line_number INTEGER NOT NULL,
    file_size INTEGER NOT NULL,
    file_mtime TEXT
);

-- Schema metadata
CREATE TABLE IF NOT EXISTS schema_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO schema_metadata (key, value) VALUES ('version', '1.0.0');
INSERT OR REPLACE INTO schema_metadata (key, value) VALUES ('created_at', datetime('now'));
