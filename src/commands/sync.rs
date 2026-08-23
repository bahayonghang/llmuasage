use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{self, IsTerminal, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use tracing::info;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    app::AppContext,
    commands::{sync_progress, sync_summary},
    models::{ParseIssues, SourceKind},
    parsers::{SourceSyncStats, SyncEvent, SyncSummaryEvent, driver},
    registry,
    remote::{
        RemoteImporter, ShardSource, SshShardSource,
        protocol::{SHARD_PROTOCOL_VERSION, ShardRecord, encode_record},
    },
    store::{
        BootstrapProgressEvent, HolderKind, LOCAL_HOST_ID, SourceSyncStatus, Store,
        latest_schema_version,
    },
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

pub async fn run_once(_app: &AppContext, store: &Store, lock_wait_ms: u64) -> Result<SyncSummary> {
    run_once_with_options(_app, store, lock_wait_ms, &SyncRunOptions::default(), None).await
}

pub async fn run_store_once_with_options(
    store: &Store,
    options: &SyncRunOptions,
) -> Result<SyncSummary> {
    run_store_once_with_remote_source(store, options, &SshShardSource::default(), None).await
}

/// Like [`run_store_once_with_options`], with an injected shard source for tests.
pub async fn run_store_once_with_remote_source(
    store: &Store,
    options: &SyncRunOptions,
    remote_source: &dyn ShardSource,
    sender: Option<&mut mpsc::Sender<SyncEvent>>,
) -> Result<SyncSummary> {
    options.validate()?;
    let lock_started = Instant::now();
    let lock = store.acquire_worker_lock_with(Duration::from_secs(30), HolderKind::Cli)?;
    let fenced_store = lock.fenced_store();
    let heartbeat = lock.start_default_heartbeat();
    let lock_wait_ms = lock_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    fenced_store.bootstrap()?;
    fenced_store.run_log().recover_running_usage_import_runs()?;
    let command_name = if options.rebuild {
        "sync --rebuild"
    } else {
        "sync"
    };
    let cancel = CancellationToken::new();
    let summary = super::run_tracked(
        &fenced_store,
        command_name,
        async {
            run_once_locked_with_remote_source(
                &fenced_store,
                lock_wait_ms,
                options,
                sender,
                &cancel,
                remote_source,
            )
            .await
        },
        |item| Some(item.summary_text()),
    )
    .await?;
    drop(heartbeat);
    drop(lock);
    Ok(summary)
}

pub async fn run_once_with_options(
    app: &AppContext,
    store: &Store,
    lock_wait_ms: u64,
    options: &SyncRunOptions,
    sender: Option<&mut mpsc::Sender<SyncEvent>>,
) -> Result<SyncSummary> {
    run_once_with_cancel(
        app,
        store,
        lock_wait_ms,
        options,
        sender,
        &CancellationToken::new(),
    )
    .await
}

pub async fn run_once_with_cancel(
    _app: &AppContext,
    store: &Store,
    lock_wait_ms: u64,
    options: &SyncRunOptions,
    sender: Option<&mut mpsc::Sender<SyncEvent>>,
    cancel: &CancellationToken,
) -> Result<SyncSummary> {
    let operation = store.write_operation(HolderKind::Library)?;
    run_once_locked(&operation.store, lock_wait_ms, options, sender, cancel).await
}

/// CLI adapter that implements `SyncExecutor`.
///
/// `JobRegistry` receives an `Arc<dyn SyncExecutor>` rather than calling this
/// function directly, so the application layer no longer needs to import the
/// CLI adapter module (ARCH-002).
pub struct CommandSyncExecutor;

// Keep the existing public convenience constructor owned by the adapter layer.
// Composition roots inject explicitly; the sync/application layer remains
// independent of this concrete executor.
impl Default for crate::sync::JobRegistry {
    fn default() -> Self {
        Self::new(Arc::new(CommandSyncExecutor))
    }
}

impl crate::sync::executor::SyncExecutor for CommandSyncExecutor {
    fn run_once<'a>(
        &'a self,
        _app: &'a AppContext,
        store: &'a Store,
        lock_wait_ms: u64,
        options: &'a SyncRunOptions,
        sender: Option<&'a mut mpsc::Sender<SyncEvent>>,
        cancel: &'a CancellationToken,
    ) -> crate::sync::executor::BoxFuture<'a, anyhow::Result<SyncSummary>> {
        Box::pin(run_once_locked(
            store,
            lock_wait_ms,
            options,
            sender,
            cancel,
        ))
    }
}

