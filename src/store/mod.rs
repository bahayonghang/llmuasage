use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    time::Duration,
};

use anyhow::Result;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rusqlite::{
    Connection, OptionalExtension, TransactionBehavior, params, params_from_iter,
    types::Value as SqlValue,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

use crate::{
    models::{ProjectInfo, SourceKind, UsageEvent, UsageTokens},
    paths::AppPaths,
    util::now_utc,
};

const WORKER_LOCK_NAME: &str = "sync-worker";
const WORKER_LEASE_MINUTES: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileCursor {
    pub cursor_key: String,
    pub file_path: String,
    pub file_fingerprint: String,
    pub file_size: u64,
    pub file_mtime_ns: i64,
    pub tail_signature: String,
    pub offset: u64,
    pub last_total: Option<UsageTokens>,
    pub last_model: Option<String>,
    pub updated_at: String,
}

impl FileCursor {
    pub fn materially_eq(&self, other: &Self) -> bool {
        self.cursor_key == other.cursor_key
            && self.file_path == other.file_path
            && self.file_fingerprint == other.file_fingerprint
            && self.file_size == other.file_size
            && self.file_mtime_ns == other.file_mtime_ns
            && self.tail_signature == other.tail_signature
            && self.offset == other.offset
            && self.last_total == other.last_total
            && self.last_model == other.last_model
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSyncStatus {
    pub source: String,
    pub files_processed: i64,
    pub changed_files: i64,
    pub bytes_scanned: i64,
    pub events_seen: i64,
    pub events_replayed: i64,
    pub events_inserted: i64,
    pub parse_ms: i64,
    pub write_ms: i64,
    pub lock_wait_ms: i64,
    pub updated_at: String,
}

pub struct WorkerLock {
    store: Store,
    lock_name: String,
    owner_id: String,
}

impl WorkerLock {
    pub fn refresh(&self) -> Result<()> {
        self.store
            .refresh_worker_lock(&self.lock_name, &self.owner_id)
    }
}

impl Drop for WorkerLock {
    fn drop(&mut self) {
        let _ = self
            .store
            .release_worker_lock(&self.lock_name, &self.owner_id);
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    pub paths: AppPaths,
}

pub struct SyncRunWriter {
    conn: Connection,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct BucketKey {
    source: String,
    model: String,
    hour_start: String,
    project_hash: String,
}

#[derive(Debug, Clone)]
struct BucketRollup {
    project_label: Option<String>,
    project_ref: Option<String>,
    tokens: UsageTokens,
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
         * 3) 补齐新字段和增量锁表，兼容已有 DB
         */
        info!("开始初始化本地目录与 SQLite schema");

        // 1.1 建立本地运行目录
        fs::create_dir_all(&self.paths.root_dir)?;
        fs::create_dir_all(&self.paths.bin_dir)?;
        fs::create_dir_all(&self.paths.backups_dir)?;
        fs::create_dir_all(&self.paths.exports_dir)?;

        // 1.2 打开数据库并应用 schema + 迁移
        let conn = self.open_connection()?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS source_cursor (
                source TEXT NOT NULL,
                cursor_key TEXT NOT NULL,
                file_path TEXT,
                file_fingerprint TEXT,
                file_size INTEGER,
                file_mtime_ns INTEGER,
                tail_signature TEXT,
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
                finished_at TEXT,
                duration_ms INTEGER
            );
            CREATE TABLE IF NOT EXISTS trigger_state (
                source TEXT PRIMARY KEY,
                last_signal_at TEXT NOT NULL,
                trigger TEXT NOT NULL,
                last_worker_started_at TEXT,
                last_worker_finished_at TEXT,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS worker_lease (
                lock_name TEXT PRIMARY KEY,
                owner_id TEXT NOT NULL,
                lease_expires_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS source_sync_status (
                source TEXT PRIMARY KEY,
                files_processed INTEGER NOT NULL,
                changed_files INTEGER NOT NULL,
                bytes_scanned INTEGER NOT NULL,
                events_seen INTEGER NOT NULL,
                events_replayed INTEGER NOT NULL,
                events_inserted INTEGER NOT NULL,
                parse_ms INTEGER NOT NULL,
                write_ms INTEGER NOT NULL,
                lock_wait_ms INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_usage_bucket_30m_hour_start
                ON usage_bucket_30m(hour_start);
            CREATE INDEX IF NOT EXISTS idx_usage_event_source_event_at
                ON usage_event(source, event_at);
            CREATE INDEX IF NOT EXISTS idx_usage_event_source_event_key
                ON usage_event(source, event_key);
            "#,
        )?;
        ensure_column(&conn, "source_cursor", "file_fingerprint", "TEXT")?;
        ensure_column(&conn, "source_cursor", "file_size", "INTEGER")?;
        ensure_column(&conn, "source_cursor", "file_mtime_ns", "INTEGER")?;
        ensure_column(&conn, "source_cursor", "tail_signature", "TEXT")?;
        ensure_column(&conn, "run_log", "duration_ms", "INTEGER")?;

        info!("完成本地目录与 SQLite schema 初始化");
        Ok(())
    }

    pub fn open_connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.paths.db_path)?;
        conn.busy_timeout(Duration::from_secs(30))?;
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
        /*
         * ========================================================================
         * 步骤2：申请 SQLite 租约锁
         * ========================================================================
         * 目标：
         * 1) 用单行租约替代失效的文件锁
         * 2) 只允许一个 sync / hook-run worker 进入
         * 3) 异常退出后依赖租约过期自动恢复
         */
        info!("开始申请 SQLite worker 租约锁");

        // 2.1 用 IMMEDIATE 事务串行化锁竞争
        let owner_id = format!(
            "{}:{}:{}",
            std::process::id(),
            now_utc(),
            self.paths.db_path.display()
        );
        let now = Utc::now();
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = tx
            .query_row(
                r#"
                SELECT owner_id, lease_expires_at
                FROM worker_lease
                WHERE lock_name = ?1
                "#,
                params![WORKER_LOCK_NAME],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        let acquired = match existing {
            None => {
                tx.execute(
                    r#"
                    INSERT INTO worker_lease(lock_name, owner_id, lease_expires_at, updated_at)
                    VALUES (?1, ?2, ?3, ?4)
                    "#,
                    params![
                        WORKER_LOCK_NAME,
                        owner_id,
                        lease_expires_at(now),
                        now.to_rfc3339(),
                    ],
                )?;
                true
            }
            Some((_owner, expires_at)) if lease_expired(&expires_at, now) => {
                tx.execute(
                    r#"
                    UPDATE worker_lease
                    SET owner_id = ?2, lease_expires_at = ?3, updated_at = ?4
                    WHERE lock_name = ?1
                    "#,
                    params![
                        WORKER_LOCK_NAME,
                        owner_id,
                        lease_expires_at(now),
                        now.to_rfc3339(),
                    ],
                )?;
                true
            }
            Some(_) => false,
        };
        tx.commit()?;

        // 2.2 命中租约才返回 guard
        if !acquired {
            info!("SQLite worker 租约锁已被占用");
            return Ok(None);
        }

        info!("完成 SQLite worker 租约锁申请");
        Ok(Some(WorkerLock {
            store: self.clone(),
            lock_name: WORKER_LOCK_NAME.to_string(),
            owner_id,
        }))
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
        let conn = self.open_connection()?;
        let changed = conn.execute(&sql, params_from_iter(params))?;
        Ok(changed)
    }

    pub fn begin_sync_run(&self) -> Result<SyncRunWriter> {
        /*
         * ========================================================================
         * 步骤3：建立单写入端
         * ========================================================================
         * 目标：
         * 1) 复用单个 SQLite 连接处理批量写
         * 2) 把 event / bucket / project / cursor 写入收敛到一个出口
         * 3) 避免每条 event 单独开连接和事务
         */
        info!("开始建立 sync 单写入端");
        let conn = self.open_connection()?;
        info!("完成 sync 单写入端建立");
        Ok(SyncRunWriter { conn })
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

    pub fn load_source_sync_statuses(&self) -> Result<Vec<SourceSyncStatus>> {
        let conn = self.open_connection()?;
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

        let mut conn = self.open_connection()?;
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

    fn refresh_worker_lock(&self, lock_name: &str, owner_id: &str) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            r#"
            UPDATE worker_lease
            SET lease_expires_at = ?3, updated_at = ?4
            WHERE lock_name = ?1 AND owner_id = ?2
            "#,
            params![lock_name, owner_id, lease_expires_at(Utc::now()), now_utc(),],
        )?;
        Ok(())
    }

    fn release_worker_lock(&self, lock_name: &str, owner_id: &str) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            "DELETE FROM worker_lease WHERE lock_name = ?1 AND owner_id = ?2",
            params![lock_name, owner_id],
        )?;
        Ok(())
    }
}

impl SyncRunWriter {
    pub fn reset_file_events_batch(
        &mut self,
        source: SourceKind,
        path_hashes: &[String],
    ) -> Result<()> {
        if path_hashes.is_empty() {
            return Ok(());
        }

        /*
         * ========================================================================
         * 步骤4：清理需要重放的旧事件
         * ========================================================================
         * 目标：
         * 1) 在整文件重放前先移除旧 event
         * 2) 同步回滚 bucket 聚合，避免双计
         * 3) 保持 path 级别重放的幂等
         */
        info!(source = %source, count = path_hashes.len(), "开始清理重放旧事件");

        // 4.1 在同一事务里扣减 bucket 并删除旧 event
        let mut unique = HashSet::new();
        let tx = self.conn.transaction()?;
        {
            let mut aggregate_stmt = tx.prepare_cached(
                r#"
                SELECT
                    model,
                    hour_start,
                    COALESCE(project_hash, ''),
                    SUM(input_tokens),
                    SUM(cached_input_tokens),
                    SUM(output_tokens),
                    SUM(reasoning_output_tokens),
                    SUM(total_tokens)
                FROM usage_event
                WHERE source = ?1 AND event_key LIKE ?2
                GROUP BY model, hour_start, COALESCE(project_hash, '')
                "#,
            )?;
            let mut update_bucket_stmt = tx.prepare_cached(
                r#"
                UPDATE usage_bucket_30m
                SET
                    input_tokens = input_tokens - ?5,
                    cached_input_tokens = cached_input_tokens - ?6,
                    output_tokens = output_tokens - ?7,
                    reasoning_output_tokens = reasoning_output_tokens - ?8,
                    total_tokens = total_tokens - ?9,
                    updated_at = ?10
                WHERE source = ?1 AND model = ?2 AND hour_start = ?3 AND project_hash = ?4
                "#,
            )?;
            let mut delete_zero_stmt = tx.prepare_cached(
                r#"
                DELETE FROM usage_bucket_30m
                WHERE source = ?1 AND model = ?2 AND hour_start = ?3 AND project_hash = ?4
                  AND input_tokens <= 0
                  AND cached_input_tokens <= 0
                  AND output_tokens <= 0
                  AND reasoning_output_tokens <= 0
                  AND total_tokens <= 0
                "#,
            )?;
            let mut delete_event_stmt = tx.prepare_cached(
                "DELETE FROM usage_event WHERE source = ?1 AND event_key LIKE ?2",
            )?;
            let updated_at = now_utc();

            for path_hash in path_hashes {
                if !unique.insert(path_hash.clone()) {
                    continue;
                }

                let prefix = format!("{}:{}:%", source.as_str(), path_hash);
                let rows = aggregate_stmt.query_map(params![source.as_str(), prefix], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        UsageTokens {
                            input_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                            cached_input_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                            output_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                            reasoning_output_tokens: row
                                .get::<_, Option<i64>>(6)?
                                .unwrap_or_default(),
                            total_tokens: row.get::<_, Option<i64>>(7)?.unwrap_or_default(),
                        },
                    ))
                })?;
                let aggregates = rows.collect::<rusqlite::Result<Vec<_>>>()?;

                for (model, hour_start, project_hash, tokens) in aggregates {
                    update_bucket_stmt.execute(params![
                        source.as_str(),
                        model,
                        hour_start,
                        project_hash,
                        tokens.input_tokens,
                        tokens.cached_input_tokens,
                        tokens.output_tokens,
                        tokens.reasoning_output_tokens,
                        tokens.total_tokens,
                        updated_at,
                    ])?;
                    delete_zero_stmt.execute(params![
                        source.as_str(),
                        model,
                        hour_start,
                        project_hash,
                    ])?;
                }

                let prefix = format!("{}:{}:%", source.as_str(), path_hash);
                delete_event_stmt.execute(params![source.as_str(), prefix])?;
            }
        }
        tx.commit()?;

        info!(source = %source, "完成重放旧事件清理");
        Ok(())
    }

    pub fn write_event_batch(&mut self, events: &[UsageEvent]) -> Result<usize> {
        if events.is_empty() {
            return Ok(0);
        }

        /*
         * ========================================================================
         * 步骤5：批量写入 usage_event 与聚合桶
         * ========================================================================
         * 目标：
         * 1) 批量 INSERT OR IGNORE usage_event
         * 2) 仅对新插入事件更新 project_dim 与 bucket
         * 3) 把每批写入保持在单事务内
         */
        info!(batch = events.len(), "开始批量写入 usage_event");

        // 5.1 在单事务中插入 event，并为新 event 做内存聚合
        let tx = self.conn.transaction()?;
        let now = now_utc();
        let inserted = {
            let mut event_stmt = tx.prepare_cached(
                r#"
                INSERT OR IGNORE INTO usage_event(
                    event_key, source, model, event_at, hour_start,
                    input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens,
                    project_hash, project_label, project_ref, path_hash, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                "#,
            )?;
            let mut projects = HashMap::new();
            let mut buckets = HashMap::new();
            let mut inserted = 0usize;

            for event in events {
                let changed = event_stmt.execute(params![
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
                    event
                        .project
                        .as_ref()
                        .map(|value| value.project_hash.as_str()),
                    event
                        .project
                        .as_ref()
                        .map(|value| value.project_label.as_str()),
                    event
                        .project
                        .as_ref()
                        .and_then(|value| value.project_ref.as_deref()),
                    event.project.as_ref().map(|value| value.path_hash.as_str()),
                    now,
                ])?;
                if changed == 0 {
                    continue;
                }

                inserted += 1;
                if let Some(project) = &event.project {
                    projects.insert(project.project_hash.clone(), project.clone());
                }
                roll_up_bucket(&mut buckets, event);
            }
            drop(event_stmt);

            // 5.2 将项目维表和 30 分钟桶一次性刷入
            flush_projects_tx(&tx, &projects)?;
            flush_buckets_tx(&tx, &buckets)?;
            inserted
        };
        tx.commit()?;

        info!(batch = events.len(), inserted, "完成批量写入 usage_event");
        Ok(inserted)
    }

    pub fn write_cursor_batch(&mut self, source: SourceKind, cursors: &[FileCursor]) -> Result<()> {
        if cursors.is_empty() {
            return Ok(());
        }

        /*
         * ========================================================================
         * 步骤6：批量刷新增量游标
         * ========================================================================
         * 目标：
         * 1) 只写本轮真正变更的 cursor
         * 2) 把文件签名、offset、累计 token 状态一并持久化
         * 3) 保持每批 cursor 写入在单事务内
         */
        info!(source = %source, count = cursors.len(), "开始批量刷新 cursor");

        // 6.1 用单事务 upsert 本轮发生变化的 cursor
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT INTO source_cursor(
                    source,
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
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(source, cursor_key) DO UPDATE SET
                    file_path = excluded.file_path,
                    file_fingerprint = excluded.file_fingerprint,
                    file_size = excluded.file_size,
                    file_mtime_ns = excluded.file_mtime_ns,
                    tail_signature = excluded.tail_signature,
                    offset = excluded.offset,
                    last_total_json = excluded.last_total_json,
                    last_model = excluded.last_model,
                    updated_at = excluded.updated_at
                "#,
            )?;
            for cursor in cursors {
                stmt.execute(params![
                    source.as_str(),
                    cursor.cursor_key,
                    cursor.file_path,
                    cursor.file_fingerprint,
                    cursor.file_size as i64,
                    cursor.file_mtime_ns,
                    cursor.tail_signature,
                    cursor.offset as i64,
                    cursor
                        .last_total
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?,
                    cursor.last_model,
                    cursor.updated_at,
                ])?;
            }
        }
        tx.commit()?;

        info!(source = %source, "完成批量刷新 cursor");
        Ok(())
    }

    pub fn finish_sync_run(self) -> Result<()> {
        info!("完成 sync 单写入端收尾");
        Ok(())
    }
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if existing.iter().any(|item| item == column) {
        return Ok(());
    }

    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn lease_expires_at(now: DateTime<Utc>) -> String {
    (now + ChronoDuration::minutes(WORKER_LEASE_MINUTES)).to_rfc3339()
}

