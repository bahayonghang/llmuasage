use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use llmusage::{
    app::AppContext,
    commands,
    models::{SessionInfo, SourceKind, UsageEvent, UsageTokens},
    parsers::{SourceParser, SourceSyncStats, SyncEvent, ZcodeParser},
    query::{Dashboard, QueryFilter},
    store::{HolderKind, Store, SyncShard, expected_token_accounting_version},
    util::hash_string,
};
use rusqlite::Connection;
use tempfile::TempDir;

#[path = "../support/env.rs"]
mod test_env;
#[path = "../support/process.rs"]
mod test_process;

/// End-to-end guard for the sync display contract: the final summary table
/// (with its aggregated `TOTAL` row) is the only thing on stdout, non-TTY
/// stdout carries no ANSI, and the removed per-source completion sentence never
/// appears on either stream — verified across a wide and a narrow `COLUMNS`.
fn grok_turn_usage_line(prompt_id: &str, timestamp_ms: i64, usage: &str) -> String {
    let usage: serde_json::Value = serde_json::from_str(usage).expect("usage json");
    let mut line = serde_json::json!({
        "params": {
            "update": {
                "sessionUpdate": "turn_completed",
                "prompt_id": prompt_id,
                "usage": usage
            },
            "_meta": { "agentTimestampMs": timestamp_ms }
        }
    })
    .to_string();
    line.push('\n');
    line
}

#[derive(Debug)]
struct GrokEventRow {
    event_key: String,
    model: String,
    input_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    total_tokens: i64,
    pricing_status: String,
    provider_label: String,
    project_label: Option<String>,
}

fn grok_event_rows(db_path: &Path) -> Result<Vec<GrokEventRow>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT event_key, model, input_tokens, cache_read_tokens, cache_creation_tokens,
               output_tokens, reasoning_output_tokens, total_tokens, pricing_status,
               provider_label, project_label
        FROM usage_event
        WHERE source = 'grok'
        ORDER BY event_key
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(GrokEventRow {
            event_key: row.get(0)?,
            model: row.get(1)?,
            input_tokens: row.get(2)?,
            cache_read_tokens: row.get(3)?,
            cache_creation_tokens: row.get(4)?,
            output_tokens: row.get(5)?,
            reasoning_tokens: row.get(6)?,
            total_tokens: row.get(7)?,
            pricing_status: row.get(8)?,
            provider_label: row.get(9)?,
            project_label: row.get(10)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn assert_grok_totals(db_path: &Path, expected_events: i64, expected_total: i64) -> Result<()> {
    let conn = Connection::open(db_path)?;
    let (events, total): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(total_tokens), 0) FROM usage_event WHERE source = 'grok'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let bucket_total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(total_tokens), 0) FROM usage_bucket_30m WHERE source = 'grok'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(events, expected_events);
    assert_eq!(total, expected_total);
    assert_eq!(bucket_total, expected_total);
    Ok(())
}

fn pi_event_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'pi'",
        [],
        |row| row.get(0),
    )?)
}

/// One stored Pi row as `(model, input, cache_read, cache_creation, output, reasoning, total)`.
type PiEventRow = (String, i64, i64, i64, i64, i64, i64);

fn pi_event_rows(db_path: &Path) -> Result<Vec<PiEventRow>> {
    source_event_rows(db_path, "pi")
}

fn omp_event_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'omp'",
        [],
        |row| row.get(0),
    )?)
}

fn omp_turn_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_turn WHERE source = 'omp'",
        [],
        |row| row.get(0),
    )?)
}

fn omp_tool_call_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_tool_call WHERE source = 'omp'",
        [],
        |row| row.get(0),
    )?)
}

fn omp_tool_kind_counts(db_path: &Path) -> Result<Vec<(String, i64)>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT tool_kind, COUNT(*) FROM usage_tool_call WHERE source = 'omp' GROUP BY 1 ORDER BY 1",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn omp_turn_retry_rows(db_path: &Path) -> Result<Vec<(i64, i64)>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT retries, one_shot FROM usage_turn WHERE source = 'omp' ORDER BY started_at",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn omp_empty_turn_project_hash_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_turn WHERE source = 'omp' AND (project_hash IS NULL OR project_hash = '')",
        [],
        |row| row.get(0),
    )?)
}

fn omp_empty_tool_call_project_hash_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_tool_call WHERE source = 'omp' AND (project_hash IS NULL OR project_hash = '')",
        [],
        |row| row.get(0),
    )?)
}

fn omp_overlong_preview_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_tool_call WHERE source = 'omp' AND LENGTH(safe_preview) > 120",
        [],
        |row| row.get(0),
    )?)
}

fn omp_preview_secret_count(db_path: &Path, secret: &str) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_tool_call WHERE source = 'omp' AND instr(COALESCE(safe_preview, ''), ?1) > 0",
        [secret],
        |row| row.get(0),
    )?)
}

fn omp_orphan_turns(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM usage_turn t
        LEFT JOIN usage_event e ON e.event_key = substr(t.turn_key, 6)
        WHERE t.source = 'omp' AND e.event_key IS NULL
        "#,
        [],
        |row| row.get(0),
    )?)
}

fn omp_orphan_tool_calls(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM usage_tool_call tc
        LEFT JOIN usage_event e ON e.event_key = tc.event_key
        WHERE tc.source = 'omp' AND e.event_key IS NULL
        "#,
        [],
        |row| row.get(0),
    )?)
}

