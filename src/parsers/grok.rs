//! Grok Build passive session parser.
//!
//! Grok stores related sidecars under
//! `~/.grok/sessions/<urlencoded-workspace>/<session-id>/`. A session is the
//! replay unit: if any present sidecar changes, every event for the session is
//! reset and rebuilt under one shared `source_path_hash`.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    time::Instant,
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio::task;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::{
    models::{
        ParseIssueKind, ParseIssues, ProjectInfo, SessionInfo, SourceKind, UsageEvent, UsageTokens,
    },
    parsers::{
        ProgressSink, SourceParser, SourceSyncStats, SyncEvent,
        file_progress::{FileProgress, FileProgressCounter},
        file_state::{
            BoundedJsonlReader, CandidateFile, DEFAULT_MAX_JSONL_RECORD_BYTES, JsonlReadStatus,
            JsonlRecordDisposition, decide_file_replay, finalize_cursor,
        },
        source_files::{self, GROK_SIDECAR_NAMES},
    },
    store::{FileCursor, Store, SyncRunWriter, SyncShard},
    util::{bucket_start_from_rfc3339, hash_string},
};

const UNKNOWN_MODEL: &str = "grok-unknown";
const EPOCH_RFC3339: &str = "1970-01-01T00:00:00+00:00";

#[derive(Debug, Clone)]
struct GrokSessionPlan {
    session_dir: PathBuf,
    files: Vec<PathBuf>,
    had_history: bool,
}

#[derive(Debug, Default)]
struct GrokSessionOutput {
    events: Vec<UsageEvent>,
    cursors: Vec<FileCursor>,
    reset_path_hashes: Vec<String>,
    seen_file_paths: Vec<String>,
    bytes_scanned: u64,
    parse_issues: ParseIssues,
    cancelled: bool,
}

#[derive(Debug, Default)]
struct UpdatesParseResult {
    events: Vec<UsageEvent>,
    parse_issues: ParseIssues,
    end_offset: u64,
    last_activity_ms: Option<i64>,
    last_model: Option<String>,
    cancelled: bool,
}

#[derive(Debug)]
struct ActiveTurn {
    baseline_total: i64,
    max_total: i64,
    timestamp_ms: i64,
    model: String,
    turn_index: usize,
}

impl ActiveTurn {
    fn new(baseline_total: i64, timestamp_ms: i64, model: String, turn_index: usize) -> Self {
        Self {
            baseline_total,
            max_total: baseline_total,
            timestamp_ms,
            model,
            turn_index,
        }
    }

    fn observe_total(&mut self, total: i64, timestamp_ms: i64) {
        if total > self.max_total {
            self.max_total = total;
            self.timestamp_ms = timestamp_ms;
        }
    }

    fn into_event(
        self,
        session_id: &str,
        session: &SessionInfo,
        project: Option<&ProjectInfo>,
    ) -> Option<UsageEvent> {
        let total_tokens = self.max_total.saturating_sub(self.baseline_total);
        if total_tokens <= 0 {
            return None;
        }
        build_event(
            format!("grok:{session_id}:{}", self.turn_index),
            non_empty_model(&self.model).unwrap_or_else(|| UNKNOWN_MODEL.to_string()),
            self.timestamp_ms,
            total_tokens,
            session,
            project,
        )
    }
}

pub struct GrokParser;

impl SourceParser for GrokParser {
    fn source(&self) -> SourceKind {
        SourceKind::Grok
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
        Box::pin(sync_grok(
            store,
            writer,
            parallelism,
            recent_cutoff,
            cancel,
            progress,
        ))
    }
}

