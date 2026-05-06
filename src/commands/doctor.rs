use anyhow::Result;
use serde::Serialize;
use tracing::info;

use crate::{app::AppContext, integrations, store::Store};

#[derive(Debug, Clone, Serialize)]
struct DoctorCheck {
    id: &'static str,
    status: &'static str,
    detail: String,
}

pub async fn run(app: &AppContext, json: bool) -> Result<()> {
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
    let store = Store::new(&app.paths);
    store.bootstrap()?;
    let probes = integrations::probe_all(app)?;
    let recent_runs = store.run_log().recent_runs(10)?;
    let opencode_db_path = std::env::var("OPENCODE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("opencode")
        })
        .join("opencode.db");

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