fn lease_expired(raw: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| value.with_timezone(&Utc) <= now)
        .unwrap_or(true)
}

fn roll_up_bucket(buckets: &mut HashMap<BucketKey, BucketRollup>, event: &UsageEvent) {
    let project_hash = event
        .project
        .as_ref()
        .map(|value| value.project_hash.clone())
        .unwrap_or_default();
    let key = BucketKey {
        source: event.source.as_str().to_string(),
        model: event.model.clone(),
        hour_start: event.hour_start.clone(),
        project_hash,
    };
    let entry = buckets.entry(key).or_insert_with(|| BucketRollup {
        project_label: event
            .project
            .as_ref()
            .map(|value| value.project_label.clone()),
        project_ref: event
            .project
            .as_ref()
            .and_then(|value| value.project_ref.clone()),
        tokens: UsageTokens::default(),
    });
    entry.tokens.input_tokens += event.tokens.input_tokens;
    entry.tokens.cached_input_tokens += event.tokens.cached_input_tokens;
    entry.tokens.output_tokens += event.tokens.output_tokens;
    entry.tokens.reasoning_output_tokens += event.tokens.reasoning_output_tokens;
    entry.tokens.total_tokens += event.tokens.total_tokens;
}

fn flush_projects_tx(
    tx: &rusqlite::Transaction<'_>,
    projects: &HashMap<String, ProjectInfo>,
) -> Result<()> {
    if projects.is_empty() {
        return Ok(());
    }

    let mut stmt = tx.prepare_cached(
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
    )?;
    let updated_at = now_utc();
    for project in projects.values() {
        stmt.execute(params![
            project.project_hash,
            project.project_label,
            project.project_ref,
            project.repo_root_hash,
            project.path_hash,
            updated_at,
        ])?;
    }
    Ok(())
}

fn flush_buckets_tx(
    tx: &rusqlite::Transaction<'_>,
    buckets: &HashMap<BucketKey, BucketRollup>,
) -> Result<()> {
    if buckets.is_empty() {
        return Ok(());
    }

    let mut stmt = tx.prepare_cached(
        r#"
        INSERT INTO usage_bucket_30m(
            source,
            model,
            hour_start,
            project_hash,
            project_label,
            project_ref,
            input_tokens,
            cached_input_tokens,
            output_tokens,
            reasoning_output_tokens,
            total_tokens,
            updated_at
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
    )?;
    let updated_at = now_utc();
    for (key, rollup) in buckets {
        stmt.execute(params![
            key.source,
            key.model,
            key.hour_start,
            key.project_hash,
            rollup.project_label,
            rollup.project_ref,
            rollup.tokens.input_tokens,
            rollup.tokens.cached_input_tokens,
            rollup.tokens.output_tokens,
            rollup.tokens.reasoning_output_tokens,
            rollup.tokens.total_tokens,
            updated_at,
        ])?;
    }
    Ok(())
}
