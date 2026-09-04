use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    time::{Duration, Instant},
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    integrations,
    models::{
        ActivityCategory, ParseIssueKind, ParseIssues, SessionInfo, SourceKind, UsageEvent,
        UsageTokens, UsageToolCall, UsageTurn,
    },
    parsers::{
        ProgressSink, SourceParser, SourceSyncStats, SyncEvent, behavior::opencode_tool_evidence,
    },
    project::ProjectResolver,
    store::{OpencodeCursor, Store, SyncRunWriter, SyncShard},
    util::{bucket_start_from_rfc3339, hash_string, normalize_model, now_utc},
};

const OPENCODE_PAGE_SIZE: i64 = 1000;
const OPENCODE_PART_PAGE_SIZE: i64 = 1000;
const SOURCE_DB_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct OpencodeRow {
    id: String,
    session_id: Option<String>,
    time_created: i64,
    role: Option<String>,
    project_worktree: Option<String>,
    data: String,
}

#[derive(Debug)]
struct OpencodeToolPartRow {
    rowid: i64,
    time_created: i64,
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
        recent_cutoff: Option<DateTime<Utc>>,
        cancel: &'a CancellationToken,
        progress: Option<ProgressSink<'a>>,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>> {
        Box::pin(sync_opencode(
            store,
            writer,
            parallelism,
            recent_cutoff,
            cancel,
            progress,
        ))
    }
}

