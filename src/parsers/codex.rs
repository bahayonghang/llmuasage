use std::{
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::Result;
use serde_json::Value;
use tokio::task;
use tracing::info;
use walkdir::WalkDir;

use crate::{
    models::{SourceKind, UsageEvent, UsageTokens},
    parsers::{
        EVENT_WRITE_BATCH_SIZE, SourceSyncStats,
        file_state::{
            CandidateFile, FileReplayMode, decide_file_replay, finalize_cursor, should_rescan_file,
        },
    },
    project::ProjectResolver,
    store::{Store, SyncRunWriter},
    util::{bucket_start_from_rfc3339, hash_string, normalize_model, resolve_home_dir},
};

#[derive(Debug, Clone)]
struct CodexShardPlan {
    files: Vec<CandidateFile>,
}

#[derive(Debug)]
struct CodexShardOutput {
    events: Vec<UsageEvent>,
    cursors: Vec<crate::store::FileCursor>,
    reset_path_hashes: Vec<String>,
    events_seen: usize,
    events_replayed: usize,
    bytes_scanned: u64,
}

#[derive(Debug)]
struct RolloutParseResult {
    end_offset: u64,
    last_total: Option<UsageTokens>,
    last_model: Option<String>,
    events: Vec<UsageEvent>,
}

pub async fn sync_codex(
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
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
    let home_dir = resolve_home_dir();
    let codex_home = std::env::var("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir.join(".codex"));
    let sessions_dir = codex_home.join("sessions");
    let files = list_rollout_files(&sessions_dir);
    let total_files = files.len();
    let cursor_map = store.load_file_cursors(SourceKind::Codex)?;

    let mut shards = std::collections::HashMap::<PathBuf, Vec<CandidateFile>>::new();
    let mut changed_files = 0usize;
    for file_path in files {
        let key = file_path.parent().unwrap_or(&sessions_dir).to_path_buf();
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
    let mut cursors = Vec::new();
    let mut events_seen = 0usize;
    let mut events_replayed = 0usize;
    let mut bytes_scanned = 0u64;
    let mut inserted = 0usize;
    let mut write_ms = 0u64;
    let mut plans = shards
        .into_values()
        .map(|files| CodexShardPlan { files })
        .collect::<Vec<_>>();
    plans.sort_by_key(|plan| plan.files.first().map(|file| file.path.clone()));

    let width = parallelism.max(1);
    for batch in plans.chunks(width) {
        let mut tasks = Vec::new();
        for plan in batch {
            let plan = plan.clone();
            tasks.push(task::spawn_blocking(move || parse_codex_shard(plan)));
        }

        for task in tasks {
            let shard = task.await??;
            events_seen += shard.events_seen;
            events_replayed += shard.events_replayed;
            bytes_scanned += shard.bytes_scanned;
            if !shard.reset_path_hashes.is_empty() {
                let write_started = Instant::now();
                writer.reset_file_events_batch(SourceKind::Codex, &shard.reset_path_hashes)?;
                write_ms += write_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            }
            let write_started = Instant::now();
            for batch in shard.events.chunks(EVENT_WRITE_BATCH_SIZE) {
                inserted += writer.write_event_batch(batch)?;
            }
            write_ms += write_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            cursors.extend(shard.cursors);
        }
    }

    if !cursors.is_empty() {
        let write_started = Instant::now();
        writer.write_cursor_batch(SourceKind::Codex, &cursors)?;
        write_ms += write_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    }

    let mut stats = SourceSyncStats {
        source: SourceKind::Codex,
        files_processed: total_files,
        changed_files,
        bytes_scanned,
        events_seen,
        events_replayed,
        events_inserted: inserted,
        write_ms,
        ..SourceSyncStats::default()
    };
    let total_elapsed = parse_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    stats.parse_ms = total_elapsed.saturating_sub(write_ms);

    info!(
        files_processed = stats.files_processed,
        changed_files = stats.changed_files,
        events_seen = stats.events_seen,
        bytes_scanned = stats.bytes_scanned,
        "完成 Codex rollout 真源解析"
    );
    Ok(stats)
}

fn list_rollout_files(sessions_dir: &Path) -> Vec<PathBuf> {
    let mut files = WalkDir::new(sessions_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("rollout-") && value.ends_with(".jsonl"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn parse_codex_shard(plan: CodexShardPlan) -> Result<CodexShardOutput> {
    let mut resolver = ProjectResolver::default();
    let mut output = CodexShardOutput {
        events: Vec::new(),
        cursors: Vec::new(),
        reset_path_hashes: Vec::new(),
        events_seen: 0,
        events_replayed: 0,
        bytes_scanned: 0,
    };

    for candidate in plan.files {
        let existing = candidate.existing.clone();
        let decision = decide_file_replay(candidate)?;
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
        });
    }

    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(start_offset))?;

    let mut offset = start_offset;
    let mut model = last_model;
    let mut totals = last_total;
    let mut current_project = None;
    let mut current_cwd: Option<String> = None;
    let mut line = String::new();
    let mut events = Vec::new();

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
            if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                let trimmed = cwd.trim().to_string();
                if !trimmed.is_empty() && current_cwd.as_deref() != Some(trimmed.as_str()) {
                    current_project = resolver.resolve(Path::new(&trimmed))?;
                    current_cwd = Some(trimmed);
                }
            }
            continue;
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
            && delta.cached_input_tokens == 0
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

        events.push(UsageEvent {
            event_key: format!("codex:{path_hash}:{file_fingerprint}:{offset}"),
            source: SourceKind::Codex,
            model: normalize_model(model.as_deref()),
            event_at: timestamp,
            hour_start,
            tokens: delta,
            project: current_project.clone(),
        });
    }

    Ok(RolloutParseResult {
        end_offset: offset,
        last_total: totals,
        last_model: model,
        events,
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
            cached_input_tokens: (total.cached_input_tokens - previous_total.cached_input_tokens)
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
    let object = value.as_object()?;
    Some(UsageTokens {
        input_tokens: object
            .get("input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        cached_input_tokens: object
            .get("cached_input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        output_tokens: object
            .get("output_tokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        reasoning_output_tokens: object
            .get("reasoning_output_tokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        total_tokens: object
            .get("total_tokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
    })
}
