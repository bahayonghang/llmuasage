use std::path::PathBuf;

use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, export, store::Store};

pub async fn run_html(app: &AppContext, out: Option<PathBuf>) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：导出离线可开的静态 HTML 报告
     * ========================================================================
     * 目标：
     * 1) 写出 index.html + snapshot.json + assets
     * 2) 保持导出结构与本地 Web UI 一致
     * 3) 导出过程只读数据库，不触发任何网络请求
     */
    info!("开始导出静态 HTML 报告");

    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let output_dir = out.unwrap_or_else(|| app.paths.exports_dir.join("latest"));
    let exported_dir = super::run_tracked(
        &store,
        "export html",
        async {
            export::export_html_bundle(&store, &output_dir)?;
            Ok(output_dir.clone())
        },
        |path| Some(format!("out={}", path.display())),
    )
    .await?;
    println!("Exported HTML bundle to {}", exported_dir.display());

    info!("完成静态 HTML 报告导出");
    Ok(())
}
