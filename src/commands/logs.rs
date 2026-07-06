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
     * 1) 从 logs/llmusage.ndjson 读取最近结构化 tracing 事件
     * 2) 同时展示 SQLite run_log 的最近命令审计记录
     * 3) 不读取 usage_event_raw，不暴露 prompt/response/raw JSON
     */
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;

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
    println!("Logs:");
    println!("- File: {}", payload.logs.path);
    println!("- Exists: {}", payload.logs.exists);
    println!("- Size: {} bytes", payload.logs.size_bytes);
    println!("- Recent errors: {}", payload.logs.recent_error_count);

    println!("Entries:");
    if payload.entries.is_empty() {
        println!("- none");
    } else {
        for entry in &payload.entries {
            let timestamp = entry.timestamp.as_deref().unwrap_or("-");
            let target = entry.target.as_deref().unwrap_or("-");
            let message = entry.message.as_deref().unwrap_or("");
            let command = entry
                .command
                .as_deref()
                .map(|value| format!(" command={value}"))
                .unwrap_or_default();
            let run_id = entry
                .run_id
                .map(|value| format!(" run_id={value}"))
                .unwrap_or_default();
            let error = entry
                .error
                .as_deref()
                .map(|value| format!(" error={value}"))
                .unwrap_or_default();
            println!(
                "- {timestamp} [{}] {target}{command}{run_id} {message}{error}",
                entry.level
            );
        }
    }

    println!("Run log:");
    if payload.recent_runs.is_empty() {
        println!("- none");
    } else {
        for run in &payload.recent_runs {
            let finished = run.finished_at.as_deref().unwrap_or("running");
            let summary = run.summary.as_deref().unwrap_or("");
            let error = run
                .error
                .as_deref()
                .map(|value| format!(" error={value}"))
                .unwrap_or_default();
            println!(
                "- #{} [{}] {} {} -> {} {}{}",
                run.id, run.status, run.command, run.started_at, finished, summary, error
            );
        }
    }
}
