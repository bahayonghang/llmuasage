//! DeepSeek Harness (`dsh`) session-log parser.
//!
//! Discovery follows the tokscale `dsh-session-log` contract: `$DSH_HOME`
//! (default `~/.dsh`) / `sessions/` at any depth, file name exactly
//! `session.jsonl` or `session.jsonl.zstd`. Compression is dispatched by the
//! zstd frame magic (`0x28 B5 2F FD`), not the extension, because
//! `compression: none` writes the same records to `session.jsonl`.
//!
//! Token semantics (official DSH meter + local samples): `inputTokens` is
//! already non-cached, `outputTokens` includes reasoning, `reasoningTokens`
//! is diagnostic only, and `total = input + cache_read + cache_creation +
//! output`. There is no authoritative upstream total.
//!
//! Incremental sync uses a per-file fingerprint. A changed file resets that
//! path and, on an unbounded run, also replays the session family so a
//! rewritten owner cannot drop a fork-shared `event_key` that another copy
//! still holds.

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    future::Future,
    io::{self, BufRead, BufReader, Read},
    path::{Path, PathBuf},
    pin::Pin,
    time::Instant,
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio::task;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    models::{ParseIssues, SessionInfo, SourceKind, UsageEvent, UsageTokens},
    parsers::{
        ProgressSink, SourceParser, SourceSyncStats, SyncEvent,
        file_progress::{FileProgress, FileProgressCounter},
        file_state::{
            BoundedJsonlReader, CandidateFile, JsonlReadStatus, JsonlRecordDisposition,
            decide_file_replay, finalize_cursor, should_rescan_file,
        },
        source_files,
    },
    project::ProjectResolver,
    store::{FileCursor, Store, SyncRunWriter, SyncShard},
    util::{bucket_start_from_rfc3339, hash_string, normalize_model},
};

/// RFC 8478 zstd frame magic. Dispatch uses these bytes, not the file name.
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];
const FALLBACK_MODEL: &str = "dsh-unknown";

#[derive(Debug, Default, Clone)]
struct SessionHeader {
    id: Option<String>,
    parent_session: Option<String>,
    seed_length: Option<i64>,
    version: Option<i64>,
    cwd: Option<String>,
}

#[derive(Debug, Default)]
struct DshShardOutput {
    events: Vec<UsageEvent>,
    cursors: Vec<FileCursor>,
    reset_path_hashes: Vec<String>,
    events_seen: usize,
    events_replayed: usize,
    bytes_scanned: u64,
    seen_file_paths: Vec<String>,
    parse_issues: ParseIssues,
}

#[derive(Debug)]
struct DshParseResult {
    events: Vec<UsageEvent>,
    parse_issues: ParseIssues,
    cancelled: bool,
}

/// Treats a mid-stream zstd error as EOF so a torn tail frame keeps the
/// already-decoded prefix (DSH `readZstdPrefix` / durable-boundary semantics).
struct PrefixPreservingRead<R: Read> {
    inner: R,
    failed: bool,
}

impl<R: Read> Read for PrefixPreservingRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.failed {
            return Ok(0);
        }
        match self.inner.read(buf) {
            Ok(n) => Ok(n),
            Err(_) => {
                self.failed = true;
                Ok(0)
            }
        }
    }
}

/// DeepSeek Harness session-log parser.
pub struct DeepseekHarnessParser;

impl SourceParser for DeepseekHarnessParser {
    fn source(&self) -> SourceKind {
        SourceKind::DeepseekHarness
    }

    fn parse<'a>(
        &'a self,
        store: &'a Store,
        writer: &'a mut SyncRunWriter,
        parallelism: usize,
        recent_cutoff: Option<DateTime<Utc>>,
        cancel: &'a CancellationToken,
        progress: Option<ProgressSink<'a>>,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>> {
        Box::pin(sync_dsh(
            store,
            writer,
            parallelism,
            recent_cutoff,
            cancel,
            progress,
        ))
    }
}