async fn sync_opencode(
    store: &Store,
    writer: &mut SyncRunWriter,
    _parallelism: usize,
    recent_cutoff: Option<DateTime<Utc>>,
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
    let mut cursor = store.cursors().load_opencode_cursor("local")?;
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
        if recent_cutoff.is_none() {
            store.cursors().save_opencode_cursor("local", &cursor)?;
        }
        return Ok(stats);
    }

    // 1.2 用已处理消息作为数据库代际锚点。文件长度/mtime 会随 SQLite
    // 正常增长变化，不能用于区分原库增长与替换库。
    let connection = match open_source_db(&db_path) {
        Ok(connection) => connection,
        Err(_) => {
            stats.last_error = Some("OpenCode SQLite DB 打开失败".to_string());
            return Ok(stats);
        }
    };
    let path_hash = hash_string(&db_path.to_string_lossy());
    let mut parse_issues = ParseIssues::default();
    if !opencode_cursor_anchor_exists(&connection, &cursor)? {
        info!(
            last_time_created = cursor.last_time_created,
            anchor_ids = cursor.last_processed_ids.len(),
            "检测到 OpenCode DB cursor 锚点缺失，重置高水位"
        );
        cursor.last_time_created = 0;
        cursor.last_processed_ids.clear();
        cursor.last_part_rowid = 0;
    }

    let mut resolver = ProjectResolver::default();
    let raw_archive_enabled = store.raw_archive_enabled()?;
    let mut latest_time = cursor.last_time_created;
    let mut latest_ids = cursor.last_processed_ids.clone();
    let mut seen_rows = 0usize;
    let mut normalized_events_seen = 0usize;
    let mut scanned_bytes = 0u64;
    let mut inserted = 0usize;
    let mut write_ms = 0u64;
    let initial_part_rowid = cursor.last_part_rowid;
    let mut part_rows_seen = 0usize;
    let recent_cutoff_ms = recent_cutoff.as_ref().map(DateTime::timestamp_millis);
    let mut page_last_time = recent_cutoff_ms
        .map(|cutoff| cutoff.max(cursor.last_time_created))
        .unwrap_or(cursor.last_time_created);
    let mut page_last_id = if page_last_time == cursor.last_time_created {
        cursor
            .last_processed_ids
            .iter()
            .max()
            .cloned()
            .unwrap_or_default()
    } else {
        String::new()
    };

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
            let commit = writer.commit_shard(SyncShard {
                events: page_events,
                raw_records: page_raw,
                turns: page_turns,
                opencode_cursor: snapshot_opencode_cursor(
                    recent_cutoff,
                    &mut cursor,
                    latest_time,
                    &latest_ids,
                ),
                ..SyncShard::new(SourceKind::Opencode)
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

    // part 工具事实使用独立 rowid 高水位。封闭上界避免本轮持续追赶活跃 DB，
    // 页级 commit 保持内存有界；part 表缺失时优雅降级为空。
    if !cancel.is_cancelled()
        && let Some(upper_rowid) = opencode_part_upper_rowid(&connection)?
    {
        let mut page_rowid = cursor.last_part_rowid;
        while page_rowid < upper_rowid && !cancel.is_cancelled() {
            let rows = load_opencode_tool_part_page(
                &connection,
                page_rowid,
                upper_rowid,
                recent_cutoff_ms,
            )?;
            if rows.is_empty() {
                break;
            }

            let mut tool_calls = Vec::new();
            for row in rows {
                page_rowid = row.rowid;
                part_rows_seen += 1;
                scanned_bytes += row.data.len() as u64;
                let Ok(value) = serde_json::from_str::<Value>(&row.data) else {
                    parse_issues.record(
                        SourceKind::Opencode,
                        &path_hash,
                        row.rowid.max(0) as u64,
                        ParseIssueKind::Malformed,
                        "opencode_tool_json",
                    );
                    continue;
                };
                if let Some(call) = part_to_tool_call(&value, row.time_created, row.rowid) {
                    tool_calls.push(call);
                }
            }
            if !tool_calls.is_empty() {
                let commit = writer.commit_shard(SyncShard {
                    tool_calls,
                    opencode_cursor: snapshot_opencode_cursor(
                        recent_cutoff,
                        &mut cursor,
                        latest_time,
                        &latest_ids,
                    ),
                    ..SyncShard::new(SourceKind::Opencode)
                })?;
                write_ms += commit.write_ms;
            }
            emit_progress(
                &mut progress,
                SyncEvent::Progress {
                    source: SourceKind::Opencode,
                    files_scanned: (seen_rows + part_rows_seen) as u64,
                    records_imported: inserted as u64,
                    current_file: Some(db_path.display().to_string()),
                },
            );
        }
        if !cancel.is_cancelled() {
            cursor.last_part_rowid = upper_rowid;
        }
    }

    if let Some(final_cursor) =
        snapshot_opencode_cursor(recent_cutoff, &mut cursor, latest_time, &latest_ids)
    {
        let commit = writer.commit_shard(SyncShard {
            opencode_cursor: Some(final_cursor),
            ..SyncShard::new(SourceKind::Opencode)
        })?;
        write_ms += commit.write_ms;
    }

    stats.files_processed = 1;
    let part_cursor_advanced = cursor.last_part_rowid > initial_part_rowid;
    let changed = seen_rows > 0 || part_cursor_advanced;
    stats.changed_files = usize::from(changed);
    stats.skipped_files = usize::from(!changed);
    stats.bytes_scanned = scanned_bytes;
    stats.events_seen = normalized_events_seen;
    stats.events_inserted = inserted;
    stats.write_ms = write_ms;
    stats.parse_issues = parse_issues;
    let total_elapsed = parse_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    stats.parse_ms = total_elapsed.saturating_sub(write_ms);

    info!(
        rows_seen = seen_rows,
        part_rows_seen,
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

/// Opens the user's OpenCode SQLite database read-only with a busy timeout.
pub fn open_source_db(path: &Path) -> Result<Connection> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(SOURCE_DB_BUSY_TIMEOUT)?;
    connection.query_row("PRAGMA schema_version", [], |row| row.get::<_, i64>(0))?;
    Ok(connection)
}

fn snapshot_opencode_cursor(
    recent_cutoff: Option<DateTime<Utc>>,
    cursor: &mut OpencodeCursor,
    latest_time: i64,
    latest_ids: &[String],
) -> Option<Box<OpencodeCursor>> {
    if recent_cutoff.is_some() {
        return None;
    }
    cursor.last_time_created = latest_time;
    cursor.last_processed_ids = latest_ids.to_vec();
    cursor.sqlite_status = "ok".to_string();
    cursor.updated_at = now_utc();
    Some(Box::new(cursor.clone()))
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

fn opencode_cursor_anchor_exists(
    connection: &Connection,
    cursor: &crate::store::OpencodeCursor,
) -> Result<bool> {
    if cursor.last_time_created == 0 || cursor.last_processed_ids.is_empty() {
        return Ok(true);
    }

    let mut statement = connection
        .prepare("SELECT EXISTS(SELECT 1 FROM message WHERE time_created = ?1 AND id = ?2)")?;
    for id in &cursor.last_processed_ids {
        let exists = statement.query_row(params![cursor.last_time_created, id], |row| {
            row.get::<_, bool>(0)
        })?;
        if !exists {
            return Ok(false);
        }
    }
    Ok(true)
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
        source_cost: None,
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
        .max(0);
    let cache_creation_tokens = value
        .get("cache")
        .and_then(|cache| cache.get("write"))
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0);
    let cache_read_tokens = value
        .get("cache")
        .and_then(|cache| cache.get("read"))
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0);
    let output_tokens = value
        .get("output")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0);
    let reasoning_output_tokens = value
        .get("reasoning")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0);
    let known_total = input_tokens + cache_creation_tokens + cache_read_tokens + output_tokens;
    let total_tokens = value
        .get("total")
        .and_then(Value::as_i64)
        .filter(|total| *total >= 0)
        .unwrap_or_default()
        .max(known_total);

    UsageTokens {
        input_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens,
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

fn opencode_part_upper_rowid(connection: &Connection) -> Result<Option<i64>> {
    let mut statement = match connection.prepare("SELECT COALESCE(MAX(rowid), 0) FROM part") {
        Ok(statement) => statement,
        Err(_) => return Ok(None),
    };
    Ok(Some(statement.query_row([], |row| row.get(0))?))
}

fn load_opencode_tool_part_page(
    connection: &Connection,
    last_rowid: i64,
    upper_rowid: i64,
    recent_cutoff_ms: Option<i64>,
) -> Result<Vec<OpencodeToolPartRow>> {
    let mut statement = connection.prepare(
        r#"
        SELECT rowid, time_created, data
        FROM part
        WHERE rowid > ?1 AND rowid <= ?2
          AND (?3 IS NULL OR time_created >= ?3)
          AND data LIKE '%"type":"tool"%'
        ORDER BY rowid ASC
        LIMIT ?4
        "#,
    )?;
    let rows = statement.query_map(
        params![
            last_rowid,
            upper_rowid,
            recent_cutoff_ms,
            OPENCODE_PART_PAGE_SIZE
        ],
        |row| {
            Ok(OpencodeToolPartRow {
                rowid: row.get(0)?,
                time_created: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                data: row.get(2)?,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Normalizes one OpenCode `part` row (already JSON-parsed) into a tool-call fact.
///
/// Association is best-effort from fields inside `part.data`: `messageID` links to
/// the message event (`opencode:<id>`), `sessionID` to the session, and the part
/// `id` (or `messageID:index`) seeds the idempotency key. Project/model are not
/// carried on parts, so they stay `None`.
fn part_to_tool_call(part: &Value, time_created: i64, rowid: i64) -> Option<UsageToolCall> {
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
        .or_else(|| message_id.as_ref().map(|id| format!("{id}:{rowid}")))
        .unwrap_or_else(|| format!("{time_created}:{rowid}"));

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
    use super::{load_opencode_page, normalize_opencode_tokens, open_source_db};
    use rusqlite::Connection;
    use serde_json::json;

    #[test]
    fn open_source_db_is_read_only_with_busy_timeout() -> anyhow::Result<()> {
        let dir = tempfile::TempDir::new()?;
        let path = dir.path().join("opencode.db");
        Connection::open(&path)?;
        let connection = open_source_db(&path)?;
        let timeout_ms: i64 = connection.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
        assert!(
            timeout_ms >= 1000,
            "busy timeout must be at least 1s, got {timeout_ms}"
        );
        assert!(
            connection
                .execute("CREATE TABLE write_probe(x INTEGER)", [])
                .is_err(),
            "OpenCode source DB must open read-only"
        );
        Ok(())
    }

    #[test]
    fn recent_lower_bound_prunes_old_message_rows_in_sql() -> anyhow::Result<()> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(
            r#"
            CREATE TABLE project(id TEXT PRIMARY KEY, worktree TEXT);
            CREATE TABLE session(id TEXT PRIMARY KEY, project_id TEXT);
            CREATE TABLE message(
                id TEXT PRIMARY KEY,
                session_id TEXT,
                time_created INTEGER,
                data TEXT
            );
            INSERT INTO message(id, time_created, data)
            VALUES
                ('old', 100, '{"role":"assistant"}'),
                ('recent', 200, '{"role":"assistant"}');
            "#,
        )?;

        let rows = load_opencode_page(&connection, 150, "")?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "recent");
        Ok(())
    }

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
        assert_eq!(tokens.total_tokens, 190);
    }

    #[test]
    fn opencode_prefers_authoritative_total_when_larger_than_known_components() {
        let tokens = normalize_opencode_tokens(Some(&json!({
            "input": 100,
            "output": 30,
            "reasoning": 7,
            "total": 250,
            "cache": {
                "write": 40,
                "read": 20
            }
        })));

        assert_eq!(tokens.total_tokens, 250);
    }

    #[test]
    fn opencode_clamps_negative_channels() {
        let tokens = normalize_opencode_tokens(Some(&json!({
            "input": -10,
            "output": -3,
            "reasoning": -1,
            "total": -14,
            "cache": {"read": -2, "write": -4}
        })));

        assert_eq!(tokens, crate::models::UsageTokens::default());
    }
}