/// Hosts that this `llmusage sync` run actually reached.
///
/// `contacted` is an in-memory set for this run. Do not compare
/// `host.last_contacted_at` to wall clock for missing-sweep or lossy-rebuild
/// control flow: same-second consecutive syncs lose that comparison
/// (`common/util.rs`).
struct RemoteRunOutcome {
    contacted: BTreeSet<String>,
    skipped: BTreeMap<String, String>,
}

async fn run_once_locked(
    store: &Store,
    lock_wait_ms: u64,
    options: &SyncRunOptions,
    sender: Option<&mut mpsc::Sender<SyncEvent>>,
    cancel: &CancellationToken,
) -> Result<SyncSummary> {
    run_once_locked_with_remote_source(
        store,
        lock_wait_ms,
        options,
        sender,
        cancel,
        &SshShardSource::default(),
    )
    .await
}

async fn run_once_locked_with_remote_source(
    store: &Store,
    lock_wait_ms: u64,
    options: &SyncRunOptions,
    mut sender: Option<&mut mpsc::Sender<SyncEvent>>,
    cancel: &CancellationToken,
    remote_source: &dyn ShardSource,
) -> Result<SyncSummary> {
    let request = options.validate()?;
    /*
     * ========================================================================
     * 步骤2：执行三阶段同步流水线
     * ========================================================================
     * 目标：
     * 1) 用 SourceParser 注册表替代硬列三连
     * 2) 由 driver 串行驱动并注入锁等待耗时
     * 3) 单 writer 顺序提交 reset / event / cursor
     * 4) 最后刷新每源诊断状态
     */
    info!("开始执行 sync 三阶段流水线");
    let pipeline_started = Instant::now();

    let parsers = registry::registered_parsers()
        .into_iter()
        .filter(|parser| {
            options
                .source
                .is_none_or(|source| parser.source() == source)
        })
        .collect::<Vec<_>>();
    let parser_sources = parsers
        .iter()
        .map(|parser| parser.source())
        .collect::<Vec<_>>();
    refuse_omp_before_pi_split_migration(store, &parser_sources)?;

    let mut remote_outcome = RemoteRunOutcome {
        contacted: BTreeSet::from([LOCAL_HOST_ID.to_string()]),
        skipped: BTreeMap::new(),
    };

    let automatic_repair_sources = if options.rebuild {
        reset_for_rebuild(store, options, &parser_sources, &remote_outcome.contacted)?;
        Vec::new()
    } else {
        let sources = automatic_token_accounting_repair_sources(
            store,
            options,
            &parser_sources,
            &remote_outcome.contacted,
        )?;
        if cancel.is_cancelled() {
            Vec::new()
        } else {
            if !sources.is_empty() {
                let source_names = source_names(&sources);
                tracing::warn!(
                    sources = %source_names,
                    "普通 sync 检测到 legacy token accounting，开始安全自动重建"
                );
                if let Some(sender) = sender.as_deref_mut() {
                    sender
                        .send(SyncEvent::TokenAccountingRepairStarted {
                            sources: sources.clone(),
                        })
                        .await?;
                }
                if let Err(error) = reset_sources_for_rebuild(store, &sources) {
                    tracing::error!(
                        sources = %source_names,
                        error = %error,
                        "legacy token accounting 自动重建 reset 失败"
                    );
                    return Err(error);
                }
            }
            sources
        }
    };

    // 2.1 计算并发度并按 source 顺序解析 + 即时写入
    let parallelism = request.parallelism();
    let recent_cutoff = request.recent_cutoff(chrono::Utc::now());
    let provider_index = crate::domain::provider_map::ProviderIndex::resolve_for_sync(
        options.provider_map.as_deref(),
    )?;
    let mut writer = store.begin_sync_run_with_provider_index(provider_index)?;
    let parserless_sources = match options.source {
        Some(source)
            if registry::source_descriptor(source)
                .is_some_and(|descriptor| !descriptor.capabilities.parser) =>
        {
            vec![source]
        }
        Some(_) => Vec::new(),
        None => registry::registered_source_descriptors()
            .iter()
            .filter(|descriptor| !descriptor.capabilities.parser)
            .map(|descriptor| descriptor.kind)
            .collect(),
    };
    let driver_started = Instant::now();
    let drive_result = driver::drive_with_events(driver::DriveContext {
        parsers: &parsers,
        store,
        writer: &mut writer,
        parallelism,
        lock_wait_ms,
        recent_cutoff,
        sender: sender.as_deref_mut(),
        cancel,
        sweep_host_ids: vec![LOCAL_HOST_ID.to_string()],
    })
    .await;
    let sources = match drive_result {
        Ok(sources) => sources,
        Err(error) => {
            if !automatic_repair_sources.is_empty() {
                tracing::error!(
                    sources = %source_names(&automatic_repair_sources),
                    error = %error,
                    "legacy token accounting 自动重建 parser/store 失败"
                );
            }
            return Err(error);
        }
    };
    tracing::debug!(
        driver_ms = driver_started.elapsed().as_millis() as u64,
        "driver finished"
    );
    let mut total_seen = 0usize;
    let mut total_inserted = 0usize;
    let mut sync_statuses = Vec::new();
    let mut source_stats = Vec::with_capacity(sources.len());

    let stored_query_started = Instant::now();
    let mut stored_queries = 0u64;
    for mut source in sources {
        total_seen += source.events_seen;
        total_inserted += source.events_inserted;
        source.stored_events = stored_events_for_source(store, source.source)?;
        stored_queries += 1;
        sync_statuses.push(SourceSyncStatus {
            source: source.source.as_str().to_string(),
            files_processed: source.files_processed as i64,
            changed_files: source.changed_files as i64,
            bytes_scanned: source.bytes_scanned as i64,
            events_seen: source.events_seen as i64,
            events_replayed: source.events_replayed as i64,
            events_inserted: source.events_inserted as i64,
            stored_events: source.stored_events as i64,
            token_accounting_version: Some(crate::store::expected_token_accounting_version(
                source.source,
            )),
            legacy_token_accounting: false,
            token_accounting_warning: None,
            parse_ms: source.parse_ms as i64,
            write_ms: source.write_ms as i64,
            lock_wait_ms: source.lock_wait_ms as i64,
            parse_issues: source.parse_issues.clone(),
            updated_at: crate::util::now_utc(),
        });
        source_stats.push(source);
    }
    for source in parserless_sources {
        let stored_events = stored_events_for_source(store, source)?;
        stored_queries += 1;
        sync_statuses.push(SourceSyncStatus {
            source: source.as_str().to_string(),
            files_processed: 0,
            changed_files: 0,
            bytes_scanned: 0,
            events_seen: 0,
            events_replayed: 0,
            events_inserted: 0,
            stored_events: stored_events as i64,
            token_accounting_version: store.token_accounting_version(source)?,
            legacy_token_accounting: store.has_legacy_token_accounting(source)?,
            token_accounting_warning: None,
            parse_ms: 0,
            write_ms: 0,
            lock_wait_ms: lock_wait_ms as i64,
            parse_issues: Default::default(),
            updated_at: crate::util::now_utc(),
        });
        source_stats.push(SourceSyncStats {
            source,
            stored_events,
            lock_wait_ms,
            ..SourceSyncStats::default()
        });
    }
    import_registered_remotes(
        store,
        &mut writer,
        sender.as_deref_mut(),
        cancel,
        remote_source,
        &mut remote_outcome,
    )
    .await?;
    if !remote_outcome.skipped.is_empty() {
        tracing::warn!(
            skipped = remote_outcome.skipped.len(),
            "skipped unreachable remote hosts"
        );
    }
    writer.finish_sync_run()?;
    tracing::debug!(
        stored_query_ms = stored_query_started.elapsed().as_millis() as u64,
        stored_queries,
        "stored_events queries finished"
    );
    if !cancel.is_cancelled() {
        for source in &source_stats {
            if registry::source_descriptor(source.source)
                .is_some_and(|descriptor| descriptor.capabilities.parser)
            {
                store.mark_current_token_accounting(source.source)?;
            }
        }
    }
    store
        .sync_status()
        .save_source_sync_statuses("local", &sync_statuses)?;
    if !automatic_repair_sources.is_empty() && !cancel.is_cancelled() {
        let source_names = source_names(&automatic_repair_sources);
        tracing::info!(
            sources = %source_names,
            "普通 sync 完成 legacy token accounting 安全自动重建"
        );
        if let Some(sender) = sender.as_deref_mut() {
            sender
                .send(SyncEvent::TokenAccountingRepairFinished {
                    sources: automatic_repair_sources.clone(),
                })
                .await?;
        }
    }
    if recent_cutoff.is_some() && !cancel.is_cancelled() {
        for source in &source_stats {
            store.sync_status().mark_recent_completed(
                source.source,
                "local",
                crate::util::now_utc(),
            )?;
            if let Some(sender) = sender.as_deref_mut() {
                sender
                    .send(SyncEvent::RecentReady {
                        source: source.source,
                    })
                    .await?;
            }
        }
    }

    let stored_events = stored_event_count(store, options.source)?;
    let stats = source_stats;
    tracing::debug!(
        pipeline_ms = pipeline_started.elapsed().as_millis() as u64,
        "sync pipeline finished"
    );
    info!("完成 sync 三阶段流水线");
    Ok(SyncSummary {
        sources: stats,
        total_seen,
        total_inserted,
        stored_events,
    })
}