async fn sync_dsh(
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
    recent_cutoff: Option<DateTime<Utc>>,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
) -> Result<SourceSyncStats> {
    /*
     * ========================================================================
     * 步骤1：解析 ~/.dsh/sessions 下的 session.jsonl(.zstd) 真源
     * ========================================================================
     * 目标：
     * 1) 只把 fingerprint 变化（或家族强制重放）的会话日志送去解析
     * 2) 流式 zstd/明文分派 → assistant/message usage → UsageEvent
     * 3) 返回 event / cursor / reset 指令给单 writer 统一落库
     */
    info!("开始同步 DeepSeek Harness 会话日志真源");

    let parse_started = Instant::now();
    let listing = source_files::list_dsh_session_files();
    let inventory_paths = listing.file_paths();
    store.source_files().mark_inventory_seen(
        SourceKind::DeepseekHarness,
        "local",
        &inventory_paths,
        writer.run_started_at(),
    )?;
    let inventory_error = listing.error_summary();
    let files = listing.paths;
    let total_files = files.len();
    let cursor_map = store
        .cursors()
        .load_file_cursors(SourceKind::DeepseekHarness, "local")?;

    let mut changed = HashSet::new();
    let mut existing_by_path = HashMap::new();
    for file_path in &files {
        let existing = file_path
            .to_str()
            .and_then(|raw| cursor_map.get(raw).cloned());
        if should_rescan_file(file_path, existing.as_ref())? {
            changed.insert(file_path.clone());
        }
        existing_by_path.insert(file_path.clone(), existing);
    }

    let headers = files
        .iter()
        .map(|path| (path.clone(), peek_session_header(path).unwrap_or_default()))
        .collect::<HashMap<_, _>>();
    if recent_cutoff.is_none() {
        expand_session_family(&mut changed, &headers);
    }

    let mut candidates = files
        .into_iter()
        .filter(|path| changed.contains(path))
        .map(|path| CandidateFile {
            existing: existing_by_path.remove(&path).flatten(),
            path,
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    let changed_files = candidates.len();

    let mut events_seen = 0usize;
    let mut events_replayed = 0usize;
    let mut bytes_scanned = 0u64;
    let mut inserted = 0usize;
    let mut write_ms = 0u64;
    let mut parse_issues = ParseIssues::default();
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source: SourceKind::DeepseekHarness,
            files_total: candidates.len() as u64,
        },
    );
    let (mut file_progress, file_progress_counter) = FileProgress::new();

    let width = parallelism.max(1);
    'batches: for batch in candidates.chunks(width) {
        if cancel.is_cancelled() {
            break;
        }
        let mut tasks = Vec::new();
        for candidate in batch {
            let candidate = candidate.clone();
            let header = headers.get(&candidate.path).cloned().unwrap_or_default();
            let counter = file_progress_counter.clone();
            let task_cancel = cancel.clone();
            tasks.push(task::spawn_blocking(move || {
                parse_dsh_file(candidate, header, counter, task_cancel)
            }));
        }

        let batch_outputs = file_progress
            .wait_for_all(tasks, |files_scanned| {
                emit_progress(
                    &mut progress,
                    SyncEvent::Progress {
                        source: SourceKind::DeepseekHarness,
                        files_scanned,
                        records_imported: inserted as u64,
                        current_file: None,
                    },
                );
            })
            .await?;
        if cancel.is_cancelled() {
            break;
        }

        for mut shard in batch_outputs {
            if cancel.is_cancelled() {
                break 'batches;
            }
            if let Some(cutoff) = recent_cutoff.as_ref() {
                shard.events.retain(|event| {
                    crate::parsers::timestamp_in_recent_window(&event.event_at, Some(cutoff))
                });
                shard.events_seen = shard.events.len();
                shard.events_replayed = shard.events.len();
                shard.cursors.clear();
                shard.reset_path_hashes.clear();
            }
            events_seen += shard.events_seen;
            events_replayed += shard.events_replayed;
            bytes_scanned += shard.bytes_scanned;
            parse_issues.merge(shard.parse_issues.clone());

            let completed_files = file_progress.boundary_snapshot();
            let commit = writer.commit_shard(SyncShard {
                source: SourceKind::DeepseekHarness,
                reset_path_hashes: shard.reset_path_hashes,
                events: shard.events,
                cursors: shard.cursors,
                seen_file_paths: shard.seen_file_paths,
                raw_records: Vec::new(),
                turns: Vec::new(),
                tool_calls: Vec::new(),
                ..SyncShard::new(SourceKind::DeepseekHarness)
            })?;
            inserted += commit.events_inserted;
            write_ms += commit.write_ms;
            emit_progress(
                &mut progress,
                SyncEvent::Progress {
                    source: SourceKind::DeepseekHarness,
                    files_scanned: completed_files,
                    records_imported: inserted as u64,
                    current_file: None,
                },
            );
        }
    }

    let mut stats = SourceSyncStats {
        source: SourceKind::DeepseekHarness,
        files_processed: total_files,
        changed_files,
        skipped_files: total_files.saturating_sub(changed_files),
        bytes_scanned,
        events_seen,
        events_replayed,
        events_inserted: inserted,
        write_ms,
        last_error: inventory_error,
        parse_issues,
        ..SourceSyncStats::default()
    };
    let total_elapsed = parse_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    stats.parse_ms = total_elapsed.saturating_sub(write_ms);

    info!(
        files_processed = stats.files_processed,
        changed_files = stats.changed_files,
        skipped_files = stats.skipped_files,
        events_seen = stats.events_seen,
        malformed_lines = stats.parse_issues.malformed_lines,
        "完成 DeepSeek Harness 会话日志真源解析"
    );
    Ok(stats)
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
}

