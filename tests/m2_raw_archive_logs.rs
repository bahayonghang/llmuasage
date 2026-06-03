//! Integration coverage for M2 raw archive, usage logs, jobs, and cancellation surfaces.

use std::{
    fs,
    future::Future,
    path::PathBuf,
    pin::Pin,
    time::{Duration, Instant},
};

use anyhow::Result;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use llmusage::{
    AppPaths, Dashboard, QueryFilter,
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

fn build_event(key: &str, event_at: &str, total_tokens: i64) -> UsageEvent {
    UsageEvent {
        event_key: key.to_string(),
        source: SourceKind::Codex,
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
#[test]
fn raw_archive_off_by_default() -> Result<()> {
    let (_tmp, store) = make_store()?;
    assert!(!store.raw_archive_enabled()?);

    let mut writer = store.begin_sync_run()?;
    writer.commit_shard(SyncShard {
        source: SourceKind::Codex,
        reset_path_hashes: Vec::new(),
        events: vec![build_event("codex:raw-off", "2026-05-08T00:00:00Z", 10)],
        cursors: Vec::new(),
        seen_file_paths: Vec::new(),
        raw_records: vec![RawRecord {
            event_key: "codex:raw-off".to_string(),
            raw_json: r#"{"secret":"local-only"}"#.to_string(),
        }],
        turns: Vec::new(),
        tool_calls: Vec::new(),
    })?;
    writer.finish_sync_run()?;

    let conn = store.open_connection()?;
    let raw_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM usage_event_raw", [], |row| row.get(0))?;
    assert_eq!(raw_count, 0);
    Ok(())
}

/// Validates F1.5 opt-in behaviour and the F4.3 logs join: once raw archive is
/// enabled, raw rows are written with the same event_key and can be surfaced by
/// `Dashboard::logs(include_raw_json=true)`.
#[test]
fn raw_archive_opt_in_is_returned_by_logs() -> Result<()> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap_with(BootstrapOptions::default().with_raw_archive(true))?;
    assert!(store.raw_archive_enabled()?);

    let mut writer = store.begin_sync_run()?;
    writer.commit_shard(SyncShard {
        source: SourceKind::Codex,
        reset_path_hashes: Vec::new(),
        events: vec![build_event("codex:raw-on", "2026-05-08T01:00:00Z", 11)],
        cursors: Vec::new(),
        seen_file_paths: Vec::new(),
        raw_records: vec![RawRecord {
            event_key: "codex:raw-on".to_string(),
            raw_json: r#"{"payload":"retained"}"#.to_string(),
        }],
        turns: Vec::new(),
        tool_calls: Vec::new(),
    })?;
    writer.finish_sync_run()?;

    let page = Dashboard::open(&store)?.logs(&llmusage::LogsQuery {
        include_raw_json: true,
        ..Default::default()
    })?;
    assert_eq!(page.records.len(), 1);
    assert_eq!(
        page.records[0].raw_json.as_deref(),
        Some(r#"{"payload":"retained"}"#)
    );
    Ok(())
}

/// Validates D26/F4.3 cursor pagination: records are ordered newest-first,
/// cursor round-trips through base64url JSON, and `include_total` counts the
/// full filtered set rather than the page size.
#[test]
fn logs_cursor_round_trip() -> Result<()> {
    let (_tmp, store) = make_store()?;
    let mut writer = store.begin_sync_run()?;
    writer.commit_shard(SyncShard {
        source: SourceKind::Codex,
        reset_path_hashes: Vec::new(),
        events: vec![
            build_event("codex:old", "2026-05-08T00:00:00Z", 1),
            build_event("codex:middle", "2026-05-08T01:00:00Z", 2),
            build_event("codex:new", "2026-05-08T02:00:00Z", 3),
        ],
        cursors: Vec::new(),
        seen_file_paths: Vec::new(),
        raw_records: Vec::new(),
        turns: Vec::new(),
        tool_calls: Vec::new(),
    })?;
    writer.finish_sync_run()?;

    let dashboard = Dashboard::open(&store)?;
    let first = dashboard.logs(&llmusage::LogsQuery {
        filter: QueryFilter {
            source: Some(SourceKind::Codex),
            ..Default::default()
        },
        page_size: 2,
        include_total: true,
        ..Default::default()
    })?;
    assert_eq!(first.total, Some(3));
    assert_eq!(
        first
            .records
            .iter()
            .map(|record| record.event_key.as_str())
            .collect::<Vec<_>>(),
        vec!["codex:new", "codex:middle"]
    );
    let cursor = first.next_cursor.expect("first page should have cursor");
    let decoded: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&cursor)?)?;
    assert_eq!(decoded["event_at"], "2026-05-08T01:00:00Z");
    assert_eq!(decoded["event_key"], "codex:middle");

    let second = dashboard.logs(&llmusage::LogsQuery {
        filter: QueryFilter {
            source: Some(SourceKind::Codex),
            ..Default::default()
        },
        page_size: 2,
        cursor: Some(cursor),
        ..Default::default()
    })?;
    assert_eq!(second.next_cursor, None);
    assert_eq!(second.records.len(), 1);
    assert_eq!(second.records[0].event_key, "codex:old");
    Ok(())
}

/// Validates the OpenCode-specific D11 rule by running the parser against a
/// real local `opencode.db`: the raw archive stores a JSON rendering of the
/// SQLite row, not an empty placeholder.
#[tokio::test]
async fn opencode_row_serialized_as_json_in_raw_table() -> Result<()> {
    let fixture = OpencodeFixture::new()?;
    fixture.seed_opencode("msg-raw", 1776823200000, 64)?;

    let store = Store::new(&fixture.paths)?;
    store.bootstrap_with(BootstrapOptions::default().with_raw_archive(true))?;
    let app = llmusage::app::AppContext {
        paths: fixture.paths.clone(),
        current_exe: std::env::current_exe()?,
    };
    llmusage::commands::sync::run(&app).await?;

    let conn = store.open_connection()?;
    let raw_json: String = conn.query_row(
        "SELECT raw_json FROM usage_event_raw WHERE event_key = 'opencode:msg-raw'",
        [],
        |row| row.get(0),
    )?;
    let value: serde_json::Value = serde_json::from_str(&raw_json)?;
    assert_eq!(value["id"], "msg-raw");
    assert_eq!(value["session_id"], "session-1");
    assert_eq!(value["data"]["modelID"], "gpt-5");
    Ok(())
}

/// Validates D27: when a recent window is requested, `RecentReady` is emitted
/// independently per source and persisted into `source_sync_status` without
/// waiting for a final full-history marker.
#[tokio::test]
async fn recent_ready_emitted_per_source_when_recent_days_set() -> Result<()> {
    let (_tmp, store) = make_store()?;
    let app = llmusage::app::AppContext {
        paths: store.paths.clone(),
        current_exe: std::env::current_exe()?,
    };
    let (mut tx, mut rx) = tokio::sync::mpsc::channel(32);

    let summary = llmusage::commands::sync::run_once_with_options(
        &app,
        &store,
        0,
        &llmusage::commands::sync::SyncRunOptions {
            source: Some(SourceKind::Codex),
            recent_days: Some(30),
            ..Default::default()
        },
        Some(&mut tx),
    )
    .await?;
    drop(tx);

    assert_eq!(summary.sources.len(), 1);
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    assert!(events.iter().any(|event| {
        matches!(
            event,
            SyncEvent::RecentReady {
                source: SourceKind::Codex
            }
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            SyncEvent::SourceFinished {
                source: SourceKind::Codex,
                ..
            }
        )
    }));

    let diagnostics = Dashboard::open(&store)?.diagnostics()?;
    let codex = diagnostics
        .by_source
        .iter()
        .find(|row| row.source == "codex")
        .expect("codex diagnostics row");
    assert!(codex.recent_completed_at.is_some());
    Ok(())
}

/// Validates D20 subset rebuild semantics: source-filtered sync only sweeps
/// the selected source's file state, leaving unrelated sources intact.
#[tokio::test]
async fn source_filtered_sync_keeps_other_sources_intact() -> Result<()> {
    let (_tmp, store) = make_store()?;
    seed_source_file(&store, SourceKind::Codex, "/codex/stale.jsonl")?;
    seed_source_file(&store, SourceKind::Claude, "/claude/keep.jsonl")?;
    assert_eq!(store.source_files().counts(SourceKind::Codex)?.live, 1);
    assert_eq!(store.source_files().counts(SourceKind::Claude)?.live, 1);

    let app = llmusage::app::AppContext {
        paths: store.paths.clone(),
        current_exe: std::env::current_exe()?,
    };
    let summary = llmusage::commands::sync::run_once_with_options(
        &app,
        &store,
        0,
        &llmusage::commands::sync::SyncRunOptions {
            source: Some(SourceKind::Codex),
            ..Default::default()
        },
        None,
    )
    .await?;
    assert_eq!(summary.sources.len(), 1);

    let codex = store.source_files().counts(SourceKind::Codex)?;
    let claude = store.source_files().counts(SourceKind::Claude)?;
    assert_eq!(codex.missing, 1);
    assert_eq!(claude.live, 1);
    assert_eq!(claude.missing, 0);
    Ok(())
}

/// Validates D20 reset semantics: `Store::reset_for_source` deletes rebuildable
/// rows for the selected source only and leaves unrelated source rows intact.
#[test]
fn reset_for_source_codex_keeps_claude_intact() -> Result<()> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap_with(BootstrapOptions::default().with_raw_archive(true))?;
    seed_resettable_row(&store, SourceKind::Codex, "reset-me")?;
    seed_resettable_row(&store, SourceKind::Claude, "keep-me")?;

    store.reset_for_source(SourceKind::Codex)?;
    let conn = store.open_connection()?;
    let codex_events: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
        [],
        |row| row.get(0),
    )?;
    let claude_events: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'claude'",
        [],
        |row| row.get(0),
    )?;
    let codex_raw: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event_raw WHERE event_key LIKE 'codex:%'",
        [],
        |row| row.get(0),
    )?;
    let claude_raw: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event_raw WHERE event_key LIKE 'claude:%'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(codex_events, 0);
    assert_eq!(codex_raw, 0);
    assert_eq!(
        count_rows(&store, "usage_turn", "WHERE source = 'codex'")?,
        0
    );
    assert_eq!(
        count_rows(&store, "usage_tool_call", "WHERE source = 'codex'")?,
        0
    );
    assert_eq!(claude_events, 1);
    assert_eq!(claude_raw, 1);
    assert_eq!(
        count_rows(&store, "usage_turn", "WHERE source = 'claude'")?,
        1
    );
    assert_eq!(
        count_rows(&store, "usage_tool_call", "WHERE source = 'claude'")?,
        1
    );
    assert_eq!(store.source_files().counts(SourceKind::Codex)?.live, 0);
    assert_eq!(store.source_files().counts(SourceKind::Claude)?.live, 1);
    Ok(())
}

