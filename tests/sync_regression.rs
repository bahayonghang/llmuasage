use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
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
        // One status per registered source: codex, claude, opencode, kimi_code,
        // and pi (parser-backed) plus the parserless antigravity source.
        assert_eq!(first_sync_status.len(), 6);

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
fn legacy_nonblocking_worker_lock_records_hook_kind() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = AppContext::discover()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;

    #[allow(deprecated)]
    let first = store
        .acquire_worker_lock()?
        .expect("legacy hook lock should acquire");
    assert_eq!(first.meta().holder_kind, "hook");
    #[allow(deprecated)]
    let second = store.acquire_worker_lock()?;
    assert!(second.is_none());

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
    let stale_lease_expires_at = "2000-01-01T00:00:00Z";
    let conn = Connection::open(&app.paths.db_path)?;
    conn.execute(
        "UPDATE worker_lock SET updated_at = ?1, lease_expires_at = ?2",
        (stale_updated_at, stale_lease_expires_at),
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
        if row.0 != stale_updated_at && row.1 != stale_lease_expires_at {
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
fn hook_run_skips_when_sync_holds_lock() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = AppContext::discover()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let _lock = store.acquire_worker_lock_with(Duration::from_millis(1), HolderKind::Cli)?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        commands::hook_run::run(&app, SourceKind::Codex, "manual-test", false).await?;
        Ok::<_, anyhow::Error>(())
    })?;

    let conn = Connection::open(&app.paths.db_path)?;
    let hook_runs: i64 = conn.query_row(
        "SELECT COUNT(*) FROM run_log WHERE command='hook-run'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(hook_runs, 0);
    let (started_at, finished_at) = trigger_worker_times(&app.paths.db_path, "codex")?;
    assert!(started_at.is_none());
    assert!(finished_at.is_none());

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
fn sync_blocks_when_hook_run_holds_lock_then_proceeds() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex("rollout-wait.jsonl", 77, "2026-04-22T01:12:00Z")?;
    let app = AppContext::discover()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let lock = {
        #[allow(deprecated)]
        store
            .acquire_worker_lock()?
            .expect("hook-style lock should acquire")
    };
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let home = fixture.home.clone();
    let codex_home = fixture.codex_home.clone();
    let opencode_home = fixture.opencode_home.clone();
    let handle = thread::spawn(move || -> Result<std::process::Output> {
        let child = Command::new(env!("CARGO_BIN_EXE_llmusage"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .arg("sync")
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("CODEX_HOME", &codex_home)
            .env("OPENCODE_HOME", &opencode_home)
            .env("RUST_LOG", "off")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        ready_tx.send(()).expect("send ready");
        release_rx.recv().expect("wait release signal");
        drop(lock);
        Ok(child.wait_with_output()?)
    });

    ready_rx.recv_timeout(Duration::from_secs(5))?;
    thread::sleep(Duration::from_millis(200));
    assert_eq!(usage_event_count(&app.paths.db_path)?, 0);
    release_tx.send(())?;
    let output = handle.join().expect("sync thread should not panic")?;
    assert!(output.status.success(), "{output:?}");
    assert_eq!(usage_event_count(&app.paths.db_path)?, 1);

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
fn hook_run_failure_marks_run_failed_and_finishes_worker() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_broken_opencode_schema()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let err = commands::hook_run::run(&app, SourceKind::Opencode, "manual-test", false)
            .await
            .expect_err("hook-run should fail");
        assert!(!err.to_string().trim().is_empty());

        let run = latest_run_record(&app.paths.db_path, "hook-run")?;
        assert_failed_run(&run);

        let (started_at, finished_at) = trigger_worker_times(&app.paths.db_path, "opencode")?;
        assert!(started_at.is_some());
        assert!(finished_at.is_some());
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
    let probes = llmusage::integrations::probe_all(app)?;
    let status =
        llmusage::commands::source_status::build_source_capability_statuses(&probes, &sources)
            .into_iter()
            .find(|status| status.source == source)
            .expect("source capability status present");
    Ok(status.status.to_string())
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

fn trigger_worker_times(db_path: &Path, source: &str) -> Result<(Option<String>, Option<String>)> {
    let conn = Connection::open(db_path)?;
    let times = conn.query_row(
        r#"
        SELECT last_worker_started_at, last_worker_finished_at
        FROM trigger_state
        WHERE source = ?1
        "#,
        [source],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(times)
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
        let payload = format!("\n{}", codex_token_line(timestamp, total_tokens, 153));
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