fn omp_event_rows(db_path: &Path) -> Result<Vec<PiEventRow>> {
    source_event_rows(db_path, "omp")
}

struct OmpPricingRow {
    model: String,
    cost_with_cache_usd: f64,
    cost_without_cache_usd: f64,
    pricing_status: String,
    pricing_source: Option<String>,
}

fn omp_pricing_rows(db_path: &Path) -> Result<Vec<OmpPricingRow>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT model, cost_with_cache_usd, cost_without_cache_usd,
               pricing_status, pricing_source
        FROM usage_event
        WHERE source = 'omp'
        ORDER BY event_at, model
        "#,
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(OmpPricingRow {
                model: row.get(0)?,
                cost_with_cache_usd: row.get(1)?,
                cost_without_cache_usd: row.get(2)?,
                pricing_status: row.get(3)?,
                pricing_source: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

struct OmpDimensionRow {
    provider_label: String,
    project_label: Option<String>,
    project_hash: Option<String>,
    project_ref: Option<String>,
    session_id: String,
    session_label: Option<String>,
}

fn omp_dimension_rows(db_path: &Path) -> Result<Vec<OmpDimensionRow>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT provider_label, project_label, project_hash, project_ref, session_id, session_label
        FROM usage_event
        WHERE source = 'omp'
        ORDER BY event_at
        "#,
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(OmpDimensionRow {
                provider_label: row.get(0)?,
                project_label: row.get(1)?,
                project_hash: row.get(2)?,
                project_ref: row.get(3)?,
                session_id: row.get(4)?,
                session_label: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_by_session<'a>(rows: &'a [OmpDimensionRow], session_id: &str) -> &'a OmpDimensionRow {
    rows.iter()
        .find(|row| row.session_id == session_id)
        .unwrap_or_else(|| panic!("missing session {session_id}"))
}

fn source_event_rows(db_path: &Path, source: &str) -> Result<Vec<PiEventRow>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT model, input_tokens, cache_read_tokens, cache_creation_tokens,
               output_tokens, reasoning_output_tokens, total_tokens
        FROM usage_event
        WHERE source = ?1
        ORDER BY event_at, model
        "#,
    )?;
    let rows = stmt
        .query_map([source], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn identity_rows(db_path: &Path, source: &str) -> Result<Vec<(String, String, String, i64)>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT source_path_hash, event_at, model, total_tokens
        FROM usage_event
        WHERE source = ?1
        ORDER BY 1, 2, 3, 4
        "#,
    )?;
    let rows = stmt
        .query_map([source], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn kimi_event_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'kimi_code'",
        [],
        |row| row.get(0),
    )?)
}

/// One stored kimi row as `(model, input, cache_read, cache_creation, output, total)`.
type KimiEventRow = (String, i64, i64, i64, i64, i64);

/// Returns every stored `kimi_code` event ordered by event time then model.
fn kimi_event_rows(db_path: &Path) -> Result<Vec<KimiEventRow>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT model, input_tokens, cache_read_tokens, cache_creation_tokens,
               output_tokens, total_tokens
        FROM usage_event
        WHERE source = 'kimi_code'
        ORDER BY event_at, model
        "#,
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Projects the Kimi Code passive source status through the same entry point the
/// `source-status` command uses (`passive_no_data` vs `passive_ready`).
fn kimi_capability_status(app: &AppContext, store: &Store) -> Result<String> {
    source_capability_status(app, store, SourceKind::KimiCode)
}

fn source_capability_status(app: &AppContext, store: &Store, source: SourceKind) -> Result<String> {
    let sources = Dashboard::open(store)?.source_breakdown(&Default::default())?;
    let _ = app;
    let status = llmusage::commands::source_status::build_source_capability_statuses(&sources)
        .into_iter()
        .find(|status| status.source == source)
        .expect("source capability status present");
    Ok(status.status.to_string())
}

/// One synthetic ZCode `model_usage` row used by the zcode fixture helpers.
#[derive(Debug, Clone)]
struct ZcodeRowFixture {
    id: &'static str,
    session_id: &'static str,
    model_id: &'static str,
    status: &'static str,
    started_at: i64,
    completed_at: i64,
    input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    provider_total_tokens: Option<i64>,
    computed_total_tokens: Option<i64>,
    error_type: Option<&'static str>,
}

impl Default for ZcodeRowFixture {
    fn default() -> Self {
        Self {
            id: "usage-row",
            session_id: "sess-1",
            model_id: "GLM-5.3",
            status: "completed",
            started_at: 1_780_000_000_000,
            completed_at: 1_780_000_001_000,
            input_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            provider_total_tokens: None,
            computed_total_tokens: None,
            error_type: None,
        }
    }
}

fn zcode_row(id: &'static str, completed_at: i64, input: i64, output: i64) -> ZcodeRowFixture {
    ZcodeRowFixture {
        id,
        completed_at,
        input_tokens: input,
        output_tokens: output,
        computed_total_tokens: Some(input + output),
        ..ZcodeRowFixture::default()
    }
}

fn zcode_source_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'zcode'",
        [],
        |row| row.get(0),
    )?)
}

// ============================================================================
// Antigravity 合成 wire 编码（protobuf varint / len-delimited，全脱敏）
// ============================================================================

