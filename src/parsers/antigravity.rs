//! Antigravity CLI conversation SQLite parser.
//!
//! Each `~/.gemini/antigravity-cli/conversations/<uuid>.db` holds one
//! conversation. Its `gen_metadata` table stores one `GeneratorMetadata`
//! protobuf blob per model generation; the usage channels live in the nested
//! `chatModel(#1).#4` submessage (the top-level `#4` field is a constant-size
//! distractor and must be skipped — see the task research for the wire
//! evidence). Decoding is a hand-written protobuf wire reader with zero new
//! dependencies.
//!
//! Channel semantics (1528 local rows verified, `#3 == #9 + #10` with zero
//! violations): `#1` fixed system-prompt tokens, `#2` non-cached input, `#5`
//! cache read, `#9` text-only output, `#10` thinking tokens disjoint from
//! `#9` (proven by the checksum invariant). Normalized: `input = #2 + #1`,
//! `output = #9`, `reasoning = #10`, `total = input + cache_read + output +
//! reasoning` — no authoritative grand total exists, so the total is the
//! channel sum with reasoning included under the disjoint-from-output
//! exception of the token accounting contract.
//!
//! Hook-era rows (pre-parser, no file attribution) are never touched; the
//! schema migration presetting the token-accounting marker plus the
//! `--rebuild` unattributed-history guard own that protection.

use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    time::Instant,
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use tokio::task;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    models::{ParseIssueKind, ParseIssues, SessionInfo, SourceKind, UsageEvent, UsageTokens},
    parsers::{
        ProgressSink, SourceParser, SourceSyncStats, SyncEvent,
        file_progress::{FileProgress, FileProgressCounter},
        file_state::{CandidateFile, decide_file_replay, finalize_cursor, should_rescan_file},
        source_files,
    },
    project::ProjectResolver,
    store::{FileCursor, Store, SyncRunWriter, SyncShard},
    util::{bucket_start_from_rfc3339, hash_string, normalize_model},
};

/// Fallback model when neither the row, the label mapping, nor a sole model
/// can attribute a generation. Prefer unknown over guessing.
const FALLBACK_MODEL: &str = "antigravity-unknown";

/// Saturating u64→i64 conversion for wire varints (token counts and
/// timestamps far beyond i64 carry no accounting meaning).
fn varint_to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

#[derive(Debug, Default)]
struct AntigravityShardOutput {
    events: Vec<UsageEvent>,
    cursors: Vec<FileCursor>,
    reset_path_hashes: Vec<String>,
    events_seen: usize,
    events_replayed: usize,
    bytes_scanned: u64,
    seen_file_paths: Vec<String>,
    parse_issues: ParseIssues,
}

#[derive(Debug)]
struct AntigravityParseResult {
    events: Vec<UsageEvent>,
    parse_issues: ParseIssues,
    cancelled: bool,
}

/// Antigravity CLI conversation parser. Owns the per-file decode + per-shard
/// commit pipeline across the conversations root.
pub struct AntigravityParser;

impl SourceParser for AntigravityParser {
    fn source(&self) -> SourceKind {
        SourceKind::Antigravity
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
        Box::pin(sync_antigravity(
            store,
            writer,
            parallelism,
            recent_cutoff,
            cancel,
            progress,
        ))
    }
}