async fn sync_grok(
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
    recent_cutoff: Option<DateTime<Utc>>,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
) -> Result<SourceSyncStats> {
    info!("starting Grok Build passive session sync");
    let parse_started = Instant::now();
    let listing = source_files::list_grok_session_files();
    let cursor_map = store.cursors().load_file_cursors(SourceKind::Grok)?;
    let tracked_paths = store.source_files().tracked_paths(SourceKind::Grok)?;
    let inventory_paths = listing.file_paths();
    store.source_files().mark_inventory_seen(
        SourceKind::Grok,
        &inventory_paths,
        writer.run_started_at(),
    )?;
    let inventory_error = listing.error_summary();
    let total_files = listing.paths.len();

    let mut sessions = BTreeMap::<PathBuf, Vec<PathBuf>>::new();
    for path in listing.paths {
        if let Some(session_dir) = path.parent() {
            sessions
                .entry(session_dir.to_path_buf())
                .or_default()
                .push(path);
        }
    }

    let mut known_by_session = HashMap::<PathBuf, HashSet<PathBuf>>::new();
    for raw_path in tracked_paths.into_iter().chain(cursor_map.keys().cloned()) {
        let path = PathBuf::from(raw_path);
        if is_grok_sidecar(&path)
            && let Some(session_dir) = path.parent()
        {
            known_by_session
                .entry(session_dir.to_path_buf())
                .or_default()
                .insert(path);
        }
    }

    let mut plans = Vec::new();
    let mut changed_files = 0usize;
    for (session_dir, mut files) in sessions {
        files.sort();
        let current = files.iter().cloned().collect::<HashSet<_>>();
        let known = known_by_session.get(&session_dir);
        if known.is_some_and(|paths| paths.iter().any(|path| !current.contains(path))) {
            warn!(
                session_hash = %hash_string(&session_dir.to_string_lossy()),
                "Grok session has a missing tracked sidecar; preserving prior usage"
            );
            continue;
        }

        let mut changed = false;
        for path in &files {
            let existing = path.to_str().and_then(|raw| cursor_map.get(raw));
            changed |= sidecar_changed(path, existing)?;
        }
        if !changed {
            continue;
        }

        changed_files = changed_files.saturating_add(files.len());
        plans.push(GrokSessionPlan {
            session_dir,
            files,
            had_history: known.is_some_and(|paths| !paths.is_empty()),
        });
    }

    let planned_files = plans.iter().map(|plan| plan.files.len()).sum::<usize>();
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source: SourceKind::Grok,
            files_total: planned_files as u64,
        },
    );
    let (mut file_progress, file_progress_counter) = FileProgress::new();
    let mut events_seen = 0usize;
    let mut events_replayed = 0usize;
    let mut bytes_scanned = 0u64;
    let mut inserted = 0usize;
    let mut write_ms = 0u64;
    let mut parse_issues = ParseIssues::default();

    'batches: for batch in plans.chunks(parallelism.max(1)) {
        if cancel.is_cancelled() {
            break;
        }
        let mut tasks = Vec::new();
        for plan in batch {
            let plan = plan.clone();
            let counter = file_progress_counter.clone();
            let task_cancel = cancel.clone();
            tasks.push(task::spawn_blocking(move || {
                parse_grok_session(plan, counter, task_cancel)
            }));
        }

        let outputs = file_progress
            .wait_for_all(tasks, |files_scanned| {
                emit_progress(
                    &mut progress,
                    SyncEvent::Progress {
                        source: SourceKind::Grok,
                        files_scanned,
                        records_imported: inserted as u64,
                        current_file: None,
                    },
                );
            })
            .await?;

        for mut output in outputs {
            if cancel.is_cancelled() || output.cancelled {
                break 'batches;
            }
            if let Some(cutoff) = recent_cutoff.as_ref() {
                output.events.retain(|event| {
                    crate::parsers::timestamp_in_recent_window(&event.event_at, Some(cutoff))
                });
                output.cursors.clear();
                output.reset_path_hashes.clear();
            }
            events_seen = events_seen.saturating_add(output.events.len());
            if !output.reset_path_hashes.is_empty() {
                events_replayed = events_replayed.saturating_add(output.events.len());
            }
            bytes_scanned = bytes_scanned.saturating_add(output.bytes_scanned);
            parse_issues.merge(output.parse_issues);

            let commit = writer.commit_shard(SyncShard {
                source: SourceKind::Grok,
                reset_path_hashes: output.reset_path_hashes,
                events: output.events,
                cursors: output.cursors,
                seen_file_paths: output.seen_file_paths,
                raw_records: Vec::new(),
                turns: Vec::new(),
                tool_calls: Vec::new(),
            })?;
            inserted = inserted.saturating_add(commit.events_inserted);
            write_ms = write_ms.saturating_add(commit.write_ms);
        }
    }

    let mut stats = SourceSyncStats {
        source: SourceKind::Grok,
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
        "finished Grok Build passive session sync"
    );
    Ok(stats)
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
}

