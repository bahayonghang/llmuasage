use super::super::*;

fn seed_sanitized_native_antigravity(fixture: &Fixture, name: &str) -> Result<PathBuf> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../fixtures/antigravity-native-usage.json"))?;
    let sample = corpus["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sample| sample["family"] == "antigravity-cli" && sample["category"] == "normal_gen")
        .unwrap();
    let rows = sample["gen_metadata"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            Ok((
                row["idx"].as_i64().unwrap(),
                STANDARD.decode(row["data_base64"].as_str().unwrap())?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let path = fixture.seed_antigravity(name, &rows)?;
    let conn = Connection::open(&path)?;
    conn.execute_batch(
        "CREATE TABLE steps(idx INTEGER PRIMARY KEY, metadata BLOB);
        CREATE TABLE trajectory_meta(source INTEGER);
        INSERT INTO trajectory_meta VALUES (17);",
    )?;
    for row in sample["steps"].as_array().unwrap() {
        conn.execute(
            "INSERT INTO steps VALUES (?1, ?2)",
            rusqlite::params![
                row["idx"].as_i64().unwrap(),
                STANDARD.decode(row["metadata_base64"].as_str().unwrap())?
            ],
        )?;
    }
    Ok(path)
}

fn antigravity_persisted_snapshot(store: &Store) -> Result<Vec<Vec<Vec<rusqlite::types::Value>>>> {
    let conn = store.open_connection()?;
    [
        "SELECT * FROM usage_event WHERE source='antigravity' ORDER BY event_key",
        "SELECT * FROM usage_bucket_30m WHERE source='antigravity' ORDER BY hour_start,model",
        "SELECT * FROM source_cursor WHERE source='antigravity' ORDER BY cursor_key",
        "SELECT * FROM source_file WHERE source='antigravity' ORDER BY file_path",
        "SELECT * FROM meta WHERE key='token_accounting_version.antigravity'",
    ]
    .into_iter()
    .map(|sql| {
        let mut statement = conn.prepare(sql)?;
        let columns = statement.column_count();
        Ok(statement
            .query_map([], |row| {
                (0..columns)
                    .map(|column| row.get::<_, rusqlite::types::Value>(column))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    })
    .collect()
}

#[test]
fn antigravity_record_faults_and_source_failures_preserve_history() -> Result<()> {
    for (case, reason) in [
        ("generation_wire", Some("invalid_generation_metadata")),
        ("usage_type", Some("invalid_generation_metadata")),
        ("step_wire", Some("invalid_step_metadata")),
        ("trajectory_wire", Some("invalid_trajectory_metadata")),
        ("timestamp_nanos", Some("invalid_generation_metadata")),
        ("missing_timestamp", Some("missing_usage_timestamp")),
        ("output_checksum", Some("output_channel_mismatch")),
        ("changed_product_wire", None),
        ("prepare_failure", None),
    ] {
        let fixture = Fixture::new()?;
        let blob = ag_gen_metadata_blob(
            100,
            20,
            3,
            0,
            "preserved-request",
            None,
            None,
            1_800_000_000,
        );
        let path = fixture.seed_antigravity(case, &[(1, blob.clone())])?;
        Connection::open(&path)?.execute_batch(
            "CREATE TABLE trajectory_meta(source INTEGER); INSERT INTO trajectory_meta VALUES (17)",
        )?;
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            let app = AppContext::discover()?;
            let store = Store::new(&app.paths)?;
            store.bootstrap()?;
            let mut options = commands::sync::SyncRunOptions {
                source: Some(SourceKind::Antigravity),
                ..Default::default()
            };
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
            assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1, "{case}");
            store.set_meta_value("token_accounting_version.antigravity", "2")?;
            let before = antigravity_persisted_snapshot(&store)?;
            let connection = Connection::open(&path)?;
            let mut usage = ag_usage_message(100, 20, 3, 0, "preserved-request");
            match case {
                "step_wire" => connection.execute_batch(
                    "CREATE TABLE steps(idx INTEGER PRIMARY KEY, metadata BLOB);
                     INSERT INTO steps VALUES (1, X'0A80')",
                )?,
                "trajectory_wire" => connection.execute_batch(
                    "UPDATE trajectory_metadata_blob SET data=X'0A80'",
                )?,
                "prepare_failure" => connection.execute_batch(
                    "DROP TABLE gen_metadata; CREATE TABLE gen_metadata(idx INTEGER)",
                )?,
                _ => {
                    if case == "changed_product_wire" {
                        connection.execute_batch("UPDATE trajectory_meta SET source=1")?;
                    }
                    let replacement = if matches!(case, "generation_wire" | "changed_product_wire") {
                        vec![0x0a, 0x80]
                    } else {
                        if case == "usage_type" {
                            usage.extend(ag_bytes_field(2, b"invalid-number"));
                        } else if case == "output_checksum" {
                            usage.extend(ag_varint_field(3, 999));
                        }
                        let mut chat = ag_bytes_field(4, &usage);
                        if case == "missing_timestamp" {
                            connection.execute_batch("DELETE FROM trajectory_metadata_blob")?;
                        } else {
                            let nanos = if case == "timestamp_nanos" { 1_000_000_000 } else { 0 };
                            chat.extend(ag_bytes_field(9, &ag_timestamp_message(1_800_000_000, nanos)));
                        }
                        ag_bytes_field(1, &chat)
                    };
                    connection.execute("UPDATE gen_metadata SET data=?1", [replacement])?;
                }
            }
            drop(connection);
            options.rebuild = true;
            let (mut sender, mut receiver) = tokio::sync::mpsc::channel(256);
            let blocked = commands::sync::run_once_with_options(
                &app, &store, 0, &options, Some(&mut sender),
            ).await?;
            drop(sender);
            while let Some(event) = receiver.recv().await {
                assert!(!matches!(event, SyncEvent::TokenAccountingRepairFinished { .. }), "{case}");
            }
            assert_eq!(blocked.total_inserted, 0, "{case}");
            let issues = &blocked.sources[0].parse_issues;
            assert_eq!(issues.malformed_lines, u64::from(reason.is_some()), "{case}: {issues:?}");
            assert_eq!(issues.oversized_lines, 0, "{case}");
            assert_eq!(issues.informational_total(), 0, "{case}");
            if let Some(reason) = reason {
                assert_eq!(issues.samples[0].reason, reason, "{case}");
                assert_eq!(issues.samples[0].source, SourceKind::Antigravity, "{case}");
                assert_eq!(issues.samples[0].path_hash, hash_string(&path.canonicalize()?.to_string_lossy()), "{case}");
            } else {
                assert!(issues.samples.is_empty(), "{case}");
            }
            let code = if reason.is_some() || case == "changed_product_wire" { "incomplete_snapshot" } else { "metadata_unreadable" };
            assert!(blocked.sources[0].last_error.as_ref().unwrap().contains(code), "{case}");
            assert_eq!(antigravity_persisted_snapshot(&store)?, before, "{case}");
            assert_eq!(store.token_accounting_version(SourceKind::Antigravity)?, Some(2), "{case}");
            let reopened = Store::new(&app.paths)?;
            let raw: String = reopened.open_connection()?.query_row(
                "SELECT parse_issues_json FROM source_sync_status WHERE host_id='local' AND source='antigravity'",
                [], |row| row.get(0),
            )?;
            let persisted: serde_json::Value = serde_json::from_str(&raw)?;
            assert_eq!(persisted["malformed_lines"], u64::from(reason.is_some()), "{case}");
            assert_eq!(persisted["source_issues"][0]["code"], code, "{case}");
            assert!(!raw.contains("invalid-number"), "{case}");

            fixture.seed_antigravity(case, &[(1, blob)])?;
            Connection::open(&path)?.execute_batch(
                "CREATE TABLE trajectory_meta(source INTEGER); INSERT INTO trajectory_meta VALUES (17)",
            )?;
            let recovered = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
            assert!(recovered.sources[0].last_error.is_none(), "{case}");
            assert_eq!(store.token_accounting_version(SourceKind::Antigravity)?, Some(3), "{case}");
            assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1, "{case}");
            let raw: String = store.open_connection()?.query_row(
                "SELECT parse_issues_json FROM source_sync_status WHERE host_id='local' AND source='antigravity'",
                [], |row| row.get(0),
            )?;
            let cleared: serde_json::Value = serde_json::from_str(&raw)?;
            assert!(cleared.get("source_issues").is_none(), "{case}");
            assert_eq!(cleared["malformed_lines"], 0, "{case}");
            Ok::<_, anyhow::Error>(())
        })?;
        fixture.restore_env();
    }
    Ok(())
}

#[test]
fn antigravity_busy_native_database_preserves_history_cursor_and_marker() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = seed_sanitized_native_antigravity(&fixture, "native-busy")?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let mut options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Antigravity), ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
        store.set_meta_value("token_accounting_version.antigravity", "2")?;
        let before = antigravity_persisted_snapshot(&store)?;

        let mut blocker = Connection::open(&path)?;
        let exclusive = blocker.transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)?;
        let probe = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        probe.busy_timeout(Duration::ZERO)?;
        let error = probe.query_row("SELECT COUNT(*) FROM gen_metadata", [], |row| row.get::<_, i64>(0))
            .expect_err("exclusive native DB lock must block a reader");
        assert!(matches!(error, rusqlite::Error::SqliteFailure(ref sqlite, _) if sqlite.code == rusqlite::ErrorCode::DatabaseBusy));
        drop(probe);

        options.rebuild = true;
        let blocked = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(blocked.total_inserted, 0);
        assert!(blocked.sources[0].last_error.is_some());
        assert_eq!(blocked.sources[0].parse_issues.malformed_lines, 0);
        assert!(blocked.sources[0].last_error.as_ref().unwrap().contains("metadata_unreadable"));
        assert_eq!(antigravity_persisted_snapshot(&store)?, before);
        assert_eq!(store.token_accounting_version(SourceKind::Antigravity)?, Some(2));

        drop(exclusive);
        drop(blocker);
        let recovered = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert!(recovered.sources[0].last_error.is_none());
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
        assert_eq!(store.token_accounting_version(SourceKind::Antigravity)?, Some(3));
        Ok::<_, anyhow::Error>(())
    })?;
    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_cancel_during_native_staging_preserves_history_and_inventory() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = seed_sanitized_native_antigravity(&fixture, "native-cancel")?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let mut options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Antigravity),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
        store.set_meta_value("token_accounting_version.antigravity", "2")?;
        // If the driver sweeps this cancelled inventory, the sentinel becomes
        // missing; comparing full rows detects that independently of token data.
        store.open_connection()?.execute(
            "UPDATE source_file SET last_seen_at='2000-01-01T00:00:00Z' WHERE source='antigravity'",
            [],
        )?;
        let before = antigravity_persisted_snapshot(&store)?;

        // Hold an actual native read boundary open while the SourceStarted
        // event reaches the cancellation watcher; no timer or sleep is needed.
        let mut blocker = Connection::open(&path)?;
        let exclusive =
            blocker.transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)?;
        let cancel = tokio_util::sync::CancellationToken::new();
        let watcher_cancel = cancel.clone();
        let (mut sender, mut receiver) = tokio::sync::mpsc::channel(256);
        let watcher = tokio::spawn(async move {
            let mut events = Vec::new();
            while let Some(event) = receiver.recv().await {
                if matches!(
                    event,
                    SyncEvent::SourceStarted {
                        source: SourceKind::Antigravity,
                        ..
                    }
                ) {
                    watcher_cancel.cancel();
                }
                events.push(event);
            }
            events
        });
        options.rebuild = true;
        let cancelled = commands::sync::run_once_with_cancel(
            &app,
            &store,
            0,
            &options,
            Some(&mut sender),
            &cancel,
        )
        .await?;
        drop(sender);
        let events = watcher.await?;
        drop(exclusive);
        drop(blocker);

        assert!(cancel.is_cancelled());
        assert!(events.iter().any(|event| matches!(
            event,
            SyncEvent::SourceStarted {
                source: SourceKind::Antigravity,
                ..
            }
        )));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, SyncEvent::TokenAccountingRepairFinished { .. }))
        );
        assert_eq!(cancelled.total_inserted, 0);
        assert_eq!(
            cancelled.sources[0].parse_issues.malformed_lines,
            0,
            "cancellation must take the staging exit before the held read is reported as a busy failure"
        );
        assert_eq!(antigravity_persisted_snapshot(&store)?, before);
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(2)
        );
        Ok::<_, anyhow::Error>(())
    })?;
    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_full_sync_reconciles_stronger_identity_in_an_unselected_copy() -> Result<()> {
    let fixture = Fixture::new()?;
    let timestamp = chrono::Utc::now().timestamp().unsigned_abs();
    let blob = |response_id: &str| {
        let mut usage = ag_usage_message(100, 20, 3, 0, response_id);
        usage.extend(ag_string_field(7, "shared-message"));
        let mut chat = ag_bytes_field(4, &usage);
        chat.extend(ag_bytes_field(9, &ag_timestamp_message(timestamp, 0)));
        chat.extend(ag_string_field(19, "gemini-3.8-flash"));
        ag_bytes_field(1, &chat)
    };
    let original = fixture.seed_antigravity("identity-original", &[(1, blob(""))])?;
    let conn = Connection::open(&original)?;
    conn.execute_batch(
        "CREATE TABLE trajectory_meta(source INTEGER); INSERT INTO trajectory_meta VALUES (17)",
    )?;
    drop(conn);
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let mut options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Antigravity),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        let before: String = store.open_connection()?.query_row(
            "SELECT event_key FROM usage_event WHERE source='antigravity'",
            [],
            |row| row.get(0),
        )?;
        let root = fixture.home.join(".gemini/antigravity-ide/conversations");
        fs::create_dir_all(&root)?;
        let copy = Connection::open(root.join("stronger-copy.db"))?;
        copy.execute_batch("CREATE TABLE gen_metadata(idx INTEGER PRIMARY KEY, data BLOB)")?;
        copy.execute(
            "INSERT INTO gen_metadata VALUES (1, ?1)",
            [blob("response-enriched")],
        )?;
        drop(copy);
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        let after: String = store.open_connection()?.query_row(
            "SELECT event_key FROM usage_event WHERE source='antigravity'",
            [],
            |row| row.get(0),
        )?;
        assert_ne!(
            before, after,
            "full sync must reconcile stronger identity even from an unselected root"
        );
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
        let unchanged =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(
            unchanged.total_inserted, 0,
            "unselected sibling files must not force replay"
        );
        options.recent_days = Some(1);
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;
    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_missing_group_member_preserves_bounded_and_forgotten_history() -> Result<()> {
    for bounded_first in [false, true] {
        let fixture = Fixture::new()?;
        let timestamp = chrono::Utc::now().timestamp().unsigned_abs();
        let missing = fixture.seed_antigravity(
            "member-a",
            &[(
                1,
                ag_gen_metadata_blob(
                    100,
                    20,
                    3,
                    0,
                    "member-a",
                    Some("gemini-3.8-flash"),
                    None,
                    timestamp,
                ),
            )],
        )?;
        fixture.seed_antigravity(
            "member-b",
            &[(
                1,
                ag_gen_metadata_blob(
                    200,
                    40,
                    6,
                    0,
                    "member-b",
                    Some("gemini-3.8-flash"),
                    None,
                    timestamp,
                ),
            )],
        )?;
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            let app = AppContext::discover()?;
            let store = Store::new(&app.paths)?;
            store.bootstrap()?;
            let mut options = commands::sync::SyncRunOptions {
                source: Some(SourceKind::Antigravity),
                recent_days: bounded_first.then_some(1),
                ..Default::default()
            };
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
            assert_eq!(antigravity_event_count(&app.paths.db_path)?, 2);
            let tracked = store
                .source_files()
                .tracked_paths(SourceKind::Antigravity, "local")?;
            assert_eq!(tracked.len(), 2);
            if bounded_first {
                assert!(
                    store
                        .cursors()
                        .load_file_cursors(SourceKind::Antigravity, "local")?
                        .is_empty()
                );
            } else {
                let tracked_missing = tracked
                    .iter()
                    .find(|path| path.ends_with("member-a.db"))
                    .unwrap();
                store.mark_source_file_deleted(
                    SourceKind::Antigravity,
                    "local",
                    tracked_missing,
                )?;
            }
            fs::remove_file(&missing)?;
            fixture.seed_antigravity(
                "member-b",
                &[(
                    1,
                    ag_gen_metadata_blob(
                        300,
                        40,
                        6,
                        0,
                        "member-b",
                        Some("gemini-3.8-flash"),
                        None,
                        timestamp,
                    ),
                )],
            )?;
            let before = antigravity_persisted_snapshot(&store)?;
            let observed_before = chrono::Utc::now();
            for recent_days in [None, Some(1)] {
                options.recent_days = recent_days;
                let (mut sender, mut receiver) = tokio::sync::mpsc::channel(256);
                let blocked = commands::sync::run_once_with_options(&app, &store, 0, &options, Some(&mut sender)).await?;
                drop(sender);
                let mut started = 0;
                let mut finished = 0;
                while let Some(event) = receiver.recv().await {
                    match event {
                        SyncEvent::SourceStarted { source: SourceKind::Antigravity, .. } => started += 1,
                        SyncEvent::SourceFinished { source: SourceKind::Antigravity, .. } => finished += 1,
                        SyncEvent::TokenAccountingRepairFinished { .. } => panic!("blocked source cannot certify repair"),
                        _ => {},
                    }
                }
                assert_eq!((started, finished), (1, 1));
                let stat = &blocked.sources[0];
                assert_eq!((stat.files_processed, stat.changed_files, stat.skipped_files), (0, 0, 0));
                assert_eq!((stat.events_seen, stat.events_replayed, stat.events_inserted), (0, 0, 0));
                assert_eq!((stat.bytes_scanned, stat.write_ms), (0, 0));
                assert_eq!(stat.stored_events, 2);
                assert_eq!(antigravity_persisted_snapshot(&store)?, before);
            }
            options.recent_days = None;
            let result =
                commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
            assert!(
                result.sources[0]
                    .last_error
                    .as_ref()
                    .unwrap()
                    .contains("missing")
            );
            assert_eq!(antigravity_event_count(&app.paths.db_path)?, 2);
            let input: i64 = store.open_connection()?.query_row(
                "SELECT SUM(input_tokens) FROM usage_event WHERE source='antigravity'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(input, 300, "snapshot must retain both original members");
            assert_eq!(result.sources[0].parse_issues.malformed_lines, 0);
            let reopened = Store::new(&app.paths)?;
            let raw: String = reopened.open_connection()?.query_row(
                "SELECT parse_issues_json FROM source_sync_status WHERE host_id='local' AND source='antigravity'",
                [], |row| row.get(0),
            )?;
            let diagnostic: serde_json::Value = serde_json::from_str(&raw)?;
            assert_eq!(diagnostic["source_issues"][0]["code"], "tracked_member_missing");
            assert_eq!(diagnostic["source_issues"][0]["count"], 1);
            assert_eq!(diagnostic["source_issues"][0]["scope"], "product_group");
            assert!(diagnostic["source_issues"][0]["observed_at"].is_string());
            let observed = chrono::DateTime::parse_from_rfc3339(diagnostic["source_issues"][0]["observed_at"].as_str().unwrap())?;
            assert!(observed >= observed_before && observed <= chrono::Utc::now());
            let dashboard = llmusage::query::Dashboard::open(&reopened)?;
            let center = dashboard.sync_command_center(&Default::default())?;
            assert_eq!(center.sources.iter().find(|row| row.source == "antigravity").unwrap().status, "error");
            for args in [vec!["source-status"], vec!["doctor", "--json"], vec!["diagnostics"]] {
                let output = crate::test_process::llmusage_command()
                    .arg("--home").arg(&app.paths.root_dir).args(&args)
                    .env("HOME", &fixture.home).env("USERPROFILE", &fixture.home)
                    .env("RUST_LOG", "off").env("LLMUSAGE_LOG", "off").output()?;
                assert!(output.status.success(), "{output:?}");
                let text = String::from_utf8(output.stdout)?;
                assert!(text.contains("tracked_member_missing"), "{args:?}: {text}");
                if args[0] == "doctor" {
                    let checks: serde_json::Value = serde_json::from_str(&text)?;
                    assert!(checks.as_array().unwrap().iter().any(|check| check["id"] == "source.issues" && check["status"] == "warn"));
                }
                if args[0] == "source-status" {
                    assert!(!text.contains(&fixture.home.to_string_lossy().to_string()));
                }
            }
            options.rebuild = true;
            assert!(commands::sync::run_once_with_options(&app, &store, 0, &options, None).await.is_err());
            assert_eq!(antigravity_persisted_snapshot(&store)?, before);
            fixture.seed_antigravity("member-a", &[(1, ag_gen_metadata_blob(100, 20, 3, 0, "member-a", Some("gemini-3.8-flash"), None, timestamp))])?;
            options.rebuild = false;
            let recovered = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
            assert!(recovered.sources[0].last_error.is_none());
            assert_eq!(antigravity_event_count(&app.paths.db_path)?, 2);
            let recovered_input: i64 = store.open_connection()?.query_row("SELECT SUM(input_tokens) FROM usage_event WHERE source='antigravity'", [], |row| row.get(0))?;
            assert_eq!(recovered_input, 400);
            let raw: String = store.open_connection()?.query_row("SELECT parse_issues_json FROM source_sync_status WHERE host_id='local' AND source='antigravity'", [], |row| row.get(0))?;
            assert!(serde_json::from_str::<serde_json::Value>(&raw)?.get("source_issues").is_none());
            fs::remove_file(&missing)?;
            options.rebuild = true;
            options.allow_lossy_rebuild = true;
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
            assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
            Ok::<_, anyhow::Error>(())
        })?;
        fixture.restore_env();
    }
    Ok(())
}

