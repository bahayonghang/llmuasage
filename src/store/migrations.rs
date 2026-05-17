use std::time::Instant;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use tracing::info;

use crate::{
    error::{LlmusageError, Result},
    query::PRICING_UNPRICED,
};

/// Migration function signature. Each migration owns one SQLite transaction.
pub type MigrationFn = fn(&Transaction<'_>) -> Result<()>;

/// Progress event emitted around a schema migration step.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MigrationProgress {
    /// Schema version being applied.
    pub version: u32,
    /// Stable migration name from [`MIGRATIONS`].
    pub name: &'static str,
    /// Elapsed wall-clock time for finished events.
    pub elapsed_ms: Option<u64>,
}

/// Callback used by CLI/job callers to mirror migration progress into their UI
/// or NDJSON stream without coupling the store layer to a concrete transport.
pub type MigrationEventSink<'a> = &'a mut dyn FnMut(MigrationProgressEvent);

/// Start/finish envelope for a migration progress callback.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "phase")]
pub enum MigrationProgressEvent {
    /// A migration transaction has started.
    Started(MigrationProgress),
    /// A migration transaction has committed successfully.
    Finished(MigrationProgress),
}

/// Ordered schema migrations applied by [`Store::bootstrap`].
///
/// M0- intentionally contains only v1 baseline. Later sprint phases append real
/// schema/data migrations here; empty placeholder migrations are not allowed.
pub const MIGRATIONS: &[(u32, &str, MigrationFn)] = &[
    (1, "baseline", m_001_baseline),
    (2, "add_cache_split", m_002_add_cache_split),
    (3, "add_cost_breakdown", m_003_add_cost_breakdown),
    (4, "add_event_count_proj", m_004_add_event_count_proj),
    (5, "add_source_file", m_005_add_source_file),
    (6, "add_recent_history", m_006_add_recent_history),
    (7, "add_raw_archive", m_007_add_raw_archive),
    (8, "add_worker_lock_meta", m_008_add_worker_lock_meta),
    (9, "add_gemini_source", m_009_add_gemini_source),
    (10, "add_pricing_meta", m_010_add_pricing_meta),
    (11, "add_behavior_facts", m_011_add_behavior_facts),
];

/// Returns the newest schema version known to this binary.
pub fn latest_schema_version() -> u32 {
    MIGRATIONS
        .last()
        .map(|(version, _, _)| *version)
        .unwrap_or(0)
}

/// Reads `meta('schema_version')`, treating missing metadata as v0.
pub fn read_schema_version(conn: &Connection) -> Result<u32> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;
    let raw = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(raw
        .as_deref()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0))
}

/// Persists the current schema version inside an active migration transaction.
pub fn write_schema_version(tx: &Transaction<'_>, version: u32) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;
    tx.execute(
        r#"
        INSERT INTO meta(key, value)
        VALUES ('schema_version', ?1)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
        [version.to_string()],
    )?;
    Ok(())
}

/// Applies all pending migrations, optionally emitting start/finish callbacks.
pub fn run_migrations_with_events(
    conn: &mut Connection,
    mut sink: Option<MigrationEventSink<'_>>,
) -> Result<()> {
    let mut current = read_schema_version(conn)?;
    for (version, name, migration) in MIGRATIONS {
        if *version <= current {
            continue;
        }

        if let Some(callback) = sink.as_mut() {
            callback(MigrationProgressEvent::Started(MigrationProgress {
                version: *version,
                name,
                elapsed_ms: None,
            }));
        }
        info!(
            version = *version,
            name = *name,
            "开始执行 schema migration"
        );
        let started = Instant::now();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = (|| -> Result<()> {
            migration(&tx)?;
            write_schema_version(&tx, *version)?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                tx.commit()?;
                let elapsed_ms = elapsed_ms(started);
                if let Some(callback) = sink.as_mut() {
                    callback(MigrationProgressEvent::Finished(MigrationProgress {
                        version: *version,
                        name,
                        elapsed_ms: Some(elapsed_ms),
                    }));
                }
                info!(
                    version = *version,
                    name = *name,
                    elapsed_ms,
                    "完成 schema migration"
                );
                current = *version;
            }
            Err(source) => {
                return Err(LlmusageError::MigrationFailed {
                    version: *version,
                    name,
                    source: anyhow::Error::new(source),
                });
            }
        }
    }
    Ok(())
}

