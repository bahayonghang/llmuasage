use std::{fs, path::Path};

use anyhow::Result;

use crate::{query::Dashboard, store::Store, web};

pub fn export_html_bundle(store: &Store, output_dir: &Path) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：导出静态页面骨架与资源清单
     * ========================================================================
     * 目标：
     * 1) 继续导出 index.html + snapshot.json + assets
     * 2) 统一从 web asset manifest 写出全部静态资源
     * 3) 保持 export html 与 serve 共用同一份前端资源
     */
    fs::create_dir_all(output_dir)?;
    fs::create_dir_all(output_dir.join("assets"))?;

    // 1.1 先构建 snapshot，再写出页面骨架
    let snapshot = Dashboard::open(store)?.snapshot()?;
    fs::write(output_dir.join("index.html"), web::snapshot_index_html())?;

    // 1.2 逐个写出 manifest 中登记的静态资源
    for asset in web::asset_manifest() {
        let asset_path = output_dir.join("assets").join(asset.path);
        if let Some(parent) = asset_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(asset_path, asset.body)?;
    }

    // 1.3 最后写出离线 snapshot 数据
    fs::write(
        output_dir.join("snapshot.json"),
        serde_json::to_vec_pretty(&snapshot)?,
    )?;
    Ok(())
}
