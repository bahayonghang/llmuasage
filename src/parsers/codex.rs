use std::{
    cell::RefCell,
    fs::File,
    future::Future,
    io::Read,
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
            BehaviorToolEvidence, extract_codex_tools, tool_calls_from_evidence, turn_from_tools,
        },
        file_progress::{FileProgress, FileProgressCounter},
        file_state::{
            BoundedJsonlReader, CandidateFile, FileReplayMode, JsonlReadStatus,
            JsonlRecordDisposition, decide_file_replay, finalize_cursor, should_rescan_file,
        },
        source_files,
    },
    project::ProjectResolver,
    store::{Store, SyncRunWriter, SyncShard},
    util::{bucket_start_from_rfc3339, hash_string, normalize_model},
};

const REPLAY_MARKER_SCAN_BYTES: usize = 16 * 1024;
const OVERSIZED_TYPE_PEEK_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone)]
struct CodexShardPlan {
    files: Vec<CandidateFile>,
}

#[derive(Debug)]
struct CodexShardOutput {
    events: Vec<UsageEvent>,
    turns: Vec<UsageTurn>,
    tool_calls: Vec<UsageToolCall>,
    cursors: Vec<crate::store::FileCursor>,
    reset_path_hashes: Vec<String>,
    events_seen: usize,
    events_replayed: usize,
    bytes_scanned: u64,
    seen_file_paths: Vec<String>,
    parse_issues: ParseIssues,
}

#[derive(Debug)]
struct RolloutParseResult {
    end_offset: u64,
    last_total: Option<UsageTokens>,
    last_model: Option<String>,
    events: Vec<UsageEvent>,
    turns: Vec<UsageTurn>,
    tool_calls: Vec<UsageToolCall>,
    parse_issues: ParseIssues,
    cancelled: bool,
}

struct CodexLiveParse<'a> {
    path_hash: &'a str,
    resolver: &'a mut ProjectResolver,
    model: Option<String>,
    totals: Option<UsageTokens>,
    session_label: Option<String>,
    current_session: Option<SessionInfo>,
    current_project: Option<ProjectInfo>,
    current_cwd: Option<String>,
    events: Vec<UsageEvent>,
    turns: Vec<UsageTurn>,
    tool_calls: Vec<UsageToolCall>,
    pending_tools: Vec<BehaviorToolEvidence>,
    skip_replay: bool,
    replay_second: Option<[u8; 19]>,
}

enum CodexOversizedClass {
    Recovered(Value),
    Skipped,
    Oversized,
}

impl CodexLiveParse<'_> {
    fn ingest_oversized(&mut self, prefix: &[u8]) -> Result<JsonlRecordDisposition> {
        match classify_oversized_codex_prefix(prefix) {
            CodexOversizedClass::Recovered(value) => self.ingest(value),
            CodexOversizedClass::Skipped => Ok(JsonlRecordDisposition::Skipped),
            CodexOversizedClass::Oversized => Ok(JsonlRecordDisposition::Ignored),
        }
    }

    fn ingest(&mut self, value: Value) -> Result<JsonlRecordDisposition> {
        if let Some(payload) = value.get("payload").and_then(|value| value.as_object())
            && matches!(
                value.get("type").and_then(Value::as_str),
                Some("turn_context" | "session_meta")
            )
        {
            if let Some(next_model) = payload.get("model").and_then(Value::as_str) {
                self.model = Some(next_model.trim().to_string());
            }
            if matches!(
                value.get("type").and_then(Value::as_str),
                Some("session_meta")
            ) && let Some(session_id) = payload.get("id").and_then(Value::as_str)
            {
                let trimmed = session_id.trim();
                if !trimmed.is_empty() {
                    self.current_session = Some(SessionInfo {
                        session_id: trimmed.to_string(),
                        session_label: self.session_label.clone(),
                        source_path_hash: Some(self.path_hash.to_string()),
                    });
                }
            }
            if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                let trimmed = cwd.trim().to_string();
                if !trimmed.is_empty() && self.current_cwd.as_deref() != Some(trimmed.as_str()) {
                    self.current_project = self.resolver.resolve(Path::new(&trimmed))?;
                    self.current_cwd = Some(trimmed);
                }
            }
            return Ok(JsonlRecordDisposition::Accepted);
        }

        let extracted_tools = extract_codex_tools(&value);
        if !extracted_tools.is_empty() {
            self.pending_tools.extend(extracted_tools);
        }

        let Some((timestamp, info)) = extract_token_count(&value) else {
            return Ok(if self.pending_tools.is_empty() {
                JsonlRecordDisposition::Ignored
            } else {
                JsonlRecordDisposition::Accepted
            });
        };
        if let Some(replay_second) = self.replay_second.as_ref()
            && self.skip_replay
        {
            if timestamp_second(&timestamp).as_ref() == Some(replay_second) {
                if let Some(next_total) = info.get("total_token_usage").and_then(parse_usage_tokens)
                {
                    self.totals = Some(next_total);
                }
                self.pending_tools.clear();
                return Ok(JsonlRecordDisposition::Accepted);
            }
            self.skip_replay = false;
        }
        let Some(hour_start) = bucket_start_from_rfc3339(&timestamp) else {
            return Ok(JsonlRecordDisposition::Malformed);
        };

        let last_usage = info.get("last_token_usage");
        let total_usage = info.get("total_token_usage");
        let delta = pick_delta(last_usage, total_usage, self.totals.as_ref());
        if delta.total_tokens == 0
            && delta.input_tokens == 0
            && delta.cache_read_tokens == 0
            && delta.output_tokens == 0
            && delta.reasoning_output_tokens == 0
        {
            if let Some(next_total) = total_usage.and_then(parse_usage_tokens) {
                self.totals = Some(next_total);
            }
            return Ok(JsonlRecordDisposition::Accepted);
        }

        if let Some(next_total) = total_usage.and_then(parse_usage_tokens) {
            self.totals = Some(next_total);
        }

        let normalized_model = normalize_model(self.model.as_deref());
        let logical_identity = format!(
            "{timestamp}\0{normalized_model}\0{}\0{}\0{}\0{}\0{}",
            delta.input_tokens,
            delta.cache_read_tokens,
            delta.output_tokens,
            delta.reasoning_output_tokens,
            delta.total_tokens,
        );
        let event = UsageEvent {
            event_key: format!("codex:logical:{}", hash_string(&logical_identity)),
            source: SourceKind::Codex,
            provider_label: String::new(),
            model: normalized_model,
            event_at: timestamp,
            hour_start,
            tokens: delta,
            project: self.current_project.clone(),
            session: self.current_session.clone(),
        };
        let tools = std::mem::take(&mut self.pending_tools);
        self.turns.push(turn_from_tools(&event, &tools));
        self.tool_calls
            .extend(tool_calls_from_evidence(&event, tools));
        self.events.push(event);
        Ok(JsonlRecordDisposition::Accepted)
    }
}