fn parse_grok_session(
    plan: GrokSessionPlan,
    progress: FileProgressCounter,
    cancel: CancellationToken,
) -> Result<GrokSessionOutput> {
    let session_hash = hash_string(&plan.session_dir.to_string_lossy());
    let session_id = plan
        .session_dir
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&session_hash)
        .to_string();
    let project = project_from_session_dir(&plan.session_dir);
    let session = SessionInfo {
        session_id: session_id.clone(),
        session_label: Some(session_id.clone()),
        source_path_hash: Some(session_hash.clone()),
    };

    let mut decisions = BTreeMap::new();
    let mut output = GrokSessionOutput {
        reset_path_hashes: plan
            .had_history
            .then_some(session_hash.clone())
            .into_iter()
            .collect(),
        ..GrokSessionOutput::default()
    };
    for path in plan.files {
        if cancel.is_cancelled() {
            output.cancelled = true;
            return Ok(output);
        }
        let decision = decide_file_replay(CandidateFile {
            path: path.clone(),
            existing: None,
        })?;
        output.bytes_scanned = output
            .bytes_scanned
            .saturating_add(decision.snapshot.file_size);
        output
            .seen_file_paths
            .push(path.to_string_lossy().to_string());
        decisions.insert(
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            decision,
        );
    }

    let mut sidecar_issues = ParseIssues::default();
    let summary = decisions.get("summary.json").and_then(|decision| {
        read_json_sidecar(&decision.snapshot.path, &session_hash, &mut sidecar_issues)
    });
    let signals = decisions.get("signals.json").and_then(|decision| {
        read_json_sidecar(&decision.snapshot.path, &session_hash, &mut sidecar_issues)
    });
    let summary_timestamp_ms = summary
        .as_ref()
        .and_then(|value| value.get("updated_at").or_else(|| value.get("created_at")))
        .and_then(parse_timestamp_value)
        .unwrap_or(0);
    let signals_model = signals.as_ref().and_then(model_from_signals);
    let summary_model = summary.as_ref().and_then(|value| {
        string_at(value, &["current_model_id"]).or_else(|| string_at(value, &["model_id"]))
    });
    let fallback_model = signals_model
        .clone()
        .or(summary_model)
        .unwrap_or_else(|| UNKNOWN_MODEL.to_string());

    let updates = if let Some(decision) = decisions.get("updates.jsonl") {
        parse_updates_file(
            &decision.snapshot.path,
            &session_hash,
            &session_id,
            &session,
            project.as_ref(),
            &fallback_model,
            summary_timestamp_ms,
            &cancel,
        )?
    } else {
        UpdatesParseResult::default()
    };
    if updates.cancelled {
        output.cancelled = true;
        return Ok(output);
    }
    output.parse_issues.merge(sidecar_issues);
    output.parse_issues.merge(updates.parse_issues);
    output.events = updates.events;

    if let Some(signals) = signals.as_ref() {
        let updates_total = output.events.iter().fold(0i64, |total, event| {
            total.saturating_add(event.tokens.total_tokens)
        });
        let extra = effective_total_from_signals(signals).saturating_sub(updates_total);
        if extra > 0 {
            let model = updates
                .last_model
                .clone()
                .or(signals_model)
                .unwrap_or_else(|| fallback_model.clone());
            let timestamp_ms = updates
                .last_activity_ms
                .filter(|value| *value > 0)
                .unwrap_or(summary_timestamp_ms);
            if let Some(event) = build_event(
                format!("grok:{session_id}:signals"),
                model,
                timestamp_ms,
                extra,
                &session,
                project.as_ref(),
            ) {
                output.events.push(event);
            }
        }
    }

    let final_model = output
        .events
        .last()
        .map(|event| event.model.clone())
        .or(updates.last_model);
    for decision in decisions.values() {
        let offset = if decision
            .snapshot
            .path
            .file_name()
            .and_then(|value| value.to_str())
            == Some("updates.jsonl")
        {
            updates.end_offset
        } else {
            decision.snapshot.file_size
        };
        output.cursors.push(finalize_cursor(
            &decision.snapshot.path,
            &decision.snapshot,
            offset,
            None,
            final_model.clone(),
        ));
        progress.advance_file();
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn parse_updates_file(
    path: &Path,
    path_hash: &str,
    session_id: &str,
    session: &SessionInfo,
    project: Option<&ProjectInfo>,
    fallback_model: &str,
    fallback_timestamp_ms: i64,
    cancel: &CancellationToken,
) -> Result<UpdatesParseResult> {
    let file = File::open(path)?;
    let mut reader = BoundedJsonlReader::new(file, 0)?;
    let mut result = UpdatesParseResult::default();
    let mut current_model = fallback_model.to_string();
    let mut last_total = None;
    let mut last_total_timestamp_ms = fallback_timestamp_ms;
    let mut active_turn: Option<ActiveTurn> = None;
    let mut turn_index = 0usize;

    let status = reader.read_json_records(
        SourceKind::Grok,
        path_hash,
        cancel,
        &mut result.parse_issues,
        |record| {
            let value = record.value;
            if let Some(model) = extract_model_id(&value) {
                current_model = model;
                if let Some(turn) = active_turn.as_mut() {
                    turn.model = current_model.clone();
                }
            }
            let timestamp_ms = extract_timestamp_ms(&value).unwrap_or(fallback_timestamp_ms);
            if timestamp_ms > 0 {
                result.last_activity_ms = Some(
                    result
                        .last_activity_ms
                        .unwrap_or_default()
                        .max(timestamp_ms),
                );
            }

            if is_user_message_chunk(&value) {
                if let Some(turn) = active_turn.take()
                    && let Some(event) = turn.into_event(session_id, session, project)
                {
                    result.events.push(event);
                }
                active_turn = Some(ActiveTurn::new(
                    last_total.unwrap_or(0),
                    timestamp_ms,
                    current_model.clone(),
                    turn_index,
                ));
                turn_index = turn_index.saturating_add(1);
            }

            let Some(total_tokens) = extract_total_tokens(&value) else {
                return Ok(JsonlRecordDisposition::Ignored);
            };
            if total_tokens < 0 {
                return Ok(JsonlRecordDisposition::Ignored);
            }
            match last_total {
                Some(previous) if total_tokens < previous => {}
                Some(previous) if total_tokens == previous => {
                    last_total_timestamp_ms = timestamp_ms;
                }
                Some(previous) => {
                    if active_turn.is_none() {
                        active_turn = Some(ActiveTurn::new(
                            previous,
                            timestamp_ms,
                            current_model.clone(),
                            turn_index,
                        ));
                        turn_index = turn_index.saturating_add(1);
                    }
                    if let Some(turn) = active_turn.as_mut() {
                        turn.observe_total(total_tokens, timestamp_ms);
                    }
                    last_total_timestamp_ms = timestamp_ms;
                    last_total = Some(total_tokens);
                }
                None => {
                    if let Some(turn) = active_turn.as_mut() {
                        turn.observe_total(total_tokens, timestamp_ms);
                    }
                    last_total_timestamp_ms = timestamp_ms;
                    last_total = Some(total_tokens);
                }
            }
            Ok(JsonlRecordDisposition::Accepted)
        },
    )?;

    if let Some(turn) = active_turn
        && let Some(event) = turn.into_event(session_id, session, project)
    {
        result.events.push(event);
    }
    if result.events.is_empty()
        && let Some(total_tokens) = last_total.filter(|value| *value > 0)
        && let Some(event) = (ActiveTurn {
            baseline_total: 0,
            max_total: total_tokens,
            timestamp_ms: last_total_timestamp_ms,
            model: current_model.clone(),
            turn_index: 0,
        })
        .into_event(session_id, session, project)
    {
        result.events.push(event);
    }
    result.end_offset = reader.complete_offset();
    result.last_model = non_empty_model(&current_model);
    result.cancelled = status == JsonlReadStatus::Cancelled;
    Ok(result)
}

fn read_json_sidecar(path: &Path, path_hash: &str, issues: &mut ParseIssues) -> Option<Value> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > DEFAULT_MAX_JSONL_RECORD_BYTES as u64 {
        issues.record(SourceKind::Grok, path_hash, 0, ParseIssueKind::Oversized);
        return None;
    }
    match std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    {
        Some(value) => Some(value),
        None => {
            issues.record(SourceKind::Grok, path_hash, 0, ParseIssueKind::Malformed);
            None
        }
    }
}

