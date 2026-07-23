//! Pi / Oh My Pi agent session JSONL parser.
//!
//! Pi and Oh My Pi both persist Pi-compatible session transcripts under
//! `<root>/<project>/agent_<session>.jsonl`. They are merged into one stable
//! `pi` source: discovery enumerates both the Pi root (`PI_AGENT_DIR` or
//! `~/.pi/agent/sessions`) and the Oh My Pi root (`~/.omp/agent/sessions`) and
//! dedupes by canonical path, while every parsed event still carries
//! `source = pi`. Only assistant `message` records that carry a `usage` block
//! become [`UsageEvent`]s; `title`/`session`/`model_change`/thinking-level
//! metadata lines are ignored. Each retained record maps 1:1 to one event, so
//! there is no cumulative-delta bookkeeping like Codex.

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
    util::{bucket_start_from_rfc3339, hash_string},
};

/// Stable fallback model when a Pi assistant message omits `model`. The raw
/// source model string (e.g. `gpt-5.5`) is otherwise preserved verbatim.
const FALLBACK_MODEL: &str = "pi";

#[derive(Debug, Clone)]
struct PiShardPlan {
    files: Vec<CandidateFile>,
}

#[derive(Debug, Default)]
struct PiShardOutput {
    events: Vec<UsageEvent>,
    cursors: Vec<FileCursor>,
    reset_path_hashes: Vec<String>,
    events_seen: usize,
    events_replayed: usize,
    bytes_scanned: u64,
    seen_file_paths: Vec<String>,
}

#[derive(Debug)]
struct PiParseResult {
    end_offset: u64,
    events: Vec<UsageEvent>,
}

/// Pi / Oh My Pi session parser. Owns the per-file scan + per-shard commit
/// pipeline across both default roots.
pub struct PiParser;

impl SourceParser for PiParser {
    fn source(&self) -> SourceKind {
        SourceKind::Pi
    }

    fn parse<'a>(
        &'a self,
        store: &'a Store,
        writer: &'a mut SyncRunWriter,
        parallelism: usize,
        cancel: &'a CancellationToken,
        progress: Option<ProgressSink<'a>>,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>> {
        Box::pin(sync_pi(store, writer, parallelism, cancel, progress))
    }
}

async fn sync_pi(
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
) -> Result<SourceSyncStats> {
    /*
     * ========================================================================
     * 步骤1：并行解析 Pi / Oh My Pi 会话真源
     * ========================================================================
     * 目标：
     * 1) 合并 ~/.pi/agent/sessions 与 ~/.omp/agent/sessions 两根并按 canonical 去重
     * 2) 只把缺失、追加或改写的文件送去解析
     * 3) 返回 event / cursor / reset 指令给单 writer 统一落库
     */
    info!("开始同步 Pi / Oh My Pi 会话真源");

    // 1.1 构建按 project 目录分片的候选文件计划
    let parse_started = Instant::now();
    let listing = source_files::list_pi_session_files();
    let inventory_paths = listing.file_paths();
    store.source_files().mark_inventory_seen(
        SourceKind::Pi,
        &inventory_paths,
        writer.run_started_at(),
    )?;
    let inventory_error = listing.error_summary();
    let files = listing.paths;
    let total_files = files.len();
    let cursor_map = store.cursors().load_file_cursors(SourceKind::Pi)?;

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
        .map(|files| PiShardPlan { files })
        .collect::<Vec<_>>();
    plans.sort_by_key(|plan| plan.files.first().map(|file| file.path.clone()));
    let planned_files = plans.iter().map(|plan| plan.files.len()).sum::<usize>();
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source: SourceKind::Pi,
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
            tasks.push(task::spawn_blocking(move || parse_pi_shard(plan, counter)));
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
                            source: SourceKind::Pi,
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
                    source: SourceKind::Pi,
                    files_scanned: completed_files,
                    records_imported: inserted as u64,
                    current_file: None,
                },
            );

            // 1.3 把 reset / event / cursor 协议交给单写入端原子提交
            let commit = writer.commit_shard(SyncShard {
                source: SourceKind::Pi,
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
                    source: SourceKind::Pi,
                    files_scanned: completed_files,
                    records_imported: inserted as u64,
                    current_file: None,
                },
            );
        }
    }

    let mut stats = SourceSyncStats {
        source: SourceKind::Pi,
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
        "完成 Pi / Oh My Pi 会话真源解析"
    );
    Ok(stats)
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
}

