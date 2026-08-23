use std::{
    collections::HashMap,
    io::{self, IsTerminal, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::Result;
use tracing::info;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    app::AppContext,
    commands::{sync_progress, sync_summary},
    models::{ParseIssues, SourceKind},
    parsers::{SyncEvent, SyncSummaryEvent, driver},
    registry,
    remote::protocol::{SHARD_PROTOCOL_VERSION, ShardRecord, encode_record},
    store::{BootstrapProgressEvent, HolderKind, Store, latest_schema_version},
    util::{hash_string, now_utc},
};

// These types belong to the sync domain layer. Re-exported here so callers that
// already import `commands::sync` don't need to change.
pub use crate::sync::types::{SyncRunOptions, SyncSummary};

/// Hard service-side bound on parser concurrency (RES-001).
///
/// An unbounded `parallelism` lets a caller spawn an arbitrary number of
/// blocking parse tasks, which is a local DoS vector — and a remote one if any
/// write-path guard is bypassed. 32 is far above the useful range (the default
/// is `min(cpu, 4)`) while staying bounded.
pub use crate::sync::types::MAX_SYNC_PARALLELISM;

/// Validates and resolves the effective parser concurrency.
///
/// `None` resolves to `min(available_parallelism, 4)`. An explicit value must
/// be in `1..=MAX_SYNC_PARALLELISM`; anything outside is rejected rather than
/// silently clamped, so a caller that asked for 10_000 learns its request was
/// invalid instead of quietly getting 32.
pub fn normalize_parallelism(requested: Option<usize>) -> Result<usize> {
    crate::sync::ValidatedSyncRequest::new(crate::sync::SyncRequestInput {
        parallelism: requested,
        ..Default::default()
    })
    .map(|request| request.parallelism())
    .map_err(anyhow::Error::from)
}

/// Options for `llmusage sync --emit-shards`.
#[derive(Debug, Clone, Default)]
pub struct EmitShardOptions {
    pub source: Option<String>,
    pub parallelism: Option<usize>,
    pub since: Option<String>,
}

pub async fn run(app: &AppContext) -> Result<()> {
    run_with_options(app, SyncRunOptions::default()).await
}

/// Parse local sources and write NDJSON shards to stdout without opening the user DB.
pub async fn emit_shards(app: &AppContext, options: EmitShardOptions) -> Result<()> {
    emit_shards_to(app, options, io::stdout()).await
}

pub async fn emit_shards_to(
    _app: &AppContext,
    options: EmitShardOptions,
    out: impl Write + Send + 'static,
) -> Result<()> {
    let request = crate::sync::ValidatedSyncRequest::new(crate::sync::SyncRequestInput {
        source: options.source,
        parallelism: options.parallelism,
        ..Default::default()
    })?;
    let recent_cutoff = match options.since.as_deref() {
        None => None,
        Some(raw) => Some(
            chrono::DateTime::parse_from_rfc3339(raw)
                .map_err(|err| anyhow::anyhow!("invalid --since RFC3339 timestamp: {err}"))?
                .with_timezone(&chrono::Utc),
        ),
    };
    let store = Store::new_emit_only()?;
    let out = Arc::new(Mutex::new(out));
    write_record(
        &out,
        &ShardRecord::Header {
            shard_protocol: SHARD_PROTOCOL_VERSION,
            llmusage_version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: latest_schema_version(),
            emitted_at: now_utc(),
        },
    )?;
    let sink = Arc::clone(&out);
    let mut writer = store.begin_collect_run(move |shard| {
        write_record(&sink, &ShardRecord::Shard { shard }).map_err(|err| {
            crate::error::LlmusageError::ConfigInvalid {
                detail: err.to_string(),
            }
        })
    })?;
    let parsers = registry::registered_parsers()
        .into_iter()
        .filter(|parser| {
            request
                .source_kind()
                .is_none_or(|source| parser.source() == source)
        })
        .collect::<Vec<_>>();
    let cancel = CancellationToken::new();
    let sources = driver::drive_with_events(driver::DriveContext {
        parsers: &parsers,
        store: &store,
        writer: &mut writer,
        parallelism: request.parallelism(),
        lock_wait_ms: 0,
        recent_cutoff,
        sender: None,
        cancel: &cancel,
        sweep_host_ids: vec![crate::store::LOCAL_HOST_ID.to_string()],
    })
    .await?;
    writer.finish_sync_run()?;
    let mut parse_issues = ParseIssues::default();
    for stats in &sources {
        parse_issues.merge(stats.parse_issues.clone());
    }
    write_record(
        &out,
        &ShardRecord::Trailer {
            sources,
            parse_issues,
        },
    )?;
    Ok(())
}

fn write_record<W: Write>(out: &Arc<Mutex<W>>, record: &ShardRecord) -> Result<()> {
    let encoded = encode_record(record)?;
    let mut guard = out
        .lock()
        .map_err(|_| anyhow::anyhow!("emit-shards stdout lock was poisoned"))?;
    writeln!(guard, "{encoded}")?;
    guard.flush()?;
    Ok(())
}

pub async fn run_with_options(app: &AppContext, options: SyncRunOptions) -> Result<()> {
    options.validate()?;
    /*
     * ========================================================================
     * 步骤1：执行全量本地真源同步
     * ========================================================================
     * 目标：
     * 1) 拿 SQLite 租约锁，避免多个 sync worker 并发
     * 2) 并行解析已注册的本地真源
     * 3) 用单 writer 批量落库并记录 run_log
     */
    info!("开始执行全量本地真源同步");

    // 1.1 建立 store、申请租约锁、回收脏 run
    let store = Store::new(&app.paths)?;
    if options.json_events {
        run_with_json_events(app, &store, &options).await
    } else {
        run_with_human_events(app, &store, &options).await
    }
}

async fn run_with_human_events(
    app: &AppContext,
    store: &Store,
    options: &SyncRunOptions,
) -> Result<()> {
    // 渲染器与 guard 的生命周期属于命令函数本身：bootstrap/锁阶段的 `?`
    // 提前返回同样经 Drop 完成终端清理，不依赖 reporter task 是否已 spawn。
    let renderer = Arc::new(Mutex::new(sync_progress::stderr_renderer()));
    let _guard = sync_progress::TerminalGuard::new(Arc::clone(&renderer));
    let render_stats = Arc::new(Mutex::new(sync_progress::RenderStats::default()));
    let bootstrap_started = Instant::now();
    sync_progress::render_shared_timed(&renderer, &render_stats, &SyncEvent::BootstrapStarted);
    sync_progress::render_shared(&renderer, &SyncEvent::LockWaiting { timeout_ms: 30_000 });
    let lock_started = Instant::now();
    let lock = store.acquire_worker_lock_with(Duration::from_secs(30), HolderKind::Cli)?;
    let fenced_store = lock.fenced_store();
    let heartbeat = lock.start_default_heartbeat();
    let lock_wait_ms = lock_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    sync_progress::render_shared(
        &renderer,
        &SyncEvent::LockAcquired {
            wait_ms: lock_wait_ms,
        },
    );
    let bootstrap_renderer = Arc::clone(&renderer);
    let bootstrap_stats = Arc::clone(&render_stats);
    let mut bootstrap_sink = move |event: BootstrapProgressEvent| {
        sync_progress::render_shared_timed(
            &bootstrap_renderer,
            &bootstrap_stats,
            &SyncEvent::from(event),
        );
    };
    fenced_store.bootstrap_with_progress(Some(&mut bootstrap_sink))?;
    tracing::debug!(
        bootstrap_ms = bootstrap_started.elapsed().as_millis() as u64,
        "bootstrap finished"
    );
    fenced_store.run_log().recover_running_usage_import_runs()?;
    let (mut tx, mut rx) = mpsc::channel(128);
    let cancel = CancellationToken::new();
    let ctrl_c_tx = tx.clone();
    let ctrl_c_cancel = cancel.clone();
    let ctrl_c_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            ctrl_c_cancel.cancel();
            let _ = ctrl_c_tx.send(SyncEvent::Cancelled).await;
        }
    });
    let reporter_renderer = Arc::clone(&renderer);
    let reporter_stats = Arc::clone(&render_stats);
    let reporter = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            sync_progress::render_shared_timed(&reporter_renderer, &reporter_stats, &event);
        }
    });

    let command_name = if options.rebuild {
        "sync --rebuild"
    } else {
        "sync"
    };
    let summary_result = super::run_tracked(
        &fenced_store,
        command_name,
        async {
            run_once_with_cancel(
                app,
                &fenced_store,
                lock_wait_ms,
                options,
                Some(&mut tx),
                &cancel,
            )
            .await
        },
        |item| Some(item.summary_text()),
    )
    .await;
    if let Err(err) = &summary_result {
        let _ = tx
            .send(SyncEvent::Failed {
                error: err.to_string(),
            })
            .await;
    }
    // 先停掉 Ctrl-C 监听并等其资源释放：它持有的 tx 克隆随任务结束而 drop，
    // 否则 channel 永不关闭、reporter 永不退出（死锁）。
    ctrl_c_task.abort();
    let _ = ctrl_c_task.await;
    drop(tx);
    let _ = reporter.await;
    if let Ok(stats) = render_stats.lock() {
        tracing::debug!(
            render_calls = stats.calls,
            render_nanos = stats.nanos,
            render_ms = stats.nanos / 1_000_000,
            "progress render cost"
        );
    }
    let summary = summary_result?;
    drop(heartbeat);
    drop(lock);
    print_summary(&summary, options, store);

    info!("完成全量本地真源同步");
    Ok(())
}

