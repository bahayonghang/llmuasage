use std::{path::PathBuf, time::Instant};

use anyhow::Result;
use rusqlite::{Connection, params};
use serde_json::Value;
use tracing::info;

use crate::{
    models::{SourceKind, UsageEvent, UsageTokens},
    parsers::{SourceParseOutput, SourceSyncStats},
    project::ProjectResolver,
    store::Store,
    util::{bucket_start_from_rfc3339, normalize_model, now_utc, resolve_home_dir},
};

const OPENCODE_PAGE_SIZE: i64 = 1000;

#[derive(Debug)]
struct OpencodeRow {
    id: String,
    time_created: i64,
    role: Option<String>,
    project_worktree: Option<String>,
    data: String,
}

pub async fn sync_opencode(store: &Store, _parallelism: usize) -> Result<SourceParseOutput> {
    /*
     * ========================================================================
     * 步骤1：按高水位分页读取 OpenCode SQLite 真源
     * ========================================================================
     * 目标：
     * 1) 直接读取本地 opencode.db，不走外部 sqlite3
     * 2) 只依赖 last_time_created + last_processed_ids 续跑
     * 3) 返回 event 和新 cursor 给单 writer 统一落库
     */
    info!("开始同步 OpenCode SQLite 真源");

    // 1.1 定位本地 DB 并读取当前 cursor
    let parse_started = Instant::now();
    let home_dir = resolve_home_dir();
    let data_home = dirs::data_local_dir().unwrap_or_else(|| home_dir.join(".local").join("share"));
    let opencode_home = std::env::var("OPENCODE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| data_home.join("opencode"));
    let db_path = opencode_home.join("opencode.db");
    let mut cursor = store.load_opencode_cursor()?;
    let mut stats = SourceSyncStats {
        source: SourceKind::Opencode,
        ..SourceSyncStats::default()
    };

    if !db_path.is_file() {
        cursor.sqlite_status = "missing-db".to_string();
        cursor.updated_at = now_utc();
        stats.last_error = Some("OpenCode SQLite DB 缺失".to_string());
        return Ok(SourceParseOutput {
            source: SourceKind::Opencode,
            events: Vec::new(),
            cursors: Vec::new(),
            opencode_cursor: Some(cursor),
            reset_path_hashes: Vec::new(),
            stats,
        });
    }

    // 1.2 按时间和主键分页读取 message 表
    let connection = Connection::open(&db_path)?;
    let mut resolver = ProjectResolver::default();
    let mut events = Vec::new();
    let mut latest_time = cursor.last_time_created;
    let mut latest_ids = cursor.last_processed_ids.clone();
    let mut seen_rows = 0usize;
    let mut scanned_bytes = 0u64;
    let mut page_last_time = cursor.last_time_created;
    let mut page_last_id = cursor
        .last_processed_ids
        .iter()
        .max()
        .cloned()
        .unwrap_or_default();

    loop {
        let rows = load_opencode_page(&connection, page_last_time, &page_last_id)?;
        if rows.is_empty() {
            break;
        }

        for row in rows {
            page_last_time = row.time_created;
            page_last_id = row.id.clone();
            scanned_bytes += row.data.len() as u64;
            seen_rows += 1;

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
            events.push(event);
        }
    }

    cursor.last_time_created = latest_time;
    cursor.last_processed_ids = latest_ids;
    cursor.sqlite_status = "ok".to_string();
    cursor.updated_at = now_utc();

    stats.files_processed = 1;
    stats.changed_files = usize::from(!events.is_empty());
    stats.bytes_scanned = scanned_bytes;
    stats.events_seen = events.len();
    stats.parse_ms = parse_started.elapsed().as_millis().min(u64::MAX as u128) as u64;

    info!(
        rows_seen = seen_rows,
        events_seen = stats.events_seen,
        bytes_scanned = stats.bytes_scanned,
        "完成 OpenCode SQLite 真源解析"
    );
    Ok(SourceParseOutput {
        source: SourceKind::Opencode,
        events,
        cursors: Vec::new(),
        opencode_cursor: Some(cursor),
        reset_path_hashes: Vec::new(),
        stats,
    })
}

fn load_opencode_page(
    connection: &Connection,
    last_time_created: i64,
    last_id: &str,
) -> Result<Vec<OpencodeRow>> {
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
        WHERE m.time_created > ?1
           OR (m.time_created = ?1 AND m.id > ?2)
        ORDER BY m.time_created ASC, m.id ASC
        LIMIT ?3
        "#,
    )?;
    let rows = statement.query_map(
        params![last_time_created, last_id, OPENCODE_PAGE_SIZE],
        |row| {
            Ok(OpencodeRow {
                id: row.get(0)?,
                time_created: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                role: row.get(2)?,
                project_worktree: row.get(3)?,
                data: row.get(4)?,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
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
