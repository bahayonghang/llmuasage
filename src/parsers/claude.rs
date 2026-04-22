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

pub fn sync_claude(store: &Store) -> Result<SourceSyncStats> {
    /*
     * ========================================================================
     * 步骤1：遍历 Claude 项目 jsonl 并做增量解析
     * ========================================================================
     * 目标：
     * 1) 读取 ~/.claude/projects 下的项目 jsonl
     * 2) 只增量消费 usage 记录
     * 3) 将标准化 token 事件落入 SQLite 真源
     */
    info!("开始同步 Claude 项目真源");

    // 1.1 枚举 Claude 项目日志与既有游标
    let home_dir = resolve_home_dir();
    let projects_dir = home_dir.join(".claude").join("projects");
    let files = list_project_logs(&projects_dir);
    let cursor_map = store.load_file_cursors(SourceKind::Claude)?;
    let mut resolver = ProjectResolver::default();
    let mut stats = SourceSyncStats {
        source: SourceKind::Claude,
        ..SourceSyncStats::default()
    };

    // 1.2 按文件偏移量增量解析 usage 行
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
        let project = resolver.resolve(file_path.parent().unwrap_or(Path::new(".")))?;
        let parsed = parse_project_file(&file_path, inode, start_offset, project)?;

        for event in &parsed.events {
            stats.events_seen += 1;
            if store.record_usage_event(event)? {
                stats.events_inserted += 1;
            }
        }

        store.save_file_cursor(
            SourceKind::Claude,
            &FileCursor {
                cursor_key: file_name.to_string(),
                file_path: file_name.to_string(),
                inode,
                offset: parsed.end_offset,
                last_total: None,
                last_model: None,
                updated_at: now_utc(),
            },
        )?;
        stats.files_processed += 1;
    }

    info!(
        files_processed = stats.files_processed,
        events_seen = stats.events_seen,
        events_inserted = stats.events_inserted,
        "完成 Claude 项目真源同步"
    );
    Ok(stats)
}

#[derive(Debug)]
struct ClaudeParseResult {
    end_offset: u64,
    events: Vec<UsageEvent>,
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

fn parse_project_file(
    file_path: &Path,
    inode: u64,
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

        let model = normalize_model(
            value
                .get("message")
                .and_then(|message| message.get("model"))
                .and_then(Value::as_str)
                .or_else(|| value.get("model").and_then(Value::as_str)),
        );
        let tokens = normalize_claude_usage(usage);
        if tokens.total_tokens == 0
            && tokens.input_tokens == 0
            && tokens.output_tokens == 0
            && tokens.cached_input_tokens == 0
        {
            continue;
        }

        events.push(UsageEvent {
            event_key: format!(
                "claude:{}:{}:{}",
                inode,
                hash_string(&file_path.to_string_lossy()),
                offset
            ),
            source: SourceKind::Claude,
            model,
            event_at: timestamp.to_string(),
            hour_start,
            tokens,
            project: project.clone(),
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