fn expand_session_family(
    changed: &mut HashSet<PathBuf>,
    headers: &HashMap<PathBuf, SessionHeader>,
) {
    let mut by_id: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut children: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for (path, header) in headers {
        if let Some(id) = header.id.as_ref().filter(|value| !value.is_empty()) {
            by_id.entry(id.clone()).or_default().push(path.clone());
        }
        if let Some(parent) = header
            .parent_session
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            children
                .entry(parent.clone())
                .or_default()
                .push(path.clone());
        }
    }

    let mut queue = changed.iter().cloned().collect::<Vec<_>>();
    while let Some(path) = queue.pop() {
        let Some(header) = headers.get(&path) else {
            continue;
        };
        let mut related = Vec::new();
        if let Some(id) = header.id.as_ref() {
            if let Some(copies) = by_id.get(id) {
                related.extend(copies.iter().cloned());
            }
            if let Some(kids) = children.get(id) {
                related.extend(kids.iter().cloned());
            }
        }
        if let Some(parent) = header.parent_session.as_ref() {
            if let Some(parents) = by_id.get(parent) {
                related.extend(parents.iter().cloned());
            }
            if let Some(siblings) = children.get(parent) {
                related.extend(siblings.iter().cloned());
            }
        }
        for other in related {
            if changed.insert(other.clone()) {
                queue.push(other);
            }
        }
    }
}

fn parse_dsh_file(
    candidate: CandidateFile,
    peeked: SessionHeader,
    progress: FileProgressCounter,
    cancel: CancellationToken,
) -> Result<DshShardOutput> {
    let mut output = DshShardOutput::default();
    let existing = candidate.existing.clone();
    let decision = decide_file_replay(candidate)?;
    output
        .seen_file_paths
        .push(decision.snapshot.path.to_string_lossy().to_string());
    let path_hash = hash_string(&decision.snapshot.path.to_string_lossy());

    let parsed = parse_session_file(&decision.snapshot.path, &path_hash, peeked, &cancel)?;
    output.parse_issues.merge(parsed.parse_issues);
    if parsed.cancelled {
        progress.advance_file();
        return Ok(output);
    }
    output.bytes_scanned = decision.snapshot.file_size;
    output.events_seen = parsed.events.len();
    if existing.is_some() {
        output.events_replayed = parsed.events.len();
        output.reset_path_hashes.push(path_hash);
    }
    output.events = parsed.events;
    output.cursors.push(finalize_cursor(
        &decision.snapshot.path,
        &decision.snapshot,
        decision.snapshot.file_size,
        None,
        None,
    ));
    progress.advance_file();
    Ok(output)
}

