use std::{collections::HashMap, fs, path::Path};

use anyhow::Result;
use fs4::fs_std::FileExt;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

use crate::{
    models::{ProjectInfo, SourceKind, UsageEvent, UsageTokens},
    paths::AppPaths,
    util::now_utc,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileCursor {
    pub cursor_key: String,
    pub file_path: String,
    pub inode: u64,
    pub offset: u64,
    pub last_total: Option<UsageTokens>,
    pub last_model: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpencodeCursor {
    pub inode: u64,
    pub last_time_created: i64,
    pub last_processed_ids: Vec<String>,
    pub sqlite_status: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationState {
    pub source: String,
    pub install_type: String,
    pub status: String,
    pub config_path: Option<String>,
    pub backup_path: Option<String>,
    pub details_json: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: i64,
    pub command: String,
    pub status: String,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerStateRecord {
    pub source: String,
    pub last_signal_at: String,
    pub trigger: String,
    pub last_worker_started_at: Option<String>,
    pub last_worker_finished_at: Option<String>,
    pub updated_at: String,
}

pub struct WorkerLock {
    _file: std::fs::File,
}

#[derive(Debug, Clone)]
pub struct Store {
    pub paths: AppPaths,
}

impl Store {
    pub fn new(paths: &AppPaths) -> Self {
        Self {
            paths: paths.clone(),
        }
    }

    pub fn bootstrap(&self) -> Result<()> {
        /*
         * ========================================================================
         * 步骤1：初始化本地目录与 SQLite schema
         * ========================================================================
         * 目标：
         * 1) 建立 llmusage 运行目录、备份目录与导出目录
         * 2) 创建 SQLite 真源与全部核心表
         * 3) 为后续 cursor、聚合、诊断与 hook 状态提供统一存储
         */
        info!("开始初始化本地目录与 SQLite schema");

        // 1.1 建立本地运行目录
        fs::create_dir_all(&self.paths.root_dir)?;
        fs::create_dir_all(&self.paths.bin_dir)?;
        fs::create_dir_all(&self.paths.backups_dir)?;
        fs::create_dir_all(&self.paths.exports_dir)?;

        // 1.2 打开数据库并应用 schema
        let conn = self.open_connection()?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS source_cursor (
                source TEXT NOT NULL,
                cursor_key TEXT NOT NULL,
                file_path TEXT,
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
                finished_at TEXT
            );
            CREATE TABLE IF NOT EXISTS trigger_state (
                source TEXT PRIMARY KEY,
                last_signal_at TEXT NOT NULL,
                trigger TEXT NOT NULL,
                last_worker_started_at TEXT,
                last_worker_finished_at TEXT,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_usage_bucket_30m_hour_start
                ON usage_bucket_30m(hour_start);
            CREATE INDEX IF NOT EXISTS idx_usage_event_source_event_at
                ON usage_event(source, event_at);
            "#,
        )?;

        info!("完成本地目录与 SQLite schema 初始化");
        Ok(())
    }

    pub fn open_connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.paths.db_path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA temp_store = MEMORY;
            "#,
        )?;
        Ok(conn)
    }

    pub fn acquire_worker_lock(&self) -> Result<Option<WorkerLock>> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&self.paths.lock_path)?;

        if file.try_lock_exclusive().is_err() {
            return Ok(None);
        }

        Ok(Some(WorkerLock { _file: file }))
    }

    pub fn record_run_start(&self, command: &str) -> Result<i64> {
        let now = now_utc();
        let conn = self.open_connection()?;
        conn.execute(
            "INSERT INTO run_log(command, status, started_at) VALUES (?1, 'running', ?2)",
            params![command, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn finish_run(
        &self,
        id: i64,
        status: &str,
        summary: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let now = now_utc();
        let conn = self.open_connection()?;
        conn.execute(
            r#"
            UPDATE run_log
            SET status = ?2, summary = ?3, error = ?4, finished_at = ?5
            WHERE id = ?1
            "#,
            params![id, status, summary, error, now],
        )?;
        Ok(())
    }

    pub fn upsert_trigger_state(
        &self,
        source: SourceKind,
        trigger: &str,
        signal_at: &str,
    ) -> Result<()> {
        let now = now_utc();
        let conn = self.open_connection()?;
        conn.execute(
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
    }

    pub fn mark_trigger_worker_started(&self, source: SourceKind, started_at: &str) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            r#"
            UPDATE trigger_state
            SET last_worker_started_at = ?2, updated_at = ?2
            WHERE source = ?1
            "#,
            params![source.as_str(), started_at],
        )?;
        Ok(())
    }

    pub fn mark_trigger_worker_finished(
        &self,
        source: SourceKind,
        finished_at: &str,
    ) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            r#"
            UPDATE trigger_state
            SET last_worker_finished_at = ?2, updated_at = ?2
            WHERE source = ?1
            "#,
            params![source.as_str(), finished_at],
        )?;
        Ok(())
    }

    pub fn trigger_snapshot(&self) -> Result<Vec<(String, String)>> {
        let conn = self.open_connection()?;
        let mut stmt =
            conn.prepare("SELECT source, last_signal_at FROM trigger_state ORDER BY source ASC")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn load_file_cursors(&self, source: SourceKind) -> Result<HashMap<String, FileCursor>> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT cursor_key, file_path, inode, offset, last_total_json, last_model, updated_at
            FROM source_cursor
            WHERE source = ?1
            "#,
        )?;
        let rows = stmt.query_map(params![source.as_str()], |row| {
            let last_total_json: Option<String> = row.get(4)?;
            Ok(FileCursor {
                cursor_key: row.get(0)?,
                file_path: row.get(1)?,
                inode: row.get::<_, Option<u64>>(2)?.unwrap_or_default(),
                offset: row.get::<_, Option<u64>>(3)?.unwrap_or_default(),
                last_total: last_total_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<UsageTokens>(raw).ok()),
                last_model: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;

        let mut output = HashMap::new();
        for row in rows {
            let cursor = row?;
            output.insert(cursor.cursor_key.clone(), cursor);
        }
        Ok(output)
    }

    pub fn save_file_cursor(&self, source: SourceKind, cursor: &FileCursor) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO source_cursor(
                source, cursor_key, file_path, inode, offset, last_total_json, last_model, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(source, cursor_key) DO UPDATE SET
                file_path = excluded.file_path,
                inode = excluded.inode,
                offset = excluded.offset,
                last_total_json = excluded.last_total_json,
                last_model = excluded.last_model,
                updated_at = excluded.updated_at
            "#,
            params![
                source.as_str(),
                cursor.cursor_key,
                cursor.file_path,
                cursor.inode as i64,
                cursor.offset as i64,
                cursor
                    .last_total
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                cursor.last_model,
                cursor.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn load_opencode_cursor(&self) -> Result<OpencodeCursor> {
        let conn = self.open_connection()?;
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
                        inode: row.get::<_, Option<u64>>(0)?.unwrap_or_default(),
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
        let conn = self.open_connection()?;
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
                serde_json::to_string(&cursor.last_processed_ids)?,
                cursor.sqlite_status,
                cursor.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn record_usage_event(&self, event: &UsageEvent) -> Result<bool> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction()?;
        let inserted = tx.execute(
            r#"
            INSERT OR IGNORE INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens,
                project_hash, project_label, project_ref, path_hash, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                event.event_key,
                event.source.as_str(),
                event.model,
                event.event_at,
                event.hour_start,
                event.tokens.input_tokens,
                event.tokens.cached_input_tokens,
                event.tokens.output_tokens,
                event.tokens.reasoning_output_tokens,
                event.tokens.total_tokens,
                event.project.as_ref().map(|value| value.project_hash.as_str()),
                event.project.as_ref().map(|value| value.project_label.as_str()),
                event.project.as_ref().and_then(|value| value.project_ref.as_deref()),
                event.project.as_ref().map(|value| value.path_hash.as_str()),
                now_utc(),
            ],
        )?;

        if inserted == 0 {
            tx.commit()?;
            return Ok(false);
        }

        if let Some(project) = &event.project {
            self.upsert_project_dim_tx(&tx, project)?;
        }
        self.upsert_bucket_tx(&tx, event)?;
        tx.commit()?;
        Ok(true)
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
        let conn = self.open_connection()?;
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
                details.map(serde_json::to_string).transpose()?,
                now_utc(),
            ],
        )?;
        Ok(())
    }

    pub fn load_integration_states(&self) -> Result<Vec<IntegrationState>> {
        let conn = self.open_connection()?;
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

    pub fn recent_runs(&self, limit: usize) -> Result<Vec<RunRecord>> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, command, status, summary, error, started_at, finished_at
            FROM run_log
            ORDER BY id DESC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(RunRecord {
                id: row.get(0)?,
                command: row.get(1)?,
                status: row.get(2)?,
                summary: row.get(3)?,
                error: row.get(4)?,
                started_at: row.get(5)?,
                finished_at: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn upsert_project_dim_tx(&self, tx: &Transaction<'_>, project: &ProjectInfo) -> Result<()> {
        tx.execute(
            r#"
            INSERT INTO project_dim(
                project_hash, project_label, project_ref, repo_root_hash, path_hash, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(project_hash) DO UPDATE SET
                project_label = excluded.project_label,
                project_ref = excluded.project_ref,
                repo_root_hash = excluded.repo_root_hash,
                path_hash = excluded.path_hash,
                updated_at = excluded.updated_at
            "#,
            params![
                project.project_hash,
                project.project_label,
                project.project_ref,
                project.repo_root_hash,
                project.path_hash,
                now_utc(),
            ],
        )?;
        Ok(())
    }

    fn upsert_bucket_tx(&self, tx: &Transaction<'_>, event: &UsageEvent) -> Result<()> {
        tx.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(source, model, hour_start, project_hash) DO UPDATE SET
                project_label = excluded.project_label,
                project_ref = excluded.project_ref,
                input_tokens = usage_bucket_30m.input_tokens + excluded.input_tokens,
                cached_input_tokens = usage_bucket_30m.cached_input_tokens + excluded.cached_input_tokens,
                output_tokens = usage_bucket_30m.output_tokens + excluded.output_tokens,
                reasoning_output_tokens = usage_bucket_30m.reasoning_output_tokens + excluded.reasoning_output_tokens,
                total_tokens = usage_bucket_30m.total_tokens + excluded.total_tokens,
                updated_at = excluded.updated_at
            "#,
            params![
                event.source.as_str(),
                event.model,
                event.hour_start,
                event.project
                    .as_ref()
                    .map(|value| value.project_hash.as_str())
                    .unwrap_or(""),
                event.project.as_ref().map(|value| value.project_label.as_str()),
                event.project.as_ref().and_then(|value| value.project_ref.as_deref()),
                event.tokens.input_tokens,
                event.tokens.cached_input_tokens,
                event.tokens.output_tokens,
                event.tokens.reasoning_output_tokens,
                event.tokens.total_tokens,
                now_utc(),
            ],
        )?;
        Ok(())
    }
}
