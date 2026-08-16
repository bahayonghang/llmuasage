//! ZCode (Z.ai CLI) SQLite `model_usage` parser.
//!
//! The current ZCode CLI records one row per model call in
//! `~/.zcode/cli/db/db.sqlite::model_usage`. Only `status = 'completed'` rows
//! carry usage (verified locally: error/cancelled rows report zero tokens), so
//! the parser imports exactly those and counts skipped rows through
//! `ParseIssues` without persisting any row text.
//!
//! Token semantics (verified against 1070 local rows, see the task research):
//! `input_tokens` is cache-inclusive (`computed_total_tokens == input +
//! output` holds for every completed row and `cache_read <= input`), and
//! `output_tokens` is reasoning-inclusive. Normalization therefore subtracts
//! cache channels from input, keeps output verbatim, treats `reasoning` as a
//! diagnostic channel, and trusts `computed_total_tokens` as the authoritative
//! total.
//!
//! Incremental sync uses an OpenCode-style high-water cursor anchored on
//! `completed_at` (visibility semantics): anchoring on `started_at` would
//! permanently miss requests that start before but complete after the
//! watermark advances. Event timestamps still report `started_at`.

use std::{future::Future, path::PathBuf, pin::Pin, time::Instant};

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags, params};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    integrations,
    models::{ParseIssueKind, ParseIssues, SessionInfo, SourceKind, UsageEvent, UsageTokens},
    parsers::{ProgressSink, SourceParser, SourceSyncStats, SyncEvent},
    project::ProjectResolver,
    store::{Store, SyncRunWriter, SyncShard, ZcodeCursor},
    util::{bucket_start_from_rfc3339, hash_string, normalize_model, now_utc},
};

const ZCODE_PAGE_SIZE: i64 = 1000;
/// Fallback model when a completed row omits `model_id` (never observed
/// locally, but the schema allows it).
const FALLBACK_MODEL: &str = "zcode-unknown";

#[derive(Debug)]
struct ZcodeRow {
    id: String,
    session_id: Option<String>,
    model_id: Option<String>,
    started_at: i64,
    completed_at: i64,
    input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    cache_creation_input_tokens: i64,
    cache_read_input_tokens: i64,
    computed_total_tokens: Option<i64>,
    provider_total_tokens: Option<i64>,
    session_directory: Option<String>,
    session_path: Option<String>,
}

/// ZCode SQLite parser. Owns the watermark-paged scan + per-page commit
/// pipeline for the local `~/.zcode/cli/db/db.sqlite` `model_usage` table.
pub struct ZcodeParser;

impl SourceParser for ZcodeParser {
    fn source(&self) -> SourceKind {
        SourceKind::Zcode
    }

    fn parse<'a>(
        &'a self,
        store: &'a Store,
        writer: &'a mut SyncRunWriter,
        _parallelism: usize,
        recent_cutoff: Option<DateTime<Utc>>,
        cancel: &'a CancellationToken,
        progress: Option<ProgressSink<'a>>,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>> {
        Box::pin(sync_zcode(store, writer, recent_cutoff, cancel, progress))
    }
}

