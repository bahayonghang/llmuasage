use std::{fs, path::Path};

use anyhow::Result;
use serde_json::Value;

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
    let snapshot = Dashboard::open(store)?.snapshot(&Default::default())?;
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
        snapshot_json_bytes(&snapshot)?,
    )?;
    Ok(())
}

fn snapshot_json_bytes(snapshot: &crate::query::DashboardSnapshot) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(snapshot)?;
    strip_export_path_fields(&mut value);
    Ok(serde_json::to_vec_pretty(&value)?)
}

fn strip_export_path_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("archive_root");
            for child in map.values_mut() {
                strip_export_path_fields(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_export_path_fields(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        query::Dashboard,
        testing::{Fixture, SeedEvent},
    };

    fn json_contains_key(value: &Value, key: &str) -> bool {
        match value {
            Value::Object(map) => {
                map.contains_key(key) || map.values().any(|child| json_contains_key(child, key))
            }
            Value::Array(items) => items.iter().any(|item| json_contains_key(item, key)),
            _ => false,
        }
    }

    #[test]
    fn export_snapshot_json_omits_archive_root_and_keeps_aggregate_tables() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_event(SeedEvent {
            event_key: "codex:export:project",
            project_hash: "project-a",
            project_label: "Project A",
            ..Default::default()
        })?;
        let run_id = fixture.store().run_log().record_run_start("sync")?;
        fixture.store().run_log().finish_run(
            run_id,
            "failed",
            None,
            Some("SELECT * FROM usage_event; C:\\secret\\llmusage.db"),
        )?;

        let live = serde_json::to_value(
            &Dashboard::open(fixture.store())?.snapshot(&Default::default())?,
        )?;
        assert!(
            json_contains_key(&live, "archive_root"),
            "live snapshot still carries archive_root"
        );

        let output_dir = fixture.paths().root_dir.join("html-export");
        export_html_bundle(fixture.store(), &output_dir)?;
        let snapshot: Value =
            serde_json::from_str(&fs::read_to_string(output_dir.join("snapshot.json"))?)?;
        assert!(
            !json_contains_key(&snapshot, "archive_root"),
            "export JSON must omit archive_root: {snapshot}"
        );
        assert!(snapshot["projects"].is_array());
        assert!(snapshot["hosts"].is_array());
        assert!(snapshot["models"].is_array());
        assert!(snapshot["sources"].is_array());
        assert!(
            snapshot["diagnostics"]["recent_failures"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty()),
            "default strip keeps recent_failures strings: {snapshot}"
        );
        Ok(())
    }

    #[test]
    fn safety_docs_describe_real_export_snapshot_fields() {
        let en = include_str!("../../docs/safety/index.md");
        let zh = include_str!("../../docs/zh/safety/index.md");
        for (label, page) in [("en", en), ("zh", zh)] {
            for needle in ["archive_root", "projects", "hosts", "recent_failures"] {
                assert!(
                    page.contains(needle),
                    "{label} safety page must mention {needle}"
                );
            }
        }
    }
}
