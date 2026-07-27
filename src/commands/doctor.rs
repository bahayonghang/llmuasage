use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;
use tracing::info;

use crate::{app::AppContext, store::Store};

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

/// Validates and activates a complete local pricing snapshot, then recomputes
/// per-event costs. URLs are refused and the persisted file is content-addressed.
async fn refresh_pricing_catalog(app: &AppContext, source_path: &Path) -> Result<()> {
    info!(path = %source_path.display(), "刷新定价快照");
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let result = store.activate_pricing_snapshot(source_path)?;
    info!(
        updated = result.updated_events,
        catalog = result.effective.identity,
        snapshot = result.effective.file.as_deref().unwrap_or("embedded"),
        "定价快照已落盘并重算"
    );
    println!(
        "已激活 catalog `{}`（{}）并重算 {} 条事件的成本列",
        result.effective.version,
        result.effective.file.as_deref().unwrap_or("embedded"),
        result.updated_events
    );
    Ok(())
}

async fn diagnostics(app: &AppContext, json: bool) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：基于本地真源执行健康检查
     * ========================================================================
     * 目标：
     * 1) 覆盖被动数据源、OpenCode DB、运行日志与最近失败
     * 2) 用一份规则清单输出人读或 JSON 结果
     * 3) 保持 doctor 只读，不修复任何配置
     */
    info!("开始执行 doctor 健康检查");

    // 1.1 读取最近运行结果与关键文件存在性
    let store = Store::new(&app.paths)?;
    store.require_initialized()?;
    let recent_runs = store.run_log().recent_runs(10)?;
    let logs = crate::logging::runtime_status(&app.paths)?;
    let logging_disabled =
        std::env::var("LLMUSAGE_LOG").is_ok_and(|value| value.eq_ignore_ascii_case("off"));
    let opencode_db_path = crate::integrations::opencode::resolve_db_path();

    let mut checks = vec![DoctorCheck {
        id: "opencode.db",
        status: if opencode_db_path.is_file() {
            "ok"
        } else {
            "warn"
        },
        detail: format!("OpenCode DB: {}", opencode_db_path.display()),
    }];

    checks.push(DoctorCheck {
        id: "logs.file",
        status: if logging_disabled || logs.exists {
            "ok"
        } else {
            "warn"
        },
        detail: if logging_disabled {
            format!("structured runtime logging disabled; path: {}", logs.path)
        } else {
            format!(
                "structured runtime log: {} ({} bytes)",
                logs.path, logs.size_bytes
            )
        },
    });

    checks.push(DoctorCheck {
        id: "logs.recent_errors",
        status: if logs.recent_error_count == 0 {
            "ok"
        } else {
            "warn"
        },
        detail: format!(
            "{} ERROR entries in the recent local log scan",
            logs.recent_error_count
        ),
    });

    checks.push(DoctorCheck {
        id: "logs.dropped_events",
        status: if logs.dropped_event_count == 0 {
            "ok"
        } else {
            "warn"
        },
        detail: format!(
            "{} events dropped by the non-blocking log queue",
            logs.dropped_event_count
        ),
    });

    checks.push(DoctorCheck {
        id: "logs.maintenance_errors",
        status: if logs.maintenance_error_count == 0 {
            "ok"
        } else {
            "warn"
        },
        detail: format!(
            "{} runtime log rotation or retention errors",
            logs.maintenance_error_count
        ),
    });

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