#[test]
fn antigravity_preflight_existing_legacy_paths_and_changed_root_preserve_history() -> Result<()> {
    for changed_root in [false, true] {
        let fixture = Fixture::new()?;
        let path = seed_sanitized_native_antigravity(&fixture, "coverage")?;
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            let app = AppContext::discover()?;
            let store = Store::new(&app.paths)?;
            store.bootstrap()?;
            let mut options = commands::sync::SyncRunOptions { source: Some(SourceKind::Antigravity), ..Default::default() };
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
            assert!(antigravity_event_count(&app.paths.db_path)? > 0);
            let old_root = path.parent().unwrap();
            let mut legacy_paths = Vec::new();
            if changed_root {
                unsafe { std::env::set_var("GEMINI_CLI_HOME", fixture.home.join("changed-gemini-root")); }
            } else {
                for name in ["old-a.json", "old-b.json"] {
                    let legacy = old_root.join(name);
                    fs::write(&legacy, b"{}")?;
                    legacy_paths.push(legacy.canonicalize()?.to_string_lossy().into_owned());
                }
                store.source_files().mark_inventory_seen(SourceKind::Antigravity, "local", &legacy_paths, "2025-01-01T00:00:00Z")?;
            }
            let before = antigravity_persisted_snapshot(&store)?;
            let observed_before = chrono::Utc::now();
            for rebuild in [false, true] {
                options.rebuild = rebuild;
                let blocked = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
                assert_eq!(antigravity_persisted_snapshot(&store)?, before);
                assert_eq!(blocked.total_inserted, 0);
                assert_eq!(blocked.sources[0].changed_files, 0);
                let error = blocked.sources[0].last_error.as_ref().unwrap();
                assert!(error.contains("tracked_member_out_of_scope"));
                assert!(!error.contains("tracked_member_missing"));
                let reopened = Store::new(&app.paths)?;
                let raw: String = reopened.open_connection()?.query_row("SELECT parse_issues_json FROM source_sync_status WHERE host_id='local' AND source='antigravity'", [], |row| row.get(0))?;
                let diagnostic: serde_json::Value = serde_json::from_str(&raw)?;
                let issue = &diagnostic["source_issues"][0];
                assert_eq!(issue["code"], "tracked_member_out_of_scope");
                assert_eq!(issue["count"], if changed_root { 1 } else { 2 });
                assert_eq!(issue["scope"], "product_group");
                let observed = chrono::DateTime::parse_from_rfc3339(issue["observed_at"].as_str().unwrap())?;
                assert!(observed >= observed_before && observed <= chrono::Utc::now());
                assert!(!raw.contains(&fixture.home.to_string_lossy().to_string()));
            }
            if changed_root {
                unsafe { std::env::remove_var("GEMINI_CLI_HOME"); }
            } else {
                options.allow_lossy_rebuild = true;
            }
            let recovered = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
            assert!(recovered.sources[0].last_error.is_none());
            assert!(antigravity_event_count(&app.paths.db_path)? > 0);
            assert_eq!(store.token_accounting_version(SourceKind::Antigravity)?, Some(3));
            Ok::<_, anyhow::Error>(())
        })?;
        fixture.restore_env();
    }
    Ok(())
}

