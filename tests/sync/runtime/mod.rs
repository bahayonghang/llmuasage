//! Integration coverage for M2 raw archive, usage logs, jobs, and cancellation surfaces.

use std::{
    fs,
    future::Future,
    io::Write,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use llmusage::{
    AppPaths, Dashboard, QueryFilter,
    logging::read_recent_log_entries,
    models::{
        ActivityCategory, SourceKind, ToolKind, UsageEvent, UsageTokens, UsageToolCall, UsageTurn,
    },
    parsers::{SourceParser, SourceSyncStats, SyncEvent, driver},
    store::{BootstrapOptions, FileCursor, RawRecord, Store, SyncRunWriter, SyncShard},
    sync::{JobRegistry, JobStatus, SyncOptions},
};
use rusqlite::Connection;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn make_store() -> Result<(TempDir, Store)> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    Ok((temp, store))
}

struct SourceEnvFixture {
    _root: TempDir,
    _env: crate::test_env::ScopedEnv,
}

impl SourceEnvFixture {
    fn new() -> Result<Self> {
        let root = TempDir::new()?;
        let home = root.path().join("home");
        let codex_home = home.join(".codex");
        let opencode_home = root.path().join("opencode-home");
        fs::create_dir_all(codex_home.join("sessions"))?;
        fs::create_dir_all(&opencode_home)?;

        let env = crate::test_env::ScopedEnv::capture(&[
            "HOME",
            "USERPROFILE",
            "CODEX_HOME",
            "OPENCODE_HOME",
        ]);
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("USERPROFILE", &home);
            std::env::set_var("CODEX_HOME", &codex_home);
            std::env::set_var("OPENCODE_HOME", &opencode_home);
        }

        Ok(Self {
            _root: root,
            _env: env,
        })
    }
}

fn build_event(key: &str, event_at: &str, total_tokens: i64) -> UsageEvent {
    UsageEvent {
        event_key: key.to_string(),
        source: SourceKind::Codex,
        provider_label: String::new(),
        model: "gpt-5".to_string(),
        event_at: event_at.to_string(),
        hour_start: event_at.to_string(),
        tokens: UsageTokens {
            input_tokens: total_tokens,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens,
        },
        project: None,
        session: None,
        source_cost: None,
    }
}

fn build_file_cursor(file_path: &str, index: usize) -> FileCursor {
    FileCursor {
        cursor_key: file_path.to_string(),
        file_path: file_path.to_string(),
        file_fingerprint: format!("fingerprint-{index}"),
        file_size: 100 + index as u64,
        file_mtime_ns: index as i64,
        tail_signature: format!("tail-{index}"),
        offset: 100 + index as u64,
        last_total: None,
        last_model: Some("gpt-5".to_string()),
        updated_at: "2026-05-08T00:00:00Z".to_string(),
    }
}

fn seed_source_file(store: &Store, source: SourceKind, path: &str) -> Result<()> {
    let mut writer = store.begin_sync_run()?;
    writer.commit_shard(SyncShard {
        source,
        reset_path_hashes: Vec::new(),
        events: Vec::new(),
        cursors: Vec::new(),
        seen_file_paths: vec![path.to_string()],
        raw_records: Vec::new(),
        turns: Vec::new(),
        tool_calls: Vec::new(),
        ..SyncShard::new(source)
    })?;
    writer.finish_sync_run()?;
    Ok(())
}

