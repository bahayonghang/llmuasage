use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, integrations, store::Store};

pub async fn run(app: &AppContext, best_effort: bool) -> Result<()> {
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
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;

    // 1.2 安装三类本地 hook / plugin
    let actions = super::run_tracked(
        &store,
        "init",
        async { integrations::install_all(app, &store) },
        |actions| Some(integration_summary(actions)),
    )
    .await?;

    println!("Init finished:");
    for action in &actions {
        println!("- {}: {} ({})", action.source, action.status, action.detail);
    }

    // REL-002: install_all folds per-integration errors into `status: error`
    // rows and still returns Ok. Reporting exit 0 there makes automation
    // believe hooks are installed when they are not, and the resulting missing
    // sync data is hard to trace back. Fail loudly unless --best-effort.
    let failed = actions
        .iter()
        .filter(|action| action.status == "error")
        .collect::<Vec<_>>();
    if !failed.is_empty() && !best_effort {
        let detail = failed
            .iter()
            .map(|action| format!("{} ({})", action.source, action.detail))
            .collect::<Vec<_>>()
            .join("; ");
        anyhow::bail!(
            "{} 个集成安装失败：{detail}\n使用 --best-effort 可在部分失败时仍返回 0。",
            failed.len()
        );
    }

    info!("完成本地运行时初始化与集成安装");
    Ok(())
}

fn integration_summary(actions: &[crate::integrations::IntegrationAction]) -> String {
    actions
        .iter()
        .map(|item| format!("{}={}", item.source, item.status))
        .collect::<Vec<_>>()
        .join(", ")
}