async fn run_with_json_events(
    app: &AppContext,
    store: &Store,
    options: &SyncRunOptions,
) -> Result<()> {
    let (mut tx, mut rx) = mpsc::channel(128);
    // JSON 路径只接取消 token，不挂渲染器；driver 在多 parser 的取消边界自行
    // 发 Cancelled，单 parser（--source）取消时 NDJSON 以 finished 收尾。
    let cancel = CancellationToken::new();
    let ctrl_c_cancel = cancel.clone();
    let ctrl_c_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            ctrl_c_cancel.cancel();
        }
    });
    tx.send(SyncEvent::Started {
        job_id: "cli".to_string(),
        files_total: 0,
    })
    .await?;
    tx.send(SyncEvent::BootstrapStarted).await?;
    let collector = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            println!("{}", serde_json::to_string(&event)?);
        }
        Ok::<_, anyhow::Error>(())
    });

    let result = async {
        tx.send(SyncEvent::LockWaiting { timeout_ms: 30_000 })
            .await?;
        let lock_started = Instant::now();
        let lock = store.acquire_worker_lock_with(Duration::from_secs(30), HolderKind::Cli)?;
        let fenced_store = lock.fenced_store();
        let heartbeat = lock.start_default_heartbeat();
        let lock_wait_ms = lock_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        tx.send(SyncEvent::LockAcquired {
            wait_ms: lock_wait_ms,
        })
        .await?;
        {
            let bootstrap_tx = tx.clone();
            let mut bootstrap_sink = move |event: BootstrapProgressEvent| {
                let _ = bootstrap_tx.try_send(SyncEvent::from(event));
            };
            fenced_store.bootstrap_with_progress(Some(&mut bootstrap_sink))?;
        }
        fenced_store.run_log().recover_running_usage_import_runs()?;
        let command_name = if options.rebuild {
            "sync --rebuild"
        } else {
            "sync"
        };
        let summary = super::run_tracked(
            &fenced_store,
            command_name,
            async {
                run_once_with_cancel(
                    app,
                    &fenced_store,
                    lock_wait_ms,
                    options,
                    Some(&mut tx),
                    &cancel,
                )
                .await
            },
            |item| Some(item.summary_text()),
        )
        .await?;
        drop(heartbeat);
        drop(lock);
        Ok::<SyncSummary, anyhow::Error>(summary)
    }
    .await;

    match &result {
        Ok(summary) => {
            tx.send(SyncEvent::Finished {
                summary: SyncSummaryEvent {
                    sources: summary.sources.len(),
                    total_seen: summary.total_seen,
                    total_inserted: summary.total_inserted,
                    stored_events: summary.stored_events,
                },
            })
            .await?;
        }
        Err(err) => {
            tx.send(SyncEvent::Failed {
                error: err.to_string(),
            })
            .await?;
        }
    }
    drop(tx);
    collector.await??;
    // JSON 路径的 ctrl-c 任务不持 channel 克隆，abort 顺序无害；await 仅为与
    // human 路径对称、确保任务资源已释放。
    ctrl_c_task.abort();
    let _ = ctrl_c_task.await;
    result.map(|_| ())
}