fn m_001_baseline(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS source_cursor (
            source TEXT NOT NULL,
            cursor_key TEXT NOT NULL,
            file_path TEXT,
            file_fingerprint TEXT,
            file_size INTEGER,
            file_mtime_ns INTEGER,
            tail_signature TEXT,
            inode INTEGER,
            offset INTEGER,
            last_total_json TEXT,
            last_model TEXT,
            last_time_created INTEGER,
            last_processed_ids_json TEXT,
            sqlite_status TEXT,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (source, cursor_key)
        );
        CREATE TABLE IF NOT EXISTS usage_event (
            event_key TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            model TEXT NOT NULL,
            event_at TEXT NOT NULL,
            hour_start TEXT NOT NULL,
            input_tokens INTEGER NOT NULL,
            cached_input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            reasoning_output_tokens INTEGER NOT NULL,
            total_tokens INTEGER NOT NULL,
            project_hash TEXT,
            project_label TEXT,
            project_ref TEXT,
            path_hash TEXT,
            session_id TEXT,
            session_label TEXT,
            source_path_hash TEXT,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS usage_bucket_30m (
            source TEXT NOT NULL,
            model TEXT NOT NULL,
            hour_start TEXT NOT NULL,
            project_hash TEXT NOT NULL DEFAULT '',
            project_label TEXT,
            project_ref TEXT,
            input_tokens INTEGER NOT NULL,
            cached_input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            reasoning_output_tokens INTEGER NOT NULL,
            total_tokens INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (source, model, hour_start, project_hash)
        );
        CREATE TABLE IF NOT EXISTS project_dim (
            project_hash TEXT PRIMARY KEY,
            project_label TEXT NOT NULL,
            project_ref TEXT,
            repo_root_hash TEXT NOT NULL,
            path_hash TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS integration_install (
            source TEXT PRIMARY KEY,
            install_type TEXT NOT NULL,
            status TEXT NOT NULL,
            config_path TEXT,
            backup_path TEXT,
            details_json TEXT,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS run_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            command TEXT NOT NULL,
            status TEXT NOT NULL,
            summary TEXT,
            error TEXT,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            duration_ms INTEGER
        );
        CREATE TABLE IF NOT EXISTS trigger_state (
            source TEXT PRIMARY KEY,
            last_signal_at TEXT NOT NULL,
            trigger TEXT NOT NULL,
            last_worker_started_at TEXT,
            last_worker_finished_at TEXT,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS worker_lock (
            lock_name TEXT PRIMARY KEY,
            owner_id TEXT NOT NULL,
            lease_expires_at TEXT NOT NULL,
            holder_pid INTEGER,
            holder_kind TEXT,
            acquired_at TEXT,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS source_sync_status (
            source TEXT PRIMARY KEY,
            files_processed INTEGER NOT NULL,
            changed_files INTEGER NOT NULL,
            bytes_scanned INTEGER NOT NULL,
            events_seen INTEGER NOT NULL,
            events_replayed INTEGER NOT NULL,
            events_inserted INTEGER NOT NULL,
            stored_events INTEGER NOT NULL DEFAULT 0,
            parse_ms INTEGER NOT NULL,
            write_ms INTEGER NOT NULL,
            lock_wait_ms INTEGER NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_usage_bucket_30m_hour_start
            ON usage_bucket_30m(hour_start);
        CREATE INDEX IF NOT EXISTS idx_usage_event_source_event_at
            ON usage_event(source, event_at);
        CREATE INDEX IF NOT EXISTS idx_usage_event_source_event_key
            ON usage_event(source, event_key);
        "#,
    )?;
    ensure_column(tx, "source_cursor", "file_fingerprint", "TEXT")?;
    ensure_column(tx, "source_cursor", "file_size", "INTEGER")?;
    ensure_column(tx, "source_cursor", "file_mtime_ns", "INTEGER")?;
    ensure_column(tx, "source_cursor", "tail_signature", "TEXT")?;
    ensure_column(tx, "usage_event", "session_id", "TEXT")?;
    ensure_column(tx, "usage_event", "session_label", "TEXT")?;
    ensure_column(tx, "usage_event", "source_path_hash", "TEXT")?;
    ensure_column(tx, "run_log", "duration_ms", "INTEGER")?;
    tx.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_usage_event_session
            ON usage_event(source, session_id, event_at);
        "#,
    )?;
    Ok(())
}

fn m_002_add_cache_split(tx: &Transaction<'_>) -> Result<()> {
    rename_column_if_exists(
        tx,
        "usage_event",
        "cached_input_tokens",
        "cache_read_tokens",
    )?;
    rename_column_if_exists(
        tx,
        "usage_bucket_30m",
        "cached_input_tokens",
        "cache_read_tokens",
    )?;
    ensure_column(
        tx,
        "usage_event",
        "cache_creation_tokens",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        tx,
        "usage_bucket_30m",
        "cache_creation_tokens",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    Ok(())
}

fn m_003_add_cost_breakdown(tx: &Transaction<'_>) -> Result<()> {
    for table in ["usage_event", "usage_bucket_30m"] {
        ensure_column(
            tx,
            table,
            "cost_with_cache_usd",
            "REAL NOT NULL DEFAULT 0.0",
        )?;
        ensure_column(
            tx,
            table,
            "cost_without_cache_usd",
            "REAL NOT NULL DEFAULT 0.0",
        )?;
        ensure_column(
            tx,
            table,
            "pricing_status",
            &format!("TEXT NOT NULL DEFAULT '{PRICING_UNPRICED}'"),
        )?;
        ensure_column(tx, table, "pricing_source", "TEXT")?;
        ensure_column(tx, table, "pricing_rate", "TEXT")?;
    }
    Ok(())
}

fn m_004_add_event_count_proj(tx: &Transaction<'_>) -> Result<()> {
    ensure_column(tx, "usage_event", "project_path", "TEXT")?;
    ensure_column(
        tx,
        "usage_bucket_30m",
        "event_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    tx.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_usage_event_source_path_hash
            ON usage_event(source, source_path_hash);
        CREATE TEMP TABLE IF NOT EXISTS temp.llmusage_event_count_backfill (
            source TEXT NOT NULL,
            model TEXT NOT NULL,
            hour_start TEXT NOT NULL,
            project_hash TEXT NOT NULL,
            event_count INTEGER NOT NULL,
            PRIMARY KEY (source, model, hour_start, project_hash)
        ) WITHOUT ROWID;
        DELETE FROM temp.llmusage_event_count_backfill;
        INSERT INTO temp.llmusage_event_count_backfill(
            source, model, hour_start, project_hash, event_count
        )
        SELECT
            source,
            model,
            hour_start,
            COALESCE(project_hash, '') AS project_hash,
            COUNT(*) AS event_count
        FROM usage_event
        GROUP BY source, model, hour_start, COALESCE(project_hash, '');
        UPDATE usage_bucket_30m
        SET event_count = COALESCE((
            SELECT counts.event_count
            FROM temp.llmusage_event_count_backfill counts
            WHERE counts.source = usage_bucket_30m.source
              AND counts.model = usage_bucket_30m.model
              AND counts.hour_start = usage_bucket_30m.hour_start
              AND counts.project_hash = usage_bucket_30m.project_hash
        ), 0)
        WHERE event_count = 0;
        DROP TABLE temp.llmusage_event_count_backfill;
        "#,
    )?;
    Ok(())
}

fn m_005_add_source_file(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS source_file (
            source TEXT NOT NULL,
            file_path TEXT NOT NULL,
            state TEXT NOT NULL,
            last_seen_at TEXT,
            last_state_change_at TEXT NOT NULL,
            PRIMARY KEY (source, file_path)
        );
        CREATE INDEX IF NOT EXISTS idx_source_file_source_state
            ON source_file(source, state);
        "#,
    )?;
    Ok(())
}

fn m_006_add_recent_history(tx: &Transaction<'_>) -> Result<()> {
    ensure_column(tx, "source_sync_status", "recent_completed_at", "TEXT")?;
    ensure_column(tx, "source_sync_status", "history_completed_at", "TEXT")?;
    ensure_column(
        tx,
        "source_sync_status",
        "stored_events",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    Ok(())
}

/// D11 / F1.5: opt-in raw archive surface.
///
/// Creates the `usage_event_raw` side table and seeds the
/// `meta('raw_archive_enabled', '0')` flag so [`Store::raw_archive_enabled`]
/// returns false on freshly bootstrapped databases. Existing rows in the meta
/// table are left untouched, which preserves the user's previous opt-in choice
/// across schema upgrades.
fn m_007_add_raw_archive(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS usage_event_raw (
            event_key TEXT PRIMARY KEY,
            raw_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        "#,
    )?;
    tx.execute(
        r#"
        INSERT INTO meta(key, value)
        VALUES ('raw_archive_enabled', '0')
        ON CONFLICT(key) DO NOTHING
        "#,
        [],
    )?;
    Ok(())
}

fn m_008_add_worker_lock_meta(tx: &Transaction<'_>) -> Result<()> {
    if table_exists(tx, "worker_lease")? && !table_exists(tx, "worker_lock")? {
        tx.execute_batch("ALTER TABLE worker_lease RENAME TO worker_lock;")?;
    }
    if !table_exists(tx, "worker_lock")? {
        tx.execute_batch(
            r#"
            CREATE TABLE worker_lock (
                lock_name TEXT PRIMARY KEY,
                owner_id TEXT NOT NULL,
                lease_expires_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )?;
    }
    ensure_column(tx, "worker_lock", "holder_pid", "INTEGER")?;
    ensure_column(tx, "worker_lock", "holder_kind", "TEXT")?;
    ensure_column(tx, "worker_lock", "acquired_at", "TEXT")?;
    if table_exists(tx, "worker_lease")? {
        tx.execute_batch(
            r#"
            INSERT OR IGNORE INTO worker_lock(lock_name, owner_id, lease_expires_at, updated_at)
            SELECT lock_name, owner_id, lease_expires_at, updated_at
            FROM worker_lease;
            DROP TABLE worker_lease;
            "#,
        )?;
    }
    tx.execute_batch(
        r#"
        UPDATE worker_lock
        SET holder_pid = COALESCE(holder_pid, 0),
            holder_kind = COALESCE(holder_kind, 'unknown'),
            acquired_at = COALESCE(acquired_at, updated_at)
        "#,
    )?;
    Ok(())
}

fn m_009_add_gemini_source(tx: &Transaction<'_>) -> Result<()> {
    tx.execute(
        r#"
        INSERT INTO meta(key, value)
        VALUES ('source.gemini.enabled', '1')
        ON CONFLICT(key) DO NOTHING
        "#,
        [],
    )?;
    Ok(())
}

fn m_010_add_pricing_meta(tx: &Transaction<'_>) -> Result<()> {
    tx.execute(
        r#"
        INSERT INTO meta(key, value)
        VALUES ('pricing_catalog_version', ?1)
        ON CONFLICT(key) DO NOTHING
        "#,
        [crate::query::pricing_catalog::PricingCatalog::static_v1()
            .version
            .as_str()],
    )?;
    Ok(())
}

fn m_011_add_behavior_facts(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS usage_turn (
            turn_key TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            session_id TEXT,
            source_path_hash TEXT,
            project_hash TEXT,
            primary_model TEXT NOT NULL,
            started_at TEXT NOT NULL,
            category TEXT NOT NULL,
            has_edits INTEGER NOT NULL DEFAULT 0,
            retries INTEGER NOT NULL DEFAULT 0,
            one_shot INTEGER NOT NULL DEFAULT 0,
            call_count INTEGER NOT NULL DEFAULT 0,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens INTEGER NOT NULL DEFAULT 0,
            cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
            total_tokens INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS usage_tool_call (
            tool_call_key TEXT PRIMARY KEY,
            turn_key TEXT,
            event_key TEXT,
            source TEXT NOT NULL,
            session_id TEXT,
            source_path_hash TEXT,
            project_hash TEXT,
            model TEXT,
            occurred_at TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            tool_kind TEXT NOT NULL,
            mcp_server TEXT,
            mcp_tool TEXT,
            input_fingerprint TEXT,
            safe_preview TEXT,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_usage_turn_source_started
            ON usage_turn(source, started_at);
        CREATE INDEX IF NOT EXISTS idx_usage_turn_category_started
            ON usage_turn(category, started_at);
        CREATE INDEX IF NOT EXISTS idx_usage_turn_model_started
            ON usage_turn(primary_model, started_at);
        CREATE INDEX IF NOT EXISTS idx_usage_turn_event_key_expr
            ON usage_turn(substr(turn_key, 6));
        CREATE INDEX IF NOT EXISTS idx_usage_tool_call_source_occurred
            ON usage_tool_call(source, occurred_at);
        CREATE INDEX IF NOT EXISTS idx_usage_tool_call_kind_name
            ON usage_tool_call(tool_kind, tool_name);
        CREATE INDEX IF NOT EXISTS idx_usage_tool_call_turn
            ON usage_tool_call(turn_key);
        "#,
    )?;
    Ok(())
}

fn ensure_column(tx: &Transaction<'_>, table: &str, column: &str, definition: &str) -> Result<()> {
    if table_has_column(tx, table, column)? {
        return Ok(());
    }

    tx.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn rename_column_if_exists(
    tx: &Transaction<'_>,
    table: &str,
    old_column: &str,
    new_column: &str,
) -> Result<()> {
    if table_has_column(tx, table, new_column)? {
        return Ok(());
    }
    if !table_has_column(tx, table, old_column)? {
        return Ok(());
    }
    tx.execute(
        &format!("ALTER TABLE {table} RENAME COLUMN {old_column} TO {new_column}"),
        [],
    )?;
    Ok(())
}

fn table_has_column(tx: &Transaction<'_>, table: &str, column: &str) -> Result<bool> {
    let mut stmt = tx.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(existing.iter().any(|item| item == column))
}

fn table_exists(tx: &Transaction<'_>, table: &str) -> Result<bool> {
    let exists = tx.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
        [table],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(exists > 0)
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

#[cfg(test)]
pub(crate) fn run_migrations_for_test(
    conn: &mut Connection,
    migrations: &[(u32, &'static str, MigrationFn)],
) -> Result<()> {
    run_migrations_for_test_with_events(conn, migrations, None)
}

#[cfg(test)]
pub(crate) fn run_migrations_for_test_with_events(
    conn: &mut Connection,
    migrations: &[(u32, &'static str, MigrationFn)],
    mut sink: Option<MigrationEventSink<'_>>,
) -> Result<()> {
    let mut current = read_schema_version(conn)?;
    for (version, name, migration) in migrations {
        if *version <= current {
            continue;
        }
        if let Some(callback) = sink.as_mut() {
            callback(MigrationProgressEvent::Started(MigrationProgress {
                version: *version,
                name,
                elapsed_ms: None,
            }));
        }
        let started = Instant::now();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = (|| -> Result<()> {
            migration(&tx)?;
            write_schema_version(&tx, *version)?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                tx.commit()?;
                if let Some(callback) = sink.as_mut() {
                    callback(MigrationProgressEvent::Finished(MigrationProgress {
                        version: *version,
                        name,
                        elapsed_ms: Some(elapsed_ms(started)),
                    }));
                }
                current = *version;
            }
            Err(source) => {
                return Err(LlmusageError::MigrationFailed {
                    version: *version,
                    name,
                    source: anyhow::Error::new(source),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pragma_columns(conn: &Connection, table: &str) -> anyhow::Result<Vec<String>> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn create_marker(tx: &Transaction<'_>) -> Result<()> {
        tx.execute_batch("CREATE TABLE marker(id INTEGER PRIMARY KEY);")?;
        Ok(())
    }

    fn fail_after_create(tx: &Transaction<'_>) -> Result<()> {
        tx.execute_batch("CREATE TABLE rolled_back(id INTEGER PRIMARY KEY);")?;
        Err(std::io::Error::other("forced migration failure").into())
    }

    #[test]
    fn migrations_run_in_order_v0_to_latest() -> anyhow::Result<()> {
        let mut conn = Connection::open_in_memory()?;
        run_migrations_for_test(&mut conn, &[(1, "marker", create_marker)])?;
        assert_eq!(read_schema_version(&conn)?, 1);
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='marker'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1);
        Ok(())
    }

    #[test]
    fn migration_idempotent_when_already_at_latest() -> anyhow::Result<()> {
        let mut conn = Connection::open_in_memory()?;
        run_migrations_for_test(&mut conn, &[(1, "marker", create_marker)])?;
        run_migrations_for_test(&mut conn, &[(1, "marker", create_marker)])?;
        assert_eq!(read_schema_version(&conn)?, 1);
        Ok(())
    }

    #[test]
    fn migration_failure_rolls_back_transaction() -> anyhow::Result<()> {
        let mut conn = Connection::open_in_memory()?;
        let err = run_migrations_for_test(&mut conn, &[(1, "boom", fail_after_create)])
            .expect_err("migration should fail");
        assert!(matches!(
            err,
            LlmusageError::MigrationFailed { version: 1, .. }
        ));
        assert_eq!(read_schema_version(&conn)?, 0);
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='rolled_back'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 0);
        Ok(())
    }

    /// Validates D8/F1.2: migration v2 renames `cached_input_tokens` to
    /// `cache_read_tokens` on both `usage_event` and `usage_bucket_30m`,
    /// adds a non-null `cache_creation_tokens` column with default 0, and
    /// preserves all values written by 0.4.x bootstrap.
    #[test]
    fn migration_v2_renames_cached_input_to_cache_read_preserving_data() -> anyhow::Result<()> {
        let mut conn = Connection::open_in_memory()?;
        run_migrations_for_test(&mut conn, &[(1, "baseline", m_001_baseline)])?;

        conn.execute(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cached_input_tokens, output_tokens,
                reasoning_output_tokens, total_tokens, created_at
            )
            VALUES ('k1', 'codex', 'gpt-5', '2026-05-01T00:00:00Z',
                    '2026-05-01T00:00:00Z', 10, 5, 8, 0, 23,
                    '2026-05-01T00:00:00Z')
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash,
                input_tokens, cached_input_tokens, output_tokens,
                reasoning_output_tokens, total_tokens, updated_at
            )
            VALUES ('codex', 'gpt-5', '2026-05-01T00:00:00Z', '',
                    10, 5, 8, 0, 23, '2026-05-01T00:00:00Z')
            "#,
            [],
        )?;

        run_migrations_for_test(&mut conn, &[(2, "add_cache_split", m_002_add_cache_split)])?;

        let (event_read, event_creation): (i64, i64) = conn.query_row(
            "SELECT cache_read_tokens, cache_creation_tokens FROM usage_event WHERE event_key='k1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(event_read, 5);
        assert_eq!(event_creation, 0);

        let (bucket_read, bucket_creation): (i64, i64) = conn.query_row(
            r#"
            SELECT cache_read_tokens, cache_creation_tokens FROM usage_bucket_30m
            WHERE source='codex' AND model='gpt-5' AND hour_start='2026-05-01T00:00:00Z'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(bucket_read, 5);
        assert_eq!(bucket_creation, 0);

        // 旧字面量 cached_input_tokens 已不复存在
        let legacy_present = conn
            .prepare("SELECT cached_input_tokens FROM usage_event LIMIT 1")
            .is_ok();
        assert!(
            !legacy_present,
            "cached_input_tokens 列应在 v2 之后被重命名"
        );
        Ok(())
    }

    #[test]
    fn migration_v4_backfills_event_count_with_aggregate_table() -> anyhow::Result<()> {
        let mut conn = Connection::open_in_memory()?;
        run_migrations_for_test(
            &mut conn,
            &[
                (1, "baseline", m_001_baseline),
                (2, "add_cache_split", m_002_add_cache_split),
                (3, "add_cost_breakdown", m_003_add_cost_breakdown),
            ],
        )?;

        conn.execute_batch(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                project_hash, created_at
            )
            VALUES
                ('codex:null:1', 'codex', 'gpt-5', '2026-05-01T00:01:00Z', '2026-05-01T00:00:00Z', 1, 0, 0, 1, 0, 2, NULL, '2026-05-01T00:01:00Z'),
                ('codex:null:2', 'codex', 'gpt-5', '2026-05-01T00:02:00Z', '2026-05-01T00:00:00Z', 1, 0, 0, 1, 0, 2, NULL, '2026-05-01T00:02:00Z'),
                ('codex:project:1', 'codex', 'gpt-5', '2026-05-01T00:03:00Z', '2026-05-01T00:00:00Z', 1, 0, 0, 1, 0, 2, 'project-a', '2026-05-01T00:03:00Z'),
                ('claude:project:1', 'claude', 'claude-3-5', '2026-05-01T00:04:00Z', '2026-05-01T00:00:00Z', 1, 0, 0, 1, 0, 2, 'project-a', '2026-05-01T00:04:00Z');

            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                updated_at
            )
            VALUES
                ('codex', 'gpt-5', '2026-05-01T00:00:00Z', '', 2, 0, 0, 2, 0, 4, '2026-05-01T00:00:00Z'),
                ('codex', 'gpt-5', '2026-05-01T00:00:00Z', 'project-a', 1, 0, 0, 1, 0, 2, '2026-05-01T00:00:00Z'),
                ('claude', 'claude-3-5', '2026-05-01T00:00:00Z', 'project-a', 1, 0, 0, 1, 0, 2, '2026-05-01T00:00:00Z'),
                ('codex', 'gpt-5', '2026-05-01T00:30:00Z', '', 0, 0, 0, 0, 0, 0, '2026-05-01T00:30:00Z');
            "#,
        )?;

        let mut events = Vec::new();
        let mut sink = |event: MigrationProgressEvent| events.push(event);
        run_migrations_for_test_with_events(
            &mut conn,
            &[(4, "add_event_count_proj", m_004_add_event_count_proj)],
            Some(&mut sink),
        )?;

        let count_for =
            |conn: &Connection, source: &str, model: &str, hour: &str, project: &str| {
                conn.query_row(
                    r#"
                SELECT event_count
                FROM usage_bucket_30m
                WHERE source = ?1 AND model = ?2 AND hour_start = ?3 AND project_hash = ?4
                "#,
                    rusqlite::params![source, model, hour, project],
                    |row| row.get::<_, i64>(0),
                )
            };
        assert_eq!(
            count_for(&conn, "codex", "gpt-5", "2026-05-01T00:00:00Z", "")?,
            2,
            "NULL event project_hash should match bucket default empty string"
        );
        assert_eq!(
            count_for(&conn, "codex", "gpt-5", "2026-05-01T00:00:00Z", "project-a")?,
            1
        );
        assert_eq!(
            count_for(
                &conn,
                "claude",
                "claude-3-5",
                "2026-05-01T00:00:00Z",
                "project-a"
            )?,
            1
        );
        assert_eq!(
            count_for(&conn, "codex", "gpt-5", "2026-05-01T00:30:00Z", "")?,
            0,
            "buckets without matching events stay at zero"
        );
        assert_eq!(read_schema_version(&conn)?, 4);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events.first(),
            Some(MigrationProgressEvent::Started(MigrationProgress {
                version: 4,
                name: "add_event_count_proj",
                elapsed_ms: None
            }))
        ));
        assert!(matches!(
            events.last(),
            Some(MigrationProgressEvent::Finished(MigrationProgress {
                version: 4,
                name: "add_event_count_proj",
                elapsed_ms: Some(_)
            }))
        ));
        Ok(())
    }

    #[test]
    fn migration_v4_sql_avoids_correlated_usage_event_count_shape() {
        let source = include_str!("migrations.rs");
        assert!(
            source.contains("temp.llmusage_event_count_backfill"),
            "v4 should aggregate event counts once before bucket update"
        );
        assert!(
            !source.contains("SELECT COUNT(*)\n            FROM usage_event e"),
            "v4 must not reintroduce the old per-bucket correlated COUNT(*)"
        );
    }

    #[test]
    fn migration_v10_seeds_pricing_catalog_version() -> anyhow::Result<()> {
        let mut conn = Connection::open_in_memory()?;
        run_migrations_for_test(
            &mut conn,
            &[(10, "add_pricing_meta", m_010_add_pricing_meta)],
        )?;

        let value: String = conn.query_row(
            "SELECT value FROM meta WHERE key = 'pricing_catalog_version'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            value,
            crate::query::pricing_catalog::PricingCatalog::static_v1().version
        );
        Ok(())
    }

    #[test]
    fn migration_v11_creates_behavior_fact_tables() -> anyhow::Result<()> {
        let mut conn = Connection::open_in_memory()?;
        run_migrations_for_test(
            &mut conn,
            &[(11, "add_behavior_facts", m_011_add_behavior_facts)],
        )?;

        for table in ["usage_turn", "usage_tool_call"] {
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )?;
            assert_eq!(exists, 1, "{table} should be created");
        }

        let turn_columns = pragma_columns(&conn, "usage_turn")?;
        assert!(turn_columns.contains(&"category".to_string()));
        assert!(turn_columns.contains(&"one_shot".to_string()));
        let turn_event_expr_index: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_usage_turn_event_key_expr'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(turn_event_expr_index, 1);
        let columns = pragma_columns(&conn, "usage_tool_call")?;
        assert!(columns.contains(&"tool_kind".to_string()));
        assert!(columns.contains(&"safe_preview".to_string()));
        assert_eq!(read_schema_version(&conn)?, 11);
        Ok(())
    }
}