fn stored_event_count(store: &Store, source: Option<SourceKind>) -> Result<usize> {
    let conn = store.open_connection()?;
    let count: i64 = if let Some(source) = source {
        conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = ?1",
            [source.as_str()],
            |row| row.get(0),
        )?
    } else {
        conn.query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?
    };
    Ok(count.max(0) as usize)
}

fn stored_events_for_source(store: &Store, source: SourceKind) -> Result<usize> {
    stored_event_count(store, Some(source))
}

fn reset_for_rebuild(
    store: &Store,
    options: &SyncRunOptions,
    parser_sources: &[SourceKind],
    contacted: &BTreeSet<String>,
) -> Result<()> {
    let rebuild_sources = rebuild_sources(options.source, parser_sources)?;
    assert_no_unattributed_antigravity_history(store, &rebuild_sources)?;
    assert_lossless_rebuild(store, options, &rebuild_sources, contacted)?;
    reset_sources_for_rebuild(store, &rebuild_sources)
}

/// Refuses any rebuild that would delete hook-era Antigravity history.
///
/// Those rows predate the passive parser, carry no `source_path_hash`
/// attribution, and do not exist in `conversations/*.db`, so once deleted they
/// are gone forever. The guard is absolute (not bypassed by
/// `--allow-lossy-rebuild`): export a backup first if you truly need to clear
/// them. Once no unattributed rows remain, rebuild behaves like any other
/// parser-backed source.
fn assert_no_unattributed_antigravity_history(
    store: &Store,
    rebuild_sources: &[SourceKind],
) -> Result<()> {
    if !rebuild_sources.contains(&SourceKind::Antigravity) {
        return Ok(());
    }
    let unattributed = store.unattributed_event_count(SourceKind::Antigravity)?;
    if unattributed == 0 {
        return Ok(());
    }
    bail!(
        "Refusing `sync --rebuild` for antigravity because {unattributed} stored event(s) are hook-era history without file attribution; they cannot be reconstructed from local artifacts and are not covered by --allow-lossy-rebuild. Export a backup first (e.g. `llmusage export`) if you intentionally want to drop them."
    )
}

