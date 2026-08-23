use super::super::*;

#[test]
fn sync_hot_run_and_append_remain_incremental() -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：验证热启动空跑与追加续跑
     * ========================================================================
     * 目标：
     * 1) 首次 sync 导入基础数据
     * 2) 二次空跑不重复导入
     * 3) 追加同一文件时只导入新增事件
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
        let first_overview = Dashboard::open(&store)?.overview(&Default::default())?;
        let first_sync_status = store.sync_status().load_source_sync_statuses("local")?;
        // One status per registered source: codex, claude, opencode,
        // antigravity, kimi_code, pi, omp, grok, zcode, and deepseek_harness.
        assert_eq!(first_sync_status.len(), 10);

        commands::sync::run(&app).await?;
        let second_overview = Dashboard::open(&store)?.overview(&Default::default())?;
        assert_eq!(
            first_overview.total.total_tokens,
            second_overview.total.total_tokens
        );

        let hot_status = store.sync_status().load_source_sync_statuses("local")?;
        let claude_status = hot_status
            .iter()
            .find(|item| item.source == "claude")
            .expect("claude sync status");
        assert_eq!(claude_status.changed_files, 0);
        let codex_status = hot_status
            .iter()
            .find(|item| item.source == "codex")
            .expect("codex sync status");
        assert_eq!(codex_status.changed_files, 0);

        fixture.append_codex("rollout-test.jsonl", 33, "2026-04-22T03:12:00Z")?;
        fixture.append_claude("session.jsonl", 44, "2026-04-22T03:00:00Z")?;
        commands::sync::run(&app).await?;

        let third_overview = Dashboard::open(&store)?.overview(&Default::default())?;
        assert!(third_overview.total.total_tokens > second_overview.total.total_tokens);
        let count = usage_event_count(&app.paths.db_path)?;
        assert_eq!(count, 5);

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn hot_sync_keeps_unchanged_source_files_live_and_reports_stored_events() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex("rollout-a.jsonl", 120, "2026-04-22T01:12:00Z")?;
    fixture.seed_codex("rollout-b.jsonl", 80, "2026-04-22T02:12:00Z")?;

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
        assert_eq!(first.total_inserted, 2);
        assert_eq!(first.stored_events, 2);
        assert_eq!(first.sources[0].stored_events, 2);

        let counts = store.source_files().counts(SourceKind::Codex, "local")?;
        assert_eq!(counts.live, 2);
        assert_eq!(counts.missing, 0);

        let second = commands::sync::run_once_with_options(
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
        assert_eq!(second.total_inserted, 0);
        assert_eq!(second.stored_events, 2);
        assert_eq!(second.sources[0].changed_files, 0);
        assert_eq!(second.sources[0].skipped_files, 2);
        assert_eq!(second.sources[0].stored_events, 2);

        let counts = store.source_files().counts(SourceKind::Codex, "local")?;
        assert_eq!(counts.live, 2);
        assert_eq!(counts.missing, 0, "unchanged-but-present files stay live");
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn codex_append_scans_only_changed_file() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex("rollout-a.jsonl", 120, "2026-04-22T01:12:00Z")?;
    fixture.seed_codex("rollout-b.jsonl", 80, "2026-04-22T02:12:00Z")?;
    let changed_path = fixture
        .codex_home
        .join("sessions/2026/04/22/rollout-a.jsonl");

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
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            None,
        )
        .await?;

        let before = fs::metadata(&changed_path)?.len();
        fixture.append_codex("rollout-a.jsonl", 33, "2026-04-22T03:12:00Z")?;
        let appended_bytes = fs::metadata(&changed_path)?.len() - before;
        let (mut progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(32);
        let summary = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            Some(&mut progress_tx),
        )
        .await?;
        drop(progress_tx);
        let mut progress_events = Vec::new();
        while let Ok(event) = progress_rx.try_recv() {
            progress_events.push(event);
        }

        let stats = &summary.sources[0];
        assert_eq!(stats.files_processed, 2);
        assert_eq!(stats.changed_files, 1);
        assert_eq!(stats.skipped_files, 1);
        assert_eq!(stats.bytes_scanned, appended_bytes);
        assert_eq!(stats.events_seen, 1);
        assert_eq!(
            progress_events.iter().find_map(|event| match event {
                SyncEvent::SourceStarted {
                    source: SourceKind::Codex,
                    files_total,
                } => Some(*files_total),
                _ => None,
            }),
            Some(1),
            "Codex progress total must count planned replay files"
        );
        let progress_snapshots = progress_events
            .iter()
            .filter_map(|event| match event {
                SyncEvent::Progress {
                    source: SourceKind::Codex,
                    files_scanned,
                    records_imported,
                    ..
                } => Some((*files_scanned, *records_imported)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            progress_snapshots
                .iter()
                .all(|(position, _)| *position <= 1)
        );
        assert!(progress_snapshots.contains(&(1, 0)));
        assert_eq!(
            progress_snapshots.last(),
            Some(&(1, stats.events_inserted as u64))
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn claude_changed_project_does_not_replay_other_projects() -> Result<()> {
    let fixture = Fixture::new()?;
    let project_a_main = fixture.seed_claude_lines(
        "project-a",
        "session.jsonl",
        &[claude_logical_usage_line(
            "msg-shared",
            "req-main",
            false,
            80,
            "2026-04-22T01:00:00Z",
        )],
    )?;
    let project_a_sidechain = fixture.seed_claude_lines(
        "project-a",
        "sidechain.jsonl",
        &[claude_logical_usage_line(
            "msg-shared",
            "req-side",
            true,
            100,
            "2026-04-22T01:01:00Z",
        )],
    )?;
    fixture.seed_claude_lines(
        "project-b",
        "session.jsonl",
        &[claude_logical_usage_line(
            "msg-other",
            "req-other",
            false,
            60,
            "2026-04-22T02:00:00Z",
        )],
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
                source: Some(SourceKind::Claude),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(first.sources[0].events_seen, 3);
        assert_eq!(usage_event_count(&app.paths.db_path)?, 2);

        fixture.append_claude_line(
            "project-a",
            "session.jsonl",
            &claude_logical_usage_line("msg-new", "req-new", false, 40, "2026-04-22T03:00:00Z"),
        )?;
        let changed_project_bytes =
            fs::metadata(&project_a_main)?.len() + fs::metadata(&project_a_sidechain)?.len();
        let (mut progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(32);
        let second = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Claude),
                ..Default::default()
            },
            Some(&mut progress_tx),
        )
        .await?;
        drop(progress_tx);
        let mut progress_events = Vec::new();
        while let Ok(event) = progress_rx.try_recv() {
            progress_events.push(event);
        }

        let stats = &second.sources[0];
        assert_eq!(stats.files_processed, 3);
        assert_eq!(stats.changed_files, 2);
        assert_eq!(stats.skipped_files, 1);
        assert_eq!(stats.bytes_scanned, changed_project_bytes);
        assert_eq!(stats.events_seen, 3);
        assert_eq!(
            progress_events.iter().find_map(|event| match event {
                SyncEvent::SourceStarted {
                    source: SourceKind::Claude,
                    files_total,
                } => Some(*files_total),
                _ => None,
            }),
            Some(2),
            "Claude progress total must include every file replayed in the selected project"
        );
        let progress_snapshots = progress_events
            .iter()
            .filter_map(|event| match event {
                SyncEvent::Progress {
                    source: SourceKind::Claude,
                    files_scanned,
                    records_imported,
                    ..
                } => Some((*files_scanned, *records_imported)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            progress_snapshots
                .iter()
                .all(|(position, _)| *position <= 2)
        );
        assert!(progress_snapshots.contains(&(2, 0)));
        assert_eq!(
            progress_snapshots.last(),
            Some(&(2, stats.events_inserted as u64))
        );
        assert_eq!(usage_event_count(&app.paths.db_path)?, 3);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn claude_recent_window_preserves_full_history_cursor_and_later_recovers_old_event() -> Result<()> {
    let fixture = Fixture::new()?;
    let old_at = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
    let recent_at = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    fixture.seed_claude_lines(
        "recent-window",
        "session.jsonl",
        &[
            claude_usage_line(&old_at, 11),
            claude_usage_line(&recent_at, 22),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let bounded = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Claude),
            recent_days: Some(30),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &bounded, None).await?;

        assert_eq!(
            source_token_totals(&app.paths.db_path, SourceKind::Claude)?,
            vec![22],
            "bounded Claude sync must import only the recent event"
        );
        assert!(
            store
                .cursors()
                .load_file_cursors(SourceKind::Claude, "local")?
                .is_empty(),
            "bounded Claude sync must not advance the full-history cursor"
        );

        let full = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Claude),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &full, None).await?;
        assert_eq!(
            source_token_totals(&app.paths.db_path, SourceKind::Claude)?,
            vec![11, 22],
            "later full sync must recover the window-excluded event exactly once"
        );
        assert_eq!(
            store
                .cursors()
                .load_file_cursors(SourceKind::Claude, "local")?
                .len(),
            1
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn sync_replay_replaces_old_file_totals() -> Result<()> {
    /*
     * ========================================================================
     * 步骤2：验证整文件重放会先清理旧事件
     * ========================================================================
     * 目标：
     * 1) 首次 sync 导入原始 Codex 文件
     * 2) 覆盖同一路径文件，触发整文件重放
     * 3) 最终总量应等于新文件，不保留旧值
     */
    let fixture = Fixture::new()?;
    fixture.seed_codex("rollout-reset.jsonl", 120, "2026-04-22T01:12:00Z")?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::sync::run(&app).await?;

        let store = Store::new(&app.paths)?;
        let first_total = Dashboard::open(&store)?
            .overview(&Default::default())?
            .total
            .total_tokens;
        assert_eq!(first_total, 120);

        fixture.replace_codex("rollout-reset.jsonl", 45, "2026-04-22T04:00:00Z")?;
        commands::sync::run(&app).await?;

        let replaced_total = Dashboard::open(&store)?
            .overview(&Default::default())?
            .total
            .total_tokens;
        assert_eq!(replaced_total, 45);
        assert_eq!(usage_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn codex_missing_history_survives_regular_sync_and_blocks_rebuild_by_default() -> Result<()> {
    /*
     * ========================================================================
     * 步骤2.5：验证 Codex 历史文件删除后的 rebuild 保护
     * ========================================================================
     * 目标：
     * 1) 首次 sync 导入 Codex rollout 后删除原始文件
     * 2) 普通 sync 只标记 source_file.missing，不删除 usage history
     * 3) sync --rebuild --source codex 默认拒绝 lossy reset
     */
    let fixture = Fixture::new()?;
    fixture.seed_codex("rollout-lossy.jsonl", 120, "2026-04-22T01:12:00Z")?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::sync::run(&app).await?;
        let store = Store::new(&app.paths)?;
        let first_total = Dashboard::open(&store)?
            .overview(&Default::default())?
            .total
            .total_tokens;
        let first_count = usage_event_count(&app.paths.db_path)?;
        assert_eq!(first_total, 120);
        assert_eq!(first_count, 1);

        fixture.remove_codex("rollout-lossy.jsonl")?;
        commands::sync::run(&app).await?;

        let after_regular = Dashboard::open(&store)?.overview(&Default::default())?;
        assert_eq!(after_regular.total.total_tokens, first_total);
        assert_eq!(usage_event_count(&app.paths.db_path)?, first_count);
        let source_counts = store.source_files().counts(SourceKind::Codex, "local")?;
        assert_eq!(source_counts.missing, 1);
        let diagnostics = Dashboard::open(&store)?.diagnostics()?;
        let codex = diagnostics
            .by_source
            .iter()
            .find(|row| row.source == "codex")
            .expect("codex diagnostics row");
        assert_eq!(codex.missing_file_count, 1);
        assert_eq!(codex.protected_event_count, 1);
        assert!(codex.lossy_rebuild_risk);

        let blocked = commands::sync::run_with_options(
            &app,
            commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
        )
        .await;
        let err = blocked.expect_err("lossy rebuild should be refused by default");
        assert!(
            err.to_string().contains("Refusing lossy sync --rebuild"),
            "{err:#}"
        );
        assert_eq!(usage_event_count(&app.paths.db_path)?, first_count);
        assert_eq!(
            Dashboard::open(&store)?
                .overview(&Default::default())?
                .total
                .total_tokens,
            first_total
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn codex_lossy_rebuild_can_be_explicitly_allowed() -> Result<()> {
    /*
     * ========================================================================
     * 步骤2.6：验证显式逃生口保留 destructive rebuild 语义
     * ========================================================================
     * 目标：
     * 1) 构造 Codex 已导入但原始文件缺失的 lossy 状态
     * 2) 加 --allow-lossy-rebuild 后允许 reset + 重扫
     * 3) 因源文件已不在磁盘上，Codex usage 被显式清空
     */
    let fixture = Fixture::new()?;
    fixture.seed_codex("rollout-lossy-allowed.jsonl", 120, "2026-04-22T01:12:00Z")?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::sync::run(&app).await?;
        assert_eq!(usage_event_count(&app.paths.db_path)?, 1);

        fixture.remove_codex("rollout-lossy-allowed.jsonl")?;
        commands::sync::run(&app).await?;

        commands::sync::run_with_options(
            &app,
            commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::Codex),
                allow_lossy_rebuild: true,
                ..Default::default()
            },
        )
        .await?;

        let store = Store::new(&app.paths)?;
        assert_eq!(usage_event_count(&app.paths.db_path)?, 0);
        assert_eq!(
            Dashboard::open(&store)?
                .overview(&Default::default())?
                .total
                .total_tokens,
            0
        );
        assert_eq!(
            store
                .source_files()
                .counts(SourceKind::Codex, "local")?
                .missing,
            0
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}
