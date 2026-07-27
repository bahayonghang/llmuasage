use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};

use anyhow::Result;
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing_appender::non_blocking::{ErrorCounter, NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::paths::AppPaths;

const DEFAULT_FILE_LEVEL: &str = "warn";
const MAX_LOG_FILE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_TOTAL_LOG_BYTES: u64 = 30 * 1024 * 1024;
const MAX_LOG_FILES: usize = 7;
const MAX_LOG_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAINTENANCE_INTERVAL_BYTES: u64 = 1024 * 1024;
const RECENT_ERROR_SCAN_LIMIT: usize = 200;
const TAIL_READ_BLOCK_BYTES: usize = 8 * 1024;
const LOG_FILE_PREFIX: &str = "llmusage.ndjson";

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();
static LOG_ERROR_COUNTER: OnceLock<ErrorCounter> = OnceLock::new();
static LOG_MAINTENANCE_ERRORS: AtomicU64 = AtomicU64::new(0);

/// One structured entry read back from a retained runtime log shard.
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

/// Runtime log status exposed by diagnostics and `llmusage logs`.
#[derive(Debug, Clone, Serialize)]
pub struct LogsRuntimeStatus {
    /// Most recently modified structured log shard, or the legacy base path.
    pub path: String,
    /// Whether at least one retained log shard exists.
    pub exists: bool,
    /// Size of the current shard.
    pub size_bytes: u64,
    /// Number of retained log shards.
    pub retained_files: usize,
    /// Total bytes across retained log shards.
    pub total_size_bytes: u64,
    /// Number of `ERROR` entries in the most recent scan window.
    pub recent_error_count: usize,
    /// Events dropped because the non-blocking logging queue was full.
    pub dropped_event_count: u64,
    /// Rotation or retention failures recorded without writing another log event.
    pub maintenance_error_count: u64,
}

#[derive(Clone, Copy)]
struct LogLimits {
    shard_bytes: u64,
    total_bytes: u64,
    max_files: usize,
    max_age: Duration,
    maintenance_bytes: u64,
}

impl Default for LogLimits {
    fn default() -> Self {
        Self {
            shard_bytes: MAX_LOG_FILE_BYTES,
            total_bytes: MAX_TOTAL_LOG_BYTES,
            max_files: MAX_LOG_FILES,
            max_age: MAX_LOG_AGE,
            maintenance_bytes: MAINTENANCE_INTERVAL_BYTES,
        }
    }
}

struct RotatingLogWriter {
    logs_dir: PathBuf,
    current_path: PathBuf,
    current: File,
    current_size: u64,
    retained_bytes: u64,
    bytes_since_maintenance: u64,
    sequence: u64,
    current_day: NaiveDate,
    limits: LogLimits,
}

impl RotatingLogWriter {
    fn new(logs_dir: PathBuf, limits: LogLimits) -> io::Result<Self> {
        std::fs::create_dir_all(&logs_dir)?;
        let (current_path, current, sequence, current_day) = create_log_shard(&logs_dir, 0)?;
        let current_size = current.metadata()?.len();
        let mut writer = Self {
            logs_dir,
            current_path,
            current,
            current_size,
            retained_bytes: current_size,
            bytes_since_maintenance: 0,
            sequence,
            current_day,
            limits,
        };
        writer.maintain();
        Ok(writer)
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.current.flush()?;
        let next_sequence = self.sequence.saturating_add(1);
        let (path, file, sequence, current_day) = create_log_shard(&self.logs_dir, next_sequence)?;
        self.current = file;
        self.current_path = path;
        self.current_size = 0;
        self.sequence = sequence;
        self.current_day = current_day;
        self.bytes_since_maintenance = 0;
        self.maintain();
        Ok(())
    }

    fn maintain(&mut self) {
        match cleanup_log_files(
            &self.logs_dir,
            Some(&self.current_path),
            self.limits,
            &mut |path| std::fs::remove_file(path),
        ) {
            Ok(total_bytes) => self.retained_bytes = total_bytes,
            Err(error) => record_maintenance_error(&error),
        }
        self.bytes_since_maintenance = 0;
    }

    fn write_record(&mut self, buf: &[u8]) -> io::Result<()> {
        if buf.is_empty() {
            return Ok(());
        }

        let next_size = self.current_size.saturating_add(buf.len() as u64);
        if (self.current_size > 0 && next_size > self.limits.shard_bytes
            || self.current_day != Utc::now().date_naive())
            && let Err(error) = self.rotate()
        {
            record_maintenance_error(&error);
        }

        // The non-blocking worker passes one formatted event to write_all.
        // Keep that NDJSON record intact even when one unusually large event
        // must temporarily exceed the normal shard budget.
        let was_within_total_budget = self.retained_bytes <= self.limits.total_bytes;
        self.current.write_all(buf)?;
        let written = buf.len() as u64;
        self.current_size = self.current_size.saturating_add(written);
        self.retained_bytes = self.retained_bytes.saturating_add(written);
        self.bytes_since_maintenance = self.bytes_since_maintenance.saturating_add(written);
        if self.bytes_since_maintenance >= self.limits.maintenance_bytes
            || (was_within_total_budget && self.retained_bytes > self.limits.total_bytes)
        {
            self.maintain();
        }
        Ok(())
    }
}

impl Write for RotatingLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_record(buf)?;
        Ok(buf.len())
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.write_record(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.current.flush()
    }
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
        let file_writer = RotatingLogWriter::new(paths.logs_dir.clone(), LogLimits::default())?;
        let (writer, guard) = NonBlockingBuilder::default().finish(file_writer);
        let error_counter = writer.error_counter();
        let file_layer = fmt::layer()
            .json()
            .with_writer(writer)
            .with_current_span(false)
            .with_span_list(false)
            .with_filter(file_filter);
        let _ = LOG_ERROR_COUNTER.set(error_counter);
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
    runtime_status_with_metrics(
        paths,
        LOG_ERROR_COUNTER.get(),
        LOG_MAINTENANCE_ERRORS.load(Ordering::Relaxed),
    )
}

