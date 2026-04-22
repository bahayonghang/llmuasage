use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, integrations, query, store::Store};

pub async fn run(app: &AppContext) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：读取本地统计与集成探针摘要
     * ========================================================================
     * 目标：
     * 1) 输出 DB 路径、bucket 数和最近同步时间
     * 2) 汇总来源层与项目层的用量
     * 3) 展示实时集成状态与最近失败
     */
    info!("开始输出状态摘要");

    // 1.1 读取概览、来源、健康和实时集成探针
    let store = Store::new(&app.paths);
    store.bootstrap()?;
    let overview = query::load_overview(&store)?;
    let sources = query::load_source_breakdown(&store)?;
    let health = query::load_health(&store)?;
    let probes = integrations::probe_all(app)?;

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
    for probe in probes {
        println!(
            "- Integration {}: {} ({})",
            probe.source, probe.status, probe.detail
        );
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
