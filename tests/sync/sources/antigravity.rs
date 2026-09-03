use super::super::*;

#[test]
fn rebuild_rejects_unattributed_antigravity_history_and_preserves_rows() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex(
        "rollout-antigravity-history.jsonl",
        120,
        "2026-04-22T01:12:00Z",
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::sync::run(&app).await?;
        let store = Store::new(&app.paths)?;
        let conn = store.open_connection()?;
        // Simulate hook-era antigravity rows: renamed source AND no file
        // attribution (source_path_hash NULL), the real legacy shape.
        conn.execute("UPDATE usage_event SET source = 'antigravity'", [])?;
        conn.execute(
            "UPDATE usage_event SET source_path_hash = NULL WHERE source = 'antigravity'",
            [],
        )?;
        conn.execute("UPDATE usage_bucket_30m SET source = 'antigravity'", [])?;
        let before: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'antigravity'",
            [],
            |row| row.get(0),
        )?;
        assert!(before > 0);
        drop(conn);

        let error = commands::sync::run_with_options(
            &app,
            commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::Antigravity),
                allow_lossy_rebuild: true,
                ..Default::default()
            },
        )
        .await
        .expect_err("rebuild must refuse while unattributed antigravity history exists");
        assert!(error.to_string().contains("hook-era history"));

        let after: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'antigravity'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(after, before);

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_sync_twice_is_idempotent() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_antigravity(
        "11111111-1111-1111-1111-111111111111",
        &[
            (
                1,
                ag_gen_metadata_blob(
                    500,
                    234,
                    50,
                    1200,
                    "resp-1",
                    Some("gemini-3.6-flash"),
                    Some("Gemini 3.6 Flash (High)"),
                    1_785_140_200,
                ),
            ),
            (
                2,
                ag_gen_metadata_blob(
                    300,
                    100,
                    20,
                    0,
                    "resp-2",
                    Some("gemini-3.6-flash"),
                    Some("Gemini 3.6 Flash (High)"),
                    1_785_140_300,
                ),
            ),
        ],
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
                source: Some(SourceKind::Antigravity),
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
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(second.total_inserted, 0);
        assert_eq!(second.sources[0].skipped_files, 1);
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_tokens_separate_output_from_reasoning() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_antigravity(
        "22222222-2222-2222-2222-222222222222",
        &[(
            1,
            ag_gen_metadata_blob(
                500,
                234,
                50,
                1200,
                "resp-1",
                Some("gemini-3.6-flash"),
                None,
                1_785_140_200,
            ),
        )],
    )?;

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
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;

        let conn = Connection::open(&app.paths.db_path)?;
        let row = conn.query_row(
            "SELECT input_tokens, cache_read_tokens, output_tokens, reasoning_output_tokens, total_tokens, model FROM usage_event WHERE source = 'antigravity'",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )?;
        // input = #2 + #1（system prompt）；total 含 reasoning（#9/#10 不相交）。
        assert_eq!(row.0, 500 + 1132);
        assert_eq!(row.1, 1200);
        assert_eq!(row.2, 234);
        assert_eq!(row.3, 50);
        assert_eq!(row.4, 1632 + 1200 + 234 + 50);
        assert_eq!(row.5, "gemini-3.6-flash");
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_append_replays_file_and_replaces_stale_rows() -> Result<()> {
    let fixture = Fixture::new()?;
    let uuid = "33333333-3333-3333-3333-333333333333";
    fixture.seed_antigravity(
        uuid,
        &[(
            1,
            ag_gen_metadata_blob(
                500,
                234,
                50,
                1200,
                "resp-1",
                Some("gemini-3.6-flash"),
                None,
                1_785_140_200,
            ),
        )],
    )?;

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
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);

        // 重写同一 conversation（新增一行）：fingerprint 变化 → 全文件重解析 +
        // reset_path_hashes 替换旧行。
        fixture.seed_antigravity(
            uuid,
            &[
                (
                    1,
                    ag_gen_metadata_blob(
                        500,
                        234,
                        50,
                        1200,
                        "resp-1",
                        Some("gemini-3.6-flash"),
                        None,
                        1_785_140_200,
                    ),
                ),
                (
                    2,
                    ag_gen_metadata_blob(
                        300,
                        100,
                        10,
                        0,
                        "resp-2",
                        Some("gemini-3.6-flash"),
                        None,
                        1_785_140_400,
                    ),
                ),
            ],
        )?;
        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(second.total_inserted, 2, "reparse re-inserts both rows");
        assert_eq!(
            antigravity_event_count(&app.paths.db_path)?,
            2,
            "stale row is replaced, not duplicated"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_deleted_conversation_preserves_history() -> Result<()> {
    let fixture = Fixture::new()?;
    let uuid = "44444444-4444-4444-4444-444444444444";
    let path = fixture.seed_antigravity(
        uuid,
        &[(
            1,
            ag_gen_metadata_blob(
                500,
                234,
                50,
                1200,
                "resp-1",
                Some("gemini-3.6-flash"),
                None,
                1_785_140_200,
            ),
        )],
    )?;

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
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;

        fs::remove_file(&path)?;
        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(second.total_inserted, 0);
        assert_eq!(
            antigravity_event_count(&app.paths.db_path)?,
            1,
            "deleted conversation history is preserved"
        );
        let counts = store
            .source_files()
            .counts(SourceKind::Antigravity, "local")?;
        assert_eq!(counts.missing, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_unreadable_conversation_preserves_imported_events() -> Result<()> {
    let fixture = Fixture::new()?;
    let uuid = "55555555-5555-5555-5555-555555555555";
    let path = fixture.seed_antigravity(
        uuid,
        &[(
            1,
            ag_gen_metadata_blob(
                500,
                234,
                50,
                1200,
                "resp-1",
                Some("gemini-3.6-flash"),
                None,
                1_785_140_200,
            ),
        )],
    )?;

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
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
        let live_cursor = store
            .cursors()
            .load_file_cursors(SourceKind::Antigravity, "local")?
            .into_iter()
            .find(|(key, _)| key.ends_with(&format!("{uuid}.db")))
            .map(|(_, cursor)| cursor)
            .expect("live cursor");

        fs::write(&path, b"not a sqlite database")?;
        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            antigravity_event_count(&app.paths.db_path)?,
            1,
            "unreadable rewrite must not reset imported events"
        );
        let antigravity = second
            .sources
            .iter()
            .find(|stats| stats.source == SourceKind::Antigravity)
            .expect("antigravity stats");
        assert!(antigravity.parse_issues.malformed_lines >= 1);
        let after = store
            .cursors()
            .load_file_cursors(SourceKind::Antigravity, "local")?
            .into_iter()
            .find(|(key, _)| key.ends_with(&format!("{uuid}.db")))
            .map(|(_, cursor)| cursor)
            .expect("cursor after unreadable rewrite");
        assert_eq!(after.file_fingerprint, live_cursor.file_fingerprint);
        assert_eq!(after.file_size, live_cursor.file_size);

        let third = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
        let antigravity = third
            .sources
            .iter()
            .find(|stats| stats.source == SourceKind::Antigravity)
            .expect("antigravity stats");
        assert!(
            antigravity.parse_issues.malformed_lines >= 1,
            "unreadable file must stay eligible for retry"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_missing_root_reports_no_data() -> Result<()> {
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
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(summary.total_inserted, 0);
        assert_eq!(
            source_capability_status(&app, &store, SourceKind::Antigravity)?,
            "passive_no_data",
            "parser-backed antigravity reports passive_no_data without artifacts"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_cli_home_override() -> Result<()> {
    let fixture = Fixture::new()?;
    let custom_root = fixture.home.join("custom-gemini");
    let conversations = custom_root.join("antigravity-cli").join("conversations");
    fs::create_dir_all(&conversations)?;
    let conn = Connection::open(conversations.join("55555555-5555-5555-5555-555555555555.db"))?;
    conn.execute_batch(
        r#"
        CREATE TABLE gen_metadata(idx INTEGER PRIMARY KEY, data BLOB, size INTEGER);
        CREATE TABLE trajectory_metadata_blob(id TEXT, data BLOB);
        "#,
    )?;
    let blob = ag_gen_metadata_blob(
        100,
        40,
        5,
        0,
        "resp-x",
        Some("gemini-3.6-flash"),
        None,
        1_785_140_200,
    );
    conn.execute(
        "INSERT INTO gen_metadata(idx, data, size) VALUES (1, ?1, ?2)",
        rusqlite::params![&blob, blob.len() as i64],
    )?;
    conn.execute(
        "INSERT INTO trajectory_metadata_blob(id, data) VALUES ('traj', ?1)",
        rusqlite::params![&antigravity_trajectory_blob()],
    )?;
    drop(conn);
    unsafe {
        std::env::set_var("GEMINI_CLI_HOME", &custom_root);
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
                source: Some(SourceKind::Antigravity),
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
fn antigravity_upgrade_from_historical_only_keeps_legacy_rows() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_antigravity(
        "66666666-6666-6666-6666-666666666666",
        &[(
            1,
            ag_gen_metadata_blob(
                500,
                234,
                50,
                1200,
                "resp-1",
                Some("gemini-3.6-flash"),
                None,
                1_785_140_200,
            ),
        )],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;

        // 预置 hook 时代存量行：真实旧 key 形状 + 无文件归属。
        // v21 marker 预置后 has_legacy_token_accounting 必须为 false。
        let conn = store.open_connection()?;
        conn.execute_batch(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens, created_at
            ) VALUES ('antigravity:test:event', 'antigravity', 'gemini-2.5-pro',
                      '2026-07-15T03:00:00Z', '2026-07-15T03:00:00Z',
                      20, 0, 0, 5, 0, 25, '2026-07-15T03:00:00Z');
            "#,
        )?;
        drop(conn);
        assert!(
            !store.has_legacy_token_accounting(SourceKind::Antigravity)?,
            "v21 marker must keep hook-era rows out of automatic legacy repair"
        );

        // 无界 sync：解析器导入新行，存量行不动、不重复。
        let summary = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(summary.total_inserted, 1);

        let conn = Connection::open(&app.paths.db_path)?;
        let legacy: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE event_key = 'antigravity:test:event'",
            [],
            |row| row.get(0),
        )?;
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'antigravity'",
            [],
            |row| row.get(0),
        )?;
        let distinct: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT event_key) FROM usage_event WHERE source = 'antigravity'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(legacy, 1, "unbounded sync must not delete hook-era rows");
        assert_eq!(total, 2, "legacy row and parser row coexist");
        assert_eq!(distinct, total);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_recent_days_run_skips_reset_and_window_filters() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_antigravity(
        "77777777-7777-7777-7777-777777777777",
        &[(
            1,
            ag_gen_metadata_blob(
                500,
                234,
                50,
                1200,
                "resp-old",
                Some("gemini-3.6-flash"),
                None,
                1_785_140_200,
            ),
        )],
    )?;

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
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);

        // 窗口内的追加 + 窗口外的旧行重写（fingerprint 变化）。
        let uuid = "77777777-7777-7777-7777-777777777777";
        let now_seconds = chrono::Utc::now().timestamp().unsigned_abs();
        fixture.seed_antigravity(
            uuid,
            &[
                (
                    1,
                    ag_gen_metadata_blob(
                        500,
                        234,
                        50,
                        1200,
                        "resp-old",
                        Some("gemini-3.6-flash"),
                        None,
                        1_785_140_200,
                    ),
                ),
                (
                    2,
                    ag_gen_metadata_blob(
                        300,
                        100,
                        10,
                        0,
                        "resp-new",
                        Some("gemini-3.6-flash"),
                        None,
                        now_seconds,
                    ),
                ),
            ],
        )?;
        let bounded = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Antigravity),
                recent_days: Some(1),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            bounded.total_inserted, 1,
            "bounded run imports only the in-window event"
        );

        // bounded 不 reset：窗口外旧行不被重放删除（仍 1 条旧 + 1 条新）。
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 2);

        // 随后的全量 sync 恢复窗口外历史语义（重放替换，行数不膨胀）。
        let full = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(full.total_inserted, 2, "full sync replays both rows");
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}
