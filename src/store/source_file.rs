//! `source_file` state machine: `live` / `missing` / `deleted_by_user`.
//!
//! Records every candidate file llmusage has seen for file-backed sources
//! (codex / claude). Streaming sources without a per-file identity (opencode)
//! do not participate.
//!
//! Three states (D15, ADR 0006):
//!
//! - `live` — the file was seen in the most recent sync run.
//! - `missing` — previously live, but not seen in the latest run; ccr-ui
//!   surfaces this as "may have been moved/deleted on disk".
//! - `deleted_by_user` — the user explicitly forgot the file via
//!   `diagnostics --forget-file` / `POST /api/diagnostics/forget`.
//!
//! Transitions:
//!
//! - (no row)         + observed   → live
//! - live             + not seen   → missing
//! - missing          + observed   → live (resurrect)
//! - any              + user mark  → deleted_by_user (cursor row also dropped)
//! - deleted_by_user  + observed   → live (resurrect)

use rusqlite::{Connection, Transaction, params};
use serde::Serialize;

use crate::{error::Result, models::SourceKind, util::now_utc};

use super::Store;

/// State string stored in `source_file.state` for files seen this run.
pub const STATE_LIVE: &str = "live";
/// State string for files previously seen but not seen this run.
pub const STATE_MISSING: &str = "missing";
/// State string for files the user explicitly removed via diagnostics.
pub const STATE_DELETED: &str = "deleted_by_user";

/// Per-source counts grouped by `source_file.state`.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct SourceFileStateCounts {
    /// Number of rows currently in `live` state.
    pub live: u64,
    /// Number of rows currently in `missing` state.
    pub missing: u64,
    /// Number of rows currently in `deleted_by_user` state.
    pub deleted: u64,
}

/// Borrowed view onto the `source_file` surface of [`Store`].
///
/// Created via [`Store::source_files`]; re-uses the parent `&Store` without
/// cloning (matches the other XxxStore facades).
pub struct SourceFileStore<'a> {
    store: &'a Store,
}

impl<'a> SourceFileStore<'a> {
    pub(super) fn new(store: &'a Store) -> Self {
        Self { store }
    }

    /// Returns the count of rows in each state for one source.
    pub fn counts(&self, source: SourceKind) -> Result<SourceFileStateCounts> {
        let conn = self.store.open_connection()?;
        counts_with_conn(&conn, source.as_str())
    }

    /// Promotes any `live` rows for this source whose `last_seen_at` is older
    /// than `run_started_at` to `missing`.
    ///
    /// Called once per parser by the sync driver after the parser's last
    /// `commit_shard` returns. Returns the number of rows transitioned.
    pub fn sweep_missing(&self, source: SourceKind, run_started_at: &str) -> Result<usize> {
        let conn = self.store.open_connection()?;
        update_missing_with_conn(&conn, source.as_str(), run_started_at)
    }
}

