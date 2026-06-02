use std::{
    collections::VecDeque,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
    sync::OnceLock,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::paths::AppPaths;

const DEFAULT_FILE_LEVEL: &str = "warn";
const MAX_LOG_FILE_BYTES: u64 = 10 * 1024 * 1024;
const RECENT_ERROR_SCAN_LIMIT: usize = 200;

static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// One structured entry read back from `logs/llmusage.ndjson`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// RFC 3339-ish timestamp emitted by tracing-subscriber.
    pub timestamp: Option<String>,
    /// Tracing level (`ERROR`, `WARN`, `INFO`, `DEBUG`, or `TRACE`).
    pub level: String,
    /// Rust module target that emitted the event.
    pub target: Option<String>,
    /// Optional command label when the event records command execution.
    pub command: Option<String>,
    /// Optional source identifier when the event is source-scoped.
    pub source: Option<String>,
    /// Optional SQLite `run_log.id` when the event is tied to a tracked run.
    pub run_id: Option<i64>,
    /// Optional error summary.
    pub error: Option<String>,
    /// Human-readable event message, if present.
    pub message: Option<String>,
    /// Full structured event fields for forward-compatible diagnostics.
    pub fields: Value,
}

/// Runtime log-file status exposed by diagnostics and `llmusage logs`.
#[derive(Debug, Clone, Serialize)]
pub struct LogsRuntimeStatus {
    /// Structured log file path.
    pub path: String,
    /// Whether the log file currently exists.
    pub exists: bool,
    /// Current file size when it exists.
    pub size_bytes: u64,
    /// Number of `ERROR` entries in the most recent scan window.
    pub recent_error_count: usize,
}

pub fn init_logging() -> Result<()> {
    let paths = AppPaths::discover()?;
    init_logging_for_paths(&paths)
}

pub fn init_logging_for_paths(paths: &AppPaths) -> Result<()> {
    let console_filter =
        EnvFilter::try_from_env("RUST_LOG").unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILE_LEVEL));
    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_filter(console_filter);

    if let Some(file_filter) = file_filter() {
        std::fs::create_dir_all(&paths.logs_dir)?;
        enforce_log_size_limit(&paths.log_file_path)?;
        let file_appender = tracing_appender::rolling::never(&paths.logs_dir, "llmusage.ndjson");
        let (writer, guard) = tracing_appender::non_blocking(file_appender);
        let file_layer = fmt::layer()
            .json()
            .with_writer(writer)
            .with_current_span(false)
            .with_span_list(false)
            .with_filter(file_filter);
        let _ = LOG_GUARD.set(guard);
        let _ = tracing_subscriber::registry()
            .with(stderr_layer)
            .with(file_layer)
            .try_init();
    } else {
        let _ = tracing_subscriber::registry().with(stderr_layer).try_init();
    }
    Ok(())
}

pub fn runtime_status(paths: &AppPaths) -> Result<LogsRuntimeStatus> {
    let metadata = std::fs::metadata(&paths.log_file_path).ok();
    let recent_error_count =
        read_recent_log_entries(paths, RECENT_ERROR_SCAN_LIMIT, Some("error"), None)?.len();
    Ok(LogsRuntimeStatus {
        path: paths.log_file_path.display().to_string(),
        exists: metadata.is_some(),
        size_bytes: metadata.map_or(0, |item| item.len()),
        recent_error_count,
    })
}

pub fn read_recent_log_entries(
    paths: &AppPaths,
    limit: usize,
    min_level: Option<&str>,
    command: Option<&str>,
) -> Result<Vec<LogEntry>> {
    if limit == 0 || !paths.log_file_path.is_file() {
        return Ok(Vec::new());
    }

    let scan_limit = limit.saturating_mul(8).max(limit).min(2_000);
    let lines = read_recent_lines(&paths.log_file_path, scan_limit)?;
    let mut entries = lines
        .into_iter()
        .filter_map(|line| parse_log_entry(&line))
        .filter(|entry| {
            min_level.is_none_or(|level| level_allows(entry.level.as_str(), level))
                && command.is_none_or(|wanted| entry.command.as_deref() == Some(wanted))
        })
        .collect::<Vec<_>>();
    if entries.len() > limit {
        let keep_from = entries.len() - limit;
        entries.drain(0..keep_from);
    }
    Ok(entries)
}

fn file_filter() -> Option<EnvFilter> {
    let raw = std::env::var("LLMUSAGE_LOG").unwrap_or_else(|_| DEFAULT_FILE_LEVEL.to_string());
    if raw.eq_ignore_ascii_case("off") {
        return None;
    }
    Some(EnvFilter::new(normalize_level(&raw)))
}

fn enforce_log_size_limit(path: &Path) -> Result<()> {
    let Ok(metadata) = std::fs::metadata(path) else {
        return Ok(());
    };
    if metadata.len() <= MAX_LOG_FILE_BYTES {
        return Ok(());
    }

    let archived = path.with_extension("ndjson.old");
    if archived.exists() {
        std::fs::remove_file(&archived)?;
    }
    std::fs::rename(path, archived)?;
    Ok(())
}

fn normalize_level(raw: &str) -> &str {
    match raw.to_ascii_lowercase().as_str() {
        "error" => "error",
        "warn" | "warning" => "warn",
        "info" => "info",
        "debug" => "debug",
        "trace" => "trace",
        _ => DEFAULT_FILE_LEVEL,
    }
}

fn read_recent_lines(path: &Path, max_lines: usize) -> Result<Vec<String>> {
    let file = File::open(path)?;
    let mut lines = VecDeque::with_capacity(max_lines.min(RECENT_ERROR_SCAN_LIMIT));
    for line in BufReader::new(file).lines() {
        let line = line?;
        if lines.len() == max_lines {
            lines.pop_front();
        }
        lines.push_back(line);
    }
    Ok(lines.into_iter().collect())
}

fn parse_log_entry(line: &str) -> Option<LogEntry> {
    let value: Value = serde_json::from_str(line).ok()?;
    let level = value.get("level")?.as_str()?.to_string();
    let fields = value.get("fields").cloned().unwrap_or(Value::Null);
    Some(LogEntry {
        timestamp: value
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_string),
        level,
        target: value
            .get("target")
            .and_then(Value::as_str)
            .map(str::to_string),
        command: field_string(&fields, "command"),
        source: field_string(&fields, "source"),
        run_id: field_i64(&fields, "run_id"),
        error: field_string(&fields, "error"),
        message: field_string(&fields, "message"),
        fields,
    })
}

fn field_string(fields: &Value, key: &str) -> Option<String> {
    fields
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| fields.get(key).map(|value| value.to_string()))
}

fn field_i64(fields: &Value, key: &str) -> Option<i64> {
    fields.get(key).and_then(Value::as_i64).or_else(|| {
        fields
            .get(key)
            .and_then(Value::as_u64)
            .map(|value| value as i64)
    })
}

fn level_allows(entry_level: &str, min_level: &str) -> bool {
    let Some(entry) = level_rank(entry_level) else {
        return false;
    };
    let Some(min) = level_rank(min_level) else {
        return true;
    };
    entry <= min
}

fn level_rank(level: &str) -> Option<u8> {
    match level.to_ascii_lowercase().as_str() {
        "error" => Some(0),
        "warn" | "warning" => Some(1),
        "info" => Some(2),
        "debug" => Some(3),
        "trace" => Some(4),
        _ => None,
    }
}
