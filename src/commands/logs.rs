use crate::common::i18n::{
    commands::message,
    current,
    logging::{project, safe_text},
    text,
};
use anyhow::Result;
use serde::Serialize;

use crate::{
    app::AppContext,
    logging::{LogEntry, LogsRuntimeStatus},
    store::{RunRecord, Store},
};

#[derive(Debug, Clone, Serialize)]
struct LogsPayload {
    logs: LogsRuntimeStatus,
    entries: Vec<LogEntry>,
    recent_runs: Vec<RunRecord>,
}

pub async fn run(
    app: &AppContext,
    limit: usize,
    level: Option<String>,
    command: Option<String>,
    json: bool,
) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：查询本地运行/排障日志
     * ========================================================================
     * 目标：
     * 1) 从 logs/llmusage.ndjson.* 分片读取最近结构化 tracing 事件
     * 2) 同时展示 SQLite run_log 的最近命令审计记录
     * 3) 不读取 usage_event_raw，不暴露 prompt/response/raw JSON
     */
    let store = Store::new(&app.paths)?;
    store.require_initialized()?;

    let limit = limit.max(1);
    let entries = crate::logging::read_recent_log_entries(
        &app.paths,
        limit,
        level.as_deref(),
        command.as_deref(),
    )?;
    let mut recent_runs = store.run_log().recent_runs(limit)?;
    if let Some(command) = command.as_deref() {
        recent_runs.retain(|run| run.command == command);
    }
    if recent_runs.len() > limit {
        recent_runs.truncate(limit);
    }
    let payload = LogsPayload {
        logs: crate::logging::runtime_status(&app.paths)?,
        entries,
        recent_runs,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print_human(&payload);
    }
    Ok(())
}

fn print_human(payload: &LogsPayload) {
    println!("{}", message!(LogsTitle));
    println!("{}", message!(LogFile, payload.logs.path));
    println!(
        "{}",
        message!(
            LogExists,
            if payload.logs.exists {
                text("yes", "是")
            } else {
                text("no", "否")
            }
        )
    );
    println!("{}", message!(LogSize, payload.logs.size_bytes));
    println!(
        "{}",
        message!(
            LogRetained,
            payload.logs.retained_files,
            payload.logs.total_size_bytes
        )
    );
    println!("{}", message!(LogErrors, payload.logs.recent_error_count));
    println!("{}", message!(LogDropped, payload.logs.dropped_event_count));
    println!(
        "{}",
        message!(LogMaintenance, payload.logs.maintenance_error_count)
    );
    println!("{}", message!(LogEntries));
    if payload.entries.is_empty() {
        println!("{}", message!(LogNone));
    } else {
        for entry in &payload.entries {
            let timestamp = safe_text(entry.timestamp.as_deref().unwrap_or("-"));
            println!(
                "- {timestamp} {}",
                project(
                    current(),
                    &entry.level,
                    entry.message.as_deref(),
                    &entry.fields,
                    matches!(entry.level.as_str(), "DEBUG" | "TRACE")
                )
            );
        }
    }
    println!("{}", message!(RunLog));
    if payload.recent_runs.is_empty() {
        println!("{}", message!(LogNone));
    } else {
        for run in &payload.recent_runs {
            let finished = run
                .finished_at
                .as_deref()
                .unwrap_or(text("running", "运行中"));
            let status = match run.status.as_str() {
                "running" => text("running", "运行中"),
                "success" => text("success", "成功"),
                "failed" => text("failed", "失败"),
                "aborted" => text("aborted", "已中止"),
                other => other,
            };
            println!(
                "- #{} [{}] {} {} -> {} {}: {} {}",
                run.id,
                status,
                safe_text(&run.command),
                run.started_at,
                finished,
                message!(OriginalRun),
                safe_text(run.summary.as_deref().unwrap_or("")),
                safe_text(run.error.as_deref().unwrap_or(""))
            );
        }
    }
}
