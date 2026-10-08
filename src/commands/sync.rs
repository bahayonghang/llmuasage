use std::{
    collections::HashMap,
    io::{self, BufRead, BufReader, IsTerminal, Write},
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
    remote::protocol::{ShardRecord, encode_record, source_accounting_versions},
    store::{BootstrapProgressEvent, HolderKind, Store},
    util::now_utc,
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
    let parsers = registry::registered_parsers()
        .into_iter()
        .filter(|parser| {
            request
                .source_kind()
                .is_none_or(|source| parser.source() == source)
        })
        .collect::<Vec<_>>();
    write_record(
        &out,
        &ShardRecord::header(
            now_utc(),
            source_accounting_versions(parsers.iter().map(|parser| parser.source())),
        ),
    )?;
    let sink = Arc::clone(&out);
    let mut writer = store.begin_collect_run(move |shard| {
        write_record(&sink, &ShardRecord::Shard { shard }).map_err(|err| {
            crate::error::LlmusageError::ConfigInvalid {
                detail: err.to_string(),
            }
        })
    })?;
    let cancel = CancellationToken::new();
    let result = driver::drive_with_rebuild(
        driver::DriveContext {
            parsers: &parsers,
            store: &store,
            writer: &mut writer,
            parallelism: request.parallelism(),
            lock_wait_ms: 0,
            recent_cutoff,
            sender: None,
            cancel: &cancel,
            sweep_host_ids: vec![crate::store::LOCAL_HOST_ID.to_string()],
        },
        false,
        false,
    )
    .await?;
    writer.finish_sync_run()?;
    let mut parse_issues = ParseIssues::default();
    let sources = result.stats;
    for stats in &sources {
        parse_issues.merge(stats.parse_issues.clone());
    }
    let trailer =
        crate::remote::protocol::encode_trailer(sources, parse_issues, &result.source_issues)?;
    let mut output = out
        .lock()
        .map_err(|_| anyhow::anyhow!("shard output lock poisoned"))?;
    writeln!(output, "{trailer}")?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncStreamCapabilities {
    pub stdin_is_terminal: bool,
    pub stdout_is_terminal: bool,
    pub stderr_is_terminal: bool,
}

impl SyncStreamCapabilities {
    pub fn detect() -> Self {
        Self {
            stdin_is_terminal: io::stdin().is_terminal(),
            stdout_is_terminal: io::stdout().is_terminal(),
            stderr_is_terminal: io::stderr().is_terminal(),
        }
    }

    pub fn can_prompt(&self, options: &SyncRunOptions) -> bool {
        !options.json_events
            && !options.rebuild
            && options.recent_days.is_none()
            && self.stdin_is_terminal
            && self.stdout_is_terminal
            && self.stderr_is_terminal
    }

    /// True when the human post-table notice should speak to a person at a terminal.
    ///
    /// `--recent-days` still counts: that run must not prompt, but it should
    /// tell the operator to rerun without the window. `--json-events` does not.
    pub fn interactive_terminal(&self, options: &SyncRunOptions) -> bool {
        !options.json_events
            && self.stdin_is_terminal
            && self.stdout_is_terminal
            && self.stderr_is_terminal
    }
}

pub async fn run_with_options(app: &AppContext, options: SyncRunOptions) -> Result<()> {
    run_with_options_and_streams(app, options, SyncStreamCapabilities::detect())
        .await
        .map(|_| ())
}

pub(crate) async fn run_with_options_and_streams(
    app: &AppContext,
    options: SyncRunOptions,
    streams: SyncStreamCapabilities,
) -> Result<SyncSummary> {
    run_with_prompt_io(
        app,
        options,
        streams,
        BufReader::new(io::stdin()),
        io::stderr(),
    )
    .await
}

/// Production sync entry with an explicit prompt reader.
///
/// The reader is touched only when `streams.can_prompt` is true and no choice
/// callback is already installed. Redirected or JSON runs drop it unread.
pub(crate) async fn run_with_prompt_io<R, W>(
    app: &AppContext,
    mut options: SyncRunOptions,
    streams: SyncStreamCapabilities,
    reader: R,
    writer: W,
) -> Result<SyncSummary>
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    options.interactive_terminal = streams.interactive_terminal(&options);
    if streams.can_prompt(&options) && options.recovery_prompt.is_none() {
        let reader = Arc::new(Mutex::new(reader));
        let writer = Arc::new(Mutex::new(writer));
        options.recovery_prompt = Some(Arc::new(move |source, coverage| {
            let mut reader = reader.lock().expect("prompt stdin lock");
            let mut writer = writer.lock().expect("prompt stderr lock");
            crate::parsers::antigravity::prompt_recovery_choice(
                &mut *reader,
                &mut *writer,
                source,
                coverage,
            )
        }));
    }
    options.validate()?;
    info!("开始执行全量本地真源同步");
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
) -> Result<SyncSummary> {
    // 渲染器与 guard 的生命周期属于命令函数本身：bootstrap/锁阶段的 `?`
    // 提前返回同样经 Drop 完成终端清理，不依赖 reporter task 是否已 spawn。
    let renderer = Arc::new(Mutex::new(sync_progress::stderr_renderer()));
    let _guard = sync_progress::TerminalGuard::new(Arc::clone(&renderer));
    let warning_renderer = Arc::clone(&renderer);
    let _warning_guard = crate::logging::install_stderr_sink(Arc::new(move |bytes| {
        warning_renderer
            .lock()
            .map_err(|_| io::Error::other("progress renderer lock poisoned"))?
            .write_warning(bytes)
    }));
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
    Ok(summary)
}

