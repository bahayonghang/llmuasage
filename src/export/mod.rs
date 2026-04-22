use std::{fs, path::Path};

use anyhow::Result;

use crate::{query, store::Store, web};

pub fn export_html_bundle(store: &Store, output_dir: &Path) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    fs::create_dir_all(output_dir.join("assets"))?;

    let snapshot = query::build_dashboard_snapshot(store)?;
    fs::write(output_dir.join("index.html"), web::snapshot_index_html())?;
    fs::write(
        output_dir.join("assets").join("app.css"),
        web::app_stylesheet(),
    )?;
    fs::write(
        output_dir.join("assets").join("app.js"),
        web::app_javascript(),
    )?;
    fs::write(
        output_dir.join("snapshot.json"),
        serde_json::to_vec_pretty(&snapshot)?,
    )?;
    Ok(())
}
