use std::fs;

use anyhow::Result;
use rusqlite::Connection;
use tracing::info;

use super::Store;

impl Store {
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
