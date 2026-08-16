use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use anyhow::Result;
use llmusage::{
    app::AppContext,
    commands,
    models::SourceKind,
    parsers::{SourceSyncStats, SyncEvent},
    query::{Dashboard, QueryFilter},
    store::{HolderKind, Store, expected_token_accounting_version},
};
use rusqlite::Connection;
use tempfile::TempDir;

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
        let first_sync_status = store.sync_status().load_source_sync_statuses()?;
        // One status per registered source: codex, claude, opencode,
        // antigravity, kimi_code, pi, grok, zcode, and deepseek_harness.
        assert_eq!(first_sync_status.len(), 9);

        commands::sync::run(&app).await?;
        let second_overview = Dashboard::open(&store)?.overview(&Default::default())?;
        assert_eq!(
            first_overview.total.total_tokens,
            second_overview.total.total_tokens
        );

        let hot_status = store.sync_status().load_source_sync_statuses()?;
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

        let counts = store.source_files().counts(SourceKind::Codex)?;
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

        let counts = store.source_files().counts(SourceKind::Codex)?;
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
        let source_counts = store.source_files().counts(SourceKind::Codex)?;
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
        assert_eq!(store.source_files().counts(SourceKind::Codex)?.missing, 0);
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
fn sqlite_worker_lock_is_exclusive() -> Result<()> {
    /*
     * ========================================================================
     * 步骤4：验证 SQLite 租约锁排他
     * ========================================================================
     * 目标：
     * 1) 第一把锁成功拿到
     * 2) 第二次尝试立刻失败
     * 3) 释放后可以再次获取
     */
    let fixture = Fixture::new()?;
    let app = AppContext::discover()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;

    let first = store.acquire_worker_lock_with(Duration::from_millis(1), HolderKind::Cli)?;
    let holder = store
        .current_worker_lock()?
        .expect("current lock holder should be visible");
    assert_eq!(holder.holder_kind, "cli");
    assert!(holder.holder_pid > 0);
    assert!(holder.acquired_at.contains('T'));
    let second = store.acquire_worker_lock_with(Duration::from_millis(1), HolderKind::Library);
    assert!(matches!(
        second,
        Err(llmusage::LlmusageError::LockBusy { .. })
    ));
    drop(first);
    let third = store.acquire_worker_lock_with(Duration::from_millis(1), HolderKind::Library)?;
    assert_eq!(third.meta().holder_kind, "library");

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
fn worker_lock_heartbeat_refreshes_existing_lease() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = AppContext::discover()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;

    let lock = store.acquire_worker_lock_with(Duration::from_millis(1), HolderKind::Cli)?;
    let stale_updated_at = "2000-01-01T00:00:00Z";
    let valid_lease_expires_at = (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339();
    let conn = Connection::open(&app.paths.db_path)?;
    conn.execute(
        "UPDATE worker_lock SET updated_at = ?1, lease_expires_at = ?2",
        (stale_updated_at, &valid_lease_expires_at),
    )?;
    drop(conn);

    let heartbeat = lock.start_heartbeat(Duration::from_millis(10));
    let mut refreshed = None;
    for _ in 0..50 {
        thread::sleep(Duration::from_millis(20));
        let conn = Connection::open(&app.paths.db_path)?;
        let row = conn.query_row(
            "SELECT updated_at, lease_expires_at FROM worker_lock",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        if row.0 != stale_updated_at && row.1 != valid_lease_expires_at {
            refreshed = Some(row);
            break;
        }
    }
    drop(heartbeat);
    drop(lock);
    assert!(
        refreshed.is_some(),
        "heartbeat should refresh updated_at and lease_expires_at before the lease can expire"
    );

    fixture.restore_env();
    Ok(())
}

#[test]
fn status_renders_lock_holder() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = AppContext::discover()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let _lock = store.acquire_worker_lock_with(Duration::from_millis(1), HolderKind::Cli)?;

    let output = Command::new(env!("CARGO_BIN_EXE_llmusage"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("status")
        .env("HOME", &fixture.home)
        .env("USERPROFILE", &fixture.home)
        .env("CODEX_HOME", &fixture.codex_home)
        .env("OPENCODE_HOME", &fixture.opencode_home)
        .env("RUST_LOG", "off")
        .output()?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("- Worker lock: holder=cli:"));

    fixture.restore_env();
    Ok(())
}

/// End-to-end guard for the sync display contract: the final summary table
/// (with its aggregated `TOTAL` row) is the only thing on stdout, non-TTY
/// stdout carries no ANSI, and the removed per-source completion sentence never
/// appears on either stream — verified across a wide and a narrow `COLUMNS`.
#[test]
fn sync_summary_table_is_stdout_only_without_ansi_or_completion_sentence() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex("rollout-table.jsonl", 123, "2026-04-22T01:12:00Z")?;

    let run = |columns: &str| -> Result<std::process::Output> {
        Ok(Command::new(env!("CARGO_BIN_EXE_llmusage"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .arg("sync")
            .env("HOME", &fixture.home)
            .env("USERPROFILE", &fixture.home)
            .env("CODEX_HOME", &fixture.codex_home)
            .env("OPENCODE_HOME", &fixture.opencode_home)
            .env("RUST_LOG", "off")
            .env("COLUMNS", columns)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?)
    };

    // Wide terminal: the source parses and finishes with data on this first run.
    let wide = run("120")?;
    assert!(wide.status.success(), "{wide:?}");
    let wide_out = String::from_utf8_lossy(&wide.stdout);
    let wide_err = String::from_utf8_lossy(&wide.stderr);
    // stdout carries the summary table and its aggregated TOTAL row.
    assert!(wide_out.contains("Sync finished:"), "stdout={wide_out}");
    assert!(wide_out.contains("TOTAL"), "stdout={wide_out}");
    assert!(wide_out.contains("codex"), "stdout={wide_out}");
    // Non-TTY stdout has no ANSI control sequences.
    assert!(!wide_out.contains('\u{1b}'), "stdout={wide_out}");
    // The legacy standalone totals line is gone (replaced by the TOTAL row).
    assert!(!wide_out.contains("- totals:"), "stdout={wide_out}");
    // Progress stays on stderr: no progress copy leaks onto stdout.
    assert!(!wide_out.contains("导入"), "stdout={wide_out}");
    // The removed per-source completion sentence appears on neither stream.
    assert!(!wide_out.contains("完成，文件"), "stdout={wide_out}");
    assert!(!wide_err.contains("完成，文件"), "stderr={wide_err}");

    // Narrow terminal: the table still renders and stays ANSI-free end to end.
    let narrow = run("60")?;
    assert!(narrow.status.success(), "{narrow:?}");
    let narrow_out = String::from_utf8_lossy(&narrow.stdout);
    assert!(narrow_out.contains("Sync finished:"), "stdout={narrow_out}");
    assert!(narrow_out.contains("TOTAL"), "stdout={narrow_out}");
    assert!(!narrow_out.contains('\u{1b}'), "stdout={narrow_out}");

    fixture.restore_env();
    Ok(())
}

#[test]
fn v0_db_with_worker_lease_table_rename_to_worker_lock_succeeds() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = AppContext::discover()?;
    fs::create_dir_all(&app.paths.root_dir)?;
    let conn = Connection::open(&app.paths.db_path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE worker_lease (
            lock_name TEXT PRIMARY KEY,
            owner_id TEXT NOT NULL,
            lease_expires_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        INSERT INTO worker_lease(lock_name, owner_id, lease_expires_at, updated_at)
        VALUES ('sync-worker', 'legacy-owner', '2000-01-01T00:00:00Z', '1999-12-31T23:59:59Z');
        "#,
    )?;
    drop(conn);

    let store = Store::new(&app.paths)?;
    store.bootstrap()?;

    let conn = Connection::open(&app.paths.db_path)?;
    let worker_lock_columns = table_columns(&conn, "worker_lock")?;
    assert!(
        worker_lock_columns
            .iter()
            .any(|column| column == "holder_pid")
    );
    assert!(
        worker_lock_columns
            .iter()
            .any(|column| column == "holder_kind")
    );
    assert!(
        worker_lock_columns
            .iter()
            .any(|column| column == "acquired_at")
    );
    let legacy_table_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='worker_lease'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(legacy_table_count, 0);
    assert_eq!(
        llmusage::store::read_schema_version(&conn)?,
        llmusage::store::latest_schema_version()
    );

    let lock = store.acquire_worker_lock_with(Duration::from_millis(1), HolderKind::Cli)?;
    assert_eq!(lock.meta().holder_kind, "cli");

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
        let first_part_rowid = store.cursors().load_opencode_cursor()?.last_part_rowid;
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
        assert!(store.cursors().load_opencode_cursor()?.last_part_rowid > first_part_rowid);
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
        let first_cursor = store.cursors().load_opencode_cursor()?;
        assert_eq!(first_cursor.last_time_created, 1776823200000);
        assert_eq!(usage_event_count(&app.paths.db_path)?, 1);

        fixture.replace_opencode_db("msg-replaced", 1776823100000, 48)?;
        commands::sync::run(&app).await?;

        let second_cursor = store.cursors().load_opencode_cursor()?;
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

        let cursor = store.cursors().load_opencode_cursor()?;
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

#[test]
fn sync_failure_marks_run_failed_immediately() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_broken_opencode_schema()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let err = commands::sync::run(&app)
            .await
            .expect_err("sync should fail");
        assert!(!err.to_string().trim().is_empty());

        let run = latest_run_record(&app.paths.db_path, "sync")?;
        assert_failed_run(&run);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn export_failure_marks_run_failed_immediately() -> Result<()> {
    let fixture = Fixture::new()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let blocked_out = fixture.home.join("blocked-export-path");
        fs::write(&blocked_out, "occupied")?;

        let err = commands::export::run_html(&app, Some(blocked_out))
            .await
            .expect_err("export should fail");
        assert!(!err.to_string().trim().is_empty());

        let run = latest_run_record(&app.paths.db_path, "export html")?;
        assert_failed_run(&run);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn sync_failure_from_invalid_active_pricing_snapshot_marks_run_failed() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex("bad-pricing.jsonl", 120, "2026-04-22T01:12:00Z")?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let pricing_dir = app.paths.root_dir.join("pricing");
        fs::create_dir_all(&pricing_dir)?;
        fs::write(pricing_dir.join("broken-snapshot.json"), "{not-json")?;
        store.set_meta_value("pricing_catalog_version", "broken-snapshot")?;

        let err = commands::sync::run(&app)
            .await
            .expect_err("invalid active pricing snapshot should fail sync");
        assert!(
            err.to_string().contains("broken-snapshot"),
            "unexpected sync error: {err:#}"
        );

        let run = latest_run_record(&app.paths.db_path, "sync")?;
        assert_failed_run(&run);
        assert!(
            run.error
                .as_deref()
                .is_some_and(|value| value.contains("broken-snapshot")),
            "run_log error should identify the active snapshot: {run:?}"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn doctor_warns_on_recovered_aborted_runs() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = AppContext::discover()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    store.run_log().record_run_start("sync")?;
    store
        .run_log()
        .recover_running_runs(&["sync", "hook-run"])?;

    let health = Dashboard::open(&store)?.health()?;
    assert!(
        health
            .recent_failures
            .iter()
            .any(|run| run.status == "aborted")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_llmusage"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["doctor", "--json"])
        .env("HOME", &fixture.home)
        .env("USERPROFILE", &fixture.home)
        .env("CODEX_HOME", &fixture.codex_home)
        .env("OPENCODE_HOME", &fixture.opencode_home)
        .env("RUST_LOG", "off")
        .output()?;
    assert!(output.status.success(), "{output:?}");

    let checks: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let recent_failures = checks
        .as_array()
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("id").and_then(serde_json::Value::as_str) == Some("recent.failures")
            })
        })
        .expect("recent.failures check");
    assert_eq!(
        recent_failures
            .get("status")
            .and_then(serde_json::Value::as_str),
        Some("warn")
    );

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_first_sync_imports_only_turn_usage_with_raw_model() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤1：首次 sync 只导入 turn-scoped usage.record
     * ========================================================================
     * 目标：
     * 1) 只有 usageScope=turn 的非零 usage.record 成为 kimi_code 事件
     * 2) 四通道 + 饱和 total 正确
     * 3) 原始模型字符串（kimi-code/k3）逐字保留
     */
    let fixture = Fixture::new()?;
    fixture.seed_kimi_code(
        "sess-first",
        &[
            kimi_turn_line("kimi-code/k3", 5102, 172, 13312, 8, 1_780_319_377_000),
            // session-scoped aggregate is not per-turn usage.
            serde_json::json!({
                "type": "usage.record", "model": "kimi-code/k3",
                "usage": {"inputOther": 999, "output": 999, "inputCacheRead": 0, "inputCacheCreation": 0},
                "usageScope": "session", "time": 1_780_319_378_000i64
            })
            .to_string(),
            // step.end duplicates the turn usage but is not a usage.record.
            serde_json::json!({
                "type": "step.end",
                "usage": {"inputOther": 777, "output": 777, "inputCacheRead": 0, "inputCacheCreation": 0},
                "usageScope": "turn", "time": 1_780_319_379_000i64
            })
            .to_string(),
            // all-zero turn record is skipped.
            kimi_turn_line("kimi-code/k3", 0, 0, 0, 0, 1_780_319_380_000),
            // unrelated line type.
            serde_json::json!({
                "type": "context.append_loop_event",
                "event": {"type": "tool.call"}, "time": 1_780_319_381_000i64
            })
            .to_string(),
            // malformed line must not fail the whole file.
            "not valid json at all".to_string(),
            kimi_turn_line("kimi-code/k3", 100, 50, 0, 0, 1_780_319_382_000),
        ],
    )?;

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
                source: Some(SourceKind::KimiCode),
                ..Default::default()
            },
            None,
        )
        .await?;

        let stats = &summary.sources[0];
        assert_eq!(stats.source, SourceKind::KimiCode);
        assert_eq!(stats.changed_files, 1);
        assert_eq!(stats.events_seen, 2);
        assert_eq!(stats.events_inserted, 2);
        assert_eq!(stats.stored_events, 2);

        // Only the two turn records survive, with raw model + saturating total.
        let rows = kimi_event_rows(&app.paths.db_path)?;
        assert_eq!(
            rows,
            vec![
                (
                    "kimi-code/k3".to_string(),
                    5102,
                    13312,
                    8,
                    172,
                    5102 + 13312 + 8 + 172,
                ),
                ("kimi-code/k3".to_string(), 100, 0, 0, 50, 150),
            ]
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_sync_twice_is_idempotent() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤2：重复 sync 幂等（onboarding gate 核心测试）
     * ========================================================================
     * 目标：二次空跑 changed_files==0、skipped_files>0、事件数不变。
     */
    let fixture = Fixture::new()?;
    fixture.seed_kimi_code(
        "sess-hot",
        &[
            kimi_turn_line("kimi-code/k3", 5102, 172, 13312, 8, 1_780_319_377_000),
            kimi_turn_line("kimi-code/k3", 100, 50, 0, 0, 1_780_319_380_000),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };

        let first = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(first.sources[0].changed_files, 1);
        assert_eq!(first.sources[0].events_inserted, 2);
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 2);

        let second = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(second.sources[0].changed_files, 0);
        assert!(second.sources[0].skipped_files > 0);
        assert_eq!(second.sources[0].bytes_scanned, 0);
        assert_eq!(second.sources[0].events_inserted, 0);
        assert_eq!(second.sources[0].stored_events, 2);
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_append_imports_only_new_record() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤3：追加只导入新增记录（字节偏移事件键幂等）
     * ========================================================================
     */
    let fixture = Fixture::new()?;
    fixture.seed_kimi_code(
        "sess-append",
        &[
            kimi_turn_line("kimi-code/k3", 5102, 172, 13312, 8, 1_780_319_377_000),
            kimi_turn_line("kimi-code/k3", 100, 50, 0, 0, 1_780_319_380_000),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };

        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 2);

        fixture.append_kimi_code(
            "sess-append",
            &kimi_turn_line("kimi-code/k3", 7, 3, 1, 0, 1_780_319_390_000),
        )?;
        let appended =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(appended.sources[0].changed_files, 1);
        assert!(appended.sources[0].bytes_scanned > 0);
        assert_eq!(appended.sources[0].events_seen, 1);
        assert_eq!(appended.sources[0].events_inserted, 1);
        // The two earlier events are not duplicated: byte-offset keys hold.
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 3);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_rewrite_resets_and_replaces_old_rows() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤4：改写/截断触发整文件重放，旧行清理后替换
     * ========================================================================
     */
    let fixture = Fixture::new()?;
    fixture.seed_kimi_code(
        "sess-rewrite",
        &[
            kimi_turn_line("kimi-code/k3", 5102, 172, 13312, 8, 1_780_319_377_000),
            kimi_turn_line("kimi-code/k3", 100, 50, 0, 0, 1_780_319_380_000),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };

        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 2);

        // Rewrite with different, shorter content: fingerprint/size change forces
        // a full reparse whose reset clears the stale rows for this path.
        fixture.seed_kimi_code(
            "sess-rewrite",
            &[kimi_turn_line(
                "kimi-code/k4",
                42,
                8,
                0,
                0,
                1_780_319_400_000,
            )],
        )?;
        let replaced =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(replaced.sources[0].changed_files, 1);
        assert_eq!(replaced.sources[0].events_replayed, 1);

        let rows = kimi_event_rows(&app.paths.db_path)?;
        assert_eq!(rows, vec![("kimi-code/k4".to_string(), 42, 0, 0, 8, 50)]);
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_deleted_history_survives_regular_sync_and_blocks_rebuild() -> Result<()> {
    let fixture = Fixture::new()?;
    let wire_path = fixture.seed_kimi_code(
        "sess-missing-history",
        &[kimi_turn_line(
            "kimi-code/k3",
            100,
            50,
            10,
            0,
            1_780_319_377_000,
        )],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };

        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 1);

        fs::remove_file(&wire_path)?;
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;

        assert_eq!(kimi_event_count(&app.paths.db_path)?, 1);
        assert_eq!(
            store.source_files().counts(SourceKind::KimiCode)?.missing,
            1
        );
        let risk = store
            .source_files()
            .lossy_rebuild_risk(SourceKind::KimiCode)?;
        assert_eq!(risk.missing_file_count, 1);
        assert_eq!(risk.protected_event_count, 1);

        let blocked = commands::sync::run_with_options(
            &app,
            commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::KimiCode),
                ..Default::default()
            },
        )
        .await;
        let error = blocked.expect_err("missing Kimi history must block a lossy rebuild");
        assert!(
            error.to_string().contains("Refusing lossy sync --rebuild"),
            "{error:#}"
        );
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_missing_root_sync_succeeds_and_status_tracks_passive_data() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤5：缺失根 sync 成功且 source 状态在 passive_no_data/ready 间切换
     * ========================================================================
     */
    let fixture = Fixture::new()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;

        // No `.kimi-code` root at all: full sync still succeeds and imports zero
        // kimi events without marking other sources missing.
        commands::sync::run(&app).await?;
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 0);
        assert_eq!(store.source_files().counts(SourceKind::Codex)?.missing, 0);
        assert_eq!(kimi_capability_status(&app, &store)?, "passive_no_data");

        // Seed one wire.jsonl: passive status flips to ready after import.
        fixture.seed_kimi_code(
            "sess-late",
            &[kimi_turn_line(
                "kimi-code/k3",
                100,
                50,
                0,
                0,
                1_780_319_377_000,
            )],
        )?;
        commands::sync::run(&app).await?;
        assert_eq!(kimi_event_count(&app.paths.db_path)?, 1);
        assert_eq!(kimi_capability_status(&app, &store)?, "passive_ready");
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_code_home_override_and_raw_models_survive_query_layer() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤6：KIMI_CODE_HOME 覆盖 + 原始模型跨 query 层保留（AC2）
     * ========================================================================
     */
    let fixture = Fixture::new()?;
    let override_root = fixture.home.join("custom-kimi");
    fixture.seed_kimi_code_under(
        &override_root,
        "sess-override",
        &[
            kimi_turn_line("kimi-code/k3", 5102, 172, 13312, 8, 1_780_319_377_000),
            kimi_turn_line("kimi-code/k4-preview", 10, 5, 0, 0, 1_780_319_380_000),
        ],
    )?;
    unsafe {
        std::env::set_var("KIMI_CODE_HOME", &override_root);
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
                source: Some(SourceKind::KimiCode),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(summary.sources[0].events_inserted, 2);

        // model_breakdown reads the aggregated buckets: both raw model strings
        // survive event -> bucket -> query without whitelist/normalization.
        let filter = QueryFilter {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };
        let mut models = Dashboard::open(&store)?
            .model_breakdown(&filter)?
            .into_iter()
            .map(|breakdown| breakdown.model)
            .collect::<Vec<_>>();
        models.sort();
        assert_eq!(
            models,
            vec![
                "kimi-code/k3".to_string(),
                "kimi-code/k4-preview".to_string(),
            ]
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn kimi_first_sync_marks_current_token_accounting() -> Result<()> {
    /*
     * ========================================================================
     * Kimi 步骤7：首次成功 sync 写入记账 marker，二次 sync 不被 legacy 拒绝
     * ========================================================================
     */
    let fixture = Fixture::new()?;
    fixture.seed_kimi_code(
        "sess-marker",
        &[kimi_turn_line(
            "kimi-code/k3",
            100,
            50,
            0,
            0,
            1_780_319_377_000,
        )],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::KimiCode),
            ..Default::default()
        };

        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(
            store.token_accounting_version(SourceKind::KimiCode)?,
            Some(expected_token_accounting_version(SourceKind::KimiCode)),
        );
        assert_eq!(expected_token_accounting_version(SourceKind::KimiCode), 2);
        assert!(!store.has_legacy_token_accounting(SourceKind::KimiCode)?);

        // Current marker keeps normal incremental writes allowed (no refusal).
        let second = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(second.sources[0].changed_files, 0);
        assert_eq!(
            store.token_accounting_version(SourceKind::KimiCode)?,
            Some(2)
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn pi_combines_default_roots_and_preserves_usage_across_query() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_pi(
        "project-pi",
        "pi-first",
        &[
            // Pi accepts usage-bearing message records whose top-level type is absent.
            serde_json::json!({
                "timestamp": "2026-06-01T00:00:00Z",
                "message": {
                    "role": "assistant",
                    "model": "pi-future-model",
                    "usage": {
                        "input": 11,
                        "output": 7,
                        "cacheRead": 3,
                        "cacheWrite": 2,
                        "reasoningTokens": 5
                    }
                }
            })
            .to_string(),
            serde_json::json!({
                "type": "title",
                "message": {"role": "assistant", "usage": {"input": 999}}
            })
            .to_string(),
        ],
    )?;
    fixture.seed_omp(
        "project-omp",
        "omp-first",
        &[
            pi_message_line(
                "2026-06-01T00:05:00Z",
                "gpt-5.5",
                100,
                50,
                40,
                8,
                333,
                10,
            ),
            // Structurally usage-shaped but malformed token fields are ignored.
            r#"{"type":"message","timestamp":"2026-06-01T00:06:00Z","message":{"role":"assistant","model":"broken","usage":{"input":"bad","totalTokens":"bad"}}}"#.to_string(),
            r#"{"type":"message","timestamp":"2026-06-01T00:07:00Z","message":{"role":"user","model":"ignored","usage":{"input":100}}}"#.to_string(),
            "not json but mentions message and usage".to_string(),
        ],
    )?;

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
                source: Some(SourceKind::Pi),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_eq!(summary.sources.len(), 1);
        let stats = &summary.sources[0];
        assert_eq!(stats.source, SourceKind::Pi);
        assert_eq!(stats.files_processed, 2);
        assert_eq!(stats.changed_files, 2);
        assert_eq!(stats.events_seen, 2);
        assert_eq!(stats.events_inserted, 2);
        assert_eq!(stats.stored_events, 2);
        assert_eq!(
            pi_event_rows(&app.paths.db_path)?,
            vec![
                ("pi-future-model".to_string(), 11, 3, 2, 7, 5, 23),
                ("gpt-5.5".to_string(), 100, 40, 8, 50, 10, 333),
            ]
        );

        let mut models = Dashboard::open(&store)?
            .model_breakdown(&QueryFilter {
                source: Some(SourceKind::Pi),
                ..Default::default()
            })?
            .into_iter()
            .map(|row| row.model)
            .collect::<Vec<_>>();
        models.sort();
        assert_eq!(models, vec!["gpt-5.5", "pi-future-model"]);
        assert_eq!(
            store.token_accounting_version(SourceKind::Pi)?,
            Some(expected_token_accounting_version(SourceKind::Pi))
        );
        assert_eq!(expected_token_accounting_version(SourceKind::Pi), 2);
        assert!(!store.has_legacy_token_accounting(SourceKind::Pi)?);
        assert_eq!(
            llmusage::registry::source_descriptor(SourceKind::Pi)
                .expect("pi source descriptor")
                .display_name,
            "Pi / Oh My Pi"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn pi_missing_default_root_still_syncs_omp_and_projects_status() -> Result<()> {
    let fixture = Fixture::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Pi),
            ..Default::default()
        };

        let empty = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(empty.sources[0].files_processed, 0);
        assert_eq!(pi_capability_status(&app, &store)?, "passive_no_data");

        fixture.seed_omp(
            "project-omp",
            "omp-only",
            &[pi_message_line(
                "2026-06-02T00:00:00Z",
                "codex-auto-review",
                9,
                4,
                2,
                1,
                16,
                3,
            )],
        )?;
        let imported =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(imported.sources[0].files_processed, 1);
        assert_eq!(imported.sources[0].events_inserted, 1);
        assert_eq!(pi_capability_status(&app, &store)?, "passive_ready");

        let monitor = commands::source_status::build_platform_monitor_statuses()
            .into_iter()
            .find(|status| status.platform_id == "pi")
            .expect("pi platform monitor");
        assert_eq!(monitor.source, Some(SourceKind::Pi));
        assert_eq!(monitor.parser_status, "registered");
        assert_eq!(monitor.roots_checked, 2);
        assert_eq!(monitor.roots_detected, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn pi_repeat_append_and_rewrite_follow_file_cursor_contract() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_omp(
        "project-cursor",
        "cursor",
        &[
            pi_message_line("2026-06-03T00:00:00Z", "gpt-5.5", 10, 5, 2, 1, 18, 3),
            pi_message_line("2026-06-03T00:01:00Z", "gpt-5.5", 20, 6, 3, 1, 30, 4),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Pi),
            ..Default::default()
        };

        let first = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(first.sources[0].events_inserted, 2);
        assert_eq!(pi_event_count(&app.paths.db_path)?, 2);

        let repeat = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(repeat.sources[0].changed_files, 0);
        assert_eq!(repeat.sources[0].skipped_files, 1);
        assert_eq!(repeat.sources[0].bytes_scanned, 0);
        assert_eq!(repeat.sources[0].events_inserted, 0);
        assert_eq!(repeat.sources[0].stored_events, 2);

        fixture.append_omp(
            "project-cursor",
            "cursor",
            &pi_message_line("2026-06-03T00:02:00Z", "gpt-5.6", 7, 3, 1, 0, 11, 2),
        )?;
        let appended =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(appended.sources[0].changed_files, 1);
        assert_eq!(appended.sources[0].events_seen, 1);
        assert_eq!(appended.sources[0].events_inserted, 1);
        assert_eq!(pi_event_count(&app.paths.db_path)?, 3);

        fixture.seed_omp(
            "project-cursor",
            "cursor",
            &[pi_message_line(
                "2026-06-03T01:00:00Z",
                "gpt-6-rewrite",
                42,
                8,
                0,
                0,
                50,
                6,
            )],
        )?;
        let rewritten =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(rewritten.sources[0].changed_files, 1);
        assert_eq!(rewritten.sources[0].events_replayed, 1);
        assert_eq!(pi_event_count(&app.paths.db_path)?, 1);
        assert_eq!(
            pi_event_rows(&app.paths.db_path)?,
            vec![("gpt-6-rewrite".to_string(), 42, 0, 0, 8, 6, 50)]
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn pi_agent_dir_lists_multiple_roots_and_dedupes_canonical_files() -> Result<()> {
    let fixture = Fixture::new()?;
    let custom_root = fixture.home.join("custom-pi-sessions");
    let omp_root = fixture.home.join(".omp").join("agent").join("sessions");
    fixture.seed_pi(
        "project-default",
        "ignored-default",
        &[pi_message_line(
            "2026-06-04T00:00:00Z",
            "should-not-import",
            99,
            1,
            0,
            0,
            100,
            0,
        )],
    )?;
    fixture.seed_pi_under(
        &custom_root,
        "project-custom",
        "custom",
        &[pi_message_line(
            "2026-06-04T00:01:00Z",
            "custom-pi",
            10,
            4,
            1,
            0,
            15,
            2,
        )],
    )?;
    fixture.seed_omp(
        "project-omp",
        "dedupe",
        &[pi_message_line(
            "2026-06-04T00:02:00Z",
            "omp-model",
            12,
            5,
            2,
            1,
            20,
            3,
        )],
    )?;
    let omp_alias = omp_root.join("..").join("sessions");
    unsafe {
        std::env::set_var(
            "PI_AGENT_DIR",
            format!(
                "{},{},{},{}",
                custom_root.display(),
                omp_root.display(),
                omp_alias.display(),
                custom_root.display()
            ),
        );
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
                source: Some(SourceKind::Pi),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_eq!(summary.sources[0].files_processed, 2);
        assert_eq!(summary.sources[0].events_inserted, 2);
        let models = pi_event_rows(&app.paths.db_path)?
            .into_iter()
            .map(|row| row.0)
            .collect::<Vec<_>>();
        assert_eq!(models, vec!["custom-pi", "omp-model"]);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn grok_session_replay_converges_and_protects_missing_sidecars() -> Result<()> {
    let fixture = Fixture::new()?;
    let updates = concat!(
        "{\"params\":{\"update\":{\"sessionUpdate\":\"available_commands_update\"},\"_meta\":{\"totalTokens\":100,\"agentTimestampMs\":1700000000000}}}\n",
        "{\"params\":{\"update\":{\"sessionUpdate\":\"user_message_chunk\",\"_meta\":{\"modelId\":\"grok-4.5\"}},\"_meta\":{\"agentTimestampMs\":1700000001000}}}\n",
        "{\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\"},\"_meta\":{\"totalTokens\":300,\"agentTimestampMs\":1700000003000}}}\n"
    );
    let session_dir = fixture.seed_grok(
        "session-replay",
        updates,
        Some("{\"current_model_id\":\"grok-4.5\",\"updated_at\":\"2023-11-14T22:13:20Z\"}"),
        Some("{\"primaryModelId\":\"grok-4.5\",\"contextTokensUsed\":500}"),
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Grok),
            ..Default::default()
        };

        let first = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(first.sources[0].source, SourceKind::Grok);
        assert_eq!(first.sources[0].changed_files, 3);
        assert_eq!(first.sources[0].events_replayed, 0);
        assert_eq!(first.sources[0].events_inserted, 2);
        assert_grok_totals(&app.paths.db_path, 2, 500)?;
        assert!(grok_event_rows(&app.paths.db_path)?.iter().all(|row| {
            row.model == "grok-4.5"
                && row.input_tokens == 0
                && row.cache_read_tokens == 0
                && row.cache_creation_tokens == 0
                && row.output_tokens == 0
                && row.reasoning_tokens == 0
                && row.pricing_status == "unpriced"
                && row.provider_label.is_empty()
                && row.project_label.as_deref() == Some("demo")
        }));
        assert_eq!(
            llmusage::registry::source_descriptor(SourceKind::Grok)
                .expect("grok descriptor")
                .quality,
            llmusage::domain::source_descriptor::UsageQuality::TotalOnly
        );
        assert_eq!(
            source_capability_status(&app, &store, SourceKind::Grok)?,
            "passive_ready"
        );
        let grok_monitor = llmusage::registry::registered_platform_monitors()
            .iter()
            .find(|monitor| monitor.platform_id == "grok")
            .expect("grok monitor");
        let probe = llmusage::domain::platform_monitor::probe_platform_descriptor(
            grok_monitor,
            &fixture.home,
        );
        assert_eq!(
            probe.parser_status,
            llmusage::domain::platform_monitor::ParserSupportStatus::Registered
        );
        assert_eq!(
            probe.status,
            llmusage::domain::platform_monitor::PlatformProbeStatus::Detected
        );

        let second = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(second.sources[0].changed_files, 0);
        assert_eq!(second.sources[0].events_inserted, 0);
        assert_eq!(second.sources[0].bytes_scanned, 0);
        assert_grok_totals(&app.paths.db_path, 2, 500)?;

        fixture.append_grok_updates(
            "session-replay",
            concat!(
                "{\"params\":{\"update\":{\"sessionUpdate\":\"user_message_chunk\"},\"_meta\":{\"agentTimestampMs\":1700000004000}}}\n",
                "{\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\"},\"_meta\":{\"totalTokens\":500,\"agentTimestampMs\":1700000005000}}}\n"
            ),
        )?;
        let appended =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(appended.sources[0].changed_files, 3);
        assert_eq!(appended.sources[0].events_replayed, 3);
        assert_grok_totals(&app.paths.db_path, 3, 500)?;

        fixture.write_grok_sidecar(
            "session-replay",
            "signals.json",
            "{\"primaryModelId\":\"grok-4.5\",\"contextTokensUsed\":700,\"revision\":2}",
        )?;
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_grok_totals(&app.paths.db_path, 3, 700)?;

        // Later updates can cover the complete signals total. Session replay
        // removes the old reconciliation row instead of double counting it.
        fixture.write_grok_sidecar(
            "session-replay",
            "updates.jsonl",
            concat!(
                "{\"params\":{\"update\":{\"sessionUpdate\":\"user_message_chunk\",\"_meta\":{\"modelId\":\"grok-4.5\"}},\"_meta\":{\"agentTimestampMs\":1700000010000}}}\n",
                "{\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\"},\"_meta\":{\"totalTokens\":700,\"agentTimestampMs\":1700000011000}}}\n"
            ),
        )?;
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_grok_totals(&app.paths.db_path, 1, 700)?;
        assert_eq!(
            store.token_accounting_version(SourceKind::Grok)?,
            Some(expected_token_accounting_version(SourceKind::Grok))
        );

        let signals_path = session_dir.join("signals.json");
        fs::remove_file(&signals_path)?;
        let missing =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(missing.sources[0].changed_files, 0);
        assert_grok_totals(&app.paths.db_path, 1, 700)?;
        assert_eq!(store.source_files().counts(SourceKind::Grok)?.missing, 1);

        let blocked = commands::sync::run_with_options(
            &app,
            commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::Grok),
                ..Default::default()
            },
        )
        .await;
        assert!(
            blocked
                .expect_err("missing Grok sidecar must block rebuild")
                .to_string()
                .contains("Refusing lossy sync --rebuild")
        );

        fixture.write_grok_sidecar(
            "session-replay",
            "signals.json",
            "{\"primaryModelId\":\"grok-4.5\",\"contextTokensUsed\":800,\"revision\":3}",
        )?;
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_grok_totals(&app.paths.db_path, 2, 800)?;
        assert_eq!(store.source_files().counts(SourceKind::Grok)?.missing, 0);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn grok_missing_root_reports_passive_no_data() -> Result<()> {
    let fixture = Fixture::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let result = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Grok),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_eq!(result.sources[0].files_processed, 0);
        assert_eq!(result.sources[0].events_inserted, 0);
        assert_eq!(
            source_capability_status(&app, &store, SourceKind::Grok)?,
            "passive_no_data"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    fixture.restore_env();
    Ok(())
}

#[test]
fn grok_home_override_is_honored() -> Result<()> {
    let fixture = Fixture::new()?;
    let custom_root = fixture.home.join("custom-grok");
    fixture.seed_grok_under(
        &custom_root,
        "session-override",
        "{\"timestamp\":1700000000,\"totalTokens\":123}\n",
        Some("{\"current_model_id\":\"grok-future\",\"updated_at\":\"2023-11-14T22:13:20Z\"}"),
        None,
    )?;
    unsafe {
        std::env::set_var("GROK_HOME", &custom_root);
    }

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
                source: Some(SourceKind::Grok),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_grok_totals(&app.paths.db_path, 1, 123)?;
        assert_eq!(grok_event_rows(&app.paths.db_path)?[0].model, "grok-future");
        Ok::<_, anyhow::Error>(())
    })?;
    fixture.restore_env();
    Ok(())
}

#[derive(Debug)]
struct GrokEventRow {
    model: String,
    input_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    pricing_status: String,
    provider_label: String,
    project_label: Option<String>,
}

fn grok_event_rows(db_path: &Path) -> Result<Vec<GrokEventRow>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT model, input_tokens, cache_read_tokens, cache_creation_tokens,
               output_tokens, reasoning_output_tokens, pricing_status,
               provider_label, project_label
        FROM usage_event
        WHERE source = 'grok'
        ORDER BY event_key
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(GrokEventRow {
            model: row.get(0)?,
            input_tokens: row.get(1)?,
            cache_read_tokens: row.get(2)?,
            cache_creation_tokens: row.get(3)?,
            output_tokens: row.get(4)?,
            reasoning_tokens: row.get(5)?,
            pricing_status: row.get(6)?,
            provider_label: row.get(7)?,
            project_label: row.get(8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn assert_grok_totals(db_path: &Path, expected_events: i64, expected_total: i64) -> Result<()> {
    let conn = Connection::open(db_path)?;
    let (events, total): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(total_tokens), 0) FROM usage_event WHERE source = 'grok'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let bucket_total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(total_tokens), 0) FROM usage_bucket_30m WHERE source = 'grok'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(events, expected_events);
    assert_eq!(total, expected_total);
    assert_eq!(bucket_total, expected_total);
    Ok(())
}

fn pi_event_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'pi'",
        [],
        |row| row.get(0),
    )?)
}

/// One stored Pi row as `(model, input, cache_read, cache_creation, output, reasoning, total)`.
type PiEventRow = (String, i64, i64, i64, i64, i64, i64);

fn pi_event_rows(db_path: &Path) -> Result<Vec<PiEventRow>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT model, input_tokens, cache_read_tokens, cache_creation_tokens,
               output_tokens, reasoning_output_tokens, total_tokens
        FROM usage_event
        WHERE source = 'pi'
        ORDER BY event_at, model
        "#,
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn kimi_event_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'kimi_code'",
        [],
        |row| row.get(0),
    )?)
}