fn effective_total_from_signals(value: &Value) -> i64 {
    let total = non_negative_i64(value.get("totalTokens"));
    let before = non_negative_i64(value.get("totalTokensBeforeCompaction"));
    let context = non_negative_i64(value.get("contextTokensUsed"));
    total.max(before.saturating_add(context))
}

fn model_from_signals(value: &Value) -> Option<String> {
    string_at(value, &["primaryModelId"]).or_else(|| {
        value
            .get("modelsUsed")
            .and_then(Value::as_array)
            .and_then(|models| models.first())
            .and_then(Value::as_str)
            .and_then(non_empty_model)
    })
}

fn extract_model_id(value: &Value) -> Option<String> {
    [
        &["params", "update", "_meta", "modelId"][..],
        &["params", "_meta", "modelId"][..],
        &["params", "modelId"][..],
        &["model_id"][..],
        &["modelId"][..],
        &["model"][..],
    ]
    .into_iter()
    .find_map(|path| string_at(value, path))
}

fn extract_total_tokens(value: &Value) -> Option<i64> {
    [
        &["params", "_meta", "totalTokens"][..],
        &["params", "update", "_meta", "totalTokens"][..],
        &["params", "update", "totalTokens"][..],
        &["params", "totalTokens"][..],
        &["usage", "totalTokens"][..],
        &["totalTokens"][..],
    ]
    .into_iter()
    .find_map(|path| get_path(value, path).and_then(parse_i64))
}