fn seed_resettable_row(store: &Store, source: SourceKind, key_suffix: &str) -> Result<()> {
    let mut writer = store.begin_sync_run()?;
    let event_key = format!("{}:{key_suffix}", source.as_str());
    let event = UsageEvent {
        event_key: event_key.clone(),
        source,
        provider_label: String::new(),
        model: "gpt-5".to_string(),
        event_at: "2026-05-08T00:00:00Z".to_string(),
        hour_start: "2026-05-08T00:00:00Z".to_string(),
        tokens: UsageTokens {
            input_tokens: 1,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 1,
        },
        project: None,
        session: None,
        source_cost: None,
    };
    let turn = UsageTurn {
        turn_key: format!("turn:{event_key}"),
        source,
        session_id: None,
        source_path_hash: None,
        project_hash: None,
        primary_model: event.model.clone(),
        started_at: event.event_at.clone(),
        category: ActivityCategory::Exploration,
        has_edits: false,
        retries: 0,
        one_shot: false,
        call_count: 1,
        tokens: event.tokens.clone(),
    };
    let tool_call = UsageToolCall {
        tool_call_key: format!("tool:{event_key}:Read"),
        turn_key: Some(turn.turn_key.clone()),
        event_key: Some(event_key.clone()),
        source,
        session_id: None,
        source_path_hash: None,
        project_hash: None,
        model: Some(event.model.clone()),
        occurred_at: event.event_at.clone(),
        tool_name: "Read".to_string(),
        tool_kind: ToolKind::Read,
        mcp_server: None,
        mcp_tool: None,
        input_fingerprint: Some(format!("fp:{key_suffix}")),
        safe_preview: Some("Read preview".to_string()),
    };
    writer.commit_shard(SyncShard {
        source,
        reset_path_hashes: Vec::new(),
        events: vec![event],
        cursors: Vec::new(),
        seen_file_paths: vec![format!("/{}/{}.jsonl", source.as_str(), key_suffix)],
        raw_records: vec![RawRecord {
            event_key,
            raw_json: r#"{"raw":true}"#.to_string(),
        }],
        turns: vec![turn],
        tool_calls: vec![tool_call],
        ..SyncShard::new(source)
    })?;
    writer.finish_sync_run()?;
    Ok(())
}

fn count_rows(store: &Store, table: &str, where_sql: &str) -> Result<i64> {
    let conn = store.open_connection()?;
    let sql = format!("SELECT COUNT(*) FROM {table} {where_sql}");
    Ok(conn.query_row(&sql, [], |row| row.get(0))?)
}

/// Validates D11/F1.5 privacy default: raw archive schema exists after
/// bootstrap, but the meta flag starts off and raw payloads are discarded until
/// a caller explicitly opts in.
/// Validates F1.5 opt-in behaviour and the F4.3 logs join: once raw archive is
/// enabled, raw rows are written with the same event_key and can be surfaced by
/// `Dashboard::logs(include_raw_json=true)`.
/// Validates D26/F4.3 cursor pagination: records are ordered newest-first,
/// cursor round-trips through base64url JSON, and `include_total` counts the
/// full filtered set rather than the page size.
/// Validates the OpenCode-specific D11 rule by running the parser against a
/// real local `opencode.db`: the raw archive stores a JSON rendering of the
/// SQLite row, not an empty placeholder.
/// Validates D27: when a recent window is requested, `RecentReady` is emitted
/// after the requested bounded stage and persisted into `source_sync_status`.
/// Validates D20 subset rebuild semantics: source-filtered sync only sweeps
/// the selected source's file state, leaving unrelated sources intact.
/// Validates D20 reset semantics: `Store::reset_for_source` deletes rebuildable
/// rows for the selected source only and leaves unrelated source rows intact.
/// Validates ADR 0005 M2 lifecycle: JobRegistry starts a real sync task,
/// forwards observable events, and ends with a completed snapshot.
/// Validates cancellation is observable quickly without marking the job
/// finished before the worker has actually observed the cancellation request.
/// Validates D5's "already written data is retained" rule at parser/file
/// boundary granularity. A synthetic parser commits three file shards and
/// then requests cancellation; the driver stops before the fourth file,
/// leaving the first three events/cursors/source_file rows durable.
/// Validates the D5 SLA shape with pending file work: when cancellation is
/// requested after five file-boundary commits, the parser returns within
/// 1500ms and does not process the remaining files.
struct CancelAfterFilesParser {
    total_files: usize,
    cancel_after_files: usize,
    per_file_delay: Duration,
}

impl SourceParser for CancelAfterFilesParser {
    fn source(&self) -> SourceKind {
        SourceKind::Codex
    }