async fn sync_zcode(
    store: &Store,
    writer: &mut SyncRunWriter,
    recent_cutoff: Option<DateTime<Utc>>,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
) -> Result<SourceSyncStats> {
    /*
     * ========================================================================
     * 步骤1：按 completed_at 高水位分页读取 ZCode SQLite 真源
     * ========================================================================
     * 目标：
     * 1) 只读打开 ~/.zcode/cli/db/db.sqlite（缺失 → passive no-data 空跑）
     * 2) 只依赖 last_completed_at + last_processed_ids 锚点续跑
     * 3) completed 行 → UsageEvent；error/cancelled 行只计数不落库
     */
    info!("开始同步 ZCode SQLite 真源");

    let parse_started = Instant::now();
    let db_path = integrations::zcode::resolve_db_path();
    let mut cursor = store.cursors().load_zcode_cursor()?;
    let mut stats = SourceSyncStats {
        source: SourceKind::Zcode,
        ..SourceSyncStats::default()
    };
    let path_hash = hash_string(&db_path.to_string_lossy());
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source: SourceKind::Zcode,
            files_total: 1,
        },
    );

    if !db_path.is_file() {
        cursor.sqlite_status = "missing-db".to_string();
        cursor.updated_at = now_utc();
        stats.absent = true;
        stats.last_error = Some("ZCode SQLite DB 缺失".to_string());
        if recent_cutoff.is_none() {
            store.cursors().save_zcode_cursor(&cursor)?;
        }
        return Ok(stats);
    }

    let connection = open_readonly(&db_path)?;
    let has_computed_total = zcode_column_exists(&connection, "computed_total_tokens")?;
    let has_session_table = zcode_table_exists(&connection, "session")?;

    // 锚点校验：页 0 查询若不再包含任何已知锚点 id，说明 DB 被重建，
    // 重置高水位全量重放（opencode 同款语义）。
    if !zcode_cursor_anchor_exists(&connection, &cursor)? {
        info!(
            last_completed_at = cursor.last_completed_at,
            anchor_ids = cursor.last_processed_ids.len(),
            "检测到 ZCode DB cursor 锚点缺失，重置高水位"
        );
        cursor.last_completed_at = 0;
        cursor.last_processed_ids.clear();
    }

    let mut resolver = ProjectResolver::default();
    let mut latest_completed = cursor.last_completed_at;
    let mut latest_ids = cursor.last_processed_ids.clone();
    let mut seen_rows = 0usize;
    let mut normalized_events_seen = 0usize;
    let mut inserted = 0usize;
    let mut write_ms = 0u64;
    let mut parse_issues = ParseIssues::default();
    let recent_cutoff_ms = recent_cutoff.as_ref().map(DateTime::timestamp_millis);

    // bounded run：以已存水位与窗口下界的较大者为起点，但不推进水位/锚点。
    let mut page_last_completed = recent_cutoff_ms
        .map(|cutoff| cutoff.max(cursor.last_completed_at))
        .unwrap_or(cursor.last_completed_at);
    let mut page_last_id = if page_last_completed == cursor.last_completed_at {
        cursor
            .last_processed_ids
            .iter()
            .max()
            .cloned()
            .unwrap_or_default()
    } else {
        String::new()
    };

    // error/cancelled 行跳过计数：独立聚合查询（事件查询的 SQL 过滤看不到这些行）。
    count_skipped_rows(
        &connection,
        page_last_completed,
        &page_last_id,
        recent_cutoff_ms,
        &path_hash,
        &mut parse_issues,
    )?;

    loop {
        if cancel.is_cancelled() {
            break;
        }
        let rows = load_zcode_page(
            &connection,
            page_last_completed,
            &page_last_id,
            recent_cutoff_ms,
            has_computed_total,
            has_session_table,
        )?;
        if rows.is_empty() {
            break;
        }

        let mut page_events = Vec::new();
        for row in rows {
            page_last_completed = row.completed_at;
            page_last_id = row.id.clone();
            seen_rows += 1;

            if row.completed_at < cursor.last_completed_at {
                continue;
            }
            if row.completed_at == cursor.last_completed_at
                && cursor.last_processed_ids.contains(&row.id)
            {
                continue;
            }

            if row.completed_at > latest_completed {
                latest_completed = row.completed_at;
                latest_ids.clear();
            }
            if row.completed_at == latest_completed {
                latest_ids.push(row.id.clone());
            }

            let Some(event) = row_to_event(&row, &path_hash, &mut resolver, &mut parse_issues)
            else {
                continue;
            };
            page_events.push(event);
        }

        normalized_events_seen += page_events.len();
        if !page_events.is_empty() {
            // 流式分页：shard 仅承载本页 event，cursor 由 save_zcode_cursor 收尾。
            let commit = writer.commit_shard(SyncShard {
                source: SourceKind::Zcode,
                reset_path_hashes: Vec::new(),
                events: page_events,
                cursors: Vec::new(),
                seen_file_paths: Vec::new(),
                raw_records: Vec::new(),
                turns: Vec::new(),
                tool_calls: Vec::new(),
            })?;
            inserted += commit.events_inserted;
            write_ms += commit.write_ms;
        }
        // 全量模式页后持久化高水位（bounded run 不推进水位/锚点）。
        if recent_cutoff.is_none() && !cancel.is_cancelled() {
            cursor.last_completed_at = latest_completed;
            cursor.last_processed_ids = latest_ids.clone();
            cursor.sqlite_status = "ok".to_string();
            cursor.updated_at = now_utc();
            store.cursors().save_zcode_cursor(&cursor)?;
        }
        emit_progress(
            &mut progress,
            SyncEvent::Progress {
                source: SourceKind::Zcode,
                files_scanned: seen_rows as u64,
                records_imported: inserted as u64,
                current_file: Some(db_path.display().to_string()),
            },
        );
    }

    if recent_cutoff.is_none() && !cancel.is_cancelled() {
        cursor.sqlite_status = "ok".to_string();
        cursor.updated_at = now_utc();
        store.cursors().save_zcode_cursor(&cursor)?;
    }

    stats.files_processed = 1;
    let changed = seen_rows > 0;
    stats.changed_files = usize::from(changed);
    stats.skipped_files = usize::from(!changed);
    stats.events_seen = normalized_events_seen;
    stats.events_inserted = inserted;
    stats.write_ms = write_ms;
    stats.parse_issues = parse_issues;
    let total_elapsed = parse_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    stats.parse_ms = total_elapsed.saturating_sub(write_ms);

    info!(
        rows_seen = seen_rows,
        events_seen = stats.events_seen,
        "完成 ZCode SQLite 真源解析"
    );
    Ok(stats)
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
}

