use super::super::*;

#[test]
fn opencode_high_water_handles_same_timestamp_ids() -> Result<()> {
    /*
     * ========================================================================
     * 步骤5：验证 OpenCode 同时间戳多主键续跑
     * ========================================================================
     * 目标：
     * 1) 首次 sync 导入第一条 assistant 记录
     * 2) 第二次插入同 time_created 但更大 id 的记录
     * 3) 第三次空跑不重复导入
     */
    let fixture = Fixture::new()?;
    fixture.seed_opencode("msg-1", 1776823200000, 64)?;

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
                source: Some(SourceKind::Opencode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(first.sources[0].events_seen, 1);
        assert_eq!(usage_event_count(&app.paths.db_path)?, 1);

        fixture.seed_opencode("msg-2", 1776823200000, 48)?;
        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Opencode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(second.sources[0].events_seen, 1);
        assert_eq!(usage_event_count(&app.paths.db_path)?, 2);

        let third = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Opencode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(third.sources[0].events_seen, 0);
        assert_eq!(third.sources[0].changed_files, 0);
        assert_eq!(third.sources[0].skipped_files, 1);
        assert_eq!(usage_event_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn opencode_recent_window_preserves_high_waters_and_later_recovers_old_rows() -> Result<()> {
    let fixture = Fixture::new()?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let old_ms = now_ms - chrono::Duration::days(90).num_milliseconds();
    let recent_ms = now_ms - chrono::Duration::days(1).num_milliseconds();
    fixture.seed_opencode("msg-old", old_ms, 11)?;
    fixture.seed_opencode("msg-recent", recent_ms, 22)?;
    fixture.seed_opencode_tool_part(
        "part-old",
        "msg-old",
        "session-1",
        old_ms,
        serde_json::json!({
            "id": "part-old",
            "messageID": "msg-old",
            "sessionID": "session-1",
            "type": "tool",
            "tool": "read",
            "state": { "status": "completed", "input": { "file_path": "/old/sentinel.rs" } }
        }),
    )?;
    fixture.seed_opencode_tool_part(
        "part-recent",
        "msg-recent",
        "session-1",
        recent_ms,
        serde_json::json!({
            "id": "part-recent",
            "messageID": "msg-recent",
            "sessionID": "session-1",
            "type": "tool",
            "tool": "edit",
            "state": { "status": "completed", "input": { "file_path": "/recent/sentinel.rs" } }
        }),
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let bounded = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Opencode),
            recent_days: Some(30),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &bounded, None).await?;

        assert_eq!(
            source_token_totals(&app.paths.db_path, SourceKind::Opencode)?,
            vec![22]
        );
        assert_eq!(usage_tool_call_count(&app.paths.db_path)?, 1);
        let bounded_cursor = store.cursors().load_opencode_cursor("local")?;
        assert_eq!(bounded_cursor.last_time_created, 0);
        assert!(bounded_cursor.last_processed_ids.is_empty());
        assert_eq!(bounded_cursor.last_part_rowid, 0);

        let full = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Opencode),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &full, None).await?;
        assert_eq!(
            source_token_totals(&app.paths.db_path, SourceKind::Opencode)?,
            vec![11, 22]
        );
        assert_eq!(usage_tool_call_count(&app.paths.db_path)?, 2);
        let full_cursor = store.cursors().load_opencode_cursor("local")?;
        assert_eq!(full_cursor.last_time_created, recent_ms);
        assert_eq!(full_cursor.last_processed_ids, vec!["msg-recent"]);
        assert!(full_cursor.last_part_rowid > 0);

        commands::sync::run_once_with_options(&app, &store, 0, &full, None).await?;
        assert_eq!(
            source_token_totals(&app.paths.db_path, SourceKind::Opencode)?,
            vec![11, 22]
        );
        assert_eq!(usage_tool_call_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn opencode_part_scan_uses_persisted_high_water() -> Result<()> {
    /*
     * ========================================================================
     * 步骤6：验证 OpenCode part 表工具调用进入 usage_tool_call
     * ========================================================================
     * 目标：
     * 1) message + 两条 tool part（builtin read + MCP）sync 后落 usage_tool_call
     * 2) MCP 工具按 `<server>_<tool>` 归类，mcp_server 正确
     * 3) 重复 sync 幂等（part 全量重扫但 tool_call_key 去重）
     */
    let fixture = Fixture::new()?;
    fixture.seed_opencode("msg-1", 1776823200000, 64)?;
    fixture.seed_opencode_tool_part(
        "prt-1",
        "msg-1",
        "session-1",
        1776823200050,
        serde_json::json!({
            "id": "prt-1",
            "messageID": "msg-1",
            "sessionID": "session-1",
            "type": "tool",
            "tool": "read",
            "state": { "status": "completed", "input": { "file_path": "src/lib.rs" } }
        }),
    )?;
    fixture.seed_opencode_tool_part(
        "prt-2",
        "msg-1",
        "session-1",
        1776823200060,
        serde_json::json!({
            "id": "prt-2",
            "messageID": "msg-1",
            "sessionID": "session-1",
            "type": "tool",
            "tool": "context7_query-docs",
            "state": { "status": "completed" }
        }),
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
                source: Some(SourceKind::Opencode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(first.sources[0].changed_files, 1);
        assert_eq!(usage_tool_call_count(&app.paths.db_path)?, 2);
        assert_eq!(
            opencode_mcp_servers(&app.paths.db_path)?,
            vec!["context7".to_string()]
        );
        let first_part_rowid = store
            .cursors()
            .load_opencode_cursor("local")?
            .last_part_rowid;
        assert!(first_part_rowid > 0);

        let hot = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Opencode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(hot.sources[0].changed_files, 0);
        assert_eq!(hot.sources[0].skipped_files, 1);
        assert_eq!(hot.sources[0].bytes_scanned, 0);
        assert_eq!(usage_tool_call_count(&app.paths.db_path)?, 2);

        fixture.seed_opencode_tool_part(
            "prt-3",
            "msg-1",
            "session-1",
            1776823200070,
            serde_json::json!({
                "id": "prt-3",
                "messageID": "msg-1",
                "sessionID": "session-1",
                "type": "tool",
                "tool": "edit",
                "state": { "status": "completed", "input": { "file_path": "src/main.rs" } }
            }),
        )?;
        let appended = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Opencode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(appended.sources[0].changed_files, 1);
        assert!(appended.sources[0].bytes_scanned > 0);
        assert_eq!(usage_tool_call_count(&app.paths.db_path)?, 3);
        assert!(
            store
                .cursors()
                .load_opencode_cursor("local")?
                .last_part_rowid
                > first_part_rowid
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn opencode_replaced_db_resets_high_water() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_opencode("msg-1", 1776823200000, 64)?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::sync::run(&app).await?;

        let store = Store::new(&app.paths)?;
        let first_cursor = store.cursors().load_opencode_cursor("local")?;
        assert_eq!(first_cursor.last_time_created, 1776823200000);
        assert_eq!(usage_event_count(&app.paths.db_path)?, 1);

        fixture.replace_opencode_db("msg-replaced", 1776823100000, 48)?;
        commands::sync::run(&app).await?;

        let second_cursor = store.cursors().load_opencode_cursor("local")?;
        assert_eq!(second_cursor.last_time_created, 1776823100000);
        assert_eq!(usage_event_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn opencode_missing_db_reports_absent_without_failing_sync() -> Result<()> {
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
                source: Some(SourceKind::Opencode),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_eq!(summary.sources.len(), 1);
        assert_eq!(summary.total_seen, 0);
        assert_eq!(summary.total_inserted, 0);
        let stats = &summary.sources[0];
        assert_eq!(stats.source, SourceKind::Opencode);
        assert!(stats.absent);
        assert_eq!(stats.last_error.as_deref(), Some("OpenCode SQLite DB 缺失"));
        assert_eq!(stats.events_seen, 0);
        assert_eq!(stats.events_inserted, 0);

        let cursor = store.cursors().load_opencode_cursor("local")?;
        assert_eq!(cursor.sqlite_status, "missing-db");
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn opencode_channel_db_without_opencode_home_is_imported() -> Result<()> {
    let fixture = Fixture::new()?;
    unsafe {
        std::env::remove_var("OPENCODE_HOME");
    }
    let channel_db = fixture
        .home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode-stable.db");
    fixture.seed_opencode_at(&channel_db, "msg-stable", 1776823200000, 64)?;

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
                source: Some(SourceKind::Opencode),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_eq!(summary.total_inserted, 1);
        assert_eq!(usage_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn opencode_explicit_db_env_is_imported() -> Result<()> {
    let fixture = Fixture::new()?;
    let explicit_db = fixture
        .home
        .join("custom-opencode")
        .join("opencode-nightly.db");
    fixture.seed_opencode_at(&explicit_db, "msg-nightly", 1776823200000, 64)?;
    unsafe {
        std::env::set_var("OPENCODE_DB", &explicit_db);
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
                source: Some(SourceKind::Opencode),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_eq!(summary.total_inserted, 1);
        assert_eq!(usage_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}
