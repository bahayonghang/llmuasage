use std::{
    collections::{HashMap, HashSet},
    fs::File,
    future::Future,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    pin::Pin,
    time::Instant,
};

use anyhow::Result;
use serde_json::Value;
use tokio::task;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    models::{SessionInfo, SourceKind, UsageEvent, UsageTokens, UsageToolCall, UsageTurn},
    parsers::{
        ProgressSink, SourceParser, SourceSyncStats, SyncEvent,
        behavior::{extract_claude_tools, tool_calls_from_evidence, turn_from_tools},
        file_progress::{FileProgress, FileProgressCounter},
        file_state::{
            CandidateFile, FileReplayMode, decide_file_replay, finalize_cursor, should_rescan_file,
        },
        source_files,
    },
    project::ProjectResolver,
    store::{Store, SyncRunWriter, SyncShard},
    util::{bucket_start_from_rfc3339, hash_string, normalize_model},
};

#[derive(Debug, Clone)]
struct ClaudeShardPlan {
    files: Vec<CandidateFile>,
    reset_path_hashes: Vec<String>,
}

#[derive(Debug)]
struct ClaudeShardOutput {
    events: Vec<UsageEvent>,
    turns: Vec<UsageTurn>,
    tool_calls: Vec<UsageToolCall>,
    cursors: Vec<crate::store::FileCursor>,
    reset_path_hashes: Vec<String>,
    events_seen: usize,
    events_replayed: usize,
    bytes_scanned: u64,
    seen_file_paths: Vec<String>,
}

#[derive(Debug)]
struct ClaudeParseResult {
    end_offset: u64,
    events: Vec<ClaudeEventCandidate>,
    turns: Vec<UsageTurn>,
    tool_calls: Vec<UsageToolCall>,
}

#[derive(Debug)]
struct ClaudeEventCandidate {
    event: UsageEvent,
    message_id: Option<String>,
    request_id: Option<String>,
    is_sidechain: bool,
    message_only_key: bool,
}

/// Claude project log parser. Owns the per-file scan + per-shard commit
/// pipeline for `~/.claude/projects`.
pub struct ClaudeParser;

impl SourceParser for ClaudeParser {
    fn source(&self) -> SourceKind {
        SourceKind::Claude
    }

    fn parse<'a>(
        &'a self,
        store: &'a Store,
        writer: &'a mut SyncRunWriter,
        parallelism: usize,
        cancel: &'a CancellationToken,
        progress: Option<ProgressSink<'a>>,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>> {
        Box::pin(sync_claude(store, writer, parallelism, cancel, progress))
    }
}

