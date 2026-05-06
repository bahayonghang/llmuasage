use anyhow::Result;
use tracing::info;

use super::{SourceParser, SourceSyncStats};
use crate::store::{Store, SyncRunWriter};

/// Drives a fixed list of [`SourceParser`] implementations against the shared
/// writer in registration order.
///
/// Sequencing is intentional: every parser shares one [`SyncRunWriter`] /
/// SQLite connection, so concurrent parsers would contend on the same write
/// path. After each parse the driver overrides `lock_wait_ms` so callers see
/// a uniform wait metric regardless of which parser ran first.
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
    /*
     * ========================================================================
     * 步骤1：按注册顺序串行驱动每个 SourceParser
     * ========================================================================
     * 目标：
     * 1) 依次调用每个 parser 的 parse 方法
     * 2) 把外部锁等待耗时注入每个 source 的 stats
     * 3) 收集每源 SourceSyncStats 后返回
     */
    info!(parsers = parsers.len(), "开始驱动 SourceParser 列表");

    let mut all_stats = Vec::with_capacity(parsers.len());
    for parser in parsers {
        // 1.1 调用 parser 的 parse 协议并注入锁等待耗时
        let mut stats = parser.parse(store, writer, parallelism).await?;
        stats.lock_wait_ms = lock_wait_ms;
        all_stats.push(stats);
    }

    info!(sources = all_stats.len(), "完成 SourceParser 列表驱动");
    Ok(all_stats)
}