fn reset_sources_for_rebuild(store: &Store, sources: &[SourceKind]) -> Result<()> {
    for source in sources {
        let source = *source;
        store.reset_for_source(source, "local")?;
        store.clear_token_accounting_version(source)?;
    }
    Ok(())
}

fn automatic_token_accounting_repair_sources(
    store: &Store,
    options: &SyncRunOptions,
    parser_sources: &[SourceKind],
    contacted: &BTreeSet<String>,
) -> Result<Vec<SourceKind>> {
    let legacy = legacy_token_accounting_sources_for(store, parser_sources)?;
    if legacy.is_empty() {
        return Ok(Vec::new());
    }
    let sources = source_names(&legacy);
    if options.recent_days.is_some() {
        tracing::warn!(
            sources = %sources,
            recent_days = options.recent_days,
            "bounded sync 拒绝自动重建 legacy token accounting"
        );
        bail!(
            "Refusing automatic token-accounting repair during bounded sync for source(s): {sources}. No source was reset. Run `llmusage sync` without --recent-days to perform a safe full-history repair, then retry the bounded sync."
        );
    }

    let risks = lossy_rebuild_risks(store, &legacy, contacted)?;
    if risks.is_empty() {
        return Ok(legacy);
    }
    let details = format_lossy_rebuild_risks(&risks);
    tracing::warn!(
        sources = %sources,
        risks = %details,
        risk_count = risks.len(),
        "普通 sync 的 legacy token accounting 自动重建存在数据丢失风险，已拒绝"
    );
    let mut message = format!(
        "Refusing automatic token-accounting repair because imported usage has missing source files ({details}). No source was reset and --allow-lossy-rebuild was not enabled automatically. Restore the source files and rerun `llmusage sync`, or explicitly run `llmusage sync --rebuild --source <source> --allow-lossy-rebuild` for each source whose unrebuildable history you intentionally accept clearing."
    );
    if legacy.contains(&SourceKind::Pi) {
        message.push_str(
            " If this includes pi after the Oh My Pi split, restore the original `.pi` session files or run a full `llmusage sync` once `.omp` files are present so those rows can land as `omp`.",
        );
    }
    bail!(message)
}