/// Codex rollout parser. Owns the per-file scan + per-shard commit pipeline
/// for `~/.codex/sessions`.
pub struct CodexParser;

impl SourceParser for CodexParser {
    fn source(&self) -> SourceKind {
        SourceKind::Codex
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
        Box::pin(sync_codex(
            store,
            writer,
            parallelism,
            recent_cutoff,
            cancel,
            progress,
        ))
    }
}

async fn sync_codex(
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
    recent_cutoff: Option<DateTime<Utc>>,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
) -> Result<SourceSyncStats> {
    /*
     * ========================================================================
     * 步骤1：按日期分片并行解析 Codex rollout 真源
     * ========================================================================
     * 目标：
     * 1) 读取 ~/.codex/sessions 下的 rollout-*.jsonl 文件
     * 2) 只把缺失、追加或重放文件送去解析
     * 3) 返回 event / cursor / reset 指令给单 writer 统一落库
     */
    info!("开始同步 Codex rollout 真源");

    // 1.1 构建按日期目录分片的候选文件计划
    let parse_started = Instant::now();
    let listing = source_files::list_codex_session_files();
    let inventory_paths = listing.file_paths();
    store.source_files().mark_inventory_seen(
        SourceKind::Codex,
        &inventory_paths,
        writer.run_started_at(),
    )?;
    let inventory_error = listing.error_summary();
    let files = listing.paths;
    let total_files = files.len();
    let cursor_map = store.cursors().load_file_cursors(SourceKind::Codex)?;

    let mut shards = std::collections::HashMap::<PathBuf, Vec<CandidateFile>>::new();
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
        .map(|files| CodexShardPlan { files })
        .collect::<Vec<_>>();
    plans.sort_by_key(|plan| plan.files.first().map(|file| file.path.clone()));
    let planned_files = plans.iter().map(|plan| plan.files.len()).sum::<usize>();
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source: SourceKind::Codex,
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
            tasks.push(task::spawn_blocking(move || {
                parse_codex_shard(plan, counter, task_cancel)
            }));
        }

        let batch_outputs = file_progress
            .wait_for_all(tasks, |files_scanned| {
                emit_progress(
                    &mut progress,
                    SyncEvent::Progress {
                        source: SourceKind::Codex,
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
                shard.turns.retain(|turn| {
                    crate::parsers::timestamp_in_recent_window(&turn.started_at, Some(cutoff))
                });
                shard.tool_calls.retain(|call| {
                    crate::parsers::timestamp_in_recent_window(&call.occurred_at, Some(cutoff))
                });
                shard.events_seen = shard.events.len();
                shard.events_replayed = shard.events.len();
                shard.cursors.clear();
                shard.reset_path_hashes.clear();
            }
            events_seen += shard.events_seen;
            events_replayed += shard.events_replayed;
            bytes_scanned += shard.bytes_scanned;
            parse_issues.merge(shard.parse_issues);

            let completed_files = file_progress.boundary_snapshot();
            emit_progress(
                &mut progress,
                SyncEvent::Progress {
                    source: SourceKind::Codex,
                    files_scanned: completed_files,
                    records_imported: inserted as u64,
                    current_file: None,
                },
            );

            // 1.3 把 reset / event / cursor 协议交给单写入端原子提交
            let commit = writer.commit_shard(SyncShard {
                source: SourceKind::Codex,
                reset_path_hashes: shard.reset_path_hashes,
                events: shard.events,
                cursors: shard.cursors,
                seen_file_paths: shard.seen_file_paths,
                raw_records: Vec::new(),
                turns: shard.turns,
                tool_calls: shard.tool_calls,
            })?;
            inserted += commit.events_inserted;
            write_ms += commit.write_ms;
            emit_progress(
                &mut progress,
                SyncEvent::Progress {
                    source: SourceKind::Codex,
                    files_scanned: completed_files,
                    records_imported: inserted as u64,
                    current_file: None,
                },
            );
        }
    }

    let mut stats = SourceSyncStats {
        source: SourceKind::Codex,
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
        "完成 Codex rollout 真源解析"
    );
    Ok(stats)
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
}

