//! Pi / Oh My Pi agent session JSONL parser.
//!
//! Pi and Oh My Pi persist the same session transcript shape under
//! `<root>/<project>/agent_<session>.jsonl`. They share this parse
//! implementation and register as two sources: `pi` enumerates `PI_AGENT_DIR`
//! or `~/.pi/agent/sessions`, and `omp` enumerates `~/.omp/agent/sessions`.
//! Overlapping paths belong to `pi`. Only assistant `message` records that
//! carry a `usage` block become [`UsageEvent`]s; `title`/`session`/
//! `model_change`/thinking-level metadata lines are ignored. Each retained
//! record maps 1:1 to one event, so there is no cumulative-delta bookkeeping
//! like Codex.

use std::{
    collections::HashMap,
    fs::File,
    future::Future,
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
    models::{
        ParseIssues, ProjectInfo, SessionInfo, SourceKind, UsageEvent, UsageTokens, UsageToolCall,
        UsageTurn,
    },
    parsers::{
        ProgressSink, SourceParser, SourceSyncStats, SyncEvent,
        behavior::{
            apply_turn_retries, extract_pi_tools, tool_calls_from_evidence, turn_from_tools,
        },
        file_progress::{FileProgress, FileProgressCounter},
        file_state::{
            BoundedJsonlReader, CandidateFile, FileReplayMode, JsonlReadStatus,
            JsonlRecordDisposition, decide_file_replay, finalize_cursor, should_rescan_file,
        },
        source_files,
    },
    project::ProjectResolver,
    store::{FileCursor, Store, SyncRunWriter, SyncShard},
    util::{bucket_start_from_rfc3339, hash_string},
};

/// Stable fallback model when a Pi assistant message omits `model`. The raw
/// source model string (e.g. `gpt-5.5`) is otherwise preserved verbatim.
const FALLBACK_MODEL: &str = "pi";

/// Session headers sit after optional title/metadata lines. Scan only this
/// many head records so the extra read stays bounded and independent of the
/// incremental cursor.
const SESSION_HEADER_SCAN_LIMIT: usize = 32;

#[derive(Debug, Clone)]
struct PiShardPlan {
    files: Vec<CandidateFile>,
}

#[derive(Debug, Default)]
struct PiShardOutput {
    events: Vec<UsageEvent>,
    turns: Vec<UsageTurn>,
    tool_calls: Vec<UsageToolCall>,
    cursors: Vec<FileCursor>,
    reset_path_hashes: Vec<String>,
    events_seen: usize,
    events_replayed: usize,
    bytes_scanned: u64,
    seen_file_paths: Vec<String>,
    parse_issues: ParseIssues,
}

#[derive(Debug)]
struct PiParseResult {
    end_offset: u64,
    events: Vec<UsageEvent>,
    turns: Vec<UsageTurn>,
    tool_calls: Vec<UsageToolCall>,
    parse_issues: ParseIssues,
    cancelled: bool,
}

/// Shared Pi-format session parser. `pi` and `omp` register two instances.
#[derive(Clone, Copy)]
pub struct PiFormatParser {
    source: SourceKind,
    list_files: fn() -> source_files::SourceFileListing,
}

impl PiFormatParser {
    pub fn pi() -> Self {
        Self {
            source: SourceKind::Pi,
            list_files: source_files::list_pi_session_files,
        }
    }

    pub fn omp() -> Self {
        Self {
            source: SourceKind::Omp,
            list_files: source_files::list_omp_session_files,
        }
    }
}

impl SourceParser for PiFormatParser {
    fn source(&self) -> SourceKind {
        self.source
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
        Box::pin(sync_pi_format(
            *self,
            store,
            writer,
            parallelism,
            recent_cutoff,
            cancel,
            progress,
        ))
    }
}