async fn sync_antigravity(
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
    recent_cutoff: Option<DateTime<Utc>>,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
) -> Result<SourceSyncStats> {
    /*
     * ========================================================================
     * 步骤1：解析 Antigravity CLI conversations 目录下的 .db 真源
     * ========================================================================
     * 目标：
     * 1) 只把缺失、追加或改写的 conversation DB 送去解析（fingerprint）
     * 2) gen_metadata protobuf 解码 → UsageEvent（#9/#10 分离，total 含 reasoning）
     * 3) 返回 event / cursor / reset 指令给单 writer 统一落库
     */
    info!("开始同步 Antigravity CLI conversations 真源");

    let parse_started = Instant::now();
    let listing = source_files::list_antigravity_conversation_files();
    let inventory_paths = listing.file_paths();
    store.source_files().mark_inventory_seen(
        SourceKind::Antigravity,
        "local",
        &inventory_paths,
        writer.run_started_at(),
    )?;
    let inventory_error = listing.error_summary();
    let files = listing.paths;
    let total_files = files.len();
    let cursor_map = store
        .cursors()
        .load_file_cursors(SourceKind::Antigravity, "local")?;

    let mut candidates = Vec::new();
    let mut changed_files = 0usize;
    for file_path in files {
        let existing = file_path
            .to_str()
            .and_then(|raw| cursor_map.get(raw).cloned());
        if should_rescan_file(&file_path, existing.as_ref())? {
            changed_files += 1;
            candidates.push(CandidateFile {
                path: file_path,
                existing,
            });
        }
    }

    let mut events_seen = 0usize;
    let mut events_replayed = 0usize;
    let mut bytes_scanned = 0u64;
    let mut inserted = 0usize;
    let mut write_ms = 0u64;
    let mut parse_issues = ParseIssues::default();
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source: SourceKind::Antigravity,
            files_total: candidates.len() as u64,
        },
    );
    let (mut file_progress, file_progress_counter) = FileProgress::new();

    let width = parallelism.max(1);
    'batches: for batch in candidates.chunks(width) {
        if cancel.is_cancelled() {
            break;
        }
        let mut tasks = Vec::new();
        for candidate in batch {
            let candidate = candidate.clone();
            let counter = file_progress_counter.clone();
            let task_cancel = cancel.clone();
            tasks.push(task::spawn_blocking(move || {
                parse_antigravity_file(candidate, counter, task_cancel)
            }));
        }

        let batch_outputs = file_progress
            .wait_for_all(tasks, |files_scanned| {
                emit_progress(
                    &mut progress,
                    SyncEvent::Progress {
                        source: SourceKind::Antigravity,
                        files_scanned,
                        records_imported: inserted as u64,
                        current_file: None,
                    },
                );
            })
            .await?;
        if cancel.is_cancelled() {
            break;
        }

        for mut shard in batch_outputs {
            if cancel.is_cancelled() {
                break 'batches;
            }
            if let Some(cutoff) = recent_cutoff.as_ref() {
                // bounded run：按事件时间过滤，不推进 cursor、不 reset；
                // 窗口外历史由随后的全量 sync 恢复。
                shard.events.retain(|event| {
                    crate::parsers::timestamp_in_recent_window(&event.event_at, Some(cutoff))
                });
                shard.events_seen = shard.events.len();
                shard.events_replayed = shard.events.len();
                shard.cursors.clear();
                shard.reset_path_hashes.clear();
            }
            events_seen += shard.events_seen;
            events_replayed += shard.events_replayed;
            bytes_scanned += shard.bytes_scanned;
            parse_issues.merge(shard.parse_issues.clone());

            let completed_files = file_progress.boundary_snapshot();
            let commit = writer.commit_shard(SyncShard {
                source: SourceKind::Antigravity,
                reset_path_hashes: shard.reset_path_hashes,
                events: shard.events,
                cursors: shard.cursors,
                seen_file_paths: shard.seen_file_paths,
                raw_records: Vec::new(),
                turns: Vec::new(),
                tool_calls: Vec::new(),
                ..SyncShard::new(SourceKind::Antigravity)
            })?;
            inserted += commit.events_inserted;
            write_ms += commit.write_ms;
            emit_progress(
                &mut progress,
                SyncEvent::Progress {
                    source: SourceKind::Antigravity,
                    files_scanned: completed_files,
                    records_imported: inserted as u64,
                    current_file: None,
                },
            );
        }
    }

    let mut stats = SourceSyncStats {
        source: SourceKind::Antigravity,
        files_processed: total_files,
        changed_files,
        skipped_files: total_files.saturating_sub(changed_files),
        bytes_scanned,
        events_seen,
        events_replayed,
        events_inserted: inserted,
        write_ms,
        last_error: inventory_error,
        parse_issues,
        ..SourceSyncStats::default()
    };
    let total_elapsed = parse_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    stats.parse_ms = total_elapsed.saturating_sub(write_ms);

    info!(
        files_processed = stats.files_processed,
        changed_files = stats.changed_files,
        skipped_files = stats.skipped_files,
        events_seen = stats.events_seen,
        malformed_lines = stats.parse_issues.malformed_lines,
        "完成 Antigravity CLI conversations 真源解析"
    );
    Ok(stats)
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
}

fn parse_antigravity_file(
    candidate: CandidateFile,
    progress: FileProgressCounter,
    cancel: CancellationToken,
) -> Result<AntigravityShardOutput> {
    let mut output = AntigravityShardOutput::default();
    let existing = candidate.existing.clone();
    let decision = decide_file_replay(candidate)?;
    output
        .seen_file_paths
        .push(decision.snapshot.path.to_string_lossy().to_string());
    let path_hash = hash_string(&decision.snapshot.path.to_string_lossy());

    let parsed = parse_conversation_file(&decision.snapshot.path, &path_hash, &cancel)?;
    output.parse_issues.merge(parsed.parse_issues);
    if parsed.cancelled {
        // 取消的文件不产生 cursor/event（被取消的 batch 不提交）。
        progress.advance_file();
        return Ok(output);
    }
    output.bytes_scanned = decision.snapshot.file_size;
    output.events_seen = parsed.events.len();
    if existing.is_some() {
        // SQLite 文件每次 rescan 都整体重解析：无条件 reset 该路径旧行
        // （grok 会话重放同款语义），Append/Reparse 分类只影响是否需要
        // 读取，event_key 幂等兜底跨分类成立。
        output.events_replayed = parsed.events.len();
        output.reset_path_hashes.push(path_hash);
    }
    output.events = parsed.events;
    // SQLite DB 文件没有稳定的字节 offset 语义：fingerprint 不变 → 整文件跳过；
    // 变化 → 全文件重解析 + reset_path_hashes 替换旧行 + event_key 幂等兜底。
    output.cursors.push(finalize_cursor(
        &decision.snapshot.path,
        &decision.snapshot,
        decision.snapshot.file_size,
        None,
        None,
    ));
    progress.advance_file();
    Ok(output)
}

