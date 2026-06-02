use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, integrations, store::Store};

pub async fn run(app: &AppContext, purge: bool) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：恢复已安装 hook / plugin，并按需清理本地目录
     * ========================================================================
     * 目标：
     * 1) 恢复 Codex notify、Claude hooks、OpenCode plugin
     * 2) 记录卸载 run_log 与每个集成的恢复状态
     * 3) 只有 --purge 才删除 ~/.llmusage
     */
    info!("开始执行本地卸载");

    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let actions = super::run_tracked(
        &store,
        "uninstall",
        async { integrations::uninstall_all(app, &store) },
        |_| Some("local uninstall completed".to_string()),
    )
    .await?;

    println!("Uninstall finished:");
    for action in actions {
        println!("- {}: {} ({})", action.source, action.status, action.detail);
    }

    if purge && app.paths.root_dir.exists() {
        std::fs::remove_dir_all(&app.paths.root_dir)?;
        println!("Purged {}", app.paths.root_dir.display());
    }

    info!("完成本地卸载");
    Ok(())
}