fn runtime_status_with_metrics(
    paths: &AppPaths,
    error_counter: Option<&ErrorCounter>,
    maintenance_error_count: u64,
) -> Result<LogsRuntimeStatus> {
    let files = log_files(&paths.logs_dir);
    let current = files.first();
    let display_path = current
        .map(|file| &file.path)
        .unwrap_or(&paths.log_file_path)
        .display()
        .to_string();
    let recent_error_count =
        read_recent_log_entries(paths, RECENT_ERROR_SCAN_LIMIT, Some("error"), None)?.len();
    Ok(LogsRuntimeStatus {
        path: display_path,
        exists: current.is_some(),
        size_bytes: current.map_or(0, |file| file.size),
        retained_files: files.len(),
        total_size_bytes: files.iter().map(|file| file.size).sum(),
        recent_error_count,
        dropped_event_count: error_counter.map_or(0, |counter| counter.dropped_lines() as u64),
        maintenance_error_count,
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
    let scan_limit = limit.saturating_mul(8).max(limit).min(2_000);
    let mut newest_chunks = Vec::new();
    let mut scanned = 0;
    for file in log_files(&paths.logs_dir) {
        let wanted = scan_limit.saturating_sub(scanned);
        if wanted == 0 {
            break;
        }
        let chunk = match read_tail_lines(&file.path, wanted) {
            Ok(tail) => tail.lines,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        scanned += chunk.len();
        newest_chunks.push(chunk);
    }

    let mut entries = newest_chunks
        .into_iter()
        .rev()
        .flatten()
        .filter_map(|line| parse_log_entry(&line))
        .filter(|entry| {
            min_level.is_none_or(|level| level_allows(entry.level.as_str(), level))
                && command.is_none_or(|wanted| entry.command.as_deref() == Some(wanted))
        })
        .collect::<Vec<_>>();
    if entries.len() > limit {
        entries.drain(0..entries.len() - limit);
    }
    Ok(entries)
}

/// Returns the most recently modified runtime log shard, if one exists.
pub fn current_log_file(logs_dir: &Path) -> Option<PathBuf> {
    log_files(logs_dir).into_iter().next().map(|file| file.path)
}

#[derive(Debug)]
struct LogFileInfo {
    modified: SystemTime,
    path: PathBuf,
    size: u64,
}

fn log_files(logs_dir: &Path) -> Vec<LogFileInfo> {
    scan_log_files(logs_dir).unwrap_or_default()
}

fn scan_log_files(logs_dir: &Path) -> io::Result<Vec<LogFileInfo>> {
    let entries = match std::fs::read_dir(logs_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let Some(file) = log_file_info(&entry)? else {
            continue;
        };
        files.push(file);
    }
    files.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| right.path.cmp(&left.path))
    });
    Ok(files)
}

