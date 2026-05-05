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
    models::{SessionInfo, SourceKind, UsageEvent, UsageTokens},
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
struct ClaudeShardPlan {
    files: Vec<CandidateFile>,
}

#[derive(Debug)]
struct ClaudeShardOutput {
    events: Vec<UsageEvent>,
    cursors: Vec<crate::store::FileCursor>,
    reset_path_hashes: Vec<String>,
    events_seen: usize,
    events_replayed: usize,
    bytes_scanned: u64,
}

#[derive(Debug)]
struct ClaudeParseResult {
    end_offset: u64,
    events: Vec<UsageEvent>,
}

pub async fn sync_claude(
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
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
    let home_dir = resolve_home_dir();
    let projects_dir = home_dir.join(".claude").join("projects");
    let files = list_project_logs(&projects_dir);
    let total_files = files.len();
    let cursor_map = store.load_file_cursors(SourceKind::Claude)?;

    let mut shards = std::collections::HashMap::<PathBuf, Vec<CandidateFile>>::new();
    let mut changed_files = 0usize;
    for file_path in files {
        let key = file_path.parent().unwrap_or(&projects_dir).to_path_buf();
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
        .map(|files| ClaudeShardPlan { files })
        .collect::<Vec<_>>();
    plans.sort_by_key(|plan| plan.files.first().map(|file| file.path.clone()));

    let width = parallelism.max(1);
    for batch in plans.chunks(width) {
        let mut tasks = Vec::new();
        for plan in batch {
            let plan = plan.clone();
            tasks.push(task::spawn_blocking(move || parse_claude_shard(plan)));
        }

        for task in tasks {
            let shard = task.await??;
            events_seen += shard.events_seen;
            events_replayed += shard.events_replayed;
            bytes_scanned += shard.bytes_scanned;
            if !shard.reset_path_hashes.is_empty() {
                let write_started = Instant::now();
                writer.reset_file_events_batch(SourceKind::Claude, &shard.reset_path_hashes)?;
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
        writer.write_cursor_batch(SourceKind::Claude, &cursors)?;
        write_ms += write_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    }

    let mut stats = SourceSyncStats {
        source: SourceKind::Claude,
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
        "完成 Claude 项目真源解析"
    );
    Ok(stats)
}

fn list_project_logs(projects_dir: &Path) -> Vec<PathBuf> {
    let mut files = WalkDir::new(projects_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.ends_with(".jsonl"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn parse_claude_shard(plan: ClaudeShardPlan) -> Result<ClaudeShardOutput> {
    let mut resolver = ProjectResolver::default();
    let mut output = ClaudeShardOutput {
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
        if decision.replay_mode == FileReplayMode::Reparse && existing.is_some() {
            output.events_replayed += parsed.events.len();
            output.reset_path_hashes.push(path_hash);
        }
        output.events.extend(parsed.events);
        output.cursors.push(finalize_cursor(
            &decision.snapshot.path,
            &decision.snapshot,
            parsed.end_offset,
            None,
            None,
        ));
    }

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
            && tokens.cached_input_tokens == 0
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

        events.push(UsageEvent {
            event_key: format!("claude:{path_hash}:{file_fingerprint}:{offset}"),
            source: SourceKind::Claude,
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
        });
    }

    Ok(ClaudeParseResult {
        end_offset: offset,
        events,
    })
}

fn normalize_claude_usage(value: &Value) -> UsageTokens {
    let input_tokens = value
        .get("input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        + value
            .get("cache_creation_input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or_default();

    let output_tokens = value
        .get("output_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let cached_input_tokens = value
        .get("cache_read_input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let total_tokens = value
        .get("total_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(input_tokens + output_tokens);

    UsageTokens {
        input_tokens,
        cached_input_tokens,
        output_tokens,
        reasoning_output_tokens: 0,
        total_tokens,
    }
}
