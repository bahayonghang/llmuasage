use super::super::*;

#[test]
fn zcode_sync_twice_is_idempotent() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("row-a", 1_000, 100, 40))?;
    fixture.insert_zcode_row(zcode_row("row-b", 2_000, 200, 60))?;

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
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(first.total_inserted, 2);

        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(second.total_inserted, 0);
        assert_eq!(second.sources[0].changed_files, 0);
        assert_eq!(second.sources[0].skipped_files, 1);
        assert_eq!(zcode_source_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_append_imports_only_new_rows() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("row-a", 1_000, 100, 40))?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;

        fixture.insert_zcode_row(zcode_row("row-b", 2_000, 200, 60))?;
        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(second.total_inserted, 1);
        assert_eq!(zcode_source_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_late_completing_request_is_not_missed() -> Result<()> {
    let fixture = Fixture::new()?;
    // Row A starts early but is still running (no completed_at).
    let mut row_a = zcode_row("row-a", 5_000, 100, 40);
    row_a.started_at = 1_000;
    row_a.status = "running";
    row_a.completed_at = 0;
    fixture.insert_zcode_row(row_a)?;
    // Row B starts and completes later, advancing the watermark to 2_000.
    let mut row_b = zcode_row("row-b", 2_000, 200, 60);
    row_b.started_at = 1_500;
    fixture.insert_zcode_row(row_b)?;

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
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(first.total_inserted, 1, "only the completed row B imports");

        // Row A finishes after the watermark advanced.
        let conn = Connection::open(fixture.zcode_db_path())?;
        conn.execute(
            "UPDATE model_usage SET status = 'completed', completed_at = 6_000 WHERE id = 'row-a'",
            [],
        )?;
        drop(conn);

        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            second.total_inserted, 1,
            "late-completing row A must import"
        );
        assert_eq!(zcode_source_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_skips_error_and_cancelled_rows_and_counts_them() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("ok-1", 1_000, 100, 40))?;
    let mut error_row = zcode_row("err-1", 2_000, 0, 0);
    error_row.status = "error";
    error_row.error_type = Some("invalid_request");
    fixture.insert_zcode_row(error_row)?;
    let mut cancelled_row = zcode_row("cancel-1", 3_000, 0, 0);
    cancelled_row.status = "cancelled";
    fixture.insert_zcode_row(cancelled_row)?;

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
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(first.total_inserted, 1);
        assert_eq!(
            first.sources[0].parse_issues.skipped_lines, 2,
            "error and cancelled rows are counted, not imported"
        );
        assert_eq!(first.sources[0].parse_issues.malformed_lines, 0);
        assert_eq!(zcode_source_count(&app.paths.db_path)?, 1);
        let reasons: Vec<_> = first.sources[0]
            .parse_issues
            .samples
            .iter()
            .map(|sample| sample.cli_line(None))
            .collect();
        assert!(
            reasons
                .iter()
                .any(|line| line == "skipped zcode_unfinished:error:invalid_request"),
            "{reasons:?}"
        );
        assert!(
            reasons
                .iter()
                .any(|line| line == "skipped zcode_unfinished:cancelled:unknown"),
            "{reasons:?}"
        );
        assert!(reasons.iter().all(|line| !line.contains("@0")));
        assert!(reasons.iter().all(|line| !line.contains("err-1")));

        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            second.sources[0].parse_issues.skipped_lines, 0,
            "the same unfinished rows must not reappear on an unchanged sync"
        );
        assert!(second.sources[0].parse_issues.samples.is_empty());
        assert_eq!(zcode_source_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_new_unfinished_row_after_skip_watermark_reports_once() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("ok-1", 1_000, 100, 40))?;
    let mut first_error = zcode_row("err-1", 2_000, 0, 0);
    first_error.status = "error";
    first_error.error_type = Some("invalid_request");
    fixture.insert_zcode_row(first_error)?;

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
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(first.sources[0].parse_issues.skipped_lines, 1);

        let mut newer = zcode_row("err-2", 4_000, 0, 0);
        newer.status = "error";
        newer.error_type = Some("rate_limit");
        fixture.insert_zcode_row(newer)?;
        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(second.sources[0].parse_issues.skipped_lines, 1);
        assert_eq!(
            second.sources[0].parse_issues.samples[0].reason,
            "zcode_unfinished:error:rate_limit"
        );

        let third = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(third.sources[0].parse_issues.skipped_lines, 0);
        assert!(third.sources[0].parse_issues.samples.is_empty());
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_db_rebuild_replays_from_zero() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("row-a", 1_000, 100, 40))?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(zcode_source_count(&app.paths.db_path)?, 1);

        // The DB is replaced with fresh ids; the persisted anchor no longer
        // exists, so the cursor must reset and replay from zero.
        fixture.rebuild_zcode_db(&[zcode_row("row-new", 1_000, 300, 90)])?;
        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(second.total_inserted, 1, "rebuilt DB replays from zero");
        // The pre-rebuild event is preserved (opencode semantics); no
        // duplicate event keys exist.
        let conn = Connection::open(&app.paths.db_path)?;
        let distinct_keys: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT event_key) FROM usage_event WHERE source = 'zcode'",
            [],
            |row| row.get(0),
        )?;
        let total_rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'zcode'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(distinct_keys, total_rows);
        assert_eq!(total_rows, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_rebuild_resets_skip_watermark() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("row-a", 1_000, 100, 40))?;
    let mut error_row = zcode_row("err-old", 2_000, 0, 0);
    error_row.status = "error";
    error_row.error_type = Some("invalid_request");
    fixture.insert_zcode_row(error_row)?;

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
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(first.sources[0].parse_issues.skipped_lines, 1);
        let cursor = store.cursors().load_zcode_cursor("local")?;
        assert!(cursor.last_skipped_at > 0);

        let mut rebuilt_error = zcode_row("err-new", 2_000, 0, 0);
        rebuilt_error.status = "error";
        rebuilt_error.error_type = Some("invalid_request");
        fixture.rebuild_zcode_db(&[zcode_row("row-new", 1_000, 300, 90), rebuilt_error])?;
        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            second.sources[0].parse_issues.skipped_lines, 1,
            "missing completed anchors must reset the skip watermark"
        );
        assert_eq!(
            second.sources[0].parse_issues.samples[0].reason,
            "zcode_unfinished:error:invalid_request"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_missing_root_sync_succeeds_and_reports_no_data() -> Result<()> {
    let fixture = Fixture::new()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let summary = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(summary.total_inserted, 0);
        assert!(summary.sources[0].absent, "missing DB reports absent");
        assert_eq!(
            source_capability_status(&app, &store, SourceKind::Zcode)?,
            "passive_no_data"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_home_override_points_parser_at_custom_root() -> Result<()> {
    let fixture = Fixture::new()?;
    let custom_root = fixture.home.join("custom-zcode");
    let db_path = custom_root.join("cli").join("db").join("db.sqlite");
    fs::create_dir_all(db_path.parent().unwrap())?;
    let conn = Connection::open(&db_path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE model_usage(
            id TEXT PRIMARY KEY, session_id TEXT, model_id TEXT, status TEXT,
            started_at INTEGER, completed_at INTEGER, input_tokens INTEGER,
            output_tokens INTEGER, reasoning_tokens INTEGER,
            cache_creation_input_tokens INTEGER, cache_read_input_tokens INTEGER,
            provider_total_tokens INTEGER, computed_total_tokens INTEGER
        );
        INSERT INTO model_usage(id, model_id, status, started_at, completed_at,
            input_tokens, output_tokens, computed_total_tokens)
        VALUES ('override-row', 'GLM-5.3', 'completed', 1, 2, 10, 5, 15);
        "#,
    )?;
    drop(conn);
    unsafe {
        std::env::set_var("ZCODE_HOME", &custom_root);
    }

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let summary = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(summary.total_inserted, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_first_sync_marks_current_token_accounting() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("row-a", 1_000, 100, 40))?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        assert_eq!(store.token_accounting_version(SourceKind::Zcode)?, None);

        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_eq!(
            store.token_accounting_version(SourceKind::Zcode)?,
            Some(expected_token_accounting_version(SourceKind::Zcode))
        );
        assert_eq!(expected_token_accounting_version(SourceKind::Zcode), 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_recent_days_run_filters_window_without_advancing_cursor() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("row-old", 1_000_000_000, 100, 40))?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(zcode_source_count(&app.paths.db_path)?, 1);

        // An append inside the recent window must import during the bounded
        // run, while the watermark and anchors stay untouched.
        let now_ms = chrono::Utc::now().timestamp_millis();
        fixture.insert_zcode_row(zcode_row("row-new", now_ms, 300, 90))?;
        let bounded = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                recent_days: Some(1),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            bounded.total_inserted, 1,
            "bounded run imports only the in-window append"
        );

        let cursor = store.cursors().load_zcode_cursor("local")?;
        assert_eq!(
            cursor.last_completed_at, 1_000_000_000,
            "bounded run must not advance the watermark"
        );
        assert!(
            !cursor.last_processed_ids.iter().any(|id| id == "row-new"),
            "bounded run must not promote in-window rows to anchors"
        );
        assert_eq!(zcode_source_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_recent_days_does_not_advance_skip_watermark() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("row-old", 1_000_000_000, 100, 40))?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;

        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut error_row = zcode_row("err-recent", now_ms, 0, 0);
        error_row.status = "error";
        error_row.error_type = Some("invalid_request");
        fixture.insert_zcode_row(error_row)?;
        let bounded = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                recent_days: Some(1),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(bounded.sources[0].parse_issues.skipped_lines, 1);
        let cursor = store.cursors().load_zcode_cursor("local")?;
        assert_eq!(
            cursor.last_skipped_at, 0,
            "bounded run must not advance the skip watermark"
        );
        assert!(cursor.last_skipped_ids.is_empty());

        let full = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            full.sources[0].parse_issues.skipped_lines, 1,
            "a later full sync still reports the unfinished row once"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_cancel_after_first_page_does_not_advance_skip_watermark() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("ok-1", 1_000, 100, 40))?;
    let mut error_row = zcode_row("err-1", 2_000, 0, 0);
    error_row.status = "error";
    error_row.error_type = Some("invalid_request");
    fixture.insert_zcode_row(error_row)?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let mut writer = store.begin_sync_run()?;
        let cancel = tokio_util::sync::CancellationToken::new();
        let mut progress = |event: SyncEvent| {
            if matches!(
                event,
                SyncEvent::Progress {
                    source: SourceKind::Zcode,
                    ..
                }
            ) {
                cancel.cancel();
            }
        };
        ZcodeParser
            .parse(&store, &mut writer, 1, None, &cancel, Some(&mut progress))
            .await?;
        writer.finish_sync_run()?;
        store.mark_current_token_accounting(SourceKind::Zcode)?;

        let cursor = store.cursors().load_zcode_cursor("local")?;
        assert_eq!(
            cursor.last_skipped_at, 0,
            "cancel after the first page save must not persist the skip watermark"
        );
        assert!(cursor.last_skipped_ids.is_empty());
        assert!(
            cursor.last_completed_at > 0,
            "the completed page may still persist its completed watermark"
        );

        let full = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            full.sources[0].parse_issues.skipped_lines, 1,
            "a later full sync still reports the unfinished row once"
        );
        assert_eq!(
            full.sources[0].parse_issues.samples[0].reason,
            "zcode_unfinished:error:invalid_request"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_source_db_opens_read_only_with_busy_timeout() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("row-a", 1_000, 100, 40))?;
    let conn = llmusage::parsers::zcode::open_source_db(&fixture.zcode_db_path())?;
    let timeout_ms: i64 = conn.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
    assert!(
        timeout_ms >= 1000,
        "busy timeout must be at least 1s, got {timeout_ms}"
    );
    assert!(
        conn.execute("CREATE TABLE write_probe(x INTEGER)", [])
            .is_err(),
        "ZCode source DB must open read-only"
    );
    fixture.restore_env();
    Ok(())
}

#[test]
fn zcode_shard_commits_cursor_with_events() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.insert_zcode_row(zcode_row("row-a", 1_000, 100, 40))?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let observed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed_for_hook = std::sync::Arc::clone(&observed);
        let _guard = llmusage::store::set_after_commit_shard_hook(move |store, shard| {
            if shard.source != SourceKind::Zcode {
                return;
            }
            if shard.events.is_empty() {
                return;
            }
            observed_for_hook.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let cursor = shard
                .zcode_cursor
                .as_deref()
                .expect("ZCode events must commit with zcode_cursor");
            let loaded = store
                .cursors()
                .load_zcode_cursor(&shard.host_id)
                .expect("load zcode cursor after commit_shard");
            assert_eq!(loaded.last_completed_at, cursor.last_completed_at);
            assert_eq!(loaded.last_processed_ids, cursor.last_processed_ids);
        });
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Zcode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert!(
            observed.load(std::sync::atomic::Ordering::SeqCst) >= 1,
            "commit_shard hook must observe a ZCode event shard"
        );
        let cursor = store.cursors().load_zcode_cursor("local")?;
        assert_eq!(cursor.last_completed_at, 1_000);
        assert_eq!(zcode_source_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}
