use crate::error::Result;
use rusqlite::{Connection, Row, params, params_from_iter, types::Value as SqlValue};

use super::{RunRecord, Store};
use crate::util::now_utc;

/// Commands that import usage into SQLite. Command-center last-run and
/// failure headlines read this family, not mixed `serve` rows.
pub(crate) const USAGE_IMPORT_COMMANDS: [&str; 3] = ["sync", "sync --rebuild", "hook-run"];

fn map_run_record(row: &Row<'_>) -> rusqlite::Result<RunRecord> {
    Ok(RunRecord {
        id: row.get(0)?,
        command: row.get(1)?,
        status: row.get(2)?,
        summary: row.get(3)?,
        error: row.get(4)?,
        started_at: row.get(5)?,
        finished_at: row.get(6)?,
    })
}

/// Borrowed view onto the `run_log` surface of [`Store`].
///
/// 通过 `store.run_log()` 创建。
pub struct RunLog<'a> {
    store: &'a Store,
}

impl<'a> RunLog<'a> {
    pub(super) fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub fn record_run_start(&self, command: &str) -> Result<i64> {
        let now = now_utc();
        self.store.write_transaction(|tx| {
            tx.execute(
                "INSERT INTO run_log(command, status, started_at) VALUES (?1, 'running', ?2)",
                params![command, now],
            )?;
            Ok(tx.last_insert_rowid())
        })
    }

    pub fn finish_run(
        &self,
        id: i64,
        status: &str,
        summary: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let now = now_utc();
        self.store.write_transaction(|tx| {
            tx.execute(
                r#"
            UPDATE run_log
            SET status = ?2,
                summary = ?3,
                error = ?4,
                finished_at = ?5,
                duration_ms = CAST((julianday(?5) - julianday(started_at)) * 86400000 AS INTEGER)
            WHERE id = ?1
            "#,
                params![id, status, summary, error, now],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn recover_running_runs(&self, commands: &[&str]) -> Result<usize> {
        if commands.is_empty() {
            return Ok(0);
        }

        let now = now_utc();
        let mut params = vec![
            SqlValue::Text("aborted".to_string()),
            SqlValue::Text("recovered stale running record".to_string()),
            SqlValue::Text(now.clone()),
            SqlValue::Text(now.clone()),
        ];
        let placeholders = commands
            .iter()
            .enumerate()
            .map(|(idx, _)| format!("?{}", idx + 5))
            .collect::<Vec<_>>()
            .join(", ");
        for command in commands {
            params.push(SqlValue::Text((*command).to_string()));
        }

        let sql = format!(
            r#"
            UPDATE run_log
            SET status = ?1,
                error = COALESCE(error, ?2),
                finished_at = ?3,
                duration_ms = CAST((julianday(?4) - julianday(started_at)) * 86400000 AS INTEGER)
            WHERE status = 'running'
              AND command IN ({placeholders})
            "#
        );
        self.store
            .write_transaction(|tx| Ok(tx.execute(&sql, params_from_iter(params))?))
    }

    pub fn recent_runs(&self, limit: usize) -> Result<Vec<RunRecord>> {
        let conn = self.store.open_connection()?;
        self.recent_runs_with_conn(&conn, limit)
    }

    pub(crate) fn recent_runs_with_conn(
        &self,
        conn: &Connection,
        limit: usize,
    ) -> Result<Vec<RunRecord>> {
        let mut stmt = conn.prepare(
            r#"
            SELECT id, command, status, summary, error, started_at, finished_at
            FROM run_log
            ORDER BY id DESC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit as i64], map_run_record)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub(crate) fn recent_usage_import_runs_with_conn(
        &self,
        conn: &Connection,
        limit: usize,
    ) -> Result<Vec<RunRecord>> {
        let placeholders = USAGE_IMPORT_COMMANDS
            .iter()
            .enumerate()
            .map(|(idx, _)| format!("?{}", idx + 2))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            r#"
            SELECT id, command, status, summary, error, started_at, finished_at
            FROM run_log
            WHERE command IN ({placeholders})
            ORDER BY id DESC
            LIMIT ?1
            "#
        );
        let mut params = vec![SqlValue::Integer(limit as i64)];
        for command in USAGE_IMPORT_COMMANDS {
            params.push(SqlValue::Text(command.to_string()));
        }
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params), map_run_record)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
