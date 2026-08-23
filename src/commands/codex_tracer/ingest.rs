use std::{collections::HashMap, fs::File, path::Path, time::Instant};

use anyhow::{Context, Result, bail};
use tokio_util::sync::CancellationToken;

use crate::{
    models::{ParseIssues, SourceKind},
    parsers::file_state::{
        BoundedJsonlReader, CandidateFile, FileReplayMode, JsonlReadStatus, JsonlRecordDisposition,
        decide_file_replay,
    },
    store::FileCursor,
    util::{hash_string, now_utc, read_window_signature_at},
};

use super::{
    parser::{TracerParserCheckpoint, TracerRecordParser, link_previous_next_records},
    store::{CodexTracerStore, TracerFileState},
};

const DEFAULT_BATCH_SIZE: usize = 2_048;
const SIGNATURE_WINDOW: usize = 4_096;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CodexTracerIngestOptions {
    pub batch_size: usize,
}

impl Default for CodexTracerIngestOptions {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CodexTracerIngestStats {
    pub files_seen: usize,
    pub files_changed: usize,
    pub files_skipped: usize,
    pub records_read: u64,
    pub events_found: usize,
    pub rows_written: usize,
    pub batch_peak: usize,
    pub errors: usize,
    pub ingest_ms: u64,
    pub relink_ms: u64,
}

#[derive(Debug, Default)]
struct FileIngestStats {
    changed: bool,
    skipped: bool,
    records_read: u64,
    events_found: usize,
    rows_written: usize,
    batch_peak: usize,
}

pub(crate) fn ingest_rollout_dir(
    store: &mut CodexTracerStore,
    rollout_dir: &Path,
    cancel: &CancellationToken,
    options: CodexTracerIngestOptions,
) -> Result<CodexTracerIngestStats> {
    if options.batch_size == 0 {
        bail!("Codex tracer batch size must be greater than zero");
    }

    let mut paths = walkdir::WalkDir::new(rollout_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("jsonl"))
        .collect::<Vec<_>>();
    paths.sort();

    let mut stats = CodexTracerIngestStats::default();
    let ingest_started = Instant::now();
    let mut any_change = false;
    for path in paths {
        if cancel.is_cancelled() {
            bail!("Codex tracer ingestion cancelled");
        }
        stats.files_seen = stats.files_seen.saturating_add(1);
        let path_hash = canonical_path_hash(&path);
        match ingest_file(store, &path, &path_hash, cancel, options.batch_size) {
            Ok(file_stats) => {
                any_change |= file_stats.changed;
                stats.files_changed += usize::from(file_stats.changed);
                stats.files_skipped += usize::from(file_stats.skipped);
                stats.records_read = stats.records_read.saturating_add(file_stats.records_read);
                stats.events_found = stats.events_found.saturating_add(file_stats.events_found);
                stats.rows_written = stats.rows_written.saturating_add(file_stats.rows_written);
                stats.batch_peak = stats.batch_peak.max(file_stats.batch_peak);
            }
            Err(error) => {
                any_change = true;
                stats.errors = stats.errors.saturating_add(1);
                tracing::warn!(path_hash, error = %error, "Failed to ingest Codex tracer file");
            }
        }
    }

    if cancel.is_cancelled() {
        bail!("Codex tracer ingestion cancelled");
    }
    stats.ingest_ms = ingest_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    if any_change {
        let relink_started = Instant::now();
        store
            .relink_threads()
            .context("Failed to rebuild Codex tracer thread links")?;
        stats.relink_ms = relink_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    }
    Ok(stats)
}

fn ingest_file(
    store: &mut CodexTracerStore,
    path: &Path,
    path_hash: &str,
    cancel: &CancellationToken,
    batch_size: usize,
) -> Result<FileIngestStats> {
    let existing_state = store.file_state(path_hash)?;
    let existing_cursor = existing_state.as_ref().map(state_as_cursor);
    let decision = decide_file_replay(CandidateFile {
        path: path.to_path_buf(),
        existing: existing_cursor,
    })?;

    if existing_state.is_some()
        && decision.replay_mode == FileReplayMode::Append
        && decision.start_offset == decision.snapshot.file_size
    {
        return Ok(FileIngestStats {
            skipped: true,
            ..FileIngestStats::default()
        });
    }

    let reset_file = decision.replay_mode == FileReplayMode::Reparse;
    let start_line_number = if reset_file {
        0
    } else {
        existing_state
            .as_ref()
            .map(|state| state.line_number)
            .unwrap_or_default()
    };
    let checkpoint_json = (!reset_file)
        .then(|| {
            existing_state
                .as_ref()
                .map(|state| state.parser_state_json.as_str())
        })
        .flatten();
    let mut parser = TracerRecordParser::from_checkpoint_json(path, checkpoint_json)?;
    let mut durable_checkpoint = parser.checkpoint();
    let file =
        File::open(path).with_context(|| format!("Failed to open tracer input {path_hash}"))?;
    let mut reader =
        BoundedJsonlReader::with_state(file, decision.start_offset, start_line_number)?;
    let source_file = path.to_string_lossy().to_string();
    let mut parse_issues = ParseIssues::default();
    let mut batch = Vec::with_capacity(batch_size);
    let mut batch_peak = 0usize;
    let mut events_found = 0usize;
    let mut rows_written = 0usize;
    let mut reset_pending = reset_file;
    let mut committed_offset = decision.start_offset;
    let mut committed_line = start_line_number;
    let mut durable_offset = decision.start_offset;
    let mut durable_line = start_line_number;
    let mut thread_counts = HashMap::<String, i32>::new();
    let mut thread_last = HashMap::<String, String>::new();

    let status = reader.read_json_records(
        SourceKind::Codex,
        path_hash,
        cancel,
        &mut parse_issues,
        |record| {
            // Malformed and oversized records are consumed by the shared reader
            // before this callback. The next valid record's start boundary lets
            // us include that durable progress in the pending checkpoint.
            if record.start_offset > durable_offset {
                durable_offset = record.start_offset;
                durable_line = record.line_number.saturating_sub(1);
                durable_checkpoint = parser.checkpoint();
            }

            if batch.len() >= batch_size
                || durable_line.saturating_sub(committed_line) >= batch_size as u64
            {
                let state = build_file_state(
                    path,
                    path_hash,
                    &decision.snapshot,
                    durable_offset,
                    durable_line,
                    &durable_checkpoint,
                )?;
                prepare_batch_links(&mut batch, &mut thread_counts, &mut thread_last);
                rows_written = rows_written.saturating_add(store.commit_ingest_batch(
                    &source_file,
                    &state,
                    &batch,
                    reset_pending,
                )?);
                reset_pending = false;
                batch.clear();
                committed_offset = durable_offset;
                committed_line = durable_line;
            }

            let durable = record.durable;
            let end_offset = record.end_offset;
            let line_number = record.line_number;
            if let Some(event) = parser.ingest(record)? {
                batch.push(event);
                events_found = events_found.saturating_add(1);
                batch_peak = batch_peak.max(batch.len());
            }
            if durable {
                durable_offset = end_offset;
                durable_line = line_number;
                durable_checkpoint = parser.checkpoint();
            }
            Ok(JsonlRecordDisposition::Accepted)
        },
    )?;

    if status == JsonlReadStatus::Cancelled {
        bail!("Codex tracer ingestion cancelled");
    }

    // The reader may have consumed trailing malformed/oversized records that
    // never entered the callback. They advance the durable byte/line boundary
    // without changing parser context.
    durable_offset = reader.complete_offset();
    durable_line = reader.complete_line_number();
    if durable_offset != committed_offset || reset_pending || !batch.is_empty() {
        let state = build_file_state(
            path,
            path_hash,
            &decision.snapshot,
            durable_offset,
            durable_line,
            &durable_checkpoint,
        )?;
        prepare_batch_links(&mut batch, &mut thread_counts, &mut thread_last);
        rows_written = rows_written.saturating_add(store.commit_ingest_batch(
            &source_file,
            &state,
            &batch,
            reset_pending,
        )?);
    }

    Ok(FileIngestStats {
        changed: true,
        skipped: false,
        records_read: reader.records_read(),
        events_found,
        rows_written,
        batch_peak,
    })
}

fn state_as_cursor(state: &TracerFileState) -> FileCursor {
    FileCursor {
        cursor_key: state.path_hash.clone(),
        file_path: String::new(),
        file_fingerprint: state.file_fingerprint.clone(),
        file_size: state.file_size,
        file_mtime_ns: state.file_mtime_ns,
        tail_signature: state.tail_signature.clone(),
        offset: state.durable_offset,
        last_total: None,
        last_model: None,
        updated_at: state.updated_at.clone(),
    }
}

fn prepare_batch_links(
    events: &mut [super::models::CodexTracerEvent],
    thread_counts: &mut HashMap<String, i32>,
    thread_last: &mut HashMap<String, String>,
) {
    link_previous_next_records(events);
    let mut batch_counts = HashMap::<String, i32>::new();
    let mut batch_last = HashMap::<String, String>::new();
    for event in events {
        let Some(thread_key) = event.thread_key.as_ref() else {
            continue;
        };
        let local_index = event.thread_call_index.unwrap_or_default();
        let offset = thread_counts.get(thread_key).copied().unwrap_or_default();
        event.thread_call_index = Some(offset.saturating_add(local_index));
        if local_index == 0
            && let Some(previous) = thread_last.get(thread_key)
        {
            event.previous_record_id = Some(previous.clone());
        }
        let count = batch_counts.entry(thread_key.clone()).or_default();
        *count = (*count).max(local_index.saturating_add(1));
        if event.next_record_id.is_none() {
            batch_last.insert(thread_key.clone(), event.record_id.clone());
        }
    }
    for (thread_key, count) in batch_counts {
        let total = thread_counts.entry(thread_key.clone()).or_default();
        *total = total.saturating_add(count);
        if let Some(last) = batch_last.remove(&thread_key) {
            thread_last.insert(thread_key, last);
        }
    }
}

fn build_file_state(
    path: &Path,
    path_hash: &str,
    snapshot: &crate::parsers::file_state::FileSnapshot,
    durable_offset: u64,
    line_number: u64,
    checkpoint: &TracerParserCheckpoint,
) -> Result<TracerFileState> {
    Ok(TracerFileState {
        path_hash: path_hash.to_string(),
        file_fingerprint: snapshot.file_fingerprint.clone(),
        file_size: snapshot.file_size,
        file_mtime_ns: snapshot.file_mtime_ns,
        tail_signature: read_window_signature_at(path, durable_offset, SIGNATURE_WINDOW)?,
        durable_offset,
        line_number,
        parser_state_json: checkpoint.to_json()?,
        updated_at: now_utc(),
    })
}

fn canonical_path_hash(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let normalized = canonical.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    let normalized = normalized.to_ascii_lowercase();
    hash_string(&normalized)
}

#[cfg(test)]
mod tests {
    use std::{hint::black_box, io::Write, time::Instant};