#[test]
fn reset_usage_data_clears_behavior_facts() -> Result<()> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap_with(BootstrapOptions::default().with_raw_archive(true))?;
    seed_resettable_row(&store, SourceKind::Codex, "reset-codex")?;
    seed_resettable_row(&store, SourceKind::Claude, "reset-claude")?;

    store.reset_usage_data()?;

    assert_eq!(count_rows(&store, "usage_event", "")?, 0);
    assert_eq!(count_rows(&store, "usage_event_raw", "")?, 0);
    assert_eq!(count_rows(&store, "usage_turn", "")?, 0);
    assert_eq!(count_rows(&store, "usage_tool_call", "")?, 0);
    Ok(())
}

/// Validates ADR 0005 M2 lifecycle: JobRegistry starts a real sync task,
/// forwards observable events, and ends with a completed snapshot.
#[tokio::test]
async fn start_run_complete_lifecycle_observable_via_snapshot() -> Result<()> {
    let (_tmp, store) = make_store()?;
    let registry = JobRegistry::default();
    let (job_id, mut rx) = registry.start(
        &store,
        SyncOptions {
            source: Some("codex".to_string()),
            ..Default::default()
        },
    );

    let mut saw_finished = false;
    while let Some(event) = rx.recv().await {
        if matches!(event, SyncEvent::Finished { .. }) {
            saw_finished = true;
            break;
        }
    }
    assert!(saw_finished, "job should forward Finished event");

    let snapshot = registry.snapshot(&job_id).expect("job snapshot");
    assert_eq!(snapshot.status, JobStatus::Completed);
    assert!(snapshot.summary.is_some());
    assert!(snapshot.finished_at.is_some());
    assert!(matches!(
        snapshot.last_event,
        Some(SyncEvent::Finished { .. }) | Some(SyncEvent::SourceFinished { .. })
    ));
    Ok(())
}

