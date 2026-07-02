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
    parsers::SourceSyncStats,
    query::Dashboard,
    store::{HolderKind, Store},
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
        assert_eq!(first_sync_status.len(), 4);

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
        commands::sync::run(&app).await?;
        assert_eq!(usage_event_count(&app.paths.db_path)?, 1);

        fixture.seed_opencode("msg-2", 1776823200000, 48)?;
        commands::sync::run(&app).await?;
        assert_eq!(usage_event_count(&app.paths.db_path)?, 2);

        commands::sync::run(&app).await?;
        assert_eq!(usage_event_count(&app.paths.db_path)?, 2);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn opencode_part_tool_calls_land_in_usage_tool_call() -> Result<()> {
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
        commands::sync::run(&app).await?;
        assert_eq!(usage_tool_call_count(&app.paths.db_path)?, 2);
        assert_eq!(
            opencode_mcp_servers(&app.paths.db_path)?,
            vec!["context7".to_string()]
        );

        commands::sync::run(&app).await?;
        assert_eq!(usage_tool_call_count(&app.paths.db_path)?, 2);
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
        assert_ne!(first_cursor.inode, 0);
        assert_eq!(usage_event_count(&app.paths.db_path)?, 1);

        fixture.replace_opencode_db("msg-replaced", 1776823100000, 48)?;
        commands::sync::run(&app).await?;

        let second_cursor = store.cursors().load_opencode_cursor()?;
        assert_ne!(second_cursor.inode, first_cursor.inode);
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
        ] {
            saved.push((key.to_string(), std::env::var(key).ok()));
        }
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("USERPROFILE", &home);
            std::env::set_var("CODEX_HOME", &codex_home);
            std::env::set_var("CCR_ROOT", &ccr_root);
            std::env::set_var("OPENCODE_HOME", &opencode_home);
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

    fn seed_claude(&self, name: &str, total_tokens: i64, timestamp: &str) -> Result<()> {
        let claude_file = self
            .home
            .join(".claude")
            .join("projects")
            .join("demo")
            .join(name);
        fs::write(
            claude_file,
            format!("{}\n", claude_usage_line(timestamp, total_tokens)),
        )?;
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