fn assert_lossless_rebuild(
    store: &Store,
    options: &SyncRunOptions,
    rebuild_sources: &[SourceKind],
    contacted: &BTreeSet<String>,
) -> Result<()> {
    if options.allow_lossy_rebuild {
        return Ok(());
    }

    let risks = lossy_rebuild_risks(store, rebuild_sources, contacted)?;
    if risks.is_empty() {
        return Ok(());
    }

    let details = format_lossy_rebuild_risks(&risks);
    bail!(
        "Refusing lossy sync --rebuild because imported usage has missing source files ({details}). \
Regular `llmusage sync` is safe: it marks missing source files for diagnostics but does not delete usage history. \
`llmusage sync --rebuild` first deletes rebuildable usage rows and cannot reconstruct records whose original source files are gone. \
Restore the source files or pass --allow-lossy-rebuild to explicitly accept clearing unrebuildable history."
    );
}

fn lossy_rebuild_risks(
    store: &Store,
    sources: &[SourceKind],
    contacted: &BTreeSet<String>,
) -> Result<Vec<crate::store::LossyRebuildRisk>> {
    let wanted = sources.iter().copied().collect::<BTreeSet<_>>();
    let mut risks = Vec::new();
    for risk in store.source_files().lossy_rebuild_risks()? {
        if !wanted.contains(&risk.source) {
            continue;
        }
        if risk.host_id != LOCAL_HOST_ID && !contacted.contains(&risk.host_id) {
            continue;
        }
        if risk.has_risk() {
            risks.push(risk);
        }
    }
    Ok(risks)
}

