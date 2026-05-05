use std::time::Instant;

use anyhow::Result;
use tracing::info;

use crate::{
    app::AppContext,
    parsers::{SourceSyncStats, claude::sync_claude, codex::sync_codex, opencode::sync_opencode},
    store::{SourceSyncStatus, Store},
};

#[derive(Debug, Clone)]
pub struct SyncSummary {
    pub sources: Vec<SourceSyncStats>,
    pub total_seen: usize,
    pub total_inserted: usize,
}

pub async fn run(app: &AppContext) -> Result<()> {
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
    let store = Store::new(&app.paths);
    store.bootstrap()?;
    let lock_started = Instant::now();
    let Some(lock) = store.acquire_worker_lock()? else {
        let run_id = store.record_run_start("sync")?;
        store.finish_run(
            run_id,
            "skipped",
            Some("已有 worker 在运行，跳过本次 sync"),
            None,
        )?;
        println!("已有 worker 在运行，跳过本次 sync。");
        return Ok(());
    };
    let lock_wait_ms = lock_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    store.recover_running_runs(&["sync", "hook-run"])?;
    // 1.2 并行解析、单 writer 落库并记录每源统计
    let summary = super::run_tracked(
        &store,
        "sync",
        run_once(app, &store, lock_wait_ms),
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

    println!("Sync finished:");
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

    info!("完成全量本地真源同步");
    Ok(())
}

pub async fn run_once(_app: &AppContext, store: &Store, lock_wait_ms: u64) -> Result<SyncSummary> {
    /*
     * ========================================================================
     * 步骤2：执行三阶段同步流水线
     * ========================================================================
     * 目标：
     * 1) 先并行解析三类真源
     * 2) 再用单 writer 顺序提交 reset / event / cursor
     * 3) 最后刷新每源诊断状态
     */
    info!("开始执行 sync 三阶段流水线");

    // 2.1 计算并发度并按 source 顺序解析 + 即时写入
    let parallelism = std::thread::available_parallelism()
        .map(|value| value.get().min(4))
        .unwrap_or(1);
    let mut writer = store.begin_sync_run()?;
    let mut sources = vec![
        sync_codex(store, &mut writer, parallelism).await?,
        sync_claude(store, &mut writer, parallelism).await?,
        sync_opencode(store, &mut writer, parallelism).await?,
    ];
    let mut total_seen = 0usize;
    let mut total_inserted = 0usize;
    let mut sync_statuses = Vec::new();

    for source in &mut sources {
        source.lock_wait_ms = lock_wait_ms;
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
    store.save_source_sync_statuses(&sync_statuses)?;

    let stats = sources;
    info!("完成 sync 三阶段流水线");
    Ok(SyncSummary {
        sources: stats,
        total_seen,
        total_inserted,
    })
}