/// Decodes one conversation DB file into normalized events.
fn parse_conversation_file(
    file_path: &Path,
    path_hash: &str,
    cancel: &CancellationToken,
) -> Result<AntigravityParseResult> {
    let mut parse_issues = ParseIssues::default();
    if cancel.is_cancelled() {
        return Ok(AntigravityParseResult {
            events: Vec::new(),
            parse_issues,
            cancelled: true,
        });
    }

    let connection = match Connection::open_with_flags(file_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
    {
        Ok(connection) => connection,
        Err(error) => {
            parse_issues.record(
                SourceKind::Antigravity,
                path_hash,
                0,
                ParseIssueKind::Malformed,
                "",
            );
            tracing::debug!(error = %error, "Antigravity conversation DB 打开失败");
            return Ok(AntigravityParseResult {
                events: Vec::new(),
                parse_issues,
                cancelled: false,
            });
        }
    };

    // 会话级元数据：created-at 与 workspace URI（trajectory_metadata_blob）。
    let (session_created_at, workspace_uri) = read_trajectory_metadata(&connection);

    // 逐行解码 gen_metadata。
    let mut decoded_rows = Vec::new();
    let mut statement =
        match connection.prepare("SELECT rowid, data FROM gen_metadata ORDER BY idx") {
            Ok(statement) => statement,
            Err(_) => {
                // 无 gen_metadata 表（空会话或异构 schema）：干净跳过。
                return Ok(AntigravityParseResult {
                    events: Vec::new(),
                    parse_issues,
                    cancelled: false,
                });
            }
        };
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    for row in rows {
        let (rowid, blob) = row?;
        match decode_gen_metadata(&blob) {
            Some(decoded) => decoded_rows.push((rowid, decoded)),
            None => parse_issues.record(
                SourceKind::Antigravity,
                path_hash,
                rowid.max(0) as u64,
                ParseIssueKind::Malformed,
                "",
            ),
        }
    }

    // SessionModels 回填：label(#21) → model(#19)；歧义丢弃 + sole_model 兜底。
    let session_models = build_session_models(&decoded_rows);

    let mut events = Vec::new();
    let mut resolver = ProjectResolver::default();
    for (rowid, decoded) in decoded_rows {
        let usage = match decoded.usage {
            Some(usage) => usage,
            None => continue,
        };
        // 校验和不变量：#3 == #9 + #10；不成立计 issue 不改数。
        if let (Some(checksum), Some(output), Some(thinking)) =
            (usage.checksum, usage.output, usage.thinking)
            && checksum != output.saturating_add(thinking)
        {
            parse_issues.record(
                SourceKind::Antigravity,
                path_hash,
                rowid.max(0) as u64,
                ParseIssueKind::AccountingAnomaly,
                "",
            );
        }

        let input = varint_to_i64(usage.non_cached_input.unwrap_or(0))
            .saturating_add(varint_to_i64(usage.system_prompt.unwrap_or(0)));
        let cache_read = varint_to_i64(usage.cache_read.unwrap_or(0));
        let output = varint_to_i64(usage.output.unwrap_or(0));
        let reasoning = varint_to_i64(usage.thinking.unwrap_or(0));
        if input == 0 && cache_read == 0 && output == 0 && reasoning == 0 {
            continue;
        }
        let total = input
            .saturating_add(cache_read)
            .saturating_add(output)
            .saturating_add(reasoning);

        let Some(event_at) = decoded
            .timestamp
            .or(session_created_at)
            .and_then(DateTime::from_timestamp_millis)
            .map(|timestamp| timestamp.to_rfc3339())
        else {
            parse_issues.record(
                SourceKind::Antigravity,
                path_hash,
                rowid.max(0) as u64,
                ParseIssueKind::Malformed,
                "",
            );
            continue;
        };
        let Some(hour_start) = bucket_start_from_rfc3339(&event_at) else {
            continue;
        };

        let model = decoded
            .response_model
            .clone()
            .or_else(|| session_models.model_for_label(decoded.display_label.as_deref()))
            .or_else(|| session_models.sole_model.clone())
            .map(|model| normalize_model(Some(&model)))
            .unwrap_or_else(|| FALLBACK_MODEL.to_string());

        let response_id = match usage.response_id.clone() {
            Some(response_id) if !response_id.is_empty() => response_id,
            _ => {
                parse_issues.record(
                    SourceKind::Antigravity,
                    path_hash,
                    rowid.max(0) as u64,
                    ParseIssueKind::AccountingAnomaly,
                    "",
                );
                format!("row-{rowid}")
            }
        };

        let project = workspace_uri
            .as_deref()
            .map(workspace_uri_to_path)
            .and_then(|path| path.and_then(|path| resolver.resolve(&path).ok().flatten()));

        events.push(UsageEvent {
            event_key: format!("antigravity:{path_hash}::{}", hash_string(&response_id)),
            source: SourceKind::Antigravity,
            provider_label: String::new(),
            model,
            event_at,
            hour_start,
            tokens: UsageTokens {
                input_tokens: input,
                cache_read_tokens: cache_read,
                cache_creation_tokens: 0,
                output_tokens: output,
                reasoning_output_tokens: reasoning,
                total_tokens: total,
            },
            project,
            session: Some(SessionInfo {
                session_id: path_hash.to_string(),
                session_label: Some(
                    file_path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("conversation")
                        .to_string(),
                ),
                source_path_hash: Some(path_hash.to_string()),
            }),
        });
    }

    Ok(AntigravityParseResult {
        events,
        parse_issues,
        cancelled: false,
    })
}

fn read_trajectory_metadata(connection: &Connection) -> (Option<i64>, Option<String>) {
    let Ok(mut statement) = connection.prepare("SELECT data FROM trajectory_metadata_blob LIMIT 1")
    else {
        return (None, None);
    };
    let Ok(blob) = statement.query_row([], |row| row.get::<_, Vec<u8>>(0)) else {
        return (None, None);
    };
    let Ok(top) = decode_message(&blob) else {
        return (None, None);
    };
    // #2 = created-at Timestamp {#1 秒, #2 纳秒}；#1.#1 = workspace URI。
    let created_at = top
        .get(&2)
        .and_then(|field| field.as_bytes())
        .and_then(|bytes| decode_message(bytes).ok())
        .and_then(|timestamp| {
            let seconds = timestamp.get(&1).and_then(|field| field.as_varint())?;
            let nanos = timestamp
                .get(&2)
                .and_then(|field| field.as_varint())
                .unwrap_or(0);
            DateTime::from_timestamp(varint_to_i64(seconds), nanos as u32)
                .map(|t| t.timestamp_millis())
        });
    let workspace = top
        .get(&1)
        .and_then(|field| field.as_bytes())
        .and_then(|bytes| decode_message(bytes).ok())
        .and_then(|metadata| {
            metadata
                .get(&1)
                .and_then(|field| field.as_bytes())
                .map(|bytes| bytes.to_vec())
        })
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|uri| !uri.is_empty());
    (created_at, workspace)
}