fn ag_varint(value: u64, out: &mut Vec<u8>) {
    let mut value = value;
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn ag_varint_field(field_no: u32, value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    ag_varint(u64::from(field_no) << 3, &mut out);
    ag_varint(value, &mut out);
    out
}

fn ag_bytes_field(field_no: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    ag_varint((u64::from(field_no) << 3) | 2, &mut out);
    ag_varint(payload.len() as u64, &mut out);
    out.extend_from_slice(payload);
    out
}

fn ag_string_field(field_no: u32, value: &str) -> Vec<u8> {
    ag_bytes_field(field_no, value.as_bytes())
}

/// `chatModel.#9.#4` timestamp message `{#1 秒, #2 纳秒}` wrapper.
fn ag_timestamp_message(seconds: u64, nanos: u64) -> Vec<u8> {
    let mut stamp = Vec::new();
    stamp.extend_from_slice(&ag_varint_field(1, seconds));
    stamp.extend_from_slice(&ag_varint_field(2, nanos));
    ag_bytes_field(4, &stamp)
}

/// usage 子消息（chatModel.#4）：#3 checksum 恒 = #9 + #10。
fn ag_usage_message(
    input: u64,
    output: u64,
    thinking: u64,
    cache_read: u64,
    response_id: &str,
) -> Vec<u8> {
    let mut usage = Vec::new();
    usage.extend_from_slice(&ag_varint_field(1, 1132));
    usage.extend_from_slice(&ag_varint_field(2, input));
    usage.extend_from_slice(&ag_varint_field(3, output + thinking));
    if cache_read > 0 {
        usage.extend_from_slice(&ag_varint_field(5, cache_read));
    }
    usage.extend_from_slice(&ag_varint_field(6, 24));
    usage.extend_from_slice(&ag_varint_field(9, output));
    usage.extend_from_slice(&ag_varint_field(10, thinking));
    usage.extend_from_slice(&ag_string_field(11, response_id));
    usage
}

/// 完整 gen_metadata blob：chatModel(#1) 嵌套 usage/model/label/timestamp +
/// 顶层 #4 干扰字段。
#[allow(clippy::too_many_arguments)]
fn ag_gen_metadata_blob(
    input: u64,
    output: u64,
    thinking: u64,
    cache_read: u64,
    response_id: &str,
    model: Option<&str>,
    label: Option<&str>,
    timestamp_seconds: u64,
) -> Vec<u8> {
    let usage = ag_usage_message(input, output, thinking, cache_read, response_id);
    let mut chat_model = Vec::new();
    chat_model.extend_from_slice(&ag_bytes_field(4, &usage));
    chat_model.extend_from_slice(&ag_bytes_field(
        9,
        &ag_timestamp_message(timestamp_seconds, 657_105_100),
    ));
    if let Some(model) = model {
        chat_model.extend_from_slice(&ag_string_field(19, model));
    }
    if let Some(label) = label {
        chat_model.extend_from_slice(&ag_string_field(21, label));
    }
    let mut blob = Vec::new();
    blob.extend_from_slice(&ag_bytes_field(1, &chat_model));
    blob.extend_from_slice(&ag_bytes_field(4, &[0u8; 36]));
    blob
}

/// `trajectory_metadata_blob` 行：#2 created-at + #1.#1 workspace URI。
fn antigravity_trajectory_blob() -> Vec<u8> {
    let mut stamp = Vec::new();
    stamp.extend_from_slice(&ag_varint_field(1, 1_785_140_245));
    stamp.extend_from_slice(&ag_varint_field(2, 657_105_100));
    let mut workspace = Vec::new();
    workspace.extend_from_slice(&ag_string_field(1, "file:///D:/Documents/demo"));
    let mut trajectory = Vec::new();
    trajectory.extend_from_slice(&ag_bytes_field(1, &workspace));
    trajectory.extend_from_slice(&ag_bytes_field(2, &stamp));
    trajectory
}

fn antigravity_event_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'antigravity'",
        [],
        |row| row.get(0),
    )?)
}

/// P0 升级路径：真实旧 key 形状的存量行（ADR-0009：迁移只改 source 不改
/// key）与新导入行共存；无界 sync / 自动 legacy 修复不删除存量行。
/// bounded run：按事件时间过滤，不推进 cursor、不 reset；窗口外历史由
/// 随后的全量 sync 恢复。
/// A request that starts before the watermark but completes after it must not
/// be missed: the watermark anchors on `completed_at`, not `started_at`.
/// A bounded `--recent-days` run may reuse the stored watermark as a lower
/// bound but must not advance it; a later full sync still recovers history
/// outside the window (source-sync-contracts).
fn dsh_event_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'deepseek_harness'",
        [],
        |row| row.get(0),
    )?)
}

fn dsh_event_keys(db_path: &Path) -> Result<Vec<String>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT event_key FROM usage_event WHERE source = 'deepseek_harness' ORDER BY event_key",
    )?;
    Ok(stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn dsh_session_line(id: &str, parent: Option<&str>, seed_length: Option<i64>) -> String {
    let mut value = serde_json::json!({
        "type": "session",
        "version": 0,
        "id": id,
        "cwd": "/tmp/demo",
    });
    if let Some(parent) = parent {
        value["parentSession"] = serde_json::json!(parent);
    }
    if let Some(seed) = seed_length {
        value["seedLength"] = serde_json::json!(seed);
    }
    value.to_string()
}

fn dsh_usage_line(seq: i64, time_ms: i64, message_id: &str, input: i64, output: i64) -> String {
    serde_json::json!({
        "type": "assistant/message",
        "seq": seq,
        "time": time_ms,
        "data": {
            "usage": {
                "inputTokens": input,
                "outputTokens": output,
                "cacheReadTokens": 0,
                "cacheWriteTokens": 0,
                "reasoningTokens": 0,
            },
            "message": {
                "id": message_id,
                "source": {
                    "kind": "model",
                    "provider": "deepseek-official",
                    "model": "deepseek-v4-flash",
                }
            }
        }
    })
    .to_string()
}

fn dsh_encode_frames(lines: &[String]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for line in lines {
        let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 0)?;
        encoder.write_all(format!("{line}\n").as_bytes())?;
        out.extend(encoder.finish()?);
    }
    Ok(out)
}

