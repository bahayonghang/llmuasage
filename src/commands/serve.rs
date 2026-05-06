use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, store::Store, web};

pub async fn run(app: &AppContext, port: Option<u16>) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：启动本地 Web UI 与 JSON API
     * ========================================================================
     * 目标：
     * 1) 只监听 127.0.0.1
     * 2) 启动固定端口组探测后的本地服务
     * 3) 持续运行到 Ctrl+C，避免任何公网暴露
     */
    info!("开始启动本地 Web UI 服务");

    let store = Store::new(&app.paths);
    store.bootstrap()?;
    let run_id = store.run_log().record_run_start("serve")?;
    let addr = web::serve(store.clone(), port).await?;
    store
        .run_log()
        .finish_run(run_id, "success", Some(&format!("listen={addr}")), None)?;

    println!("Local dashboard: http://{}", addr);
    tokio::signal::ctrl_c().await?;
    info!("收到 Ctrl+C，准备停止本地 Web UI 服务");
    Ok(())
}