fn print_summary(summary: &SyncSummary, options: &SyncRunOptions, store: &Store) {
    let color = std::io::stdout().is_terminal();
    let basenames = sample_basenames(store, summary);
    for line in sync_summary::format_summary_lines_with_basenames(
        summary,
        options.rebuild,
        color,
        terminal_width(),
        &basenames,
    ) {
        println!("{line}");
    }
}

fn sample_basenames(store: &Store, summary: &SyncSummary) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for stats in &summary.sources {
        if stats.parse_issues.samples.is_empty() {
            continue;
        }
        let Ok(cursors) = store.cursors().load_file_cursors(stats.source, "local") else {
            continue;
        };
        for cursor in cursors.into_values() {
            let raw = if cursor.file_path.is_empty() {
                cursor.cursor_key
            } else {
                cursor.file_path
            };
            let Some(name) = sync_summary::path_basename(&raw) else {
                continue;
            };
            map.insert(hash_string(&raw), name.to_string());
        }
    }
    map
}

/// Terminal column budget for the summary table: `COLUMNS` when set, otherwise
/// the detected terminal width or a 120-column default.
fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            crossterm::terminal::size()
                .ok()
                .map(|(width, _)| width as usize)
        })
        .unwrap_or(120)
        .max(60)
}

/// Compatibility wrapper for the historical command-layer path.
pub async fn run_once(app: &AppContext, store: &Store, lock_wait_ms: u64) -> Result<SyncSummary> {
    crate::sync::run_once(app, store, lock_wait_ms).await
}

