use super::*;

#[test]
fn default_ccr_provider_map_labels_sync_and_rebuild() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex(
        "rollout-provider-default.jsonl",
        120,
        "2026-04-22T01:12:00Z",
    )?;
    fixture.write_provider_map(
        r#"{"platform":"codex","provider":"anyrouter","activated_at":"2026-04-22T01:00:00Z","event":"activate"}"#,
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;

        let first = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(first.total_inserted, 1);
        assert_provider_label(&app.paths.db_path, "anyrouter")?;

        fixture.write_provider_map(
            r#"{"platform":"codex","provider":"methink","activated_at":"2026-04-22T01:00:00Z","event":"activate"}"#,
        )?;
        let rebuilt = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(rebuilt.total_inserted, 1);
        assert_provider_label(&app.paths.db_path, "methink")?;

        let explicit_map = fixture.home.join("explicit-provider-map.jsonl");
        fs::write(
            &explicit_map,
            r#"{"platform":"codex","provider":"glm","activated_at":"2026-04-22T01:00:00Z","event":"activate"}"#,
        )?;
        let rebuilt_explicit = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::Codex),
                provider_map: Some(explicit_map),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(rebuilt_explicit.total_inserted, 1);
        assert_provider_label(&app.paths.db_path, "glm")?;

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn source_breakdown_matches_bucket_totals() -> Result<()> {
    /*
     * ========================================================================
     * 步骤3：验证来源汇总与 bucket 总量一致
     * ========================================================================
     * 目标：
     * 1) 走一轮全量 sync
     * 2) 校验 source breakdown 不再被 join 放大
     * 3) 汇总值必须与 overview 总量一致
     */
    let fixture = Fixture::new()?;
    fixture.seed_codex("rollout-test.jsonl", 120, "2026-04-22T01:12:00Z")?;
    fixture.seed_claude("session.jsonl", 90, "2026-04-22T02:00:00Z")?;
    fixture.seed_opencode("msg-1", 1776823200000, 64)?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::sync::run(&app).await?;

        let store = Store::new(&app.paths)?;
        let dashboard = Dashboard::open(&store)?;
        let overview = dashboard.overview(&Default::default())?;
        let sources = dashboard.source_breakdown(&Default::default())?;
        let total_from_sources = sources.iter().map(|item| item.total_tokens).sum::<i64>();
        assert_eq!(overview.total.total_tokens, total_from_sources);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn historical_hook_rows_and_holder_kind_remain_read_compatible() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = AppContext::discover()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let conn = store.open_connection()?;
    conn.execute(
        "INSERT INTO trigger_state (
            source, last_signal_at, trigger, last_worker_started_at,
            last_worker_finished_at, updated_at
         ) VALUES ('codex', '2026-01-01T00:00:00Z', 'Stop', NULL, NULL, '2026-01-01T00:00:00Z')",
        [],
    )?;
    conn.execute(
        "INSERT INTO integration_install (
            source, install_type, status, config_path, backup_path, details_json, updated_at
         ) VALUES ('codex', 'init', 'ready', NULL, NULL, '{}', '2026-01-01T00:00:00Z')",
        [],
    )?;
    conn.execute(
        "INSERT INTO run_log (
            command, started_at, finished_at, status, summary, error, duration_ms
         ) VALUES ('hook-run', '2026-01-02T00:00:00Z', '2026-01-02T00:00:01Z',
                   'success', NULL, NULL, 1000)",
        [],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO worker_lock (
            lock_name, owner_id, lease_expires_at, holder_pid, holder_kind,
            acquired_at, updated_at, generation
         ) VALUES ('sync-worker', 'legacy-owner', '2000-01-01T00:00:00Z',
                   1, 'hook', '1999-12-31T23:59:00Z', '1999-12-31T23:59:00Z', 1)",
        [],
    )?;
    drop(conn);

    let overview = Dashboard::open(&store)?.overview(&Default::default())?;
    assert_eq!(
        overview.last_sync_at.as_deref(),
        Some("2026-01-02T00:00:01Z")
    );
    assert!(store.current_worker_lock()?.is_none());
    let conn = store.open_connection()?;
    let trigger_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM trigger_state", [], |row| row.get(0))?;
    let integration_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM integration_install", [], |row| {
            row.get(0)
        })?;
    assert_eq!(trigger_count, 1);
    assert_eq!(integration_count, 1);

    fixture.restore_env();
    Ok(())
}