fn parse_codex_shard(
    plan: CodexShardPlan,
    progress: FileProgressCounter,
    cancel: CancellationToken,
) -> Result<CodexShardOutput> {
    let mut resolver = ProjectResolver::default();
    let mut output = CodexShardOutput {
        events: Vec::new(),
        turns: Vec::new(),
        tool_calls: Vec::new(),
        cursors: Vec::new(),
        reset_path_hashes: Vec::new(),
        events_seen: 0,
        events_replayed: 0,
        bytes_scanned: 0,
        seen_file_paths: Vec::new(),
        parse_issues: ParseIssues::default(),
    };

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
        let (last_total, last_model) = if decision.replay_mode == FileReplayMode::Append {
            (
                existing
                    .as_ref()
                    .and_then(|cursor| cursor.last_total.clone()),
                existing
                    .as_ref()
                    .and_then(|cursor| cursor.last_model.clone()),
            )
        } else {
            (None, None)
        };

        let parsed = parse_rollout_file(
            &decision.snapshot.path,
            &path_hash,
            decision.start_offset,
            last_total,
            last_model,
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
            parsed.last_total,
            parsed.last_model,
        ));
        progress.advance_file();
    }

    Ok(output)
}

fn parse_rollout_file(
    file_path: &Path,
    path_hash: &str,
    start_offset: u64,
    last_total: Option<UsageTokens>,
    last_model: Option<String>,
    resolver: &mut ProjectResolver,
    cancel: &CancellationToken,
) -> Result<RolloutParseResult> {
    let replay_second = (start_offset == 0 && is_codex_replay_session(file_path))
        .then(|| detect_replay_second(file_path, cancel))
        .flatten();
    let file = File::open(file_path)?;
    let file_len = file.metadata()?.len();
    if start_offset >= file_len {
        return Ok(RolloutParseResult {
            end_offset: file_len,
            last_total,
            last_model,
            events: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
            parse_issues: ParseIssues::default(),
            cancelled: cancel.is_cancelled(),
        });
    }

    let mut reader = BoundedJsonlReader::new(file, start_offset)?;

    let session_label = file_path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::to_string);
    let fallback_session_id = session_label
        .clone()
        .unwrap_or_else(|| path_hash.to_string());
    let live = RefCell::new(CodexLiveParse {
        path_hash,
        resolver,
        model: last_model,
        totals: last_total,
        session_label: session_label.clone(),
        current_session: Some(SessionInfo {
            session_id: fallback_session_id,
            session_label,
            source_path_hash: Some(path_hash.to_string()),
        }),
        current_project: None,
        current_cwd: None,
        events: Vec::new(),
        turns: Vec::new(),
        tool_calls: Vec::new(),
        pending_tools: Vec::new(),
        skip_replay: replay_second.is_some(),
        replay_second,
    });
    let mut parse_issues = ParseIssues::default();
    let status = reader.read_json_records_with_oversized(
        SourceKind::Codex,
        path_hash,
        cancel,
        &mut parse_issues,
        |record| live.borrow_mut().ingest(record.value),
        |prefix, _offset| live.borrow_mut().ingest_oversized(prefix),
    )?;
    let live = live.into_inner();

    Ok(RolloutParseResult {
        end_offset: reader.complete_offset(),
        last_total: live.totals,
        last_model: live.model,
        events: live.events,
        turns: live.turns,
        tool_calls: live.tool_calls,
        parse_issues,
        cancelled: status == JsonlReadStatus::Cancelled,
    })
}

#[cfg(test)]
pub(super) fn bounded_contract_parse(file_path: &Path) -> Result<(ParseIssues, u64, bool)> {
    let result = parse_rollout_file(
        file_path,
        "bounded-contract-path-hash",
        0,
        None,
        None,
        &mut ProjectResolver::default(),
        &CancellationToken::new(),
    )?;
    Ok((result.parse_issues, result.end_offset, result.cancelled))
}

fn is_codex_replay_session(file_path: &Path) -> bool {
    let Ok(mut file) = File::open(file_path) else {
        return false;
    };
    let mut buffer = [0u8; REPLAY_MARKER_SCAN_BYTES];
    let Ok(bytes_read) = file.read(&mut buffer) else {
        return false;
    };
    let head = &buffer[..bytes_read];
    contains_bytes(head, b"thread_spawn") || contains_bytes(head, b"forked_from_id")
}

fn detect_replay_second(file_path: &Path, cancel: &CancellationToken) -> Option<[u8; 19]> {
    let file = File::open(file_path).ok()?;
    let mut reader = BoundedJsonlReader::new(file, 0).ok()?;
    let path_hash = hash_string(&file_path.to_string_lossy());
    let mut issues = ParseIssues::default();
    let mut first_second = None;
    let mut replay_second = None;
    reader
        .read_json_records(
            SourceKind::Codex,
            &path_hash,
            cancel,
            &mut issues,
            |record| {
                let value = record.value;
                let Some((timestamp, info)) = extract_token_count(&value) else {
                    return Ok(JsonlRecordDisposition::Ignored);
                };
                if info.get("last_token_usage").is_none() && info.get("total_token_usage").is_none()
                {
                    return Ok(JsonlRecordDisposition::Ignored);
                }
                let Some(current_second) = timestamp_second(&timestamp) else {
                    return Ok(JsonlRecordDisposition::Malformed);
                };
                match first_second {
                    None => first_second = Some(current_second),
                    Some(first) if first == current_second => {
                        replay_second = Some(current_second);
                        return Ok(JsonlRecordDisposition::Stop);
                    }
                    Some(_) => return Ok(JsonlRecordDisposition::Stop),
                }
                Ok(JsonlRecordDisposition::Accepted)
            },
        )
        .ok()?;
    replay_second
}

