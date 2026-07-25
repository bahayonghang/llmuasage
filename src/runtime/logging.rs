use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

use crate::paths::AppPaths;

const DEFAULT_FILE_LEVEL: &str = "warn";
/// Maximum total bytes kept across all retained daily log files.
const MAX_TOTAL_LOG_BYTES: u64 = 30 * 1024 * 1024; // 30 MiB (3 × 10 MiB rotations)
/// Number of daily log files to retain (older files are deleted on startup).
const MAX_LOG_FILES: usize = 7;
const RECENT_ERROR_SCAN_LIMIT: usize = 200;
/// Generous per-line estimate for the reverse tail reader (NDJSON warn entries).
const AVG_LOG_LINE_BYTES: u64 = 512;
/// Log filename prefix used by tracing-appender daily rotation.
const LOG_FILE_PREFIX: &str = "llmusage.ndjson";

static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// One structured entry read back from `logs/llmusage.ndjson.*`.
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
    /// Structured log file path (current daily file, or base path if none yet).
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
        cleanup_old_log_files(&paths.logs_dir);
        // daily rotation: tracing-appender creates files named `llmusage.ndjson.YYYY-MM-DD`
        let file_appender = tracing_appender::rolling::daily(&paths.logs_dir, LOG_FILE_PREFIX);
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
    let current = current_log_file(&paths.logs_dir);
    let display_path = current
        .as_deref()
        .unwrap_or(&paths.log_file_path)
        .display()
        .to_string();
    let metadata = current.as_deref().and_then(|p| std::fs::metadata(p).ok());
    let recent_error_count =
        read_recent_log_entries(paths, RECENT_ERROR_SCAN_LIMIT, Some("error"), None)?.len();
    Ok(LogsRuntimeStatus {
        path: display_path,
        exists: metadata.is_some(),
        size_bytes: metadata.map_or(0, |m| m.len()),
        recent_error_count,
    })
}

pub fn read_recent_log_entries(
    paths: &AppPaths,
    limit: usize,
    min_level: Option<&str>,
    command: Option<&str>,
) -> Result<Vec<LogEntry>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let Some(log_file) = current_log_file(&paths.logs_dir) else {
        return Ok(Vec::new());
    };
    let scan_limit = limit.saturating_mul(8).max(limit).min(2_000);
    let lines = read_tail_lines(&log_file, scan_limit)?;
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

/// Returns the path of the most recently modified daily log file in `logs_dir`,
/// or `None` if no matching file exists.
pub fn current_log_file(logs_dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(logs_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(LOG_FILE_PREFIX)
        })
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            if meta.is_file() {
                Some((
                    meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                    entry.path(),
                ))
            } else {
                None
            }
        })
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().next().map(|(_, path)| path)
}

fn file_filter() -> Option<EnvFilter> {
    let raw = std::env::var("LLMUSAGE_LOG").unwrap_or_else(|_| DEFAULT_FILE_LEVEL.to_string());
    if raw.eq_ignore_ascii_case("off") {
        return None;
    }
    Some(EnvFilter::new(normalize_level(&raw)))
}

/// Deletes old daily log files, keeping at most `MAX_LOG_FILES` most recent
/// entries and ensuring total size stays within `MAX_TOTAL_LOG_BYTES`.
fn cleanup_old_log_files(logs_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(logs_dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf, u64)> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(LOG_FILE_PREFIX)
        })
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            if meta.is_file() {
                Some((
                    meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                    entry.path(),
                    meta.len(),
                ))
            } else {
                None
            }
        })
        .collect();
    // sort newest first
    files.sort_by(|a, b| b.0.cmp(&a.0));

    let mut total_bytes: u64 = 0;
    for (idx, (_, path, size)) in files.iter().enumerate() {
        total_bytes += size;
        let over_count = idx >= MAX_LOG_FILES;
        let over_size = total_bytes > MAX_TOTAL_LOG_BYTES;
        if over_count || over_size {
            let _ = std::fs::remove_file(path);
        }
    }
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

/// Reads the last `max_lines` lines from `path` in O(tail bytes), not O(file size).
///
/// Seeks to an estimated tail position based on `AVG_LOG_LINE_BYTES`, skips
/// any leading partial line, then parses forward. Returns at most `max_lines`
/// lines from the end of the file.
fn read_tail_lines(path: &Path, max_lines: usize) -> Result<Vec<String>> {
    let mut file = File::open(path)?;
    let file_size = file.seek(SeekFrom::End(0))?;
    if file_size == 0 {
        return Ok(Vec::new());
    }

    let tail_bytes = (max_lines as u64)
        .saturating_mul(AVG_LOG_LINE_BYTES)
        .min(file_size);
    let seek_pos = file_size - tail_bytes;
    file.seek(SeekFrom::Start(seek_pos))?;

    let mut buf = String::new();
    file.read_to_string(&mut buf)?;

    // If we didn't start at the file beginning, skip the partial leading line.
    let start = if seek_pos > 0 {
        buf.find('\n').map(|i| i + 1).unwrap_or(buf.len())
    } else {
        0
    };

    let lines: Vec<String> = buf[start..]
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();

    if lines.len() > max_lines {
        Ok(lines[lines.len() - max_lines..].to_vec())
    } else {
        Ok(lines)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_ndjson_lines(path: &Path, count: usize) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        for i in 0..count {
            writeln!(
                f,
                r#"{{"timestamp":"2026-07-25T00:00:{:02}Z","level":"WARN","fields":{{"message":"line {i}"}}}}"#,
                i % 60
            )
            .unwrap();
        }
    }

    #[test]
    fn tail_reads_last_n_lines_only() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.ndjson");
        write_ndjson_lines(&path, 500);

        let lines = read_tail_lines(&path, 50).unwrap();
        assert!(lines.len() <= 50, "should not return more than 50 lines");
        assert!(!lines.is_empty(), "should return some lines");
        // last line should parse correctly
        let last = parse_log_entry(lines.last().unwrap());
        assert!(last.is_some(), "last line should parse as LogEntry");
    }

    #[test]
    fn tail_on_empty_file_returns_empty() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("empty.ndjson");
        std::fs::write(&path, "").unwrap();
        let lines = read_tail_lines(&path, 100).unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn cleanup_keeps_at_most_max_log_files() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();
        // create MAX_LOG_FILES + 3 log files
        for i in 0..(MAX_LOG_FILES + 3) {
            let name = format!("{LOG_FILE_PREFIX}.2026-07-{:02}", i + 1);
            let path = dir.join(&name);
            std::fs::write(&path, format!("line {i}\n")).unwrap();
        }
        cleanup_old_log_files(dir);
        let remaining = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(LOG_FILE_PREFIX))
            .count();
        assert!(
            remaining <= MAX_LOG_FILES,
            "should keep at most {MAX_LOG_FILES} files, got {remaining}"
        );
    }
}