async fn sync_pi_format(
    parser: PiFormatParser,
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
    recent_cutoff: Option<DateTime<Utc>>,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
) -> Result<SourceSyncStats> {
    let source = parser.source;
    let list_files = parser.list_files;
    /*
     * ========================================================================
     * 步骤1：并行解析 Pi 格式会话真源
     * ========================================================================
     * 目标：
     * 1) 按实例的发现函数枚举候选文件
     * 2) 只把缺失、追加或改写的文件送去解析
     * 3) 返回 event / cursor / reset 指令给单 writer 统一落库
     */
    info!(source = %source, "开始同步 Pi 格式会话真源");

    // 1.1 构建按 project 目录分片的候选文件计划
    let parse_started = Instant::now();
    let listing = list_files();
    let inventory_paths = listing.file_paths();
    store.source_files().mark_inventory_seen(
        source,
        "local",
        &inventory_paths,
        writer.run_started_at(),
    )?;
    let inventory_error = listing.error_summary();
    let files = listing.paths;
    let total_files = files.len();
    let discovery_roots = match source {
        SourceKind::Pi => source_files::pi_session_roots(),
        SourceKind::Omp => vec![source_files::omp_session_root()],
        _ => Vec::new(),
    };
    let cursor_map = store.cursors().load_file_cursors(source, "local")?;

    let mut shards = HashMap::<PathBuf, Vec<CandidateFile>>::new();
    let mut changed_files = 0usize;
    for file_path in files {
        let key = file_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let existing = file_path
            .to_str()
            .and_then(|raw| cursor_map.get(raw).cloned());
        if should_rescan_file(&file_path, existing.as_ref())? {
            changed_files += 1;
            shards.entry(key).or_default().push(CandidateFile {
                path: file_path,
                existing,
            });
        }
    }

    // 1.2 控制并发度并行解析分片
    let mut events_seen = 0usize;
    let mut events_replayed = 0usize;
    let mut bytes_scanned = 0u64;
    let mut inserted = 0usize;
    let mut write_ms = 0u64;
    let mut parse_issues = ParseIssues::default();
    let mut plans = shards
        .into_values()
        .map(|files| PiShardPlan { files })
        .collect::<Vec<_>>();
    plans.sort_by_key(|plan| plan.files.first().map(|file| file.path.clone()));
    let planned_files = plans.iter().map(|plan| plan.files.len()).sum::<usize>();
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source,
            files_total: planned_files as u64,
        },
    );
    let (mut file_progress, file_progress_counter) = FileProgress::new();

    let width = parallelism.max(1);
    'batches: for batch in plans.chunks(width) {
        if cancel.is_cancelled() {
            break;
        }
        let mut tasks = Vec::new();
        for plan in batch {
            let plan = plan.clone();
            let counter = file_progress_counter.clone();
            let task_cancel = cancel.clone();
            let discovery_roots = discovery_roots.clone();
            tasks.push(task::spawn_blocking(move || {
                parse_pi_shard(source, plan, discovery_roots, counter, task_cancel)
            }));
        }

        let batch_outputs = file_progress
            .wait_for_all(tasks, |files_scanned| {
                emit_progress(
                    &mut progress,
                    SyncEvent::Progress {
                        source,
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
                apply_recent_cutoff(&mut shard, cutoff);
            }
            events_seen += shard.events_seen;
            events_replayed += shard.events_replayed;
            bytes_scanned += shard.bytes_scanned;
            parse_issues.merge(shard.parse_issues);

            let completed_files = file_progress.boundary_snapshot();
            emit_progress(
                &mut progress,
                SyncEvent::Progress {
                    source,
                    files_scanned: completed_files,
                    records_imported: inserted as u64,
                    current_file: None,
                },
            );

            // 1.3 把 reset / event / cursor 协议交给单写入端原子提交
            let commit = writer.commit_shard(SyncShard {
                source,
                reset_path_hashes: shard.reset_path_hashes,
                events: shard.events,
                cursors: shard.cursors,
                seen_file_paths: shard.seen_file_paths,
                raw_records: Vec::new(),
                turns: shard.turns,
                tool_calls: shard.tool_calls,
                ..SyncShard::new(source)
            })?;
            inserted += commit.events_inserted;
            write_ms += commit.write_ms;
            emit_progress(
                &mut progress,
                SyncEvent::Progress {
                    source,
                    files_scanned: completed_files,
                    records_imported: inserted as u64,
                    current_file: None,
                },
            );
        }
    }

    let mut stats = SourceSyncStats {
        source,
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
        bytes_scanned = stats.bytes_scanned,
        malformed_lines = stats.parse_issues.malformed_lines,
        oversized_lines = stats.parse_issues.oversized_lines,
        "完成 Pi 格式会话真源解析"
    );
    Ok(stats)
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
}

fn apply_recent_cutoff(shard: &mut PiShardOutput, cutoff: &DateTime<Utc>) {
    shard
        .events
        .retain(|event| crate::parsers::timestamp_in_recent_window(&event.event_at, Some(cutoff)));
    shard
        .turns
        .retain(|turn| crate::parsers::timestamp_in_recent_window(&turn.started_at, Some(cutoff)));
    shard
        .tool_calls
        .retain(|call| crate::parsers::timestamp_in_recent_window(&call.occurred_at, Some(cutoff)));
    shard.events_seen = shard.events.len();
    shard.events_replayed = shard.events.len();
    shard.cursors.clear();
    shard.reset_path_hashes.clear();
}

fn parse_pi_shard(
    source: SourceKind,
    plan: PiShardPlan,
    discovery_roots: Vec<PathBuf>,
    progress: FileProgressCounter,
    cancel: CancellationToken,
) -> Result<PiShardOutput> {
    let mut output = PiShardOutput::default();
    let mut resolver = ProjectResolver::default();

    for candidate in plan.files {
        if cancel.is_cancelled() {
            break;
        }
        let existing = candidate.existing.clone();
        let decision = decide_file_replay(candidate)?;
        output
            .seen_file_paths
            .push(decision.snapshot.path.to_string_lossy().to_string());
        let path_hash = hash_string(&decision.snapshot.path.to_string_lossy());

        let parsed = parse_session_file(
            source,
            &decision.snapshot.path,
            &path_hash,
            decision.start_offset,
            &discovery_roots,
            &mut resolver,
            &cancel,
        )?;
        output.parse_issues.merge(parsed.parse_issues);
        if parsed.cancelled {
            break;
        }
        output.bytes_scanned += decision
            .snapshot
            .file_size
            .saturating_sub(decision.start_offset);
        output.events_seen += parsed.events.len();
        if decision.replay_mode == FileReplayMode::Reparse && existing.is_some() {
            output.events_replayed += parsed.events.len();
            output.reset_path_hashes.push(path_hash);
        }
        output.events.extend(parsed.events);
        output.turns.extend(parsed.turns);
        output.tool_calls.extend(parsed.tool_calls);
        output.cursors.push(finalize_cursor(
            &decision.snapshot.path,
            &decision.snapshot,
            parsed.end_offset,
            None,
            None,
        ));
        progress.advance_file();
    }

    Ok(output)
}