#[test]
fn antigravity_native_product_survives_cross_root_copy_and_unreadable_file() -> Result<()> {
    let fixture = Fixture::new()?;
    let original = fixture.seed_antigravity(
        "copied-native",
        &[(
            1,
            ag_gen_metadata_blob(
                100,
                20,
                3,
                0,
                "native-copy",
                Some("gemini-3.8-flash"),
                None,
                1_800_000_000,
            ),
        )],
    )?;
    let connection = Connection::open(&original)?;
    connection.execute_batch(
        "CREATE TABLE trajectory_meta(source INTEGER); INSERT INTO trajectory_meta VALUES (17);",
    )?;
    drop(connection);
    let ide_root = fixture.home.join(".gemini/antigravity-ide/conversations");
    fs::create_dir_all(&ide_root)?;
    let copied = ide_root.join("copied-cli.db");
    fs::copy(&original, &copied)?;
    fs::remove_file(original)?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Antigravity),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
        let before = store
            .cursors()
            .load_file_cursors(SourceKind::Antigravity, "local")?;
        assert_eq!(before.len(), 1);
        fs::write(&copied, b"damaged copied database")?;
        let result = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert!(result.sources[0].last_error.is_some());
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
        let after = store
            .cursors()
            .load_file_cursors(SourceKind::Antigravity, "local")?;
        assert_eq!(
            before.values().next().unwrap().file_fingerprint,
            after.values().next().unwrap().file_fingerprint
        );
        Ok::<_, anyhow::Error>(())
    })?;
    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_wal_only_commit_replays_snapshot() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.seed_antigravity(
        "wal-snapshot",
        &[(
            1,
            ag_gen_metadata_blob(
                100,
                20,
                3,
                0,
                "wal-first",
                Some("gemini-3.8-flash"),
                None,
                1_800_000_000,
            ),
        )],
    )?;
    let connection = Connection::open(&path)?;
    connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Antigravity),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        let main_before = fs::read(&path)?;
        let blob = ag_gen_metadata_blob(
            200,
            40,
            6,
            0,
            "wal-second",
            Some("gemini-3.8-flash"),
            None,
            1_800_000_010,
        );
        connection.execute(
            "INSERT INTO gen_metadata(idx, data, size) VALUES (77, ?1, ?2)",
            rusqlite::params![&blob, blob.len() as i64],
        )?;
        assert_eq!(fs::read(&path)?, main_before);
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 2);
        let again = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(again.total_inserted, 0);
        Ok::<_, anyhow::Error>(())
    })?;
    drop(connection);
    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_copied_request_conflict_preserves_previous_snapshot() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.seed_antigravity(
        "conflict-a",
        &[(
            1,
            ag_gen_metadata_blob(
                100,
                20,
                3,
                0,
                "conflict-id",
                Some("gemini-3.8-flash"),
                None,
                1_800_000_000,
            ),
        )],
    )?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions { source: Some(SourceKind::Antigravity), ..Default::default() };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        fixture.seed_antigravity("conflict-b", &[(1, ag_gen_metadata_blob(200, 20, 3, 0,
            "conflict-id", Some("gemini-3.8-flash"), None, 1_800_000_000))])?;
        let result = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert!(result.sources[0].last_error.as_ref().unwrap().contains("incomplete_snapshot"));
        let input: i64 = store.open_connection()?.query_row("SELECT SUM(input_tokens) FROM usage_event WHERE source='antigravity'", [], |row| row.get(0))?;
        assert_eq!(input, 100);
        let connection = Connection::open(&path)?;
        connection.execute_batch("DROP TABLE gen_metadata; DROP TABLE trajectory_metadata_blob; CREATE TABLE unrelated(value TEXT);")?;
        drop(connection);
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;
    fixture.restore_env();
    Ok(())
}

