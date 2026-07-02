//! Integration tests for the `source_file` state machine (D15 / ADR 0006).
//!
//! Verifies that the three persistence entries the state machine exposes
//! (commit_shard's `seen_file_paths`, the driver's `sweep_missing`, and
//! `Store::mark_source_file_deleted`) compose into the documented
//! live / missing / deleted_by_user transitions, and that
//! `Dashboard::diagnostics` reports them coherently.

use anyhow::Result;
use llmusage::{
    AppPaths, Dashboard, QueryFilter,
    models::{SourceKind, UsageEvent, UsageTokens},
    store::{Store, SyncShard},
};
use tempfile::TempDir;

fn make_store() -> Result<(TempDir, Store)> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().to_path_buf())?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    Ok((temp, store))
}

fn build_event(suffix: &str, path_hash: &str) -> UsageEvent {
    UsageEvent {
        event_key: format!("codex:{path_hash}:{suffix}"),
        source: SourceKind::Codex,
        provider_label: String::new(),
        model: "gpt-5".to_string(),
        event_at: "2026-05-08T00:00:00Z".to_string(),
        hour_start: "2026-05-08T00:00:00Z".to_string(),
        tokens: UsageTokens {
            input_tokens: 1,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 1,
            reasoning_output_tokens: 0,
            total_tokens: 2,
        },
        project: None,
        session: None,
    }
}

/// Validates that three real persistence entries (commit_shard upsert,
/// driver sweep, user-forget mark) compose into the three documented
/// state-machine end states without race or double-counting.
#[tokio::test]
async fn three_entries_lead_to_consistent_state() -> Result<()> {
    let (_tmp, store) = make_store()?;

    // Run 1: parser saw three files; commit_shard registers them as live.
    {
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            reset_path_hashes: Vec::new(),
            events: vec![build_event("e1", "p1")],
            cursors: Vec::new(),
            seen_file_paths: vec![
                "/codex/a.jsonl".to_string(),
                "/codex/b.jsonl".to_string(),
                "/codex/c.jsonl".to_string(),
            ],
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
        })?;
        writer.finish_sync_run()?;
    }

    let counts = store.source_files().counts(SourceKind::Codex)?;
    assert_eq!(counts.live, 3);
    assert_eq!(counts.missing, 0);
    assert_eq!(counts.deleted, 0);

    // Run 2: parser only saw two files; driver sweeps the third to missing.
    {
        let mut writer = store.begin_sync_run()?;
        let run_started_at = writer.run_started_at().to_string();
        writer.commit_shard(SyncShard {
            source: SourceKind::Codex,
            reset_path_hashes: Vec::new(),
            events: Vec::new(),
            cursors: Vec::new(),
            seen_file_paths: vec!["/codex/a.jsonl".to_string(), "/codex/b.jsonl".to_string()],
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
        })?;
        writer.finish_sync_run()?;

        let swept = store
            .source_files()
            .sweep_missing(SourceKind::Codex, &run_started_at)?;
        assert_eq!(swept, 1, "/codex/c.jsonl should flip to missing");
    }

    let counts = store.source_files().counts(SourceKind::Codex)?;
    assert_eq!(counts.live, 2);
    assert_eq!(counts.missing, 1);
    assert_eq!(counts.deleted, 0);

    // Third entry: the user forgets one of the live files explicitly.
    store.mark_source_file_deleted(SourceKind::Codex, "/codex/a.jsonl")?;

    let counts = store.source_files().counts(SourceKind::Codex)?;
    assert_eq!(counts.live, 1);
    assert_eq!(counts.missing, 1);
    assert_eq!(counts.deleted, 1);

    // Dashboard::diagnostics surfaces the same numbers, plus archive_root.
    let diagnostics = Dashboard::open(&store)?.diagnostics()?;
    assert_eq!(
        diagnostics.archive_root,
        store.paths.root_dir.display().to_string()
    );
    let codex_row = diagnostics
        .by_source
        .iter()
        .find(|row| row.source == "codex")
        .expect("codex row should be present");
    assert_eq!(codex_row.live_files, 1);
    assert_eq!(codex_row.missing_files, 1);
    assert_eq!(codex_row.deleted_files, 1);

    let payload = Dashboard::open(&store)?.home_overview(&QueryFilter::default())?;
    assert_eq!(payload.archive.by_source.len(), 1);
    assert_eq!(payload.archive.by_source[0].source, "codex");
    Ok(())
}

/// Validates the resurrect rule: a file the user explicitly forgot returns
/// to `live` the next time the parser observes it on disk.
#[tokio::test]
async fn deleted_then_seen_again_resurrects_to_live() -> Result<()> {
    let (_tmp, store) = make_store()?;

    // First run: file lands as live.
    {
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(SyncShard {
            source: SourceKind::Claude,
            reset_path_hashes: Vec::new(),
            events: Vec::new(),
            cursors: Vec::new(),
            seen_file_paths: vec!["/claude/proj/log.jsonl".to_string()],
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
        })?;
        writer.finish_sync_run()?;
    }
    assert_eq!(store.source_files().counts(SourceKind::Claude)?.live, 1);

    // User forgets the file via the diagnostics entry.
    store.mark_source_file_deleted(SourceKind::Claude, "/claude/proj/log.jsonl")?;
    let counts = store.source_files().counts(SourceKind::Claude)?;
    assert_eq!(counts.live, 0);
    assert_eq!(counts.deleted, 1);

    // Second run: the parser sees the same file on disk again.
    {
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(SyncShard {
            source: SourceKind::Claude,
            reset_path_hashes: Vec::new(),
            events: Vec::new(),
            cursors: Vec::new(),
            seen_file_paths: vec!["/claude/proj/log.jsonl".to_string()],
            raw_records: Vec::new(),
            turns: Vec::new(),
            tool_calls: Vec::new(),
        })?;
        writer.finish_sync_run()?;
    }

    let counts = store.source_files().counts(SourceKind::Claude)?;
    assert_eq!(counts.live, 1);
    assert_eq!(counts.deleted, 0, "deleted_by_user must resurrect to live");
    Ok(())
}
