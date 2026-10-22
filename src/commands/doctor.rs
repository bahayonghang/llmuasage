use crate::common::i18n::commands::message;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;
use tracing::info;

use crate::{
    app::AppContext,
    store::{SourceSyncStatus, Store},
};

#[derive(Debug, Clone, Serialize)]
struct DoctorCheck {
    id: &'static str,
    status: &'static str,
    detail: String,
    #[serde(skip)]
    presentation: DoctorFact,
}

#[derive(Debug, Clone)]
enum DoctorFact {
    OpenCode {
        path: String,
    },
    Logs {
        disabled: bool,
        path: String,
        size: u64,
    },
    Errors(usize),
    Dropped(u64),
    Maintenance(u64),
    RecentRuns {
        failed: bool,
    },
    ParseIssues(Vec<(String, u64, u64)>),
    SourceIssues(crate::domain::source_diagnostics::SourceIssues),
}
impl DoctorFact {
    fn render(&self) -> String {
        match self {
            Self::OpenCode { path } => message!(DoctorDb, path),
            Self::Logs {
                disabled: true,
                path,
                ..
            } => message!(DoctorLogsDisabled, path),
            Self::Logs { path, size, .. } => message!(DoctorLogs, path, size),
            Self::Errors(count) => message!(DoctorErrors, count),
            Self::Dropped(count) => message!(DoctorDropped, count),
            Self::Maintenance(count) => message!(DoctorMaintenance, count),
            Self::RecentRuns { failed: true } => message!(DoctorRunsFailed),
            Self::RecentRuns { failed: false } => message!(DoctorRunsOk),
            Self::ParseIssues(faults) if faults.is_empty() => message!(DoctorNoParse),
            Self::ParseIssues(faults) => faults
                .iter()
                .map(|(source, malformed, oversized)| {
                    message!(DoctorParse, source, malformed, oversized)
                })
                .collect::<Vec<_>>()
                .join("; "),
            Self::SourceIssues(issues) if issues.is_empty() => message!(DoctorNoSources),
            Self::SourceIssues(issues) => issues
                .iter()
                .flat_map(|(source, issues)| {
                    issues.iter().flat_map(move |issue| {
                        super::sync_summary::notice_lines(
                            &crate::sync::notices::RunNotice::SourceIssue {
                                source: *source,
                                issue: issue.clone(),
                            },
                            false,
                        )
                    })
                })
                .collect::<Vec<_>>()
                .join("; "),
        }
    }
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
    info!(event_code = "commands.doctor.01", path = %source_path.display(), "刷新定价快照");
    let store = Store::new(&app.paths)?;
    super::cli_errors::pricing_result(store.bootstrap_with_pricing_facts())?;
    let result =
        super::cli_errors::pricing_result(store.activate_pricing_snapshot_with_facts(source_path))?;
    info!(
        event_code = "commands.doctor.02",
        updated = result.updated_events,
        catalog = result.effective.identity,
        snapshot = result.effective.file.as_deref().unwrap_or("embedded"),
        "定价快照已落盘并重算"
    );
    println!(
        "{}",
        message!(
            DoctorPricing,
            result.effective.version,
            result.effective.file.as_deref().unwrap_or("embedded"),
            result.updated_events
        )
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
    info!(
        event_code = "commands.doctor.03",
        "开始执行 doctor 健康检查"
    );

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
        presentation: DoctorFact::OpenCode {
            path: opencode_db_path.display().to_string(),
        },
    }];

    checks.push(DoctorCheck {
        id: "logs.file",
        presentation: DoctorFact::Logs {
            disabled: logging_disabled,
            path: logs.path.clone(),
            size: logs.size_bytes,
        },
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
        presentation: DoctorFact::Errors(logs.recent_error_count),
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
        presentation: DoctorFact::Dropped(logs.dropped_event_count),
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
        presentation: DoctorFact::Maintenance(logs.maintenance_error_count),
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
            presentation: DoctorFact::RecentRuns { failed: true },
            status: "warn",
            detail: "最近运行中存在 failed/aborted 等非成功记录".to_string(),
        });
    } else {
        checks.push(DoctorCheck {
            id: "recent.failures",
            presentation: DoctorFact::RecentRuns { failed: false },
            status: "ok",
            detail: "最近运行未发现 failed/aborted 等非成功记录".to_string(),
        });
    }

    let sync_statuses = store.sync_status().load_source_sync_statuses("local")?;
    checks.push(parse_issues_doctor_check(&sync_statuses));
    let source_issues = store.sync_status().load_source_issues("local")?;
    checks.push(source_issues_doctor_check(&source_issues));

    if json {
        println!("{}", serde_json::to_string_pretty(&checks)?);
    } else {
        println!("{}", message!(DoctorTitle));
        for check in checks {
            println!(
                "- [{}] {} {}",
                crate::common::i18n::commands::status_label(check.status),
                check.id,
                check.presentation.render()
            );
        }
    }

    info!(event_code = "commands.doctor.04", "完成 doctor 健康检查");
    Ok(())
}

