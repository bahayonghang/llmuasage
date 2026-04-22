use std::fs;

use anyhow::Result;
use rusqlite::Connection;

use crate::paths::AppPaths;

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
        fs::create_dir_all(&self.paths.root_dir)?;
        fs::create_dir_all(&self.paths.bin_dir)?;
        fs::create_dir_all(&self.paths.backups_dir)?;
        fs::create_dir_all(&self.paths.exports_dir)?;

        let conn = Connection::open(&self.paths.db_path)?;
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
            "#,
        )?;
        Ok(())
    }
}