fn timestamp_second(timestamp: &str) -> Option<[u8; 19]> {
    timestamp.as_bytes().get(..19)?.try_into().ok()
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn extract_token_count(value: &Value) -> Option<(String, &Value)> {
    let timestamp = value.get("timestamp")?.as_str()?.to_string();
    let payload = value.get("payload")?;

    if payload.get("type").and_then(Value::as_str) == Some("token_count") {
        return Some((timestamp, payload.get("info")?));
    }

    let msg = payload.get("msg")?;
    if msg.get("type").and_then(Value::as_str) == Some("token_count") {
        return Some((timestamp, msg.get("info")?));
    }

    None
}

fn classify_oversized_codex_prefix(prefix: &[u8]) -> CodexOversizedClass {
    if let Ok(value) = serde_json::from_slice::<Value>(prefix) {
        if extract_token_count(&value).is_some() {
            return CodexOversizedClass::Recovered(value);
        }
        return match payload_or_msg_type_from_value(&value) {
            Some("token_count") => CodexOversizedClass::Oversized,
            Some(_) => CodexOversizedClass::Skipped,
            None => CodexOversizedClass::Oversized,
        };
    }

    match peek_codex_payload_or_msg_type(prefix) {
        Some(kind) if kind == "token_count" => CodexOversizedClass::Oversized,
        Some(_) => CodexOversizedClass::Skipped,
        None => CodexOversizedClass::Oversized,
    }
}

fn payload_or_msg_type_from_value(value: &Value) -> Option<&str> {
    let payload = value.get("payload")?;
    payload.get("type").and_then(Value::as_str).or_else(|| {
        payload
            .get("msg")
            .and_then(|msg| msg.get("type"))
            .and_then(Value::as_str)
    })
}

fn peek_codex_payload_or_msg_type(prefix: &[u8]) -> Option<String> {
    let peek = &prefix[..prefix.len().min(OVERSIZED_TYPE_PEEK_BYTES)];
    if let Ok(value) = serde_json::from_slice::<Value>(peek) {
        return payload_or_msg_type_from_value(&value).map(str::to_owned);
    }
    object_after_key(peek, br#""payload""#)
        .and_then(|payload| {
            json_type_at_object_depth1(payload).or_else(|| {
                object_after_key(payload, br#""msg""#).and_then(json_type_at_object_depth1)
            })
        })
        .or_else(|| object_after_key(peek, br#""msg""#).and_then(json_type_at_object_depth1))
}

fn object_after_key<'a>(haystack: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let key_pos = haystack
        .windows(key.len())
        .position(|window| window == key)?;
    let after = skip_ws(&haystack[key_pos + key.len()..]);
    if after.first() != Some(&b':') {
        return None;
    }
    let value = skip_ws(&after[1..]);
    if value.first() != Some(&b'{') {
        return None;
    }
    Some(&value[1..])
}

fn json_type_at_object_depth1(object_body: &[u8]) -> Option<String> {
    let type_key = br#""type""#;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut index = 0usize;
    while index < object_body.len() {
        let byte = object_body[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => {
                if depth == 0
                    && object_body[index..].starts_with(type_key)
                    && let Some(value) =
                        json_string_value_after_key(&object_body[index..], type_key)
                {
                    return Some(value);
                }
                in_string = true;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn json_string_value_after_key(haystack: &[u8], key: &[u8]) -> Option<String> {
    if !haystack.starts_with(key) {
        return None;
    }
    let after = skip_ws(&haystack[key.len()..]);
    if after.first() != Some(&b':') {
        return None;
    }
    let value = skip_ws(&after[1..]);
    parse_json_string(value)
}

fn parse_json_string(bytes: &[u8]) -> Option<String> {
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut escaped = false;
    for (index, byte) in bytes.iter().enumerate().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        if *byte == b'\\' {
            escaped = true;
            continue;
        }
        if *byte == b'"' {
            return serde_json::from_slice::<String>(&bytes[..=index]).ok();
        }
    }
    None
}

fn skip_ws(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    &bytes[start..]
}

fn pick_delta(
    last_usage: Option<&Value>,
    total_usage: Option<&Value>,
    previous_total: Option<&UsageTokens>,
) -> UsageTokens {
    if let Some(parsed) = last_usage.and_then(parse_usage_tokens) {
        return parsed;
    }

    let Some(total) = total_usage.and_then(parse_usage_tokens) else {
        return UsageTokens::default();
    };

    if let Some(previous_total) = previous_total {
        if total.total_tokens < previous_total.total_tokens {
            return total;
        }
        return UsageTokens {
            input_tokens: (total.input_tokens - previous_total.input_tokens).max(0),
            cache_read_tokens: (total.cache_read_tokens - previous_total.cache_read_tokens).max(0),
            cache_creation_tokens: (total.cache_creation_tokens
                - previous_total.cache_creation_tokens)
                .max(0),
            output_tokens: (total.output_tokens - previous_total.output_tokens).max(0),
            reasoning_output_tokens: (total.reasoning_output_tokens
                - previous_total.reasoning_output_tokens)
                .max(0),
            total_tokens: (total.total_tokens - previous_total.total_tokens).max(0),
        };
    }

    total
}

fn parse_usage_tokens(value: &Value) -> Option<UsageTokens> {
    value.as_object()?;
    let raw_input_tokens = read_i64(value, "input_tokens")
        .or_else(|| read_i64(value, "prompt_tokens"))
        .unwrap_or_default()
        .max(0);
    let separate_cache_read_tokens = read_i64(value, "cache_read_tokens")
        .or_else(|| read_i64(value, "cache_read_input_tokens"))
        .map(|tokens| tokens.max(0));
    let inclusive_cache_read_tokens = read_i64(value, "cached_input_tokens")
        .or_else(|| {
            read_nested_i64(value, &["prompt_tokens_details", "cached_tokens"])
                .or_else(|| read_nested_i64(value, &["input_tokens_details", "cached_tokens"]))
                .or_else(|| {
                    read_nested_i64(value, &["usage", "prompt_tokens_details", "cached_tokens"])
                })
                .or_else(|| {
                    read_nested_i64(value, &["usage", "input_tokens_details", "cached_tokens"])
                })
        })
        .map(|tokens| tokens.max(0));
    let cache_read_tokens = separate_cache_read_tokens.unwrap_or_else(|| {
        inclusive_cache_read_tokens
            .unwrap_or_default()
            .clamp(0, raw_input_tokens.max(0))
    });
    let input_tokens = if separate_cache_read_tokens.is_some() {
        raw_input_tokens
    } else if inclusive_cache_read_tokens.is_some() {
        (raw_input_tokens - cache_read_tokens).max(0)
    } else {
        raw_input_tokens
    };
    let cache_creation_tokens = read_i64(value, "cache_creation_tokens")
        .or_else(|| read_i64(value, "cache_creation_input_tokens"))
        .or_else(|| read_nested_i64(value, &["input_tokens_details", "cache_creation_tokens"]))
        .or_else(|| read_nested_i64(value, &["prompt_tokens_details", "cache_creation_tokens"]))
        .or_else(|| {
            read_nested_i64(
                value,
                &["usage", "input_tokens_details", "cache_creation_tokens"],
            )
        })
        .or_else(|| {
            read_nested_i64(
                value,
                &["usage", "prompt_tokens_details", "cache_creation_tokens"],
            )
        })
        .unwrap_or_default()
        .max(0);
    let output_tokens = read_i64(value, "output_tokens")
        .or_else(|| read_i64(value, "completion_tokens"))
        .unwrap_or_default()
        .max(0);
    let reasoning_output_tokens = read_i64(value, "reasoning_output_tokens")
        .or_else(|| read_i64(value, "reasoning_tokens"))
        .or_else(|| read_nested_i64(value, &["completion_tokens_details", "reasoning_tokens"]))
        .or_else(|| read_nested_i64(value, &["output_tokens_details", "reasoning_tokens"]))
        .or_else(|| {
            read_nested_i64(
                value,
                &["usage", "completion_tokens_details", "reasoning_tokens"],
            )
        })
        .or_else(|| {
            read_nested_i64(
                value,
                &["usage", "output_tokens_details", "reasoning_tokens"],
            )
        })
        .unwrap_or_default()
        .max(0);
    let total_tokens = read_i64(value, "total_tokens")
        .filter(|tokens| *tokens >= 0)
        .unwrap_or(input_tokens + cache_creation_tokens + cache_read_tokens + output_tokens);
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

fn read_nested_i64(value: &Value, path: &[&str]) -> Option<i64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_i64()
}

#[cfg(test)]
mod tests {
    use super::{
        CodexOversizedClass, classify_oversized_codex_prefix, is_codex_replay_session,
        parse_rollout_file, parse_usage_tokens, peek_codex_payload_or_msg_type, pick_delta,
    };
    use crate::models::UsageTokens;
    use anyhow::Result;
    use serde_json::json;
    use std::{fs, io::Write};
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn codex_parser_accepts_cached_input_tokens_alias() {
        let usage = json!({
            "input_tokens": 100,
            "cached_input_tokens": 42,
            "output_tokens": 8,
            "reasoning_output_tokens": 3,
            "total_tokens": 153
        });

        let tokens = parse_usage_tokens(&usage).expect("usage tokens");

        assert_eq!(tokens.input_tokens, 58);
        assert_eq!(tokens.cache_read_tokens, 42);
        assert_eq!(tokens.output_tokens, 8);
        assert_eq!(tokens.reasoning_output_tokens, 3);
        assert_eq!(tokens.total_tokens, 153);
    }

    #[test]
    fn codex_parser_keeps_legacy_cache_read_tokens() {
        let usage = json!({
            "input_tokens": 100,
            "cache_read_tokens": 7,
            "cached_input_tokens": 42,
            "output_tokens": 8,
            "total_tokens": 115
        });

        let tokens = parse_usage_tokens(&usage).expect("usage tokens");

        assert_eq!(tokens.cache_read_tokens, 7);
    }

    #[test]
    fn codex_parser_reads_nested_cached_tokens_as_cache_read_and_non_cached_input() {
        let usage = json!({
            "input_tokens": 100,
            "prompt_tokens_details": {
                "cached_tokens": 42
            },
            "output_tokens": 8,
            "total_tokens": 150
        });

        let tokens = parse_usage_tokens(&usage).expect("usage tokens");

        assert_eq!(tokens.input_tokens, 58);
        assert_eq!(tokens.cache_read_tokens, 42);
        assert_eq!(tokens.output_tokens, 8);
    }

    #[test]
    fn codex_parser_reads_cache_read_input_tokens_alias() {
        let usage = json!({
            "input_tokens": 100,
            "cache_read_input_tokens": 24,
            "output_tokens": 8,
        });

        let tokens = parse_usage_tokens(&usage).expect("usage tokens");

        assert_eq!(tokens.input_tokens, 100);
        assert_eq!(tokens.cache_read_tokens, 24);
        assert_eq!(tokens.total_tokens, 132);
    }

    #[test]
    fn codex_parser_reads_nested_usage_cached_tokens() {
        let usage = json!({
            "input_tokens": 100,
            "usage": {
                "input_tokens_details": {
                    "cached_tokens": 24
                }
            },
            "output_tokens": 8,
        });

        let tokens = parse_usage_tokens(&usage).expect("usage tokens");

        assert_eq!(tokens.input_tokens, 76);
        assert_eq!(tokens.cache_read_tokens, 24);
        assert_eq!(tokens.total_tokens, 108);
    }

    #[test]
    fn codex_parser_reads_nested_reasoning_without_adding_to_output() {
        let usage = json!({
            "prompt_tokens": 100,
            "completion_tokens": 30,
            "usage": {
                "completion_tokens_details": {
                    "reasoning_tokens": 9
                }
            }
        });

        let tokens = parse_usage_tokens(&usage).expect("usage tokens");

        assert_eq!(tokens.input_tokens, 100);
        assert_eq!(tokens.output_tokens, 30);
        assert_eq!(tokens.reasoning_output_tokens, 9);
        assert_eq!(tokens.total_tokens, 130);
    }

    #[test]
    fn codex_parser_clamps_cached_tokens_above_input() {
        let usage = json!({
            "prompt_tokens": 30,
            "usage": {
                "prompt_tokens_details": {
                    "cached_tokens": 42
                }
            },
            "completion_tokens": 8,
            "completion_tokens_details": {
                "reasoning_tokens": 2
            }
        });

        let tokens = parse_usage_tokens(&usage).expect("usage tokens");

        assert_eq!(tokens.input_tokens, 0);
        assert_eq!(tokens.cache_read_tokens, 30);
        assert_eq!(tokens.output_tokens, 8);
        assert_eq!(tokens.reasoning_output_tokens, 2);
        assert_eq!(tokens.total_tokens, 38);
    }

    #[test]
    fn codex_usage_preserves_cache_creation_tokens() {
        let usage = json!({
            "input_tokens": 100,
            "cache_creation_input_tokens": 12,
            "cached_input_tokens": 42,
            "output_tokens": 8,
            "reasoning_output_tokens": 2
        });
        let tokens = parse_usage_tokens(&usage).expect("usage tokens");

        assert_eq!(tokens.input_tokens, 58);
        assert_eq!(tokens.cache_creation_tokens, 12);
        assert_eq!(tokens.cache_read_tokens, 42);
        assert_eq!(tokens.output_tokens, 8);
        assert_eq!(tokens.reasoning_output_tokens, 2);
        assert_eq!(tokens.total_tokens, 120);
    }

    #[test]
    fn codex_total_delta_diffs_cached_input_tokens() {
        let total = json!({
            "input_tokens": 150,
            "cached_input_tokens": 75,
            "output_tokens": 25,
            "reasoning_output_tokens": 5,
            "total_tokens": 255
        });
        let previous = UsageTokens {
            input_tokens: 100,
            cache_creation_tokens: 4,
            cache_read_tokens: 50,
            output_tokens: 10,
            reasoning_output_tokens: 2,
            total_tokens: 162,
        };

        let delta = pick_delta(None, Some(&total), Some(&previous));

        assert_eq!(delta.input_tokens, 0);
        assert_eq!(delta.cache_creation_tokens, 0);
        assert_eq!(delta.cache_read_tokens, 25);
        assert_eq!(delta.output_tokens, 15);
        assert_eq!(delta.reasoning_output_tokens, 3);
        assert_eq!(delta.total_tokens, 93);
    }

    #[test]
    fn codex_parser_clamps_negative_channels() {
        let tokens = parse_usage_tokens(&json!({
            "input_tokens": -10,
            "cached_input_tokens": -4,
            "output_tokens": -3,
            "reasoning_output_tokens": -2,
            "total_tokens": -19
        }))
        .expect("usage tokens");

        assert_eq!(tokens, UsageTokens::default());
    }

    #[test]
    fn codex_parser_attaches_pending_tool_calls_to_next_token_event() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("rollout-test.jsonl");
        let mut file = fs::File::create(&path)?;
        writeln!(
            file,
            "{}",
            json!({
                "type": "session_meta",
                "payload": {"id":"session-a","model":"gpt-5"}
            })
        )?;
        writeln!(
            file,
            "{}",
            json!({
                "type": "response_item",
                "payload": {
                    "item": {
                        "type": "function_call",
                        "name": "functions.shell_command",
                        "arguments": {"command":"cargo test behavior"}
                    }
                }
            })
        )?;
        writeln!(
            file,
            "{}",
            json!({
                "timestamp":"2026-05-01T00:00:00Z",
                "payload": {
                    "type":"token_count",
                    "info": {
                        "last_token_usage": {
                            "input_tokens": 10,
                            "output_tokens": 5,
                            "total_tokens": 15
                        }
                    }
                }
            })
        )?;

        let mut resolver = crate::project::ProjectResolver::default();
        let parsed = parse_rollout_file(
            &path,
            "path-hash",
            0,
            None,
            None,
            &mut resolver,
            &CancellationToken::new(),
        )?;

        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.turns.len(), 1);
        assert_eq!(parsed.turns[0].category.as_str(), "testing");
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].tool_name, "functions.shell_command");
        assert_eq!(parsed.tool_calls[0].tool_kind.as_str(), "bash");
        assert!(
            parsed.tool_calls[0]
                .safe_preview
                .as_deref()
                .unwrap()
                .contains("cargo test")
        );
        Ok(())
    }

    #[test]
    fn codex_parser_skips_fork_replay_prefix_and_keeps_cumulative_baseline() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("rollout-replay.jsonl");
        let mut file = fs::File::create(&path)?;
        writeln!(
            file,
            "{}",
            json!({
                "timestamp": "2026-05-12T08:03:00.000Z",
                "type": "session_meta",
                "payload": {
                    "id": "subagent",
                    "model": "gpt-5.2",
                    "forked_from_id": "parent",
                    "source": {"subagent": {"thread_spawn": {"parent_thread_id": "parent"}}}
                }
            })
        )?;
        writeln!(file, r#"{{"payload":{{"type":"token_count""#)?;
        writeln!(
            file,
            "{}",
            json!({
                "type": "response_item",
                "payload": {
                    "item": {
                        "type": "function_call",
                        "name": "functions.shell_command",
                        "arguments": {"command": "replayed command"}
                    }
                }
            })
        )?;
        for (last, total) in [
            (
                json!({
                    "input_tokens": 100,
                    "cached_input_tokens": 40,
                    "output_tokens": 20,
                    "total_tokens": 120
                }),
                json!({
                    "input_tokens": 100,
                    "cached_input_tokens": 40,
                    "output_tokens": 20,
                    "total_tokens": 120
                }),
            ),
            (
                json!({
                    "input_tokens": 50,
                    "cached_input_tokens": 20,
                    "output_tokens": 10,
                    "total_tokens": 60
                }),
                json!({
                    "input_tokens": 150,
                    "cached_input_tokens": 60,
                    "output_tokens": 30,
                    "total_tokens": 180
                }),
            ),
        ] {
            writeln!(
                file,
                "{}",
                json!({
                    "timestamp": "2026-05-12T08:03:00.000Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "last_token_usage": last,
                            "total_token_usage": total
                        }
                    }
                })
            )?;
        }
        writeln!(
            file,
            "{}",
            json!({
                "timestamp": "2026-05-12T08:04:00.000Z",
                "type": "event_msg",
                "payload": {
                    "type": "token_count",
                    "info": {
                        "total_token_usage": {
                            "input_tokens": 170,
                            "cached_input_tokens": 65,
                            "output_tokens": 35,
                            "total_tokens": 205
                        }
                    }
                }
            })
        )?;

        let mut resolver = crate::project::ProjectResolver::default();
        let parsed = parse_rollout_file(
            &path,
            "path-hash",
            0,
            None,
            None,
            &mut resolver,
            &CancellationToken::new(),
        )?;

        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].tokens.input_tokens, 15);
        assert_eq!(parsed.events[0].tokens.cache_read_tokens, 5);
        assert_eq!(parsed.events[0].tokens.output_tokens, 5);
        assert_eq!(parsed.events[0].tokens.total_tokens, 25);
        assert!(parsed.tool_calls.is_empty());
        assert_eq!(
            parsed.last_total,
            Some(UsageTokens {
                input_tokens: 105,
                cache_read_tokens: 65,
                output_tokens: 35,
                total_tokens: 205,
                ..UsageTokens::default()
            })
        );
        Ok(())
    }

    #[test]
    fn codex_parser_keeps_ordinary_same_second_token_events() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("rollout-ordinary.jsonl");
        let mut file = fs::File::create(&path)?;
        writeln!(
            file,
            "{}",
            json!({
                "timestamp": "2026-05-12T08:03:00.000Z",
                "type": "session_meta",
                "payload": {"id": "ordinary", "model": "gpt-5.2"}
            })
        )?;
        for input_tokens in [100, 50] {
            writeln!(
                file,
                "{}",
                json!({
                    "timestamp": "2026-05-12T08:03:00.000Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "last_token_usage": {
                                "input_tokens": input_tokens,
                                "cached_input_tokens": 20,
                                "output_tokens": 10,
                                "total_tokens": input_tokens + 10
                            }
                        }
                    }
                })
            )?;
        }

        let mut resolver = crate::project::ProjectResolver::default();
        let parsed = parse_rollout_file(
            &path,
            "path-hash",
            0,
            None,
            None,
            &mut resolver,
            &CancellationToken::new(),
        )?;

        assert_eq!(parsed.events.len(), 2);
        assert_eq!(
            parsed
                .events
                .iter()
                .map(|event| event.tokens.input_tokens)
                .sum::<i64>(),
            110
        );
        assert_eq!(
            parsed
                .events
                .iter()
                .map(|event| event.tokens.cache_read_tokens)
                .sum::<i64>(),
            40
        );
        Ok(())
    }

    #[test]
    fn codex_replay_session_accepts_each_upstream_marker() -> Result<()> {
        let temp = TempDir::new()?;
        for (name, payload) in [
            (
                "thread-spawn.jsonl",
                json!({"source": {"subagent": {"thread_spawn": {}}}}),
            ),
            ("forked.jsonl", json!({"forked_from_id": "parent-session"})),
        ] {
            let path = temp.path().join(name);
            fs::write(
                &path,
                json!({"type": "session_meta", "payload": payload}).to_string(),
            )?;
            assert!(
                is_codex_replay_session(&path),
                "marker not detected: {name}"
            );
        }
        Ok(())
    }

    /// DATA-001 contract: a partial last line (no trailing '\n') must not
    /// advance the durable cursor.
    #[test]
    fn partial_tail_does_not_advance_cursor() -> Result<()> {
        let dir = TempDir::new()?;
        let path = dir.path().join("session.jsonl");

        let complete_line = r#"{"timestamp":"2026-05-12T08:03:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"output_tokens":50,"total_tokens":150},"total_token_usage":{"input_tokens":100,"output_tokens":50,"total_tokens":150}}}}"#;
        let complete = format!("{complete_line}\n");
        let partial = format!("{complete_line}\n{complete_line}"); // last line missing '\n'
        fs::write(&path, &partial)?;

        let result = parse_rollout_file(
            &path,
            "path-hash",
            0,
            None,
            None,
            &mut Default::default(),
            &CancellationToken::new(),
        )?;
        assert_eq!(result.events.len(), 2, "valid EOF records are parsed");
        assert_eq!(
            result.end_offset,
            complete.len() as u64,
            "cursor must not include the partial tail"
        );
        let partial_event_key = result.events[1].event_key.clone();

        // Simulate flush + re-sync from saved cursor.
        let full = format!("{complete_line}\n{complete_line}\n");
        fs::write(&path, &full)?;
        let incremental = parse_rollout_file(
            &path,
            "path-hash",
            result.end_offset,
            None,
            None,
            &mut Default::default(),
            &CancellationToken::new(),
        )?;
        assert_eq!(
            incremental.events.len(),
            1,
            "incremental sync picks up completed line"
        );
        assert_eq!(incremental.events[0].event_key, partial_event_key);
        Ok(())
    }

    fn token_count_line(input_tokens: i64, output_tokens: i64) -> String {
        json!({
            "timestamp": "2026-05-12T08:03:00.000Z",
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "last_token_usage": {
                        "input_tokens": input_tokens,
                        "output_tokens": output_tokens,
                        "total_tokens": input_tokens + output_tokens
                    },
                    "total_token_usage": {
                        "input_tokens": input_tokens,
                        "output_tokens": output_tokens,
                        "total_tokens": input_tokens + output_tokens
                    }
                }
            }
        })
        .to_string()
    }

    #[test]
    fn peek_none_on_junk_prefix_stays_oversized() {
        assert!(peek_codex_payload_or_msg_type(&[b'x'; 256]).is_none());
        assert!(matches!(
            classify_oversized_codex_prefix(&[b'x'; 256]),
            CodexOversizedClass::Oversized
        ));
    }

    #[test]
    fn oversized_non_token_count_line_is_skipped_and_later_rows_parse() -> Result<()> {
        let dir = TempDir::new()?;
        let path = dir.path().join("session.jsonl");
        let mut content = br#"{"timestamp":"2026-05-12T08:03:00.000Z","type":"event_msg","payload":{"type":"function_call","arguments":""#.to_vec();
        content.extend(std::iter::repeat_n(b'x', 10 * 1024 * 1024));
        content.extend_from_slice(b"\"}}\n");
        let follow = token_count_line(100, 50);
        content.extend_from_slice(follow.as_bytes());
        content.push(b'\n');
        fs::write(&path, &content)?;

        let result = parse_rollout_file(
            &path,
            "path-hash",
            0,
            None,
            None,
            &mut Default::default(),
            &CancellationToken::new(),
        )?;
        assert_eq!(result.parse_issues.skipped_lines, 1);
        assert_eq!(result.parse_issues.oversized_lines, 0);
        assert_eq!(result.parse_issues.malformed_lines, 0);
        assert_eq!(result.parse_issues.accounting_anomaly_lines, 0);
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].tokens.total_tokens, 150);
        assert_eq!(result.end_offset, content.len() as u64);
        Ok(())
    }

    #[test]
    fn complete_token_count_prefix_padded_with_whitespace_is_recovered() -> Result<()> {
        let dir = TempDir::new()?;
        let path = dir.path().join("session.jsonl");
        let token = token_count_line(80, 20);
        let mut content = token.as_bytes().to_vec();
        content.resize(
            crate::parsers::file_state::DEFAULT_MAX_JSONL_RECORD_BYTES + 32,
            b' ',
        );
        content.push(b'\n');
        fs::write(&path, &content)?;

        let result = parse_rollout_file(
            &path,
            "path-hash",
            0,
            None,
            None,
            &mut Default::default(),
            &CancellationToken::new(),
        )?;
        assert_eq!(result.parse_issues.total(), 0);
        assert_eq!(result.parse_issues.informational_total(), 0);
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].tokens.input_tokens, 80);
        assert_eq!(result.events[0].tokens.output_tokens, 20);
        Ok(())
    }

    #[test]
    fn unusable_token_count_prefix_stays_oversized_and_later_rows_parse() -> Result<()> {
        let dir = TempDir::new()?;
        let path = dir.path().join("session.jsonl");
        let mut content = br#"{"timestamp":"2026-05-12T08:03:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":"#.to_vec();
        content.extend(std::iter::repeat_n(
            b'1',
            crate::parsers::file_state::DEFAULT_MAX_JSONL_RECORD_BYTES,
        ));
        content.extend_from_slice(b"\n");
        let follow = token_count_line(12, 8);
        content.extend_from_slice(follow.as_bytes());
        content.push(b'\n');
        fs::write(&path, &content)?;

        let result = parse_rollout_file(
            &path,
            "path-hash",
            0,
            None,
            None,
            &mut Default::default(),
            &CancellationToken::new(),
        )?;
        assert_eq!(result.parse_issues.oversized_lines, 1);
        assert_eq!(result.parse_issues.skipped_lines, 0);
        assert_eq!(result.parse_issues.malformed_lines, 0);
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].tokens.total_tokens, 20);
        Ok(())
    }
}