async fn sync_claude(
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
) -> Result<SourceSyncStats> {
    /*
     * ========================================================================
     * 步骤1：按项目目录分片并行解析 Claude 真源
     * ========================================================================
     * 目标：
     * 1) 读取 ~/.claude/projects 下的项目 jsonl
     * 2) 只把缺失、追加或重放文件送去解析
     * 3) 返回 event / cursor / reset 指令给单 writer 统一落库
     */
    info!("开始同步 Claude 项目真源");

    // 1.1 构建按项目目录分片的候选文件计划
    let parse_started = Instant::now();
    let listing = source_files::list_claude_project_logs();
    let inventory_paths = listing.file_paths();
    store.source_files().mark_inventory_seen(
        SourceKind::Claude,
        &inventory_paths,
        writer.run_started_at(),
    )?;
    let inventory_error = listing.error_summary();
    let inventory_root = listing.root;
    let files = listing.paths;
    let total_files = files.len();
    let cursor_map = store.cursors().load_file_cursors(SourceKind::Claude)?;

    let mut projects =
        HashMap::<PathBuf, Vec<(PathBuf, Option<crate::store::FileCursor>, bool)>>::new();
    let mut trigger_files = 0usize;
    for file_path in files {
        let existing = file_path
            .to_str()
            .and_then(|raw| cursor_map.get(raw).cloned());
        let changed = should_rescan_file(&file_path, existing.as_ref())?;
        if changed {
            trigger_files += 1;
        }
        projects
            .entry(claude_project_key(&inventory_root, &file_path))
            .or_default()
            .push((file_path, existing, changed));
    }

    // 1.2 控制并发度并行解析分片
    let mut events_seen = 0usize;
    let mut events_replayed = 0usize;
    let mut bytes_scanned = 0u64;
    let mut inserted = 0usize;
    let mut write_ms = 0u64;
    let mut files_scanned = 0usize;
    let mut plans = projects
        .into_values()
        .filter(|files| files.iter().any(|(_, _, changed)| *changed))
        .map(|files| {
            let reset_path_hashes = files
                .iter()
                .filter_map(|(path, existing, _)| {
                    existing
                        .as_ref()
                        .map(|_| hash_string(&path.to_string_lossy()))
                })
                .collect();
            ClaudeShardPlan {
                files: files
                    .into_iter()
                    .map(|(path, _, _)| CandidateFile {
                        path,
                        existing: None,
                    })
                    .collect(),
                reset_path_hashes,
            }
        })
        .collect::<Vec<_>>();
    plans.sort_by_key(|plan| plan.files.first().map(|file| file.path.clone()));
    let planned_files = plans.iter().map(|plan| plan.files.len()).sum::<usize>();
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source: SourceKind::Claude,
            files_total: planned_files as u64,
        },
    );
    let (mut file_progress, file_progress_counter) = FileProgress::new();

    let width = parallelism.max(1);
    for batch in plans.chunks(width) {
        if cancel.is_cancelled() {
            break;
        }
        let mut tasks = Vec::new();
        for plan in batch {
            let plan = plan.clone();
            let counter = file_progress_counter.clone();
            tasks.push(task::spawn_blocking(move || {
                parse_claude_shard(plan, counter)
            }));
        }

        let mut combined = SyncShard::new(SourceKind::Claude);
        let mut batch_cancelled = false;
        for task in tasks {
            if cancel.is_cancelled() {
                batch_cancelled = true;
                break;
            }
            let shard = file_progress
                .wait_for(task, |files_scanned| {
                    emit_progress(
                        &mut progress,
                        SyncEvent::Progress {
                            source: SourceKind::Claude,
                            files_scanned,
                            records_imported: inserted as u64,
                            current_file: None,
                        },
                    );
                })
                .await??;
            events_seen += shard.events_seen;
            events_replayed += shard.events_replayed;
            bytes_scanned += shard.bytes_scanned;
            combined.reset_path_hashes.extend(shard.reset_path_hashes);
            combined.events.extend(shard.events);
            combined.cursors.extend(shard.cursors);
            combined.seen_file_paths.extend(shard.seen_file_paths);
            combined.turns.extend(shard.turns);
            combined.tool_calls.extend(shard.tool_calls);
        }
        if batch_cancelled {
            break;
        }

        let completed_files = file_progress.boundary_snapshot();
        emit_progress(
            &mut progress,
            SyncEvent::Progress {
                source: SourceKind::Claude,
                files_scanned: completed_files,
                records_imported: inserted as u64,
                current_file: None,
            },
        );

        // 1.3 同一有界解析批次合并为一个事务，避免每项目重复 fsync/checkpoint。
        // 项目级 logical dedupe 已在 parse_claude_shard 内完成；writer 继续负责
        // 跨项目最终幂等与 reset → event → cursor 的原子提交顺序。
        let commit = writer.commit_shard(combined)?;
        files_scanned += commit.files_seen;
        inserted += commit.events_inserted;
        write_ms += commit.write_ms;
        emit_progress(
            &mut progress,
            SyncEvent::Progress {
                source: SourceKind::Claude,
                files_scanned: completed_files,
                records_imported: inserted as u64,
                current_file: None,
            },
        );
    }

    let mut stats = SourceSyncStats {
        source: SourceKind::Claude,
        files_processed: total_files,
        changed_files: files_scanned,
        skipped_files: total_files.saturating_sub(files_scanned),
        bytes_scanned,
        events_seen,
        events_replayed,
        events_inserted: inserted,
        write_ms,
        last_error: inventory_error,
        ..SourceSyncStats::default()
    };
    let total_elapsed = parse_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    stats.parse_ms = total_elapsed.saturating_sub(write_ms);

    info!(
        files_processed = stats.files_processed,
        trigger_files,
        changed_files = stats.changed_files,
        skipped_files = stats.skipped_files,
        events_seen = stats.events_seen,
        bytes_scanned = stats.bytes_scanned,
        "完成 Claude 项目真源解析"
    );
    Ok(stats)
}

