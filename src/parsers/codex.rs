use std::{
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
        behavior::{extract_codex_tools, tool_calls_from_evidence, turn_from_tools},
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
}

#[derive(Debug)]
struct RolloutParseResult {
    end_offset: u64,
    last_total: Option<UsageTokens>,
    last_model: Option<String>,
    events: Vec<UsageEvent>,
    turns: Vec<UsageTurn>,
    tool_calls: Vec<UsageToolCall>,
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
        cancel: &'a CancellationToken,
        progress: Option<ProgressSink<'a>>,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>> {
        Box::pin(sync_codex(store, writer, parallelism, cancel, progress))
    }
}

async fn sync_codex(
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
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
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source: SourceKind::Codex,
            files_total: total_files as u64,
        },
    );
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
    let mut files_scanned = 0usize;
    let mut plans = shards
        .into_values()
        .map(|files| CodexShardPlan { files })
        .collect::<Vec<_>>();
    plans.sort_by_key(|plan| plan.files.first().map(|file| file.path.clone()));

    let width = parallelism.max(1);
    for batch in plans.chunks(width) {
        if cancel.is_cancelled() {
            break;
        }
        let mut tasks = Vec::new();
        for plan in batch {
            let plan = plan.clone();
            tasks.push(task::spawn_blocking(move || parse_codex_shard(plan)));
        }

        for task in tasks {
            if cancel.is_cancelled() {
                break;
            }
            let shard = task.await??;
            events_seen += shard.events_seen;
            events_replayed += shard.events_replayed;
            bytes_scanned += shard.bytes_scanned;

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
            files_scanned += commit.files_seen;
            inserted += commit.events_inserted;
            write_ms += commit.write_ms;
            emit_progress(
                &mut progress,
                SyncEvent::Progress {
                    source: SourceKind::Codex,
                    files_scanned: files_scanned as u64,
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
        "完成 Codex rollout 真源解析"
    );
    Ok(stats)
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
}

fn parse_codex_shard(plan: CodexShardPlan) -> Result<CodexShardOutput> {
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
    };

    for candidate in plan.files {
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
            &decision.snapshot.file_fingerprint,
            decision.start_offset,
            last_total,
            last_model,
            &mut resolver,
        )?;
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
    }

    Ok(output)
}

