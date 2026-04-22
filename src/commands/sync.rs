use anyhow::Result;
use tracing::info;

use crate::{
    app::AppContext,
    parsers::{SourceSyncStats, claude::sync_claude, codex::sync_codex, opencode::sync_opencode},
    store::Store,
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
     * 1) 拿全局锁，避免 hook-run 与手动 sync 并发
     * 2) 顺序消费 Codex、Claude、OpenCode 三类真源
     * 3) 输出人读摘要并记录 run_log
     */
    info!("开始执行全量本地真源同步");

    // 1.1 建立 store、记录 run_log 并拿全局锁
    let store = Store::new(&app.paths);
    store.bootstrap()?;
    let run_id = store.record_run_start("sync")?;
    let Some(_lock) = store.acquire_worker_lock()? else {
        store.finish_run(
            run_id,
            "skipped",
            Some("已有 worker 在运行，跳过本次 sync"),
            None,
        )?;
        println!("已有 worker 在运行，跳过本次 sync。");
        return Ok(());
    };

    // 1.2 串行消费三类本地真源
    let summary = run_once(app, &store)?;
    let summary_text = format!(
        "sources={} seen={} inserted={}",
        summary.sources.len(),
        summary.total_seen,
        summary.total_inserted
    );
    store.finish_run(run_id, "success", Some(&summary_text), None)?;

    println!("Sync finished:");
    for item in &summary.sources {
        println!(
            "- {}: files={} seen={} inserted={}",
            item.source, item.files_processed, item.events_seen, item.events_inserted
        );
    }
    println!(
        "- totals: seen={} inserted={}",
        summary.total_seen, summary.total_inserted
    );

    info!("完成全量本地真源同步");
    Ok(())
}

pub fn run_once(_app: &AppContext, store: &Store) -> Result<SyncSummary> {
    let sources = vec![
        sync_codex(store)?,
        sync_claude(store)?,
        sync_opencode(store)?,
    ];
    let total_seen = sources.iter().map(|item| item.events_seen).sum();
    let total_inserted = sources.iter().map(|item| item.events_inserted).sum();
    Ok(SyncSummary {
        sources,
        total_seen,
        total_inserted,
    })
}