/// Compatibility wrapper for the historical command-layer path.
pub async fn run_store_once_with_options(
    store: &Store,
    options: &SyncRunOptions,
) -> Result<SyncSummary> {
    crate::sync::run_store_once_with_options(store, options).await
}

/// Compatibility wrapper with an injected remote shard source for tests.
pub async fn run_store_once_with_remote_source(
    store: &Store,
    options: &SyncRunOptions,
    remote_source: &dyn crate::remote::ShardSource,
    sender: Option<&mut mpsc::Sender<SyncEvent>>,
) -> Result<SyncSummary> {
    crate::sync::run_store_once_with_remote_source(store, options, remote_source, sender).await
}

/// Compatibility wrapper for the historical command-layer path.
pub async fn run_once_with_options(
    app: &AppContext,
    store: &Store,
    lock_wait_ms: u64,
    options: &SyncRunOptions,
    sender: Option<&mut mpsc::Sender<SyncEvent>>,
) -> Result<SyncSummary> {
    crate::sync::run_once_with_options(app, store, lock_wait_ms, options, sender).await
}

/// Compatibility wrapper for the historical command-layer path.
pub async fn run_once_with_cancel(
    app: &AppContext,
    store: &Store,
    lock_wait_ms: u64,
    options: &SyncRunOptions,
    sender: Option<&mut mpsc::Sender<SyncEvent>>,
    cancel: &CancellationToken,
) -> Result<SyncSummary> {
    crate::sync::run_once_with_cancel(app, store, lock_wait_ms, options, sender, cancel).await
}

/// Compatibility alias for callers that still import the command-owned name.
pub use crate::sync::DefaultSyncExecutor as CommandSyncExecutor;

pub(crate) fn legacy_token_accounting_sources(store: &Store) -> Result<Vec<SourceKind>> {
    crate::sync::legacy_token_accounting_sources(store)
}