fn parse_session_file(
    file_path: &Path,
    path_hash: &str,
    peeked: SessionHeader,
    cancel: &CancellationToken,
) -> Result<DshParseResult> {
    let fallback_session_id = file_path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path_hash)
        .to_string();

    let mut reader = open_session_reader(file_path)?;
    let mut events = Vec::new();
    let mut parse_issues = ParseIssues::default();
    let mut header = peeked;
    let mut fallback_route: Option<(String, String)> = None;
    let mut version_issue_recorded = false;
    let mut resolver = ProjectResolver::default();

    let status = reader.read_json_records(
        SourceKind::DeepseekHarness,
        path_hash,
        cancel,
        &mut parse_issues,
        |record| {
            let value = record.value;
            let record_type = value.get("type").and_then(Value::as_str).unwrap_or("");
            match record_type {
                "session" => {
                    header = session_header_from_value(&value);
                    if header.version.is_some_and(|version| version != 0) && !version_issue_recorded
                    {
                        version_issue_recorded = true;
                        return Ok(JsonlRecordDisposition::Malformed);
                    }
                    Ok(JsonlRecordDisposition::Ignored)
                }
                "request/header" => {
                    if let Some(route) = header_route(&value) {
                        fallback_route = Some(route);
                    }
                    Ok(JsonlRecordDisposition::Ignored)
                }
                "assistant/message" => {
                    if let Some(event) = assistant_message_to_event(
                        &value,
                        path_hash,
                        &header,
                        fallback_route.as_ref(),
                        &fallback_session_id,
                        &mut resolver,
                    ) {
                        events.push(event);
                    }
                    Ok(JsonlRecordDisposition::Accepted)
                }
                _ => Ok(JsonlRecordDisposition::Ignored),
            }
        },
    )?;

    Ok(DshParseResult {
        events,
        parse_issues,
        cancelled: status == JsonlReadStatus::Cancelled,
    })
}

fn open_session_reader(path: &Path) -> Result<BoundedJsonlReader<Box<dyn Read + Send>>> {
    let file = File::open(path)?;
    let mut peek = BufReader::new(file);
    let is_zstd = peek
        .fill_buf()?
        .get(..4)
        .is_some_and(|magic| magic == ZSTD_MAGIC);
    let reader: Box<dyn Read + Send> = if is_zstd {
        let decoder = zstd::stream::read::Decoder::new(peek)?;
        Box::new(PrefixPreservingRead {
            inner: decoder,
            failed: false,
        })
    } else {
        Box::new(peek)
    };
    Ok(BoundedJsonlReader::from_read(reader))
}

fn peek_session_header(path: &Path) -> Result<SessionHeader> {
    let mut reader = open_session_reader(path)?;
    let mut header = SessionHeader::default();
    let mut issues = ParseIssues::default();
    let cancel = CancellationToken::new();
    let _ = reader.read_json_records(
        SourceKind::DeepseekHarness,
        "peek",
        &cancel,
        &mut issues,
        |record| {
            if record.value.get("type").and_then(Value::as_str) == Some("session") {
                header = session_header_from_value(&record.value);
                return Ok(JsonlRecordDisposition::Stop);
            }
            Ok(JsonlRecordDisposition::Ignored)
        },
    )?;
    Ok(header)
}

fn session_header_from_value(value: &Value) -> SessionHeader {
    SessionHeader {
        id: read_nonempty_str(value, "id"),
        parent_session: read_nonempty_str(value, "parentSession"),
        seed_length: value.get("seedLength").and_then(Value::as_i64),
        version: value.get("version").and_then(Value::as_i64),
        cwd: read_nonempty_str(value, "cwd"),
    }
}

fn header_route(value: &Value) -> Option<(String, String)> {
    let config = value
        .pointer("/data/header/config")
        .or_else(|| value.pointer("/data/config"))?;
    let provider = read_nonempty_str(config, "provider")?;
    let model = read_nonempty_str(config, "model")?;
    Some((provider, model))
}