fn claude_project_key(root: &Path, file_path: &Path) -> PathBuf {
    file_path
        .strip_prefix(root)
        .ok()
        .and_then(|relative| relative.components().next())
        .map(|component| root.join(component.as_os_str()))
        .or_else(|| file_path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
}

fn parse_claude_shard(
    plan: ClaudeShardPlan,
    progress: FileProgressCounter,
) -> Result<ClaudeShardOutput> {
    let mut resolver = ProjectResolver::default();
    let replay_path_hashes = plan
        .reset_path_hashes
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let mut output = ClaudeShardOutput {
        events: Vec::new(),
        turns: Vec::new(),
        tool_calls: Vec::new(),
        cursors: Vec::new(),
        reset_path_hashes: plan.reset_path_hashes,
        events_seen: 0,
        events_replayed: 0,
        bytes_scanned: 0,
        seen_file_paths: Vec::new(),
    };

    let mut candidates = Vec::new();
    for candidate in plan.files {
        let existing = candidate.existing.clone();
        let decision = decide_file_replay(candidate)?;
        output
            .seen_file_paths
            .push(decision.snapshot.path.to_string_lossy().to_string());
        let project =
            resolver.resolve(decision.snapshot.path.parent().unwrap_or(Path::new(".")))?;
        let path_hash = hash_string(&decision.snapshot.path.to_string_lossy());
        let parsed = parse_project_file(
            &decision.snapshot.path,
            &path_hash,
            &decision.snapshot.file_fingerprint,
            decision.start_offset,
            project,
        )?;

        output.bytes_scanned += decision
            .snapshot
            .file_size
            .saturating_sub(decision.start_offset);
        output.events_seen += parsed.events.len();
        if replay_path_hashes.contains(&path_hash)
            || (decision.replay_mode == FileReplayMode::Reparse && existing.is_some())
        {
            output.events_replayed += parsed.events.len();
        }
        candidates.extend(parsed.events);
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

    output.events = dedupe_claude_events(candidates);

    Ok(output)
}

fn parse_project_file(
    file_path: &Path,
    path_hash: &str,
    file_fingerprint: &str,
    start_offset: u64,
    project: Option<crate::models::ProjectInfo>,
) -> Result<ClaudeParseResult> {
    let file = File::open(file_path)?;
    let file_len = file.metadata()?.len();
    if start_offset >= file_len {
        return Ok(ClaudeParseResult {
            end_offset: file_len,
            events: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
        });
    }

    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(start_offset))?;

    let mut offset = start_offset;
    let fallback_session_label = file_path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::to_string);
    let fallback_session_id = fallback_session_label
        .clone()
        .unwrap_or_else(|| path_hash.to_string());
    let mut line = String::new();
    let mut events = Vec::new();
    let mut turns = Vec::new();
    let mut tool_calls = Vec::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break;
        }
        offset += bytes_read as u64;
        if !line.contains("\"usage\"") {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        let usage = value
            .get("message")
            .and_then(|message| message.get("usage"))
            .or_else(|| value.get("usage"));
        let Some(usage) = usage else {
            continue;
        };
        let Some(timestamp) = value.get("timestamp").and_then(Value::as_str) else {
            continue;
        };
        let Some(hour_start) = bucket_start_from_rfc3339(timestamp) else {
            continue;
        };

        let tokens = normalize_claude_usage(usage);
        if tokens.total_tokens == 0
            && tokens.input_tokens == 0
            && tokens.output_tokens == 0
            && tokens.cache_read_tokens == 0
            && tokens.cache_creation_tokens == 0
        {
            continue;
        }

        let session_id = value
            .get("sessionId")
            .and_then(Value::as_str)
            .or_else(|| value.get("session_id").and_then(Value::as_str))
            .or_else(|| {
                value
                    .get("message")
                    .and_then(|message| message.get("sessionId"))
                    .and_then(Value::as_str)
            })
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_session_id.as_str())
            .to_string();

        let message_id = value
            .get("message")
            .and_then(|message| message.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let request_id = value
            .get("requestId")
            .or_else(|| value.get("request_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let is_sidechain = value
            .get("isSidechain")
            .or_else(|| value.get("is_sidechain"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let event_key = claude_event_key(
            message_id.as_deref(),
            request_id.as_deref(),
            path_hash,
            file_fingerprint,
            offset,
            false,
        );
        let event = UsageEvent {
            event_key,
            source: SourceKind::Claude,
            provider_label: String::new(),
            model: normalize_model(
                value
                    .get("message")
                    .and_then(|message| message.get("model"))
                    .and_then(Value::as_str)
                    .or_else(|| value.get("model").and_then(Value::as_str)),
            ),
            event_at: timestamp.to_string(),
            hour_start,
            tokens,
            project: project.clone(),
            session: Some(SessionInfo {
                session_id,
                session_label: fallback_session_label.clone(),
                source_path_hash: Some(path_hash.to_string()),
            }),
        };
        let tools = extract_claude_tools(&value);
        turns.push(turn_from_tools(&event, &tools));
        tool_calls.extend(tool_calls_from_evidence(&event, tools));
        events.push(ClaudeEventCandidate {
            event,
            message_id,
            request_id,
            is_sidechain,
            message_only_key: false,
        });
    }

    Ok(ClaudeParseResult {
        end_offset: offset,
        events,
        turns,
        tool_calls,
    })
}

fn dedupe_claude_events(candidates: Vec<ClaudeEventCandidate>) -> Vec<UsageEvent> {
    let mut deduped: Vec<ClaudeEventCandidate> = Vec::new();
    let mut exact_indexes = HashMap::<(String, Option<String>), usize>::new();
    let mut message_indexes = HashMap::<String, Vec<usize>>::new();

    for candidate in candidates {
        let Some(message_id) = candidate.message_id.as_ref() else {
            deduped.push(candidate);
            continue;
        };
        let exact_key = (message_id.clone(), candidate.request_id.clone());
        let existing_index = exact_indexes.get(&exact_key).copied().or_else(|| {
            message_indexes.get(message_id).and_then(|indexes| {
                indexes
                    .iter()
                    .copied()
                    .find(|index| candidate.is_sidechain || deduped[*index].is_sidechain)
            })
        });

        if let Some(index) = existing_index {
            let cross_request = deduped[index].request_id != candidate.request_id;
            merge_claude_candidate(&mut deduped[index], candidate);
            deduped[index].message_only_key |= cross_request;
            exact_indexes.insert(exact_key, index);
            continue;
        }

        let index = deduped.len();
        exact_indexes.insert(exact_key, index);
        message_indexes
            .entry(message_id.clone())
            .or_default()
            .push(index);
        deduped.push(candidate);
    }

    deduped
        .into_iter()
        .map(|mut candidate| {
            if candidate.message_id.is_some() {
                candidate.event.event_key = claude_event_key(
                    candidate.message_id.as_deref(),
                    candidate.request_id.as_deref(),
                    "",
                    "",
                    0,
                    candidate.message_only_key,
                );
            }
            candidate.event
        })
        .collect()
}

fn merge_claude_candidate(existing: &mut ClaudeEventCandidate, candidate: ClaudeEventCandidate) {
    let prefer_candidate = (existing.is_sidechain && !candidate.is_sidechain)
        || (existing.is_sidechain == candidate.is_sidechain
            && candidate.event.tokens.total_tokens > existing.event.tokens.total_tokens);
    let merged = UsageTokens {
        input_tokens: existing
            .event
            .tokens
            .input_tokens
            .max(candidate.event.tokens.input_tokens),
        cache_read_tokens: existing
            .event
            .tokens
            .cache_read_tokens
            .max(candidate.event.tokens.cache_read_tokens),
        cache_creation_tokens: existing
            .event
            .tokens
            .cache_creation_tokens
            .max(candidate.event.tokens.cache_creation_tokens),
        output_tokens: existing
            .event
            .tokens
            .output_tokens
            .max(candidate.event.tokens.output_tokens),
        reasoning_output_tokens: existing
            .event
            .tokens
            .reasoning_output_tokens
            .max(candidate.event.tokens.reasoning_output_tokens),
        total_tokens: existing
            .event
            .tokens
            .total_tokens
            .max(candidate.event.tokens.total_tokens),
    };
    if prefer_candidate {
        let message_only_key = existing.message_only_key;
        *existing = candidate;
        existing.message_only_key = message_only_key;
    }
    existing.event.tokens = UsageTokens {
        total_tokens: merged.total_tokens.max(
            merged.input_tokens
                + merged.cache_creation_tokens
                + merged.cache_read_tokens
                + merged.output_tokens,
        ),
        ..merged
    };
}

fn claude_event_key(
    message_id: Option<&str>,
    request_id: Option<&str>,
    path_hash: &str,
    file_fingerprint: &str,
    offset: u64,
    message_only: bool,
) -> String {
    let Some(message_id) = message_id else {
        return format!("claude:{path_hash}:{file_fingerprint}:{offset}");
    };
    let identity = if message_only {
        message_id.to_string()
    } else {
        format!("{message_id}\0{}", request_id.unwrap_or_default())
    };
    format!("claude:logical:{}", hash_string(&identity))
}

fn normalize_claude_usage(value: &Value) -> UsageTokens {
    let input_tokens = value
        .get("input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0);
    let cache_creation_tokens = read_i64(value, "cache_creation_input_tokens").max(0)
        + read_i64(value, "cache_creation_input_tokens_5m").max(0)
        + read_i64(value, "cache_creation_input_tokens_1h").max(0);
    let cache_read_tokens = value
        .get("cache_read_input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0);
    let output_tokens = value
        .get("output_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0);
    let reasoning_output_tokens = value
        .get("reasoning_output_tokens")
        .or_else(|| value.get("thinking_output_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0);
    let total_tokens = value
        .get("total_tokens")
        .and_then(Value::as_i64)
        .filter(|tokens| *tokens >= 0)
        .unwrap_or(input_tokens + cache_creation_tokens + cache_read_tokens + output_tokens);

    UsageTokens {
        input_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens,
    }
}

fn read_i64(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{normalize_claude_usage, parse_project_file};
    use anyhow::Result;
    use serde_json::json;
    use std::{fs, io::Write};
    use tempfile::TempDir;

    /// Validates D8: Claude's `cache_creation_input_tokens` populates the
    /// dedicated `cache_creation_tokens` column instead of being merged back
    /// into `input_tokens`, and that `cache_read_input_tokens` flows into
    /// `cache_read_tokens` 1:1.
    #[test]
    fn claude_parser_separates_cache_read_and_creation() {
        let usage = json!({
            "input_tokens": 100,
            "cache_creation_input_tokens": 30,
            "cache_read_input_tokens": 50,
            "output_tokens": 200,
        });
        let tokens = normalize_claude_usage(&usage);
        assert_eq!(tokens.input_tokens, 100);
        assert_eq!(tokens.cache_creation_tokens, 30);
        assert_eq!(tokens.cache_read_tokens, 50);
        assert_eq!(tokens.output_tokens, 200);
        assert_eq!(tokens.total_tokens, 100 + 30 + 50 + 200);
        assert_eq!(tokens.reasoning_output_tokens, 0);
    }

    /// Validates the fallback total formula picks up every component when the
    /// upstream payload omits `total_tokens`.
    #[test]
    fn claude_parser_total_falls_back_to_component_sum_when_missing() {
        let usage = json!({
            "input_tokens": 10,
            "cache_creation_input_tokens": 4,
            "cache_read_input_tokens": 2,
            "output_tokens": 5,
        });
        let tokens = normalize_claude_usage(&usage);
        assert_eq!(tokens.total_tokens, 21);
    }

    #[test]
    fn claude_parser_reads_official_reasoning_field_when_present() {
        let usage = json!({
            "input_tokens": 10,
            "cache_creation_input_tokens": 4,
            "cache_read_input_tokens": 2,
            "output_tokens": 5,
            "reasoning_output_tokens": 7,
        });

        let tokens = normalize_claude_usage(&usage);

        assert_eq!(tokens.reasoning_output_tokens, 7);
        assert_eq!(tokens.total_tokens, 21);
    }

    #[test]
    fn claude_parser_accepts_thinking_output_tokens_alias() {
        let usage = json!({
            "input_tokens": 10,
            "output_tokens": 5,
            "thinking_output_tokens": 3,
        });

        let tokens = normalize_claude_usage(&usage);

        assert_eq!(tokens.reasoning_output_tokens, 3);
        assert_eq!(tokens.total_tokens, 15);
    }

    #[test]
    fn claude_parser_sums_cache_creation_ttl_subfields() {
        let usage = json!({
            "input_tokens": 10,
            "cache_creation_input_tokens_5m": 4,
            "cache_creation_input_tokens_1h": 6,
            "cache_read_input_tokens": 2,
            "output_tokens": 5,
        });

        let tokens = normalize_claude_usage(&usage);

        assert_eq!(tokens.cache_creation_tokens, 10);
        assert_eq!(tokens.cache_read_tokens, 2);
        assert_eq!(tokens.total_tokens, 27);
    }

    #[test]
    fn claude_parser_clamps_negative_channels() {
        let tokens = normalize_claude_usage(&json!({
            "input_tokens": -10,
            "cache_creation_input_tokens": -4,
            "cache_read_input_tokens": -2,
            "output_tokens": -3,
            "reasoning_output_tokens": -1,
            "total_tokens": -20
        }));

        assert_eq!(tokens, crate::models::UsageTokens::default());
    }

    #[test]
    fn claude_parser_emits_tool_facts_and_coding_turns() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("session.jsonl");
        let mut file = fs::File::create(&path)?;
        writeln!(
            file,
            "{}",
            json!({
                "type": "assistant",
                "sessionId": "session-a",
                "timestamp": "2026-05-01T00:00:00Z",
                "message": {
                    "model": "claude-sonnet-4-5",
                    "content": [
                        {"type":"tool_use","name":"Edit","input":{"file_path":"src/lib.rs","old_string":"private text","new_string":"new private text"}},
                        {"type":"tool_use","name":"Bash","input":{"command":"cargo test behavior"}}
                    ],
                    "usage": {"input_tokens": 10, "output_tokens": 5}
                }
            })
        )?;

        let parsed = parse_project_file(&path, "path-hash", "fingerprint", 0, None)?;

        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.turns.len(), 1);
        assert_eq!(parsed.turns[0].category.as_str(), "coding");
        assert!(parsed.turns[0].has_edits);
        assert_eq!(parsed.tool_calls.len(), 2);
        assert_eq!(parsed.tool_calls[0].tool_name, "Edit");
        assert_eq!(parsed.tool_calls[0].tool_kind.as_str(), "edit");
        assert!(
            parsed.tool_calls[0]
                .safe_preview
                .as_deref()
                .unwrap()
                .contains("src/lib.rs")
        );
        assert!(
            !parsed.tool_calls[0]
                .safe_preview
                .as_deref()
                .unwrap()
                .contains("private text")
        );
        Ok(())
    }
}