fn usage_event_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    let count = conn.query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
    Ok(count)
}

fn source_token_totals(db_path: &Path, source: SourceKind) -> Result<Vec<i64>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT total_tokens FROM usage_event WHERE source = ?1 ORDER BY event_at, event_key",
    )?;
    Ok(stmt
        .query_map([source.as_str()], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn assert_provider_label(db_path: &Path, expected: &str) -> Result<()> {
    let conn = Connection::open(db_path)?;
    let event_labels = {
        let mut stmt = conn.prepare("SELECT provider_label FROM usage_event ORDER BY event_key")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    assert_eq!(event_labels, vec![expected.to_string()]);

    let bucket_labels = {
        let mut stmt =
            conn.prepare("SELECT provider_label FROM usage_bucket_30m ORDER BY provider_label")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    assert_eq!(bucket_labels, vec![expected.to_string()]);
    Ok(())
}

fn usage_tool_call_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    let count = conn.query_row("SELECT COUNT(*) FROM usage_tool_call", [], |row| row.get(0))?;
    Ok(count)
}

fn opencode_mcp_servers(db_path: &Path) -> Result<Vec<String>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT mcp_server FROM usage_tool_call WHERE tool_kind = 'mcp' AND mcp_server IS NOT NULL ORDER BY mcp_server",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[derive(Debug)]
struct RunLogRecord {
    status: String,
    error: Option<String>,
    finished_at: Option<String>,
    duration_ms: Option<i64>,
}

fn latest_run_record(db_path: &Path, command: &str) -> Result<RunLogRecord> {
    let conn = Connection::open(db_path)?;
    let run = conn.query_row(
        r#"
        SELECT status, error, finished_at, duration_ms
        FROM run_log
        WHERE command = ?1
        ORDER BY id DESC
        LIMIT 1
        "#,
        [command],
        |row| {
            Ok(RunLogRecord {
                status: row.get(0)?,
                error: row.get(1)?,
                finished_at: row.get(2)?,
                duration_ms: row.get(3)?,
            })
        },
    )?;
    Ok(run)
}

fn assert_failed_run(run: &RunLogRecord) {
    assert_eq!(run.status, "failed");
    assert!(
        run.error
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    );
    assert!(run.finished_at.is_some());
    assert!(run.duration_ms.is_some());
}

struct Fixture {
    _root: TempDir,
    home: PathBuf,
    codex_home: PathBuf,
    ccr_root: PathBuf,
    opencode_home: PathBuf,
    env: test_env::ScopedEnv,
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = TempDir::new()?;
        let home = root.path().join("home");
        let codex_home = home.join(".codex");
        let ccr_root = home.join(".ccr");
        let opencode_home = root.path().join("opencode-home");
        fs::create_dir_all(&home)?;
        fs::create_dir_all(&codex_home)?;
        fs::create_dir_all(&ccr_root)?;
        fs::create_dir_all(&opencode_home)?;

        let env = test_env::ScopedEnv::capture(&[
            "HOME",
            "USERPROFILE",
            "CODEX_HOME",
            "CCR_ROOT",
            "OPENCODE_HOME",
            "OPENCODE_DB",
            "KIMI_CODE_HOME",
            "PI_AGENT_DIR",
            "GROK_HOME",
            "ZCODE_HOME",
            "GEMINI_CLI_HOME",
            "DSH_HOME",
        ]);
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("USERPROFILE", &home);
            std::env::set_var("CODEX_HOME", &codex_home);
            std::env::set_var("CCR_ROOT", &ccr_root);
            std::env::set_var("OPENCODE_HOME", &opencode_home);
            // Kimi Code discovery falls back to `$HOME/.kimi-code/sessions`;
            // clear any real developer override so the temp HOME is authoritative.
            std::env::remove_var("KIMI_CODE_HOME");
            // Pi discovery falls back to the two roots under the temp HOME.
            std::env::remove_var("PI_AGENT_DIR");
            // Grok discovery also falls back under the isolated temp HOME.
            std::env::remove_var("GROK_HOME");
            // ZCode / Antigravity CLI / dsh discovery fall back under the temp HOME.
            std::env::remove_var("ZCODE_HOME");
            std::env::remove_var("GEMINI_CLI_HOME");
            std::env::remove_var("DSH_HOME");
        }

        fs::create_dir_all(home.join(".claude").join("projects").join("demo"))?;
        write_git_repo(&home.join("workspace").join("demo-repo"))?;

        Ok(Self {
            _root: root,
            home,
            codex_home,
            ccr_root,
            opencode_home,
            env,
        })
    }

    fn restore_env(&self) {
        self.env.restore();
    }

    fn seed_codex(&self, name: &str, total_tokens: i64, timestamp: &str) -> Result<()> {
        let sessions_dir = self
            .codex_home
            .join("sessions")
            .join("2026")
            .join("04")
            .join("22");
        fs::create_dir_all(&sessions_dir)?;
        let repo_root = self.home.join("workspace").join("demo-repo");
        let payload = format!(
            "{}\n",
            [
                serde_json::json!({
                    "type": "session_meta",
                    "payload": {
                        "model": "gpt-5",
                        "cwd": repo_root.to_string_lossy().to_string(),
                    }
                })
                .to_string(),
                codex_token_line(timestamp, total_tokens, total_tokens),
            ]
            .join("\n")
        );
        fs::write(sessions_dir.join(name), payload)?;
        Ok(())
    }

    fn write_provider_map(&self, contents: &str) -> Result<PathBuf> {
        let path = self
            .ccr_root
            .join("analytics")
            .join("provider_activation.jsonl");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, contents)?;
        Ok(path)
    }

    fn append_codex(&self, name: &str, total_tokens: i64, timestamp: &str) -> Result<()> {
        let path = self
            .codex_home
            .join("sessions")
            .join("2026")
            .join("04")
            .join("22")
            .join(name);
        let payload = format!("{}\n", codex_token_line(timestamp, total_tokens, 153));
        fs::OpenOptions::new()
            .append(true)
            .open(path)?
            .write_all(payload.as_bytes())?;
        Ok(())
    }

    fn replace_codex(&self, name: &str, total_tokens: i64, timestamp: &str) -> Result<()> {
        let path = self
            .codex_home
            .join("sessions")
            .join("2026")
            .join("04")
            .join("22")
            .join(name);
        let repo_root = self.home.join("workspace").join("demo-repo");
        let payload = [
            serde_json::json!({
                "type": "session_meta",
                "payload": {
                    "model": "gpt-5",
                    "cwd": repo_root.to_string_lossy().to_string(),
                }
            })
            .to_string(),
            codex_token_line(timestamp, total_tokens, total_tokens),
        ]
        .join("\n");
        fs::write(path, payload)?;
        Ok(())
    }

    fn remove_codex(&self, name: &str) -> Result<()> {
        let path = self
            .codex_home
            .join("sessions")
            .join("2026")
            .join("04")
            .join("22")
            .join(name);
        fs::remove_file(path)?;
        Ok(())
    }

    /// Absolute path of a Kimi Code `wire.jsonl` under the given sessions root,
    /// mirroring the real `sessions/WORKSPACE/SESSION/agents/AGENT` layout.
    fn kimi_wire_path(root: &Path, session: &str) -> PathBuf {
        root.join("sessions")
            .join("workspace-1")
            .join(session)
            .join("agents")
            .join("main")
            .join("wire.jsonl")
    }

    /// Seeds a synthetic `wire.jsonl` under the default `$HOME/.kimi-code` root
    /// (discovered via the parser's home fallback), overwriting any prior file.
    fn seed_kimi_code(&self, session: &str, lines: &[String]) -> Result<PathBuf> {
        self.seed_kimi_code_under(&self.home.join(".kimi-code"), session, lines)
    }

    /// Seeds a synthetic `wire.jsonl` under an explicit sessions root, used to
    /// exercise the `KIMI_CODE_HOME` override path.
    fn seed_kimi_code_under(
        &self,
        root: &Path,
        session: &str,
        lines: &[String],
    ) -> Result<PathBuf> {
        let path = Self::kimi_wire_path(root, session);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, format!("{}\n", lines.join("\n")))?;
        Ok(path)
    }

    /// Appends one raw JSONL line to an existing default-root `wire.jsonl`.
    fn append_kimi_code(&self, session: &str, line: &str) -> Result<()> {
        let path = Self::kimi_wire_path(&self.home.join(".kimi-code"), session);
        fs::OpenOptions::new()
            .append(true)
            .open(path)?
            .write_all(format!("{line}\n").as_bytes())?;
        Ok(())
    }

    fn grok_session_dir(&self, session: &str) -> PathBuf {
        self.home
            .join(".grok")
            .join("sessions")
            .join("D%3A%5Cwork%5Cdemo")
            .join(session)
    }

    fn seed_grok(
        &self,
        session: &str,
        updates: &str,
        summary: Option<&str>,
        signals: Option<&str>,
    ) -> Result<PathBuf> {
        self.seed_grok_under(&self.home.join(".grok"), session, updates, summary, signals)
    }

    fn seed_grok_under(
        &self,
        root: &Path,
        session: &str,
        updates: &str,
        summary: Option<&str>,
        signals: Option<&str>,
    ) -> Result<PathBuf> {
        let session_dir = root
            .join("sessions")
            .join("D%3A%5Cwork%5Cdemo")
            .join(session);
        fs::create_dir_all(&session_dir)?;
        fs::write(session_dir.join("updates.jsonl"), updates)?;
        if let Some(summary) = summary {
            fs::write(session_dir.join("summary.json"), summary)?;
        }
        if let Some(signals) = signals {
            fs::write(session_dir.join("signals.json"), signals)?;
        }
        Ok(session_dir)
    }

    fn write_grok_sidecar(&self, session: &str, name: &str, content: &str) -> Result<()> {
        fs::write(self.grok_session_dir(session).join(name), content)?;
        Ok(())
    }

    fn append_grok_updates(&self, session: &str, content: &str) -> Result<()> {
        fs::OpenOptions::new()
            .append(true)
            .open(self.grok_session_dir(session).join("updates.jsonl"))?
            .write_all(content.as_bytes())?;
        Ok(())
    }

    fn pi_session_path(root: &Path, project: &str, session: &str) -> PathBuf {
        root.join(project).join(format!("agent_{session}.jsonl"))
    }

    fn seed_pi(&self, project: &str, session: &str, lines: &[String]) -> Result<PathBuf> {
        self.seed_pi_under(
            &self.home.join(".pi").join("agent").join("sessions"),
            project,
            session,
            lines,
        )
    }

    fn seed_omp(&self, project: &str, session: &str, lines: &[String]) -> Result<PathBuf> {
        self.seed_pi_under(
            &self.home.join(".omp").join("agent").join("sessions"),
            project,
            session,
            lines,
        )
    }

    fn seed_omp_relative(&self, relative: impl AsRef<Path>, lines: &[String]) -> Result<PathBuf> {
        let path = self
            .home
            .join(".omp")
            .join("agent")
            .join("sessions")
            .join(relative);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, format!("{}\n", lines.join("\n")))?;
        Ok(path)
    }

    fn seed_pi_under(
        &self,
        root: &Path,
        project: &str,
        session: &str,
        lines: &[String],
    ) -> Result<PathBuf> {
        let path = Self::pi_session_path(root, project, session);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, format!("{}\n", lines.join("\n")))?;
        Ok(path)
    }

    fn append_omp(&self, project: &str, session: &str, line: &str) -> Result<()> {
        let root = self.home.join(".omp").join("agent").join("sessions");
        let path = Self::pi_session_path(&root, project, session);
        fs::OpenOptions::new()
            .append(true)
            .open(path)?
            .write_all(format!("{line}\n").as_bytes())?;
        Ok(())
    }

    fn seed_claude(&self, name: &str, total_tokens: i64, timestamp: &str) -> Result<()> {
        self.seed_claude_lines("demo", name, &[claude_usage_line(timestamp, total_tokens)])?;
        Ok(())
    }

    fn seed_claude_lines(&self, project: &str, name: &str, lines: &[String]) -> Result<PathBuf> {
        let claude_file = self
            .home
            .join(".claude")
            .join("projects")
            .join(project)
            .join(name);
        if let Some(parent) = claude_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&claude_file, format!("{}\n", lines.join("\n")))?;
        Ok(claude_file)
    }

    fn append_claude_line(&self, project: &str, name: &str, line: &str) -> Result<()> {
        let claude_file = self
            .home
            .join(".claude")
            .join("projects")
            .join(project)
            .join(name);
        fs::OpenOptions::new()
            .append(true)
            .open(claude_file)?
            .write_all(format!("{line}\n").as_bytes())?;
        Ok(())
    }

    fn append_claude(&self, name: &str, total_tokens: i64, timestamp: &str) -> Result<()> {
        let claude_file = self
            .home
            .join(".claude")
            .join("projects")
            .join("demo")
            .join(name);
        let payload = format!("{}\n", claude_usage_line(timestamp, total_tokens));
        fs::OpenOptions::new()
            .append(true)
            .open(claude_file)?
            .write_all(payload.as_bytes())?;
        Ok(())
    }

    fn seed_opencode(&self, message_id: &str, time_created: i64, total_tokens: i64) -> Result<()> {
        let db_path = self.opencode_home.join("opencode.db");
        self.seed_opencode_at(&db_path, message_id, time_created, total_tokens)
    }

    fn seed_opencode_at(
        &self,
        db_path: &Path,
        message_id: &str,
        time_created: i64,
        total_tokens: i64,
    ) -> Result<()> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS project(id TEXT PRIMARY KEY, worktree TEXT);
            CREATE TABLE IF NOT EXISTS session(id TEXT PRIMARY KEY, project_id TEXT);
            CREATE TABLE IF NOT EXISTS message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
            "#,
        )?;
        let repo_root = self.home.join("workspace").join("demo-repo");
        conn.execute(
            "INSERT OR IGNORE INTO project(id, worktree) VALUES (?1, ?2)",
            (&"project-1", &repo_root.to_string_lossy().to_string()),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO session(id, project_id) VALUES (?1, ?2)",
            (&"session-1", &"project-1"),
        )?;
        let message = serde_json::json!({
            "id": message_id,
            "role": "assistant",
            "modelID": "gpt-5",
            "tokens": {
                "input": total_tokens,
                "output": 0,
                "reasoning": 0,
                "cache": { "read": 0, "write": 0 }
            },
            "time": {
                "created": time_created,
                "completed": time_created
            }
        });
        conn.execute(
            "INSERT OR REPLACE INTO message(id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (&message_id, &"session-1", &time_created, &message.to_string()),
        )?;
        Ok(())
    }

    fn seed_broken_opencode_schema(&self) -> Result<()> {
        let db_path = self.opencode_home.join("opencode.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("CREATE TABLE broken(id INTEGER PRIMARY KEY);")?;
        Ok(())
    }

    fn replace_opencode_db(
        &self,
        message_id: &str,
        time_created: i64,
        total_tokens: i64,
    ) -> Result<()> {
        let db_path = self.opencode_home.join("opencode.db");
        if db_path.exists() {
            fs::remove_file(&db_path)?;
        }
        self.seed_opencode(message_id, time_created, total_tokens)
    }

    fn seed_opencode_tool_part(
        &self,
        part_id: &str,
        message_id: &str,
        session_id: &str,
        time_created: i64,
        data: serde_json::Value,
    ) -> Result<()> {
        let db_path = self.opencode_home.join("opencode.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS part(id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO part(id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (&part_id, &message_id, &session_id, &time_created, &data.to_string()),
        )?;
        Ok(())
    }

    /// Path of the synthetic ZCode usage DB under the fixture HOME.
    fn zcode_db_path(&self) -> PathBuf {
        self.home
            .join(".zcode")
            .join("cli")
            .join("db")
            .join("db.sqlite")
    }

    /// Creates or reopens the synthetic ZCode `model_usage` database.
    fn zcode_connection(&self) -> Result<Connection> {
        let db_path = self.zcode_db_path();
        fs::create_dir_all(db_path.parent().unwrap())?;
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS session(
                id TEXT PRIMARY KEY,
                directory TEXT,
                path TEXT
            );
            CREATE TABLE IF NOT EXISTS model_usage(
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
                provider_total_tokens INTEGER,
                computed_total_tokens INTEGER,
                error_type TEXT
            );
            INSERT OR IGNORE INTO session(id, directory, path)
            VALUES ('sess-1', '', '');
            "#,
        )?;
        Ok(conn)
    }

    /// Inserts one synthetic completed `model_usage` row.
    fn insert_zcode_row(&self, row: ZcodeRowFixture) -> Result<()> {
        let conn = self.zcode_connection()?;
        conn.execute(
            "INSERT OR REPLACE INTO model_usage(
                id, session_id, model_id, status, started_at, completed_at,
                input_tokens, output_tokens, reasoning_tokens,
                cache_creation_input_tokens, cache_read_input_tokens,
                provider_total_tokens, computed_total_tokens, error_type
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                row.id,
                row.session_id,
                row.model_id,
                row.status,
                row.started_at,
                row.completed_at,
                row.input_tokens,
                row.output_tokens,
                row.reasoning_tokens,
                row.cache_creation_tokens,
                row.cache_read_tokens,
                row.provider_total_tokens,
                row.computed_total_tokens,
                row.error_type,
            ],
        )?;
        Ok(())
    }

    /// Rebuilds the synthetic ZCode DB from scratch, simulating database
    /// replacement (fresh ids, no anchor rows).
    fn rebuild_zcode_db(&self, rows: &[ZcodeRowFixture]) -> Result<()> {
        let db_path = self.zcode_db_path();
        if db_path.exists() {
            fs::remove_file(&db_path)?;
        }
        for row in rows {
            self.insert_zcode_row(row.clone())?;
        }
        Ok(())
    }

    /// Root of the synthetic Antigravity CLI conversations directory.
    fn antigravity_conversations_root(&self) -> PathBuf {
        self.home
            .join(".gemini")
            .join("antigravity-cli")
            .join("conversations")
    }

    /// Writes one synthetic Antigravity conversation DB carrying the given
    /// `gen_metadata` blobs (fully synthesized wire bytes, no prompt text).
    /// An existing file is replaced (rewrite semantics).
    fn seed_antigravity(&self, uuid: &str, blobs: &[(i64, Vec<u8>)]) -> Result<PathBuf> {
        let path = self
            .antigravity_conversations_root()
            .join(format!("{uuid}.db"));
        fs::create_dir_all(path.parent().unwrap())?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE gen_metadata(idx INTEGER PRIMARY KEY, data BLOB, size INTEGER);
            CREATE TABLE trajectory_metadata_blob(id TEXT, data BLOB);
            "#,
        )?;
        for (idx, blob) in blobs {
            conn.execute(
                "INSERT INTO gen_metadata(idx, data, size) VALUES (?1, ?2, ?3)",
                rusqlite::params![idx, blob, blob.len() as i64],
            )?;
        }
        let trajectory = antigravity_trajectory_blob();
        conn.execute(
            "INSERT INTO trajectory_metadata_blob(id, data) VALUES ('traj', ?1)",
            rusqlite::params![&trajectory],
        )?;
        drop(conn);
        Ok(path)
    }

    fn dsh_session_dir(root: &Path, session: &str) -> PathBuf {
        root.join("sessions").join("--tmp-demo--").join(session)
    }

    fn seed_dsh(&self, session: &str, lines: &[String]) -> Result<PathBuf> {
        self.seed_dsh_under(&self.home.join(".dsh"), session, lines)
    }

    fn seed_dsh_under(&self, root: &Path, session: &str, lines: &[String]) -> Result<PathBuf> {
        let path = Self::dsh_session_dir(root, session).join("session.jsonl");
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, format!("{}\n", lines.join("\n")))?;
        Ok(path)
    }

    fn seed_dsh_zstd(&self, session: &str, lines: &[String]) -> Result<PathBuf> {
        let path =
            Self::dsh_session_dir(&self.home.join(".dsh"), session).join("session.jsonl.zstd");
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, dsh_encode_frames(lines)?)?;
        Ok(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.restore_env();
    }
}