fn source_issues_doctor_check(
    issues: &crate::domain::source_diagnostics::SourceIssues,
) -> DoctorCheck {
    let details = issues
        .iter()
        .flat_map(|(source, issues)| {
            issues
                .iter()
                .map(move |issue| format!("{source}: {}", issue.cli_line(*source)))
        })
        .collect::<Vec<_>>();
    DoctorCheck {
        id: "source.issues",
        presentation: DoctorFact::SourceIssues(issues.clone()),
        status: if details.is_empty() { "ok" } else { "warn" },
        detail: if details.is_empty() {
            "no source blockers in the latest source observations".to_string()
        } else {
            details.join("; ")
        },
    }
}

fn parse_issues_doctor_check(statuses: &[SourceSyncStatus]) -> DoctorCheck {
    let mut faults = Vec::new();
    for status in statuses {
        if status.parse_issues.total() == 0 {
            continue;
        }
        faults.push(format!(
            "{} malformed={} oversized={}",
            status.source, status.parse_issues.malformed_lines, status.parse_issues.oversized_lines
        ));
    }
    if faults.is_empty() {
        DoctorCheck {
            id: "parse.issues",
            presentation: DoctorFact::ParseIssues(
                statuses
                    .iter()
                    .filter(|s| s.parse_issues.total() > 0)
                    .map(|s| {
                        (
                            s.source.clone(),
                            s.parse_issues.malformed_lines,
                            s.parse_issues.oversized_lines,
                        )
                    })
                    .collect(),
            ),
            status: "ok",
            detail: "no malformed or oversized parse issues".to_string(),
        }
    } else {
        DoctorCheck {
            id: "parse.issues",
            presentation: DoctorFact::ParseIssues(
                statuses
                    .iter()
                    .filter(|s| s.parse_issues.total() > 0)
                    .map(|s| {
                        (
                            s.source.clone(),
                            s.parse_issues.malformed_lines,
                            s.parse_issues.oversized_lines,
                        )
                    })
                    .collect(),
            ),
            status: "warn",
            detail: faults.join("; "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_issues_doctor_check;
    use crate::{
        models::{ParseIssueKind, ParseIssueSample, ParseIssues, SourceKind},
        store::SourceSyncStatus,
    };

    fn status_with_issues(source: &str, issues: ParseIssues) -> SourceSyncStatus {
        SourceSyncStatus {
            source: source.to_string(),
            files_processed: 1,
            changed_files: 1,
            bytes_scanned: 0,
            events_seen: 0,
            events_replayed: 0,
            events_inserted: 0,
            stored_events: 0,
            token_accounting_version: None,
            legacy_token_accounting: false,
            token_accounting_warning: None,
            parse_ms: 0,
            write_ms: 0,
            lock_wait_ms: 0,
            parse_issues: issues,
            updated_at: "2026-08-17T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn doctor_warns_only_on_malformed_or_oversized() {
        let skipped_only = status_with_issues(
            "zcode",
            ParseIssues {
                skipped_lines: 17,
                samples: vec![ParseIssueSample {
                    source: SourceKind::Zcode,
                    path_hash: "hash".to_string(),
                    offset: 0,
                    kind: ParseIssueKind::Skipped,
                    reason: String::new(),
                }],
                ..ParseIssues::default()
            },
        );
        let check = parse_issues_doctor_check(std::slice::from_ref(&skipped_only));
        assert_eq!(check.status, "ok");

        let anomaly_only = status_with_issues(
            "antigravity",
            ParseIssues {
                accounting_anomaly_lines: 1,
                ..ParseIssues::default()
            },
        );
        let check = parse_issues_doctor_check(std::slice::from_ref(&anomaly_only));
        assert_eq!(check.status, "ok");

        let faults = status_with_issues(
            "codex",
            ParseIssues {
                malformed_lines: 1,
                oversized_lines: 2,
                skipped_lines: 9,
                ..ParseIssues::default()
            },
        );
        let check = parse_issues_doctor_check(&[skipped_only, faults]);
        assert_eq!(check.status, "warn");
        assert!(check.detail.contains("codex malformed=1 oversized=2"));
        assert!(!check.detail.contains("zcode"));
        assert!(!check.detail.contains("skipped"));
    }
}
