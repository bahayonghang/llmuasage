//! Kimi Code `wire.jsonl` parser.
//!
//! Kimi Code writes `<root>/sessions/WORKSPACE/SESSION/agents/AGENT/wire.jsonl`
//! where each turn-scoped LLM call is persisted as a `usage.record` line. This
//! parser only imports records tagged `usageScope == "turn"`; session-scoped
//! bookkeeping, `step.end` duplicates, and non-usage lines are ignored. Each
//! record already carries its own absolute per-turn token counts, so there is
//! no cumulative-delta bookkeeping like Codex — a record maps 1:1 to one
//! `UsageEvent`.

use std::{
    collections::HashMap,
    fs::File,
    future::Future,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    pin::Pin,
    time::Instant,
};

use anyhow::Result;
use serde_json::Value;
use tokio::task;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    models::{SessionInfo, SourceKind, UsageEvent, UsageTokens},
    parsers::{
        ProgressSink, SourceParser, SourceSyncStats, SyncEvent,
        file_progress::{FileProgress, FileProgressCounter},
        file_state::{
            CandidateFile, FileReplayMode, decide_file_replay, finalize_cursor, should_rescan_file,
        },
        source_files,
    },
    store::{FileCursor, Store, SyncRunWriter, SyncShard},
    util::{bucket_start_from_rfc3339, hash_string, metadata_modified_ns},
};

/// Stable fallback model when a `usage.record` omits `model`. The raw source
/// model string (e.g. `kimi-code/k3`) is otherwise preserved verbatim.
const FALLBACK_MODEL: &str = "kimi-code";

#[derive(Debug, Clone)]
struct KimiShardPlan {
    files: Vec<CandidateFile>,
}

#[derive(Debug, Default)]
struct KimiShardOutput {
    events: Vec<UsageEvent>,
    cursors: Vec<FileCursor>,
    reset_path_hashes: Vec<String>,
    events_seen: usize,
    events_replayed: usize,
    bytes_scanned: u64,
    seen_file_paths: Vec<String>,
}

#[derive(Debug)]
struct KimiParseResult {
    end_offset: u64,
    events: Vec<UsageEvent>,
}

/// Kimi Code `wire.jsonl` parser. Owns the per-file scan + per-shard commit
/// pipeline for `~/.kimi-code/sessions` (or `KIMI_CODE_HOME/sessions`).
pub struct KimiCodeParser;

impl SourceParser for KimiCodeParser {
    fn source(&self) -> SourceKind {
        SourceKind::KimiCode
    }

    fn parse<'a>(
        &'a self,
        store: &'a Store,
        writer: &'a mut SyncRunWriter,
        parallelism: usize,
        cancel: &'a CancellationToken,
        progress: Option<ProgressSink<'a>>,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>> {
        Box::pin(sync_kimi_code(store, writer, parallelism, cancel, progress))
    }
}

