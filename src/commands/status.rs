use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, commands::source_status, query::Dashboard, store::Store};

pub async fn run(app: &AppContext) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：读取本地统计与被动来源摘要
     * ========================================================================
     * 目标：
     * 1) 输出 DB 路径、bucket 数和最近同步时间
     * 2) 汇总来源层与项目层的用量
     * 3) 展示来源状态与最近失败
     */
    info!("开始输出状态摘要");

    // 1.1 读取概览、来源和健康信息
    let store = Store::new(&app.paths)?;
    store.require_initialized()?;
    let dashboard = Dashboard::open(&store)?;
    let overview = dashboard.overview(&Default::default())?;
    let sources = dashboard.source_breakdown(&Default::default())?;
    let health = dashboard.health()?;
    let mut capability_statuses = source_status::build_source_capability_statuses(&sources);
    source_status::apply_token_accounting_statuses(&store, &mut capability_statuses)?;
    let platform_statuses = source_status::build_platform_monitor_statuses();
    let lock = store.current_worker_lock()?;

    // 1.2 打印人读摘要
    println!("Status:");
    println!("- DB: {}", app.paths.db_path.display());
    println!("- Buckets: {}", overview.bucket_count);
    println!(
        "- Last sync: {}",
        overview.last_sync_at.as_deref().unwrap_or("never")
    );
    println!(
        "- Last export: {}",
        overview.last_export_at.as_deref().unwrap_or("never")
    );
    for source in sources {
        println!(
            "- Source {}: total={} last={}",
            source.source,
            source.total_tokens,
            source.last_event_at.as_deref().unwrap_or("never")
        );
    }
    source_status::print_human_statuses(&capability_statuses, &platform_statuses);
    if let Some(lock) = lock {
        println!(
            "- Worker lock: holder={} expires={}",
            lock.holder_identity(),
            lock.lease_expires_at
        );
    } else {
        println!("- Worker lock: idle");
    }
    if let Some(run) = health.recent_failures.first() {
        println!(
            "- Recent error: {} {}",
            run.command,
            run.error.as_deref().unwrap_or("")
        );
    }

    info!("完成状态摘要输出");
    Ok(())
}
