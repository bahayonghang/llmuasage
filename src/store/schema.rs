use std::fs;

use rusqlite::OptionalExtension;
use tracing::info;

use super::{BootstrapOptions, BootstrapProgressEvent, BootstrapProgressSink, Store, migrations};
use crate::{error::Result, models::SourceKind};

const META_RAW_ARCHIVE_KEY: &str = "raw_archive_enabled";

/// Default parser-owned token normalization contract.
pub const TOKEN_ACCOUNTING_VERSION: u32 = 2;

/// Returns the current token-accounting contract for one parser source.
pub const fn expected_token_accounting_version(source: SourceKind) -> u32 {
    match source {
        SourceKind::Codex => 3,
        SourceKind::Claude | SourceKind::Opencode | SourceKind::Antigravity => {
            TOKEN_ACCOUNTING_VERSION
        }
    }
}

impl Store {
    pub fn bootstrap(&self) -> Result<()> {
        self.bootstrap_with(BootstrapOptions::default())
    }

    /// Bootstraps the store while exposing migration lifecycle progress.
    pub fn bootstrap_with_migration_events(
        &self,
        sink: Option<migrations::MigrationEventSink<'_>>,
    ) -> Result<()> {
        if let Some(migration_sink) = sink {
            let mut adapter = |event| {
                if let BootstrapProgressEvent::Migration(event) = event {
                    migration_sink(event);
                }
            };
            self.bootstrap_with_events(BootstrapOptions::default(), Some(&mut adapter))
        } else {
            self.bootstrap_with_events(BootstrapOptions::default(), None)
        }
    }

    pub(crate) fn bootstrap_with_progress(
        &self,
        sink: Option<BootstrapProgressSink<'_>>,
    ) -> Result<()> {
        self.bootstrap_with_events(BootstrapOptions::default(), sink)
    }

    /// Bootstraps the store and applies optional configuration via
    /// [`BootstrapOptions`]. Currently the only knob is the raw archive flag
    /// (D11 / F1.5); setting it here lets ccr-ui or library callers express
    /// the toggle as part of their first contact with the database.
    pub fn bootstrap_with(&self, options: BootstrapOptions) -> Result<()> {
        self.bootstrap_with_events(options, None)
    }

    fn bootstrap_with_events(
        &self,
        options: BootstrapOptions,
        mut progress_sink: Option<BootstrapProgressSink<'_>>,
    ) -> Result<()> {
        /*
         * ========================================================================
         * 步骤1：初始化本地目录与 SQLite schema
         * ========================================================================
         * 目标：
         * 1) 建立 llmusage 运行目录、备份目录与导出目录
         * 2) 通过 migration runner 创建或升级 SQLite schema
         * 3) 0.4.x v0 老库升级前保留一次自动备份
         * 4) 按 BootstrapOptions 落实 raw archive 开关（默认保持当前值）
         */
        info!("开始初始化本地目录与 SQLite schema");

        fs::create_dir_all(&self.paths.root_dir)?;
        fs::create_dir_all(&self.paths.bin_dir)?;
        fs::create_dir_all(&self.paths.backups_dir)?;
        fs::create_dir_all(&self.paths.exports_dir)?;
        fs::create_dir_all(&self.paths.logs_dir)?;

        let mut conn = self.open_connection()?;
        if migrations::read_schema_version(&conn)? == 0 && self.paths.db_path.is_file() {
            self.backup_pre_0_5_0_db()?;
        }
        if let Some(sink) = progress_sink.as_deref_mut() {
            let mut migration_sink = |event| sink(BootstrapProgressEvent::Migration(event));
            migrations::run_migrations_with_events(&mut conn, Some(&mut migration_sink))?;
        } else {
            migrations::run_migrations_with_events(&mut conn, None)?;
        }

        if let Some(enabled) = options.enable_raw_archive {
            write_meta_flag(&conn, META_RAW_ARCHIVE_KEY, enabled)?;
        }
        drop(conn);
        self.upgrade_embedded_pricing_if_needed(progress_sink)?;

        info!(
            version = migrations::latest_schema_version(),
            "完成本地目录与 SQLite schema 初始化"
        );
        Ok(())
    }

    /// Reads the persisted raw archive flag (D11 / F1.5).
    ///
    /// Returns `false` if the meta row is missing or unrecognised. The
    /// migration v7 seeds `'0'` so a freshly bootstrapped database always
    /// reports `false` here.
    pub fn raw_archive_enabled(&self) -> Result<bool> {
        let conn = self.open_connection()?;
        read_meta_flag(&conn, META_RAW_ARCHIVE_KEY)
    }

    /// Persists the raw archive flag without touching schema.
    pub fn set_raw_archive(&self, enabled: bool) -> Result<()> {
        let conn = self.open_connection()?;
        write_meta_flag(&conn, META_RAW_ARCHIVE_KEY, enabled)
    }

    /// Reads a raw string value from the `meta` table.
    pub fn meta_value(&self, key: &str) -> Result<Option<String>> {
        let conn = self.open_connection()?;
        read_meta_value(&conn, key)
    }