fn extract_timestamp_ms(value: &Value) -> Option<i64> {
    [
        &["params", "_meta", "agentTimestampMs"][..],
        &["params", "update", "_meta", "agentTimestampMs"][..],
        &["params", "timestamp"][..],
        &["timestamp"][..],
        &["ts"][..],
    ]
    .into_iter()
    .find_map(|path| get_path(value, path).and_then(parse_timestamp_value))
}

fn is_user_message_chunk(value: &Value) -> bool {
    get_path(value, &["params", "update", "sessionUpdate"]).and_then(Value::as_str)
        == Some("user_message_chunk")
}

fn parse_timestamp_value(value: &Value) -> Option<i64> {
    if let Some(raw) = value.as_str() {
        if let Ok(timestamp) = DateTime::parse_from_rfc3339(raw) {
            return Some(timestamp.timestamp_millis());
        }
        return raw.parse::<i64>().ok().and_then(normalize_unix_timestamp);
    }
    parse_i64(value).and_then(normalize_unix_timestamp)
}

fn normalize_unix_timestamp(value: i64) -> Option<i64> {
    if value <= 0 {
        None
    } else if value >= 1_000_000_000_000 {
        Some(value)
    } else {
        Some(value.saturating_mul(1_000))
    }
}

fn parse_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn non_negative_i64(value: Option<&Value>) -> i64 {
    value.and_then(parse_i64).unwrap_or_default().max(0)
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    get_path(value, path)
        .and_then(Value::as_str)
        .and_then(non_empty_model)
}