fn parse_pi_shard(plan: PiShardPlan, progress: FileProgressCounter) -> Result<PiShardOutput> {
    let mut output = PiShardOutput::default();

    for candidate in plan.files {
        let existing = candidate.existing.clone();
        let decision = decide_file_replay(candidate)?;
        output
            .seen_file_paths
            .push(decision.snapshot.path.to_string_lossy().to_string());
        let path_hash = hash_string(&decision.snapshot.path.to_string_lossy());

        let parsed =
            parse_session_file(&decision.snapshot.path, &path_hash, decision.start_offset)?;
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

/// Parses a Pi session JSONL file starting at `start_offset`.
///
/// Each retained assistant `usage` line becomes one [`UsageEvent`]. The record's
/// **start byte offset** is used as its stable position: byte offsets are
/// identical whether the file is reparsed from `0` or appended from the stored
/// cursor offset, so the derived `event_key` is idempotent across re-sync.
fn parse_session_file(
    file_path: &Path,
    path_hash: &str,
    start_offset: u64,
) -> Result<PiParseResult> {
    let file = File::open(file_path)?;
    let file_len = file.metadata()?.len();
    if start_offset >= file_len {
        return Ok(PiParseResult {
            end_offset: file_len,
            events: Vec::new(),
        });
    }

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

        // Cheap prefilter before the JSON parse: a usable Pi line carries token
        // counts under a `usage` key nested in a `message` object.
        if !(line.contains("usage") && line.contains("message")) {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        // `type` is absent or `"message"` on usage-bearing records; anything else
        // (title/session/model_change/thinking-level metadata) is ignored.
        if value
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|message_type| message_type != "message")
        {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(tokens) = message.get("usage").and_then(parse_pi_tokens) else {
            continue;
        };
        let Some(timestamp_raw) = value.get("timestamp").and_then(Value::as_str) else {
            continue;
        };
        let Ok(timestamp) = chrono::DateTime::parse_from_rfc3339(timestamp_raw) else {
            continue;
        };
        let event_at = timestamp.with_timezone(&chrono::Utc).to_rfc3339();
        let Some(hour_start) = bucket_start_from_rfc3339(&event_at) else {
            continue;
        };
        let model = message
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| FALLBACK_MODEL.to_string());

        let logical_identity = format!(
            "{path_hash}\0{record_offset}\0{event_at}\0{model}\0{}\0{}\0{}\0{}\0{}\0{}",
            tokens.input_tokens,
            tokens.cache_read_tokens,
            tokens.cache_creation_tokens,
            tokens.output_tokens,
            tokens.reasoning_output_tokens,
            tokens.total_tokens,
        );
        events.push(UsageEvent {
            event_key: format!("pi:{}", hash_string(&logical_identity)),
            source: SourceKind::Pi,
            provider_label: String::new(),
            model,
            event_at,
            hour_start,
            tokens,
            project: None,
            session: session.clone(),
        });
    }

    Ok(PiParseResult {
        end_offset: offset,
        events,
    })
}

/// Maps a Pi `usage` object to normalized [`UsageTokens`].
///
/// Channels are clamped to non-negative. A trustworthy `totalTokens` (> 0) is
/// authoritative; otherwise the total is the saturating sum of the four
/// input/cache/output channels, each once. `reasoningTokens` is a separate
/// diagnostic channel and is never added to output or total. Returns `None`
/// when every observed field (channels, reasoning, and total) is zero so the
/// caller skips the record.
fn parse_pi_tokens(usage: &Value) -> Option<UsageTokens> {
    let input_tokens = read_i64(usage, "input").unwrap_or_default().max(0);
    let output_tokens = read_i64(usage, "output").unwrap_or_default().max(0);
    let cache_read_tokens = read_i64(usage, "cacheRead").unwrap_or_default().max(0);
    let cache_creation_tokens = read_i64(usage, "cacheWrite").unwrap_or_default().max(0);
    let reasoning_output_tokens = read_i64(usage, "reasoningTokens")
        .unwrap_or_default()
        .max(0);
    let upstream_total = read_i64(usage, "totalTokens").unwrap_or_default().max(0);

    if input_tokens == 0
        && output_tokens == 0
        && cache_read_tokens == 0
        && cache_creation_tokens == 0
        && reasoning_output_tokens == 0
        && upstream_total == 0
    {
        return None;
    }

    let total_tokens = if upstream_total > 0 {
        upstream_total
    } else {
        input_tokens
            .saturating_add(cache_read_tokens)
            .saturating_add(cache_creation_tokens)
            .saturating_add(output_tokens)
    };

    Some(UsageTokens {
        input_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens,
    })
}

fn read_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

/// Extracts the session id from a Pi session file stem.
///
/// Layout: `.../sessions/PROJECT/agent_SESSION.jsonl`, so the session id is the
/// file stem portion after the first `_` (mirrors the reference Pi adapter).
fn extract_session_id(path: &Path) -> String {
    let filename = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    filename
        .split_once('_')
        .map_or(filename, |(_, session)| session)
        .to_string()
}

fn build_session(file_path: &Path, path_hash: &str) -> Option<SessionInfo> {
    let session_id = extract_session_id(file_path);
    Some(SessionInfo {
        session_label: Some(session_id.clone()),
        session_id,
        source_path_hash: Some(path_hash.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::{FALLBACK_MODEL, parse_session_file};
    use std::{fs, path::PathBuf};
    use tempfile::TempDir;

    /// Builds a synthetic Pi session file under a fake `.omp` layout so
    /// `extract_session_id` resolves the `sess-abc-123` stem segment.
    fn write_session_file(content: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir
            .path()
            .join(".omp")
            .join("agent")
            .join("sessions")
            .join("project-a")
            .join("agent_sess-abc-123.jsonl");
        fs::create_dir_all(path.parent().unwrap()).expect("create layout");
        fs::write(&path, content).expect("write session file");
        (dir, path)
    }

    fn parse(content: &str) -> Vec<crate::models::UsageEvent> {
        let (_dir, path) = write_session_file(content);
        parse_session_file(&path, "path-hash", 0)
            .expect("parse session file")
            .events
    }

    #[test]
    fn maps_channels_with_authoritative_total_and_separate_reasoning() {
        // All four channels + reasoning + a trustworthy upstream total.
        let content = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":100,"output":50,"cacheRead":40,"cacheWrite":8,"reasoningTokens":10,"totalTokens":333}}}"#;

        let events = parse(content);

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.source, crate::models::SourceKind::Pi);
        // R4: the raw source model string is preserved verbatim (no store prefix,
        // no whitelist).
        assert_eq!(event.model, "gpt-5.5");
        assert_eq!(event.tokens.input_tokens, 100);
        assert_eq!(event.tokens.output_tokens, 50);
        assert_eq!(event.tokens.cache_read_tokens, 40);
        assert_eq!(event.tokens.cache_creation_tokens, 8);
        // R3: reasoning is stored separately and NEVER folded into output/total.
        assert_eq!(event.tokens.reasoning_output_tokens, 10);
        // A trustworthy `totalTokens` is authoritative (not the channel sum, not
        // channel-sum + reasoning).
        assert_eq!(event.tokens.total_tokens, 333);
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
    fn falls_back_to_channel_sum_when_total_absent() {
        // No `totalTokens`: the total is the sum of the four channels; reasoning
        // stays separate and is excluded.
        let content = r#"{"type":"message","timestamp":"2026-01-02T00:05:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":100,"output":50,"cacheRead":40,"cacheWrite":8,"reasoningTokens":10}}}"#;

        let events = parse(content);

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.tokens.reasoning_output_tokens, 10);
        assert_eq!(event.tokens.total_tokens, 100 + 40 + 8 + 50);
    }

    #[test]
    fn preserves_unknown_future_model() {
        let content = r#"{"type":"message","timestamp":"2026-01-02T00:10:00.000Z","message":{"role":"assistant","model":"codex-auto-review-next","usage":{"input":10,"output":5}}}"#;

        let events = parse(content);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].model, "codex-auto-review-next");
    }

    #[test]
    fn falls_back_to_stable_model_when_absent() {
        let content = r#"{"type":"message","timestamp":"2026-01-02T00:15:00.000Z","message":{"role":"assistant","usage":{"input":10,"output":5}}}"#;

        let events = parse(content);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].model, FALLBACK_MODEL);
    }

    #[test]
    fn skips_metadata_user_zero_and_malformed_lines() {
        let content = concat!(
            // title metadata carries no usage semantics even if it mentions them.
            r#"{"type":"title","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","usage":{"input":999,"output":999}}}"#,
            "\n",
            // session metadata line.
            r#"{"type":"session","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","usage":{"input":888,"output":888}}}"#,
            "\n",
            // model_change metadata line.
            r#"{"type":"model_change","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","usage":{"input":777,"output":777}}}"#,
            "\n",
            // a user message must not become a usage event.
            r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"user","usage":{"input":666,"output":666}}}"#,
            "\n",
            // an all-zero assistant record is skipped.
            r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"reasoningTokens":0,"totalTokens":0}}}"#,
            "\n",
            // a malformed line (with the prefilter substrings) must not fail the file.
            "garbage usage message line",
            "\n",
            // the single real assistant usage record.
            r#"{"type":"message","timestamp":"2026-01-02T00:20:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":100,"output":50}}}"#,
        );

        let events = parse(content);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tokens.input_tokens, 100);
        assert_eq!(events[0].tokens.output_tokens, 50);
    }

    #[test]
    fn distinct_records_get_distinct_idempotent_event_keys() {
        let content = concat!(
            r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":10,"output":5}}}"#,
            "\n",
            r#"{"type":"message","timestamp":"2026-01-02T00:05:00.000Z","message":{"role":"assistant","model":"gpt-5.5","usage":{"input":20,"output":6}}}"#,
        );

        let (_dir, path) = write_session_file(content);
        let first = parse_session_file(&path, "path-hash", 0).expect("first parse");
        let second = parse_session_file(&path, "path-hash", 0).expect("second parse");

        assert_eq!(first.events.len(), 2);
        assert_ne!(first.events[0].event_key, first.events[1].event_key);
        // Reparsing from offset 0 yields identical keys (idempotent re-sync).
        assert_eq!(first.events[0].event_key, second.events[0].event_key);
        assert_eq!(first.events[1].event_key, second.events[1].event_key);
        assert_eq!(first.end_offset, content.len() as u64);
    }
}