#[cfg(windows)]
#[test]
fn antigravity_rebuild_accepts_raw_windows_cursor_path_without_lossy_flag() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.seed_antigravity(
        "raw-path",
        &[(
            1,
            ag_gen_metadata_blob(
                100,
                20,
                3,
                0,
                "raw-path-id",
                Some("gemini-3.8-flash"),
                None,
                1_800_000_000,
            ),
        )],
    )?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let mut options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Antigravity),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        let raw = path
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_owned();
        store.open_connection()?.execute(
            "UPDATE source_cursor SET file_path=?1, cursor_key=?1 WHERE source='antigravity'",
            [&raw],
        )?;
        store.set_meta_value("token_accounting_version.antigravity", "2")?;
        options.rebuild = true;
        let result = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert!(result.sources[0].last_error.is_none());
        assert_eq!(antigravity_event_count(&app.paths.db_path)?, 1);
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(3)
        );
        Ok::<_, anyhow::Error>(())
    })?;
    fixture.restore_env();
    Ok(())
}

#[test]
fn rebuild_preserves_unattributed_antigravity_history() -> Result<()> {
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

        commands::sync::run_with_options(
            &app,
            commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::Antigravity),
                allow_lossy_rebuild: true,
                ..Default::default()
            },
        )
        .await?;

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
        // #1 is the model enum; #2 alone is fresh input.
        assert_eq!(row.0, 500);
        assert_eq!(row.1, 1200);
        assert_eq!(row.2, 234);
        assert_eq!(row.3, 50);
        assert_eq!(row.4, 500 + 1200 + 234 + 50);
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
        // A blocked group preserves its inventory and cursors for a retry.
        assert_eq!(counts.live + counts.missing, 1);
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
        assert_eq!(antigravity.parse_issues.malformed_lines, 0);
        assert!(
            antigravity
                .last_error
                .as_ref()
                .unwrap()
                .contains("metadata_unreadable")
        );
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
            antigravity.last_error.is_some(),
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
        // The historical v21 marker remains version 2 until explicit repair.
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
            store.has_legacy_token_accounting(SourceKind::Antigravity)?,
            "old accounting must require explicit repair"
        );

        // Explicit repair imports the current parser while retaining hook history.
        let summary = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Antigravity),
                rebuild: true,
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