/// One stored kimi row as `(model, input, cache_read, cache_creation, output, total)`.
type KimiEventRow = (String, i64, i64, i64, i64, i64);

/// Returns every stored `kimi_code` event ordered by event time then model.
fn kimi_event_rows(db_path: &Path) -> Result<Vec<KimiEventRow>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT model, input_tokens, cache_read_tokens, cache_creation_tokens,
               output_tokens, total_tokens
        FROM usage_event
        WHERE source = 'kimi_code'
        ORDER BY event_at, model
        "#,
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Projects the Kimi Code passive source status through the same entry point the
/// `source-status` command uses (`passive_no_data` vs `passive_ready`).
fn kimi_capability_status(app: &AppContext, store: &Store) -> Result<String> {
    source_capability_status(app, store, SourceKind::KimiCode)
}

fn pi_capability_status(app: &AppContext, store: &Store) -> Result<String> {
    source_capability_status(app, store, SourceKind::Pi)
}

fn source_capability_status(app: &AppContext, store: &Store, source: SourceKind) -> Result<String> {
    let sources = Dashboard::open(store)?.source_breakdown(&Default::default())?;
    let _ = app;
    let status = llmusage::commands::source_status::build_source_capability_statuses(&sources)
        .into_iter()
        .find(|status| status.source == source)
        .expect("source capability status present");
    Ok(status.status.to_string())
}