/// Counts source_file rows by state on a caller-supplied connection.
///
/// Lets `Dashboard::diagnostics` read live/missing/deleted totals on the same
/// connection that already holds the WAL read lock.
pub(crate) fn counts_with_conn(conn: &Connection, source: &str) -> Result<SourceFileStateCounts> {
    let mut stmt = conn.prepare(
        r#"
        SELECT state, COUNT(*)
        FROM source_file
        WHERE source = ?1
        GROUP BY state
        "#,
    )?;
    let mut counts = SourceFileStateCounts::default();
    let mapped = stmt.query_map([source], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in mapped {
        let (state, count) = row?;
        let count = count.max(0) as u64;
        match state.as_str() {
            STATE_LIVE => counts.live = count,
            STATE_MISSING => counts.missing = count,
            STATE_DELETED => counts.deleted = count,
            _ => {}
        }
    }
    Ok(counts)
}

/// Upserts `source_file` rows to the `live` state inside the given transaction.
///
/// Used by `commit_shard` so the source_file write lands in the same atomic
/// transaction as events / cursors. Resurrects `missing` and `deleted_by_user`
/// rows when the user-deleted file reappears on disk (D15 resurrect rule).
///
/// `seen_at` is the run start timestamp; `update_missing_with_conn` later
/// uses it to decide which old `live` rows fall back to `missing`.
pub(crate) fn upsert_live_in_tx(
    tx: &Transaction<'_>,
    source: &str,
    file_paths: &[String],
    seen_at: &str,
) -> Result<()> {
    if file_paths.is_empty() {
        return Ok(());
    }
    let mut stmt = tx.prepare_cached(
        r#"
        INSERT INTO source_file(
            source, file_path, state, last_seen_at, last_state_change_at
        )
        VALUES (?1, ?2, 'live', ?3, ?3)
        ON CONFLICT(source, file_path) DO UPDATE SET
            state = 'live',
            last_seen_at = excluded.last_seen_at,
            last_state_change_at = CASE
                WHEN source_file.state = 'live'
                    THEN source_file.last_state_change_at
                ELSE excluded.last_state_change_at
            END
        "#,
    )?;
    for path in file_paths {
        stmt.execute(params![source, path, seen_at])?;
    }
    Ok(())
}

/// Promotes stale `live` rows to `missing` after a sync run completes.
///
/// A row is stale when its `last_seen_at` is strictly older than the run's
/// start timestamp (or NULL — only possible for legacy data that predates the
/// state machine). Returns the number of rows transitioned, mostly for tests.
pub(crate) fn update_missing_with_conn(
    conn: &Connection,
    source: &str,
    run_started_at: &str,
) -> Result<usize> {
    let updated = conn.execute(
        r#"
        UPDATE source_file
        SET state = 'missing',
            last_state_change_at = ?3
        WHERE source = ?1
          AND state = 'live'
          AND (last_seen_at IS NULL OR last_seen_at < ?2)
        "#,
        params![source, run_started_at, now_utc()],
    )?;
    Ok(updated)
}

/// Removes all `source_file` rows belonging to one source. Used by
/// `Store::reset_for_source` (Phase 4.5) and not for general cleanup.
#[allow(dead_code)]
pub(crate) fn delete_for_source_in_tx(tx: &Transaction<'_>, source: &str) -> Result<()> {
    tx.execute("DELETE FROM source_file WHERE source = ?1", [source])?;
    Ok(())
}

impl Store {
    /// Borrowed view onto the `source_file` surface.
    pub fn source_files(&self) -> SourceFileStore<'_> {
        SourceFileStore::new(self)
    }

    /// Marks one file as `deleted_by_user` and removes its `source_cursor`
    /// row in a single transaction (D15 user-forget entry).
    ///
    /// The next sync run will resurrect it back to `live` if the file is
    /// still present on disk; that's intentional — "forget" is not "ban".
    pub fn mark_source_file_deleted(&self, source: SourceKind, file_path: &str) -> Result<()> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction()?;
        let now = now_utc();
        tx.execute(
            r#"
            INSERT INTO source_file(
                source, file_path, state, last_seen_at, last_state_change_at
            )
            VALUES (?1, ?2, 'deleted_by_user', NULL, ?3)
            ON CONFLICT(source, file_path) DO UPDATE SET
                state = 'deleted_by_user',
                last_state_change_at = excluded.last_state_change_at
            "#,
            params![source.as_str(), file_path, now],
        )?;
        tx.execute(
            "DELETE FROM source_cursor WHERE source = ?1 AND file_path = ?2",
            params![source.as_str(), file_path],
        )?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::SourceKind, paths::AppPaths};
    use tempfile::TempDir;

    fn make_store() -> anyhow::Result<(TempDir, Store)> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        Ok((temp, store))
    }

    fn upsert_live(store: &Store, source: &str, paths: &[&str], seen_at: &str) -> Result<()> {
        let mut conn = store.open_connection()?;
        let tx = conn.transaction()?;
        let owned: Vec<String> = paths.iter().map(|p| (*p).to_string()).collect();
        upsert_live_in_tx(&tx, source, &owned, seen_at)?;
        tx.commit()?;
        Ok(())
    }

    /// Transition 1: (no row) + observed → live.
    #[test]
    fn unseen_to_live_on_first_observation() -> anyhow::Result<()> {
        let (_tmp, store) = make_store()?;
        upsert_live(&store, "codex", &["/x.jsonl"], "2026-05-08T00:00:00Z")?;
        let counts = store.source_files().counts(SourceKind::Codex)?;
        assert_eq!(
            counts,
            SourceFileStateCounts {
                live: 1,
                missing: 0,
                deleted: 0,
            }
        );
        Ok(())
    }

    /// Transition 2: live + not seen this run → missing.
    #[test]
    fn live_becomes_missing_when_not_seen() -> anyhow::Result<()> {
        let (_tmp, store) = make_store()?;
        upsert_live(&store, "codex", &["/a.jsonl"], "2026-05-08T00:00:00Z")?;

        let conn = store.open_connection()?;
        let updated = update_missing_with_conn(&conn, "codex", "2026-05-08T01:00:00Z")?;
        assert_eq!(updated, 1, "the stale live row should transition");

        let counts = store.source_files().counts(SourceKind::Codex)?;
        assert_eq!(counts.live, 0);
        assert_eq!(counts.missing, 1);
        Ok(())
    }

    /// Transition 3: missing + observed again → live (resurrect).
    #[test]
    fn missing_resurrects_when_seen_again() -> anyhow::Result<()> {
        let (_tmp, store) = make_store()?;
        upsert_live(&store, "codex", &["/a.jsonl"], "2026-05-08T00:00:00Z")?;
        {
            let conn = store.open_connection()?;
            update_missing_with_conn(&conn, "codex", "2026-05-08T01:00:00Z")?;
        }
        let counts = store.source_files().counts(SourceKind::Codex)?;
        assert_eq!(counts.missing, 1);

        upsert_live(&store, "codex", &["/a.jsonl"], "2026-05-08T02:00:00Z")?;
        let counts = store.source_files().counts(SourceKind::Codex)?;
        assert_eq!(counts.live, 1);
        assert_eq!(counts.missing, 0);
        Ok(())
    }

    /// Transition 4: live + user mark → deleted_by_user (cursor also dropped).
    #[test]
    fn live_becomes_deleted_when_user_marks_it() -> anyhow::Result<()> {
        let (_tmp, store) = make_store()?;
        upsert_live(&store, "codex", &["/a.jsonl"], "2026-05-08T00:00:00Z")?;

        // Seed a matching cursor row to validate the join-deletion behavior.
        {
            let conn = store.open_connection()?;
            conn.execute(
                r#"
                INSERT INTO source_cursor(
                    source, cursor_key, file_path, updated_at
                ) VALUES ('codex', 'cursor:/a.jsonl', '/a.jsonl', '2026-05-08T00:00:00Z')
                "#,
                [],
            )?;
        }

        store.mark_source_file_deleted(SourceKind::Codex, "/a.jsonl")?;
        let counts = store.source_files().counts(SourceKind::Codex)?;
        assert_eq!(counts.live, 0);
        assert_eq!(counts.deleted, 1);

        let conn = store.open_connection()?;
        let cursor_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM source_cursor WHERE source = 'codex' AND file_path = '/a.jsonl'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(cursor_count, 0, "cursor row should be cleared atomically");
        Ok(())
    }

    /// Transition 5: deleted_by_user + observed again → live (resurrect).
    /// Forget is not ban; if the file reappears on disk it is processed again.
    #[test]
    fn deleted_resurrects_when_seen_again() -> anyhow::Result<()> {
        let (_tmp, store) = make_store()?;
        store.mark_source_file_deleted(SourceKind::Codex, "/a.jsonl")?;
        let counts = store.source_files().counts(SourceKind::Codex)?;
        assert_eq!(counts.deleted, 1);

        upsert_live(&store, "codex", &["/a.jsonl"], "2026-05-08T00:00:00Z")?;
        let counts = store.source_files().counts(SourceKind::Codex)?;
        assert_eq!(counts.live, 1);
        assert_eq!(counts.deleted, 0);
        Ok(())
    }
}
