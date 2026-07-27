use crate::error::Result;
use rusqlite::params;

use super::Store;
use crate::{models::SourceKind, util::now_utc};

/// Borrowed view onto the `trigger_state` surface of [`Store`].
///
/// 通过 `store.triggers()` 创建。
pub struct TriggerStore<'a> {
    store: &'a Store,
}

impl<'a> TriggerStore<'a> {
    pub(super) fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub fn upsert_trigger_state(
        &self,
        source: SourceKind,
        trigger: &str,
        signal_at: &str,
    ) -> Result<()> {
        let now = now_utc();
        self.store.control_plane_transaction(|tx| {
            tx.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS trigger_state (
                    source TEXT PRIMARY KEY,
                    last_signal_at TEXT NOT NULL,
                    trigger TEXT NOT NULL,
                    last_worker_started_at TEXT,
                    last_worker_finished_at TEXT,
                    updated_at TEXT NOT NULL
                );
                "#,
            )?;
            tx.execute(
                r#"
            INSERT INTO trigger_state(source, last_signal_at, trigger, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(source) DO UPDATE SET
                last_signal_at = excluded.last_signal_at,
                trigger = excluded.trigger,
                updated_at = excluded.updated_at
            "#,
                params![source.as_str(), signal_at, trigger, now],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn mark_trigger_worker_started(&self, source: SourceKind, started_at: &str) -> Result<()> {
        self.store.write_transaction(|tx| {
            tx.execute(
                r#"
            UPDATE trigger_state
            SET last_worker_started_at = ?2, updated_at = ?2
            WHERE source = ?1
            "#,
                params![source.as_str(), started_at],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn mark_trigger_worker_finished(
        &self,
        source: SourceKind,
        finished_at: &str,
    ) -> Result<()> {
        self.store.write_transaction(|tx| {
            tx.execute(
                r#"
            UPDATE trigger_state
            SET last_worker_finished_at = ?2, updated_at = ?2
            WHERE source = ?1
            "#,
                params![source.as_str(), finished_at],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn trigger_snapshot(&self) -> Result<Vec<(String, String)>> {
        let conn = self.store.open_connection()?;
        let mut stmt =
            conn.prepare("SELECT source, last_signal_at FROM trigger_state ORDER BY source ASC")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