fn assistant_message_to_event(
    value: &Value,
    path_hash: &str,
    header: &SessionHeader,
    fallback_route: Option<&(String, String)>,
    fallback_session_id: &str,
    resolver: &mut ProjectResolver,
) -> Option<UsageEvent> {
    let seq = value.get("seq").and_then(Value::as_i64).unwrap_or(0);
    if header.seed_length.is_some_and(|seed| seq < seed) {
        return None;
    }
    let time_ms = value.get("time").and_then(Value::as_i64).unwrap_or(0);
    if time_ms <= 0 {
        return None;
    }
    let usage = value.pointer("/data/usage")?;
    let tokens = parse_dsh_tokens(usage)?;
    let source = value.pointer("/data/message/source");
    let provider = source
        .and_then(|value| read_nonempty_str(value, "provider"))
        .or_else(|| fallback_route.map(|(provider, _)| provider.clone()))
        .unwrap_or_default();
    let model = source
        .and_then(|value| read_nonempty_str(value, "model"))
        .or_else(|| fallback_route.map(|(_, model)| model.clone()))
        .map(|model| normalize_model(Some(&model)))
        .unwrap_or_else(|| FALLBACK_MODEL.to_string());
    let timestamp = DateTime::from_timestamp_millis(time_ms)?;
    let event_at = timestamp.to_rfc3339();
    let hour_start = bucket_start_from_rfc3339(&event_at)?;
    let session_id = header
        .id
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback_session_id.to_string());
    let identity = value
        .pointer("/data/message")
        .and_then(|message| read_nonempty_str(message, "id"))
        .unwrap_or_else(|| format!("sid:{session_id}"));
    let logical_identity = format!(
        "{identity}\0{time_ms}\0{provider}\0{model}\0{}\0{}\0{}\0{}\0{}",
        tokens.input_tokens,
        tokens.output_tokens,
        tokens.cache_read_tokens,
        tokens.cache_creation_tokens,
        tokens.reasoning_output_tokens,
    );
    let project = header
        .cwd
        .as_deref()
        .map(PathBuf::from)
        .and_then(|path| resolver.resolve(&path).ok().flatten());

    Some(UsageEvent {
        event_key: format!("deepseek_harness:{}", hash_string(&logical_identity)),
        source: SourceKind::DeepseekHarness,
        provider_label: provider,
        model,
        event_at,
        hour_start,
        tokens,
        project,
        session: Some(SessionInfo {
            session_label: Some(session_id.clone()),
            session_id,
            source_path_hash: Some(path_hash.to_string()),
        }),
        source_cost: None,
    })
}

fn parse_dsh_tokens(usage: &Value) -> Option<UsageTokens> {
    let input_tokens = read_i64(usage, "inputTokens").unwrap_or_default().max(0);
    let output_tokens = read_i64(usage, "outputTokens").unwrap_or_default().max(0);
    let cache_read_tokens = read_i64(usage, "cacheReadTokens")
        .unwrap_or_default()
        .max(0);
    let cache_creation_tokens = read_i64(usage, "cacheWriteTokens")
        .unwrap_or_default()
        .max(0);
    let reasoning_output_tokens = read_i64(usage, "reasoningTokens")
        .unwrap_or_default()
        .max(0);

    if input_tokens == 0
        && output_tokens == 0
        && cache_read_tokens == 0
        && cache_creation_tokens == 0
        && reasoning_output_tokens == 0
    {
        return None;
    }

    Some(UsageTokens {
        input_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens: input_tokens
            .saturating_add(cache_read_tokens)
            .saturating_add(cache_creation_tokens)
            .saturating_add(output_tokens),
    })
}

