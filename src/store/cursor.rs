use std::collections::HashMap;

use crate::error::{LlmusageError, Result};
use rusqlite::{OptionalExtension, params};

use super::{FileCursor, OpencodeCursor, Store, ZcodeCursor};
use crate::{
    models::{SourceKind, UsageTokens},
    util::now_utc,
};

/// Borrowed view onto the cursor surface of [`Store`].
///
/// 通过 `store.cursors()` 创建；持借用 `&Store` 不引入 cascade clone。
pub struct CursorStore<'a> {
    store: &'a Store,
}

impl<'a> CursorStore<'a> {
    pub(super) fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub fn load_file_cursors(
        &self,
        source: SourceKind,
        host_id: &str,
    ) -> Result<HashMap<String, FileCursor>> {
        if self.store.emit_only() {
            return Ok(HashMap::new());
        }
        let conn = self.store.open_connection()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
                cursor_key,
                file_path,
                file_fingerprint,
                file_size,
                file_mtime_ns,
                tail_signature,
                offset,
                last_total_json,
                last_model,
                updated_at
            FROM source_cursor
            WHERE source = ?1 AND host_id = ?2
            "#,
        )?;
        let rows = stmt.query_map(params![source.as_str(), host_id], |row| {
            let last_total_json: Option<String> = row.get(7)?;
            Ok(FileCursor {
                cursor_key: row.get(0)?,
                file_path: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                file_fingerprint: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                file_size: row.get::<_, Option<i64>>(3)?.unwrap_or_default().max(0) as u64,
                file_mtime_ns: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                tail_signature: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                offset: row.get::<_, Option<i64>>(6)?.unwrap_or_default().max(0) as u64,
                last_total: last_total_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<UsageTokens>(raw).ok()),
                last_model: row.get(8)?,
                updated_at: row.get::<_, Option<String>>(9)?.unwrap_or_else(now_utc),
            })
        })?;

        let mut output = HashMap::new();
        for row in rows {
            let cursor = row?;
            output.insert(cursor.cursor_key.clone(), cursor);
        }
        Ok(output)
    }

    pub fn load_opencode_cursor(&self, host_id: &str) -> Result<OpencodeCursor> {
        if self.store.emit_only() {
            return Ok(OpencodeCursor::default());
        }
        let conn = self.store.open_connection()?;
        let row = conn
            .query_row(
                r#"
                SELECT inode, last_time_created, last_processed_ids_json,
                       last_part_rowid, sqlite_status, updated_at
                FROM source_cursor
                WHERE host_id = ?1 AND source = 'opencode' AND cursor_key = 'main'
                "#,
                [host_id],
                |row| {
                    let ids_json: Option<String> = row.get(2)?;
                    Ok(OpencodeCursor {
                        inode: row.get::<_, Option<i64>>(0)?.unwrap_or_default().max(0) as u64,
                        last_time_created: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                        last_processed_ids: ids_json
                            .as_deref()
                            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                            .unwrap_or_default(),
                        last_part_rowid: row.get::<_, Option<i64>>(3)?.unwrap_or_default().max(0),
                        sqlite_status: row
                            .get::<_, Option<String>>(4)?
                            .unwrap_or_else(|| "never_checked".to_string()),
                        updated_at: row.get::<_, Option<String>>(5)?.unwrap_or_else(now_utc),
                    })
                },
            )
            .optional()?;

        Ok(row.unwrap_or_default())
    }

    pub fn save_opencode_cursor(&self, host_id: &str, cursor: &OpencodeCursor) -> Result<()> {
        if self.store.emit_only() {
            return Ok(());
        }
        let processed_ids =
            serde_json::to_string(&cursor.last_processed_ids).map_err(|source| {
                LlmusageError::Parse {
                    context: "opencode cursor",
                    source,
                }
            })?;
        self.store.write_transaction(|tx| {
            tx.execute(
                r#"
            INSERT INTO source_cursor(
                host_id, source, cursor_key, inode, last_time_created, last_processed_ids_json,
                last_part_rowid, sqlite_status, updated_at
            ) VALUES (?1, 'opencode', 'main', ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(host_id, source, cursor_key) DO UPDATE SET
                inode = excluded.inode,
                last_time_created = excluded.last_time_created,
                last_processed_ids_json = excluded.last_processed_ids_json,
                last_part_rowid = excluded.last_part_rowid,
                sqlite_status = excluded.sqlite_status,
                updated_at = excluded.updated_at
            "#,
                params![
                    host_id,
                    cursor.inode as i64,
                    cursor.last_time_created,
                    processed_ids,
                    cursor.last_part_rowid,
                    cursor.sqlite_status,
                    cursor.updated_at,
                ],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn load_zcode_cursor(&self, host_id: &str) -> Result<ZcodeCursor> {
        if self.store.emit_only() {
            return Ok(ZcodeCursor::default());
        }
        let conn = self.store.open_connection()?;
        let row = conn
            .query_row(
                r#"
                SELECT last_time_created, last_processed_ids_json, sqlite_status, updated_at,
                       last_skipped_at, last_skipped_ids_json
                FROM source_cursor
                WHERE host_id = ?1 AND source = 'zcode' AND cursor_key = 'main'
                "#,
                [host_id],
                |row| {
                    let ids_json: Option<String> = row.get(1)?;
                    let skipped_ids_json: Option<String> = row.get(5)?;
                    Ok(ZcodeCursor {
                        last_completed_at: row.get::<_, Option<i64>>(0)?.unwrap_or_default(),
                        last_processed_ids: ids_json
                            .as_deref()
                            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                            .unwrap_or_default(),
                        sqlite_status: row
                            .get::<_, Option<String>>(2)?
                            .unwrap_or_else(|| "never_checked".to_string()),
                        updated_at: row.get::<_, Option<String>>(3)?.unwrap_or_else(now_utc),
                        last_skipped_at: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                        last_skipped_ids: skipped_ids_json
                            .as_deref()
                            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                            .unwrap_or_default(),
                    })
                },
            )
            .optional()?;

        Ok(row.unwrap_or_default())
    }

    pub fn save_zcode_cursor(&self, host_id: &str, cursor: &ZcodeCursor) -> Result<()> {
        if self.store.emit_only() {
            return Ok(());
        }
        let processed_ids =
            serde_json::to_string(&cursor.last_processed_ids).map_err(|source| {
                LlmusageError::Parse {
                    context: "zcode cursor",
                    source,
                }
            })?;
        let skipped_ids = serde_json::to_string(&cursor.last_skipped_ids).map_err(|source| {
            LlmusageError::Parse {
                context: "zcode skip cursor",
                source,
            }
        })?;
        self.store.write_transaction(|tx| {
            tx.execute(
                r#"
            INSERT INTO source_cursor(
                host_id, source, cursor_key, last_time_created, last_processed_ids_json,
                sqlite_status, updated_at, last_skipped_at, last_skipped_ids_json
            ) VALUES (?1, 'zcode', 'main', ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(host_id, source, cursor_key) DO UPDATE SET
                last_time_created = excluded.last_time_created,
                last_processed_ids_json = excluded.last_processed_ids_json,
                sqlite_status = excluded.sqlite_status,
                updated_at = excluded.updated_at,
                last_skipped_at = excluded.last_skipped_at,
                last_skipped_ids_json = excluded.last_skipped_ids_json
            "#,
                params![
                    host_id,
                    cursor.last_completed_at,
                    processed_ids,
                    cursor.sqlite_status,
                    cursor.updated_at,
                    cursor.last_skipped_at,
                    skipped_ids,
                ],
            )?;
            Ok(())
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::paths::AppPaths;

    #[test]
    fn load_file_cursors_treats_corrupt_last_total_json_as_none() -> Result<()> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        store.write_transaction(|tx| {
            tx.execute(
                r#"
                INSERT INTO source_cursor(host_id, source, cursor_key, last_total_json, updated_at)
                VALUES ('local', 'codex', 'corrupt.jsonl', 'not-json', '2026-05-08T00:00:00Z')
                "#,
                [],
            )?;
            Ok(())
        })?;

        let cursors = store
            .cursors()
            .load_file_cursors(SourceKind::Codex, "local")?;
        let cursor = cursors
            .get("corrupt.jsonl")
            .expect("corrupt cursor row still loads");
        assert_eq!(cursor.last_total, None);
        Ok(())
    }
}