fn write_git_repo(repo_root: &Path) -> Result<()> {
    write_git_repo_with_url(repo_root, "https://github.com/example/demo-repo.git")
}

fn write_git_repo_with_url(repo_root: &Path, url: &str) -> Result<()> {
    fs::create_dir_all(repo_root.join(".git"))?;
    fs::write(
        repo_root.join(".git").join("config"),
        format!("[remote \"origin\"]\n    url = {url}\n"),
    )?;
    Ok(())
}

fn codex_token_line(timestamp: &str, last_total: i64, total_total: i64) -> String {
    serde_json::json!({
        "timestamp": timestamp,
        "payload": {
            "type": "token_count",
            "info": {
                "last_token_usage": {
                    "input_tokens": last_total,
                    "cached_input_tokens": 0,
                    "output_tokens": 0,
                    "reasoning_output_tokens": 0,
                    "total_tokens": last_total,
                },
                "total_token_usage": {
                    "input_tokens": total_total,
                    "cached_input_tokens": 0,
                    "output_tokens": 0,
                    "reasoning_output_tokens": 0,
                    "total_tokens": total_total,
                }
            }
        }
    })
    .to_string()
}

/// Builds one turn-scoped Kimi Code `usage.record` line with synthetic tokens.
/// `time` is epoch milliseconds, matching the real wire format.
fn kimi_turn_line(
    model: &str,
    input_other: i64,
    output: i64,
    input_cache_read: i64,
    input_cache_creation: i64,
    time_ms: i64,
) -> String {
    serde_json::json!({
        "type": "usage.record",
        "model": model,
        "usage": {
            "inputOther": input_other,
            "output": output,
            "inputCacheRead": input_cache_read,
            "inputCacheCreation": input_cache_creation,
        },
        "usageScope": "turn",
        "time": time_ms,
    })
    .to_string()
}