fn open_readonly(path: &PathBuf) -> Result<Connection> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

fn zcode_column_exists(connection: &Connection, column: &str) -> Result<bool> {
    let mut statement = connection.prepare("PRAGMA table_info(model_usage)")?;
    let rows = statement.query_map([], |row| {
        row.get::<_, String>(1).map(|name| name.to_lowercase())
    })?;
    for name in rows {
        if name?.eq_ignore_ascii_case(column) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn zcode_table_exists(connection: &Connection, table: &str) -> Result<bool> {
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get::<_, bool>(0),
    )?;
    Ok(exists)
}

fn zcode_cursor_anchor_exists(connection: &Connection, cursor: &ZcodeCursor) -> Result<bool> {
    if cursor.last_completed_at == 0 || cursor.last_processed_ids.is_empty() {
        return Ok(true);
    }

    let mut statement = connection
        .prepare("SELECT EXISTS(SELECT 1 FROM model_usage WHERE completed_at = ?1 AND id = ?2)")?;
    for id in &cursor.last_processed_ids {
        let exists = statement.query_row(params![cursor.last_completed_at, id], |row| {
            row.get::<_, bool>(0)
        })?;
        if !exists {
            return Ok(false);
        }
    }
    Ok(true)
}

fn count_skipped_rows(
    connection: &Connection,
    watermark: i64,
    last_id: &str,
    recent_cutoff_ms: Option<i64>,
    path_hash: &str,
    issues: &mut ParseIssues,
) -> Result<()> {
    let mut statement = connection.prepare(
        r#"
        SELECT status, COUNT(*)
        FROM model_usage
        WHERE status != 'completed'
          AND (completed_at > ?1 OR (completed_at = ?1 AND id > ?2))
          AND (?3 IS NULL OR completed_at >= ?3)
        GROUP BY status
        "#,
    )?;
    let rows = statement.query_map(params![watermark, last_id, recent_cutoff_ms], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (status, count) = row?;
        for _ in 0..count {
            issues.record(SourceKind::Zcode, path_hash, 0, ParseIssueKind::Malformed);
        }
        tracing::debug!(status, count, "ZCode 跳过未完成 model_usage 行");
    }
    Ok(())
}

fn load_zcode_page(
    connection: &Connection,
    last_completed_at: i64,
    last_id: &str,
    recent_cutoff_ms: Option<i64>,
    has_computed_total: bool,
    has_session_table: bool,
) -> Result<Vec<ZcodeRow>> {
    // 旧 schema 没有 computed_total_tokens 列：切换投影变体（CAST NULL），
    // 代码层降级 provider_total_tokens → 通道求和。缺 session 表的防御
    // 变体去掉 join，project 归属留空。
    let computed_projection = if has_computed_total {
        "mu.computed_total_tokens"
    } else {
        "CAST(NULL AS INTEGER)"
    };
    let (session_projection, session_join) = if has_session_table {
        (
            "s.directory,\n            s.path",
            "LEFT JOIN session s ON s.id = mu.session_id",
        )
    } else {
        ("CAST(NULL AS TEXT),\n            CAST(NULL AS TEXT)", "")
    };
    let sql = format!(
        r#"
        SELECT
            mu.id,
            mu.session_id,
            mu.model_id,
            mu.started_at,
            mu.completed_at,
            mu.input_tokens,
            mu.output_tokens,
            mu.reasoning_tokens,
            mu.cache_creation_input_tokens,
            mu.cache_read_input_tokens,
            {computed_projection} AS computed_total_tokens,
            mu.provider_total_tokens,
            {session_projection}
        FROM model_usage mu
        {session_join}
        WHERE mu.status = 'completed'
          AND (
            mu.completed_at > ?1
            OR (mu.completed_at = ?1 AND mu.id > ?2)
          )
          AND (?3 IS NULL OR mu.completed_at >= ?3)
        ORDER BY mu.completed_at ASC, mu.id ASC
        LIMIT ?4
        "#
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            last_completed_at,
            last_id,
            recent_cutoff_ms,
            ZCODE_PAGE_SIZE
        ],
        |row| {
            let opt_i64 = |index: usize| -> rusqlite::Result<i64> {
                Ok(row.get::<_, Option<i64>>(index)?.unwrap_or_default())
            };
            Ok(ZcodeRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                model_id: row.get(2)?,
                started_at: opt_i64(3)?,
                completed_at: opt_i64(4)?,
                input_tokens: opt_i64(5)?,
                output_tokens: opt_i64(6)?,
                reasoning_tokens: opt_i64(7)?,
                cache_creation_input_tokens: opt_i64(8)?,
                cache_read_input_tokens: opt_i64(9)?,
                computed_total_tokens: row.get(10)?,
                provider_total_tokens: row.get(11)?,
                session_directory: row.get(12)?,
                session_path: row.get(13)?,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Maps one completed `model_usage` row to a normalized [`UsageEvent`].
///
/// Returns `None` for all-zero rows. Semantic anomalies (negative channels,
/// authoritative-total mismatch) clamp to safe values and count one parse
/// issue each without changing the reported channels silently.
fn row_to_event(
    row: &ZcodeRow,
    path_hash: &str,
    resolver: &mut ProjectResolver,
    issues: &mut ParseIssues,
) -> Option<UsageEvent> {
    let cache_read = row.cache_read_input_tokens.max(0);
    let cache_creation = row.cache_creation_input_tokens.max(0);
    let raw_input = row.input_tokens.max(0);
    let output = row.output_tokens.max(0);
    let reasoning = row.reasoning_tokens.max(0);

    // cache-inclusive 修正：内部 input 通道必须非缓存（饱和减）。
    let cache_overlap = cache_read.saturating_add(cache_creation);
    let input = if raw_input < cache_overlap {
        issues.record(
            SourceKind::Zcode,
            path_hash,
            row.started_at.max(0) as u64,
            ParseIssueKind::Malformed,
        );
        0
    } else {
        raw_input - cache_overlap
    };

    // total：computed_total_tokens 权威 → provider_total_tokens 降级 → 通道求和。
    let channel_total = input
        .saturating_add(cache_read)
        .saturating_add(cache_creation)
        .saturating_add(output);
    let total = match row
        .computed_total_tokens
        .or(row.provider_total_tokens)
        .map(|total| total.max(0))
    {
        Some(total) => total,
        None => channel_total,
    };
    if let Some(computed) = row.computed_total_tokens.filter(|total| *total >= 0) {
        let reported = row.input_tokens.max(0).saturating_add(output);
        if computed != reported {
            issues.record(
                SourceKind::Zcode,
                path_hash,
                row.started_at.max(0) as u64,
                ParseIssueKind::Malformed,
            );
        }
    }

    if total == 0 && input == 0 && cache_read == 0 && cache_creation == 0 && output == 0 {
        return None;
    }

    let timestamp = chrono::DateTime::from_timestamp_millis(row.started_at)?;
    let event_at = timestamp.to_rfc3339();
    let hour_start = bucket_start_from_rfc3339(&event_at)?;

    let project = row
        .session_directory
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            row.session_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .map(std::path::PathBuf::from)
        .and_then(|path| resolver.resolve(&path).ok().flatten());

    let session_id = row
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string();

    Some(UsageEvent {
        event_key: format!("zcode:{}", hash_string(&row.id)),
        source: SourceKind::Zcode,
        provider_label: String::new(),
        model: match row.model_id.as_deref().map(str::trim) {
            Some(model) if !model.is_empty() => normalize_model(Some(model)),
            _ => FALLBACK_MODEL.to_string(),
        },
        event_at,
        hour_start,
        tokens: UsageTokens {
            input_tokens: input,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
            output_tokens: output,
            reasoning_output_tokens: reasoning,
            total_tokens: total,
        },
        project,
        session: (!session_id.is_empty()).then(|| SessionInfo {
            session_label: Some(session_id.clone()),
            session_id,
            source_path_hash: None,
        }),
    })
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;

    use super::*;

    /// Creates a synthetic ZCode DB with the current `model_usage` schema and
    /// the minimal `session` columns the parser joins for project attribution.
    fn synthetic_db(computed_total: bool) -> Connection {
        let conn = Connection::open_in_memory().expect("memory db");
        conn.execute_batch(
            r#"
            CREATE TABLE session(
                id TEXT PRIMARY KEY,
                directory TEXT,
                path TEXT
            );
            CREATE TABLE model_usage(
                id TEXT PRIMARY KEY,
                session_id TEXT,
                model_id TEXT,
                status TEXT,
                started_at INTEGER,
                completed_at INTEGER,
                input_tokens INTEGER,
                output_tokens INTEGER,
                reasoning_tokens INTEGER,
                cache_creation_input_tokens INTEGER,
                cache_read_input_tokens INTEGER,
                provider_total_tokens INTEGER
            );
            "#,
        )
        .expect("base schema");
        if computed_total {
            conn.execute_batch("ALTER TABLE model_usage ADD COLUMN computed_total_tokens INTEGER;")
                .expect("computed column");
        }
        conn
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_row(
        conn: &Connection,
        id: &str,
        status: &str,
        started: i64,
        completed: i64,
        input: i64,
        output: i64,
        reasoning: i64,
        cache_creation: i64,
        cache_read: i64,
        computed: Option<i64>,
        provider_total: Option<i64>,
    ) {
        let has_computed = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('model_usage') WHERE name = 'computed_total_tokens')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .expect("probe computed column");
        if has_computed {
            conn.execute(
                "INSERT INTO model_usage(
                    id, session_id, model_id, status, started_at, completed_at,
                    input_tokens, output_tokens, reasoning_tokens,
                    cache_creation_input_tokens, cache_read_input_tokens,
                    provider_total_tokens, computed_total_tokens
                ) VALUES (?1, 'sess-1', 'GLM-5.3', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    id,
                    status,
                    started,
                    completed,
                    input,
                    output,
                    reasoning,
                    cache_creation,
                    cache_read,
                    provider_total,
                    computed,
                ],
            )
            .expect("insert row");
        } else {
            conn.execute(
                "INSERT INTO model_usage(
                    id, session_id, model_id, status, started_at, completed_at,
                    input_tokens, output_tokens, reasoning_tokens,
                    cache_creation_input_tokens, cache_read_input_tokens,
                    provider_total_tokens
                ) VALUES (?1, 'sess-1', 'GLM-5.3', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    id,
                    status,
                    started,
                    completed,
                    input,
                    output,
                    reasoning,
                    cache_creation,
                    cache_read,
                    provider_total,
                ],
            )
            .expect("insert row");
        }
    }

    fn load_all(conn: &Connection, has_computed: bool) -> Vec<ZcodeRow> {
        load_zcode_page(conn, 0, "", None, has_computed, true).expect("page load")
    }

    #[test]
    fn only_completed_rows_are_selected_and_ordered() {
        let conn = synthetic_db(true);
        insert_row(
            &conn,
            "a",
            "error",
            100,
            100,
            0,
            0,
            0,
            0,
            0,
            Some(0),
            Some(0),
        );
        insert_row(
            &conn,
            "b",
            "completed",
            100,
            110,
            10,
            5,
            0,
            0,
            0,
            Some(15),
            Some(15),
        );
        insert_row(
            &conn,
            "c",
            "cancelled",
            100,
            120,
            0,
            0,
            0,
            0,
            0,
            Some(0),
            Some(0),
        );

        let rows = load_all(&conn, true);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "b");
    }

    #[test]
    fn glm_row_normalizes_cache_inclusive_input() {
        // 本机 GLM-5.3 例证形状：in=60543, cr=56960, out=3254, total=63797。
        let conn = synthetic_db(true);
        insert_row(
            &conn,
            "glm",
            "completed",
            1_700_000_000_000,
            1_700_000_001_000,
            60_543,
            3_254,
            0,
            0,
            56_960,
            Some(63_797),
            Some(63_797),
        );

        let row = &load_all(&conn, true)[0];
        let mut issues = ParseIssues::default();
        let mut resolver = ProjectResolver::default();
        let event = row_to_event(row, "hash", &mut resolver, &mut issues).expect("event");

        assert_eq!(event.tokens.input_tokens, 60_543 - 56_960);
        assert_eq!(event.tokens.cache_read_tokens, 56_960);
        assert_eq!(event.tokens.output_tokens, 3_254);
        assert_eq!(event.tokens.total_tokens, 63_797);
        assert_eq!(issues.total(), 0);
    }

    #[test]
    fn deepseek_row_keeps_reasoning_inside_output_and_authoritative_total() {
        // 本机 deepseek-v4-flash 例证：in=316, cr=256, out=391, reasoning=380,
        // total=707 (= in + out；reasoning 是 output 子集，诊断通道单列)。
        let conn = synthetic_db(true);
        insert_row(
            &conn,
            "ds",
            "completed",
            1_700_000_000_000,
            1_700_000_001_000,
            316,
            391,
            380,
            0,
            256,
            Some(707),
            Some(707),
        );

        let row = &load_all(&conn, true)[0];
        let mut issues = ParseIssues::default();
        let mut resolver = ProjectResolver::default();
        let event = row_to_event(row, "hash", &mut resolver, &mut issues).expect("event");

        assert_eq!(event.tokens.input_tokens, 60);
        assert_eq!(event.tokens.cache_read_tokens, 256);
        assert_eq!(event.tokens.output_tokens, 391);
        assert_eq!(event.tokens.reasoning_output_tokens, 380);
        assert_eq!(event.tokens.total_tokens, 707);
        assert_eq!(issues.total(), 0);
    }

    #[test]
    fn old_schema_without_computed_total_falls_back() {
        let conn = synthetic_db(false);
        insert_row(
            &conn,
            "old",
            "completed",
            1_700_000_000_000,
            1_700_000_001_000,
            100,
            40,
            0,
            0,
            60,
            None,
            None,
        );

        assert!(!zcode_column_exists(&conn, "computed_total_tokens").unwrap());
        let row = &load_all(&conn, false)[0];
        assert!(row.computed_total_tokens.is_none());
        let mut issues = ParseIssues::default();
        let mut resolver = ProjectResolver::default();
        let event = row_to_event(row, "hash", &mut resolver, &mut issues).expect("event");

        // 无 computed/provider total：通道求和 40 + 60 + 40 = 140。
        assert_eq!(event.tokens.input_tokens, 40);
        assert_eq!(event.tokens.total_tokens, 140);
    }

    #[test]
    fn provider_total_is_the_second_fallback() {
        let conn = synthetic_db(false);
        insert_row(
            &conn,
            "pt",
            "completed",
            1_700_000_000_000,
            1_700_000_001_000,
            100,
            40,
            0,
            0,
            60,
            None,
            Some(500),
        );

        let row = &load_all(&conn, false)[0];
        let mut issues = ParseIssues::default();
        let mut resolver = ProjectResolver::default();
        let event = row_to_event(row, "hash", &mut resolver, &mut issues).expect("event");

        assert_eq!(event.tokens.total_tokens, 500);
    }

    #[test]
    fn negative_and_pathological_channels_clamp_with_issue() {
        let conn = synthetic_db(true);
        insert_row(
            &conn,
            "bad",
            "completed",
            1_700_000_000_000,
            1_700_000_001_000,
            10,
            5,
            -3,
            0,
            900,
            Some(15),
            None,
        );

        let row = &load_all(&conn, true)[0];
        let mut issues = ParseIssues::default();
        let mut resolver = ProjectResolver::default();
        let event = row_to_event(row, "hash", &mut resolver, &mut issues).expect("event");

        assert_eq!(
            event.tokens.input_tokens, 0,
            "input clamps below cache overlap"
        );
        assert_eq!(event.tokens.reasoning_output_tokens, 0);
        assert!(issues.total() >= 1, "pathological row counts a parse issue");
    }

    #[test]
    fn all_zero_rows_are_skipped() {
        let conn = synthetic_db(true);
        insert_row(
            &conn,
            "zero",
            "completed",
            1_700_000_000_000,
            1_700_000_001_000,
            0,
            0,
            0,
            0,
            0,
            Some(0),
            Some(0),
        );

        let row = &load_all(&conn, true)[0];
        let mut issues = ParseIssues::default();
        let mut resolver = ProjectResolver::default();
        assert!(row_to_event(row, "hash", &mut resolver, &mut issues).is_none());
    }

    #[test]
    fn computed_total_mismatch_counts_issue_without_changing_numbers() {
        let conn = synthetic_db(true);
        insert_row(
            &conn,
            "mismatch",
            "completed",
            1_700_000_000_000,
            1_700_000_001_000,
            100,
            40,
            0,
            0,
            0,
            Some(999),
            None,
        );

        let row = &load_all(&conn, true)[0];
        let mut issues = ParseIssues::default();
        let mut resolver = ProjectResolver::default();
        let event = row_to_event(row, "hash", &mut resolver, &mut issues).expect("event");

        assert_eq!(event.tokens.total_tokens, 999);
        assert_eq!(issues.malformed_lines, 1);
    }

    #[test]
    fn pagination_uses_completed_watermark_and_id_tiebreaker() {
        let conn = synthetic_db(true);
        insert_row(
            &conn,
            "a",
            "completed",
            10,
            100,
            1,
            1,
            0,
            0,
            0,
            Some(2),
            None,
        );
        insert_row(
            &conn,
            "b",
            "completed",
            10,
            100,
            2,
            2,
            0,
            0,
            0,
            Some(4),
            None,
        );
        insert_row(
            &conn,
            "c",
            "completed",
            10,
            200,
            3,
            3,
            0,
            0,
            0,
            Some(6),
            None,
        );

        // 水位 (100, "a")：只剩同毫秒的 b 与更晚的 c。
        let rows = load_zcode_page(&conn, 100, "a", None, true, true).expect("page");
        let ids: Vec<_> = rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "c"]);

        // 水位 (100, "b")：只剩 c。
        let rows = load_zcode_page(&conn, 100, "b", None, true, true).expect("page");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "c");
    }

    #[test]
    fn recent_cutoff_filters_in_sql() {
        let conn = synthetic_db(true);
        insert_row(
            &conn,
            "old",
            "completed",
            10,
            100,
            1,
            1,
            0,
            0,
            0,
            Some(2),
            None,
        );
        insert_row(
            &conn,
            "new",
            "completed",
            10,
            5_000,
            3,
            3,
            0,
            0,
            0,
            Some(6),
            None,
        );

        let rows = load_zcode_page(&conn, 0, "", Some(1_000), true, true).expect("page");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "new");
    }

    #[test]
    fn skipped_status_rows_are_counted() {
        let conn = synthetic_db(true);
        insert_row(&conn, "e1", "error", 10, 100, 0, 0, 0, 0, 0, Some(0), None);
        insert_row(&conn, "e2", "error", 10, 101, 0, 0, 0, 0, 0, Some(0), None);
        insert_row(
            &conn,
            "x1",
            "cancelled",
            10,
            102,
            0,
            0,
            0,
            0,
            0,
            Some(0),
            None,
        );

        let mut issues = ParseIssues::default();
        count_skipped_rows(&conn, 0, "", None, "hash", &mut issues).expect("count");
        assert_eq!(issues.malformed_lines, 3);
    }

    #[test]
    fn anchor_existence_detects_rebuilt_database() {
        let conn = synthetic_db(true);
        insert_row(
            &conn,
            "a",
            "completed",
            10,
            100,
            1,
            1,
            0,
            0,
            0,
            Some(2),
            None,
        );

        let cursor = ZcodeCursor {
            last_completed_at: 100,
            last_processed_ids: vec!["a".to_string()],
            ..ZcodeCursor::default()
        };
        assert!(zcode_cursor_anchor_exists(&conn, &cursor).unwrap());

        let rebuilt = ZcodeCursor {
            last_completed_at: 100,
            last_processed_ids: vec!["ghost".to_string()],
            ..ZcodeCursor::default()
        };
        assert!(!zcode_cursor_anchor_exists(&conn, &rebuilt).unwrap());
    }

    #[test]
    fn event_key_is_stable_hash_of_row_id() {
        let conn = synthetic_db(true);
        insert_row(
            &conn,
            "stable",
            "completed",
            1_700_000_000_000,
            1_700_000_001_000,
            10,
            5,
            0,
            0,
            0,
            Some(15),
            None,
        );
        let row = &load_all(&conn, true)[0];
        let mut resolver = ProjectResolver::default();
        let first =
            row_to_event(row, "hash", &mut resolver, &mut ParseIssues::default()).expect("event");
        let second =
            row_to_event(row, "hash", &mut resolver, &mut ParseIssues::default()).expect("event");
        assert_eq!(first.event_key, second.event_key);
        assert_eq!(first.event_key, format!("zcode:{}", hash_string("stable")));
        let _ = json!({}); // keep serde_json referenced for future fixtures
    }
}