async fn sync_kimi_code(
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
) -> Result<SourceSyncStats> {
    /*
     * ========================================================================
     * 步骤1：并行解析 Kimi Code wire.jsonl 真源
     * ========================================================================
     * 目标：
     * 1) 读取 ~/.kimi-code/sessions（或 KIMI_CODE_HOME/sessions）下的 wire.jsonl
     * 2) 只把缺失、追加或改写的文件送去解析
     * 3) 返回 event / cursor / reset 指令给单 writer 统一落库
     */
    info!("开始同步 Kimi Code wire.jsonl 真源");

    // 1.1 构建按 agent 目录分片的候选文件计划
    let parse_started = Instant::now();
    let listing = source_files::list_kimi_wire_files();
    let inventory_paths = listing.file_paths();
    store.source_files().mark_inventory_seen(
        SourceKind::KimiCode,
        &inventory_paths,
        writer.run_started_at(),
    )?;
    let inventory_error = listing.error_summary();
    let files = listing.paths;
    let total_files = files.len();
    let cursor_map = store.cursors().load_file_cursors(SourceKind::KimiCode)?;

    let mut shards = HashMap::<PathBuf, Vec<CandidateFile>>::new();
    let mut changed_files = 0usize;
    for file_path in files {
        let key = file_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
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
    let mut events_seen = 0usize;
    let mut events_replayed = 0usize;
    let mut bytes_scanned = 0u64;
    let mut inserted = 0usize;
    let mut write_ms = 0u64;
    let mut plans = shards
        .into_values()
        .map(|files| KimiShardPlan { files })
        .collect::<Vec<_>>();
    plans.sort_by_key(|plan| plan.files.first().map(|file| file.path.clone()));
    let planned_files = plans.iter().map(|plan| plan.files.len()).sum::<usize>();
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source: SourceKind::KimiCode,
            files_total: planned_files as u64,
        },
    );
    let (mut file_progress, file_progress_counter) = FileProgress::new();

    let width = parallelism.max(1);
    for batch in plans.chunks(width) {
        if cancel.is_cancelled() {
            break;
        }
        let mut tasks = Vec::new();
        for plan in batch {
            let plan = plan.clone();
            let counter = file_progress_counter.clone();
            tasks.push(task::spawn_blocking(move || {
                parse_kimi_shard(plan, counter)
            }));
        }

        for task in tasks {
            if cancel.is_cancelled() {
                break;
            }
            let shard = file_progress
                .wait_for(task, |files_scanned| {
                    emit_progress(
                        &mut progress,
                        SyncEvent::Progress {
                            source: SourceKind::KimiCode,
                            files_scanned,
                            records_imported: inserted as u64,
                            current_file: None,
                        },
                    );
                })
                .await??;
            events_seen += shard.events_seen;
            events_replayed += shard.events_replayed;
            bytes_scanned += shard.bytes_scanned;

            let completed_files = file_progress.boundary_snapshot();
            emit_progress(
                &mut progress,
                SyncEvent::Progress {
                    source: SourceKind::KimiCode,
                    files_scanned: completed_files,
                    records_imported: inserted as u64,
                    current_file: None,
                },
            );

            // 1.3 把 reset / event / cursor 协议交给单写入端原子提交
            let commit = writer.commit_shard(SyncShard {
                source: SourceKind::KimiCode,
                reset_path_hashes: shard.reset_path_hashes,
                events: shard.events,
                cursors: shard.cursors,
                seen_file_paths: shard.seen_file_paths,
                raw_records: Vec::new(),
                turns: Vec::new(),
                tool_calls: Vec::new(),
            })?;
            inserted += commit.events_inserted;
            write_ms += commit.write_ms;
            emit_progress(
                &mut progress,
                SyncEvent::Progress {
                    source: SourceKind::KimiCode,
                    files_scanned: completed_files,
                    records_imported: inserted as u64,
                    current_file: None,
                },
            );
        }
    }

    let mut stats = SourceSyncStats {
        source: SourceKind::KimiCode,
        files_processed: total_files,
        changed_files,
        skipped_files: total_files.saturating_sub(changed_files),
        bytes_scanned,
        events_seen,
        events_replayed,
        events_inserted: inserted,
        write_ms,
        last_error: inventory_error,
        ..SourceSyncStats::default()
    };
    let total_elapsed = parse_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    stats.parse_ms = total_elapsed.saturating_sub(write_ms);

    info!(
        files_processed = stats.files_processed,
        changed_files = stats.changed_files,
        skipped_files = stats.skipped_files,
        events_seen = stats.events_seen,
        bytes_scanned = stats.bytes_scanned,
        "完成 Kimi Code wire.jsonl 真源解析"
    );
    Ok(stats)
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
}

fn parse_kimi_shard(plan: KimiShardPlan, progress: FileProgressCounter) -> Result<KimiShardOutput> {
    let mut output = KimiShardOutput::default();

    for candidate in plan.files {
        let existing = candidate.existing.clone();
        let decision = decide_file_replay(candidate)?;
        output
            .seen_file_paths
            .push(decision.snapshot.path.to_string_lossy().to_string());
        let path_hash = hash_string(&decision.snapshot.path.to_string_lossy());

        let parsed = parse_wire_file(&decision.snapshot.path, &path_hash, decision.start_offset)?;
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
        progress.advance_file();
    }

    Ok(output)
}