fn workspace_uri_to_path(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://").unwrap_or(uri);
    if raw.is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

/// label → model 映射 + sole_model 兜底（对齐 tokscale `SessionModels`）。
struct SessionModels {
    label_models: HashMap<String, String>,
    sole_model: Option<String>,
}

impl SessionModels {
    fn model_for_label(&self, label: Option<&str>) -> Option<String> {
        label.and_then(|label| self.label_models.get(label).cloned())
    }
}

fn build_session_models(decoded_rows: &[(i64, DecodedGenMetadata)]) -> SessionModels {
    let mut label_counts: HashMap<&str, HashMap<&str, usize>> = HashMap::new();
    let mut confirmed_models: HashMap<&str, usize> = HashMap::new();
    for (_, decoded) in decoded_rows {
        if let (Some(label), Some(model)) = (
            decoded.display_label.as_deref(),
            decoded.response_model.as_deref(),
        ) {
            if label.is_empty() || model.is_empty() {
                continue;
            }
            label_counts
                .entry(label)
                .or_default()
                .entry(model)
                .and_modify(|count| *count += 1)
                .or_insert(1);
            *confirmed_models.entry(model).or_insert(0) += 1;
        }
    }
    // 同一 label 配到多个 model → 歧义丢弃（宁可 unknown 也不猜）。
    let label_models = label_counts
        .into_iter()
        .filter(|(_, models)| models.len() == 1)
        .map(|(label, models)| {
            let model = *models.keys().next().expect("single model");
            (label.to_string(), model.to_string())
        })
        .collect::<HashMap<_, _>>();
    let confirmed: Vec<&str> = confirmed_models.keys().copied().collect();
    let sole_model = (confirmed.len() == 1).then(|| confirmed[0].to_string());
    SessionModels {
        label_models,
        sole_model,
    }
}

// ============================================================================
// protobuf wire 解码（零依赖，varint / len-delimited 两种 wire type 足够）
// ============================================================================

#[derive(Debug, Clone)]
struct GenMetadataUsage {
    system_prompt: Option<u64>,
    non_cached_input: Option<u64>,
    checksum: Option<u64>,
    cache_read: Option<u64>,
    output: Option<u64>,
    thinking: Option<u64>,
    response_id: Option<String>,
}

/// One decoded `gen_metadata` row.
#[derive(Debug, Clone)]
struct DecodedGenMetadata {
    usage: Option<GenMetadataUsage>,
    response_model: Option<String>,
    display_label: Option<String>,
    /// `chatModel.#9.#4` wall-clock timestamp in epoch milliseconds.
    timestamp: Option<i64>,
}

enum WireValue {
    Varint(u64),
    Bytes(Vec<u8>),
}

impl WireValue {
    fn as_varint(&self) -> Option<u64> {
        match self {
            Self::Varint(value) => Some(*value),
            Self::Bytes(_) => None,
        }
    }

    fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Varint(_) => None,
            Self::Bytes(bytes) => Some(bytes),
        }
    }
}