async fn import_registered_remotes(
    store: &Store,
    writer: &mut crate::store::SyncRunWriter,
    mut sender: Option<&mut mpsc::Sender<SyncEvent>>,
    cancel: &CancellationToken,
    remote_source: &dyn ShardSource,
    outcome: &mut RemoteRunOutcome,
) -> Result<()> {
    if cancel.is_cancelled() {
        return Ok(());
    }
    let hosts = store
        .hosts()
        .list()?
        .into_iter()
        .filter(|host| host.transport == "ssh")
        .collect::<Vec<_>>();
    if hosts.is_empty() {
        return Ok(());
    }
    let run_started_at = writer.run_started_at().to_string();
    for host in hosts {
        if cancel.is_cancelled() {
            break;
        }
        emit_sync_event(
            sender.as_deref_mut(),
            SyncEvent::RemoteHostStarted {
                host_id: host.host_id.clone(),
                label: host.label.clone(),
            },
        )
        .await?;
        match RemoteImporter::import(&host, store, writer, remote_source) {
            Ok(imported) => {
                for warning in &imported.warnings {
                    tracing::warn!(host_id = %host.host_id, "{warning}");
                }
                sweep_imported_host(store, &host.host_id, &imported.sources, &run_started_at)?;
                outcome.contacted.insert(host.host_id.clone());
                emit_sync_event(
                    sender.as_deref_mut(),
                    SyncEvent::RemoteHostFinished {
                        host_id: host.host_id.clone(),
                        label: host.label.clone(),
                        stats: imported.sources,
                    },
                )
                .await?;
            }
            Err(err) => {
                let reason = err.to_string();
                let _ = store.hosts().record_contact(&host.host_id, Some(&reason));
                outcome.skipped.insert(host.host_id.clone(), reason.clone());
                emit_sync_event(
                    sender.as_deref_mut(),
                    SyncEvent::RemoteHostSkipped {
                        host_id: host.host_id.clone(),
                        label: host.label.clone(),
                        reason,
                    },
                )
                .await?;
            }
        }
    }
    Ok(())
}

fn sweep_imported_host(
    store: &Store,
    host_id: &str,
    sources: &[SourceSyncStats],
    run_started_at: &str,
) -> Result<()> {
    for stats in sources {
        if stats.last_error.is_some() {
            info!(
                source = %stats.source,
                host_id,
                "source inventory incomplete; skipping missing sweep"
            );
            continue;
        }
        let swept = store
            .source_files()
            .sweep_missing(stats.source, host_id, run_started_at)?;
        if swept > 0 {
            info!(
                source = %stats.source,
                host_id,
                swept,
                "标记 missing 文件完成"
            );
        }
    }
    Ok(())
}

async fn emit_sync_event(
    sender: Option<&mut mpsc::Sender<SyncEvent>>,
    event: SyncEvent,
) -> Result<()> {
    if let Some(sender) = sender {
        sender.send(event).await?;
    }
    Ok(())
}

fn format_lossy_rebuild_risks(risks: &[crate::store::LossyRebuildRisk]) -> String {
    risks
        .iter()
        .map(|risk| {
            format!(
                "{}: missing_files={} protected_events={}",
                risk.source, risk.missing_file_count, risk.protected_event_count
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn source_names(sources: &[SourceKind]) -> String {
    sources
        .iter()
        .map(|source| source.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn legacy_token_accounting_sources(store: &Store) -> Result<Vec<SourceKind>> {
    let parser_sources = registry::registered_parsers()
        .into_iter()
        .map(|parser| parser.source())
        .collect::<Vec<_>>();
    legacy_token_accounting_sources_for(store, &parser_sources)
}

fn legacy_token_accounting_sources_for(
    store: &Store,
    parser_sources: &[SourceKind],
) -> Result<Vec<SourceKind>> {
    let mut legacy_sources = Vec::new();
    for source in parser_sources {
        if store.has_legacy_token_accounting(*source)? {
            legacy_sources.push(*source);
        }
    }
    Ok(legacy_sources)
}

fn refuse_omp_before_pi_split_migration(
    store: &Store,
    parser_sources: &[SourceKind],
) -> Result<()> {
    let selecting_omp_without_pi =
        parser_sources.contains(&SourceKind::Omp) && !parser_sources.contains(&SourceKind::Pi);
    if selecting_omp_without_pi && store.has_legacy_token_accounting(SourceKind::Pi)? {
        bail!(
            "Refusing `--source omp` because stored pi rows still use the pre-split token-accounting contract. Run `llmusage sync` with no `--source` and no `--recent-days` first so those rows can migrate to `omp`."
        );
    }
    Ok(())
}

fn rebuild_sources(
    selected_source: Option<SourceKind>,
    parser_sources: &[SourceKind],
) -> Result<Vec<SourceKind>> {
    if let Some(source) = selected_source {
        if !parser_sources.contains(&source) {
            bail!(
                "Cannot rebuild source `{source}` because it has no passive parser; historical usage was preserved."
            );
        }
        return Ok(vec![source]);
    }
    Ok(parser_sources.to_vec())
}

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