/// One synthetic ZCode `model_usage` row used by the zcode fixture helpers.
#[derive(Debug, Clone)]
struct ZcodeRowFixture {
    id: &'static str,
    session_id: &'static str,
    model_id: &'static str,
    status: &'static str,
    started_at: i64,
    completed_at: i64,
    input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    provider_total_tokens: Option<i64>,
    computed_total_tokens: Option<i64>,
}

impl Default for ZcodeRowFixture {
    fn default() -> Self {
        Self {
            id: "usage-row",
            session_id: "sess-1",
            model_id: "GLM-5.3",
            status: "completed",
            started_at: 1_780_000_000_000,
            completed_at: 1_780_000_001_000,
            input_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            provider_total_tokens: None,
            computed_total_tokens: None,
        }
    }
}

fn zcode_row(id: &'static str, completed_at: i64, input: i64, output: i64) -> ZcodeRowFixture {
    ZcodeRowFixture {
        id,
        completed_at,
        input_tokens: input,
        output_tokens: output,
        computed_total_tokens: Some(input + output),
        ..ZcodeRowFixture::default()
    }
}

fn zcode_source_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'zcode'",
        [],
        |row| row.get(0),
    )?)
}

// ============================================================================
// Antigravity 合成 wire 编码（protobuf varint / len-delimited，全脱敏）
// ============================================================================