    fn parse<'a>(
        &'a self,
        _store: &'a Store,
        writer: &'a mut SyncRunWriter,
        _parallelism: usize,
        _recent_cutoff: Option<chrono::DateTime<chrono::Utc>>,
        cancel: &'a CancellationToken,
        mut progress: Option<llmusage::parsers::ProgressSink<'a>>,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>> {
        Box::pin(async move {
            let mut stats = SourceSyncStats {
                source: SourceKind::Codex,
                ..Default::default()
            };
            if let Some(progress) = progress.as_deref_mut() {
                progress(SyncEvent::SourceStarted {
                    source: SourceKind::Codex,
                    files_total: self.total_files as u64,
                });
            }
            for index in 0..self.total_files {
                if cancel.is_cancelled() {
                    break;
                }
                if !self.per_file_delay.is_zero() {
                    tokio::time::sleep(self.per_file_delay).await;
                }
                let file_path = format!("/codex/cancel-file-{index}.jsonl");
                let commit = writer.commit_shard(SyncShard {
                    source: SourceKind::Codex,
                    reset_path_hashes: Vec::new(),
                    events: vec![build_event(
                        &format!("codex:cancel-file-{index}"),
                        "2026-05-08T00:00:00Z",
                        1,
                    )],
                    cursors: vec![build_file_cursor(&file_path, index)],
                    seen_file_paths: vec![file_path],
                    raw_records: Vec::new(),
                    turns: Vec::new(),
                    tool_calls: Vec::new(),
                    ..SyncShard::new(SourceKind::Codex)
                })?;
                stats.files_processed += 1;
                stats.changed_files += 1;
                stats.events_seen += 1;
                stats.events_inserted += commit.events_inserted;
                stats.write_ms += commit.write_ms;
                if let Some(progress) = progress.as_deref_mut() {
                    progress(SyncEvent::Progress {
                        source: SourceKind::Codex,
                        files_scanned: stats.files_processed as u64,
                        records_imported: stats.events_inserted as u64,
                        current_file: Some(format!("/codex/cancel-file-{index}.jsonl")),
                    });
                }
                if stats.files_processed == self.cancel_after_files {
                    cancel.cancel();
                }
            }
            Ok(stats)
        })
    }
}

/// Validates the subprocess fallback surface: `llmusage sync --json-events`
/// emits parseable NDJSON lifecycle events on stdout.
/// Validates the human progress surface: piped (non-TTY) `llmusage sync`
/// writes no ANSI escape sequences to stderr, both with the default renderer
/// selection and with `LLMUSAGE_PROGRESS=off` forcing the line renderer.
struct OpencodeFixture {
    _root: TempDir,
    paths: AppPaths,
    opencode_home: PathBuf,
    _env: crate::test_env::ScopedEnv,
}

impl OpencodeFixture {
    fn new() -> Result<Self> {
        let root = TempDir::new()?;
        let home = root.path().join("home");
        let opencode_home = root.path().join("opencode-home");
        let llmusage_home = root.path().join(".llmusage");
        fs::create_dir_all(&home)?;
        fs::create_dir_all(&opencode_home)?;
        fs::create_dir_all(home.join(".claude").join("projects"))?;

        let env = crate::test_env::ScopedEnv::capture(&[
            "HOME",
            "USERPROFILE",
            "OPENCODE_HOME",
            "CODEX_HOME",
        ]);
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("USERPROFILE", &home);
            std::env::set_var("OPENCODE_HOME", &opencode_home);
            std::env::set_var("CODEX_HOME", home.join(".codex"));
        }

        Ok(Self {
            _root: root,
            paths: AppPaths::with_root(llmusage_home)?,
            opencode_home,
            _env: env,
        })
    }

    fn seed_opencode(&self, message_id: &str, time_created: i64, total_tokens: i64) -> Result<()> {
        let db_path = self.opencode_home.join("opencode.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS project(id TEXT PRIMARY KEY, worktree TEXT);
            CREATE TABLE IF NOT EXISTS session(id TEXT PRIMARY KEY, project_id TEXT);
            CREATE TABLE IF NOT EXISTS message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
            "#,
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO project(id, worktree) VALUES ('project-1', '/tmp/demo')",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO session(id, project_id) VALUES ('session-1', 'project-1')",
            [],
        )?;
        let message = serde_json::json!({
            "id": message_id,
            "role": "assistant",
            "modelID": "gpt-5",
            "tokens": {
                "input": total_tokens,
                "output": 0,
                "reasoning": 0,
                "cache": { "read": 0, "write": 0 }
            },
            "time": {
                "created": time_created,
                "completed": time_created
            }
        });
        conn.execute(
            "INSERT OR REPLACE INTO message(id, session_id, time_created, data) VALUES (?1, 'session-1', ?2, ?3)",
            (message_id, time_created, message.to_string()),
        )?;
        Ok(())
    }
}

mod jobs;
mod progress_io;
mod raw_archive;
mod recent;
mod reset;
