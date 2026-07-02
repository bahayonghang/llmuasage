use std::{future::Future, path::PathBuf, pin::Pin, time::Instant};

use anyhow::Result;
use rusqlite::{Connection, params};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    integrations,
    models::{
        ActivityCategory, SessionInfo, SourceKind, UsageEvent, UsageTokens, UsageToolCall,
        UsageTurn,
    },
    parsers::{
        ProgressSink, SourceParser, SourceSyncStats, SyncEvent, behavior::opencode_tool_evidence,
    },
    project::ProjectResolver,
    store::{Store, SyncRunWriter, SyncShard},
    util::{bucket_start_from_rfc3339, file_identity, normalize_model, now_utc},
};

const OPENCODE_PAGE_SIZE: i64 = 1000;

#[derive(Debug)]
struct OpencodeRow {
    id: String,
    session_id: Option<String>,
    time_created: i64,
    role: Option<String>,
    project_worktree: Option<String>,
    data: String,
}

/// OpenCode SQLite parser. Owns the page-streamed scan + per-page commit
/// pipeline for the local `opencode.db` message table.
pub struct OpencodeParser;

impl SourceParser for OpencodeParser {
    fn source(&self) -> SourceKind {
        SourceKind::Opencode
    }

    fn parse<'a>(
        &'a self,
        store: &'a Store,
        writer: &'a mut SyncRunWriter,
        parallelism: usize,
        cancel: &'a CancellationToken,
        progress: Option<ProgressSink<'a>>,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>> {
        Box::pin(sync_opencode(store, writer, parallelism, cancel, progress))
    }
}