fn ag_varint(value: u64, out: &mut Vec<u8>) {
    let mut value = value;
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn ag_varint_field(field_no: u32, value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    ag_varint(u64::from(field_no) << 3, &mut out);
    ag_varint(value, &mut out);
    out
}

fn ag_bytes_field(field_no: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    ag_varint((u64::from(field_no) << 3) | 2, &mut out);
    ag_varint(payload.len() as u64, &mut out);
    out.extend_from_slice(payload);
    out
}

fn ag_string_field(field_no: u32, value: &str) -> Vec<u8> {
    ag_bytes_field(field_no, value.as_bytes())
}

/// `chatModel.#9.#4` timestamp message `{#1 秒, #2 纳秒}` wrapper.
fn ag_timestamp_message(seconds: u64, nanos: u64) -> Vec<u8> {
    let mut stamp = Vec::new();
    stamp.extend_from_slice(&ag_varint_field(1, seconds));
    stamp.extend_from_slice(&ag_varint_field(2, nanos));
    ag_bytes_field(4, &stamp)
}

/// usage 子消息（chatModel.#4）：#3 checksum 恒 = #9 + #10。
fn ag_usage_message(
    input: u64,
    output: u64,
    thinking: u64,
    cache_read: u64,
    response_id: &str,
) -> Vec<u8> {
    let mut usage = Vec::new();
    usage.extend_from_slice(&ag_varint_field(1, 1132));
    usage.extend_from_slice(&ag_varint_field(2, input));
    usage.extend_from_slice(&ag_varint_field(3, output + thinking));
    if cache_read > 0 {
        usage.extend_from_slice(&ag_varint_field(5, cache_read));
    }
    usage.extend_from_slice(&ag_varint_field(6, 24));
    usage.extend_from_slice(&ag_varint_field(9, output));
    usage.extend_from_slice(&ag_varint_field(10, thinking));
    usage.extend_from_slice(&ag_string_field(11, response_id));
    usage
}

/// 完整 gen_metadata blob：chatModel(#1) 嵌套 usage/model/label/timestamp +
/// 顶层 #4 干扰字段。
#[allow(clippy::too_many_arguments)]
fn ag_gen_metadata_blob(
    input: u64,
    output: u64,
    thinking: u64,
    cache_read: u64,
    response_id: &str,
    model: Option<&str>,
    label: Option<&str>,
    timestamp_seconds: u64,
) -> Vec<u8> {
    let usage = ag_usage_message(input, output, thinking, cache_read, response_id);
    let mut chat_model = Vec::new();
    chat_model.extend_from_slice(&ag_bytes_field(4, &usage));
    chat_model.extend_from_slice(&ag_bytes_field(
        9,
        &ag_timestamp_message(timestamp_seconds, 657_105_100),
    ));
    if let Some(model) = model {
        chat_model.extend_from_slice(&ag_string_field(19, model));
    }
    if let Some(label) = label {
        chat_model.extend_from_slice(&ag_string_field(21, label));
    }
    let mut blob = Vec::new();
    blob.extend_from_slice(&ag_bytes_field(1, &chat_model));
    blob.extend_from_slice(&ag_bytes_field(4, &[0u8; 36]));
    blob
}

/// `trajectory_metadata_blob` 行：#2 created-at + #1.#1 workspace URI。
fn antigravity_trajectory_blob() -> Vec<u8> {
    let mut stamp = Vec::new();
    stamp.extend_from_slice(&ag_varint_field(1, 1_785_140_245));
    stamp.extend_from_slice(&ag_varint_field(2, 657_105_100));
    let mut workspace = Vec::new();
    workspace.extend_from_slice(&ag_string_field(1, "file:///D:/Documents/demo"));
    let mut trajectory = Vec::new();
    trajectory.extend_from_slice(&ag_bytes_field(1, &workspace));
    trajectory.extend_from_slice(&ag_bytes_field(2, &stamp));
    trajectory
}

fn antigravity_event_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'antigravity'",
        [],
        |row| row.get(0),
    )?)
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
        let counts = store.source_files().counts(SourceKind::Antigravity)?;
        assert_eq!(counts.missing, 1);
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

