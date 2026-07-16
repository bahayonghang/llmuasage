use std::{
    io::{IsTerminal, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use tracing::info;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    app::AppContext,
    models::SourceKind,
    parsers::{SourceSyncStats, SyncEvent, SyncSummaryEvent, driver},
    registry,
    store::{BootstrapProgressEvent, HolderKind, SourceSyncStatus, Store},
};

#[derive(Debug, Clone)]
pub struct SyncSummary {
    pub sources: Vec<SourceSyncStats>,
    pub total_seen: usize,
    pub total_inserted: usize,
    pub stored_events: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SyncRunOptions {
    pub rebuild: bool,
    pub source: Option<SourceKind>,
    pub recent_days: Option<u32>,
    pub parallelism: Option<usize>,
    pub provider_map: Option<PathBuf>,
    pub json_events: bool,
    pub allow_lossy_rebuild: bool,
}

pub async fn run(app: &AppContext) -> Result<()> {
    run_with_options(app, SyncRunOptions::default()).await
}

pub async fn run_with_options(app: &AppContext, options: SyncRunOptions) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：执行全量本地真源同步
     * ========================================================================
     * 目标：
     * 1) 拿 SQLite 租约锁，避免 hook-run 与手动 sync 并发
     * 2) 并行解析 Codex、Claude、OpenCode 三类真源
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
    let mut progress = HumanProgress::new();
    progress.render(&SyncEvent::BootstrapStarted);
    let mut bootstrap_sink = |event: BootstrapProgressEvent| {
        progress.render(&SyncEvent::from(event));
    };
    store.bootstrap_with_progress(Some(&mut bootstrap_sink))?;
    progress.render(&SyncEvent::LockWaiting { timeout_ms: 30_000 });
    let lock_started = Instant::now();
    let lock = store.acquire_worker_lock_with(Duration::from_secs(30), HolderKind::Cli)?;
    let heartbeat = lock.start_default_heartbeat();
    let lock_wait_ms = lock_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    progress.render(&SyncEvent::LockAcquired {
        wait_ms: lock_wait_ms,
    });
    store
        .run_log()
        .recover_running_runs(&["sync", "hook-run"])?;
    let (mut tx, mut rx) = mpsc::channel(128);
    let reporter = tokio::spawn(async move {
        let mut progress = progress;
        while let Some(event) = rx.recv().await {
            progress.render(&event);
        }
    });

    let command_name = if options.rebuild {
        "sync --rebuild"
    } else {
        "sync"
    };
    let summary_result = super::run_tracked(
        store,
        command_name,
        async { run_once_with_options(app, store, lock_wait_ms, options, Some(&mut tx)).await },
        |item| {
            Some(format!(
                "sources={} seen={} inserted_delta={} stored_events={}",
                item.sources.len(),
                item.total_seen,
                item.total_inserted,
                item.stored_events
            ))
        },
    )
    .await;
    if let Err(err) = &summary_result {
        let _ = tx
            .send(SyncEvent::Failed {
                error: err.to_string(),
            })
            .await;
    }
    drop(tx);
    let _ = reporter.await;
    let summary = summary_result?;
    drop(heartbeat);
    drop(lock);
    print_summary(&summary, options);

    info!("完成全量本地真源同步");
    Ok(())
}

