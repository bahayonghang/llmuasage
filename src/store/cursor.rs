use std::collections::HashMap;

use crate::error::{LlmusageError, Result};
use rusqlite::{OptionalExtension, params};

use super::{FileCursor, OpencodeCursor, Store};
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

    pub fn load_file_cursors(&self, source: SourceKind) -> Result<HashMap<String, FileCursor>> {
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
            WHERE source = ?1
            "#,
        )?;
        let rows = stmt.query_map(params![source.as_str()], |row| {
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

    pub fn load_opencode_cursor(&self) -> Result<OpencodeCursor> {
        let conn = self.store.open_connection()?;
        let row = conn
            .query_row(
                r#"
                SELECT inode, last_time_created, last_processed_ids_json, sqlite_status, updated_at
                FROM source_cursor
                WHERE source = 'opencode' AND cursor_key = 'main'
                "#,
                [],
                |row| {
                    let ids_json: Option<String> = row.get(2)?;
                    Ok(OpencodeCursor {
                        inode: row.get::<_, Option<i64>>(0)?.unwrap_or_default().max(0) as u64,
                        last_time_created: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                        last_processed_ids: ids_json
                            .as_deref()
                            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                            .unwrap_or_default(),
                        sqlite_status: row
                            .get::<_, Option<String>>(3)?
                            .unwrap_or_else(|| "never_checked".to_string()),
                        updated_at: row.get::<_, Option<String>>(4)?.unwrap_or_else(now_utc),
                    })
                },
            )
            .optional()?;

        Ok(row.unwrap_or_default())
    }

    pub fn save_opencode_cursor(&self, cursor: &OpencodeCursor) -> Result<()> {
        let conn = self.store.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO source_cursor(
                source, cursor_key, inode, last_time_created, last_processed_ids_json, sqlite_status, updated_at
            ) VALUES ('opencode', 'main', ?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(source, cursor_key) DO UPDATE SET
                inode = excluded.inode,
                last_time_created = excluded.last_time_created,
                last_processed_ids_json = excluded.last_processed_ids_json,
                sqlite_status = excluded.sqlite_status,
                updated_at = excluded.updated_at
            "#,
            params![
                cursor.inode as i64,
                cursor.last_time_created,
                serde_json::to_string(&cursor.last_processed_ids).map_err(|source| {
                    LlmusageError::Parse {
                        context: "opencode cursor",
                        source,
                    }
                })?,
                cursor.sqlite_status,
                cursor.updated_at,
            ],
        )?;
        Ok(())
    }
}
