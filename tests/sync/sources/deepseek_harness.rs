use super::super::*;

#[test]
fn dsh_sync_twice_is_idempotent() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dsh(
        "sess-a",
        &[
            dsh_session_line("sess-a", None, None),
            dsh_usage_line(1, 1_700_000_000_000, "msg-1", 10, 4),
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
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(first.total_inserted, 1);
        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(second.total_inserted, 0);
        assert_eq!(dsh_event_count(&app.paths.db_path)?, 1);
        assert_eq!(
            source_capability_status(&app, &store, SourceKind::DeepseekHarness)?,
            "passive_ready"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn dsh_append_imports_new_frame_after_reparse() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dsh_zstd(
        "sess-a",
        &[
            dsh_session_line("sess-a", None, None),
            dsh_usage_line(1, 1_700_000_000_000, "msg-1", 10, 4),
        ],
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
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        fixture.seed_dsh_zstd(
            "sess-a",
            &[
                dsh_session_line("sess-a", None, None),
                dsh_usage_line(1, 1_700_000_000_000, "msg-1", 10, 4),
                dsh_usage_line(2, 1_700_000_000_100, "msg-2", 6, 2),
            ],
        )?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            dsh_event_count(&app.paths.db_path)?,
            2,
            "reparse after a new frame must keep the old event and add the new one"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn dsh_rewrite_replaces_stale_rows() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dsh(
        "sess-a",
        &[
            dsh_session_line("sess-a", None, None),
            dsh_usage_line(1, 1_700_000_000_000, "old", 10, 4),
        ],
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
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        fixture.seed_dsh(
            "sess-a",
            &[
                dsh_session_line("sess-a", None, None),
                dsh_usage_line(1, 1_700_000_000_200, "new", 20, 5),
            ],
        )?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(dsh_event_count(&app.paths.db_path)?, 1);
        let models = {
            let conn = Connection::open(&app.paths.db_path)?;
            conn.query_row(
                "SELECT input_tokens FROM usage_event WHERE source = 'deepseek_harness'",
                [],
                |row| row.get::<_, i64>(0),
            )?
        };
        assert_eq!(models, 20);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn dsh_deleted_session_preserves_history() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.seed_dsh(
        "sess-a",
        &[
            dsh_session_line("sess-a", None, None),
            dsh_usage_line(1, 1_700_000_000_000, "msg-1", 10, 4),
        ],
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
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        fs::remove_file(path)?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(dsh_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn dsh_missing_root_reports_no_data() -> Result<()> {
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
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(summary.total_inserted, 0);
        assert_eq!(
            source_capability_status(&app, &store, SourceKind::DeepseekHarness)?,
            "passive_no_data"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn dsh_home_override_points_parser_at_custom_root() -> Result<()> {
    let fixture = Fixture::new()?;
    let custom_root = fixture.home.join("custom-dsh");
    fixture.seed_dsh_under(
        &custom_root,
        "sess-custom",
        &[
            dsh_session_line("sess-custom", None, None),
            dsh_usage_line(1, 1_700_000_000_000, "msg-1", 7, 3),
        ],
    )?;
    unsafe {
        std::env::set_var("DSH_HOME", &custom_root);
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
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(summary.total_inserted, 1);
        assert_eq!(dsh_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn dsh_first_sync_marks_current_token_accounting() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dsh(
        "sess-a",
        &[
            dsh_session_line("sess-a", None, None),
            dsh_usage_line(1, 1_700_000_000_000, "msg-1", 10, 4),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        assert_eq!(
            store.token_accounting_version(SourceKind::DeepseekHarness)?,
            None
        );
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            store.token_accounting_version(SourceKind::DeepseekHarness)?,
            Some(expected_token_accounting_version(
                SourceKind::DeepseekHarness
            ))
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn dsh_parser_provider_survives_loaded_ccr_timeline() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.write_provider_map(
        r#"
{"platform":"codex","provider":"anyrouter","activated_at":"2026-04-22T01:00:00Z","event":"activate"}
{"platform":"claude","provider":"glm","activated_at":"2026-04-22T01:00:00Z","event":"activate"}
"#,
    )?;
    fixture.seed_dsh(
        "sess-a",
        &[
            dsh_session_line("sess-a", None, None),
            dsh_usage_line(1, 1_700_000_000_000, "msg-1", 10, 4),
        ],
    )?;
    fixture.seed_codex("rollout-dsh-ccr.jsonl", 120, "2026-04-22T01:12:00Z")?;
    fixture.seed_claude("session-dsh-ccr.jsonl", 90, "2026-04-22T02:00:00Z")?;

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
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        commands::sync::run_once_with_options(
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
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Claude),
                ..Default::default()
            },
            None,
        )
        .await?;

        let conn = Connection::open(&app.paths.db_path)?;
        let dsh_empty: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source='deepseek_harness' AND provider_label=''",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(dsh_empty, 0);
        let dsh_labels: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT provider_label FROM usage_event WHERE source='deepseek_harness'",
            )?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        assert_eq!(dsh_labels, vec!["deepseek-official".to_string()]);

        let codex_labels: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT DISTINCT provider_label FROM usage_event WHERE source='codex'")?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        assert_eq!(codex_labels, vec!["anyrouter".to_string()]);

        let claude_labels: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT DISTINCT provider_label FROM usage_event WHERE source='claude'")?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        assert_eq!(claude_labels, vec!["glm".to_string()]);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn dsh_fork_parent_and_child_do_not_double_count() -> Result<()> {
    let fixture = Fixture::new()?;
    let shared = dsh_usage_line(1, 1_700_000_000_000, "shared-msg", 10, 4);
    fixture.seed_dsh(
        "parent",
        &[
            dsh_session_line("parent", None, None),
            shared.clone(),
            dsh_usage_line(2, 1_700_000_000_010, "parent-only", 3, 1),
        ],
    )?;
    fixture.seed_dsh(
        "child",
        &[
            dsh_session_line("child", Some("parent"), Some(2)),
            shared,
            dsh_usage_line(1, 1_700_000_000_010, "parent-only", 3, 1),
            dsh_usage_line(2, 1_700_000_000_020, "child-only", 8, 2),
        ],
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
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            dsh_event_count(&app.paths.db_path)?,
            3,
            "shared fork rows collapse; seedLength skips the child prefix"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn dsh_family_replay_keeps_shared_event_when_owner_rewrites() -> Result<()> {
    let fixture = Fixture::new()?;
    let shared = dsh_usage_line(1, 1_700_000_000_000, "shared-msg", 10, 4);
    fixture.seed_dsh(
        "parent",
        &[
            dsh_session_line("parent", None, None),
            shared.clone(),
            dsh_usage_line(2, 1_700_000_000_010, "parent-only", 3, 1),
        ],
    )?;
    fixture.seed_dsh(
        "child",
        &[
            dsh_session_line("child", Some("parent"), None),
            shared,
            dsh_usage_line(2, 1_700_000_000_020, "child-only", 8, 2),
        ],
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
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(dsh_event_count(&app.paths.db_path)?, 3);
        let before = dsh_event_keys(&app.paths.db_path)?;

        fixture.seed_dsh(
            "parent",
            &[
                dsh_session_line("parent", None, None),
                dsh_usage_line(2, 1_700_000_000_010, "parent-only", 3, 1),
            ],
        )?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(
            dsh_event_count(&app.paths.db_path)?,
            3,
            "family replay must reinsert the shared key from the unchanged child"
        );
        let after = dsh_event_keys(&app.paths.db_path)?;
        assert_eq!(before, after);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn dsh_recent_days_run_skips_reset_and_does_not_advance_cursor() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.seed_dsh(
        "sess-a",
        &[
            dsh_session_line("sess-a", None, None),
            dsh_usage_line(1, 1_577_836_800_000, "old", 10, 4),
        ],
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
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        let cursors = store
            .cursors()
            .load_file_cursors(SourceKind::DeepseekHarness, "local")?;
        let before = cursors
            .get(&path.to_string_lossy().to_string())
            .cloned()
            .expect("cursor after first sync");

        fixture.seed_dsh(
            "sess-a",
            &[
                dsh_session_line("sess-a", None, None),
                dsh_usage_line(1, 1_577_836_800_000, "old", 10, 4),
                dsh_usage_line(2, chrono::Utc::now().timestamp_millis(), "new", 6, 2),
            ],
        )?;
        let bounded = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::DeepseekHarness),
                recent_days: Some(1),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(bounded.total_inserted, 1);
        let cursors = store
            .cursors()
            .load_file_cursors(SourceKind::DeepseekHarness, "local")?;
        let after = cursors
            .get(&path.to_string_lossy().to_string())
            .cloned()
            .expect("cursor after bounded sync");
        assert_eq!(before.file_fingerprint, after.file_fingerprint);
        assert_eq!(before.file_size, after.file_size);
        assert_eq!(dsh_event_count(&app.paths.db_path)?, 2);

        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::DeepseekHarness),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(dsh_event_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}