fn pi_title_line() -> String {
    r#"{"type":"title","title":"Demo"}"#.to_string()
}

fn pi_session_line(id: &str, cwd: Option<&str>) -> String {
    let mut value = serde_json::json!({
        "type": "session",
        "id": id,
    });
    if let Some(cwd) = cwd {
        value["cwd"] = serde_json::json!(cwd);
    }
    value.to_string()
}

fn pi_tool_call(name: &str, arguments: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": "toolCall",
        "id": format!("call-{name}"),
        "name": name,
        "arguments": arguments,
    })
}

fn pi_tool_result_line(content: &str) -> String {
    serde_json::json!({
        "type": "message",
        "timestamp": "2026-08-20T00:00:30Z",
        "message": {
            "role": "toolResult",
            "toolCallId": "call-bash",
            "toolName": "bash",
            "content": content,
            "isError": false,
        }
    })
    .to_string()
}

fn pi_assistant_behavior_line(
    timestamp: &str,
    model: &str,
    provider: Option<&str>,
    content: Vec<serde_json::Value>,
    retry_attempt: Option<i64>,
) -> String {
    let mut message = serde_json::json!({
        "role": "assistant",
        "model": model,
        "usage": {
            "input": 10,
            "output": 5,
            "cacheRead": 0,
            "cacheWrite": 0,
            "totalTokens": 15,
            "reasoningTokens": 0,
        },
        "content": content,
    });
    if let Some(provider) = provider {
        message["provider"] = serde_json::json!(provider);
    }
    if let Some(attempt) = retry_attempt {
        message["retryRecovery"] = serde_json::json!({
            "kind": "auto-retry",
            "status": "recovered",
            "attempt": attempt,
            "recovery": "plain",
        });
    }
    serde_json::json!({
        "type": "message",
        "timestamp": timestamp,
        "message": message,
    })
    .to_string()
}