fn get_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |current, key| current.get(*key))
}

fn non_empty_model(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn build_event(
    event_key: String,
    model: String,
    timestamp_ms: i64,
    total_tokens: i64,
    session: &SessionInfo,
    project: Option<&ProjectInfo>,
) -> Option<UsageEvent> {
    let event_at = DateTime::from_timestamp_millis(timestamp_ms)
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| EPOCH_RFC3339.to_string());
    let hour_start = bucket_start_from_rfc3339(&event_at)?;
    Some(UsageEvent {
        event_key,
        source: SourceKind::Grok,
        provider_label: String::new(),
        model: non_empty_model(&model).unwrap_or_else(|| UNKNOWN_MODEL.to_string()),
        event_at,
        hour_start,
        tokens: UsageTokens {
            total_tokens,
            ..UsageTokens::default()
        },
        project: project.cloned(),
        session: Some(session.clone()),
    })
}

fn project_from_session_dir(session_dir: &Path) -> Option<ProjectInfo> {
    let encoded = session_dir.parent()?.file_name()?.to_str()?;
    let workspace = percent_decode_lossy(encoded);
    let workspace = workspace.trim();
    if workspace.is_empty() {
        return None;
    }
    let label = workspace
        .rsplit(['/', '\\'])
        .find(|value| !value.is_empty())
        .unwrap_or(workspace)
        .to_string();
    let workspace_hash = hash_string(workspace);
    Some(ProjectInfo {
        project_hash: workspace_hash.clone(),
        project_label: label,
        project_ref: None,
        repo_root_hash: workspace_hash.clone(),
        path_hash: workspace_hash,
    })
}

fn percent_decode_lossy(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            decoded.push((high << 4) | low);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn is_grok_sidecar(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| GROK_SIDECAR_NAMES.contains(&name))
}

fn sidecar_changed(path: &Path, existing: Option<&FileCursor>) -> Result<bool> {
    let Some(existing) = existing else {
        return Ok(true);
    };
    let snapshot = decide_file_replay(CandidateFile {
        path: path.to_path_buf(),
        existing: Some(existing.clone()),
    })?
    .snapshot;
    Ok(existing.offset != existing.file_size
        || existing.file_size != snapshot.file_size
        || existing.file_mtime_ns != snapshot.file_mtime_ns
        || existing.file_fingerprint != snapshot.file_fingerprint
        || existing.tail_signature != snapshot.tail_signature)
}