/// Parses a `wire.jsonl` file starting at `start_offset`.
///
/// Each retained `usage.record` line becomes one [`UsageEvent`]. The record's
/// **start byte offset** is used as its stable position: byte offsets are
/// identical whether the file is reparsed from `0` or appended from the stored
/// cursor offset, so the derived `event_key` is idempotent across re-sync.
fn parse_wire_file(
    file_path: &Path,
    path_hash: &str,
    start_offset: u64,
) -> Result<KimiParseResult> {
    let file = File::open(file_path)?;
    let file_len = file.metadata()?.len();
    if start_offset >= file_len {
        return Ok(KimiParseResult {
            end_offset: file_len,
            events: Vec::new(),
        });
    }

    let fallback_ms = file_mtime_ms(file_path);
    let session = build_session(file_path, path_hash);

    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(start_offset))?;

    let mut offset = start_offset;
    let mut line = String::new();
    let mut events = Vec::new();

    loop {
        line.clear();
        let record_offset = offset;
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break;
        }
        offset += bytes_read as u64;

        // Cheap prefilter before the JSON parse; every retained line is a
        // `usage.record`.
        if !line.contains("usage.record") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("usage.record") {
            continue;
        }
        // kimi-code treats a missing `usageScope` as session-scoped, so require
        // an explicit `"turn"` to avoid counting aggregate/compaction records.
        if value.get("usageScope").and_then(Value::as_str) != Some("turn") {
            continue;
        }
        let Some(tokens) = value.get("usage").and_then(parse_kimi_tokens) else {
            continue;
        };
        let time_ms = value
            .get("time")
            .and_then(Value::as_i64)
            .unwrap_or(fallback_ms);
        let Some(timestamp) = chrono::DateTime::from_timestamp_millis(time_ms) else {
            continue;
        };
        let event_at = timestamp.to_rfc3339();
        let Some(hour_start) = bucket_start_from_rfc3339(&event_at) else {
            continue;
        };
        let model = value
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| FALLBACK_MODEL.to_string());

        let logical_identity = format!(
            "{path_hash}\0{record_offset}\0{time_ms}\0{model}\0{}\0{}\0{}\0{}\0{}",
            tokens.input_tokens,
            tokens.cache_read_tokens,
            tokens.cache_creation_tokens,
            tokens.output_tokens,
            tokens.total_tokens,
        );
        events.push(UsageEvent {
            event_key: format!("kimi_code:{}", hash_string(&logical_identity)),
            source: SourceKind::KimiCode,
            provider_label: String::new(),
            model,
            event_at,
            hour_start,
            tokens,
            project: None,
            session: session.clone(),
        });
    }

    Ok(KimiParseResult {
        end_offset: offset,
        events,
    })
}

/// Maps a Kimi Code `usage` object to normalized [`UsageTokens`].
///
/// Channels are clamped to non-negative. Kimi wire has no reasoning channel and
/// no upstream total, so the total is the saturating sum of the four channels
/// (samples include `i64::MAX` values). Returns `None` when every channel is
/// zero so the caller skips the record.
fn parse_kimi_tokens(usage: &Value) -> Option<UsageTokens> {
    let input_tokens = read_i64(usage, "inputOther").unwrap_or_default().max(0);
    let output_tokens = read_i64(usage, "output").unwrap_or_default().max(0);
    let cache_read_tokens = read_i64(usage, "inputCacheRead").unwrap_or_default().max(0);
    let cache_creation_tokens = read_i64(usage, "inputCacheCreation")
        .unwrap_or_default()
        .max(0);

    if input_tokens == 0
        && output_tokens == 0
        && cache_read_tokens == 0
        && cache_creation_tokens == 0
    {
        return None;
    }

    let total_tokens = input_tokens
        .saturating_add(cache_read_tokens)
        .saturating_add(cache_creation_tokens)
        .saturating_add(output_tokens);

    Some(UsageTokens {
        input_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        output_tokens,
        reasoning_output_tokens: 0,
        total_tokens,
    })
}

fn read_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

/// Extracts the session id from a Kimi Code wire path.
///
/// Layout: `.../sessions/WORKSPACE/SESSION_UUID/agents/AGENT/wire.jsonl`, so the
/// `SESSION_UUID` segment is three parents above the file.
fn extract_session_id(path: &Path) -> Option<String> {
    path.parent() // AGENT
        .and_then(Path::parent) // agents
        .and_then(Path::parent) // SESSION_UUID
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::to_string)
}

fn build_session(file_path: &Path, path_hash: &str) -> Option<SessionInfo> {
    let session_id = extract_session_id(file_path).unwrap_or_else(|| path_hash.to_string());
    Some(SessionInfo {
        session_label: Some(session_id.clone()),
        session_id,
        source_path_hash: Some(path_hash.to_string()),
    })
}