fn read_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn read_nonempty_str(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn session_line(id: &str, seed_length: Option<i64>, version: i64, cwd: &str) -> String {
        let mut value = serde_json::json!({
            "type": "session",
            "version": version,
            "id": id,
            "cwd": cwd,
        });
        if let Some(seed) = seed_length {
            value["seedLength"] = serde_json::json!(seed);
        }
        value.to_string()
    }

    fn session_line_with_parent(id: &str, parent: &str) -> String {
        serde_json::json!({
            "type": "session",
            "version": 0,
            "id": id,
            "parentSession": parent,
            "cwd": "/tmp/demo",
        })
        .to_string()
    }

    struct UsageLine {
        seq: i64,
        time_ms: i64,
        message_id: &'static str,
        input: i64,
        output: i64,
        cache_read: i64,
        reasoning: i64,
        model: &'static str,
    }

    fn usage(seq: i64, time_ms: i64, message_id: &'static str, input: i64, output: i64) -> String {
        usage_line(UsageLine {
            seq,
            time_ms,
            message_id,
            input,
            output,
            cache_read: 0,
            reasoning: 0,
            model: "m",
        })
    }

    fn usage_line(line: UsageLine) -> String {
        serde_json::json!({
            "type": "assistant/message",
            "seq": line.seq,
            "time": line.time_ms,
            "data": {
                "usage": {
                    "inputTokens": line.input,
                    "outputTokens": line.output,
                    "cacheReadTokens": line.cache_read,
                    "cacheWriteTokens": 0,
                    "reasoningTokens": line.reasoning,
                },
                "message": {
                    "id": line.message_id,
                    "source": {
                        "kind": "model",
                        "provider": "deepseek-official",
                        "model": line.model,
                    }
                }
            }
        })
        .to_string()
    }

    fn chunk_usage_line(seq: i64, time_ms: i64, input: i64, output: i64) -> String {
        serde_json::json!({
            "type": "assistant/chunk",
            "seq": seq,
            "time": time_ms,
            "data": {
                "chunk": {
                    "usage": {
                        "inputTokens": input,
                        "outputTokens": output,
                        "cacheReadTokens": 0,
                        "reasoningTokens": 0,
                    }
                }
            }
        })
        .to_string()
    }

    fn write_jsonl(dir: &Path, name: &str, lines: &[String]) -> PathBuf {
        let path = dir.join(name);
        fs_write_lines(&path, lines);
        path
    }

    fn fs_write_lines(path: &Path, lines: &[String]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, format!("{}\n", lines.join("\n"))).expect("write jsonl");
    }

    fn encode_frames(lines: &[String]) -> Vec<u8> {
        let mut out = Vec::new();
        for line in lines {
            let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 0).expect("encoder");
            encoder
                .write_all(format!("{line}\n").as_bytes())
                .expect("write frame");
            out.extend(encoder.finish().expect("finish frame"));
        }
        out
    }

    fn write_zstd(dir: &Path, name: &str, lines: &[String]) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(&path, encode_frames(lines)).expect("write zstd");
        path
    }

    fn parse_path(path: &Path) -> DshParseResult {
        parse_session_file(
            path,
            "test-path-hash",
            SessionHeader::default(),
            &CancellationToken::new(),
        )
        .expect("parse session")
    }

    #[test]
    fn assistant_message_usage_is_imported_and_chunk_duplicate_is_ignored() {
        let temp = TempDir::new().expect("temp");
        let path = write_jsonl(
            temp.path(),
            "session.jsonl",
            &[
                session_line("sess-1", None, 0, "/tmp/demo"),
                chunk_usage_line(1, 1_700_000_000_000, 10, 4),
                usage_line(UsageLine {
                    seq: 2,
                    time_ms: 1_700_000_000_000,
                    message_id: "msg-1",
                    input: 10,
                    output: 4,
                    cache_read: 0,
                    reasoning: 1,
                    model: "deepseek-v4-flash",
                }),
            ],
        );
        let parsed = parse_path(&path);
        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].tokens.input_tokens, 10);
        assert_eq!(parsed.events[0].tokens.output_tokens, 4);
        assert_eq!(parsed.events[0].tokens.reasoning_output_tokens, 1);
        assert_eq!(parsed.events[0].tokens.total_tokens, 14);
        assert_eq!(parsed.events[0].model, "deepseek-v4-flash");
        assert_eq!(parsed.events[0].provider_label, "deepseek-official");
    }

    #[test]
    fn empty_session_imports_no_events() {
        let temp = TempDir::new().expect("temp");
        let path = write_jsonl(
            temp.path(),
            "session.jsonl",
            &[
                session_line("sess-empty", None, 0, "/tmp/demo"),
                r#"{"type":"permission/preset"}"#.to_string(),
                r#"{"type":"sandbox/mode"}"#.to_string(),
            ],
        );
        let parsed = parse_path(&path);
        assert!(parsed.events.is_empty());
        assert_eq!(parsed.parse_issues.total(), 0);
    }

    #[test]
    fn all_zero_usage_and_missing_time_are_skipped() {
        let temp = TempDir::new().expect("temp");
        let path = write_jsonl(
            temp.path(),
            "session.jsonl",
            &[
                session_line("sess-1", None, 0, "/tmp/demo"),
                usage(1, 1_700_000_000_000, "zero", 0, 0),
                usage(2, 0, "notime", 5, 1),
            ],
        );
        let parsed = parse_path(&path);
        assert!(parsed.events.is_empty());
    }

    #[test]
    fn version_drift_is_counted_as_a_parse_issue() {
        let temp = TempDir::new().expect("temp");
        let path = write_jsonl(
            temp.path(),
            "session.jsonl",
            &[
                session_line("sess-1", None, 2, "/tmp/demo"),
                usage(1, 1_700_000_000_000, "msg-1", 1, 1),
            ],
        );
        let parsed = parse_path(&path);
        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.parse_issues.malformed_lines, 1);
    }

    #[test]
    fn local_sample_numbers_keep_cache_out_of_input_and_reasoning_out_of_total() {
        let tokens = parse_dsh_tokens(&serde_json::json!({
            "inputTokens": 7619,
            "outputTokens": 171,
            "cacheReadTokens": 19840,
            "reasoningTokens": 40,
        }))
        .expect("tokens");
        assert_eq!(tokens.input_tokens, 7619);
        assert_eq!(tokens.cache_read_tokens, 19840);
        assert_eq!(tokens.output_tokens, 171);
        assert_eq!(tokens.reasoning_output_tokens, 40);
        assert_eq!(tokens.total_tokens, 7619 + 19840 + 171);
    }

    #[test]
    fn official_snapshot_numbers_sum_to_dsh_meter_total() {
        let tokens = parse_dsh_tokens(&serde_json::json!({
            "inputTokens": 2885,
            "outputTokens": 25,
            "cacheReadTokens": 0,
            "cacheWriteTokens": 0,
            "reasoningTokens": 23,
        }))
        .expect("tokens");
        assert_eq!(tokens.output_tokens, 25);
        assert_eq!(tokens.reasoning_output_tokens, 23);
        assert_eq!(tokens.total_tokens, 2910);
    }

    #[test]
    fn magic_dispatch_reads_compressed_payload_from_jsonl_name() {
        let temp = TempDir::new().expect("temp");
        let lines = vec![
            session_line("sess-1", None, 0, "/tmp/demo"),
            usage(1, 1_700_000_000_000, "msg-1", 3, 2),
        ];
        let path = write_zstd(temp.path(), "session.jsonl", &lines);
        let parsed = parse_path(&path);
        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].tokens.input_tokens, 3);
    }

    #[test]
    fn magic_dispatch_reads_plain_payload_from_zstd_name() {
        let temp = TempDir::new().expect("temp");
        let path = write_jsonl(
            temp.path(),
            "session.jsonl.zstd",
            &[
                session_line("sess-1", None, 0, "/tmp/demo"),
                usage(1, 1_700_000_000_000, "msg-1", 8, 1),
            ],
        );
        let parsed = parse_path(&path);
        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].tokens.input_tokens, 8);
    }

    #[test]
    fn torn_tail_frame_keeps_already_decoded_prefix() {
        let temp = TempDir::new().expect("temp");
        let complete = vec![
            session_line("sess-1", None, 0, "/tmp/demo"),
            usage(1, 1_700_000_000_000, "msg-1", 5, 1),
        ];
        let torn = usage(2, 1_700_000_000_100, "msg-2", 9, 2);
        let mut bytes = encode_frames(&complete);
        let torn_frame = encode_frames(&[torn]);
        bytes.extend(&torn_frame[..torn_frame.len() / 2]);
        let path = temp.path().join("session.jsonl.zstd");
        std::fs::write(&path, bytes).expect("write torn");
        let parsed = parse_path(&path);
        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].tokens.input_tokens, 5);
    }

    #[test]
    fn oversized_record_is_discarded_and_counted() {
        let temp = TempDir::new().expect("temp");
        let path = temp.path().join("session.jsonl");
        let oversized = format!(
            "{{\"type\":\"noise\",\"pad\":\"{}\"}}",
            "x".repeat(5 * 1024 * 1024)
        );
        std::fs::write(
            &path,
            format!(
                "{}\n{oversized}\n{}\n",
                session_line("sess-1", None, 0, "/tmp/demo"),
                usage(1, 1_700_000_000_000, "msg-1", 2, 1)
            ),
        )
        .expect("write oversized");
        let parsed = parse_path(&path);
        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.parse_issues.oversized_lines, 1);
    }

    #[test]
    fn seed_length_skips_fork_prefix_rows() {
        let temp = TempDir::new().expect("temp");
        let path = write_jsonl(
            temp.path(),
            "session.jsonl",
            &[
                session_line("child", Some(2), 0, "/tmp/demo"),
                usage(0, 1_700_000_000_000, "parent-msg", 10, 1),
                usage(1, 1_700_000_000_010, "parent-msg-2", 11, 1),
                usage(2, 1_700_000_000_020, "child-msg", 4, 1),
            ],
        );
        let parsed = parse_path(&path);
        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].tokens.input_tokens, 4);
    }

    #[test]
    fn placeholder_message_ids_stay_separated_by_time_and_tokens() {
        let temp = TempDir::new().expect("temp");
        let path = write_jsonl(
            temp.path(),
            "session.jsonl",
            &[
                session_line("sess-1", None, 0, "/tmp/demo"),
                usage(1, 1_700_000_000_000, "REDACTED", 10, 1),
                usage(2, 1_700_000_000_100, "REDACTED", 20, 2),
            ],
        );
        let parsed = parse_path(&path);
        assert_eq!(parsed.events.len(), 2);
        assert_ne!(parsed.events[0].event_key, parsed.events[1].event_key);
    }

    #[test]
    fn identical_fork_copy_collapses_to_the_same_event_key() {
        let temp = TempDir::new().expect("temp");
        let shared = usage(1, 1_700_000_000_000, "shared-msg", 10, 1);
        let parent = write_jsonl(
            temp.path(),
            "parent/session.jsonl",
            &[session_line("parent", None, 0, "/tmp/demo"), shared.clone()],
        );
        let child = write_jsonl(
            temp.path(),
            "child/session.jsonl",
            &[session_line_with_parent("child", "parent"), shared],
        );
        let parent_parsed = parse_path(&parent);
        let child_parsed = parse_path(&child);
        assert_eq!(parent_parsed.events.len(), 1);
        assert_eq!(child_parsed.events.len(), 1);
        assert_eq!(
            parent_parsed.events[0].event_key,
            child_parsed.events[0].event_key
        );
    }

    #[test]
    fn family_expansion_includes_parent_and_child() {
        let parent = PathBuf::from("/tmp/parent/session.jsonl");
        let child = PathBuf::from("/tmp/child/session.jsonl");
        let other = PathBuf::from("/tmp/other/session.jsonl");
        let mut headers = HashMap::new();
        headers.insert(
            parent.clone(),
            SessionHeader {
                id: Some("parent".into()),
                ..SessionHeader::default()
            },
        );
        headers.insert(
            child.clone(),
            SessionHeader {
                id: Some("child".into()),
                parent_session: Some("parent".into()),
                ..SessionHeader::default()
            },
        );
        headers.insert(
            other.clone(),
            SessionHeader {
                id: Some("other".into()),
                ..SessionHeader::default()
            },
        );
        let mut changed = HashSet::from([parent.clone()]);
        expand_session_family(&mut changed, &headers);
        assert!(changed.contains(&parent));
        assert!(changed.contains(&child));
        assert!(!changed.contains(&other));
    }
}
