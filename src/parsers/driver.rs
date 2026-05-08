use anyhow::Result;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::{SourceParser, SourceSyncStats, SyncEvent};
use crate::store::{Store, SyncRunWriter};

/// Drives a fixed list of [`SourceParser`] implementations against the shared
/// writer in registration order.
///
/// Sequencing is intentional: every parser shares one [`SyncRunWriter`] /
/// SQLite connection, so concurrent parsers would contend on the same write
/// path. After each parse the driver overrides `lock_wait_ms` so callers see
/// a uniform wait metric regardless of which parser ran first, then sweeps
/// stale `source_file.state='live'` rows for that source to `missing`
/// (D15 / ADR 0006). The sweep runs per-parser rather than once at the end so
/// each source's state machine reflects the parser that just ran, regardless
/// of whether later parsers fail.
///
/// Returning `Vec<SourceSyncStats>` (not a richer outcome type) keeps the
/// driver thin: caller-side aggregation in `commands/sync.rs` already iterates
/// stats once to fold totals and `SourceSyncStatus` rows.
pub async fn drive(
    parsers: &[Box<dyn SourceParser>],
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
    lock_wait_ms: u64,
) -> Result<Vec<SourceSyncStats>> {
    drive_with_events(DriveContext {
        parsers,
        store,
        writer,
        parallelism,
        lock_wait_ms,
        recent_days: None,
        sender: None,
        cancel: &CancellationToken::new(),
    })
    .await
}

/// Parameter object for [`drive_with_events`]. Keeps the public driver call
/// readable as M2 adds RecentReady, progress streaming, and cancellation.
pub struct DriveContext<'a, 'b> {
    pub parsers: &'a [Box<dyn SourceParser>],
    pub store: &'a Store,
    pub writer: &'a mut SyncRunWriter,
    pub parallelism: usize,
    pub lock_wait_ms: u64,
    pub recent_days: Option<u32>,
    pub sender: Option<&'b mut mpsc::Sender<SyncEvent>>,
    pub cancel: &'a CancellationToken,
}

/// Same as [`drive`], but emits sync lifecycle events for JobRegistry and
/// `llmusage sync --json-events`.
pub async fn drive_with_events(mut ctx: DriveContext<'_, '_>) -> Result<Vec<SourceSyncStats>> {
    /*
     * ========================================================================
     * 步骤1：按注册顺序串行驱动每个 SourceParser
     * ========================================================================
     * 目标：
     * 1) 依次调用每个 parser 的 parse 方法
     * 2) 把外部锁等待耗时注入每个 source 的 stats
     * 3) 收集每源 SourceSyncStats 后返回
     */
    info!(parsers = ctx.parsers.len(), "开始驱动 SourceParser 列表");

    let run_started_at = ctx.writer.run_started_at().to_string();
    let mut all_stats = Vec::with_capacity(ctx.parsers.len());
    for parser in ctx.parsers {
        if ctx.cancel.is_cancelled() {
            emit(ctx.sender.as_deref_mut(), SyncEvent::Cancelled).await?;
            break;
        }
        // 1.1 调用 parser 的 parse 协议并注入锁等待耗时
        let progress_sender = ctx.sender.as_deref().cloned();
        let mut progress_sink = move |event: SyncEvent| {
            if let Some(sender) = &progress_sender {
                let _ = sender.try_send(event);
            }
        };
        let mut stats = parser
            .parse(
                ctx.store,
                ctx.writer,
                ctx.parallelism,
                ctx.cancel,
                ctx.sender.as_ref().map(|_| &mut progress_sink as _),
            )
            .await?;
        stats.lock_wait_ms = ctx.lock_wait_ms;
        let source = parser.source();

        // 1.2 把本轮没扫到、上次还是 live 的文件改成 missing
        let swept = ctx
            .store
            .source_files()
            .sweep_missing(source, &run_started_at)?;
        if swept > 0 {
            info!(source = %source, swept, "标记 missing 文件完成");
        }

        if ctx.recent_days.is_some() {
            ctx.store
                .sync_status()
                .mark_recent_completed(source, crate::util::now_utc())?;
            emit(ctx.sender.as_deref_mut(), SyncEvent::RecentReady { source }).await?;
        }

        emit(
            ctx.sender.as_deref_mut(),
            SyncEvent::SourceFinished {
                source,
                stats: stats.clone(),
            },
        )
        .await?;
        all_stats.push(stats);
    }

    info!(sources = all_stats.len(), "完成 SourceParser 列表驱动");
    Ok(all_stats)
}

async fn emit(sender: Option<&mut mpsc::Sender<SyncEvent>>, event: SyncEvent) -> Result<()> {
    if let Some(sender) = sender {
        sender.send(event).await?;
    }
    Ok(())
}