/// Parses a Pi session JSONL file starting at `start_offset`.
///
/// Each retained assistant `usage` line becomes one [`UsageEvent`]. The record's
/// **start byte offset** is used as its stable position: byte offsets are
/// identical whether the file is reparsed from `0` or appended from the stored
/// cursor offset, so the derived `event_key` is idempotent across re-sync.
/// Session-header metadata is read from a separate bounded head scan and does
/// not change the incremental cursor.
#[allow(clippy::too_many_arguments)]
fn parse_session_file(
    source: SourceKind,
    file_path: &Path,
    path_hash: &str,
    start_offset: u64,
    discovery_roots: &[PathBuf],
    resolver: &mut ProjectResolver,
    cancel: &CancellationToken,
) -> Result<PiParseResult> {
    let file_len = std::fs::metadata(file_path)?.len();
    if start_offset >= file_len {
        return Ok(PiParseResult {
            end_offset: file_len,
            events: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            parse_issues: ParseIssues::default(),
            cancelled: cancel.is_cancelled(),
        });
    }

    let header = peek_session_header(source, file_path)?;
    let session = build_session(file_path, path_hash, header.id.as_deref());
    let project = resolve_pi_project(resolver, header.cwd.as_deref(), file_path, discovery_roots)?;

    let file = File::open(file_path)?;
    let mut reader = BoundedJsonlReader::new(file, start_offset)?;
    let mut events = Vec::new();
    let mut turns = Vec::new();
    let mut tool_calls = Vec::new();
    let mut parse_issues = ParseIssues::default();
    let status =
        reader.read_json_records(source, path_hash, cancel, &mut parse_issues, |record| {
            let value = record.value;
            // `type` is absent or `"message"` on usage-bearing records; anything else
            // (title/session/model_change/thinking-level metadata) is ignored.
            if value
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|message_type| message_type != "message")
            {
                return Ok(JsonlRecordDisposition::Ignored);
            }
            let Some(message) = value.get("message") else {
                return Ok(JsonlRecordDisposition::Ignored);
            };
            if message.get("role").and_then(Value::as_str) != Some("assistant") {
                return Ok(JsonlRecordDisposition::Ignored);
            }
            let Some(usage) = message.get("usage") else {
                return Ok(JsonlRecordDisposition::Accepted);
            };
            let Some(tokens) = parse_pi_tokens(usage) else {
                return Ok(JsonlRecordDisposition::Accepted);
            };
            let Some(timestamp_raw) = value.get("timestamp").and_then(Value::as_str) else {
                return Ok(JsonlRecordDisposition::Malformed);
            };
            let Ok(timestamp) = chrono::DateTime::parse_from_rfc3339(timestamp_raw) else {
                return Ok(JsonlRecordDisposition::Malformed);
            };
            let event_at = timestamp.with_timezone(&chrono::Utc).to_rfc3339();
            let Some(hour_start) = bucket_start_from_rfc3339(&event_at) else {
                return Ok(JsonlRecordDisposition::Malformed);
            };
            let model = message
                .get("model")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| FALLBACK_MODEL.to_string());
            let provider_label = read_trimmed_str(message, "provider").unwrap_or_default();

            let logical_identity = format!(
                "{path_hash}\0{}\0{event_at}\0{model}\0{}\0{}\0{}\0{}\0{}\0{}",
                record.start_offset,
                tokens.input_tokens,
                tokens.cache_read_tokens,
                tokens.cache_creation_tokens,
                tokens.output_tokens,
                tokens.reasoning_output_tokens,
                tokens.total_tokens,
            );
            let event = UsageEvent {
                event_key: format!("{}:{}", source.as_str(), hash_string(&logical_identity)),
                source,
                provider_label,
                model,
                event_at,
                hour_start,
                tokens,
                project: project.clone(),
                session: session.clone(),
                source_cost: parse_pi_source_cost(usage),
            };
            let tools = extract_pi_tools(&value);
            let mut turn = turn_from_tools(&event, &tools);
            apply_turn_retries(&mut turn, read_retry_attempt(message));
            turns.push(turn);
            tool_calls.extend(tool_calls_from_evidence(&event, tools));
            events.push(event);
            Ok(JsonlRecordDisposition::Accepted)
        })?;

    Ok(PiParseResult {
        end_offset: reader.complete_offset(),
        events,
        turns,
        tool_calls,
        parse_issues,
        cancelled: status == JsonlReadStatus::Cancelled,
    })
}

#[cfg(test)]
pub(super) fn bounded_contract_parse(file_path: &Path) -> Result<(ParseIssues, u64, bool)> {
    bounded_contract_parse_for(SourceKind::Pi, file_path)
}

#[cfg(test)]
pub(super) fn bounded_contract_parse_omp(file_path: &Path) -> Result<(ParseIssues, u64, bool)> {
    bounded_contract_parse_for(SourceKind::Omp, file_path)
}

#[cfg(test)]
fn bounded_contract_parse_for(
    source: SourceKind,
    file_path: &Path,
) -> Result<(ParseIssues, u64, bool)> {
    let mut resolver = ProjectResolver::default();
    let result = parse_session_file(
        source,
        file_path,
        "bounded-contract-path-hash",
        0,
        &[],
        &mut resolver,
        &CancellationToken::new(),
    )?;
    Ok((result.parse_issues, result.end_offset, result.cancelled))
}

/// Maps a Pi `usage.cost` object to [`SourceCost`].
///
/// Missing `cost`, a non-object `cost`, or a non-finite `total` become
/// `None` so the writer can fall through to catalog pricing.
fn parse_pi_source_cost(usage: &Value) -> Option<crate::models::SourceCost> {
    let cost = usage.get("cost")?;
    let obj = cost.as_object()?;
    let total = obj.get("total").and_then(Value::as_f64)?;
    if !total.is_finite() {
        return None;
    }
    Some(crate::models::SourceCost {
        total,
        input: finite_f64(obj.get("input")),
        output: finite_f64(obj.get("output")),
        cache_read: finite_f64(obj.get("cacheRead")),
        cache_write: finite_f64(obj.get("cacheWrite")),
    })
}