    /// Persists a raw string value into the `meta` table.
    pub fn set_meta_value(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.open_connection()?;
        write_meta_value(&conn, key, value)
    }

    /// Returns the token-accounting contract recorded for one parser source.
    pub fn token_accounting_version(&self, source: SourceKind) -> Result<Option<u32>> {
        Ok(self
            .meta_value(&token_accounting_key(source))?
            .and_then(|value| value.parse().ok()))
    }

    /// True when persisted rows predate the current token-accounting contract.
    pub fn has_legacy_token_accounting(&self, source: SourceKind) -> Result<bool> {
        if self.token_accounting_version(source)? == Some(expected_token_accounting_version(source))
        {
            return Ok(false);
        }
        let conn = self.open_connection()?;
        let rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = ?1",
            [source.as_str()],
            |row| row.get(0),
        )?;
        Ok(rows > 0)
    }

    /// Marks a source only after its parser/store sync completes successfully.
    pub fn mark_current_token_accounting(&self, source: SourceKind) -> Result<()> {
        self.set_meta_value(
            &token_accounting_key(source),
            &expected_token_accounting_version(source).to_string(),
        )
    }

    /// Clears the marker before a guarded rebuild starts.
    pub fn clear_token_accounting_version(&self, source: SourceKind) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            [token_accounting_key(source)],
        )?;
        Ok(())
    }

    /// Deletes rebuildable usage state for exactly one source (D20 / F3.3).
    ///
    /// `project_dim` is intentionally preserved because projects can be shared
    /// by multiple sources and are cheap stale metadata until the next full GC.
    pub fn reset_for_source(&self, source: crate::models::SourceKind) -> Result<()> {
        info!(source = %source, "开始按源清空可重建用量数据");
        let mut conn = self.open_connection()?;
        let tx = conn.transaction()?;
        {
            let source = source.as_str();
            tx.execute("DELETE FROM usage_tool_call WHERE source = ?1", [source])?;
            tx.execute("DELETE FROM usage_turn WHERE source = ?1", [source])?;
            tx.execute("DELETE FROM usage_event WHERE source = ?1", [source])?;
            tx.execute("DELETE FROM usage_bucket_30m WHERE source = ?1", [source])?;
            tx.execute("DELETE FROM source_cursor WHERE source = ?1", [source])?;
            tx.execute("DELETE FROM source_sync_status WHERE source = ?1", [source])?;
            tx.execute("DELETE FROM source_file WHERE source = ?1", [source])?;
            tx.execute(
                r#"
                DELETE FROM usage_event_raw
                WHERE event_key IN (
                    SELECT raw.event_key
                    FROM usage_event_raw raw
                    WHERE raw.event_key LIKE ?1
                )
                "#,
                [format!("{source}:%")],
            )?;
        }
        tx.commit()?;
        info!(source = %source, "完成按源清空可重建用量数据");
        Ok(())
    }

    pub fn reset_usage_data(&self) -> Result<()> {
        /*
         * ========================================================================
         * 步骤2：低层全局清空用量数据
         * ========================================================================
         * 目标：
         * 1) 为明确需要全局 reset 的内部调用方保留兼容 API
         * 2) 清空所有来源，包括没有 parser capability 的来源
         * 3) command-level `sync --rebuild` 不得调用；它必须按 parser registry 逐源 reset
         * 4) 保留 run_log / integration_install / trigger_state 等运维记录
         */
        info!("开始清空可重建用量数据");

        let conn = self.open_connection()?;
        conn.execute_batch(
            r#"
            DELETE FROM usage_tool_call;
            DELETE FROM usage_turn;
            DELETE FROM usage_event;
            DELETE FROM usage_bucket_30m;
            DELETE FROM project_dim;
            DELETE FROM source_cursor;
            DELETE FROM source_sync_status;
            DELETE FROM usage_event_raw;
            "#,
        )?;

        info!("完成清空可重建用量数据");
        Ok(())
    }

    fn backup_pre_0_5_0_db(&self) -> Result<()> {
        fs::create_dir_all(&self.paths.backups_dir)?;
        let backup_path = self.paths.backups_dir.join("llmusage.db.pre-0.5.0");
        if !backup_path.exists() {
            fs::copy(&self.paths.db_path, &backup_path)?;
        }
        Ok(())
    }
}

fn token_accounting_key(source: crate::models::SourceKind) -> String {
    format!("token_accounting_version.{}", source.as_str())
}

fn read_meta_flag(conn: &rusqlite::Connection, key: &str) -> Result<bool> {
    let raw = read_meta_value(conn, key)?;
    Ok(matches!(raw.as_deref(), Some("1")))
}

fn write_meta_flag(conn: &rusqlite::Connection, key: &str, enabled: bool) -> Result<()> {
    let value = if enabled { "1" } else { "0" };
    write_meta_value(conn, key, value)
}

fn read_meta_value(conn: &rusqlite::Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .optional()?)
}

fn write_meta_value(conn: &rusqlite::Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO meta(key, value)
        VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
        rusqlite::params![key, value],
    )?;
    Ok(())
}