/// P0 升级路径：真实旧 key 形状的存量行（ADR-0009：迁移只改 source 不改
/// key）与新导入行共存；无界 sync / 自动 legacy 修复不删除存量行。
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

/// bounded run：按事件时间过滤，不推进 cursor、不 reset；窗口外历史由
/// 随后的全量 sync 恢复。
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

/// A request that starts before the watermark but completes after it must not
/// be missed: the watermark anchors on `completed_at`, not `started_at`.
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
    fixture.insert_zcode_row(error_row)?;
    let mut cancelled_row = zcode_row("cancel-1", 3_000, 0, 0);
    cancelled_row.status = "cancelled";
    fixture.insert_zcode_row(cancelled_row)?;

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
        assert_eq!(
            summary.sources[0].parse_issues.malformed_lines, 2,
            "error and cancelled rows are counted, not imported"
        );
        assert_eq!(zcode_source_count(&app.paths.db_path)?, 1);
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

/// A bounded `--recent-days` run may reuse the stored watermark as a lower
/// bound but must not advance it; a later full sync still recovers history
/// outside the window (source-sync-contracts).
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

        let cursor = store.cursors().load_zcode_cursor()?;
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

fn dsh_event_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'deepseek_harness'",
        [],
        |row| row.get(0),
    )?)
}