fn log_file_info(entry: &std::fs::DirEntry) -> io::Result<Option<LogFileInfo>> {
    if !is_log_file_name(&entry.file_name()) {
        return Ok(None);
    }
    let path = entry.path();
    // DirEntry metadata can report a stale size for an open file on Windows.
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(metadata.is_file().then(|| LogFileInfo {
        modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        path,
        size: metadata.len(),
    }))
}

fn is_log_file_name(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name == LOG_FILE_PREFIX
        || name
            .strip_prefix(LOG_FILE_PREFIX)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

fn create_log_shard(
    logs_dir: &Path,
    first_sequence: u64,
) -> io::Result<(PathBuf, File, u64, NaiveDate)> {
    let timestamp = Utc::now();
    let date = timestamp.format("%Y-%m-%d");
    let instant = timestamp.timestamp_micros();
    for offset in 0..1_000 {
        let sequence = first_sequence.saturating_add(offset);
        let path = logs_dir.join(format!(
            "{LOG_FILE_PREFIX}.{date}.{instant:020}.{sequence:03}"
        ));
        match OpenOptions::new().create_new(true).append(true).open(&path) {
            Ok(file) => return Ok((path, file, sequence, timestamp.date_naive())),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique runtime log shard",
    ))
}

fn cleanup_log_files<F>(
    logs_dir: &Path,
    active_path: Option<&Path>,
    limits: LogLimits,
    remove: &mut F,
) -> io::Result<u64>
where
    F: FnMut(&Path) -> io::Result<()>,
{
    let mut files = scan_log_files(logs_dir)?;
    files.reverse();
    let now = SystemTime::now();
    let mut total_bytes = files.iter().map(|file| file.size).sum::<u64>();
    let mut file_count = files.len();

    for file in files {
        if active_path.is_some_and(|active| active == file.path) {
            continue;
        }
        let expired = now
            .duration_since(file.modified)
            .is_ok_and(|age| age > limits.max_age);
        if !expired && file_count <= limits.max_files && total_bytes <= limits.total_bytes {
            continue;
        }
        match remove(&file.path) {
            Ok(()) => {
                file_count = file_count.saturating_sub(1);
                total_bytes = total_bytes.saturating_sub(file.size);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                file_count = file_count.saturating_sub(1);
                total_bytes = total_bytes.saturating_sub(file.size);
            }
            Err(error) => record_maintenance_error(&error),
        }
    }
    Ok(total_bytes)
}

fn record_maintenance_error(_error: &io::Error) {
    LOG_MAINTENANCE_ERRORS.fetch_add(1, Ordering::Relaxed);
}

fn file_filter() -> Option<EnvFilter> {
    let raw = std::env::var("LLMUSAGE_LOG").unwrap_or_else(|_| DEFAULT_FILE_LEVEL.to_string());
    if raw.eq_ignore_ascii_case("off") {
        return None;
    }
    Some(EnvFilter::new(normalize_level(&raw)))
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

struct TailLines {
    lines: Vec<String>,
    #[cfg(test)]
    bytes_read: u64,
}

/// Reads complete tail lines by walking fixed-size blocks backward.
fn read_tail_lines(path: &Path, max_lines: usize) -> io::Result<TailLines> {
    if max_lines == 0 {
        return Ok(TailLines {
            lines: Vec::new(),
            #[cfg(test)]
            bytes_read: 0,
        });
    }
    let mut file = File::open(path)?;
    let mut position = file.seek(SeekFrom::End(0))?;
    let mut chunks = Vec::new();
    let mut newline_count = 0;
    #[cfg(test)]
    let mut bytes_read = 0;

    while position > 0 && newline_count <= max_lines {
        let chunk_len = position.min(TAIL_READ_BLOCK_BYTES as u64) as usize;
        position -= chunk_len as u64;
        file.seek(SeekFrom::Start(position))?;
        let mut chunk = vec![0; chunk_len];
        file.read_exact(&mut chunk)?;
        newline_count += chunk.iter().filter(|byte| **byte == b'\n').count();
        #[cfg(test)]
        {
            bytes_read += chunk_len as u64;
        }
        chunks.push(chunk);
    }

    chunks.reverse();
    let bytes = chunks.concat();
    let start = if position > 0 {
        bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |index| index + 1)
    } else {
        0
    };
    let text = String::from_utf8_lossy(&bytes[start..]);
    let mut lines = text
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.len() > max_lines {
        lines.drain(0..lines.len() - max_lines);
    }
    Ok(TailLines {
        lines,
        #[cfg(test)]
        bytes_read,
    })
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
    use std::{
        collections::HashSet,
        sync::mpsc::{Receiver, SyncSender, sync_channel},
    };
    use tempfile::TempDir;

    fn write_ndjson_lines(path: &Path, start: usize, count: usize) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        for index in start..start + count {
            writeln!(
                file,
                r#"{{"timestamp":"2026-07-25T00:00:{:02}Z","level":"WARN","fields":{{"message":"line {index}"}}}}"#,
                index % 60
            )
            .unwrap();
        }
    }

    fn test_limits() -> LogLimits {
        LogLimits {
            shard_bytes: 256,
            total_bytes: 768,
            max_files: 7,
            max_age: MAX_LOG_AGE,
            maintenance_bytes: 64,
        }
    }

    #[test]
    fn one_process_rotates_repeatedly_and_enforces_total_budget() {
        let temp = TempDir::new().unwrap();
        let mut writer = RotatingLogWriter::new(temp.path().to_path_buf(), test_limits()).unwrap();
        for index in 0..100 {
            let record = format!("event {index:03}: {}\n", "x".repeat(80));
            writer.write_all(record.as_bytes()).unwrap();
            let files = log_files(temp.path());
            assert!(files.len() <= test_limits().max_files);
            assert!(files.iter().map(|file| file.size).sum::<u64>() <= test_limits().total_bytes);
        }
        writer.maintain();
        let files = log_files(temp.path());
        assert!(files.len() > 1, "one process should create multiple shards");
        assert!(files.iter().all(|file| file.size <= 256));
        assert!(files.iter().map(|file| file.size).sum::<u64>() <= 768);
    }

    #[test]
    fn total_budget_is_enforced_before_periodic_maintenance() {
        let temp = TempDir::new().unwrap();
        let limits = LogLimits {
            shard_bytes: 96,
            total_bytes: 288,
            max_files: 7,
            max_age: MAX_LOG_AGE,
            maintenance_bytes: 12,
        };
        let mut writer = RotatingLogWriter::new(temp.path().to_path_buf(), limits).unwrap();

        for _ in 0..73 {
            writer.write_all(b"abc\n").unwrap();
        }
        writer.flush().unwrap();

        let files = log_files(temp.path());
        let total_bytes = files
            .iter()
            .map(|file| std::fs::metadata(&file.path).unwrap().len())
            .sum::<u64>();
        assert!(files.len() <= limits.max_files);
        assert!(total_bytes <= limits.total_bytes);
    }

    #[test]
    fn production_tracing_pipeline_rotates_complete_json_events() {
        let temp = TempDir::new().unwrap();
        let limits = LogLimits {
            shard_bytes: 256,
            total_bytes: 10_000,
            max_files: 7,
            max_age: MAX_LOG_AGE,
            maintenance_bytes: 10_000,
        };
        let file_writer = RotatingLogWriter::new(temp.path().to_path_buf(), limits).unwrap();
        let (writer, guard) = NonBlockingBuilder::default()
            .lossy(false)
            .finish(file_writer);
        let subscriber = tracing_subscriber::registry().with(
            fmt::layer()
                .json()
                .with_current_span(false)
                .with_span_list(false)
                .with_writer(writer),
        );
        let payload = "x".repeat(512);

        tracing::subscriber::with_default(subscriber, || {
            for event_index in 0..3 {
                tracing::warn!(event_index, %payload, "production boundary");
            }
        });
        drop(guard);

        let files = log_files(temp.path());
        assert!(files.len() > 1);
        let mut event_count = 0;
        for file in files {
            let contents = std::fs::read_to_string(file.path).unwrap();
            assert!(contents.ends_with('\n'));
            for line in contents.lines() {
                serde_json::from_str::<Value>(line).unwrap();
                event_count += 1;
            }
        }
        assert_eq!(event_count, 3);
    }

    #[test]
    fn rotation_never_splits_one_ndjson_record() {
        let temp = TempDir::new().unwrap();
        let limits = LogLimits {
            shard_bytes: 12,
            total_bytes: 128,
            maintenance_bytes: 128,
            ..test_limits()
        };
        let mut writer = RotatingLogWriter::new(temp.path().to_path_buf(), limits).unwrap();
        writer.write_all(b"first-row\n").unwrap();
        writer.write_all(b"second-row\n").unwrap();
        writer.flush().unwrap();

        let contents = log_files(temp.path())
            .into_iter()
            .map(|file| std::fs::read_to_string(file.path).unwrap())
            .collect::<HashSet<_>>();
        assert_eq!(
            contents,
            HashSet::from(["first-row\n".to_string(), "second-row\n".to_string()])
        );
    }

    #[test]
    fn rotation_creation_failure_keeps_current_shard_and_retries() {
        let temp = TempDir::new().unwrap();
        let paths = AppPaths::with_root(temp.path().join(".llmusage")).unwrap();
        let limits = LogLimits {
            shard_bytes: 12,
            total_bytes: 128,
            maintenance_bytes: 128,
            ..test_limits()
        };
        let mut writer = RotatingLogWriter::new(paths.logs_dir.clone(), limits).unwrap();
        writer.write_all(b"first\n").unwrap();
        let original_logs_dir = writer.logs_dir.clone();
        let original_path = writer.current_path.clone();
        let unavailable_dir = temp.path().join("not-a-directory");
        std::fs::write(&unavailable_dir, b"file").unwrap();
        writer.logs_dir = unavailable_dir;
        let before = LOG_MAINTENANCE_ERRORS.load(Ordering::Relaxed);

        writer.write_all(b"second\n").unwrap();
        writer.flush().unwrap();
        assert_eq!(writer.current_path, original_path);
        assert_eq!(
            std::fs::read_to_string(&original_path).unwrap(),
            "first\nsecond\n"
        );
        assert!(LOG_MAINTENANCE_ERRORS.load(Ordering::Relaxed) > before);

        writer.logs_dir = original_logs_dir;
        writer.write_all(b"third\n").unwrap();
        writer.flush().unwrap();
        assert_ne!(writer.current_path, original_path);
        let status = runtime_status(&paths).unwrap();
        assert!(status.maintenance_error_count > before);
        assert_eq!(status.retained_files, 2);
    }

    #[test]
    fn open_current_shard_is_included_in_runtime_totals() {
        let temp = TempDir::new().unwrap();
        let paths = AppPaths::with_root(temp.path().join(".llmusage")).unwrap();
        let mut writer = RotatingLogWriter::new(paths.logs_dir.clone(), test_limits()).unwrap();
        writer.write_all(b"open current shard\n").unwrap();
        writer.flush().unwrap();

        let status = runtime_status(&paths).unwrap();
        assert_eq!(status.size_bytes, 19);
        assert_eq!(status.total_size_bytes, 19);
    }

    #[test]
    fn one_process_rotates_when_the_utc_day_changes() {
        let temp = TempDir::new().unwrap();
        let mut writer = RotatingLogWriter::new(temp.path().to_path_buf(), test_limits()).unwrap();
        writer.write_all(b"day one\n").unwrap();
        writer.current_day = Utc::now().date_naive() - chrono::Duration::days(1);
        writer.write_all(b"day two\n").unwrap();
        assert_eq!(log_files(temp.path()).len(), 2);
    }

    #[test]
    fn tail_reads_across_shards_in_chronological_order() {
        let temp = TempDir::new().unwrap();
        let paths = AppPaths::with_root(temp.path().join(".llmusage")).unwrap();
        std::fs::create_dir_all(&paths.logs_dir).unwrap();
        let older = paths
            .logs_dir
            .join(format!("{LOG_FILE_PREFIX}.2026-07-25.001"));
        let newer = paths
            .logs_dir
            .join(format!("{LOG_FILE_PREFIX}.2026-07-25.002"));
        write_ndjson_lines(&older, 0, 3);
        std::thread::sleep(Duration::from_millis(20));
        write_ndjson_lines(&newer, 3, 3);

        let entries = read_recent_log_entries(&paths, 5, None, None).unwrap();
        let messages = entries
            .iter()
            .map(|entry| entry.message.as_deref().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(messages, ["line 1", "line 2", "line 3", "line 4", "line 5"]);
    }

    #[test]
    fn reverse_tail_reads_only_requested_tail_blocks() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("large.ndjson");
        write_ndjson_lines(&path, 0, 100_000);
        let file_size = std::fs::metadata(&path).unwrap().len();

        let tail = read_tail_lines(&path, 10).unwrap();
        assert_eq!(tail.lines.len(), 10);
        assert!(tail.bytes_read <= (TAIL_READ_BLOCK_BYTES * 2) as u64);
        assert!(tail.bytes_read * 100 < file_size);
    }

    #[test]
    fn vanished_log_entry_is_ignored_during_scan() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join(format!("{LOG_FILE_PREFIX}.vanished"));
        std::fs::write(&path, b"line\n").unwrap();
        let entry = std::fs::read_dir(temp.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        std::fs::remove_file(path).unwrap();

        assert!(log_file_info(&entry).unwrap().is_none());
    }

    #[test]
    fn cleanup_failure_is_counted_and_retried_without_panicking() {
        let temp = TempDir::new().unwrap();
        for index in 0..5 {
            std::fs::write(
                temp.path().join(format!("{LOG_FILE_PREFIX}.{index:03}")),
                vec![b'x'; 200],
            )
            .unwrap();
        }
        let before = LOG_MAINTENANCE_ERRORS.load(Ordering::Relaxed);
        let mut blocked = HashSet::new();
        cleanup_log_files(temp.path(), None, test_limits(), &mut |path| {
            if blocked.insert(path.to_path_buf()) {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "occupied"))
            } else {
                std::fs::remove_file(path)
            }
        })
        .unwrap();
        assert!(LOG_MAINTENANCE_ERRORS.load(Ordering::Relaxed) > before);

        cleanup_log_files(temp.path(), None, test_limits(), &mut |path| {
            std::fs::remove_file(path)
        })
        .unwrap();
        let files = log_files(temp.path());
        assert!(files.iter().map(|file| file.size).sum::<u64>() <= 768);
    }

    #[test]
    fn retention_enforces_file_count_and_age() {
        let temp = TempDir::new().unwrap();
        for index in 0..5 {
            std::fs::write(
                temp.path().join(format!("{LOG_FILE_PREFIX}.{index:03}")),
                b"line\n",
            )
            .unwrap();
        }
        let count_limits = LogLimits {
            total_bytes: u64::MAX,
            max_files: 2,
            ..test_limits()
        };
        cleanup_log_files(temp.path(), None, count_limits, &mut |path| {
            std::fs::remove_file(path)
        })
        .unwrap();
        assert_eq!(log_files(temp.path()).len(), 2);

        std::thread::sleep(Duration::from_millis(10));
        let age_limits = LogLimits {
            total_bytes: u64::MAX,
            max_files: usize::MAX,
            max_age: Duration::from_millis(1),
            ..test_limits()
        };
        cleanup_log_files(temp.path(), None, age_limits, &mut |path| {
            std::fs::remove_file(path)
        })
        .unwrap();
        assert!(log_files(temp.path()).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn occupied_windows_file_is_counted_then_removed_after_release() {
        use std::os::windows::fs::OpenOptionsExt;

        let temp = TempDir::new().unwrap();
        let locked_path = temp.path().join(format!("{LOG_FILE_PREFIX}.locked"));
        std::fs::write(&locked_path, vec![b'x'; 200]).unwrap();
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked_path)
            .unwrap();
        let limits = LogLimits {
            total_bytes: 100,
            maintenance_bytes: 16,
            ..test_limits()
        };
        let before = LOG_MAINTENANCE_ERRORS.load(Ordering::Relaxed);

        let mut writer = RotatingLogWriter::new(temp.path().to_path_buf(), limits).unwrap();
        assert!(locked_path.exists());
        assert!(LOG_MAINTENANCE_ERRORS.load(Ordering::Relaxed) > before);

        drop(locked);
        writer.write_all(b"retry maintenance\n").unwrap();
        assert!(!locked_path.exists());
    }

    struct BlockingWriter {
        started: Option<SyncSender<()>>,
        release: Receiver<()>,
    }

    impl Write for BlockingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if let Some(started) = self.started.take() {
                let _ = started.send(());
                let _ = self.release.recv();
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn full_non_blocking_queue_increments_dropped_counter() {
        let temp = TempDir::new().unwrap();
        let paths = AppPaths::with_root(temp.path().join(".llmusage")).unwrap();
        let (started_tx, started_rx) = sync_channel(0);
        let (release_tx, release_rx) = sync_channel(0);
        let sink = BlockingWriter {
            started: Some(started_tx),
            release: release_rx,
        };
        let (mut writer, guard) = NonBlockingBuilder::default()
            .buffered_lines_limit(1)
            .lossy(true)
            .finish(sink);
        let counter = writer.error_counter();

        writer.write_all(b"worker-blocking event\n").unwrap();
        started_rx.recv().unwrap();
        writer.write_all(b"queued event\n").unwrap();
        writer.write_all(b"dropped event\n").unwrap();
        assert_eq!(counter.dropped_lines(), 1);
        assert_eq!(
            runtime_status_with_metrics(&paths, Some(&counter), 0)
                .unwrap()
                .dropped_event_count,
            1
        );

        release_tx.send(()).unwrap();
        drop(writer);
        drop(guard);
    }

    #[test]
    fn tail_handles_utf8_and_partial_final_line() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("utf8.ndjson");
        std::fs::write(&path, "ignored\n中文日志\npartial-final").unwrap();
        let tail = read_tail_lines(&path, 2).unwrap();
        assert_eq!(tail.lines, ["中文日志", "partial-final"]);
    }
}