async fn run_with_json_events(
    app: &AppContext,
    store: &Store,
    options: &SyncRunOptions,
) -> Result<SyncSummary> {
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
    result
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
    sync_summary::sample_basenames(
        store,
        "local",
        summary
            .sources
            .iter()
            .filter(|stats| !stats.parse_issues.samples.is_empty())
            .map(|stats| stats.source),
    )
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

    struct SpyReader {
        seen: std::sync::Arc<std::sync::atomic::AtomicBool>,
        data: Vec<u8>,
        pos: usize,
    }

    impl std::io::Read for SpyReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.seen.store(true, std::sync::atomic::Ordering::SeqCst);
            let rest = &self.data[self.pos..];
            let n = rest.len().min(buf.len());
            buf[..n].copy_from_slice(&rest[..n]);
            self.pos += n;
            Ok(n)
        }
    }

    impl std::io::BufRead for SpyReader {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            self.seen.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(&self.data[self.pos..])
        }

        fn consume(&mut self, amt: usize) {
            self.pos += amt;
        }
    }

    struct FailingReader;

    impl std::io::Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("failing reader was read"))
        }
    }

    impl std::io::BufRead for FailingReader {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            Err(std::io::Error::other("failing reader was read"))
        }

        fn consume(&mut self, _: usize) {}
    }

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: impl AsRef<std::path::Path>) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::set_var(key, value.as_ref()) };
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.prev {
                    Some(value) => std::env::set_var(self.key, value),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    fn seed_promptable_antigravity(store: &Store) -> Result<()> {
        store.open_connection()?.execute_batch(
            "INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens, created_at
             ) VALUES (
                'antigravity:test:event', 'antigravity', 'gemini-2.5-pro',
                '2026-07-15T03:00:00Z', '2026-07-15T03:00:00Z',
                20, 0, 0, 5, 0, 25, '2026-07-15T03:00:00Z'
             );
             INSERT INTO source_cursor(source, cursor_key, file_path, updated_at)
             VALUES ('antigravity', 'antigravity:test', '/missing/antigravity-history.jsonl', '2026-07-15T03:00:00Z');
             INSERT INTO source_file(source, file_path, state, last_state_change_at)
             VALUES ('antigravity', '/missing/antigravity-history.jsonl', 'missing', '2026-07-15T03:00:00Z');",
        )?;
        store.set_meta_value("token_accounting_version.antigravity", "2")?;
        Ok(())
    }

    #[test]
    fn stream_capabilities_decision_matrix() {
        let all_terminals = SyncStreamCapabilities {
            stdin_is_terminal: true,
            stdout_is_terminal: true,
            stderr_is_terminal: true,
        };
        let default_opts = SyncRunOptions::default();
        assert!(all_terminals.can_prompt(&default_opts));
        assert!(all_terminals.interactive_terminal(&default_opts));

        // stdout-only redirected
        let stdout_redirect = SyncStreamCapabilities {
            stdin_is_terminal: true,
            stdout_is_terminal: false,
            stderr_is_terminal: true,
        };
        assert!(!stdout_redirect.can_prompt(&default_opts));
        assert!(!stdout_redirect.interactive_terminal(&default_opts));

        // stderr-only redirected
        let stderr_redirect = SyncStreamCapabilities {
            stdin_is_terminal: true,
            stdout_is_terminal: true,
            stderr_is_terminal: false,
        };
        assert!(!stderr_redirect.can_prompt(&default_opts));
        assert!(!stderr_redirect.interactive_terminal(&default_opts));

        // stdin pipe
        let stdin_pipe = SyncStreamCapabilities {
            stdin_is_terminal: false,
            stdout_is_terminal: true,
            stderr_is_terminal: true,
        };
        assert!(!stdin_pipe.can_prompt(&default_opts));
        assert!(!stdin_pipe.interactive_terminal(&default_opts));

        // --json-events never prompts
        let json_opts = SyncRunOptions {
            json_events: true,
            ..Default::default()
        };
        assert!(!all_terminals.can_prompt(&json_opts));
        assert!(!all_terminals.interactive_terminal(&json_opts));

        // --rebuild never prompts
        let rebuild_opts = SyncRunOptions {
            rebuild: true,
            ..Default::default()
        };
        assert!(!all_terminals.can_prompt(&rebuild_opts));

        // --recent-days never prompts
        let window_opts = SyncRunOptions {
            recent_days: Some(7),
            ..Default::default()
        };
        assert!(!all_terminals.can_prompt(&window_opts));
        assert!(all_terminals.interactive_terminal(&window_opts));
    }

    #[test]
    fn prompt_recovery_choice_parsing_and_retry() {
        let coverage = crate::parsers::antigravity::AntigravityProductCoverage {
            source: SourceKind::Antigravity,
            discovered_count: 10,
            new_files_count: 2,
            tracked_count: 5,
            stored_events: 100,
            missing_count: 1,
            out_of_scope_count: 1,
            unreadable_count: 0,
            discovery_incomplete: false,
        };

        // Empty input -> Keep
        let mut out = Vec::new();
        let mut reader = std::io::Cursor::new(b"\n");
        assert_eq!(
            crate::parsers::antigravity::prompt_recovery_choice(
                &mut reader,
                &mut out,
                SourceKind::Antigravity,
                &coverage
            ),
            crate::sync::types::AntigravityRecoveryChoice::Keep
        );

        // 'k' -> Keep
        let mut out = Vec::new();
        let mut reader = std::io::Cursor::new(b"k\n");
        assert_eq!(
            crate::parsers::antigravity::prompt_recovery_choice(
                &mut reader,
                &mut out,
                SourceKind::Antigravity,
                &coverage
            ),
            crate::sync::types::AntigravityRecoveryChoice::Keep
        );

        // 'r' -> AcceptLoss
        let mut out = Vec::new();
        let mut reader = std::io::Cursor::new(b"r\n");
        assert_eq!(
            crate::parsers::antigravity::prompt_recovery_choice(
                &mut reader,
                &mut out,
                SourceKind::Antigravity,
                &coverage
            ),
            crate::sync::types::AntigravityRecoveryChoice::AcceptLoss
        );

        // Invalid input then 'r' -> AcceptLoss
        let mut out = Vec::new();
        let mut reader = std::io::Cursor::new(b"invalid\nr\n");
        assert_eq!(
            crate::parsers::antigravity::prompt_recovery_choice(
                &mut reader,
                &mut out,
                SourceKind::Antigravity,
                &coverage
            ),
            crate::sync::types::AntigravityRecoveryChoice::AcceptLoss
        );

        // Invalid input then 'x' -> Keep
        let mut out = Vec::new();
        let mut reader = std::io::Cursor::new(b"invalid\nx\n");
        assert_eq!(
            crate::parsers::antigravity::prompt_recovery_choice(
                &mut reader,
                &mut out,
                SourceKind::Antigravity,
                &coverage
            ),
            crate::sync::types::AntigravityRecoveryChoice::Keep
        );
    }

    #[tokio::test]
    async fn stdin_pipe_with_choice_text_and_failing_stdin_skips_without_reading() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home)?;
        let _gemini = EnvGuard::set("GEMINI_CLI_HOME", home.join(".gemini"));
        let _home = EnvGuard::set("HOME", &home);
        let _profile = EnvGuard::set("USERPROFILE", &home);
        let paths = crate::paths::AppPaths::with_root(temp.path().join(".llmusage"))?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        seed_promptable_antigravity(&store)?;
        let events_before: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'antigravity'",
            [],
            |row| row.get(0),
        )?;
        let app = AppContext {
            paths,
            current_exe: std::env::current_exe()?,
        };
        let forbidden = [
            (
                "stdout redirect",
                SyncStreamCapabilities {
                    stdin_is_terminal: true,
                    stdout_is_terminal: false,
                    stderr_is_terminal: true,
                },
                SyncRunOptions {
                    source: Some(SourceKind::Antigravity),
                    ..Default::default()
                },
            ),
            (
                "stderr redirect",
                SyncStreamCapabilities {
                    stdin_is_terminal: true,
                    stdout_is_terminal: true,
                    stderr_is_terminal: false,
                },
                SyncRunOptions {
                    source: Some(SourceKind::Antigravity),
                    ..Default::default()
                },
            ),
            (
                "stdin pipe",
                SyncStreamCapabilities {
                    stdin_is_terminal: false,
                    stdout_is_terminal: true,
                    stderr_is_terminal: true,
                },
                SyncRunOptions {
                    source: Some(SourceKind::Antigravity),
                    ..Default::default()
                },
            ),
            (
                "json events",
                SyncStreamCapabilities {
                    stdin_is_terminal: true,
                    stdout_is_terminal: true,
                    stderr_is_terminal: true,
                },
                SyncRunOptions {
                    source: Some(SourceKind::Antigravity),
                    json_events: true,
                    ..Default::default()
                },
            ),
        ];
        for (name, streams, options) in forbidden {
            let seen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let reader = SpyReader {
                seen: std::sync::Arc::clone(&seen),
                data: b"r\n".to_vec(),
                pos: 0,
            };
            let result =
                run_with_prompt_io(&app, options.clone(), streams, reader, Vec::<u8>::new()).await;
            assert!(
                result.is_ok(),
                "{name} must skip without reading stdin: {result:?}"
            );
            assert!(
                !seen.load(std::sync::atomic::Ordering::SeqCst),
                "{name} read stdin"
            );
            let failing_res =
                run_with_prompt_io(&app, options, streams, FailingReader, Vec::<u8>::new()).await;
            assert!(
                failing_res.is_ok(),
                "{name} must not call a stdin that fails on first read: {failing_res:?}"
            );
        }
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(2)
        );
        let events_after: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'antigravity'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(events_before, events_after);

        let seen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = SpyReader {
            seen: std::sync::Arc::clone(&seen),
            data: b"k\n".to_vec(),
            pos: 0,
        };
        let prompted = run_with_prompt_io(
            &app,
            SyncRunOptions {
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            SyncStreamCapabilities {
                stdin_is_terminal: true,
                stdout_is_terminal: true,
                stderr_is_terminal: true,
            },
            reader,
            Vec::<u8>::new(),
        )
        .await;
        assert!(prompted.is_ok(), "{prompted:?}");
        assert!(
            seen.load(std::sync::atomic::Ordering::SeqCst),
            "a fully interactive gap must read the prompt stdin"
        );
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(2),
            "keep must not rebuild"
        );
        Ok(())
    }

    #[tokio::test]
    async fn windowed_terminal_notice_does_not_read_stdin() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home)?;
        let _gemini = EnvGuard::set("GEMINI_CLI_HOME", home.join(".gemini"));
        let _home = EnvGuard::set("HOME", &home);
        let _profile = EnvGuard::set("USERPROFILE", &home);
        let paths = crate::paths::AppPaths::with_root(temp.path().join(".llmusage"))?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        seed_promptable_antigravity(&store)?;
        let events_before: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'antigravity'",
            [],
            |row| row.get(0),
        )?;
        let app = AppContext {
            paths,
            current_exe: std::env::current_exe()?,
        };
        let cases = [
            (
                "interactive window",
                SyncStreamCapabilities {
                    stdin_is_terminal: true,
                    stdout_is_terminal: true,
                    stderr_is_terminal: true,
                },
                "without `--recent-days` to choose recovery",
                "--allow-lossy-rebuild",
            ),
            (
                "redirected window",
                SyncStreamCapabilities {
                    stdin_is_terminal: true,
                    stdout_is_terminal: false,
                    stderr_is_terminal: true,
                },
                "do not pass `--recent-days`",
                "to choose recovery",
            ),
        ];
        for (name, streams, present, absent) in cases {
            let seen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let reader = SpyReader {
                seen: std::sync::Arc::clone(&seen),
                data: b"r\n".to_vec(),
                pos: 0,
            };
            let summary = run_with_prompt_io(
                &app,
                SyncRunOptions {
                    source: Some(SourceKind::Antigravity),
                    recent_days: Some(7),
                    ..Default::default()
                },
                streams,
                reader,
                Vec::<u8>::new(),
            )
            .await
            .unwrap_or_else(|err| panic!("{name} failed: {err}"));
            assert!(
                !seen.load(std::sync::atomic::Ordering::SeqCst),
                "{name} read stdin"
            );
            let notice = summary.sources[0]
                .last_error
                .as_deref()
                .unwrap_or_else(|| panic!("{name} missing notice"));
            assert!(notice.contains(present), "{name}: {notice}");
            assert!(!notice.contains(absent), "{name}: {notice}");
            let failing = run_with_prompt_io(
                &app,
                SyncRunOptions {
                    source: Some(SourceKind::Antigravity),
                    recent_days: Some(7),
                    ..Default::default()
                },
                streams,
                FailingReader,
                Vec::<u8>::new(),
            )
            .await;
            assert!(
                failing.is_ok(),
                "{name} must not call a stdin that fails on first read: {failing:?}"
            );
        }
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(2)
        );
        let events_after: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'antigravity'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(events_before, events_after);
        Ok(())
    }
}