fn dsh_event_keys(db_path: &Path) -> Result<Vec<String>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT event_key FROM usage_event WHERE source = 'deepseek_harness' ORDER BY event_key",
    )?;
    Ok(stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn dsh_session_line(id: &str, parent: Option<&str>, seed_length: Option<i64>) -> String {
    let mut value = serde_json::json!({
        "type": "session",
        "version": 0,
        "id": id,
        "cwd": "/tmp/demo",
    });
    if let Some(parent) = parent {
        value["parentSession"] = serde_json::json!(parent);
    }
    if let Some(seed) = seed_length {
        value["seedLength"] = serde_json::json!(seed);
    }
    value.to_string()
}

fn dsh_usage_line(seq: i64, time_ms: i64, message_id: &str, input: i64, output: i64) -> String {
    serde_json::json!({
        "type": "assistant/message",
        "seq": seq,
        "time": time_ms,
        "data": {
            "usage": {
                "inputTokens": input,
                "outputTokens": output,
                "cacheReadTokens": 0,
                "cacheWriteTokens": 0,
                "reasoningTokens": 0,
            },
            "message": {
                "id": message_id,
                "source": {
                    "kind": "model",
                    "provider": "deepseek-official",
                    "model": "deepseek-v4-flash",
                }
            }
        }
    })
    .to_string()
}

fn dsh_encode_frames(lines: &[String]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for line in lines {
        let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 0)?;
        encoder.write_all(format!("{line}\n").as_bytes())?;
        out.extend(encoder.finish()?);
    }
    Ok(out)
}

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
            .load_file_cursors(SourceKind::DeepseekHarness)?;
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
            .load_file_cursors(SourceKind::DeepseekHarness)?;
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

fn usage_event_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    let count = conn.query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
    Ok(count)
}