async fn sync_opencode(
    store: &Store,
    writer: &mut SyncRunWriter,
    _parallelism: usize,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
) -> Result<SourceSyncStats> {
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
    let db_path = integrations::opencode::resolve_db_path();
    let mut cursor = store.cursors().load_opencode_cursor()?;
    let mut stats = SourceSyncStats {
        source: SourceKind::Opencode,
        ..SourceSyncStats::default()
    };
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source: SourceKind::Opencode,
            files_total: 1,
        },
    );

    if !db_path.is_file() {
        cursor.sqlite_status = "missing-db".to_string();
        cursor.updated_at = now_utc();
        stats.absent = true;
        stats.last_error = Some("OpenCode SQLite DB 缺失".to_string());
        store.cursors().save_opencode_cursor(&cursor)?;
        return Ok(stats);
    }

    // 1.2 按时间和主键分页读取 message 表
    let db_identity = file_identity(&db_path)?;
    if cursor.inode != 0 && cursor.inode != db_identity {
        info!(
            previous_inode = cursor.inode,
            current_inode = db_identity,
            "检测到 OpenCode DB 身份变化，重置高水位"
        );
        cursor.last_time_created = 0;
        cursor.last_processed_ids.clear();
    }
    cursor.inode = db_identity;

    let connection = Connection::open(&db_path)?;
    let mut resolver = ProjectResolver::default();
    let raw_archive_enabled = store.raw_archive_enabled()?;
    let mut latest_time = cursor.last_time_created;
    let mut latest_ids = cursor.last_processed_ids.clone();
    let mut seen_rows = 0usize;
    let mut normalized_events_seen = 0usize;
    let mut scanned_bytes = 0u64;
    let mut inserted = 0usize;
    let mut write_ms = 0u64;
    let mut page_last_time = cursor.last_time_created;
    let mut page_last_id = cursor
        .last_processed_ids
        .iter()
        .max()
        .cloned()
        .unwrap_or_default();

    loop {
        if cancel.is_cancelled() {
            break;
        }
        let rows = load_opencode_page(&connection, page_last_time, &page_last_id)?;
        if rows.is_empty() {
            break;
        }

        let mut page_events = Vec::new();
        let mut page_turns = Vec::new();
        let mut page_raw = Vec::new();
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

            // 序列化 OpenCode SQLite row 为 JSON（D11 / F1.5）。仅在 raw archive
            // 开关打开时持有；否则丢弃，避免 commit_shard 同事务多写。
            let raw_payload = if raw_archive_enabled {
                Some(serialize_opencode_row(&row))
            } else {
                None
            };

            let Some(event) = row_to_event(&row, &mut resolver)? else {
                continue;
            };
            if let Some(raw_json) = raw_payload {
                page_raw.push(crate::store::RawRecord {
                    event_key: event.event_key.clone(),
                    raw_json,
                });
            }
            page_turns.push(UsageTurn::from_event(&event, ActivityCategory::General));
            page_events.push(event);
        }

        normalized_events_seen += page_events.len();
        if !page_events.is_empty() {
            // OpenCode 是流式分页，shard 仅承载本页 event；
            // OpencodeCursor 仍由 store.save_opencode_cursor 自行收尾，故 cursors/resets 均为空。
            let commit = writer.commit_shard(SyncShard {
                source: SourceKind::Opencode,
                reset_path_hashes: Vec::new(),
                events: page_events,
                cursors: Vec::new(),
                seen_file_paths: Vec::new(),
                raw_records: page_raw,
                turns: page_turns,
                tool_calls: Vec::new(),
            })?;
            inserted += commit.events_inserted;
            write_ms += commit.write_ms;
        }
        emit_progress(
            &mut progress,
            SyncEvent::Progress {
                source: SourceKind::Opencode,
                files_scanned: seen_rows as u64,
                records_imported: inserted as u64,
                current_file: Some(db_path.display().to_string()),
            },
        );
    }

    // 扫描 part 表的工具调用：独立于 message 分页（part 自带 time_created，
    // 通过 data 内的 messageID/sessionID 关联）；part 表缺失时优雅降级为空。
    if !cancel.is_cancelled() {
        let tool_calls = scan_opencode_tool_parts(&connection)?;
        if !tool_calls.is_empty() {
            let commit = writer.commit_shard(SyncShard {
                source: SourceKind::Opencode,
                reset_path_hashes: Vec::new(),
                events: Vec::new(),
                cursors: Vec::new(),
                seen_file_paths: Vec::new(),
                raw_records: Vec::new(),
                turns: Vec::new(),
                tool_calls,
            })?;
            write_ms += commit.write_ms;
        }
    }

    cursor.last_time_created = latest_time;
    cursor.last_processed_ids = latest_ids;
    cursor.sqlite_status = "ok".to_string();
    cursor.updated_at = now_utc();
    store.cursors().save_opencode_cursor(&cursor)?;

    stats.files_processed = 1;
    stats.changed_files = usize::from(seen_rows > 0);
    stats.skipped_files = usize::from(seen_rows == 0);
    stats.bytes_scanned = scanned_bytes;
    stats.events_seen = normalized_events_seen;
    stats.events_inserted = inserted;
    stats.write_ms = write_ms;
    let total_elapsed = parse_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    stats.parse_ms = total_elapsed.saturating_sub(write_ms);

    info!(
        rows_seen = seen_rows,
        events_seen = stats.events_seen,
        bytes_scanned = stats.bytes_scanned,
        "完成 OpenCode SQLite 真源解析"
    );
    Ok(stats)
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
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
            m.session_id,
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
                session_id: row.get(1)?,
                time_created: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                role: row.get(3)?,
                project_worktree: row.get(4)?,
                data: row.get(5)?,
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
        && tokens.cache_read_tokens == 0
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

    let session_id = row
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(row.id.as_str())
        .to_string();

    Ok(Some(UsageEvent {
        event_key: format!("opencode:{}", row.id),
        source: SourceKind::Opencode,
        provider_label: String::new(),
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
        session: Some(SessionInfo {
            session_label: Some(session_id.clone()),
            session_id,
            source_path_hash: None,
        }),
    }))
}

