use std::path::PathBuf;

use anyhow::{Result, bail};
use serde_json::json;
use tracing::info;

use crate::{app::AppContext, integrations, models::SourceKind, query::Dashboard, store::Store};

pub async fn run(
    app: &AppContext,
    out: Option<PathBuf>,
    forget_file: Option<PathBuf>,
    source: Option<SourceKind>,
) -> Result<()> {
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;

    if let Some(file_path) = forget_file {
        return run_forget_file(&store, file_path, source);
    }

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
    let dashboard = Dashboard::open(&store)?;
    let overview = dashboard.overview(&Default::default())?;
    let health = dashboard.health()?;
    let sources = dashboard.source_breakdown(&Default::default())?;
    let archive = dashboard.diagnostics()?;
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
        "archive": archive,
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

fn run_forget_file(store: &Store, file_path: PathBuf, source: Option<SourceKind>) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：受信任入口下标记文件为用户主动删除
     * ========================================================================
     * 目标：
     * 1) 把 source_file.state 切到 deleted_by_user
     * 2) 同事务删除 source_cursor 同 file_path 行
     * 3) 不指定 --source 时若多源都存在则要求显式选源
     */
    info!(?file_path, ?source, "开始登记 forget-file");

    let raw_path = file_path.to_string_lossy().to_string();
    let target_source = match source {
        Some(value) => value,
        None => resolve_unique_source(store, &raw_path)?,
    };

    store.mark_source_file_deleted(target_source, &raw_path)?;
    println!(
        "已将 {raw_path} 标记为 {} 源的 deleted_by_user",
        target_source.as_str()
    );
    info!(source = %target_source, "完成 forget-file 登记");
    Ok(())
}

fn resolve_unique_source(store: &Store, file_path: &str) -> Result<SourceKind> {
    let conn = store.open_connection()?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT source FROM source_file WHERE file_path = ?1
         UNION
         SELECT DISTINCT source FROM source_cursor WHERE file_path = ?1",
    )?;
    let rows = stmt.query_map([file_path], |row| row.get::<_, String>(0))?;
    let candidates = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    match candidates.as_slice() {
        [] => bail!(
            "找不到 file_path={file_path} 对应的源；请显式传 --source codex|claude|opencode|gemini"
        ),
        [single] => SourceKind::parse_id(single)
            .ok_or_else(|| anyhow::anyhow!("source_file 表里 source 列出现未知值：{single}")),
        many => bail!(
            "{file_path} 在多个源（{}）中存在，请用 --source 指定",
            many.join(", ")
        ),
    }
}