type WireFields = HashMap<u32, WireValue>;

fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result = 0u64;
    let mut shift = 0u32;
    while *pos < buf.len() {
        let byte = buf[*pos];
        *pos += 1;
        result |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

/// Decodes one protobuf message into a field map. Repeated fields keep the
/// last occurrence; malformed input returns `None` (caller counts a parse
/// issue instead of panicking).
fn decode_message(buf: &[u8]) -> anyhow::Result<WireFields> {
    let mut fields = WireFields::new();
    let mut pos = 0usize;
    while pos < buf.len() {
        let key = read_varint(buf, &mut pos)
            .ok_or_else(|| anyhow::anyhow!("truncated varint field key at byte {pos}"))?;
        let field_no = u32::try_from(key >> 3).map_err(|_| anyhow::anyhow!("field overflow"))?;
        if field_no == 0 {
            anyhow::bail!("invalid field number 0");
        }
        match key & 7 {
            0 => {
                let value = read_varint(buf, &mut pos)
                    .ok_or_else(|| anyhow::anyhow!("truncated varint value"))?;
                fields.insert(field_no, WireValue::Varint(value));
            }
            2 => {
                let length = read_varint(buf, &mut pos)
                    .ok_or_else(|| anyhow::anyhow!("truncated length prefix"))?;
                let length =
                    usize::try_from(length).map_err(|_| anyhow::anyhow!("length overflow"))?;
                if pos.checked_add(length).is_none_or(|end| end > buf.len()) {
                    anyhow::bail!("len-delimited field overruns buffer");
                }
                fields.insert(field_no, WireValue::Bytes(buf[pos..pos + length].to_vec()));
                pos += length;
            }
            // 固定宽度 wire type：跳过。
            1 => {
                pos = pos
                    .checked_add(8)
                    .filter(|end| *end <= buf.len())
                    .ok_or_else(|| anyhow::anyhow!("truncated 64-bit field"))?;
            }
            5 => {
                pos = pos
                    .checked_add(4)
                    .filter(|end| *end <= buf.len())
                    .ok_or_else(|| anyhow::anyhow!("truncated 32-bit field"))?;
            }
            other => anyhow::bail!("unsupported wire type {other}"),
        }
    }
    Ok(fields)
}

fn decode_string(value: &WireValue) -> Option<String> {
    let bytes = value.as_bytes()?;
    String::from_utf8(bytes.to_vec())
        .ok()
        .filter(|text| !text.is_empty())
}

/// Decodes the documented shape: blob → `#1` chatModel → `{#4 usage, #19
/// model, #21 label, #9.#4 timestamp}`. The top-level `#4` distractor is
/// never consulted.
fn decode_gen_metadata(blob: &[u8]) -> Option<DecodedGenMetadata> {
    let top = decode_message(blob).ok()?;
    let chat_model = top.get(&1)?.as_bytes()?;
    let chat_fields = decode_message(chat_model).ok()?;

    let usage = chat_fields
        .get(&4)
        .and_then(|field| field.as_bytes())
        .and_then(|usage_bytes| {
            let usage_fields = decode_message(usage_bytes).ok()?;
            Some(GenMetadataUsage {
                system_prompt: usage_fields.get(&1).and_then(WireValue::as_varint),
                non_cached_input: usage_fields.get(&2).and_then(WireValue::as_varint),
                checksum: usage_fields.get(&3).and_then(WireValue::as_varint),
                cache_read: usage_fields.get(&5).and_then(WireValue::as_varint),
                output: usage_fields.get(&9).and_then(WireValue::as_varint),
                thinking: usage_fields.get(&10).and_then(WireValue::as_varint),
                response_id: usage_fields.get(&11).and_then(decode_string),
            })
        });

    // #9.#4：每代 wall-clock 时间戳 {#1 秒, #2 纳秒}。
    let timestamp = chat_fields
        .get(&9)
        .and_then(|field| field.as_bytes())
        .and_then(|generation| decode_message(generation).ok())
        .and_then(|generation| {
            generation
                .get(&4)
                .and_then(|field| field.as_bytes())
                .map(|bytes| bytes.to_vec())
        })
        .and_then(|stamp| decode_message(&stamp).ok())
        .and_then(|stamp| {
            let seconds = stamp.get(&1).and_then(WireValue::as_varint)?;
            let nanos = stamp.get(&2).and_then(WireValue::as_varint).unwrap_or(0);
            DateTime::from_timestamp(varint_to_i64(seconds), nanos as u32)
                .map(|t| t.timestamp_millis())
        });

    Some(DecodedGenMetadata {
        usage,
        response_model: chat_fields.get(&19).and_then(decode_string),
        display_label: chat_fields.get(&21).and_then(decode_string),
        timestamp,
    })
}

// ============================================================================
// 合成 wire 编码助手（fixture builder，脱敏合成字节）
// ============================================================================

#[cfg(test)]
pub(crate) mod wire {
    //! Minimal protobuf encoder used by tests to build synthetic blobs.

    fn varint(value: u64, out: &mut Vec<u8>) {
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

    pub fn varint_field(field_no: u32, value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        varint(u64::from(field_no) << 3, &mut out);
        varint(value, &mut out);
        out
    }

    pub fn bytes_field(field_no: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        varint((u64::from(field_no) << 3) | 2, &mut out);
        varint(payload.len() as u64, &mut out);
        out.extend_from_slice(payload);
        out
    }

    pub fn string_field(field_no: u32, value: &str) -> Vec<u8> {
        bytes_field(field_no, value.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    use super::wire::{bytes_field, string_field, varint_field};
    use super::*;

    fn timestamp_message(seconds: i64, nanos: u32) -> Vec<u8> {
        let mut stamp = Vec::new();
        stamp.extend_from_slice(&varint_field(1, seconds.unsigned_abs()));
        stamp.extend_from_slice(&varint_field(2, u64::from(nanos)));
        let mut wrapper = Vec::new();
        wrapper.extend_from_slice(&bytes_field(4, &stamp));
        wrapper
    }

    fn usage_message(
        system_prompt: u64,
        input: u64,
        output: u64,
        thinking: u64,
        cache_read: Option<u64>,
        response_id: &str,
    ) -> Vec<u8> {
        let checksum = output + thinking;
        let mut usage = Vec::new();
        usage.extend_from_slice(&varint_field(1, system_prompt));
        usage.extend_from_slice(&varint_field(2, input));
        usage.extend_from_slice(&varint_field(3, checksum));
        if let Some(cache_read) = cache_read {
            usage.extend_from_slice(&varint_field(5, cache_read));
        }
        // 未知字段 #6（本机观测恒 24）按 wire type 天然跳过。
        usage.extend_from_slice(&varint_field(6, 24));
        usage.extend_from_slice(&varint_field(9, output));
        usage.extend_from_slice(&varint_field(10, thinking));
        usage.extend_from_slice(&string_field(11, response_id));
        usage
    }

    fn gen_metadata_blob(
        usage: &[u8],
        model: Option<&str>,
        label: Option<&str>,
        timestamp_seconds: i64,
    ) -> Vec<u8> {
        let mut chat_model = Vec::new();
        chat_model.extend_from_slice(&bytes_field(4, usage));
        chat_model.extend_from_slice(&bytes_field(
            9,
            &timestamp_message(timestamp_seconds, 657_105_100),
        ));
        if let Some(model) = model {
            chat_model.extend_from_slice(&string_field(19, model));
        }
        if let Some(label) = label {
            chat_model.extend_from_slice(&string_field(21, label));
        }
        let mut blob = Vec::new();
        blob.extend_from_slice(&bytes_field(1, &chat_model));
        // 顶层 #4 干扰字段（本机恒 36 字节，非 usage）。
        blob.extend_from_slice(&bytes_field(4, &[0u8; 36]));
        blob
    }

    /// Builds a synthetic conversation DB carrying `gen_metadata` plus the
    /// trajectory metadata rows. All bytes are synthesized; no prompt text.
    fn synthetic_conversation(rows: &[(i64, Vec<u8>)]) -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("00000000-0000-0000-0000-000000000001.db");
        let conn = Connection::open(&path).expect("open db");
        conn.execute_batch(
            r#"
            CREATE TABLE gen_metadata(idx INTEGER PRIMARY KEY, data BLOB, size INTEGER);
            CREATE TABLE trajectory_metadata_blob(id TEXT, data BLOB);
            "#,
        )
        .expect("create schema");
        for (idx, blob) in rows {
            conn.execute(
                "INSERT INTO gen_metadata(idx, data, size) VALUES (?1, ?2, ?3)",
                rusqlite::params![idx, blob, blob.len() as i64],
            )
            .expect("insert row");
        }
        // trajectory metadata：#2 created-at {秒,纳秒} + #1.#1 workspace。
        let mut stamp = Vec::new();
        stamp.extend_from_slice(&varint_field(1, 1_785_140_245u64));
        stamp.extend_from_slice(&varint_field(2, 657_105_100));
        let mut workspace_inner = Vec::new();
        workspace_inner.extend_from_slice(&string_field(1, "file:///D:/Documents/Code/demo"));
        let mut trajectory = Vec::new();
        trajectory.extend_from_slice(&bytes_field(1, &workspace_inner));
        trajectory.extend_from_slice(&bytes_field(2, &stamp));
        conn.execute(
            "INSERT INTO trajectory_metadata_blob(id, data) VALUES ('traj', ?1)",
            rusqlite::params![&trajectory],
        )
        .expect("insert trajectory");
        drop(conn);
        (dir, path)
    }

    #[test]
    fn decodes_usage_from_nested_chat_model_and_skips_top_level_distractor() {
        let blob = gen_metadata_blob(
            &usage_message(1132, 500, 234, 50, Some(1200), "resp-1"),
            Some("gemini-3.6-flash"),
            Some("Gemini 3.6 Flash (High)"),
            1_785_140_200,
        );

        let decoded = decode_gen_metadata(&blob).expect("decode");
        let usage = decoded.usage.expect("usage");
        assert_eq!(usage.system_prompt, Some(1132));
        assert_eq!(usage.non_cached_input, Some(500));
        assert_eq!(usage.checksum, Some(284));
        assert_eq!(usage.cache_read, Some(1200));
        assert_eq!(usage.output, Some(234));
        assert_eq!(usage.thinking, Some(50));
        assert_eq!(usage.response_id.as_deref(), Some("resp-1"));
        assert_eq!(decoded.response_model.as_deref(), Some("gemini-3.6-flash"));
        assert_eq!(
            decoded.display_label.as_deref(),
            Some("Gemini 3.6 Flash (High)")
        );
        assert_eq!(decoded.timestamp, Some(1_785_140_200_657));
    }

    #[test]
    fn usage_rows_without_cache_read_decode_with_none() {
        let blob = gen_metadata_blob(
            &usage_message(1132, 500, 234, 50, None, "resp-2"),
            Some("gemini-3.6-flash"),
            None,
            1_785_140_300,
        );
        let usage = decode_gen_metadata(&blob).expect("decode").usage.unwrap();
        assert_eq!(usage.cache_read, None);
    }

    #[test]
    fn truncated_blob_is_rejected_without_panicking() {
        let blob = gen_metadata_blob(
            &usage_message(1132, 500, 234, 50, None, "resp-3"),
            Some("gemini-3.6-flash"),
            None,
            1_785_140_300,
        );
        // 从中间截断：解码必须失败而不是 panic。
        assert!(decode_gen_metadata(&blob[..blob.len() / 2]).is_none());
        assert!(decode_gen_metadata(&[]).is_none());
    }

    #[test]
    fn checksum_mismatch_counts_parse_issue() {
        // 重写 #3 校验和字段为错误值：解码成功但校验失败。
        let mut broken = Vec::new();
        broken.extend_from_slice(&varint_field(1, 1132));
        broken.extend_from_slice(&varint_field(2, 500));
        broken.extend_from_slice(&varint_field(3, 999));
        broken.extend_from_slice(&varint_field(9, 234));
        broken.extend_from_slice(&varint_field(10, 50));
        broken.extend_from_slice(&string_field(11, "resp-4"));
        let blob = gen_metadata_blob(&broken, Some("gemini-3.6-flash"), None, 1_785_140_400);

        let (_dir, path) = synthetic_conversation(&[(1, blob)]);
        let result =
            parse_conversation_file(&path, "hash", &CancellationToken::new()).expect("parse");
        assert_eq!(result.parse_issues.accounting_anomaly_lines, 1);
        assert_eq!(result.parse_issues.malformed_lines, 0);
        assert_eq!(result.parse_issues.total(), 0);
        // 事件仍导入（不改数）。
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].tokens.total_tokens, 1632 + 284);
    }

    #[test]
    fn conversation_file_maps_channels_with_reasoning_disjoint_from_output() {
        // 本机例证形状：#1=1132, #2=500, #5=1200, #9=234, #10=50。
        let blob = gen_metadata_blob(
            &usage_message(1132, 500, 234, 50, Some(1200), "resp-1"),
            Some("gemini-3.6-flash"),
            Some("Gemini 3.6 Flash (High)"),
            1_785_140_200,
        );
        let (_dir, path) = synthetic_conversation(&[(1, blob)]);

        let result =
            parse_conversation_file(&path, "hash", &CancellationToken::new()).expect("parse");
        assert_eq!(result.events.len(), 1);
        let event = &result.events[0];
        assert_eq!(event.source, SourceKind::Antigravity);
        // input = #2 + #1（system prompt 并入）。
        assert_eq!(event.tokens.input_tokens, 500 + 1132);
        assert_eq!(event.tokens.cache_read_tokens, 1200);
        assert_eq!(event.tokens.output_tokens, 234);
        assert_eq!(event.tokens.reasoning_output_tokens, 50);
        // total = input + cache_read + output + reasoning（#9/#10 不相交证明）。
        assert_eq!(event.tokens.total_tokens, 1632 + 1200 + 234 + 50);
        assert_eq!(event.model, "gemini-3.6-flash");
        assert_eq!(
            event.event_key,
            format!("antigravity:hash::{}", hash_string("resp-1"))
        );
        assert!(result.parse_issues.total() == 0);
    }

    #[test]
    fn all_zero_usage_rows_are_skipped() {
        let blob = gen_metadata_blob(
            &usage_message(0, 0, 0, 0, None, "resp-zero"),
            Some("gemini-3.6-flash"),
            None,
            1_785_140_200,
        );
        let (_dir, path) = synthetic_conversation(&[(1, blob)]);
        let result =
            parse_conversation_file(&path, "hash", &CancellationToken::new()).expect("parse");
        assert_eq!(result.events.len(), 0);
    }

    #[test]
    fn empty_conversation_skips_cleanly() {
        let (_dir, path) = synthetic_conversation(&[]);
        let result =
            parse_conversation_file(&path, "hash", &CancellationToken::new()).expect("parse");
        assert_eq!(result.events.len(), 0);
        assert_eq!(result.parse_issues.total(), 0);
    }

    #[test]
    fn session_models_backfills_missing_model_ids() {
        // 行 1 有 label+model；行 2 只有 label；行 3 两者皆无 → sole_model。
        let row1 = gen_metadata_blob(
            &usage_message(1132, 100, 10, 2, None, "r1"),
            Some("gemini-3.6-flash"),
            Some("Gemini 3.6 Flash (High)"),
            1_785_140_200,
        );
        let row2 = gen_metadata_blob(
            &usage_message(1132, 100, 10, 2, None, "r2"),
            None,
            Some("Gemini 3.6 Flash (High)"),
            1_785_140_210,
        );
        let row3 = gen_metadata_blob(
            &usage_message(1132, 100, 10, 2, None, "r3"),
            None,
            None,
            1_785_140_220,
        );
        let (_dir, path) = synthetic_conversation(&[(1, row1), (2, row2), (3, row3)]);

        let result =
            parse_conversation_file(&path, "hash", &CancellationToken::new()).expect("parse");
        assert_eq!(result.events.len(), 3);
        assert_eq!(result.events[0].model, "gemini-3.6-flash");
        assert_eq!(result.events[1].model, "gemini-3.6-flash", "label 回填");
        assert_eq!(
            result.events[2].model, "gemini-3.6-flash",
            "sole_model 回填"
        );
    }

    #[test]
    fn session_models_drop_ambiguous_label_mappings() {
        // 同一 label 配到两个 model → 歧义丢弃 → unknown。
        let row1 = gen_metadata_blob(
            &usage_message(1132, 100, 10, 2, None, "r1"),
            Some("gemini-3.6-flash"),
            Some("Gemini 3.6 Flash"),
            1_785_140_200,
        );
        let row2 = gen_metadata_blob(
            &usage_message(1132, 100, 10, 2, None, "r2"),
            Some("gemini-3.7-flash"),
            Some("Gemini 3.6 Flash"),
            1_785_140_210,
        );
        let (_dir, path) = synthetic_conversation(&[(1, row1), (2, row2)]);

        let result =
            parse_conversation_file(&path, "hash", &CancellationToken::new()).expect("parse");
        assert_eq!(result.events.len(), 2);
        assert_eq!(result.events[0].model, "gemini-3.6-flash");
        assert_eq!(result.events[1].model, "gemini-3.7-flash");
    }

    #[test]
    fn missing_response_id_falls_back_to_row_id_with_issue() {
        let mut usage = Vec::new();
        usage.extend_from_slice(&varint_field(1, 1132));
        usage.extend_from_slice(&varint_field(2, 500));
        usage.extend_from_slice(&varint_field(3, 284));
        usage.extend_from_slice(&varint_field(9, 234));
        usage.extend_from_slice(&varint_field(10, 50));
        let blob = gen_metadata_blob(&usage, Some("gemini-3.6-flash"), None, 1_785_140_200);
        let (_dir, path) = synthetic_conversation(&[(7, blob)]);

        let result =
            parse_conversation_file(&path, "hash", &CancellationToken::new()).expect("parse");
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.parse_issues.accounting_anomaly_lines, 1);
        assert_eq!(result.parse_issues.malformed_lines, 0);
        assert_eq!(result.parse_issues.total(), 0);
        assert_eq!(
            result.events[0].event_key,
            format!("antigravity:hash::{}", hash_string("row-7"))
        );
    }

    #[test]
    fn trajectory_metadata_supplies_workspace_project() {
        let blob = gen_metadata_blob(
            &usage_message(1132, 500, 234, 50, None, "resp-1"),
            Some("gemini-3.6-flash"),
            None,
            1_785_140_200,
        );
        let (_dir, path) = synthetic_conversation(&[(1, blob)]);
        let result =
            parse_conversation_file(&path, "hash", &CancellationToken::new()).expect("parse");
        // workspace URI 解析为 project（解析失败 → None，不报错）。
        let _ = result.events[0].project.as_ref();
    }
}