/// Validates cancellation is observable quickly without marking the job
/// finished before the worker has actually observed the cancellation request.
#[tokio::test]
async fn cancel_within_1500ms() -> Result<()> {
    let (_tmp, store) = make_store()?;
    let blocker = store
        .acquire_worker_lock_with(Duration::from_secs(0), llmusage::store::HolderKind::Library)?;
    let registry = JobRegistry::default();
    let (job_id, mut rx) = registry.start(
        &store,
        SyncOptions {
            source: Some("antigravity".to_string()),
            ..Default::default()
        },
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(event) = rx.recv().await {
            if matches!(event, SyncEvent::LockWaiting { .. }) {
                return;
            }
        }
    })
    .await?;

    let started = std::time::Instant::now();
    assert!(registry.cancel(&job_id));
    let snapshot = registry.snapshot(&job_id).expect("job snapshot");
    assert_eq!(snapshot.status, JobStatus::Cancelling);
    assert!(snapshot.finished_at.is_none());
    drop(blocker);
    loop {
        let snapshot = registry.snapshot(&job_id).expect("job snapshot");
        if snapshot.status == JobStatus::Cancelled {
            break;
        }
        if started.elapsed() >= std::time::Duration::from_millis(1500) {
            anyhow::bail!("job did not reach cancelled state within 1500ms");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(started.elapsed() < std::time::Duration::from_millis(1500));
    Ok(())
}

/// Validates D5's "already written data is retained" rule at parser/file
/// boundary granularity. A synthetic parser commits three file shards and
/// then requests cancellation; the driver stops before the fourth file,
/// leaving the first three events/cursors/source_file rows durable.
#[tokio::test]
async fn file_boundary_cancel_preserves_written_events() -> Result<()> {
    let (_tmp, store) = make_store()?;
    let cancel = CancellationToken::new();
    let parser = CancelAfterFilesParser {
        total_files: 10,
        cancel_after_files: 3,
        per_file_delay: Duration::from_millis(0),
    };
    let parsers: Vec<Box<dyn SourceParser>> = vec![Box::new(parser)];
    let (mut tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut writer = store.begin_sync_run()?;

    let stats = driver::drive_with_events(driver::DriveContext {
        parsers: &parsers,
        store: &store,
        writer: &mut writer,
        parallelism: 1,
        lock_wait_ms: 0,
        recent_days: None,
        sender: Some(&mut tx),
        cancel: &cancel,
    })
    .await?;
    writer.finish_sync_run()?;
    drop(tx);

    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].files_processed, 3);
    assert_eq!(stats[0].events_inserted, 3);
    assert!(cancel.is_cancelled());

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SyncEvent::SourceFinished { .. })),
        "driver should finish the source with partial stats"
    );

    let conn = store.open_connection()?;
    let event_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
        [],
        |row| row.get(0),
    )?;
    let cursor_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM source_cursor WHERE source = 'codex'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(event_count, 3);
    assert_eq!(cursor_count, 3);
    assert!(store.source_files().counts(SourceKind::Codex)?.live >= 3);

    let imported_keys = (0..10)
        .filter_map(|index| {
            conn.query_row(
                "SELECT event_key FROM usage_event WHERE event_key = ?1",
                [format!("codex:cancel-file-{index}")],
                |row| row.get::<_, String>(0),
            )
            .ok()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        imported_keys,
        vec![
            "codex:cancel-file-0".to_string(),
            "codex:cancel-file-1".to_string(),
            "codex:cancel-file-2".to_string()
        ]
    );
    Ok(())
}