fn file_mtime_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .ok()
        .map(|metadata| metadata_modified_ns(&metadata) / 1_000_000)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{FALLBACK_MODEL, parse_wire_file};
    use std::{fs, path::PathBuf};
    use tempfile::TempDir;

    /// Builds a synthetic `wire.jsonl` under a fake kimi-code layout so
    /// `extract_session_id` resolves the `sess-abc-123` segment.
    fn write_wire_file(content: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir
            .path()
            .join(".kimi-code")
            .join("sessions")
            .join("test-ws")
            .join("sess-abc-123")
            .join("agents")
            .join("main")
            .join("wire.jsonl");
        fs::create_dir_all(path.parent().unwrap()).expect("create layout");
        fs::write(&path, content).expect("write wire.jsonl");
        (dir, path)
    }

    fn parse(content: &str) -> Vec<crate::models::UsageEvent> {
        let (_dir, path) = write_wire_file(content);
        parse_wire_file(&path, "path-hash", 0)
            .expect("parse wire.jsonl")
            .events
    }

    #[test]
    fn maps_turn_scoped_usage_and_preserves_k3_model() {
        let content = r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":5102,"output":172,"inputCacheRead":13312,"inputCacheCreation":8},"usageScope":"turn","time":1780319377014}"#;

        let events = parse(content);

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.source, crate::models::SourceKind::KimiCode);
        // R4: the raw source model string is preserved verbatim (no prefix
        // stripping, no whitelist).
        assert_eq!(event.model, "kimi-code/k3");
        assert_eq!(event.tokens.input_tokens, 5102);
        assert_eq!(event.tokens.output_tokens, 172);
        assert_eq!(event.tokens.cache_read_tokens, 13312);
        assert_eq!(event.tokens.cache_creation_tokens, 8);
        assert_eq!(event.tokens.reasoning_output_tokens, 0);
        // Kimi has no upstream total: total = sum of the four channels.
        assert_eq!(event.tokens.total_tokens, 5102 + 172 + 13312 + 8);
        assert_eq!(
            event
                .session
                .as_ref()
                .map(|session| session.session_id.as_str()),
            Some("sess-abc-123")
        );
        assert!(event.project.is_none());
    }

    #[test]
    fn preserves_unknown_future_model_suffix() {
        let content = r#"{"type":"usage.record","model":"kimi-code/k4-preview","usage":{"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377100}"#;

        let events = parse(content);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].model, "kimi-code/k4-preview");
    }

    #[test]
    fn falls_back_to_stable_model_when_absent() {
        let content = r#"{"type":"usage.record","usage":{"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377100}"#;

        let events = parse(content);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].model, FALLBACK_MODEL);
    }

    #[test]
    fn skips_session_scoped_missing_scope_zero_step_end_and_malformed() {
        let content = concat!(
            // session-scoped bookkeeping (e.g. compaction) is not turn usage
            r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":999,"output":999,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"session","time":1780319377000}"#,
            "\n",
            // missing usageScope => treated as session-scoped by kimi-code
            r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":888,"output":888,"inputCacheRead":0,"inputCacheCreation":0},"time":1780319377005}"#,
            "\n",
            // all-zero turn record is skipped
            r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":0,"output":0,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377008}"#,
            "\n",
            // step.end duplicates the same turn usage and is not a usage.record
            r#"{"type":"step.end","usage":{"inputOther":777,"output":777,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377009}"#,
            "\n",
            // other line type
            r#"{"type":"context.append_loop_event","event":{"type":"tool.call","name":"Read"},"time":1780319377011}"#,
            "\n",
            // malformed non-JSON line must not fail the whole file
            "not valid json at all",
            "\n",
            // the single real turn record
            r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":100,"output":50,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377010}"#,
        );

        let events = parse(content);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tokens.input_tokens, 100);
        assert_eq!(events[0].tokens.output_tokens, 50);
    }

    #[test]
    fn extreme_values_saturate_without_panicking() {
        let content = concat!(
            r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":9223372036854775807,"output":9223372036854775807,"inputCacheRead":2,"inputCacheCreation":0},"usageScope":"turn","time":1780319377014}"#,
            "\n",
            r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":0,"output":0,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377015}"#,
        );

        let events = parse(content);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tokens.input_tokens, i64::MAX);
        assert_eq!(events[0].tokens.output_tokens, i64::MAX);
        assert_eq!(events[0].tokens.cache_read_tokens, 2);
        assert_eq!(events[0].tokens.total_tokens, i64::MAX);
    }

    #[test]
    fn distinct_records_get_distinct_idempotent_event_keys() {
        let content = concat!(
            r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377010}"#,
            "\n",
            r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":20,"output":6,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377020}"#,
        );

        let (_dir, path) = write_wire_file(content);
        let first = parse_wire_file(&path, "path-hash", 0).expect("first parse");
        let second = parse_wire_file(&path, "path-hash", 0).expect("second parse");

        assert_eq!(first.events.len(), 2);
        assert_ne!(first.events[0].event_key, first.events[1].event_key);
        // Reparsing from offset 0 yields identical keys (idempotent re-sync).
        assert_eq!(first.events[0].event_key, second.events[0].event_key);
        assert_eq!(first.events[1].event_key, second.events[1].event_key);
        assert_eq!(first.end_offset, content.len() as u64);
    }
}
