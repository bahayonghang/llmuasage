use anyhow::Result;
use rusqlite::params;

use super::{SourceSyncStatus, Store};

/// Borrowed view onto the `source_sync_status` surface of [`Store`].
///
/// 通过 `store.sync_status()` 创建。
pub struct SyncStatusStore<'a> {
    store: &'a Store,
}

impl<'a> SyncStatusStore<'a> {
    pub(super) fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub fn load_source_sync_statuses(&self) -> Result<Vec<SourceSyncStatus>> {
        let conn = self.store.open_connection()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
                source,
                files_processed,
                changed_files,
                bytes_scanned,
                events_seen,
                events_replayed,
                events_inserted,
                parse_ms,
                write_ms,
                lock_wait_ms,
                updated_at
            FROM source_sync_status
            ORDER BY source ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SourceSyncStatus {
                source: row.get(0)?,
                files_processed: row.get(1)?,
                changed_files: row.get(2)?,
                bytes_scanned: row.get(3)?,
                events_seen: row.get(4)?,
                events_replayed: row.get(5)?,
                events_inserted: row.get(6)?,
                parse_ms: row.get(7)?,
                write_ms: row.get(8)?,
                lock_wait_ms: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn save_source_sync_statuses(&self, statuses: &[SourceSyncStatus]) -> Result<()> {
        if statuses.is_empty() {
            return Ok(());
        }

        let mut conn = self.store.open_connection()?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT INTO source_sync_status(
                    source,
                    files_processed,
                    changed_files,
                    bytes_scanned,
                    events_seen,
                    events_replayed,
                    events_inserted,
                    parse_ms,
                    write_ms,
                    lock_wait_ms,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(source) DO UPDATE SET
                    files_processed = excluded.files_processed,
                    changed_files = excluded.changed_files,
                    bytes_scanned = excluded.bytes_scanned,
                    events_seen = excluded.events_seen,
                    events_replayed = excluded.events_replayed,
                    events_inserted = excluded.events_inserted,
                    parse_ms = excluded.parse_ms,
                    write_ms = excluded.write_ms,
                    lock_wait_ms = excluded.lock_wait_ms,
                    updated_at = excluded.updated_at
                "#,
            )?;
            for status in statuses {
                stmt.execute(params![
                    status.source,
                    status.files_processed,
                    status.changed_files,
                    status.bytes_scanned,
                    status.events_seen,
                    status.events_replayed,
                    status.events_inserted,
                    status.parse_ms,
                    status.write_ms,
                    status.lock_wait_ms,
                    status.updated_at,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
