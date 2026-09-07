use crate::error::{LlmusageError, Result};
use rusqlite::{params, types::Type};

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

    /// Warning copy for a skipped legacy parser source. Ordinary sync and serve
    /// keep history; repair is explicit `sync --rebuild --source <source>`.
    pub fn legacy_repair_warning(source: crate::models::SourceKind) -> String {
        format!(
            "legacy token accounting; existing history was kept and this source was skipped for this round. Run `llmusage sync --rebuild --source {}` to repair. If source files are missing, restore them or add `--allow-lossy-rebuild` to that rebuild command to explicitly accept clearing unrebuildable history.",
            source.as_str()
        )
    }

    pub fn load_source_sync_statuses(&self, host_id: &str) -> Result<Vec<SourceSyncStatus>> {
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
                stored_events,
                parse_ms,
                write_ms,
                lock_wait_ms,
                parse_issues_json,
                updated_at
            FROM source_sync_status
            WHERE host_id = ?1
            ORDER BY source ASC
            "#,
        )?;
        let rows = stmt.query_map([host_id], |row| {
            let parse_issues_raw = row.get::<_, String>(11)?;
            let parse_issues = serde_json::from_str(&parse_issues_raw).map_err(|source| {
                rusqlite::Error::FromSqlConversionFailure(11, Type::Text, Box::new(source))
            })?;
            Ok(SourceSyncStatus {
                source: row.get(0)?,
                files_processed: row.get(1)?,
                changed_files: row.get(2)?,
                bytes_scanned: row.get(3)?,
                events_seen: row.get(4)?,
                events_replayed: row.get(5)?,
                events_inserted: row.get(6)?,
                stored_events: row.get(7)?,
                token_accounting_version: None,
                legacy_token_accounting: false,
                token_accounting_warning: None,
                parse_ms: row.get(8)?,
                write_ms: row.get(9)?,
                lock_wait_ms: row.get(10)?,
                parse_issues,
                updated_at: row.get(12)?,
            })
        })?;
        let mut statuses = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        for status in &mut statuses {
            let Some(source) = crate::models::SourceKind::parse_id(&status.source) else {
                continue;
            };
            if !crate::registry::source_descriptor(source)
                .is_some_and(|descriptor| descriptor.capabilities.parser)
            {
                continue;
            }
            if host_id == crate::store::LOCAL_HOST_ID {
                status.token_accounting_version = self.store.token_accounting_version(source)?;
                status.legacy_token_accounting = self.store.has_legacy_token_accounting(source)?;
                if status.legacy_token_accounting {
                    status.token_accounting_warning = Some(Self::legacy_repair_warning(source));
                }
                continue;
            }
            status.token_accounting_version = self
                .store
                .token_accounting_version_for_host(host_id, source)?;
            let expected = crate::store::expected_token_accounting_version(source);
            match status.token_accounting_version {
                Some(version) if version == expected => {
                    status.legacy_token_accounting = false;
                }
                Some(_) => {
                    status.legacy_token_accounting = true;
                    status.token_accounting_warning = Some(format!(
                        "remote host {host_id} source {} token accounting is not current; a full restore is required",
                        source.as_str()
                    ));
                }
                None => {
                    status.legacy_token_accounting = false;
                }
            }
        }
        Ok(statuses)
    }

    pub fn save_source_sync_statuses(
        &self,
        host_id: &str,
        statuses: &[SourceSyncStatus],
    ) -> Result<()> {
        if statuses.is_empty() {
            return Ok(());
        }

        self.store.write_transaction(|tx| {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT INTO source_sync_status(
                    host_id,
                    source,
                    files_processed,
                    changed_files,
                    bytes_scanned,
                    events_seen,
                    events_replayed,
                    events_inserted,
                    stored_events,
                    parse_ms,
                    write_ms,
                    lock_wait_ms,
                    parse_issues_json,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                ON CONFLICT(host_id, source) DO UPDATE SET
                    files_processed = excluded.files_processed,
                    changed_files = excluded.changed_files,
                    bytes_scanned = excluded.bytes_scanned,
                    events_seen = excluded.events_seen,
                    events_replayed = excluded.events_replayed,
                    events_inserted = excluded.events_inserted,
                    stored_events = excluded.stored_events,
                    parse_ms = excluded.parse_ms,
                    write_ms = excluded.write_ms,
                    lock_wait_ms = excluded.lock_wait_ms,
                    parse_issues_json = excluded.parse_issues_json,
                    updated_at = excluded.updated_at
                "#,
            )?;
            for status in statuses {
                let parse_issues_json =
                    serde_json::to_string(&status.parse_issues).map_err(|source| {
                        LlmusageError::Parse {
                            context: "source sync parse issues",
                            source,
                        }
                    })?;
                stmt.execute(params![
                    host_id,
                    status.source,
                    status.files_processed,
                    status.changed_files,
                    status.bytes_scanned,
                    status.events_seen,
                    status.events_replayed,
                    status.events_inserted,
                    status.stored_events,
                    status.parse_ms,
                    status.write_ms,
                    status.lock_wait_ms,
                    parse_issues_json,
                    status.updated_at,
                ])?;
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Marks the source's recent-window scan as completed (D27 / F6).
    ///
    /// This is intentionally separate from the per-run stats upsert so a
    /// `RecentReady` signal can update `recent_completed_at` immediately after
    /// one source finishes, before the whole sync job is done.
    pub fn mark_recent_completed(
        &self,
        source: crate::models::SourceKind,
        host_id: &str,
        at: String,
    ) -> Result<()> {
        self.store.write_transaction(|tx| {
            tx.execute(
                r#"
            INSERT INTO source_sync_status(
                host_id,
                source,
                files_processed,
                changed_files,
                bytes_scanned,
                events_seen,
                events_replayed,
                events_inserted,
                stored_events,
                parse_ms,
                write_ms,
                lock_wait_ms,
                updated_at,
                recent_completed_at
            ) VALUES (?1, ?2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, ?3, ?3)
            ON CONFLICT(host_id, source) DO UPDATE SET
                recent_completed_at = excluded.recent_completed_at,
                updated_at = excluded.updated_at
            "#,
                params![host_id, source.as_str(), at],
            )?;
            Ok(())
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        models::{ParseIssueKind, ParseIssueSample, ParseIssues, SourceKind},
        paths::AppPaths,
    };
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn parse_issue_diagnostics_round_trip_and_reject_invalid_json() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        let status = SourceSyncStatus {
            source: SourceKind::Codex.as_str().to_string(),
            files_processed: 1,
            changed_files: 1,
            bytes_scanned: 10,
            events_seen: 0,
            events_replayed: 0,
            events_inserted: 0,
            stored_events: 0,
            token_accounting_version: None,
            legacy_token_accounting: false,
            token_accounting_warning: None,
            parse_ms: 1,
            write_ms: 0,
            lock_wait_ms: 0,
            parse_issues: ParseIssues {
                malformed_lines: 1,
                oversized_lines: 2,
                samples: vec![ParseIssueSample {
                    source: SourceKind::Codex,
                    path_hash: "safe-path-hash".to_string(),
                    offset: 7,
                    kind: ParseIssueKind::Malformed,
                    reason: String::new(),
                }],
                ..ParseIssues::default()
            },
            updated_at: crate::util::now_utc(),
        };

        store
            .sync_status()
            .save_source_sync_statuses("local", std::slice::from_ref(&status))?;
        let loaded = store.sync_status().load_source_sync_statuses("local")?;
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].parse_issues, status.parse_issues);
        let encoded = serde_json::to_string(&loaded)?;
        assert!(encoded.contains("safe-path-hash"));
        assert!(!encoded.contains("prompt"));

        let conn = store.open_connection()?;
        conn.execute(
            "UPDATE source_sync_status SET parse_issues_json = 'not-json' WHERE source = 'codex'",
            [],
        )?;
        let error = store
            .sync_status()
            .load_source_sync_statuses("local")
            .expect_err("invalid issue JSON must not be treated as clean counters");
        assert!(matches!(
            error,
            LlmusageError::Db(rusqlite::Error::FromSqlConversionFailure(11, Type::Text, _))
        ));
        Ok(())
    }
}
