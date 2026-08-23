use super::*;

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

    let output = test_process::llmusage_command()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("status")
        .env("HOME", &fixture.home)
        .env("USERPROFILE", &fixture.home)
        .env("CODEX_HOME", &fixture.codex_home)
        .env("OPENCODE_HOME", &fixture.opencode_home)
        .env("RUST_LOG", "off")
        .output()
        .context("spawn llmusage status subprocess")?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("- Worker lock: holder=cli:"));

    fixture.restore_env();
    Ok(())
}

#[test]
fn sync_summary_table_is_stdout_only_without_ansi_or_completion_sentence() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex("rollout-table.jsonl", 123, "2026-04-22T01:12:00Z")?;

    let run = |columns: &str| -> Result<std::process::Output> {
        test_process::llmusage_command()
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
            .output()
            .context("spawn llmusage sync summary subprocess")
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