fn parse_rollout_file(
    file_path: &Path,
    path_hash: &str,
    file_fingerprint: &str,
    start_offset: u64,
    last_total: Option<UsageTokens>,
    last_model: Option<String>,
    resolver: &mut ProjectResolver,
) -> Result<RolloutParseResult> {
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
        });
    }

    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(start_offset))?;

    let mut offset = start_offset;
    let mut model = last_model;
    let mut totals = last_total;
    let session_label = file_path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::to_string);
    let fallback_session_id = session_label
        .clone()
        .unwrap_or_else(|| path_hash.to_string());
    let mut current_session = Some(SessionInfo {
        session_id: fallback_session_id,
        session_label: session_label.clone(),
        source_path_hash: Some(path_hash.to_string()),
    });
    let mut current_project = None;
    let mut current_cwd: Option<String> = None;
    let mut line = String::new();
    let mut events = Vec::new();
    let mut turns = Vec::new();
    let mut tool_calls = Vec::new();
    let mut pending_tools = Vec::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break;
        }
        offset += bytes_read as u64;

        if !line.contains("token_count")
            && !line.contains("turn_context")
            && !line.contains("session_meta")
            && !line.contains("function_call")
            && !line.contains("tool_call")
            && !line.contains("recipient_name")
        {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if let Some(payload) = value.get("payload").and_then(|value| value.as_object())
            && matches!(
                value.get("type").and_then(Value::as_str),
                Some("turn_context" | "session_meta")
            )
        {
            if let Some(next_model) = payload.get("model").and_then(Value::as_str) {
                model = Some(next_model.trim().to_string());
            }
            if matches!(
                value.get("type").and_then(Value::as_str),
                Some("session_meta")
            ) && let Some(session_id) = payload.get("id").and_then(Value::as_str)
            {
                let trimmed = session_id.trim();
                if !trimmed.is_empty() {
                    current_session = Some(SessionInfo {
                        session_id: trimmed.to_string(),
                        session_label: session_label.clone(),
                        source_path_hash: Some(path_hash.to_string()),
                    });
                }
            }
            if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                let trimmed = cwd.trim().to_string();
                if !trimmed.is_empty() && current_cwd.as_deref() != Some(trimmed.as_str()) {
                    current_project = resolver.resolve(Path::new(&trimmed))?;
                    current_cwd = Some(trimmed);
                }
            }
            continue;
        }

        let extracted_tools = extract_codex_tools(&value);
        if !extracted_tools.is_empty() {
            pending_tools.extend(extracted_tools);
        }

        let Some((timestamp, info)) = extract_token_count(&value) else {
            continue;
        };
        let Some(hour_start) = bucket_start_from_rfc3339(&timestamp) else {
            continue;
        };

        let last_usage = info.get("last_token_usage");
        let total_usage = info.get("total_token_usage");
        let delta = pick_delta(last_usage, total_usage, totals.as_ref());
        if delta.total_tokens == 0
            && delta.input_tokens == 0
            && delta.cache_read_tokens == 0
            && delta.output_tokens == 0
            && delta.reasoning_output_tokens == 0
        {
            if let Some(next_total) = total_usage.and_then(parse_usage_tokens) {
                totals = Some(next_total);
            }
            continue;
        }

        if let Some(next_total) = total_usage.and_then(parse_usage_tokens) {
            totals = Some(next_total);
        }

        let event = UsageEvent {
            event_key: format!("codex:{path_hash}:{file_fingerprint}:{offset}"),
            source: SourceKind::Codex,
            model: normalize_model(model.as_deref()),
            event_at: timestamp,
            hour_start,
            tokens: delta,
            project: current_project.clone(),
            session: current_session.clone(),
        };
        let tools = std::mem::take(&mut pending_tools);
        turns.push(turn_from_tools(&event, &tools));
        tool_calls.extend(tool_calls_from_evidence(&event, tools));
        events.push(event);
    }

    Ok(RolloutParseResult {
        end_offset: offset,
        last_total: totals,
        last_model: model,
        events,
        turns,
        tool_calls,
    })
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
        .unwrap_or_default();
    let explicit_cache_read_tokens = read_i64(value, "cache_read_tokens")
        .or_else(|| read_i64(value, "cached_input_tokens"))
        .or_else(|| read_i64(value, "cache_read_input_tokens"));
    let nested_cache_read_tokens =
        read_nested_i64(value, &["prompt_tokens_details", "cached_tokens"])
            .or_else(|| read_nested_i64(value, &["input_tokens_details", "cached_tokens"]))
            .or_else(|| {
                read_nested_i64(value, &["usage", "prompt_tokens_details", "cached_tokens"])
            })
            .or_else(|| {
                read_nested_i64(value, &["usage", "input_tokens_details", "cached_tokens"])
            });
    let cache_read_tokens = explicit_cache_read_tokens
        .or(nested_cache_read_tokens)
        .unwrap_or_default();
    let input_tokens = if explicit_cache_read_tokens.is_some() {
        raw_input_tokens
    } else if nested_cache_read_tokens.is_some() {
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
        .unwrap_or_default();
    let output_tokens = read_i64(value, "output_tokens")
        .or_else(|| read_i64(value, "completion_tokens"))
        .unwrap_or_default();
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
        .unwrap_or_default();
    let total_tokens = read_i64(value, "total_tokens").unwrap_or(
        input_tokens
            + cache_creation_tokens
            + cache_read_tokens
            + output_tokens
            + reasoning_output_tokens,
    );
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
    use super::{parse_rollout_file, parse_usage_tokens, pick_delta};
    use crate::models::UsageTokens;
    use anyhow::Result;
    use serde_json::json;
    use std::{fs, io::Write};
    use tempfile::TempDir;

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

        assert_eq!(tokens.input_tokens, 100);
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
        assert_eq!(tokens.total_tokens, 139);
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
        assert_eq!(tokens.cache_read_tokens, 42);
        assert_eq!(tokens.output_tokens, 8);
        assert_eq!(tokens.reasoning_output_tokens, 2);
        assert_eq!(tokens.total_tokens, 52);
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

        assert_eq!(tokens.input_tokens, 100);
        assert_eq!(tokens.cache_creation_tokens, 12);
        assert_eq!(tokens.cache_read_tokens, 42);
        assert_eq!(tokens.output_tokens, 8);
        assert_eq!(tokens.reasoning_output_tokens, 2);
        assert_eq!(tokens.total_tokens, 164);
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

        assert_eq!(delta.input_tokens, 50);
        assert_eq!(delta.cache_creation_tokens, 0);
        assert_eq!(delta.cache_read_tokens, 25);
        assert_eq!(delta.output_tokens, 15);
        assert_eq!(delta.reasoning_output_tokens, 3);
        assert_eq!(delta.total_tokens, 93);
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
            "fingerprint",
            0,
            None,
            None,
            &mut resolver,
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
}
