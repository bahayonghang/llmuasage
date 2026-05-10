use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use tracing::info;

use crate::{app::AppContext, integrations, query::PricingCatalog, store::Store};

#[derive(Debug, Clone, Serialize)]
struct DoctorCheck {
    id: &'static str,
    status: &'static str,
    detail: String,
}

pub async fn run(app: &AppContext, json: bool, refresh_pricing: Option<PathBuf>) -> Result<()> {
    if let Some(path) = refresh_pricing {
        return refresh_pricing_catalog(app, &path).await;
    }
    diagnostics(app, json).await
}

/// Validates a local litellm pricing snapshot, copies it to
/// `~/.llmusage/pricing/<catalog-version>.json`, and recomputes the
/// per-event cost columns. URLs are refused; PRD §1.2 explicitly excludes
/// remote price fetching from 0.5.x.
async fn refresh_pricing_catalog(app: &AppContext, source_path: &Path) -> Result<()> {
    info!(path = %source_path.display(), "刷新定价快照");

    if let Some(raw) = source_path.to_str()
        && (raw.starts_with("http://") || raw.starts_with("https://"))
    {
        anyhow::bail!("llmusage 0.5.x 不支持从 URL 拉取定价快照；请下载 JSON 后传本地路径");
    }
    if !source_path.is_file() {
        anyhow::bail!("定价快照路径不存在或不是文件: {}", source_path.display());
    }

    // 用 catalog loader 校验 schema 同时拿到 catalog 实例，下一步直接驱动 recompute。
    let catalog = PricingCatalog::load_snapshot(source_path)?;

    let pricing_dir = app.paths.root_dir.join("pricing");
    std::fs::create_dir_all(&pricing_dir)
        .with_context(|| format!("创建定价目录失败: {}", pricing_dir.display()))?;
    let target = pricing_dir.join(format!("{}.json", catalog.version));
    std::fs::copy(source_path, &target)
        .with_context(|| format!("写入定价快照失败: {}", target.display()))?;

    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let updated = store.recompute_costs_with(&catalog)?;
    info!(
        updated,
        catalog = catalog.version,
        snapshot = %target.display(),
        "定价快照已落盘并重算"
    );
    println!(
        "已写入 {} 并按 catalog `{}` 重算 {} 条事件的成本列",
        target.display(),
        catalog.version,
        updated
    );
    Ok(())
}

async fn diagnostics(app: &AppContext, json: bool) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：基于本地真源执行健康检查
     * ========================================================================
     * 目标：
     * 1) 覆盖 hook 漂移、包装器缺失、OpenCode DB 缺失、最近失败
     * 2) 用一份规则清单输出人读或 JSON 结果
     * 3) 保持 doctor 只读，不修复任何配置
     */
    info!("开始执行 doctor 健康检查");

    // 1.1 读取探针结果、最近运行结果与关键文件存在性
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let probes = integrations::probe_all(app)?;
    let recent_runs = store.run_log().recent_runs(10)?;
    let opencode_db_path = integrations::opencode::resolve_db_path();

    let mut checks = vec![
        DoctorCheck {
            id: "wrapper.cmd",
            status: if app.paths.hook_cmd_path.is_file() {
                "ok"
            } else {
                "warn"
            },
            detail: format!("hook cmd: {}", app.paths.hook_cmd_path.display()),
        },
        DoctorCheck {
            id: "wrapper.sh",
            status: if app.paths.hook_sh_path.is_file() {
                "ok"
            } else {
                "warn"
            },
            detail: format!("hook sh: {}", app.paths.hook_sh_path.display()),
        },
        DoctorCheck {
            id: "opencode.db",
            status: if opencode_db_path.is_file() {
                "ok"
            } else {
                "warn"
            },
            detail: format!("OpenCode DB: {}", opencode_db_path.display()),
        },
    ];

    for probe in probes {
        checks.push(DoctorCheck {
            id: match probe.source {
                crate::models::SourceKind::Codex => "codex.notify",
                crate::models::SourceKind::Claude => "claude.hooks",
                crate::models::SourceKind::Opencode => "opencode.plugin",
                crate::models::SourceKind::Gemini => "gemini.hooks",
            },
            status: if probe.status == "ready" {
                "ok"
            } else {
                "warn"
            },
            detail: probe.detail,
        });
    }

    if recent_runs
        .iter()
        .any(crate::store::RunRecord::counts_as_failure)
    {
        checks.push(DoctorCheck {
            id: "recent.failures",
            status: "warn",
            detail: "最近运行中存在 failed/aborted 等非成功记录".to_string(),
        });
    } else {
        checks.push(DoctorCheck {
            id: "recent.failures",
            status: "ok",
            detail: "最近运行未发现 failed/aborted 等非成功记录".to_string(),
        });
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&checks)?);
    } else {
        println!("Doctor:");
        for check in checks {
            println!("- [{}] {} {}", check.status, check.id, check.detail);
        }
    }

    info!("完成 doctor 健康检查");
    Ok(())
}
