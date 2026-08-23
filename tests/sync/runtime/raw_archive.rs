use super::*;

#[test]
fn raw_archive_off_by_default() -> Result<()> {
    let (_tmp, store) = make_store()?;
    assert!(!store.raw_archive_enabled()?);

    let mut writer = store.begin_sync_run()?;
    writer.commit_shard(SyncShard {
        source: SourceKind::Codex,
        reset_path_hashes: Vec::new(),
        events: vec![build_event("codex:raw-off", "2026-05-08T00:00:00Z", 10)],
        cursors: Vec::new(),
        seen_file_paths: Vec::new(),
        raw_records: vec![RawRecord {
            event_key: "codex:raw-off".to_string(),
            raw_json: r#"{"secret":"local-only"}"#.to_string(),
        }],
        turns: Vec::new(),
        tool_calls: Vec::new(),
        ..SyncShard::new(SourceKind::Codex)
    })?;
    writer.finish_sync_run()?;

    let conn = store.open_connection()?;
    let raw_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM usage_event_raw", [], |row| row.get(0))?;
    assert_eq!(raw_count, 0);
    Ok(())
}

#[test]
fn raw_archive_opt_in_is_returned_by_logs() -> Result<()> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap_with(BootstrapOptions::default().with_raw_archive(true))?;
    assert!(store.raw_archive_enabled()?);

    let mut writer = store.begin_sync_run()?;
    writer.commit_shard(SyncShard {
        source: SourceKind::Codex,
        reset_path_hashes: Vec::new(),
        events: vec![build_event("codex:raw-on", "2026-05-08T01:00:00Z", 11)],
        cursors: Vec::new(),
        seen_file_paths: Vec::new(),
        raw_records: vec![RawRecord {
            event_key: "codex:raw-on".to_string(),
            raw_json: r#"{"payload":"retained"}"#.to_string(),
        }],
        turns: Vec::new(),
        tool_calls: Vec::new(),
        ..SyncShard::new(SourceKind::Codex)
    })?;
    writer.finish_sync_run()?;

    let page = Dashboard::open(&store)?.logs(&llmusage::LogsQuery {
        include_raw_json: true,
        ..Default::default()
    })?;
    assert_eq!(page.records.len(), 1);
    assert_eq!(
        page.records[0].raw_json.as_deref(),
        Some(r#"{"payload":"retained"}"#)
    );
    Ok(())
}

#[test]
fn logs_cursor_round_trip() -> Result<()> {
    let (_tmp, store) = make_store()?;
    let mut writer = store.begin_sync_run()?;
    writer.commit_shard(SyncShard {
        source: SourceKind::Codex,
        reset_path_hashes: Vec::new(),
        events: vec![
            build_event("codex:old", "2026-05-08T00:00:00Z", 1),
            build_event("codex:middle", "2026-05-08T01:00:00Z", 2),
            build_event("codex:new", "2026-05-08T02:00:00Z", 3),
        ],
        cursors: Vec::new(),
        seen_file_paths: Vec::new(),
        raw_records: Vec::new(),
        turns: Vec::new(),
        tool_calls: Vec::new(),
        ..SyncShard::new(SourceKind::Codex)
    })?;
    writer.finish_sync_run()?;

    let dashboard = Dashboard::open(&store)?;
    let first = dashboard.logs(&llmusage::LogsQuery {
        filter: QueryFilter {
            source: Some(SourceKind::Codex),
            ..Default::default()
        },
        page_size: 2,
        include_total: true,
        ..Default::default()
    })?;
    assert_eq!(first.total, Some(3));
    assert_eq!(
        first
            .records
            .iter()
            .map(|record| record.event_key.as_str())
            .collect::<Vec<_>>(),
        vec!["local:codex:new", "local:codex:middle"]
    );
    let cursor = first.next_cursor.expect("first page should have cursor");
    let decoded: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&cursor)?)?;
    assert_eq!(decoded["event_at"], "2026-05-08T01:00:00Z");
    assert_eq!(decoded["event_key"], "local:codex:middle");

    let second = dashboard.logs(&llmusage::LogsQuery {
        filter: QueryFilter {
            source: Some(SourceKind::Codex),
            ..Default::default()
        },
        page_size: 2,
        cursor: Some(cursor),
        ..Default::default()
    })?;
    assert_eq!(second.next_cursor, None);
    assert_eq!(second.records.len(), 1);
    assert_eq!(second.records[0].event_key, "local:codex:old");
    Ok(())
}

#[tokio::test]
async fn opencode_row_serialized_as_json_in_raw_table() -> Result<()> {
    let fixture = OpencodeFixture::new()?;
    fixture.seed_opencode("msg-raw", 1776823200000, 64)?;

    let store = Store::new(&fixture.paths)?;
    store.bootstrap_with(BootstrapOptions::default().with_raw_archive(true))?;
    let app = llmusage::app::AppContext {
        paths: fixture.paths.clone(),
        current_exe: std::env::current_exe()?,
    };
    llmusage::commands::sync::run(&app).await?;

    let conn = store.open_connection()?;
    let raw_json: String = conn.query_row(
        "SELECT raw_json FROM usage_event_raw WHERE event_key = 'local:opencode:msg-raw'",
        [],
        |row| row.get(0),
    )?;
    let value: serde_json::Value = serde_json::from_str(&raw_json)?;
    assert_eq!(value["id"], "msg-raw");
    assert_eq!(value["session_id"], "session-1");
    assert_eq!(value["data"]["modelID"], "gpt-5");
    Ok(())
}
