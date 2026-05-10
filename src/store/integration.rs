use std::path::Path;

use crate::error::{LlmusageError, Result};
use rusqlite::{Connection, params};
use serde_json::Value;

use super::{IntegrationState, Store};
use crate::{models::SourceKind, util::now_utc};

/// Borrowed view onto the integration-state surface of [`Store`].
///
/// 通过 `store.integration_state()` 创建。
pub struct IntegrationStateStore<'a> {
    store: &'a Store,
}

impl<'a> IntegrationStateStore<'a> {
    pub(super) fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub fn record_integration_state(
        &self,
        source: SourceKind,
        install_type: &str,
        status: &str,
        config_path: Option<&Path>,
        backup_path: Option<&Path>,
        details: Option<&Value>,
    ) -> Result<()> {
        let conn = self.store.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO integration_install(
                source, install_type, status, config_path, backup_path, details_json, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(source) DO UPDATE SET
                install_type = excluded.install_type,
                status = excluded.status,
                config_path = excluded.config_path,
                backup_path = excluded.backup_path,
                details_json = excluded.details_json,
                updated_at = excluded.updated_at
            "#,
            params![
                source.as_str(),
                install_type,
                status,
                config_path.map(|path| path.to_string_lossy().to_string()),
                backup_path.map(|path| path.to_string_lossy().to_string()),
                details
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|source| LlmusageError::Parse {
                        context: "integration details",
                        source,
                    })?,
                now_utc(),
            ],
        )?;
        Ok(())
    }

    pub fn load_integration_states(&self) -> Result<Vec<IntegrationState>> {
        let conn = self.store.open_connection()?;
        self.load_integration_states_with_conn(&conn)
    }

    pub(crate) fn load_integration_states_with_conn(
        &self,
        conn: &Connection,
    ) -> Result<Vec<IntegrationState>> {
        let mut stmt = conn.prepare(
            r#"
            SELECT source, install_type, status, config_path, backup_path, details_json, updated_at
            FROM integration_install
            ORDER BY source ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(IntegrationState {
                source: row.get(0)?,
                install_type: row.get(1)?,
                status: row.get(2)?,
                config_path: row.get(3)?,
                backup_path: row.get(4)?,
                details_json: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
