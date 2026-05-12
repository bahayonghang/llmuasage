use std::{
    io::{IsTerminal, Write},
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
    sources,
    store::{HolderKind, MigrationProgressEvent, SourceSyncStatus, Store},
};

#[derive(Debug, Clone)]
pub struct SyncSummary {
    pub sources: Vec<SourceSyncStats>,
    pub total_seen: usize,
    pub total_inserted: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SyncRunOptions {
    pub rebuild: bool,
    pub source: Option<SourceKind>,
    pub recent_days: Option<u32>,
    pub parallelism: Option<usize>,
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
    let mut migration_sink = |event: MigrationProgressEvent| {
        progress.render(&match event {
            MigrationProgressEvent::Started(item) => SyncEvent::MigrationStarted {
                version: item.version,
                name: item.name.to_string(),
                latest_version: crate::store::latest_schema_version(),
            },
            MigrationProgressEvent::Finished(item) => SyncEvent::MigrationFinished {
                version: item.version,
                name: item.name.to_string(),
                elapsed_ms: item.elapsed_ms.unwrap_or_default(),
            },
        });
    };
    store.bootstrap_with_migration_events(Some(&mut migration_sink))?;
    progress.render(&SyncEvent::LockWaiting { timeout_ms: 30_000 });
    let lock_started = Instant::now();
    let lock = store.acquire_worker_lock_with(Duration::from_secs(30), HolderKind::Cli)?;
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
                "sources={} seen={} inserted={}",
                item.sources.len(),
                item.total_seen,
                item.total_inserted
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
            let migration_tx = tx.clone();
            let mut migration_sink = move |event: MigrationProgressEvent| {
                let sync_event = match event {
                    MigrationProgressEvent::Started(item) => SyncEvent::MigrationStarted {
                        version: item.version,
                        name: item.name.to_string(),
                        latest_version: crate::store::latest_schema_version(),
                    },
                    MigrationProgressEvent::Finished(item) => SyncEvent::MigrationFinished {
                        version: item.version,
                        name: item.name.to_string(),
                        elapsed_ms: item.elapsed_ms.unwrap_or_default(),
                    },
                };
                let _ = migration_tx.try_send(sync_event);
            };
            store.bootstrap_with_migration_events(Some(&mut migration_sink))?;
        }
        tx.send(SyncEvent::LockWaiting { timeout_ms: 30_000 })
            .await?;
        let lock_started = Instant::now();
        let lock = store.acquire_worker_lock_with(Duration::from_secs(30), HolderKind::Cli)?;
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
                    "sources={} seen={} inserted={}",
                    item.sources.len(),
                    item.total_seen,
                    item.total_inserted
                ))
            },
        )
        .await?;
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
        println!("- rebuild: reset usage rows, buckets, projects, and cursors before sync");
    }
    for item in &summary.sources {
        println!(
            "- {}: files={} changed={} seen={} inserted={}",
            item.source,
            item.files_processed,
            item.changed_files,
            item.events_seen,
            item.events_inserted
        );
    }
    println!(
        "- totals: seen={} inserted={}",
        summary.total_seen, summary.total_inserted
    );
}

pub async fn run_once(_app: &AppContext, store: &Store, lock_wait_ms: u64) -> Result<SyncSummary> {
    run_once_with_options(_app, store, lock_wait_ms, &SyncRunOptions::default(), None).await
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

    if options.rebuild {
        reset_for_rebuild(store, options)?;
    }

    // 2.1 计算并发度并按 source 顺序解析 + 即时写入
    let default_parallelism = std::thread::available_parallelism()
        .map(|value| value.get().min(4))
        .unwrap_or(1);
    let parallelism = options.parallelism.unwrap_or(default_parallelism).max(1);
    let mut writer = store.begin_sync_run()?;
    let parsers = sources::registered_parsers()
        .into_iter()
        .filter(|parser| {
            options
                .source
                .is_none_or(|source| parser.source() == source)
        })
        .collect::<Vec<_>>();
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

    for source in &sources {
        total_seen += source.events_seen;
        total_inserted += source.events_inserted;
        sync_statuses.push(SourceSyncStatus {
            source: source.source.as_str().to_string(),
            files_processed: source.files_processed as i64,
            changed_files: source.changed_files as i64,
            bytes_scanned: source.bytes_scanned as i64,
            events_seen: source.events_seen as i64,
            events_replayed: source.events_replayed as i64,
            events_inserted: source.events_inserted as i64,
            parse_ms: source.parse_ms as i64,
            write_ms: source.write_ms as i64,
            lock_wait_ms: source.lock_wait_ms as i64,
            updated_at: crate::util::now_utc(),
        });
    }
    writer.finish_sync_run()?;
    store
        .sync_status()
        .save_source_sync_statuses(&sync_statuses)?;

    let stats = sources;
    info!("完成 sync 三阶段流水线");
    Ok(SyncSummary {
        sources: stats,
        total_seen,
        total_inserted,
    })
}

fn reset_for_rebuild(store: &Store, options: &SyncRunOptions) -> Result<()> {
    assert_lossless_rebuild(store, options)?;
    if let Some(source) = options.source {
        store.reset_for_source(source)?;
    } else {
        store.reset_usage_data()?;
    }
    Ok(())
}

fn assert_lossless_rebuild(store: &Store, options: &SyncRunOptions) -> Result<()> {
    if options.allow_lossy_rebuild {
        return Ok(());
    }

    let risks = rebuild_guard_sources(options.source)
        .into_iter()
        .filter_map(|source| {
            let risk = store.source_files().lossy_rebuild_risk(source).ok()?;
            risk.has_risk().then_some(risk)
        })
        .collect::<Vec<_>>();
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

fn rebuild_guard_sources(source: Option<SourceKind>) -> Vec<SourceKind> {
    source.map_or_else(
        || {
            sources::registered_parsers()
                .into_iter()
                .map(|parser| parser.source())
                .collect()
        },
        |source| vec![source],
    )
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
        SyncEvent::BootstrapStarted => Some("初始化数据库...".to_string()),
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
            "{}: 完成，文件 {} 个，导入 {} 条",
            source_label(*source),
            stats.files_processed,
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
        SourceKind::Gemini => "Gemini",
    }
}
