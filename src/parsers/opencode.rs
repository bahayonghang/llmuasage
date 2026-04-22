use std::path::PathBuf;

use anyhow::Result;
use rusqlite::{Connection, params};
use serde_json::Value;
use tracing::info;

use crate::{
    models::{SourceKind, UsageEvent, UsageTokens},
    parsers::SourceSyncStats,
    project::ProjectResolver,
    store::Store,
    util::{bucket_start_from_rfc3339, metadata_inode, normalize_model, now_utc, resolve_home_dir},
};

pub fn sync_opencode(store: &Store) -> Result<SourceSyncStats> {
    /*
     * ========================================================================
     * 步骤1：读取 OpenCode SQLite 真源并做增量同步
     * ========================================================================
     * 目标：
     * 1) 直接读取本地 opencode.db，不走任何外部 sqlite3 命令
     * 2) 按 db inode + last_time_created + last_processed_ids 增量续跑
     * 3) 将 assistant usage 事件直接落入 SQLite 聚合真源
     */
    info!("开始同步 OpenCode SQLite 真源");

    // 1.1 定位本地 opencode.db 与现有游标
    let home_dir = resolve_home_dir();
    let data_home = dirs::data_local_dir().unwrap_or_else(|| home_dir.join(".local").join("share"));
    let opencode_home = std::env::var("OPENCODE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| data_home.join("opencode"));
    let db_path = opencode_home.join("opencode.db");
    let mut cursor = store.load_opencode_cursor()?;
    let mut resolver = ProjectResolver::default();
    let mut stats = SourceSyncStats {
        source: SourceKind::Opencode,
        ..SourceSyncStats::default()
    };

    if !db_path.is_file() {
        cursor.sqlite_status = "missing-db".to_string();
        cursor.updated_at = now_utc();
        store.save_opencode_cursor(&cursor)?;
        stats.last_error = Some("OpenCode SQLite DB 缺失".to_string());
        return Ok(stats);
    }

    let metadata = std::fs::metadata(&db_path)?;
    let inode = metadata_inode(&metadata);
    if cursor.inode != 0 && cursor.inode != inode {
        cursor.last_time_created = 0;
        cursor.last_processed_ids.clear();
    }
    cursor.inode = inode;

    // 1.2 查询 message/session/project 三表并按时间增量消费
    let connection = Connection::open(&db_path)?;
    let mut statement = connection.prepare(
        r#"
        SELECT
            m.id,
            m.time_created,
            json_extract(m.data, '$.role') AS role,
            p.worktree,
            m.data
        FROM message m
        LEFT JOIN session s ON s.id = m.session_id
        LEFT JOIN project p ON p.id = s.project_id
        WHERE m.time_created >= ?1
        ORDER BY m.time_created ASC, m.id ASC
        "#,
    )?;
    let rows = statement.query_map(params![cursor.last_time_created], |row| {
        Ok(OpencodeRow {
            id: row.get(0)?,
            time_created: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            role: row.get(2)?,
            project_worktree: row.get(3)?,
            data: row.get(4)?,
        })
    })?;

    let mut latest_time = cursor.last_time_created;
    let mut latest_ids = Vec::new();
    for row in rows {
        let row = row?;
        if row.time_created < cursor.last_time_created {
            continue;
        }
        if row.time_created == cursor.last_time_created
            && cursor.last_processed_ids.contains(&row.id)
        {
            continue;
        }

        if row.time_created > latest_time {
            latest_time = row.time_created;
            latest_ids.clear();
        }
        if row.time_created == latest_time {
            latest_ids.push(row.id.clone());
        }

        let Some(event) = row_to_event(&row, &mut resolver)? else {
            continue;
        };
        stats.events_seen += 1;
        if store.record_usage_event(&event)? {
            stats.events_inserted += 1;
        }
    }

    cursor.last_time_created = latest_time;
    cursor.last_processed_ids = latest_ids;
    cursor.sqlite_status = "ok".to_string();
    cursor.updated_at = now_utc();
    store.save_opencode_cursor(&cursor)?;
    stats.files_processed = 1;

    info!(
        events_seen = stats.events_seen,
        events_inserted = stats.events_inserted,
        "完成 OpenCode SQLite 真源同步"
    );
    Ok(stats)
}

#[derive(Debug)]
struct OpencodeRow {
    id: String,
    time_created: i64,
    role: Option<String>,
    project_worktree: Option<String>,
    data: String,
}

fn row_to_event(row: &OpencodeRow, resolver: &mut ProjectResolver) -> Result<Option<UsageEvent>> {
    let role = row
        .role
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string();
    if !role.is_empty() && role != "assistant" {
        return Ok(None);
    }

    let value: Value = match serde_json::from_str(&row.data) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let tokens = normalize_opencode_tokens(value.get("tokens"));
    if tokens.total_tokens == 0
        && tokens.input_tokens == 0
        && tokens.cached_input_tokens == 0
        && tokens.output_tokens == 0
    {
        return Ok(None);
    }

    let timestamp_ms = value
        .get("time")
        .and_then(|time| time.get("completed"))
        .and_then(Value::as_i64)
        .or_else(|| {
            value
                .get("time")
                .and_then(|time| time.get("created"))
                .and_then(Value::as_i64)
        });
    let Some(timestamp_ms) = timestamp_ms else {
        return Ok(None);
    };
    let Some(timestamp) = chrono::DateTime::from_timestamp_millis(timestamp_ms) else {
        return Ok(None);
    };
    let event_at = timestamp.to_rfc3339();
    let Some(hour_start) = bucket_start_from_rfc3339(&event_at) else {
        return Ok(None);
    };

    let project = row
        .project_worktree
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|path| resolver.resolve(&path))
        .transpose()?
        .flatten();

    Ok(Some(UsageEvent {
        event_key: format!("opencode:{}", row.id),
        source: SourceKind::Opencode,
        model: normalize_model(
            value
                .get("modelID")
                .and_then(Value::as_str)
                .or_else(|| value.get("modelId").and_then(Value::as_str))
                .or_else(|| value.get("model").and_then(Value::as_str)),
        ),
        event_at,
        hour_start,
        tokens,
        project,
    }))
}

fn normalize_opencode_tokens(value: Option<&Value>) -> UsageTokens {
    let Some(value) = value else {
        return UsageTokens::default();
    };

    let input_tokens = value
        .get("input")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        + value
            .get("cache")
            .and_then(|cache| cache.get("write"))
            .and_then(Value::as_i64)
            .unwrap_or_default();
    let cached_input_tokens = value
        .get("cache")
        .and_then(|cache| cache.get("read"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let output_tokens = value
        .get("output")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let reasoning_output_tokens = value
        .get("reasoning")
        .and_then(Value::as_i64)
        .unwrap_or_default();

    UsageTokens {
        input_tokens,
        cached_input_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens: input_tokens + output_tokens + reasoning_output_tokens,
    }
}