fn assert_provider_label(db_path: &Path, expected: &str) -> Result<()> {
    let conn = Connection::open(db_path)?;
    let event_labels = {
        let mut stmt = conn.prepare("SELECT provider_label FROM usage_event ORDER BY event_key")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    assert_eq!(event_labels, vec![expected.to_string()]);

    let bucket_labels = {
        let mut stmt =
            conn.prepare("SELECT provider_label FROM usage_bucket_30m ORDER BY provider_label")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    assert_eq!(bucket_labels, vec![expected.to_string()]);
    Ok(())
}

fn usage_tool_call_count(db_path: &Path) -> Result<i64> {
    let conn = Connection::open(db_path)?;
    let count = conn.query_row("SELECT COUNT(*) FROM usage_tool_call", [], |row| row.get(0))?;
    Ok(count)
}

fn opencode_mcp_servers(db_path: &Path) -> Result<Vec<String>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT mcp_server FROM usage_tool_call WHERE tool_kind = 'mcp' AND mcp_server IS NOT NULL ORDER BY mcp_server",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[derive(Debug)]
struct RunLogRecord {
    status: String,
    error: Option<String>,
    finished_at: Option<String>,
    duration_ms: Option<i64>,
}

fn latest_run_record(db_path: &Path, command: &str) -> Result<RunLogRecord> {
    let conn = Connection::open(db_path)?;
    let run = conn.query_row(
        r#"
        SELECT status, error, finished_at, duration_ms
        FROM run_log
        WHERE command = ?1
        ORDER BY id DESC
        LIMIT 1
        "#,
        [command],
        |row| {
            Ok(RunLogRecord {
                status: row.get(0)?,
                error: row.get(1)?,
                finished_at: row.get(2)?,
                duration_ms: row.get(3)?,
            })
        },
    )?;
    Ok(run)
}

fn assert_failed_run(run: &RunLogRecord) {
    assert_eq!(run.status, "failed");
    assert!(
        run.error
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    );
    assert!(run.finished_at.is_some());
    assert!(run.duration_ms.is_some());
}

struct Fixture {
    _root: TempDir,
    home: PathBuf,
    codex_home: PathBuf,
    ccr_root: PathBuf,
    opencode_home: PathBuf,
    saved: Vec<(String, Option<String>)>,
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = TempDir::new()?;
        let home = root.path().join("home");
        let codex_home = home.join(".codex");
        let ccr_root = home.join(".ccr");
        let opencode_home = root.path().join("opencode-home");
        fs::create_dir_all(&home)?;
        fs::create_dir_all(&codex_home)?;
        fs::create_dir_all(&ccr_root)?;
        fs::create_dir_all(&opencode_home)?;

        let mut saved = Vec::new();
        for key in [
            "HOME",
            "USERPROFILE",
            "CODEX_HOME",
            "CCR_ROOT",
            "OPENCODE_HOME",
            "OPENCODE_DB",
            "KIMI_CODE_HOME",
            "PI_AGENT_DIR",
            "GROK_HOME",
            "ZCODE_HOME",
            "GEMINI_CLI_HOME",
            "DSH_HOME",
        ] {
            saved.push((key.to_string(), std::env::var(key).ok()));
        }
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("USERPROFILE", &home);
            std::env::set_var("CODEX_HOME", &codex_home);
            std::env::set_var("CCR_ROOT", &ccr_root);
            std::env::set_var("OPENCODE_HOME", &opencode_home);
            // Kimi Code discovery falls back to `$HOME/.kimi-code/sessions`;
            // clear any real developer override so the temp HOME is authoritative.
            std::env::remove_var("KIMI_CODE_HOME");
            // Pi discovery falls back to the two roots under the temp HOME.
            std::env::remove_var("PI_AGENT_DIR");
            // Grok discovery also falls back under the isolated temp HOME.
            std::env::remove_var("GROK_HOME");
            // ZCode / Antigravity CLI / dsh discovery fall back under the temp HOME.
            std::env::remove_var("ZCODE_HOME");
            std::env::remove_var("GEMINI_CLI_HOME");
            std::env::remove_var("DSH_HOME");
        }

        fs::create_dir_all(home.join(".claude").join("projects").join("demo"))?;
        write_git_repo(&home.join("workspace").join("demo-repo"))?;

        Ok(Self {
            _root: root,
            home,
            codex_home,
            ccr_root,
            opencode_home,
            saved,
        })
    }

    fn restore_env(&self) {
        for (key, value) in &self.saved {
            unsafe {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }

    fn seed_codex(&self, name: &str, total_tokens: i64, timestamp: &str) -> Result<()> {
        let sessions_dir = self
            .codex_home
            .join("sessions")
            .join("2026")
            .join("04")
            .join("22");
        fs::create_dir_all(&sessions_dir)?;
        let repo_root = self.home.join("workspace").join("demo-repo");
        let payload = format!(
            "{}\n",
            [
                serde_json::json!({
                    "type": "session_meta",
                    "payload": {
                        "model": "gpt-5",
                        "cwd": repo_root.to_string_lossy().to_string(),
                    }
                })
                .to_string(),
                codex_token_line(timestamp, total_tokens, total_tokens),
            ]
            .join("\n")
        );
        fs::write(sessions_dir.join(name), payload)?;
        Ok(())
    }

    fn write_provider_map(&self, contents: &str) -> Result<PathBuf> {
        let path = self
            .ccr_root
            .join("analytics")
            .join("provider_activation.jsonl");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, contents)?;
        Ok(path)
    }

    fn append_codex(&self, name: &str, total_tokens: i64, timestamp: &str) -> Result<()> {
        let path = self
            .codex_home
            .join("sessions")
            .join("2026")
            .join("04")
            .join("22")
            .join(name);
        let payload = format!("{}\n", codex_token_line(timestamp, total_tokens, 153));
        fs::OpenOptions::new()
            .append(true)
            .open(path)?
            .write_all(payload.as_bytes())?;
        Ok(())
    }

    fn replace_codex(&self, name: &str, total_tokens: i64, timestamp: &str) -> Result<()> {
        let path = self
            .codex_home
            .join("sessions")
            .join("2026")
            .join("04")
            .join("22")
            .join(name);
        let repo_root = self.home.join("workspace").join("demo-repo");
        let payload = [
            serde_json::json!({
                "type": "session_meta",
                "payload": {
                    "model": "gpt-5",
                    "cwd": repo_root.to_string_lossy().to_string(),
                }
            })
            .to_string(),
            codex_token_line(timestamp, total_tokens, total_tokens),
        ]
        .join("\n");
        fs::write(path, payload)?;
        Ok(())
    }

    fn remove_codex(&self, name: &str) -> Result<()> {
        let path = self
            .codex_home
            .join("sessions")
            .join("2026")
            .join("04")
            .join("22")
            .join(name);
        fs::remove_file(path)?;
        Ok(())
    }

    /// Absolute path of a Kimi Code `wire.jsonl` under the given sessions root,
    /// mirroring the real `sessions/WORKSPACE/SESSION/agents/AGENT` layout.
    fn kimi_wire_path(root: &Path, session: &str) -> PathBuf {
        root.join("sessions")
            .join("workspace-1")
            .join(session)
            .join("agents")
            .join("main")
            .join("wire.jsonl")
    }

    /// Seeds a synthetic `wire.jsonl` under the default `$HOME/.kimi-code` root
    /// (discovered via the parser's home fallback), overwriting any prior file.
    fn seed_kimi_code(&self, session: &str, lines: &[String]) -> Result<PathBuf> {
        self.seed_kimi_code_under(&self.home.join(".kimi-code"), session, lines)
    }

    /// Seeds a synthetic `wire.jsonl` under an explicit sessions root, used to
    /// exercise the `KIMI_CODE_HOME` override path.
    fn seed_kimi_code_under(
        &self,
        root: &Path,
        session: &str,
        lines: &[String],
    ) -> Result<PathBuf> {
        let path = Self::kimi_wire_path(root, session);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, format!("{}\n", lines.join("\n")))?;
        Ok(path)
    }

    /// Appends one raw JSONL line to an existing default-root `wire.jsonl`.
    fn append_kimi_code(&self, session: &str, line: &str) -> Result<()> {
        let path = Self::kimi_wire_path(&self.home.join(".kimi-code"), session);
        fs::OpenOptions::new()
            .append(true)
            .open(path)?
            .write_all(format!("{line}\n").as_bytes())?;
        Ok(())
    }

    fn grok_session_dir(&self, session: &str) -> PathBuf {
        self.home
            .join(".grok")
            .join("sessions")
            .join("D%3A%5Cwork%5Cdemo")
            .join(session)
    }

    fn seed_grok(
        &self,
        session: &str,
        updates: &str,
        summary: Option<&str>,
        signals: Option<&str>,
    ) -> Result<PathBuf> {
        self.seed_grok_under(&self.home.join(".grok"), session, updates, summary, signals)
    }

    fn seed_grok_under(
        &self,
        root: &Path,
        session: &str,
        updates: &str,
        summary: Option<&str>,
        signals: Option<&str>,
    ) -> Result<PathBuf> {
        let session_dir = root
            .join("sessions")
            .join("D%3A%5Cwork%5Cdemo")
            .join(session);
        fs::create_dir_all(&session_dir)?;
        fs::write(session_dir.join("updates.jsonl"), updates)?;
        if let Some(summary) = summary {
            fs::write(session_dir.join("summary.json"), summary)?;
        }
        if let Some(signals) = signals {
            fs::write(session_dir.join("signals.json"), signals)?;
        }
        Ok(session_dir)
    }

    fn write_grok_sidecar(&self, session: &str, name: &str, content: &str) -> Result<()> {
        fs::write(self.grok_session_dir(session).join(name), content)?;
        Ok(())
    }

    fn append_grok_updates(&self, session: &str, content: &str) -> Result<()> {
        fs::OpenOptions::new()
            .append(true)
            .open(self.grok_session_dir(session).join("updates.jsonl"))?
            .write_all(content.as_bytes())?;
        Ok(())
    }

    fn pi_session_path(root: &Path, project: &str, session: &str) -> PathBuf {
        root.join(project).join(format!("agent_{session}.jsonl"))
    }

    fn seed_pi(&self, project: &str, session: &str, lines: &[String]) -> Result<PathBuf> {
        self.seed_pi_under(
            &self.home.join(".pi").join("agent").join("sessions"),
            project,
            session,
            lines,
        )
    }

    fn seed_omp(&self, project: &str, session: &str, lines: &[String]) -> Result<PathBuf> {
        self.seed_pi_under(
            &self.home.join(".omp").join("agent").join("sessions"),
            project,
            session,
            lines,
        )
    }

    fn seed_pi_under(
        &self,
        root: &Path,
        project: &str,
        session: &str,
        lines: &[String],
    ) -> Result<PathBuf> {
        let path = Self::pi_session_path(root, project, session);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, format!("{}\n", lines.join("\n")))?;
        Ok(path)
    }

    fn append_omp(&self, project: &str, session: &str, line: &str) -> Result<()> {
        let root = self.home.join(".omp").join("agent").join("sessions");
        let path = Self::pi_session_path(&root, project, session);
        fs::OpenOptions::new()
            .append(true)
            .open(path)?
            .write_all(format!("{line}\n").as_bytes())?;
        Ok(())
    }

    fn seed_claude(&self, name: &str, total_tokens: i64, timestamp: &str) -> Result<()> {
        self.seed_claude_lines("demo", name, &[claude_usage_line(timestamp, total_tokens)])?;
        Ok(())
    }

    fn seed_claude_lines(&self, project: &str, name: &str, lines: &[String]) -> Result<PathBuf> {
        let claude_file = self
            .home
            .join(".claude")
            .join("projects")
            .join(project)
            .join(name);
        if let Some(parent) = claude_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&claude_file, format!("{}\n", lines.join("\n")))?;
        Ok(claude_file)
    }

    fn append_claude_line(&self, project: &str, name: &str, line: &str) -> Result<()> {
        let claude_file = self
            .home
            .join(".claude")
            .join("projects")
            .join(project)
            .join(name);
        fs::OpenOptions::new()
            .append(true)
            .open(claude_file)?
            .write_all(format!("{line}\n").as_bytes())?;
        Ok(())
    }

    fn append_claude(&self, name: &str, total_tokens: i64, timestamp: &str) -> Result<()> {
        let claude_file = self
            .home
            .join(".claude")
            .join("projects")
            .join("demo")
            .join(name);
        let payload = format!("{}\n", claude_usage_line(timestamp, total_tokens));
        fs::OpenOptions::new()
            .append(true)
            .open(claude_file)?
            .write_all(payload.as_bytes())?;
        Ok(())
    }

    fn seed_opencode(&self, message_id: &str, time_created: i64, total_tokens: i64) -> Result<()> {
        let db_path = self.opencode_home.join("opencode.db");
        self.seed_opencode_at(&db_path, message_id, time_created, total_tokens)
    }

    fn seed_opencode_at(
        &self,
        db_path: &Path,
        message_id: &str,
        time_created: i64,
        total_tokens: i64,
    ) -> Result<()> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS project(id TEXT PRIMARY KEY, worktree TEXT);
            CREATE TABLE IF NOT EXISTS session(id TEXT PRIMARY KEY, project_id TEXT);
            CREATE TABLE IF NOT EXISTS message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
            "#,
        )?;
        let repo_root = self.home.join("workspace").join("demo-repo");
        conn.execute(
            "INSERT OR IGNORE INTO project(id, worktree) VALUES (?1, ?2)",
            (&"project-1", &repo_root.to_string_lossy().to_string()),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO session(id, project_id) VALUES (?1, ?2)",
            (&"session-1", &"project-1"),
        )?;
        let message = serde_json::json!({
            "id": message_id,
            "role": "assistant",
            "modelID": "gpt-5",
            "tokens": {
                "input": total_tokens,
                "output": 0,
                "reasoning": 0,
                "cache": { "read": 0, "write": 0 }
            },
            "time": {
                "created": time_created,
                "completed": time_created
            }
        });
        conn.execute(
            "INSERT OR REPLACE INTO message(id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (&message_id, &"session-1", &time_created, &message.to_string()),
        )?;
        Ok(())
    }

    fn seed_broken_opencode_schema(&self) -> Result<()> {
        let db_path = self.opencode_home.join("opencode.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("CREATE TABLE broken(id INTEGER PRIMARY KEY);")?;
        Ok(())
    }

    fn replace_opencode_db(
        &self,
        message_id: &str,
        time_created: i64,
        total_tokens: i64,
    ) -> Result<()> {
        let db_path = self.opencode_home.join("opencode.db");
        if db_path.exists() {
            fs::remove_file(&db_path)?;
        }
        self.seed_opencode(message_id, time_created, total_tokens)
    }

    fn seed_opencode_tool_part(
        &self,
        part_id: &str,
        message_id: &str,
        session_id: &str,
        time_created: i64,
        data: serde_json::Value,
    ) -> Result<()> {
        let db_path = self.opencode_home.join("opencode.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS part(id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO part(id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (&part_id, &message_id, &session_id, &time_created, &data.to_string()),
        )?;
        Ok(())
    }

    /// Path of the synthetic ZCode usage DB under the fixture HOME.
    fn zcode_db_path(&self) -> PathBuf {
        self.home
            .join(".zcode")
            .join("cli")
            .join("db")
            .join("db.sqlite")
    }

    /// Creates or reopens the synthetic ZCode `model_usage` database.
    fn zcode_connection(&self) -> Result<Connection> {
        let db_path = self.zcode_db_path();
        fs::create_dir_all(db_path.parent().unwrap())?;
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS session(
                id TEXT PRIMARY KEY,
                directory TEXT,
                path TEXT
            );
            CREATE TABLE IF NOT EXISTS model_usage(
                id TEXT PRIMARY KEY,
                session_id TEXT,
                model_id TEXT,
                status TEXT,
                started_at INTEGER,
                completed_at INTEGER,
                input_tokens INTEGER,
                output_tokens INTEGER,
                reasoning_tokens INTEGER,
                cache_creation_input_tokens INTEGER,
                cache_read_input_tokens INTEGER,
                provider_total_tokens INTEGER,
                computed_total_tokens INTEGER
            );
            INSERT OR IGNORE INTO session(id, directory, path)
            VALUES ('sess-1', '', '');
            "#,
        )?;
        Ok(conn)
    }

    /// Inserts one synthetic completed `model_usage` row.
    fn insert_zcode_row(&self, row: ZcodeRowFixture) -> Result<()> {
        let conn = self.zcode_connection()?;
        conn.execute(
            "INSERT OR REPLACE INTO model_usage(
                id, session_id, model_id, status, started_at, completed_at,
                input_tokens, output_tokens, reasoning_tokens,
                cache_creation_input_tokens, cache_read_input_tokens,
                provider_total_tokens, computed_total_tokens
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![
                row.id,
                row.session_id,
                row.model_id,
                row.status,
                row.started_at,
                row.completed_at,
                row.input_tokens,
                row.output_tokens,
                row.reasoning_tokens,
                row.cache_creation_tokens,
                row.cache_read_tokens,
                row.provider_total_tokens,
                row.computed_total_tokens,
            ],
        )?;
        Ok(())
    }

    /// Rebuilds the synthetic ZCode DB from scratch, simulating database
    /// replacement (fresh ids, no anchor rows).
    fn rebuild_zcode_db(&self, rows: &[ZcodeRowFixture]) -> Result<()> {
        let db_path = self.zcode_db_path();
        if db_path.exists() {
            fs::remove_file(&db_path)?;
        }
        for row in rows {
            self.insert_zcode_row(row.clone())?;
        }
        Ok(())
    }

    /// Root of the synthetic Antigravity CLI conversations directory.
    fn antigravity_conversations_root(&self) -> PathBuf {
        self.home
            .join(".gemini")
            .join("antigravity-cli")
            .join("conversations")
    }

    /// Writes one synthetic Antigravity conversation DB carrying the given
    /// `gen_metadata` blobs (fully synthesized wire bytes, no prompt text).
    /// An existing file is replaced (rewrite semantics).
    fn seed_antigravity(&self, uuid: &str, blobs: &[(i64, Vec<u8>)]) -> Result<PathBuf> {
        let path = self
            .antigravity_conversations_root()
            .join(format!("{uuid}.db"));
        fs::create_dir_all(path.parent().unwrap())?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE gen_metadata(idx INTEGER PRIMARY KEY, data BLOB, size INTEGER);
            CREATE TABLE trajectory_metadata_blob(id TEXT, data BLOB);
            "#,
        )?;
        for (idx, blob) in blobs {
            conn.execute(
                "INSERT INTO gen_metadata(idx, data, size) VALUES (?1, ?2, ?3)",
                rusqlite::params![idx, blob, blob.len() as i64],
            )?;
        }
        let trajectory = antigravity_trajectory_blob();
        conn.execute(
            "INSERT INTO trajectory_metadata_blob(id, data) VALUES ('traj', ?1)",
            rusqlite::params![&trajectory],
        )?;
        drop(conn);
        Ok(path)
    }

    fn dsh_session_dir(root: &Path, session: &str) -> PathBuf {
        root.join("sessions").join("--tmp-demo--").join(session)
    }

    fn seed_dsh(&self, session: &str, lines: &[String]) -> Result<PathBuf> {
        self.seed_dsh_under(&self.home.join(".dsh"), session, lines)
    }

    fn seed_dsh_under(&self, root: &Path, session: &str, lines: &[String]) -> Result<PathBuf> {
        let path = Self::dsh_session_dir(root, session).join("session.jsonl");
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, format!("{}\n", lines.join("\n")))?;
        Ok(path)
    }

    fn seed_dsh_zstd(&self, session: &str, lines: &[String]) -> Result<PathBuf> {
        let path =
            Self::dsh_session_dir(&self.home.join(".dsh"), session).join("session.jsonl.zstd");
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, dsh_encode_frames(lines)?)?;
        Ok(path)
    }
}