async fn run_with_json_events(
    app: &AppContext,
    store: &Store,
    options: &SyncRunOptions,
) -> Result<()> {
    let (mut tx, mut rx) = mpsc::channel(128);
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
        {
            let bootstrap_tx = tx.clone();
            let mut bootstrap_sink = move |event: BootstrapProgressEvent| {
                let _ = bootstrap_tx.try_send(SyncEvent::from(event));
            };
            store.bootstrap_with_progress(Some(&mut bootstrap_sink))?;
        }
        tx.send(SyncEvent::LockWaiting { timeout_ms: 30_000 })
            .await?;
        let lock_started = Instant::now();
        let lock = store.acquire_worker_lock_with(Duration::from_secs(30), HolderKind::Cli)?;
        let heartbeat = lock.start_default_heartbeat();
        let lock_wait_ms = lock_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        tx.send(SyncEvent::LockAcquired {
            wait_ms: lock_wait_ms,
        })
        .await?;
        store
            .run_log()
            .recover_running_runs(&["sync", "hook-run"])?;
        let command_name = if options.rebuild {
            "sync --rebuild"
        } else {
            "sync"
        };
        let summary = super::run_tracked(
            store,
            command_name,
            async { run_once_with_options(app, store, lock_wait_ms, options, Some(&mut tx)).await },
            |item| {
                Some(format!(
                    "sources={} seen={} inserted_delta={} stored_events={}",
                    item.sources.len(),
                    item.total_seen,
                    item.total_inserted,
                    item.stored_events
                ))
            },
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
    result.map(|_| ())
}

fn print_summary(summary: &SyncSummary, options: &SyncRunOptions) {
    println!("Sync finished:");
    if options.rebuild {
        println!("- rebuild: reset parser-backed usage state source by source before sync");
    }
    for item in &summary.sources {
        println!(
            "- {}: files={} changed={} skipped={} seen={} committed={} stored_events={}",
            item.source,
            item.files_processed,
            item.changed_files,
            item.skipped_files,
            item.events_seen,
            item.events_inserted,
            item.stored_events
        );
    }
    println!(
        "- totals: seen={} inserted_delta={} stored_events={}",
        summary.total_seen, summary.total_inserted, summary.stored_events
    );
}

pub async fn run_once(_app: &AppContext, store: &Store, lock_wait_ms: u64) -> Result<SyncSummary> {
    run_once_with_options(_app, store, lock_wait_ms, &SyncRunOptions::default(), None).await
}

pub async fn run_store_once_with_options(
    store: &Store,
    options: &SyncRunOptions,
) -> Result<SyncSummary> {
    store.bootstrap()?;
    let lock_started = Instant::now();
    let lock = store.acquire_worker_lock_with(Duration::from_secs(30), HolderKind::Cli)?;
    let heartbeat = lock.start_default_heartbeat();
    let lock_wait_ms = lock_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    store
        .run_log()
        .recover_running_runs(&["sync", "hook-run"])?;
    let command_name = if options.rebuild {
        "sync --rebuild"
    } else {
        "sync"
    };
    let cancel = CancellationToken::new();
    let summary = super::run_tracked(
        store,
        command_name,
        async { run_once_locked(store, lock_wait_ms, options, None, &cancel).await },
        |item| {
            Some(format!(
                "sources={} seen={} inserted_delta={} stored_events={}",
                item.sources.len(),
                item.total_seen,
                item.total_inserted,
                item.stored_events
            ))
        },
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
    run_once_locked(store, lock_wait_ms, options, sender, cancel).await
}

async fn run_once_locked(
    store: &Store,
    lock_wait_ms: u64,
    options: &SyncRunOptions,
    sender: Option<&mut mpsc::Sender<SyncEvent>>,
    cancel: &CancellationToken,
) -> Result<SyncSummary> {
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

    assert_token_accounting_write_allowed(store, options, &parser_sources)?;
    if options.rebuild {
        reset_for_rebuild(store, options, &parser_sources)?;
    }

    // 2.1 计算并发度并按 source 顺序解析 + 即时写入
    let default_parallelism = std::thread::available_parallelism()
        .map(|value| value.get().min(4))
        .unwrap_or(1);
    let parallelism = options.parallelism.unwrap_or(default_parallelism).max(1);
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
    let sources = driver::drive_with_events(driver::DriveContext {
        parsers: &parsers,
        store,
        writer: &mut writer,
        parallelism,
        lock_wait_ms,
        recent_days: options.recent_days,
        sender,
        cancel,
    })
    .await?;
    let mut total_seen = 0usize;
    let mut total_inserted = 0usize;
    let mut sync_statuses = Vec::new();
    let mut source_stats = Vec::with_capacity(sources.len());

    for mut source in sources {
        total_seen += source.events_seen;
        total_inserted += source.events_inserted;
        source.stored_events = stored_events_for_source(store, source.source)?;
        sync_statuses.push(SourceSyncStatus {
            source: source.source.as_str().to_string(),
            files_processed: source.files_processed as i64,
            changed_files: source.changed_files as i64,
            bytes_scanned: source.bytes_scanned as i64,
            events_seen: source.events_seen as i64,
            events_replayed: source.events_replayed as i64,
            events_inserted: source.events_inserted as i64,
            stored_events: source.stored_events as i64,
            token_accounting_version: Some(crate::store::TOKEN_ACCOUNTING_VERSION),
            legacy_token_accounting: false,
            token_accounting_warning: None,
            parse_ms: source.parse_ms as i64,
            write_ms: source.write_ms as i64,
            lock_wait_ms: source.lock_wait_ms as i64,
            updated_at: crate::util::now_utc(),
        });
        source_stats.push(source);
    }
    for source in parserless_sources {
        let stored_events = stored_events_for_source(store, source)?;
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
            updated_at: crate::util::now_utc(),
        });
        source_stats.push(SourceSyncStats {
            source,
            stored_events,
            lock_wait_ms,
            ..SourceSyncStats::default()
        });
    }
    writer.finish_sync_run()?;
    for source in &source_stats {
        if registry::source_descriptor(source.source)
            .is_some_and(|descriptor| descriptor.capabilities.parser)
        {
            store.mark_current_token_accounting(source.source)?;
        }
    }
    store
        .sync_status()
        .save_source_sync_statuses(&sync_statuses)?;

    let stored_events = stored_event_count(store, options.source)?;
    let stats = source_stats;
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
) -> Result<()> {
    let rebuild_sources = rebuild_sources(options.source, parser_sources);
    assert_lossless_rebuild(store, options, &rebuild_sources)?;
    if let Some(source) = options.source {
        store.reset_for_source(source)?;
        store.clear_token_accounting_version(source)?;
    } else {
        for source in rebuild_sources {
            store.reset_for_source(source)?;
            store.clear_token_accounting_version(source)?;
        }
    }
    Ok(())
}