    use anyhow::Result;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::commands::codex_tracer::store::CallFilters;

    fn append_turn(
        file: &mut std::fs::File,
        session_id: &str,
        turn: usize,
        cumulative_total: i64,
    ) -> Result<()> {
        if turn == 1 {
            writeln!(
                file,
                "{}",
                json!({
                    "timestamp": "2026-08-24T00:00:00Z",
                    "type": "session_meta",
                    "payload": {"id": session_id, "thread_source": "main"}
                })
            )?;
        }
        writeln!(
            file,
            "{}",
            json!({
                "timestamp": format!("2026-08-24T00:{turn:02}:00Z"),
                "type": "turn_context",
                "payload": {
                    "turn_id": format!("turn-{turn}"),
                    "cwd": "C:/workspace/project",
                    "model": "gpt-test",
                    "effort": "high"
                }
            })
        )?;
        writeln!(
            file,
            "{}",
            json!({
                "timestamp": format!("2026-08-24T00:{turn:02}:30Z"),
                "type": "event_msg",
                "payload": {
                    "type": "token_count",
                    "info": {
                        "last_token_usage": {
                            "input_tokens": 10,
                            "cached_input_tokens": 2,
                            "output_tokens": 5,
                            "reasoning_output_tokens": 1,
                            "total_tokens": 15
                        },
                        "total_token_usage": {
                            "input_tokens": cumulative_total - 5,
                            "cached_input_tokens": 2,
                            "output_tokens": 5,
                            "reasoning_output_tokens": 1,
                            "total_tokens": cumulative_total
                        },
                        "model_context_window": 1000
                    }
                }
            })
        )?;
        Ok(())
    }

