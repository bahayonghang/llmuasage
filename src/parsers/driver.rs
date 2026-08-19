use anyhow::Result;
use chrono::{DateTime, Utc};
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
        recent_cutoff: None,
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
    pub recent_cutoff: Option<DateTime<Utc>>,
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
        // 1.1 调用 parser 的 parse 协议并注入锁等待耗时。
        //     进度事件经 try_send 非阻塞投递；通道满时丢弃并计数，
        //     供 profiling 观察背压丢弃率（不改变丢弃策略本身）。
        let progress_sender = ctx.sender.as_deref().cloned();
        let progress_dropped = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let sink_dropped = std::sync::Arc::clone(&progress_dropped);
        let mut progress_sink = move |event: SyncEvent| {
            if let Some(sender) = &progress_sender
                && sender.try_send(event).is_err()
            {
                sink_dropped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        };
        let parse_started = std::time::Instant::now();
        let mut stats = parser
            .parse(
                ctx.store,
                ctx.writer,
                ctx.parallelism,
                ctx.recent_cutoff,
                ctx.cancel,
                ctx.sender.as_ref().map(|_| &mut progress_sink as _),
            )
            .await?;
        stats.lock_wait_ms = ctx.lock_wait_ms;
        emit_parse_issues_log(&stats);
        let source = parser.source();
        tracing::debug!(
            source = %source,
            parse_wall_ms = parse_started.elapsed().as_millis() as u64,
            progress_dropped = progress_dropped.load(std::sync::atomic::Ordering::Relaxed),
            "source parse finished"
        );

        // 1.2 Parser-owned source inventory marks every candidate file seen in
        //     this run before parsing changed files. Driver only performs the
        //     stale-live sweep. If enumeration reported a non-fatal error,
        //     skip the sweep to avoid converting unreadable subtrees into
        //     false `missing` history.
        if stats.last_error.is_some() {
            info!(source = %source, "source inventory incomplete; skipping missing sweep");
        } else {
            let swept = ctx
                .store
                .source_files()
                .sweep_missing(source, &run_started_at)?;
            if swept > 0 {
                info!(source = %source, swept, "标记 missing 文件完成");
            }
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

fn emit_parse_issues_log(stats: &SourceSyncStats) {
    if stats.parse_issues.summary_text().is_none() {
        return;
    }
    let reasons = stats
        .parse_issues
        .samples
        .iter()
        .map(|sample| sample.reason.as_str())
        .filter(|reason| !reason.is_empty())
        .collect::<Vec<_>>()
        .join(",");
    info!(
        source = %stats.source,
        malformed = stats.parse_issues.malformed_lines,
        oversized = stats.parse_issues.oversized_lines,
        skipped = stats.parse_issues.skipped_lines,
        accounting = stats.parse_issues.accounting_anomaly_lines,
        reasons = reasons.as_str(),
        "parse issues"
    );
}

async fn emit(sender: Option<&mut mpsc::Sender<SyncEvent>>, event: SyncEvent) -> Result<()> {
    if let Some(sender) = sender {
        sender.send(event).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ParseIssueKind, ParseIssues, SourceKind};
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("buffer").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn parse_issue_info_event_includes_source_counts_and_reasons() {
        let buf = Buffer::default();
        let writer = buf.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        let mut issues = ParseIssues::default();
        issues.record(
            SourceKind::Zcode,
            "hash",
            0,
            ParseIssueKind::Skipped,
            "zcode_unfinished:error:invalid_request",
        );
        let stats = SourceSyncStats {
            source: SourceKind::Zcode,
            parse_issues: issues,
            ..SourceSyncStats::default()
        };
        tracing::subscriber::with_default(subscriber, || {
            emit_parse_issues_log(&stats);
        });
        let text = String::from_utf8(buf.0.lock().expect("buffer").clone()).expect("utf8");
        assert!(text.contains("zcode"), "{text}");
        assert!(text.contains("skipped"), "{text}");
        assert!(
            text.contains("zcode_unfinished:error:invalid_request"),
            "{text}"
        );
    }
}