#[cfg(test)]
pub(super) fn bounded_contract_parse(path: &Path) -> Result<(ParseIssues, u64, bool)> {
    let session = SessionInfo {
        session_id: "bounded-contract".to_string(),
        session_label: None,
        source_path_hash: Some("bounded-contract-path-hash".to_string()),
    };
    let result = parse_updates_file(
        path,
        "bounded-contract-path-hash",
        "bounded-contract",
        &session,
        None,
        UNKNOWN_MODEL,
        0,
        &CancellationToken::new(),
    )?;
    Ok((result.parse_issues, result.end_offset, result.cancelled))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn write_session(
        updates: &str,
        summary: Option<&str>,
        signals: Option<&str>,
    ) -> (TempDir, PathBuf) {
        let temp = TempDir::new().expect("temp dir");
        let session_dir = temp
            .path()
            .join(".grok")
            .join("sessions")
            .join("D%3A%5Cwork%5Cdemo")
            .join("session-1");
        fs::create_dir_all(&session_dir).expect("create session");
        fs::write(session_dir.join("updates.jsonl"), updates).expect("write updates");
        if let Some(summary) = summary {
            fs::write(session_dir.join("summary.json"), summary).expect("write summary");
        }
        if let Some(signals) = signals {
            fs::write(session_dir.join("signals.json"), signals).expect("write signals");
        }
        (temp, session_dir)
    }

    fn parse_session(session_dir: PathBuf) -> GrokSessionOutput {
        let files = GROK_SIDECAR_NAMES
            .iter()
            .map(|name| session_dir.join(name))
            .filter(|path| path.is_file())
            .collect();
        parse_grok_session(
            GrokSessionPlan {
                session_dir,
                files,
                had_history: false,
            },
            FileProgress::new().1,
            CancellationToken::new(),
        )
        .expect("parse session")
    }

    #[tokio::test]
    async fn parses_turn_deltas_and_signals_reconciliation_as_total_only() {
        let (_temp, session_dir) = write_session(
            concat!(
                "{\"params\":{\"update\":{\"sessionUpdate\":\"available_commands_update\"},\"_meta\":{\"totalTokens\":100,\"agentTimestampMs\":1700000000000}}}\n",
                "{\"params\":{\"update\":{\"sessionUpdate\":\"user_message_chunk\",\"_meta\":{\"modelId\":\"grok-4.5\"}},\"_meta\":{\"agentTimestampMs\":1700000001000}}}\n",
                "{\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\"},\"_meta\":{\"totalTokens\":300,\"agentTimestampMs\":1700000003000}}}\n"
            ),
            Some("{\"current_model_id\":\"grok-4.5\",\"updated_at\":\"2023-11-14T22:13:20Z\"}"),
            Some("{\"primaryModelId\":\"grok-4.5\",\"contextTokensUsed\":500}"),
        );
        let output = parse_session(session_dir);

        assert_eq!(output.events.len(), 2);
        assert_eq!(output.events[0].tokens.total_tokens, 200);
        assert_eq!(output.events[1].tokens.total_tokens, 300);
        assert_eq!(output.events[1].event_key, "grok:session-1:signals");
        assert_eq!(output.events[1].event_at, "2023-11-14T22:13:23+00:00");
        assert!(output.events.iter().all(|event| {
            event.tokens.input_tokens == 0
                && event.tokens.output_tokens == 0
                && event.tokens.cache_read_tokens == 0
                && event.tokens.cache_creation_tokens == 0
                && event.tokens.reasoning_output_tokens == 0
                && event.provider_label.is_empty()
        }));
        assert_eq!(
            output.events[0]
                .project
                .as_ref()
                .map(|project| project.project_label.as_str()),
            Some("demo")
        );
    }

    #[tokio::test]
    async fn signals_only_session_uses_summary_timestamp_and_model() {
        let (_temp, session_dir) = write_session(
            "{\"timestamp\":1700000000,\"params\":{\"update\":{\"sessionUpdate\":\"hook_execution\"}}}\n",
            Some("{\"current_model_id\":\"grok-4.5\",\"updated_at\":\"2023-11-14T22:13:20Z\"}"),
            Some("{\"contextTokensUsed\":77642}"),
        );
        let output = parse_session(session_dir);

        assert_eq!(output.events.len(), 1);
        assert_eq!(output.events[0].tokens.total_tokens, 77_642);
        assert_eq!(output.events[0].model, "grok-4.5");
        assert_eq!(output.events[0].event_at, "2023-11-14T22:13:20+00:00");
    }

    #[test]
    fn signals_effective_total_uses_the_larger_rollup() {
        assert_eq!(
            effective_total_from_signals(&serde_json::json!({
                "totalTokens": 900,
                "totalTokensBeforeCompaction": 600,
                "contextTokensUsed": 50
            })),
            900
        );
        assert_eq!(
            effective_total_from_signals(&serde_json::json!({
                "totalTokens": 500,
                "totalTokensBeforeCompaction": 600,
                "contextTokensUsed": 50
            })),
            650
        );
    }

    #[tokio::test]
    async fn decreasing_counters_are_ignored() {
        let (_temp, session_dir) = write_session(
            concat!(
                "{\"timestamp\":1700000000,\"totalTokens\":100}\n",
                "{\"timestamp\":1700000001,\"totalTokens\":250}\n",
                "{\"timestamp\":1700000002,\"totalTokens\":120}\n",
                "{\"timestamp\":1700000003,\"totalTokens\":300}\n"
            ),
            None,
            None,
        );
        let output = parse_session(session_dir);

        assert_eq!(output.events.len(), 1);
        assert_eq!(output.events[0].tokens.total_tokens, 200);
        assert_eq!(output.events[0].event_at, "2023-11-14T22:13:23+00:00");
    }

    #[tokio::test]
    async fn updates_model_overrides_fallback_after_turn_start() {
        let (_temp, session_dir) = write_session(
            concat!(
                "{\"timestamp\":1700000000,\"params\":{\"update\":{\"sessionUpdate\":\"user_message_chunk\"}}}\n",
                "{\"timestamp\":1700000001,\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"_meta\":{\"modelId\":\"grok-updates\"}},\"_meta\":{\"totalTokens\":250}}}\n"
            ),
            Some("{\"current_model_id\":\"grok-summary\",\"updated_at\":\"2023-11-14T22:13:20Z\"}"),
            Some("{\"primaryModelId\":\"grok-signals\",\"contextTokensUsed\":250}"),
        );
        let output = parse_session(session_dir);

        assert_eq!(output.events.len(), 1);
        assert_eq!(output.events[0].model, "grok-updates");
    }

    #[test]
    fn sidecar_change_detection_compares_persisted_fingerprints() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join("signals.json");
        fs::write(&path, "{\"totalTokens\":100}").expect("write initial sidecar");
        let initial = decide_file_replay(CandidateFile {
            path: path.clone(),
            existing: None,
        })
        .expect("capture initial sidecar");
        let mut cursor = finalize_cursor(
            &path,
            &initial.snapshot,
            initial.snapshot.file_size,
            None,
            None,
        );

        fs::write(&path, "{\"totalTokens\":200}").expect("rewrite same-size sidecar");
        let rewritten = decide_file_replay(CandidateFile {
            path: path.clone(),
            existing: None,
        })
        .expect("capture rewritten sidecar");
        assert_eq!(cursor.file_size, rewritten.snapshot.file_size);
        cursor.file_mtime_ns = rewritten.snapshot.file_mtime_ns;

        assert!(sidecar_changed(&path, Some(&cursor)).expect("detect fingerprint change"));
    }

    #[tokio::test]
    async fn empty_session_produces_no_events() {
        let (_temp, session_dir) = write_session(
            "{\"params\":{\"update\":{\"sessionUpdate\":\"hook_execution\"}}}\n",
            Some("{\"current_model_id\":\"grok-4.5\",\"updated_at\":\"2023-11-14T22:13:20Z\"}"),
            None,
        );

        assert!(parse_session(session_dir).events.is_empty());
    }

    #[test]
    fn sidecar_over_size_cap_counts_oversized() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join("signals.json");
        let mut bytes = vec![b'{'; 1];
        bytes.extend(std::iter::repeat_n(
            b'x',
            crate::parsers::file_state::DEFAULT_MAX_JSONL_RECORD_BYTES,
        ));
        fs::write(&path, bytes).expect("write oversized sidecar");
        let mut issues = ParseIssues::default();
        assert!(read_json_sidecar(&path, "hash", &mut issues).is_none());
        assert_eq!(issues.oversized_lines, 1);
        assert_eq!(issues.malformed_lines, 0);
    }

    #[test]
    fn sidecar_bad_json_counts_malformed() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join("signals.json");
        fs::write(&path, "{not-json").expect("write bad sidecar");
        let mut issues = ParseIssues::default();
        assert!(read_json_sidecar(&path, "hash", &mut issues).is_none());
        assert_eq!(issues.malformed_lines, 1);
        assert_eq!(issues.oversized_lines, 0);
    }
}