#[cfg(test)]
use crate::sync::engine::rebuild_sources;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallelism_none_resolves_to_sensible_default() {
        let p = normalize_parallelism(None).unwrap();
        assert!(p >= 1, "default must be at least 1, got {p}");
        assert!(p <= 4, "default must not exceed 4, got {p}");
    }

    #[test]
    fn parallelism_one_is_accepted() {
        assert_eq!(normalize_parallelism(Some(1)).unwrap(), 1);
    }

    #[test]
    fn parallelism_max_is_accepted() {
        assert_eq!(
            normalize_parallelism(Some(MAX_SYNC_PARALLELISM)).unwrap(),
            MAX_SYNC_PARALLELISM
        );
    }

    #[test]
    fn parallelism_zero_is_rejected() {
        let err = normalize_parallelism(Some(0)).unwrap_err();
        assert!(
            err.to_string().contains("invalid_parallelism"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parallelism_above_max_is_rejected() {
        let err = normalize_parallelism(Some(MAX_SYNC_PARALLELISM + 1)).unwrap_err();
        assert!(
            err.to_string().contains("invalid_parallelism"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parallelism_usize_max_is_rejected() {
        let err = normalize_parallelism(Some(usize::MAX)).unwrap_err();
        assert!(
            err.to_string().contains("invalid_parallelism"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rebuild_sources_rejects_parserless_historical_source() {
        let error = rebuild_sources(
            Some(SourceKind::Antigravity),
            &[SourceKind::Codex, SourceKind::Claude],
        )
        .expect_err("parserless history must never be selected for rebuild");

        assert!(error.to_string().contains("no passive parser"));
    }

    #[tokio::test]
    async fn emit_shards_does_not_open_or_lock_the_user_database() -> anyhow::Result<()> {
        let temp = tempfile::TempDir::new()?;
        let paths = crate::paths::AppPaths::with_root(temp.path().join(".llmusage"))?;
        let store = Store::new(&paths)?;
        let lock = store.acquire_worker_lock_with(Duration::from_secs(5), HolderKind::Cli)?;
        let fenced = lock.fenced_store();
        fenced.bootstrap()?;
        let mut writer = fenced.begin_sync_run()?;
        let mut shard = crate::store::SyncShard::new(SourceKind::Codex);
        shard.events.push(crate::models::UsageEvent {
            event_key: "codex:path:seed".to_string(),
            source: SourceKind::Codex,
            provider_label: String::new(),
            model: "gpt-5".to_string(),
            event_at: "2026-08-20T00:00:00Z".to_string(),
            hour_start: "2026-08-20T00:00:00Z".to_string(),
            tokens: crate::models::UsageTokens {
                input_tokens: 1,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 1,
                reasoning_output_tokens: 0,
                total_tokens: 2,
            },
            project: None,
            session: None,
            source_cost: None,
        });
        shard.seen_file_paths.push("/tmp/seed.jsonl".to_string());
        shard.cursors.push(crate::store::FileCursor {
            cursor_key: "/tmp/seed.jsonl".to_string(),
            file_path: "/tmp/seed.jsonl".to_string(),
            file_fingerprint: "fp".to_string(),
            file_size: 4,
            file_mtime_ns: 0,
            tail_signature: "tail".to_string(),
            offset: 4,
            last_total: None,
            last_model: None,
            updated_at: "2026-08-20T00:00:00Z".to_string(),
        });
        writer.commit_shard(shard)?;
        writer.finish_sync_run()?;
        drop(lock);

        let conn = store.open_connection()?;
        let schema_before = crate::store::read_schema_version(&conn)?;
        let events_before: i64 =
            conn.query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
        let files_before: i64 =
            conn.query_row("SELECT COUNT(*) FROM source_file", [], |row| row.get(0))?;
        let cursors_before: i64 =
            conn.query_row("SELECT COUNT(*) FROM source_cursor", [], |row| row.get(0))?;
        drop(conn);

        Store::reset_open_connection_counter();
        let app = AppContext {
            paths: paths.clone(),
            current_exe: std::env::current_exe()?,
        };
        let zcode_home = temp.path().join("zcode-empty");
        std::fs::create_dir_all(&zcode_home)?;
        let previous_zcode = std::env::var_os("ZCODE_HOME");
        unsafe {
            std::env::set_var("ZCODE_HOME", &zcode_home);
        }
        let emit_result = emit_shards_to(
            &app,
            EmitShardOptions {
                source: Some("zcode".to_string()),
                ..EmitShardOptions::default()
            },
            std::io::sink(),
        )
        .await;
        unsafe {
            match previous_zcode {
                Some(value) => std::env::set_var("ZCODE_HOME", value),
                None => std::env::remove_var("ZCODE_HOME"),
            }
        }
        emit_result?;
        assert_eq!(
            Store::open_connection_count(),
            0,
            "emit-shards must not open the user database"
        );

        let conn = store.open_connection()?;
        let schema_after = crate::store::read_schema_version(&conn)?;
        let events_after: i64 =
            conn.query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
        let files_after: i64 =
            conn.query_row("SELECT COUNT(*) FROM source_file", [], |row| row.get(0))?;
        let cursors_after: i64 =
            conn.query_row("SELECT COUNT(*) FROM source_cursor", [], |row| row.get(0))?;
        assert_eq!(schema_before, schema_after);
        assert_eq!(events_before, events_after);
        assert_eq!(files_before, files_after);
        assert_eq!(cursors_before, cursors_after);
        assert!(events_before > 0);
        assert!(files_before > 0);
        assert!(cursors_before > 0);
        assert!(store.current_worker_lock()?.is_none());
        Ok(())
    }
}