    fn serialized_calls(store: &CodexTracerStore) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(store.query_calls(&CallFilters {
            include_archived: true,
            ..CallFilters::default()
        })?)?)
    }

    #[test]
    fn fresh_unchanged_append_and_replace_match_clean_rebuilds() -> Result<()> {
        let temp = TempDir::new()?;
        let rollout = temp.path().join("rollout");
        std::fs::create_dir_all(&rollout)?;
        let source = rollout.join("rollout-session.jsonl");
        let mut file = std::fs::File::create(&source)?;
        append_turn(&mut file, "session-a", 1, 15)?;
        drop(file);

        let mut store = CodexTracerStore::open(&temp.path().join("incremental.db"))?;
        let options = CodexTracerIngestOptions { batch_size: 1 };
        let first = ingest_rollout_dir(&mut store, &rollout, &CancellationToken::new(), options)?;
        assert_eq!(first.files_changed, 1);
        assert_eq!(first.events_found, 1);
        assert!(first.batch_peak <= options.batch_size);

        let unchanged =
            ingest_rollout_dir(&mut store, &rollout, &CancellationToken::new(), options)?;
        assert_eq!(unchanged.files_skipped, 1);
        assert_eq!(unchanged.records_read, 0);

        let mut file = std::fs::OpenOptions::new().append(true).open(&source)?;
        append_turn(&mut file, "session-a", 2, 30)?;
        drop(file);
        let append = ingest_rollout_dir(&mut store, &rollout, &CancellationToken::new(), options)?;
        assert_eq!(append.files_changed, 1);
        assert_eq!(append.events_found, 1);
        assert_eq!(store.count_events()?, 2);

        let mut clean = CodexTracerStore::open(&temp.path().join("clean-append.db"))?;
        ingest_rollout_dir(&mut clean, &rollout, &CancellationToken::new(), options)?;
        assert_eq!(serialized_calls(&store)?, serialized_calls(&clean)?);

        let mut file = std::fs::File::create(&source)?;
        append_turn(&mut file, "session-b", 1, 20)?;
        drop(file);
        let replace = ingest_rollout_dir(&mut store, &rollout, &CancellationToken::new(), options)?;
        assert_eq!(replace.files_changed, 1);
        assert_eq!(store.count_events()?, 1);

        let mut clean = CodexTracerStore::open(&temp.path().join("clean-replace.db"))?;
        ingest_rollout_dir(&mut clean, &rollout, &CancellationToken::new(), options)?;
        assert_eq!(serialized_calls(&store)?, serialized_calls(&clean)?);
        Ok(())
    }

    #[test]
    fn cancelled_run_leaves_prior_boundary_retryable() -> Result<()> {
        let temp = TempDir::new()?;
        let rollout = temp.path().join("rollout");
        std::fs::create_dir_all(&rollout)?;
        let source = rollout.join("rollout-session.jsonl");
        let mut file = std::fs::File::create(source)?;
        append_turn(&mut file, "session-a", 1, 15)?;
        drop(file);
        let mut store = CodexTracerStore::open(&temp.path().join("tracer.db"))?;
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(
            ingest_rollout_dir(
                &mut store,
                &rollout,
                &cancel,
                CodexTracerIngestOptions::default(),
            )
            .is_err()
        );
        assert_eq!(store.count_events()?, 0);

        let retry = ingest_rollout_dir(
            &mut store,
            &rollout,
            &CancellationToken::new(),
            CodexTracerIngestOptions::default(),
        )?;
        assert_eq!(retry.events_found, 1);
        assert_eq!(store.count_events()?, 1);
        Ok(())
    }

    #[test]
    fn oversized_record_stays_bounded_and_does_not_hide_later_usage() -> Result<()> {
        let temp = TempDir::new()?;
        let rollout = temp.path().join("rollout");
        std::fs::create_dir_all(&rollout)?;
        let source = rollout.join("rollout-session.jsonl");
        let mut file = std::fs::File::create(&source)?;
        file.write_all(&vec![b'x'; 10 * 1024 * 1024])?;
        writeln!(file)?;
        append_turn(&mut file, "session-a", 1, 15)?;
        drop(file);

        let mut store = CodexTracerStore::open(&temp.path().join("tracer.db"))?;
        let stats = ingest_rollout_dir(
            &mut store,
            &rollout,
            &CancellationToken::new(),
            CodexTracerIngestOptions { batch_size: 1 },
        )?;
        assert_eq!(stats.records_read, 4);
        assert_eq!(stats.events_found, 1);
        assert_eq!(stats.batch_peak, 1);
        assert_eq!(store.count_events()?, 1);
        Ok(())
    }

    #[test]
    fn thread_links_converge_across_files_and_batches() -> Result<()> {
        let temp = TempDir::new()?;
        let rollout = temp.path().join("rollout");
        std::fs::create_dir_all(&rollout)?;
        for (name, turn, cumulative) in [("a.jsonl", 1, 15), ("b.jsonl", 1, 30)] {
            let mut file = std::fs::File::create(rollout.join(name))?;
            append_turn(&mut file, "shared-session", turn, cumulative)?;
        }

        let mut store = CodexTracerStore::open(&temp.path().join("tracer.db"))?;
        ingest_rollout_dir(
            &mut store,
            &rollout,
            &CancellationToken::new(),
            CodexTracerIngestOptions { batch_size: 1 },
        )?;
        let mut calls = store.query_calls(&CallFilters {
            include_archived: true,
            ..CallFilters::default()
        })?;
        calls.sort_by_key(|event| event.thread_call_index);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].thread_call_index, Some(0));
        assert_eq!(calls[1].thread_call_index, Some(1));
        assert_eq!(calls[0].next_record_id, Some(calls[1].record_id.clone()));
        assert_eq!(
            calls[1].previous_record_id,
            Some(calls[0].record_id.clone())
        );
        Ok(())
    }

    fn write_performance_corpus(path: &Path, event_count: usize) -> Result<()> {
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        writeln!(
            file,
            "{}",
            json!({
                "timestamp": "2026-08-24T00:00:00Z",
                "type": "session_meta",
                "payload": {"id": "performance-session", "thread_source": "main"}
            })
        )?;
        writeln!(
            file,
            "{}",
            json!({
                "timestamp": "2026-08-24T00:00:01Z",
                "type": "turn_context",
                "payload": {"turn_id": "performance-turn", "model": "gpt-test"}
            })
        )?;
        for index in 0..event_count {
            let cumulative = i64::try_from(index + 1).unwrap_or(i64::MAX) * 15;
            writeln!(
                file,
                "{{\"timestamp\":\"2026-08-24T00:00:02.{index:06}Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"last_token_usage\":{{\"input_tokens\":10,\"cached_input_tokens\":2,\"output_tokens\":5,\"reasoning_output_tokens\":1,\"total_tokens\":15}},\"total_token_usage\":{{\"input_tokens\":{},\"cached_input_tokens\":2,\"output_tokens\":5,\"reasoning_output_tokens\":1,\"total_tokens\":{}}},\"model_context_window\":1000000}}}}}}",
                cumulative - 5,
                cumulative
            )?;
        }
        file.flush()?;
        Ok(())
    }

    #[cfg(windows)]
    fn peak_working_set_bytes() -> usize {
        use std::ffi::c_void;

        #[repr(C)]
        struct ProcessMemoryCounters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut c_void;
        }
        #[link(name = "psapi")]
        unsafe extern "system" {
            fn GetProcessMemoryInfo(
                process: *mut c_void,
                counters: *mut ProcessMemoryCounters,
                size: u32,
            ) -> i32;
        }

        let mut counters = ProcessMemoryCounters {
            cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        // SAFETY: Windows owns the pseudo handle and writes exactly the
        // PROCESS_MEMORY_COUNTERS-sized buffer supplied above.
        let success =
            unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
        if success == 0 {
            0
        } else {
            counters.peak_working_set_size
        }
    }

    #[cfg(not(windows))]
    fn peak_working_set_bytes() -> usize {
        0
    }

    fn sqlite_bytes(path: &Path) -> u64 {
        ["", "-wal", "-shm"]
            .into_iter()
            .filter_map(|suffix| std::fs::metadata(format!("{}{suffix}", path.display())).ok())
            .map(|metadata| metadata.len())
            .sum()
    }

    #[test]
    #[ignore = "explicit synthetic 100k performance harness"]
    fn codex_tracer_100k_collector_baseline() -> Result<()> {
        let temp = TempDir::new()?;
        let source = temp.path().join("rollout.jsonl");
        write_performance_corpus(&source, 100_000)?;
        let db_path = temp.path().join("collector.db");
        let mut store = CodexTracerStore::open(&db_path)?;
        let started = Instant::now();
        let events = super::super::parser::parse_codex_jsonl_for_tracer(&source)?;
        let parse_ms = started.elapsed().as_millis();
        let batch_peak = events.len();
        let write_started = Instant::now();
        let rows_written = store.upsert_events(&events)?;
        let write_ms = write_started.elapsed().as_millis();
        let relink_started = Instant::now();
        store.relink_threads()?;
        let relink_ms = relink_started.elapsed().as_millis();
        black_box(&events);
        let elapsed = started.elapsed();
        let peak_rss = peak_working_set_bytes();
        let live_db_bytes = sqlite_bytes(&db_path);
        drop(store);
        let final_db_bytes = sqlite_bytes(&db_path);
        println!(
            "PERF collector events={} rows={} batch_peak={} wall_ms={} parse_ms={} write_ms={} relink_ms={} peak_rss_bytes={} live_db_bytes={} final_db_bytes={}",
            events.len(),
            rows_written,
            batch_peak,
            elapsed.as_millis(),
            parse_ms,
            write_ms,
            relink_ms,
            peak_rss,
            live_db_bytes,
            final_db_bytes,
        );
        Ok(())
    }

    #[test]
    #[ignore = "explicit synthetic 100k performance harness"]
    fn codex_tracer_100k_streaming() -> Result<()> {
        let temp = TempDir::new()?;
        let rollout = temp.path().join("rollout");
        std::fs::create_dir_all(&rollout)?;
        write_performance_corpus(&rollout.join("rollout.jsonl"), 100_000)?;
        let db_path = temp.path().join("streaming.db");
        let mut store = CodexTracerStore::open(&db_path)?;
        let started = Instant::now();
        let stats = ingest_rollout_dir(
            &mut store,
            &rollout,
            &CancellationToken::new(),
            CodexTracerIngestOptions::default(),
        )?;
        let elapsed = started.elapsed();
        let warm_started = Instant::now();
        let warm = ingest_rollout_dir(
            &mut store,
            &rollout,
            &CancellationToken::new(),
            CodexTracerIngestOptions::default(),
        )?;
        let warm_ms = warm_started.elapsed().as_millis();
        assert_eq!(warm.records_read, 0);
        assert_eq!(warm.rows_written, 0);
        let peak_rss = peak_working_set_bytes();
        let live_db_bytes = sqlite_bytes(&db_path);
        drop(store);
        let final_db_bytes = sqlite_bytes(&db_path);
        println!(
            "PERF streaming events={} rows={} batch_peak={} wall_ms={} ingest_ms={} relink_ms={} warm_ms={} peak_rss_bytes={} live_db_bytes={} final_db_bytes={}",
            stats.events_found,
            stats.rows_written,
            stats.batch_peak,
            elapsed.as_millis(),
            stats.ingest_ms,
            stats.relink_ms,
            warm_ms,
            peak_rss,
            live_db_bytes,
            final_db_bytes,
        );
        Ok(())
    }
}
