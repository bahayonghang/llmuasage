use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, store::Store, tui};

pub async fn run(app: &AppContext) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：启动本地终端运维面板
     * ========================================================================
     * 目标：
     * 1) 用同一套 query 结果输出最近 24h、来源与健康摘要
     * 2) 为本地运维提供不依赖浏览器的快速入口
     * 3) 保持终端面只读，不改任何配置
     */
    info!("开始启动本地 TUI");

    let store = Store::new(&app.paths);
    store.bootstrap()?;
    tui::run_terminal(&store)?;

    info!("完成本地 TUI 会话");
    Ok(())
}
