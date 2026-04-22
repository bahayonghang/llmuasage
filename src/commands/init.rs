use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, integrations, store::Store};

pub async fn run(app: &AppContext) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：初始化本地运行时并安装三类 hook / plugin
     * ========================================================================
     * 目标：
     * 1) 初始化 SQLite 真源与本地目录
     * 2) 生成 Windows / Unix hook 包装器
     * 3) 安装 Codex、Claude、OpenCode 的本地集成
     */
    info!("开始初始化本地运行时并安装集成");

    // 1.1 建立本地 store 与 run_log
    let store = Store::new(&app.paths);
    store.bootstrap()?;
    let run_id = store.record_run_start("init")?;

    // 1.2 安装三类本地 hook / plugin
    let actions = integrations::install_all(app, &store)?;
    let summary = actions
        .iter()
        .map(|item| format!("{}={}", item.source, item.status))
        .collect::<Vec<_>>()
        .join(", ");
    store.finish_run(run_id, "success", Some(&summary), None)?;

    println!("Init finished:");
    for action in actions {
        println!("- {}: {} ({})", action.source, action.status, action.detail);
    }

    info!("完成本地运行时初始化与集成安装");
    Ok(())
}
