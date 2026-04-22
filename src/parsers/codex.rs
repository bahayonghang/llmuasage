use std::{
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde_json::Value;
use tracing::info;
use walkdir::WalkDir;

use crate::{
    models::{SourceKind, UsageEvent, UsageTokens},
    parsers::SourceSyncStats,
    project::ProjectResolver,
    store::{FileCursor, Store},
    util::{
        bucket_start_from_rfc3339, hash_string, metadata_inode, normalize_model, now_utc,
        resolve_home_dir,
    },
};

pub fn sync_codex(store: &Store) -> Result<SourceSyncStats> {
    /*
     * ========================================================================
     * 步骤1：遍历 Codex rollout 真源并做增量解析
     * ========================================================================
     * 目标：
     * 1) 读取 ~/.codex/sessions 下的 rollout-*.jsonl 文件
     * 2) 按 file_path + inode + offset + last_total + last_model 续跑
     * 3) 将新事件写入 usage_event 与 usage_bucket_30m
     */
    info!("开始同步 Codex rollout 真源");

    // 1.1 解析 Codex 真源目录与既有游标
    let home_dir = resolve_home_dir();
    let codex_home = std::env::var("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir.join(".codex"));
    let sessions_dir = codex_home.join("sessions");
    let files = list_rollout_files(&sessions_dir);
    let cursor_map = store.load_file_cursors(SourceKind::Codex)?;
    let mut resolver = ProjectResolver::default();
    let mut stats = SourceSyncStats {
        source: SourceKind::Codex,
        ..SourceSyncStats::default()
    };

    // 1.2 按文件游标顺序做增量解析
    for file_path in files {
        let Some(file_name) = file_path.to_str() else {
            continue;
        };
        let metadata = std::fs::metadata(&file_path)?;
        let inode = metadata_inode(&metadata);
        let existing = cursor_map.get(file_name).cloned().unwrap_or_default();
        let start_offset = if existing.inode == inode {
            existing.offset
        } else {
            0
        };

        let parsed = parse_rollout_file(
            &file_path,
            inode,
            start_offset,
            existing.last_total,
            existing.last_model,
            &mut resolver,
        )?;

        for event in &parsed.events {
            stats.events_seen += 1;
            if store.record_usage_event(event)? {
                stats.events_inserted += 1;
            }
        }

        store.save_file_cursor(
            SourceKind::Codex,
            &FileCursor {
                cursor_key: file_name.to_string(),
                file_path: file_name.to_string(),
                inode,
                offset: parsed.end_offset,
                last_total: parsed.last_total,
                last_model: parsed.last_model,
                updated_at: now_utc(),
            },
        )?;
        stats.files_processed += 1;
    }

    info!(
        files_processed = stats.files_processed,
        events_seen = stats.events_seen,
        events_inserted = stats.events_inserted,
        "完成 Codex rollout 真源同步"
    );
    Ok(stats)
}

#[derive(Debug)]
struct RolloutParseResult {
    end_offset: u64,
    last_total: Option<UsageTokens>,
    last_model: Option<String>,
    events: Vec<UsageEvent>,
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

fn parse_rollout_file(
    file_path: &Path,
    inode: u64,
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

        let model_name = normalize_model(model.as_deref());
        let project = current_project.clone();
        events.push(UsageEvent {
            event_key: format!(
                "codex:{}:{}:{}",
                inode,
                hash_string(&file_path.to_string_lossy()),
                offset
            ),
            source: SourceKind::Codex,
            model: model_name,
            event_at: timestamp,
            hour_start,
            tokens: delta,
            project,
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