fn finite_f64(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

/// Maps a Pi `usage` object to normalized [`UsageTokens`].
///
/// Channels are clamped to non-negative. A trustworthy `totalTokens` (> 0) is
/// authoritative; otherwise the total is the saturating sum of the four
/// input/cache/output channels, each once. `reasoningTokens` is a separate
/// diagnostic channel and is never added to output or total. Returns `None`
/// when every observed field (channels, reasoning, and total) is zero so the
/// caller skips the record.
fn parse_pi_tokens(usage: &Value) -> Option<UsageTokens> {
    let input_tokens = read_i64(usage, "input").unwrap_or_default().max(0);
    let output_tokens = read_i64(usage, "output").unwrap_or_default().max(0);
    let cache_read_tokens = read_i64(usage, "cacheRead").unwrap_or_default().max(0);
    let cache_creation_tokens = read_i64(usage, "cacheWrite").unwrap_or_default().max(0);
    let reasoning_output_tokens = read_i64(usage, "reasoningTokens")
        .unwrap_or_default()
        .max(0);
    let upstream_total = read_i64(usage, "totalTokens").unwrap_or_default().max(0);

    if input_tokens == 0
        && output_tokens == 0
        && cache_read_tokens == 0
        && cache_creation_tokens == 0
        && reasoning_output_tokens == 0
        && upstream_total == 0
    {
        return None;
    }

    let total_tokens = if upstream_total > 0 {
        upstream_total
    } else {
        input_tokens
            .saturating_add(cache_read_tokens)
            .saturating_add(cache_creation_tokens)
            .saturating_add(output_tokens)
    };

    Some(UsageTokens {
        input_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens,
    })
}

fn read_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn read_retry_attempt(message: &Value) -> i64 {
    message
        .get("retryRecovery")
        .and_then(|recovery| recovery.get("attempt"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn read_trimmed_str(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[derive(Debug, Default, Clone)]
struct PiSessionHeader {
    id: Option<String>,
    cwd: Option<String>,
}

/// Reads the file head for `type == "session"`. Skips title/metadata lines,
/// stops on the first session record, and does not touch the incremental
/// cursor used by [`parse_session_file`].
fn peek_session_header(source: SourceKind, file_path: &Path) -> Result<PiSessionHeader> {
    let file = File::open(file_path)?;
    let mut reader = BoundedJsonlReader::new(file, 0)?;
    let mut header = PiSessionHeader::default();
    let mut issues = ParseIssues::default();
    let cancel = CancellationToken::new();
    let mut scanned = 0usize;
    let _ = reader.read_json_records(source, "session-header", &cancel, &mut issues, |record| {
        scanned += 1;
        if record.value.get("type").and_then(Value::as_str) == Some("session") {
            header = PiSessionHeader {
                id: read_trimmed_str(&record.value, "id"),
                cwd: read_trimmed_str(&record.value, "cwd"),
            };
            return Ok(JsonlRecordDisposition::Stop);
        }
        if scanned >= SESSION_HEADER_SCAN_LIMIT {
            return Ok(JsonlRecordDisposition::Stop);
        }
        Ok(JsonlRecordDisposition::Ignored)
    })?;
    Ok(header)
}

fn resolve_pi_project(
    resolver: &mut ProjectResolver,
    header_cwd: Option<&str>,
    file_path: &Path,
    discovery_roots: &[PathBuf],
) -> Result<Option<ProjectInfo>> {
    if let Some(cwd) = header_cwd.map(str::trim).filter(|value| !value.is_empty())
        && let Some(info) = resolver.resolve(Path::new(cwd))?
    {
        return Ok(Some(info));
    }
    Ok(project_from_encoded_dir(file_path, discovery_roots))
}

/// Decodes an Oh My Pi / Pi encoded workspace directory name.
///
/// Local sample: `--D--Documents-Code-CLI-llmusage--`. Path separators become
/// `-` and the value is wrapped in `--`. This is not percent-encoding, and
/// separators are not reversible; the fallback only needs a readable stable
/// label.
fn decode_encoded_workspace_dir(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let wrapped = trimmed.starts_with("--") && trimmed.ends_with("--") && trimmed.len() >= 4;
    let inner = if wrapped {
        &trimmed[2..trimmed.len() - 2]
    } else {
        trimmed
    };
    if inner.is_empty() {
        return None;
    }
    let decoded = if inner.len() >= 3
        && inner.as_bytes()[0].is_ascii_alphabetic()
        && inner[1..].starts_with("--")
    {
        format!(
            "{}:/{}",
            inner.chars().next()?,
            inner[3..].replace('-', "/")
        )
    } else if wrapped {
        inner.replace('-', "/")
    } else {
        inner.to_string()
    };
    let decoded = decoded.trim_matches('/').trim();
    if decoded.is_empty() {
        None
    } else {
        Some(decoded.to_string())
    }
}

fn strip_prefix_under_root(file_path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(relative) = file_path.strip_prefix(root)
        && !relative.as_os_str().is_empty()
    {
        return Some(relative.to_path_buf());
    }
    let file_canonical = std::fs::canonicalize(file_path).ok()?;
    let root_canonical = std::fs::canonicalize(root).ok()?;
    let relative = file_canonical.strip_prefix(root_canonical).ok()?;
    if relative.as_os_str().is_empty() {
        None
    } else {
        Some(relative.to_path_buf())
    }
}

fn project_from_encoded_dir(file_path: &Path, roots: &[PathBuf]) -> Option<ProjectInfo> {
    let relative = roots
        .iter()
        .find_map(|root| strip_prefix_under_root(file_path, root))?;
    let mut normals = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name),
            _ => None,
        });
    // First segment under the discovery root is the workspace dir. A following
    // component is required so a file sitting on the root is not treated as one.
    let encoded = normals.next()?.to_str()?;
    normals.next()?;
    let workspace = decode_encoded_workspace_dir(encoded)?;
    let workspace = workspace.trim();
    if workspace.is_empty() {
        return None;
    }
    let label = workspace
        .rsplit(['/', '\\'])
        .find(|value| !value.is_empty())
        .unwrap_or(workspace)
        .to_string();
    let workspace_hash = hash_string(workspace);
    Some(ProjectInfo {
        project_hash: workspace_hash.clone(),
        project_label: label,
        project_ref: None,
        repo_root_hash: workspace_hash.clone(),
        path_hash: workspace_hash,
    })
}

/// Extracts the session id from a Pi session file stem.
///
/// Layout: `.../sessions/PROJECT/agent_SESSION.jsonl`, so the session id is the
/// file stem portion after the first `_` (mirrors the reference Pi adapter).
fn extract_session_id(path: &Path) -> String {
    let filename = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    filename
        .split_once('_')
        .map_or(filename, |(_, session)| session)
        .to_string()
}

fn build_session(
    file_path: &Path,
    path_hash: &str,
    header_id: Option<&str>,
) -> Option<SessionInfo> {
    let filename_id = extract_session_id(file_path);
    let session_id = header_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(|| filename_id.clone(), str::to_string);
    Some(SessionInfo {
        session_label: Some(filename_id),
        session_id,
        source_path_hash: Some(path_hash.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        FALLBACK_MODEL, PiParseResult, PiShardOutput, apply_recent_cutoff,
        decode_encoded_workspace_dir, parse_session_file,
    };
    use crate::{models::SourceKind, project::ProjectResolver, util::hash_string};
    use chrono::{TimeZone, Utc};
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    /// Builds a synthetic Pi session file under a fake `.omp` layout so
    /// `extract_session_id` resolves the `sess-abc-123` stem segment.
    fn write_session_file(content: &str) -> (TempDir, PathBuf, PathBuf) {
        let dir = TempDir::new().expect("temp dir");
        let root = dir.path().join(".omp").join("agent").join("sessions");
        let path = root.join("project-a").join("agent_sess-abc-123.jsonl");
        fs::create_dir_all(path.parent().unwrap()).expect("create layout");
        fs::write(&path, content).expect("write session file");
        (dir, path, root)
    }

    fn parse_path(
        source: SourceKind,
        path: &Path,
        path_hash: &str,
        start_offset: u64,
        roots: &[PathBuf],
    ) -> PiParseResult {
        let mut resolver = ProjectResolver::default();
        parse_session_file(
            source,
            path,
            path_hash,
            start_offset,
            roots,
            &mut resolver,
            &CancellationToken::new(),
        )
        .expect("parse session file")
    }

    fn parse(content: &str) -> Vec<crate::models::UsageEvent> {
        parse_full(content).events
    }

    fn parse_full(content: &str) -> PiParseResult {
        let complete = format!("{}\n", content.trim_end_matches('\n'));
        let (_dir, path, root) = write_session_file(&complete);
        parse_path(SourceKind::Pi, &path, "path-hash", 0, &[root])
    }

    fn assistant_with_tools(
        timestamp: &str,
        content: serde_json::Value,
        retry_attempt: Option<i64>,
    ) -> String {
        let mut message = serde_json::json!({
            "role": "assistant",
            "model": "gpt-5.5",
            "provider": "openrouter",
            "usage": {"input": 10, "output": 5},
            "content": content,
        });
        if let Some(attempt) = retry_attempt {
            message["retryRecovery"] = serde_json::json!({
                "kind": "auto-retry",
                "status": "recovered",
                "attempt": attempt,
                "recovery": "plain",
            });
        }
        serde_json::json!({
            "type": "message",
            "timestamp": timestamp,
            "message": message,
        })
        .to_string()
    }

    fn usage_line(provider: Option<&str>) -> String {
        let mut message = serde_json::json!({
            "role": "assistant",
            "model": "deepseek-v4-flash",
            "usage": {"input": 10, "output": 5}
        });
        if let Some(provider) = provider {
            message["provider"] = serde_json::json!(provider);
        }
        serde_json::json!({
            "type": "message",
            "timestamp": "2026-01-02T00:00:00.000Z",
            "message": message,
        })
        .to_string()
    }

    fn session_line(id: Option<&str>, cwd: Option<&str>) -> String {
        let mut value = serde_json::json!({"type": "session"});
        if let Some(id) = id {
            value["id"] = serde_json::json!(id);
        }
        if let Some(cwd) = cwd {
            value["cwd"] = serde_json::json!(cwd);
        }
        value.to_string()
    }

    fn write_git_repo(repo_root: &Path) {
        fs::create_dir_all(repo_root.join(".git")).expect("git dir");
        fs::write(
            repo_root.join(".git").join("config"),
            "[remote \"origin\"]\n    url = https://github.com/example/llmusage.git\n",
        )
        .expect("git config");
    }

    #[test]
    fn maps_channels_with_authoritative_total_and_separate_reasoning() {
        // All four channels + reasoning + a trustworthy upstream total.
        let content = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":100,"output":50,"cacheRead":40,"cacheWrite":8,"reasoningTokens":10,"totalTokens":333}}}"#;

        let events = parse(content);

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.source, crate::models::SourceKind::Pi);
        // R4: the raw source model string is preserved verbatim (no store prefix,
        // no whitelist).
        assert_eq!(event.model, "gpt-5.5");
        assert_eq!(event.tokens.input_tokens, 100);
        assert_eq!(event.tokens.output_tokens, 50);
        assert_eq!(event.tokens.cache_read_tokens, 40);
        assert_eq!(event.tokens.cache_creation_tokens, 8);
        // R3: reasoning is stored separately and NEVER folded into output/total.
        assert_eq!(event.tokens.reasoning_output_tokens, 10);
        // A trustworthy `totalTokens` is authoritative (not the channel sum, not
        // channel-sum + reasoning).
        assert_eq!(event.tokens.total_tokens, 333);
        assert_eq!(
            event
                .session
                .as_ref()
                .map(|session| session.session_id.as_str()),
            Some("sess-abc-123")
        );
        assert_eq!(
            event
                .project
                .as_ref()
                .map(|project| project.project_label.as_str()),
            Some("project-a")
        );
    }

    #[test]
    fn falls_back_to_channel_sum_when_total_absent() {
        // No `totalTokens`: the total is the sum of the four channels; reasoning
        // stays separate and is excluded.
        let content = r#"{"type":"message","timestamp":"2026-01-02T00:05:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":100,"output":50,"cacheRead":40,"cacheWrite":8,"reasoningTokens":10}}}"#;

        let events = parse(content);

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.tokens.reasoning_output_tokens, 10);
        assert_eq!(event.tokens.total_tokens, 100 + 40 + 8 + 50);
    }

    #[test]
    fn preserves_unknown_future_model() {
        let content = r#"{"type":"message","timestamp":"2026-01-02T00:10:00.000Z","message":{"role":"assistant","model":"codex-auto-review-next","usage":{"input":10,"output":5}}}"#;

        let events = parse(content);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].model, "codex-auto-review-next");
    }

    #[test]
    fn falls_back_to_stable_model_when_absent() {
        let content = r#"{"type":"message","timestamp":"2026-01-02T00:15:00.000Z","message":{"role":"assistant","usage":{"input":10,"output":5}}}"#;

        let events = parse(content);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].model, FALLBACK_MODEL);
    }

    #[test]
    fn skips_metadata_user_zero_and_malformed_lines() {
        let content = concat!(
            // title metadata carries no usage semantics even if it mentions them.
            r#"{"type":"title","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","usage":{"input":999,"output":999}}}"#,
            "\n",
            // session metadata line.
            r#"{"type":"session","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","usage":{"input":888,"output":888}}}"#,
            "\n",
            // model_change metadata line.
            r#"{"type":"model_change","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","usage":{"input":777,"output":777}}}"#,
            "\n",
            // a user message must not become a usage event.
            r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"user","usage":{"input":666,"output":666}}}"#,
            "\n",
            // an all-zero assistant record is skipped.
            r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"reasoningTokens":0,"totalTokens":0}}}"#,
            "\n",
            // a malformed line (with the prefilter substrings) must not fail the file.
            "garbage usage message line",
            "\n",
            // the single real assistant usage record.
            r#"{"type":"message","timestamp":"2026-01-02T00:20:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":100,"output":50}}}"#,
        );

        let events = parse(content);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tokens.input_tokens, 100);
        assert_eq!(events[0].tokens.output_tokens, 50);
    }

    #[test]
    fn distinct_records_get_distinct_idempotent_event_keys() {
        let content = concat!(
            r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":10,"output":5}}}"#,
            "\n",
            r#"{"type":"message","timestamp":"2026-01-02T00:05:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":20,"output":6}}}"#,
            "\n",
        );

        let (_dir, path, root) = write_session_file(content);
        let first = parse_path(
            SourceKind::Pi,
            &path,
            "path-hash",
            0,
            std::slice::from_ref(&root),
        );
        let second = parse_path(SourceKind::Pi, &path, "path-hash", 0, &[root]);

        assert_eq!(first.events.len(), 2);
        assert_ne!(first.events[0].event_key, first.events[1].event_key);
        // Reparsing from offset 0 yields identical keys (idempotent re-sync).
        assert_eq!(first.events[0].event_key, second.events[0].event_key);
        assert_eq!(first.events[1].event_key, second.events[1].event_key);
        assert_eq!(first.end_offset, content.len() as u64);
    }

    /// DATA-001 contract: a partial last line (no trailing '\n') must not
    /// advance the durable cursor.
    #[test]
    fn partial_tail_does_not_advance_cursor() {
        let complete_line = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":100,"output":50,"cacheRead":40,"cacheWrite":8,"reasoningTokens":10,"totalTokens":333}}}"#;
        let complete = format!("{complete_line}\n");
        let partial = format!("{complete_line}\n{complete_line}"); // last line missing '\n'

        let (_dir, path, root) = write_session_file(&partial);

        let result = parse_path(
            SourceKind::Pi,
            &path,
            "path-hash",
            0,
            std::slice::from_ref(&root),
        );
        assert_eq!(result.events.len(), 2, "valid EOF records are parsed");
        assert_eq!(
            result.end_offset,
            complete.len() as u64,
            "cursor must not include the partial tail"
        );
        let partial_event_key = result.events[1].event_key.clone();

        // Simulate the tool flushing the final newline and re-syncing.
        let full = format!("{complete_line}\n{complete_line}\n");
        std::fs::write(&path, &full).expect("write full");
        let incremental = parse_path(
            SourceKind::Pi,
            &path,
            "path-hash",
            result.end_offset,
            &[root],
        );
        assert_eq!(
            incremental.events.len(),
            1,
            "incremental sync picks up completed line"
        );
        assert_eq!(incremental.events[0].event_key, partial_event_key);
    }

    #[test]
    fn pi_and_omp_event_keys_differ_and_are_idempotent() {
        let content = concat!(
            r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":10,"output":5}}}"#,
            "\n",
        );
        let (_dir, path, root) = write_session_file(content);
        let pi_first = parse_path(
            SourceKind::Pi,
            &path,
            "path-hash",
            0,
            std::slice::from_ref(&root),
        );
        let pi_second = parse_path(
            SourceKind::Pi,
            &path,
            "path-hash",
            0,
            std::slice::from_ref(&root),
        );
        let omp_first = parse_path(
            SourceKind::Omp,
            &path,
            "path-hash",
            0,
            std::slice::from_ref(&root),
        );
        let omp_second = parse_path(SourceKind::Omp, &path, "path-hash", 0, &[root]);

        assert_eq!(pi_first.events.len(), 1);
        assert_eq!(omp_first.events.len(), 1);
        assert!(pi_first.events[0].event_key.starts_with("pi:"));
        assert!(omp_first.events[0].event_key.starts_with("omp:"));
        assert_ne!(pi_first.events[0].event_key, omp_first.events[0].event_key);
        assert_eq!(pi_first.events[0].event_key, pi_second.events[0].event_key);
        assert_eq!(
            omp_first.events[0].event_key,
            omp_second.events[0].event_key
        );
        assert_eq!(pi_first.events[0].source, SourceKind::Pi);
        assert_eq!(omp_first.events[0].source, SourceKind::Omp);
    }

    #[test]
    fn decode_encoded_workspace_dir_uses_dash_wrap_not_percent_encoding() {
        let sample = "--D--Documents-Code-CLI-llmusage--";
        assert_eq!(
            decode_encoded_workspace_dir(sample).as_deref(),
            Some("D:/Documents/Code/CLI/llmusage")
        );
        assert_ne!(
            decode_encoded_workspace_dir(sample).as_deref(),
            Some(sample)
        );
        // Percent sequences stay literal: this encoding is not percent-decoding.
        assert_eq!(
            decode_encoded_workspace_dir("--foo%2Fbar--").as_deref(),
            Some("foo%2Fbar")
        );
        assert_eq!(
            decode_encoded_workspace_dir("project-a").as_deref(),
            Some("project-a")
        );
        assert_eq!(decode_encoded_workspace_dir("----"), None);
        assert_eq!(decode_encoded_workspace_dir("   "), None);
    }

    #[test]
    fn source_cost_maps_object_and_tolerates_missing_or_non_object() {
        let with_cost = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"deepseek-v4-flash","usage":{"input":1000,"output":200,"cacheRead":500,"cacheWrite":0,"totalTokens":1700,"cost":{"input":0.01,"output":0.02,"cacheRead":0.002,"cacheWrite":0,"total":0.032}}}}"#;
        let events = parse(with_cost);
        assert_eq!(events.len(), 1);
        let cost = events[0].source_cost.as_ref().expect("source cost");
        assert!((cost.total - 0.032).abs() < 1e-12);
        assert_eq!(cost.input, Some(0.01));
        assert_eq!(cost.output, Some(0.02));
        assert_eq!(cost.cache_read, Some(0.002));
        assert_eq!(cost.cache_write, Some(0.0));

        let zero_total = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"grok-4.6","usage":{"input":10,"output":5,"cost":{"input":0,"output":0,"total":0}}}}"#;
        let events = parse(zero_total);
        assert_eq!(events.len(), 1, "total==0 must still emit an event");
        let cost = events[0]
            .source_cost
            .as_ref()
            .expect("zero total is still a cost object");
        assert_eq!(cost.total, 0.0);

        let missing = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"grok-4.6","usage":{"input":10,"output":5}}}"#;
        let events = parse(missing);
        assert_eq!(events.len(), 1, "missing cost must still emit an event");
        assert!(events[0].source_cost.is_none());

        let non_object = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"grok-4.6","usage":{"input":10,"output":5,"cost":0.5}}}"#;
        let events = parse(non_object);
        assert_eq!(events.len(), 1, "non-object cost must still emit an event");
        assert!(events[0].source_cost.is_none());
    }

    #[test]
    fn provider_label_uses_trimmed_message_provider_without_model_inference() {
        let cases = [
            (Some("openai-codex"), "openai-codex"),
            (Some("  xai-oauth  "), "xai-oauth"),
            (Some(""), ""),
            (None, ""),
        ];
        for (provider, expected) in cases {
            let events = parse(&format!("{}\n", usage_line(provider)));
            assert_eq!(events.len(), 1, "provider={provider:?} must persist");
            assert_eq!(events[0].provider_label, expected);
            assert_eq!(events[0].model, "deepseek-v4-flash");
        }
    }

    #[test]
    fn title_before_session_header_supplies_session_id() {
        let title = r#"{"type":"title","title":"Demo"}"#;
        let header = session_line(Some("header-uuid"), None);
        let usage = usage_line(Some("openrouter"));
        let prefix = format!("{title}\n{header}\n");
        let content = format!("{prefix}{usage}\n");
        let (_dir, path, root) = write_session_file(&content);
        let parsed = parse_path(
            SourceKind::Pi,
            &path,
            "path-hash",
            0,
            std::slice::from_ref(&root),
        );
        assert_eq!(parsed.events.len(), 1);
        let session = parsed.events[0].session.as_ref().expect("session");
        assert_eq!(session.session_id, "header-uuid");
        assert_eq!(session.session_label.as_deref(), Some("sess-abc-123"));
        assert!(
            !parsed.events[0].event_key.contains("header-uuid"),
            "session_id must not enter event_key"
        );

        let incremental = parse_path(
            SourceKind::Pi,
            &path,
            "path-hash",
            prefix.len() as u64,
            &[root],
        );
        assert_eq!(incremental.events.len(), 1);
        assert_eq!(
            incremental.events[0]
                .session
                .as_ref()
                .map(|session| session.session_id.as_str()),
            Some("header-uuid"),
            "header scan must start at byte 0 even when the cursor is past the header"
        );
        assert_eq!(incremental.events[0].event_key, parsed.events[0].event_key);
    }

    #[test]
    fn missing_session_header_falls_back_to_filename_session_id() {
        let content = format!(
            "{}\n{}\n",
            r#"{"type":"title","title":"Demo"}"#,
            usage_line(Some("openrouter")),
        );
        let (_dir, path, root) = write_session_file(&content);
        let parsed = parse_path(SourceKind::Pi, &path, "path-hash", 0, &[root]);
        assert_eq!(parsed.events.len(), 1);
        let session = parsed.events[0].session.as_ref().expect("session");
        assert_eq!(session.session_id, "sess-abc-123");
        assert_eq!(session.session_label.as_deref(), Some("sess-abc-123"));
    }

    #[test]
    fn session_header_id_does_not_change_event_key() {
        let message = usage_line(Some("openrouter"));
        let header_a = session_line(Some("id-aaaa"), None);
        let header_b = session_line(Some("id-bbbb"), None);
        assert_eq!(header_a.len(), header_b.len());
        let (_dir, path, root) = write_session_file(&format!("{header_a}\n{message}\n"));
        let first = parse_path(
            SourceKind::Pi,
            &path,
            "path-hash",
            0,
            std::slice::from_ref(&root),
        );
        fs::write(&path, format!("{header_b}\n{message}\n")).expect("rewrite header id");
        let second = parse_path(SourceKind::Pi, &path, "path-hash", 0, &[root]);
        assert_eq!(first.events[0].event_key, second.events[0].event_key);
        assert_eq!(
            first.events[0]
                .session
                .as_ref()
                .map(|session| session.session_id.as_str()),
            Some("id-aaaa")
        );
        assert_eq!(
            second.events[0]
                .session
                .as_ref()
                .map(|session| session.session_id.as_str()),
            Some("id-bbbb")
        );
    }

    #[test]
    fn session_cwd_uses_project_resolver_for_git_repo() {
        let repo = TempDir::new().expect("git repo");
        write_git_repo(repo.path());
        let content = format!(
            "{}\n{}\n",
            session_line(Some("hdr"), Some(&repo.path().to_string_lossy())),
            usage_line(Some("openai-codex")),
        );
        let (_dir, path, root) = write_session_file(&content);
        let parsed = parse_path(SourceKind::Pi, &path, "path-hash", 0, &[root]);
        let project = parsed.events[0].project.as_ref().expect("project");
        assert_eq!(
            project.project_ref.as_deref(),
            Some("https://github.com/example/llmusage")
        );
        assert_eq!(project.project_label, "example/llmusage");
        assert_eq!(
            project.repo_root_hash,
            hash_string(&repo.path().to_string_lossy())
        );
        assert_eq!(project.project_hash, project.repo_root_hash);
    }

    #[test]
    fn nested_and_top_level_share_encoded_dir_project_hash() {
        let dir = TempDir::new().expect("temp dir");
        let root = dir.path().join("sessions");
        let encoded = "--D--Documents-Code-CLI-llmusage--";
        let run_dir = "2026-08-22T16-26-20-289Z_aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let top = root.join(encoded).join("agent_x.jsonl");
        let nested = root.join(encoded).join(run_dir).join("DiffJudge.jsonl");
        fs::create_dir_all(top.parent().unwrap()).expect("top parent");
        fs::create_dir_all(nested.parent().unwrap()).expect("nested parent");
        let body = format!("{}\n", usage_line(Some("openrouter")));
        fs::write(&top, &body).expect("write top");
        fs::write(&nested, &body).expect("write nested");

        let top_parsed = parse_path(
            SourceKind::Omp,
            &top,
            "path-hash",
            0,
            std::slice::from_ref(&root),
        );
        let nested_parsed = parse_path(
            SourceKind::Omp,
            &nested,
            "path-hash",
            0,
            std::slice::from_ref(&root),
        );
        let top_project = top_parsed.events[0].project.as_ref().expect("top project");
        let nested_project = nested_parsed.events[0]
            .project
            .as_ref()
            .expect("nested project");
        assert_eq!(top_project.project_hash, nested_project.project_hash);
        assert_eq!(top_project.project_label, "llmusage");
        assert_eq!(
            top_project.project_hash,
            hash_string("D:/Documents/Code/CLI/llmusage")
        );
        assert_ne!(top_project.project_hash, hash_string(run_dir));
        assert_ne!(
            top_project.project_hash,
            hash_string(
                &decode_encoded_workspace_dir(run_dir).unwrap_or_else(|| run_dir.to_string())
            )
        );
        assert_eq!(
            nested_parsed.events[0]
                .session
                .as_ref()
                .map(|session| session.session_label.as_deref()),
            Some(Some("DiffJudge"))
        );
    }

    #[test]
    fn missing_cwd_and_encoded_dir_leave_project_none() {
        let dir = TempDir::new().expect("temp dir");
        let root = dir.path().join("sessions");
        let path = root.join("agent_orphan.jsonl");
        fs::create_dir_all(&root).expect("root");
        fs::write(&path, format!("{}\n", usage_line(Some("openrouter")))).expect("write");
        let parsed = parse_path(
            SourceKind::Omp,
            &path,
            "path-hash",
            0,
            std::slice::from_ref(&root),
        );
        assert_eq!(parsed.events.len(), 1);
        assert!(parsed.events[0].project.is_none());
        assert_eq!(parsed.events[0].provider_label, "openrouter");
    }

    #[test]
    fn retry_recovery_overlay_recomputes_one_shot() {
        let retried = assistant_with_tools(
            "2026-01-02T00:00:00.000Z",
            serde_json::json!([
                {"type": "toolCall", "name": "edit", "arguments": {"file_path": "src/lib.rs"}}
            ]),
            Some(1),
        );
        let retried = parse_full(&retried);
        assert_eq!(retried.events.len(), 1);
        assert_eq!(retried.turns.len(), 1);
        assert_eq!(retried.tool_calls.len(), 1);
        assert_eq!(retried.turns[0].retries, 1);
        assert!(retried.turns[0].has_edits);
        assert!(!retried.turns[0].one_shot);

        let first_shot = assistant_with_tools(
            "2026-01-02T00:01:00.000Z",
            serde_json::json!([
                {"type": "toolCall", "name": "write", "arguments": {"file_path": "src/main.rs"}}
            ]),
            None,
        );
        let first_shot = parse_full(&first_shot);
        assert_eq!(first_shot.turns.len(), 1);
        assert_eq!(first_shot.turns[0].retries, 0);
        assert!(first_shot.turns[0].has_edits);
        assert!(first_shot.turns[0].one_shot);
    }

    #[test]
    fn recent_cutoff_keeps_turns_and_tool_calls_aligned_with_events() {
        let old = assistant_with_tools(
            "2020-01-01T00:00:00.000Z",
            serde_json::json!([
                {"type": "toolCall", "name": "read", "arguments": {"file_path": "old.rs"}}
            ]),
            None,
        );
        let recent = assistant_with_tools(
            "2026-08-23T00:00:00.000Z",
            serde_json::json!([
                {"type": "toolCall", "name": "write", "arguments": {"file_path": "new.rs"}}
            ]),
            None,
        );
        let parsed = parse_full(&format!("{old}\n{recent}\n"));
        assert_eq!(parsed.events.len(), 2);
        assert_eq!(parsed.turns.len(), 2);
        assert_eq!(parsed.tool_calls.len(), 2);

        let mut shard = PiShardOutput {
            events: parsed.events,
            turns: parsed.turns,
            tool_calls: parsed.tool_calls,
            events_seen: 2,
            events_replayed: 2,
            reset_path_hashes: vec!["path-hash".to_string()],
            ..PiShardOutput::default()
        };
        let cutoff = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        apply_recent_cutoff(&mut shard, &cutoff);

        assert_eq!(shard.events.len(), 1);
        assert_eq!(shard.turns.len(), 1);
        assert_eq!(shard.tool_calls.len(), 1);
        assert_eq!(shard.events_seen, 1);
        assert_eq!(shard.events_replayed, 1);
        assert!(shard.reset_path_hashes.is_empty());
        assert_eq!(
            shard.turns[0].turn_key,
            format!("turn:{}", shard.events[0].event_key)
        );
        assert_eq!(
            shard.tool_calls[0].event_key.as_deref(),
            Some(shard.events[0].event_key.as_str())
        );
    }
}