#[test]
fn bootstrap_migrates_legacy_usage_event_before_session_index() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = AppContext::discover()?;
    fs::create_dir_all(&app.paths.root_dir)?;
    let conn = Connection::open(&app.paths.db_path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE usage_event (
            event_key TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            model TEXT NOT NULL,
            event_at TEXT NOT NULL,
            hour_start TEXT NOT NULL,
            input_tokens INTEGER NOT NULL,
            cached_input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            reasoning_output_tokens INTEGER NOT NULL,
            total_tokens INTEGER NOT NULL,
            project_hash TEXT,
            project_label TEXT,
            project_ref TEXT,
            path_hash TEXT,
            created_at TEXT NOT NULL
        );
        INSERT INTO usage_event(
            event_key, source, model, event_at, hour_start,
            input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens,
            project_hash, project_label, project_ref, path_hash, created_at
        ) VALUES (
            'legacy-event', 'codex', 'gpt-5', '2026-05-05T12:00:00Z', '2026-05-05T12:00:00Z',
            10, 0, 5, 0, 15,
            'project-hash', 'demo', 'example/demo', 'path-hash', '2026-05-05T12:00:00Z'
        );
        "#,
    )?;
    drop(conn);

    let store = Store::new(&app.paths)?;
    store.bootstrap()?;

    let conn = Connection::open(&app.paths.db_path)?;
    let columns = table_columns(&conn, "usage_event")?;
    assert!(columns.iter().any(|column| column == "session_id"));
    assert!(columns.iter().any(|column| column == "session_label"));
    assert!(columns.iter().any(|column| column == "source_path_hash"));
    assert_eq!(usage_event_count(&app.paths.db_path)?, 1);
    assert_eq!(
        llmusage::store::read_schema_version(&conn)?,
        llmusage::store::latest_schema_version()
    );
    assert!(
        app.paths
            .backups_dir
            .join("llmusage.db.pre-0.5.0")
            .is_file(),
        "v0 bootstrap should keep a pre-0.5.0 backup"
    );

    let session_index_count = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM sqlite_master
        WHERE type = 'index' AND name = 'idx_usage_event_session'
        "#,
        [],
        |row| row.get::<_, i64>(0),
    )?;
    assert_eq!(session_index_count, 1);

    fixture.restore_env();
    Ok(())
}

#[test]
fn source_sync_stats_absent_wire_contract_is_backward_compatible() -> Result<()> {
    let default_value = serde_json::to_value(SourceSyncStats {
        source: SourceKind::Opencode,
        ..SourceSyncStats::default()
    })?;
    assert_eq!(default_value["absent"], false);
    assert_eq!(default_value["skipped_files"], 0);
    assert_eq!(default_value["parse_issues"]["malformed_lines"], 0);
    assert_eq!(default_value["parse_issues"]["oversized_lines"], 0);
    assert_eq!(default_value["parse_issues"]["skipped_lines"], 0);
    assert_eq!(default_value["parse_issues"]["accounting_anomaly_lines"], 0);

    let absent_value = serde_json::to_value(SourceSyncStats {
        source: SourceKind::Opencode,
        absent: true,
        last_error: Some("OpenCode SQLite DB 缺失".to_string()),
        ..SourceSyncStats::default()
    })?;
    assert_eq!(absent_value["absent"], true);

    let legacy_json = serde_json::json!({
        "source": "opencode",
        "files_processed": 0,
        "changed_files": 0,
        "bytes_scanned": 0,
        "events_seen": 0,
        "events_replayed": 0,
        "events_inserted": 0,
        "parse_ms": 0,
        "write_ms": 0,
        "lock_wait_ms": 0,
        "last_error": "OpenCode SQLite DB 缺失"
    });
    let legacy_stats: SourceSyncStats = serde_json::from_value(legacy_json)?;
    assert!(!legacy_stats.absent);
    assert_eq!(legacy_stats.skipped_files, 0);
    assert_eq!(legacy_stats.parse_issues, Default::default());
    assert_eq!(legacy_stats.source, SourceKind::Opencode);
    assert_eq!(
        legacy_stats.last_error.as_deref(),
        Some("OpenCode SQLite DB 缺失")
    );
    Ok(())
}