fn pi_assistant_line(
    timestamp: &str,
    model: &str,
    provider: Option<&str>,
    input: i64,
    output: i64,
) -> String {
    let mut message = serde_json::json!({
        "role": "assistant",
        "model": model,
        "usage": {
            "input": input,
            "output": output,
            "cacheRead": 0,
            "cacheWrite": 0,
            "totalTokens": input + output,
            "reasoningTokens": 0,
        }
    });
    if let Some(provider) = provider {
        message["provider"] = serde_json::json!(provider);
    }
    serde_json::json!({
        "type": "message",
        "timestamp": timestamp,
        "message": message,
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)]
fn pi_message_line(
    timestamp: &str,
    model: &str,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    total: i64,
    reasoning: i64,
) -> String {
    pi_message_line_with_cost(
        timestamp,
        model,
        input,
        output,
        cache_read,
        cache_write,
        total,
        reasoning,
        serde_json::Value::Null,
    )
}

#[allow(clippy::too_many_arguments)]
fn pi_message_line_with_cost(
    timestamp: &str,
    model: &str,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    total: i64,
    reasoning: i64,
    cost: serde_json::Value,
) -> String {
    let mut usage = serde_json::json!({
        "input": input,
        "output": output,
        "cacheRead": cache_read,
        "cacheWrite": cache_write,
        "totalTokens": total,
        "reasoningTokens": reasoning,
    });
    if !cost.is_null() {
        usage["cost"] = cost;
    }
    serde_json::json!({
        "type": "message",
        "timestamp": timestamp,
        "message": {
            "role": "assistant",
            "model": model,
            "usage": usage,
        }
    })
    .to_string()
}

fn claude_usage_line(timestamp: &str, total_tokens: i64) -> String {
    serde_json::json!({
        "timestamp": timestamp,
        "message": {
            "model": "claude-sonnet-4",
            "usage": {
                "input_tokens": total_tokens,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "output_tokens": 0,
                "total_tokens": total_tokens,
            }
        }
    })
    .to_string()
}

fn claude_logical_usage_line(
    message_id: &str,
    request_id: &str,
    is_sidechain: bool,
    total_tokens: i64,
    timestamp: &str,
) -> String {
    serde_json::json!({
        "timestamp": timestamp,
        "sessionId": "session-claude",
        "requestId": request_id,
        "isSidechain": is_sidechain,
        "message": {
            "id": message_id,
            "model": "claude-sonnet-4",
            "usage": {
                "input_tokens": total_tokens,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "output_tokens": 0,
                "total_tokens": total_tokens,
            }
        }
    })
    .to_string()
}

mod accounting;
mod jobs;
mod lifecycle;
mod progress_io;
mod runtime;
mod sources;
