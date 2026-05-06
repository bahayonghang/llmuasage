use std::path::PathBuf;

use anyhow::Result;
use serde_json::json;
use tracing::info;

use crate::{app::AppContext, integrations, query::Dashboard, store::Store};

pub async fn run(app: &AppContext, out: Option<PathBuf>) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：导出本地机器可读诊断 JSON
     * ========================================================================
     * 目标：
     * 1) 输出 env / paths / integrations / sqlite / cursors / sources / health_checks
     * 2) 让 doctor 与外部调试都复用同一份诊断真源
     * 3) 支持写到文件或直接输出 stdout
     */
    info!("开始导出 diagnostics JSON");

    // 1.1 聚合本地诊断所需的全部数据面
    let store = Store::new(&app.paths);
    store.bootstrap()?;
    let dashboard = Dashboard::open(&store)?;
    let overview = dashboard.overview()?;
    let health = dashboard.health()?;
    let sources = dashboard.source_breakdown()?;
    let probes = integrations::probe_all(app)?;
    let recent_runs = store.run_log().recent_runs(20)?;
    let sync_status = store.sync_status().load_source_sync_statuses()?;
    let diagnostics = json!({
        "env": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        },
        "paths": {
            "root_dir": app.paths.root_dir,
            "db_path": app.paths.db_path,
            "bin_dir": app.paths.bin_dir,
            "hook_cmd_path": app.paths.hook_cmd_path,
            "hook_sh_path": app.paths.hook_sh_path,
        },
        "integrations": probes,
        "sqlite": {
            "bucket_count": overview.bucket_count,
            "last_sync_at": overview.last_sync_at,
            "last_export_at": overview.last_export_at,
        },
        "cursors": health.cursors,
        "sources": sources,
        "sync_status": sync_status,
        "health_checks": {
            "recent_failures": health.recent_failures,
            "integration_records": health.integrations,
        },
        "recent_runs": recent_runs,
    });

    let serialized = serde_json::to_string_pretty(&diagnostics)?;
    if let Some(out_path) = out {
        std::fs::write(&out_path, serialized)?;
        println!("Wrote diagnostics to {}", out_path.display());
    } else {
        println!("{serialized}");
    }

    info!("完成 diagnostics JSON 导出");
    Ok(())
}