/// Validates the D5 SLA shape with pending file work: when cancellation is
/// requested after five file-boundary commits, the parser returns within
/// 1500ms and does not process the remaining files.
#[tokio::test]
async fn cancel_within_1500ms_with_5_pending_files() -> Result<()> {
    let (_tmp, store) = make_store()?;
    let cancel = CancellationToken::new();
    let parser = CancelAfterFilesParser {
        total_files: 10,
        cancel_after_files: 5,
        per_file_delay: Duration::from_millis(20),
    };
    let started = Instant::now();
    let mut writer = store.begin_sync_run()?;
    let stats = parser.parse(&store, &mut writer, 1, &cancel, None).await?;
    writer.finish_sync_run()?;

    assert!(started.elapsed() < Duration::from_millis(1500));
    assert!(cancel.is_cancelled());
    assert_eq!(stats.files_processed, 5);
    assert_eq!(stats.events_inserted, 5);

    let conn = store.open_connection()?;
    let event_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(event_count, 5);
    Ok(())
}

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
#[test]
fn json_events_subprocess_emits_ndjson_per_event() -> Result<()> {
    let temp = TempDir::new()?;
    let home = temp.path().join("home");
    let root = temp.path().join(".llmusage");
    fs::create_dir_all(&home)?;
    Store::new(&AppPaths::with_root(root.clone())?)?.bootstrap()?;

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_llmusage"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("--home")
        .arg(&root)
        .args(["sync", "--source", "codex", "--json-events"])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("CODEX_HOME", home.join(".codex"))
        .env("OPENCODE_HOME", home.join("opencode"))
        .env("LLMUSAGE_LOG", "info")
        .env("RUST_LOG", "off")
        .output()?;
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8(output.stdout)?;
    let stdout_lines = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    assert!(
        stdout_lines
            .iter()
            .all(|line| line.trim_start().starts_with('{')),
        "sync --json-events stdout must contain only NDJSON lifecycle events: {stdout}"
    );
    let json_lines = stdout_lines
        .iter()
        .map(|line| serde_json::from_str::<serde_json::Value>(line))
        .collect::<serde_json::Result<Vec<_>>>()?;
    assert!(json_lines.iter().any(|line| line["event"] == "started"));
    assert!(
        json_lines
            .iter()
            .any(|line| line["event"] == "bootstrap_started")
    );
    assert!(
        json_lines
            .iter()
            .any(|line| line["event"] == "lock_waiting")
    );
    assert!(
        json_lines
            .iter()
            .any(|line| line["event"] == "source_started")
    );
    assert!(
        json_lines
            .iter()
            .any(|line| line["event"] == "lock_acquired")
    );
    assert!(json_lines.iter().any(|line| line["event"] == "finished"));
    Ok(())
}

struct OpencodeFixture {
    _root: TempDir,
    paths: AppPaths,
    opencode_home: PathBuf,
    saved: Vec<(String, Option<String>)>,
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

        let mut saved = Vec::new();
        for key in ["HOME", "USERPROFILE", "OPENCODE_HOME", "CODEX_HOME"] {
            saved.push((key.to_string(), std::env::var(key).ok()));
        }
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
            saved,
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

impl Drop for OpencodeFixture {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            unsafe {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }
}