fn normalize_opencode_tokens(value: Option<&Value>) -> UsageTokens {
    let Some(value) = value else {
        return UsageTokens::default();
    };

    let input_tokens = value
        .get("input")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let cache_creation_tokens = value
        .get("cache")
        .and_then(|cache| cache.get("write"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let cache_read_tokens = value
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
        cache_read_tokens,
        cache_creation_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens: input_tokens
            + cache_creation_tokens
            + cache_read_tokens
            + output_tokens
            + reasoning_output_tokens,
    }
}

/// Renders one OpenCode SQLite row as a JSON document for the raw archive
/// (D11 / F1.5). The shape is deliberately stable and minimal: we serialize
/// only the columns the parser already reads, plus the parsed `data` payload
/// nested under `data` so consumers do not need to re-parse the inner JSON.
///
/// On `data` parse failure the original string is preserved verbatim under
/// `data_text`, so a malformed upstream row still lands in the archive.
fn serialize_opencode_row(row: &OpencodeRow) -> String {
    let parsed = serde_json::from_str::<Value>(&row.data).ok();
    let mut payload = serde_json::Map::new();
    payload.insert("id".to_string(), Value::String(row.id.clone()));
    if let Some(session_id) = row.session_id.as_deref() {
        payload.insert(
            "session_id".to_string(),
            Value::String(session_id.to_string()),
        );
    }
    payload.insert(
        "time_created".to_string(),
        Value::Number(serde_json::Number::from(row.time_created)),
    );
    if let Some(role) = row.role.as_deref() {
        payload.insert("role".to_string(), Value::String(role.to_string()));
    }
    if let Some(worktree) = row.project_worktree.as_deref() {
        payload.insert(
            "project_worktree".to_string(),
            Value::String(worktree.to_string()),
        );
    }
    match parsed {
        Some(value) => {
            payload.insert("data".to_string(), value);
        }
        None => {
            payload.insert("data_text".to_string(), Value::String(row.data.clone()));
        }
    }
    serde_json::to_string(&Value::Object(payload))
        .unwrap_or_else(|_| serde_json::json!({"id": row.id}).to_string())
}

/// Scans the OpenCode `part` table for `type == "tool"` rows and normalizes them
/// into [`UsageToolCall`] facts.
///
/// Full-scan + idempotent (`INSERT OR IGNORE` on `tool_call_key`) by design: the
/// `LIKE` filter keeps this to tool rows only, and completeness matters more than
/// re-scan cost for this secondary behavior data. The `part` table is absent on
/// older OpenCode releases, so a prepare failure degrades to an empty result
/// instead of failing the whole sync.
fn scan_opencode_tool_parts(connection: &Connection) -> Result<Vec<UsageToolCall>> {
    let mut statement = match connection.prepare(
        r#"SELECT time_created, data FROM part
           WHERE data LIKE '%"type":"tool"%'
           ORDER BY time_created ASC"#,
    ) {
        Ok(statement) => statement,
        Err(_) => return Ok(Vec::new()),
    };
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, Option<i64>>(0)?.unwrap_or_default(),
            row.get::<_, String>(1)?,
        ))
    })?;

    let mut tool_calls = Vec::new();
    for (index, row) in rows.enumerate() {
        let (time_created, data) = row?;
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        if let Some(call) = part_to_tool_call(&value, time_created, index) {
            tool_calls.push(call);
        }
    }
    Ok(tool_calls)
}

/// Normalizes one OpenCode `part` row (already JSON-parsed) into a tool-call fact.
///
/// Association is best-effort from fields inside `part.data`: `messageID` links to
/// the message event (`opencode:<id>`), `sessionID` to the session, and the part
/// `id` (or `messageID:index`) seeds the idempotency key. Project/model are not
/// carried on parts, so they stay `None`.
fn part_to_tool_call(part: &Value, time_created: i64, index: usize) -> Option<UsageToolCall> {
    let evidence = opencode_tool_evidence(part)?;

    let string_field = |key: &str| {
        part.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let part_id = string_field("id");
    let message_id = string_field("messageID");
    let session_id = string_field("sessionID");

    let key_seed = part_id
        .clone()
        .or_else(|| message_id.as_ref().map(|id| format!("{id}:{index}")))
        .unwrap_or_else(|| format!("{time_created}:{index}"));

    let occurred_at = chrono::DateTime::from_timestamp_millis(time_created)?.to_rfc3339();

    Some(UsageToolCall {
        tool_call_key: format!("tool:opencode:{key_seed}"),
        turn_key: message_id.as_ref().map(|id| format!("turn:opencode:{id}")),
        event_key: message_id.as_ref().map(|id| format!("opencode:{id}")),
        source: SourceKind::Opencode,
        session_id,
        source_path_hash: None,
        project_hash: None,
        model: None,
        occurred_at,
        tool_name: evidence.tool_name,
        tool_kind: evidence.tool_kind,
        mcp_server: evidence.mcp_server,
        mcp_tool: evidence.mcp_tool,
        input_fingerprint: evidence.input_fingerprint,
        safe_preview: evidence.safe_preview,
    })
}

#[cfg(test)]
mod tests {
    use super::normalize_opencode_tokens;
    use serde_json::json;

    #[test]
    fn opencode_cache_write_maps_to_cache_creation_not_input() {
        let tokens = normalize_opencode_tokens(Some(&json!({
            "input": 100,
            "output": 30,
            "reasoning": 7,
            "cache": {
                "write": 40,
                "read": 20
            }
        })));

        assert_eq!(tokens.input_tokens, 100);
        assert_eq!(tokens.cache_creation_tokens, 40);
        assert_eq!(tokens.cache_read_tokens, 20);
        assert_eq!(tokens.output_tokens, 30);
        assert_eq!(tokens.reasoning_output_tokens, 7);
        assert_eq!(tokens.total_tokens, 197);
    }
}