fn assert_token_accounting_write_allowed(
    store: &Store,
    options: &SyncRunOptions,
    parser_sources: &[SourceKind],
) -> Result<()> {
    if options.rebuild {
        return Ok(());
    }
    let legacy = legacy_token_accounting_sources_for(store, parser_sources)?;
    if legacy.is_empty() {
        return Ok(());
    }
    let sources = legacy
        .iter()
        .map(|source| source.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "Refusing to mix legacy and current token accounting for source(s): {sources}. Existing reports remain readable but may use the legacy accounting contract. Run `llmusage sync --rebuild --source <source>` for each listed source. The existing lossy-rebuild guard remains active; --allow-lossy-rebuild is never enabled automatically."
    )
}

fn assert_lossless_rebuild(
    store: &Store,
    options: &SyncRunOptions,
    rebuild_sources: &[SourceKind],
) -> Result<()> {
    if options.allow_lossy_rebuild {
        return Ok(());
    }

    let mut risks = Vec::new();
    for source in rebuild_sources {
        let risk = store.source_files().lossy_rebuild_risk(*source)?;
        if risk.has_risk() {
            risks.push(risk);
        }
    }
    if risks.is_empty() {
        return Ok(());
    }

    let details = risks
        .iter()
        .map(|risk| {
            format!(
                "{}: missing_files={} protected_events={}",
                risk.source, risk.missing_file_count, risk.protected_event_count
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    bail!(
        "Refusing lossy sync --rebuild because imported usage has missing source files ({details}). \
Regular `llmusage sync` is safe: it marks missing source files for diagnostics but does not delete usage history. \
`llmusage sync --rebuild` first deletes rebuildable usage rows and cannot reconstruct records whose original source files are gone. \
Restore the source files or pass --allow-lossy-rebuild to explicitly accept clearing unrebuildable history."
    );
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

fn rebuild_sources(
    selected_source: Option<SourceKind>,
    parser_sources: &[SourceKind],
) -> Vec<SourceKind> {
    selected_source.map_or_else(|| parser_sources.to_vec(), |source| vec![source])
}

struct HumanProgress {
    stderr: std::io::Stderr,
    tty: bool,
    last_line_len: usize,
}

impl HumanProgress {
    fn new() -> Self {
        let stderr = std::io::stderr();
        let tty = stderr.is_terminal();
        Self {
            stderr,
            tty,
            last_line_len: 0,
        }
    }

    fn render(&mut self, event: &SyncEvent) {
        let Some(line) = human_progress_line(event) else {
            return;
        };
        if self.tty {
            let padding = self.last_line_len.saturating_sub(line.chars().count());
            let _ = write!(self.stderr, "\r{line}{}", " ".repeat(padding));
            let _ = self.stderr.flush();
            self.last_line_len = line.chars().count();
            if matches!(
                event,
                SyncEvent::SourceFinished { .. }
                    | SyncEvent::MigrationFinished { .. }
                    | SyncEvent::PricingBucketReconcileStarted { .. }
                    | SyncEvent::PricingUpgradeFinished { .. }
                    | SyncEvent::LockAcquired { .. }
            ) {
                let _ = writeln!(self.stderr);
                self.last_line_len = 0;
            }
        } else {
            let _ = writeln!(self.stderr, "{line}");
        }
    }
}

fn human_progress_line(event: &SyncEvent) -> Option<String> {
    match event {
        SyncEvent::BootstrapStarted => Some("检查数据库 schema 与定价目录...".to_string()),
        SyncEvent::MigrationStarted {
            version,
            name,
            latest_version,
        } => Some(format!(
            "升级数据库 schema v0 → v{latest_version}，正在执行 v{version} {name}..."
        )),
        SyncEvent::MigrationFinished {
            version,
            name,
            elapsed_ms,
        } => Some(format!(
            "数据库 schema v{version} {name} 完成（{elapsed_ms}ms）"
        )),
        SyncEvent::PricingUpgradeStarted {
            from_version,
            to_version,
            total_events,
        } => Some(format!(
            "升级定价目录 {from_version} → {to_version}：共 {total_events} 条事件..."
        )),
        SyncEvent::PricingUpgradeProgress {
            from_version,
            to_version,
            processed_events,
            total_events,
            elapsed_ms,
        } => {
            let percentage = if *total_events == 0 {
                100.0
            } else {
                *processed_events as f64 * 100.0 / *total_events as f64
            };
            Some(format!(
                "升级定价目录 {from_version} → {to_version}：已处理 {processed_events}/{total_events} 条（{percentage:.1}%），已用 {elapsed_ms}ms"
            ))
        }
        SyncEvent::PricingBucketReconcileStarted {
            to_version,
            bucket_count,
        } => Some(format!(
            "事件定价完成，正在对账 {bucket_count} 个 {to_version} 汇总桶..."
        )),
        SyncEvent::PricingUpgradeFinished {
            to_version,
            updated_events,
            bucket_count,
            deleted_orphan_buckets,
            elapsed_ms,
            ..
        } => Some(format!(
            "定价目录 {to_version} 升级完成：{updated_events} 条事件，{bucket_count} 个桶，清理 {deleted_orphan_buckets} 个孤立桶（{elapsed_ms}ms）"
        )),
        SyncEvent::LockWaiting { .. } => Some("等待 SQLite sync worker 锁...".to_string()),
        SyncEvent::LockAcquired { wait_ms } => {
            Some(format!("已获取 SQLite sync worker 锁（等待 {wait_ms}ms）"))
        }
        SyncEvent::SourceStarted {
            source,
            files_total,
        } => Some(format!(
            "{}: 扫描 {files_total} 个文件...",
            source_label(*source)
        )),
        SyncEvent::Progress {
            source,
            files_scanned,
            records_imported,
            ..
        } => Some(format!(
            "{}: 已处理 {files_scanned}，导入 {records_imported} 条",
            source_label(*source)
        )),
        SyncEvent::SourceFinished { source, stats } => Some(format!(
            "{}: 完成，文件 {} 个，跳过 {} 个，提交 {} 条",
            source_label(*source),
            stats.files_processed,
            stats.skipped_files,
            stats.events_inserted
        )),
        SyncEvent::Failed { error } => Some(format!("同步失败：{error}")),
        SyncEvent::Cancelled => Some("同步已取消".to_string()),
        SyncEvent::Started { .. } | SyncEvent::Finished { .. } | SyncEvent::RecentReady { .. } => {
            None
        }
    }
}

fn source_label(source: SourceKind) -> &'static str {
    match source {
        SourceKind::Codex => "Codex",
        SourceKind::Claude => "Claude",
        SourceKind::Opencode => "OpenCode",
        SourceKind::Antigravity => "Antigravity",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_progress_describes_pricing_upgrade_phases() {
        assert_eq!(
            human_progress_line(&SyncEvent::BootstrapStarted).as_deref(),
            Some("检查数据库 schema 与定价目录...")
        );
        assert_eq!(
            human_progress_line(&SyncEvent::PricingUpgradeProgress {
                from_version: "static-v1".to_string(),
                to_version: "static-v2".to_string(),
                processed_events: 25_000,
                total_events: 50_000,
                elapsed_ms: 1_234,
            })
            .as_deref(),
            Some("升级定价目录 static-v1 → static-v2：已处理 25000/50000 条（50.0%），已用 1234ms")
        );
        assert_eq!(
            human_progress_line(&SyncEvent::PricingBucketReconcileStarted {
                to_version: "static-v2".to_string(),
                bucket_count: 7_153,
            })
            .as_deref(),
            Some("事件定价完成，正在对账 7153 个 static-v2 汇总桶...")
        );
        assert_eq!(
            human_progress_line(&SyncEvent::PricingUpgradeFinished {
                from_version: "static-v1".to_string(),
                to_version: "static-v2".to_string(),
                updated_events: 539_146,
                bucket_count: 7_153,
                deleted_orphan_buckets: 2,
                elapsed_ms: 46_000,
            })
            .as_deref(),
            Some(
                "定价目录 static-v2 升级完成：539146 条事件，7153 个桶，清理 2 个孤立桶（46000ms）"
            )
        );
    }

    #[test]
    fn pricing_sync_event_serializes_as_additive_snake_case_ndjson() -> anyhow::Result<()> {
        let value = serde_json::to_value(SyncEvent::PricingUpgradeProgress {
            from_version: "static-v1".to_string(),
            to_version: "static-v2".to_string(),
            processed_events: 25_000,
            total_events: 50_000,
            elapsed_ms: 1_234,
        })?;

        assert_eq!(value["event"], "pricing_upgrade_progress");
        assert_eq!(value["processed_events"], 25_000);
        assert_eq!(value["total_events"], 50_000);
        Ok(())
    }
}