fn write_git_repo(repo_root: &Path) -> Result<()> {
    fs::create_dir_all(repo_root.join(".git"))?;
    fs::write(
        repo_root.join(".git").join("config"),
        "[remote \"origin\"]\n    url = https://github.com/example/demo-repo.git\n",
    )?;
    Ok(())
}

fn codex_token_line(timestamp: &str, last_total: i64, total_total: i64) -> String {
    serde_json::json!({
        "timestamp": timestamp,
        "payload": {
            "type": "token_count",
            "info": {
                "last_token_usage": {
                    "input_tokens": last_total,
                    "cached_input_tokens": 0,
                    "output_tokens": 0,
                    "reasoning_output_tokens": 0,
                    "total_tokens": last_total,
                },
                "total_token_usage": {
                    "input_tokens": total_total,
                    "cached_input_tokens": 0,
                    "output_tokens": 0,
                    "reasoning_output_tokens": 0,
                    "total_tokens": total_total,
                }
            }
        }
    })
    .to_string()
}

/// Builds one turn-scoped Kimi Code `usage.record` line with synthetic tokens.
/// `time` is epoch milliseconds, matching the real wire format.
fn kimi_turn_line(
    model: &str,
    input_other: i64,
    output: i64,
    input_cache_read: i64,
    input_cache_creation: i64,
    time_ms: i64,
) -> String {
    serde_json::json!({
        "type": "usage.record",
        "model": model,
        "usage": {
            "inputOther": input_other,
            "output": output,
            "inputCacheRead": input_cache_read,
            "inputCacheCreation": input_cache_creation,
        },
        "usageScope": "turn",
        "time": time_ms,
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)]
fn pi_message_line(
    timestamp: &str,
    model: &str,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    total: i64,
    reasoning: i64,
) -> String {
    serde_json::json!({
        "type": "message",
        "timestamp": timestamp,
        "message": {
            "role": "assistant",
            "model": model,
            "usage": {
                "input": input,
                "output": output,
                "cacheRead": cache_read,
                "cacheWrite": cache_write,
                "totalTokens": total,
                "reasoningTokens": reasoning,
            }
        }
    })
    .to_string()
}

fn claude_usage_line(timestamp: &str, total_tokens: i64) -> String {
    serde_json::json!({
        "timestamp": timestamp,
        "message": {
            "model": "claude-sonnet-4",
            "usage": {
                "input_tokens": total_tokens,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "output_tokens": 0,
                "total_tokens": total_tokens,
            }
        }
    })
    .to_string()
}

fn claude_logical_usage_line(
    message_id: &str,
    request_id: &str,
    is_sidechain: bool,
    total_tokens: i64,
    timestamp: &str,
) -> String {
    serde_json::json!({
        "timestamp": timestamp,
        "sessionId": "session-claude",
        "requestId": request_id,
        "isSidechain": is_sidechain,
        "message": {
            "id": message_id,
            "model": "claude-sonnet-4",
            "usage": {
                "input_tokens": total_tokens,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "output_tokens": 0,
                "total_tokens": total_tokens,
            }
        }
    })
    .to_string()
}
