//! Sync application engine: parser selection, rebuild/repair, remote import,
//! status persistence, and summary construction.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::{
    app::AppContext,
    models::SourceKind,
    parsers::{SourceSyncStats, SyncEvent, driver},
    registry,
    remote::{RemoteImporter, ShardSource, SshShardSource},
    store::{HolderKind, LOCAL_HOST_ID, SourceSyncStatus, Store, SyncStatusStore},
    sync::types::{SyncRunOptions, SyncSummary},
};

async fn run_tracked<T, Fut, S>(
    store: &Store,
    command: &str,
    body: Fut,
    success_summary: S,
) -> Result<T>
where
    Fut: Future<Output = Result<T>>,
    S: FnOnce(&T) -> Option<String>,
{
    let run_id = store.run_log().record_run_start(command)?;
    info!(command, run_id, "run started");
    match body.await {
        Ok(value) => {
            let summary = success_summary(&value);
            store
                .run_log()
                .finish_run(run_id, "success", summary.as_deref(), None)?;
            info!(
                command,
                run_id,
                status = "success",
                summary = summary.as_deref().unwrap_or(""),
                "run finished"
            );
            Ok(value)
        }
        Err(err) => {
            if let Err(finish_err) =
                store
                    .run_log()
                    .finish_run(run_id, "failed", None, Some(&format!("{err:#}")))
            {
                return Err(err.context(format!(
                    "记录 {command} 失败 run_log 时也失败: {finish_err}"
                )));
            }
            error!(
                command,
                run_id,
                status = "failed",
                error = %err,
                "run failed"
            );
            Err(err)
        }
    }
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
    let summary = run_tracked(
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

pub(super) async fn run_once_locked(
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

    let mut parsers = registry::registered_parsers()
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

    // Ordinary sync must not reset or parse legacy sources: mixing new
    // accounting into kept rows is forbidden. Cancel after detect still skips.
    let skipped_legacy = if options.rebuild {
        reset_for_rebuild(store, options, &parser_sources, &remote_outcome.contacted)?;
        BTreeSet::new()
    } else {
        let legacy = legacy_token_accounting_sources_for(store, &parser_sources)?;
        if !legacy.is_empty() {
            let source_names = source_names(&legacy);
            tracing::warn!(
                sources = %source_names,
                "ordinary sync detected legacy token accounting; keeping existing data and skipping writes for this round"
            );
            for source in &legacy {
                eprintln!("{}", SyncStatusStore::legacy_repair_warning(*source));
            }
            exclude_legacy_sources_from_write_set(&mut parsers, &legacy);
        }
        legacy.into_iter().collect()
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
    let sources = drive_result?;
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
    for source in &skipped_legacy {
        let stored_events = stored_events_for_source(store, *source)?;
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
            token_accounting_version: store.token_accounting_version(*source)?,
            legacy_token_accounting: true,
            token_accounting_warning: Some(SyncStatusStore::legacy_repair_warning(*source)),
            parse_ms: 0,
            write_ms: 0,
            lock_wait_ms: lock_wait_ms as i64,
            parse_issues: Default::default(),
            updated_at: crate::util::now_utc(),
        });
        source_stats.push(SourceSyncStats {
            source: *source,
            stored_events,
            lock_wait_ms,
            ..SourceSyncStats::default()
        });
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
            if skipped_legacy.contains(&source.source) {
                continue;
            }
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
    if recent_cutoff.is_some() && !cancel.is_cancelled() {
        for source in &source_stats {
            if skipped_legacy.contains(&source.source) {
                continue;
            }
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
    store.reset_for_sources(sources, LOCAL_HOST_ID)?;
    Ok(())
}

fn exclude_legacy_sources_from_write_set(
    parsers: &mut Vec<Box<dyn crate::parsers::SourceParser>>,
    skip: &[SourceKind],
) {
    parsers.retain(|parser| !skip.contains(&parser.source()));
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
            "Refusing `--source omp` because stored pi rows still use the pre-split token-accounting contract. Run `llmusage sync --rebuild --source pi` first so those rows can migrate, then retry `llmusage sync --source omp`."
        );
    }
    Ok(())
}

pub(crate) fn rebuild_sources(
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
