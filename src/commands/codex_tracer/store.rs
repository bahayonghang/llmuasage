//! SQLite store for codex-tracer events.

use std::{path::Path, time::Duration};

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use super::models::{CodexTracerEvent, ThreadSummary};

/// Durable per-file state for bounded tracer ingestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TracerFileState {
    pub path_hash: String,
    pub file_fingerprint: String,
    pub file_size: u64,
    pub file_mtime_ns: i64,
    pub tail_signature: String,
    pub durable_offset: u64,
    pub line_number: u64,
    pub parser_state_json: String,
    pub updated_at: String,
}

/// Store for codex-tracer events.
pub struct CodexTracerStore {
    conn: Connection,
}

impl CodexTracerStore {
    /// Open or create the codex-tracer database.
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open database at {}", db_path.display()))?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "wal_autocheckpoint", 32_768)?;

        let mut store = Self { conn };
        store.init_schema()?;

        Ok(store)
    }

    /// Initialize the database schema.
    fn init_schema(&mut self) -> Result<()> {
        let schema = include_str!("schema.sql");
        self.conn
            .execute_batch(schema)
            .context("Failed to initialize database schema")?;
        Ok(())
    }

    /// Insert or update events in bulk.
    pub fn upsert_events(&mut self, events: &[CodexTracerEvent]) -> Result<usize> {
        self.commit_events(events, None)
    }

    /// Commit one bounded event batch and its matching durable parser state.
    pub(crate) fn commit_ingest_batch(
        &mut self,
        source_file: &str,
        state: &TracerFileState,
        events: &[CodexTracerEvent],
        reset_file: bool,
    ) -> Result<usize> {
        self.commit_events(events, Some((source_file, state, reset_file)))
    }

    fn commit_events(
        &mut self,
        events: &[CodexTracerEvent],
        file_state: Option<(&str, &TracerFileState, bool)>,
    ) -> Result<usize> {
        let tx = self
            .conn
            .transaction()
            .context("Failed to start transaction")?;

        if let Some((source_file, state, true)) = file_state {
            tx.execute(
                "DELETE FROM codex_tracer_events WHERE source_file = ?1",
                [source_file],
            )?;
            tx.execute(
                "DELETE FROM tracer_file_state WHERE path_hash = ?1",
                [&state.path_hash],
            )?;
        }

        let mut inserted = 0;

        for event in events {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO codex_tracer_events (
                    record_id, session_id, thread_name, session_updated_at,
                    thread_key, thread_call_index, previous_record_id, next_record_id,
                    thread_source, event_timestamp, source_file, line_number,
                    turn_id, turn_timestamp, cwd, current_date, timezone, is_archived,
                    model, effort, model_context_window,
                    call_initiator, call_initiator_reason, call_initiator_confidence,
                    subagent_type, agent_role, agent_nickname,
                    parent_session_id, parent_thread_name, parent_session_updated_at,
                    input_tokens, cached_input_tokens, uncached_input_tokens,
                    output_tokens, reasoning_output_tokens, total_tokens,
                    cumulative_input_tokens, cumulative_cached_input_tokens,
                    cumulative_output_tokens, cumulative_reasoning_output_tokens,
                    cumulative_total_tokens,
                    cache_ratio, reasoning_output_ratio, context_window_percent
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                    ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                    ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
                    ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40,
                    ?41, ?42, ?43, ?44
                )
                "#,
                params![
                    event.record_id,
                    event.session_id,
                    event.thread_name,
                    event.session_updated_at,
                    event.thread_key,
                    event.thread_call_index,
                    event.previous_record_id,
                    event.next_record_id,
                    event.thread_source,
                    event.event_timestamp,
                    event.source_file,
                    event.line_number,
                    event.turn_id,
                    event.turn_timestamp,
                    event.cwd,
                    event.current_date,
                    event.timezone,
                    if event.is_archived { 1 } else { 0 },
                    event.model,
                    event.effort,
                    event.model_context_window,
                    event.call_initiator,
                    event.call_initiator_reason,
                    event.call_initiator_confidence,
                    event.subagent_type,
                    event.agent_role,
                    event.agent_nickname,
                    event.parent_session_id,
                    event.parent_thread_name,
                    event.parent_session_updated_at,
                    event.input_tokens,
                    event.cached_input_tokens,
                    event.uncached_input_tokens,
                    event.output_tokens,
                    event.reasoning_output_tokens,
                    event.total_tokens,
                    event.cumulative_input_tokens,
                    event.cumulative_cached_input_tokens,
                    event.cumulative_output_tokens,
                    event.cumulative_reasoning_output_tokens,
                    event.cumulative_total_tokens,
                    event.cache_ratio,
                    event.reasoning_output_ratio,
                    event.context_window_percent,
                ],
            )?;
            inserted += 1;
        }

        if let Some((_source_file, state, _reset_file)) = file_state {
            tx.execute(
                r#"
                INSERT INTO tracer_file_state (
                    path_hash, file_fingerprint, file_size, file_mtime_ns,
                    tail_signature, durable_offset, line_number,
                    parser_state_json, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(path_hash) DO UPDATE SET
                    file_fingerprint = excluded.file_fingerprint,
                    file_size = excluded.file_size,
                    file_mtime_ns = excluded.file_mtime_ns,
                    tail_signature = excluded.tail_signature,
                    durable_offset = excluded.durable_offset,
                    line_number = excluded.line_number,
                    parser_state_json = excluded.parser_state_json,
                    updated_at = excluded.updated_at
                "#,
                params![
                    state.path_hash,
                    state.file_fingerprint,
                    i64::try_from(state.file_size).unwrap_or(i64::MAX),
                    state.file_mtime_ns,
                    state.tail_signature,
                    i64::try_from(state.durable_offset).unwrap_or(i64::MAX),
                    i64::try_from(state.line_number).unwrap_or(i64::MAX),
                    state.parser_state_json,
                    state.updated_at,
                ],
            )?;
        }

        tx.commit().context("Failed to commit transaction")?;

        Ok(inserted)
    }

    pub(crate) fn file_state(&self, path_hash: &str) -> Result<Option<TracerFileState>> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT path_hash, file_fingerprint, file_size, file_mtime_ns,
                   tail_signature, durable_offset, line_number,
                   parser_state_json, updated_at
            FROM tracer_file_state
            WHERE path_hash = ?1
            "#,
        )?;
        let mut rows = statement.query([path_hash])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let file_size = row.get::<_, i64>(2)?;
        let durable_offset = row.get::<_, i64>(5)?;
        let line_number = row.get::<_, i64>(6)?;
        Ok(Some(TracerFileState {
            path_hash: row.get(0)?,
            file_fingerprint: row.get(1)?,
            file_size: u64::try_from(file_size).unwrap_or_default(),
            file_mtime_ns: row.get(3)?,
            tail_signature: row.get(4)?,
            durable_offset: u64::try_from(durable_offset).unwrap_or_default(),
            line_number: u64::try_from(line_number).unwrap_or_default(),
            parser_state_json: row.get(7)?,
            updated_at: row.get(8)?,
        }))
    }

    /// Rebuild call indices and previous/next links across all files without
    /// retaining the historical event set in process memory.
    pub(crate) fn relink_threads(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch(
            r#"
            WITH ranked AS (
                SELECT
                    record_id,
                    ROW_NUMBER() OVER (
                        PARTITION BY thread_key
                        ORDER BY event_timestamp, record_id
                    ) - 1 AS call_index,
                    LAG(record_id) OVER (
                        PARTITION BY thread_key
                        ORDER BY event_timestamp, record_id
                    ) AS previous_record_id,
                    LEAD(record_id) OVER (
                        PARTITION BY thread_key
                        ORDER BY event_timestamp, record_id
                    ) AS next_record_id
                FROM codex_tracer_events
                WHERE thread_key IS NOT NULL
            )
            UPDATE codex_tracer_events AS events
            SET
                thread_call_index = ranked.call_index,
                previous_record_id = ranked.previous_record_id,
                next_record_id = ranked.next_record_id
            FROM ranked
            WHERE events.record_id = ranked.record_id
              AND (
                  events.thread_call_index IS NOT ranked.call_index
                  OR events.previous_record_id IS NOT ranked.previous_record_id
                  OR events.next_record_id IS NOT ranked.next_record_id
              );

            DELETE FROM thread_summaries;
            INSERT INTO thread_summaries (
                thread_key, first_record_id, last_record_id,
                call_count, total_tokens_sum, estimated_cost_sum
            )
            SELECT
                thread_key,
                MIN(CASE WHEN thread_call_index = 0 THEN record_id END),
                MAX(CASE WHEN next_record_id IS NULL THEN record_id END),
                COUNT(*),
                SUM(total_tokens),
                NULL
            FROM codex_tracer_events
            WHERE thread_key IS NOT NULL
            GROUP BY thread_key;
            "#,
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Query events with optional filters.
    pub fn query_calls(&self, filters: &CallFilters) -> Result<Vec<CodexTracerEvent>> {
        let mut sql = String::from(
            r#"
            SELECT
                record_id, session_id, thread_name, session_updated_at,
                thread_key, thread_call_index, previous_record_id, next_record_id,
                thread_source, event_timestamp, source_file, line_number,
                turn_id, turn_timestamp, cwd, current_date, timezone, is_archived,
                model, effort, model_context_window,
                call_initiator, call_initiator_reason, call_initiator_confidence,
                subagent_type, agent_role, agent_nickname,
                parent_session_id, parent_thread_name, parent_session_updated_at,
                input_tokens, cached_input_tokens, uncached_input_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cumulative_input_tokens, cumulative_cached_input_tokens,
                cumulative_output_tokens, cumulative_reasoning_output_tokens,
                cumulative_total_tokens,
                cache_ratio, reasoning_output_ratio, context_window_percent
            FROM codex_tracer_events
            WHERE 1=1
            "#,
        );

        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(model) = &filters.model {
            sql.push_str(" AND model = ?");
            params.push(Box::new(model.clone()));
        }

        if let Some(since) = &filters.since {
            sql.push_str(" AND event_timestamp >= ?");
            params.push(Box::new(since.clone()));
        }

        if let Some(until) = &filters.until {
            sql.push_str(" AND event_timestamp <= ?");
            params.push(Box::new(until.clone()));
        }

        if !filters.include_archived {
            sql.push_str(" AND is_archived = 0");
        }

        sql.push_str(" ORDER BY event_timestamp DESC");

        if let Some(limit) = filters.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let events = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(CodexTracerEvent {
                    record_id: row.get(0)?,
                    session_id: row.get(1)?,
                    thread_name: row.get(2)?,
                    session_updated_at: row.get(3)?,
                    thread_key: row.get(4)?,
                    thread_call_index: row.get(5)?,
                    previous_record_id: row.get(6)?,
                    next_record_id: row.get(7)?,
                    thread_source: row.get(8)?,
                    event_timestamp: row.get(9)?,
                    source_file: row.get(10)?,
                    line_number: row.get(11)?,
                    turn_id: row.get(12)?,
                    turn_timestamp: row.get(13)?,
                    cwd: row.get(14)?,
                    current_date: row.get(15)?,
                    timezone: row.get(16)?,
                    is_archived: row.get::<_, i32>(17)? != 0,
                    model: row.get(18)?,
                    effort: row.get(19)?,
                    model_context_window: row.get(20)?,
                    call_initiator: row.get(21)?,
                    call_initiator_reason: row.get(22)?,
                    call_initiator_confidence: row.get(23)?,
                    subagent_type: row.get(24)?,
                    agent_role: row.get(25)?,
                    agent_nickname: row.get(26)?,
                    parent_session_id: row.get(27)?,
                    parent_thread_name: row.get(28)?,
                    parent_session_updated_at: row.get(29)?,
                    input_tokens: row.get(30)?,
                    cached_input_tokens: row.get(31)?,
                    uncached_input_tokens: row.get(32)?,
                    output_tokens: row.get(33)?,
                    reasoning_output_tokens: row.get(34)?,
                    total_tokens: row.get(35)?,
                    cumulative_input_tokens: row.get(36)?,
                    cumulative_cached_input_tokens: row.get(37)?,
                    cumulative_output_tokens: row.get(38)?,
                    cumulative_reasoning_output_tokens: row.get(39)?,
                    cumulative_total_tokens: row.get(40)?,
                    cache_ratio: row.get(41)?,
                    reasoning_output_ratio: row.get(42)?,
                    context_window_percent: row.get(43)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(events)
    }

    /// Count total events.
    pub fn count_events(&self) -> Result<i64> {
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM codex_tracer_events", [], |row| {
                    row.get(0)
                })?;
        Ok(count)
    }

    /// Query thread summaries.
    pub fn query_threads(&self) -> Result<Vec<ThreadSummary>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                thread_key, first_record_id, last_record_id,
                call_count, total_tokens_sum, estimated_cost_sum
            FROM thread_summaries
            ORDER BY total_tokens_sum DESC
            "#,
        )?;

        let threads = stmt
            .query_map([], |row| {
                Ok(ThreadSummary {
                    thread_key: row.get(0)?,
                    first_record_id: row.get(1)?,
                    last_record_id: row.get(2)?,
                    call_count: row.get(3)?,
                    total_tokens_sum: row.get(4)?,
                    estimated_cost_sum: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(threads)
    }

    /// Rebuild thread summaries from events.
    pub fn rebuild_thread_summaries(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;

        tx.execute("DELETE FROM thread_summaries", [])?;

        tx.execute(
            r#"
            INSERT INTO thread_summaries (
                thread_key, first_record_id, last_record_id,
                call_count, total_tokens_sum, estimated_cost_sum
            )
            SELECT
                thread_key,
                MIN(record_id) as first_record_id,
                MAX(record_id) as last_record_id,
                COUNT(*) as call_count,
                SUM(total_tokens) as total_tokens_sum,
                NULL as estimated_cost_sum
            FROM codex_tracer_events
            WHERE thread_key IS NOT NULL
            GROUP BY thread_key
            "#,
            [],
        )?;

        tx.commit()?;

        Ok(())
    }
}

/// Filters for querying calls.
#[derive(Debug, Default, Clone)]
pub struct CallFilters {
    /// Filter by model name
    pub model: Option<String>,
    /// Filter by events after this timestamp (RFC 3339)
    pub since: Option<String>,
    /// Filter by events before this timestamp (RFC 3339)
    pub until: Option<String>,
    /// Include archived sessions
    pub include_archived: bool,
    /// Limit number of results
    pub limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_store_open_and_init() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let store = CodexTracerStore::open(&db_path);
        assert!(store.is_ok());
    }

    #[test]
    fn test_upsert_and_query_events() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = CodexTracerStore::open(&db_path).unwrap();

        let events = vec![
            CodexTracerEvent::new(
                "record-1".to_string(),
                "session-1".to_string(),
                "2026-06-16T10:00:00Z".to_string(),
                "/path/to/file1.jsonl".to_string(),
                1,
                1000,
                600,
                200,
                50,
            ),
            CodexTracerEvent::new(
                "record-2".to_string(),
                "session-1".to_string(),
                "2026-06-16T11:00:00Z".to_string(),
                "/path/to/file1.jsonl".to_string(),
                2,
                2000,
                1500,
                300,
                100,
            ),
        ];

        let inserted = store.upsert_events(&events).unwrap();
        assert_eq!(inserted, 2);

        let count = store.count_events().unwrap();
        assert_eq!(count, 2);

        let queried = store.query_calls(&CallFilters::default()).unwrap();
        assert_eq!(queried.len(), 2);
    }

    #[test]
    fn test_query_with_filters() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = CodexTracerStore::open(&db_path).unwrap();

        let mut event1 = CodexTracerEvent::new(
            "record-1".to_string(),
            "session-1".to_string(),
            "2026-06-16T10:00:00Z".to_string(),
            "/path/to/file1.jsonl".to_string(),
            1,
            1000,
            600,
            200,
            50,
        );
        event1.model = Some("gpt-4".to_string());

        let mut event2 = CodexTracerEvent::new(
            "record-2".to_string(),
            "session-1".to_string(),
            "2026-06-16T11:00:00Z".to_string(),
            "/path/to/file1.jsonl".to_string(),
            2,
            2000,
            1500,
            300,
            100,
        );
        event2.model = Some("o1-preview".to_string());

        store.upsert_events(&[event1, event2]).unwrap();

        let filters = CallFilters {
            model: Some("gpt-4".to_string()),
            ..Default::default()
        };

        let queried = store.query_calls(&filters).unwrap();
        assert_eq!(queried.len(), 1);
        assert_eq!(queried[0].model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_idempotent_upsert() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = CodexTracerStore::open(&db_path).unwrap();

        let event = CodexTracerEvent::new(
            "record-1".to_string(),
            "session-1".to_string(),
            "2026-06-16T10:00:00Z".to_string(),
            "/path/to/file1.jsonl".to_string(),
            1,
            1000,
            600,
            200,
            50,
        );

        store.upsert_events(std::slice::from_ref(&event)).unwrap();
        store.upsert_events(std::slice::from_ref(&event)).unwrap();

        let count = store.count_events().unwrap();
        assert_eq!(count, 1); // Should still be 1 (not 2)
    }

    #[test]
    fn tracer_file_state_upgrade_is_additive_and_idempotent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("legacy.db");
        let event = CodexTracerEvent::new(
            "legacy-record".to_string(),
            "legacy-session".to_string(),
            "2026-06-16T10:00:00Z".to_string(),
            "/path/to/legacy.jsonl".to_string(),
            1,
            1000,
            600,
            200,
            50,
        );
        let mut store = CodexTracerStore::open(&db_path).unwrap();
        store.upsert_events(&[event]).unwrap();
        store
            .conn
            .execute("DROP TABLE tracer_file_state", [])
            .unwrap();
        drop(store);

        let store = CodexTracerStore::open(&db_path).unwrap();
        assert_eq!(store.count_events().unwrap(), 1);
        assert!(store.file_state("missing").unwrap().is_none());
        drop(store);
        let store = CodexTracerStore::open(&db_path).unwrap();
        assert_eq!(store.count_events().unwrap(), 1);
    }

    #[test]
    fn failed_batch_rolls_back_file_reset_events_and_checkpoint() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("rollback.db");
        let source_file = "/path/to/file.jsonl";
        let old = CodexTracerEvent::new(
            "old-record".to_string(),
            "session-1".to_string(),
            "2026-06-16T10:00:00Z".to_string(),
            source_file.to_string(),
            1,
            1000,
            600,
            200,
            50,
        );
        let mut store = CodexTracerStore::open(&db_path).unwrap();
        store.upsert_events(&[old]).unwrap();
        store
            .conn
            .execute_batch(
                r#"
                CREATE TRIGGER fail_new_tracer_event
                BEFORE INSERT ON codex_tracer_events
                WHEN NEW.record_id = 'new-record'
                BEGIN
                    SELECT RAISE(ABORT, 'injected batch failure');
                END;
                "#,
            )
            .unwrap();
        let new = CodexTracerEvent::new(
            "new-record".to_string(),
            "session-1".to_string(),
            "2026-06-16T11:00:00Z".to_string(),
            source_file.to_string(),
            2,
            2000,
            1200,
            300,
            100,
        );
        let state = TracerFileState {
            path_hash: "path-hash".to_string(),
            file_fingerprint: "head".to_string(),
            file_size: 42,
            file_mtime_ns: 7,
            tail_signature: "tail".to_string(),
            durable_offset: 42,
            line_number: 2,
            parser_state_json: "{}".to_string(),
            updated_at: "2026-08-24T00:00:00Z".to_string(),
        };
        assert!(
            store
                .commit_ingest_batch(source_file, &state, &[new], true)
                .is_err()
        );
        assert_eq!(store.count_events().unwrap(), 1);
        assert_eq!(
            store
                .query_calls(&CallFilters {
                    include_archived: true,
                    ..CallFilters::default()
                })
                .unwrap()[0]
                .record_id,
            "old-record"
        );
        assert!(store.file_state("path-hash").unwrap().is_none());
    }
}
