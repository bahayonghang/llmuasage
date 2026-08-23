#[test]
fn diagnostics_counts_protected_events_from_aggregate_projection() -> Result<()> {
    let fixture = Fixture::new()?;
    for (event_key, tokens) in [("codex:diagnostics:1", 10), ("codex:diagnostics:2", 20)] {
        fixture.seed_event(SeedEvent {
            event_key,
            input_tokens: tokens,
            total_tokens: tokens,
            ..Default::default()
        })?;
    }
    let conn = fixture.store().open_connection()?;
    conn.execute(
        "INSERT INTO source_file(source, file_path, state, last_state_change_at) VALUES ('codex', ?1, 'live', '2026-07-11T00:00:00Z')",
        [fixture.paths().root_dir.join("missing-session.jsonl").display().to_string()],
    )?;
    // The aggregate projection remains populated while the fact rows are
    // removed, making this a direct regression for the diagnostics route.
    conn.execute("DELETE FROM usage_event", [])?;
    drop(conn);

    let diagnostics = Dashboard::open(fixture.store())?.diagnostics()?;
    let codex = diagnostics
        .by_source
        .iter()
        .find(|row| row.source == "codex")
        .expect("codex diagnostics");
    assert_eq!(codex.missing_file_count, 1);
    assert_eq!(codex.protected_event_count, 2);
    assert!(codex.lossy_rebuild_risk);
    Ok(())
}
#[test]
fn sync_command_center_projects_parse_issue_counters_without_samples() -> Result<()> {
    let fixture = Fixture::new()?;
    let conn = fixture.store().open_connection()?;
    conn.execute(
        r#"
        INSERT INTO source_sync_status(
            source, files_processed, changed_files, bytes_scanned,
            events_seen, events_replayed, events_inserted, stored_events,
            parse_ms, write_ms, lock_wait_ms, updated_at, parse_issues_json
        ) VALUES
            ('codex', 1, 1, 10, 2, 0, 2, 2, 1, 1, 0, '2026-08-17T00:00:00Z', ?1),
            ('zcode', 1, 1, 10, 1, 0, 1, 1, 1, 1, 0, '2026-08-17T00:00:00Z', ?2)
        "#,
        rusqlite::params![
            r#"{"malformed_lines":2,"oversized_lines":1,"skipped_lines":0,"accounting_anomaly_lines":0,"samples":[{"source":"codex","path_hash":"abc","offset":9,"kind":"malformed"}]}"#,
            r#"{"malformed_lines":0,"oversized_lines":0,"skipped_lines":5,"accounting_anomaly_lines":1,"samples":[]}"#,
        ],
    )?;
    drop(conn);

    let center = Dashboard::open(fixture.store())?.sync_command_center(&Default::default())?;
    let encoded = serde_json::to_value(&center)?;
    let sources = encoded["sources"].as_array().expect("sources");
    let codex = sources
        .iter()
        .find(|row| row["source"] == "codex")
        .expect("codex source");
    let zcode = sources
        .iter()
        .find(|row| row["source"] == "zcode")
        .expect("zcode source");

    assert_eq!(codex["malformed_lines"], 2);
    assert_eq!(codex["oversized_lines"], 1);
    assert_eq!(codex["skipped_lines"], 0);
    assert_eq!(codex["accounting_anomaly_lines"], 0);
    assert_eq!(codex["tone"], "warn");
    assert!(codex.get("samples").is_none());
    assert!(!serde_json::to_string(codex)?.contains("path_hash"));

    assert_eq!(zcode["malformed_lines"], 0);
    assert_eq!(zcode["skipped_lines"], 5);
    assert_eq!(zcode["accounting_anomaly_lines"], 1);
    assert_eq!(zcode["tone"], "good");
    assert!(zcode.get("samples").is_none());
    Ok(())
}

#[test]
fn sync_command_center_ignores_recovered_abort_after_successful_sync() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "claude:center:1",
        source: "claude",
        model: "claude-sonnet-4",
        event_at: "2026-08-19T00:00:00Z",
        input_tokens: 10,
        total_tokens: 10,
        ..Default::default()
    })?;
    let conn = fixture.store().open_connection()?;
    conn.execute(
        r#"
        INSERT INTO source_sync_status(
            source, files_processed, changed_files, bytes_scanned,
            events_seen, events_replayed, events_inserted, stored_events,
            parse_ms, write_ms, lock_wait_ms, updated_at
        ) VALUES ('claude', 3, 1, 10, 5, 0, 2, 2, 1, 1, 0, '2026-08-19T08:58:21Z')
        "#,
        [],
    )?;
    conn.execute(
        "INSERT INTO source_file(source, file_path, state, last_state_change_at) VALUES ('claude', ?1, 'missing', '2026-08-19T08:58:21Z')",
        [fixture
            .paths()
            .root_dir
            .join("gone-claude.jsonl")
            .display()
            .to_string()],
    )?;
    conn.execute(
        r#"
        INSERT INTO run_log(command, status, error, started_at, finished_at)
        VALUES
            ('sync', 'aborted', 'recovered stale running record', '2026-08-18T12:40:45Z', '2026-08-19T05:12:30Z'),
            ('sync', 'success', NULL, '2026-08-19T08:58:20Z', '2026-08-19T08:58:21Z'),
            ('serve', 'aborted', 'recovered stale running record', '2026-08-19T09:00:00Z', '2026-08-19T09:01:00Z'),
            ('serve', 'aborted', 'recovered stale running record', '2026-08-19T09:02:00Z', '2026-08-19T09:03:00Z'),
            ('serve', 'aborted', 'recovered stale running record', '2026-08-19T09:04:00Z', '2026-08-19T09:05:00Z'),
            ('serve', 'aborted', 'recovered stale running record', '2026-08-19T09:06:00Z', '2026-08-19T09:07:00Z'),
            ('serve', 'aborted', 'recovered stale running record', '2026-08-19T09:08:00Z', '2026-08-19T09:09:00Z'),
            ('serve', 'aborted', 'recovered stale running record', '2026-08-19T09:10:00Z', '2026-08-19T09:11:00Z'),
            ('serve', 'aborted', 'recovered stale running record', '2026-08-19T09:12:00Z', '2026-08-19T09:13:00Z'),
            ('serve', 'aborted', 'recovered stale running record', '2026-08-19T09:14:00Z', '2026-08-19T09:15:00Z'),
            ('serve', 'running', NULL, '2026-08-19T11:04:41Z', NULL)
        "#,
        [],
    )?;
    drop(conn);

    let center = Dashboard::open(fixture.store())?.sync_command_center(&Default::default())?;
    assert_eq!(center.tone, "good");
    assert_eq!(center.headline_key, "syncCenter.headline.ready");
    assert_eq!(center.reason_key, "syncCenter.reason.ready");
    let last_run = center.last_run.as_ref().expect("last run");
    assert_eq!(last_run.status, "success");
    assert!(last_run.error_key.is_none());
    assert_eq!(center.safety.recent_failures, 0);
    let claude = center
        .sources
        .iter()
        .find(|row| row.source == "claude")
        .expect("claude source");
    assert!(claude.lossy_rebuild_risk);
    assert_eq!(claude.status, "ok");
    assert_eq!(claude.tone, "good");
    assert_eq!(center.safety.risk_sources, vec!["claude".to_string()]);
    assert_eq!(center.safety.risk_details.len(), 1);
    assert_eq!(center.safety.risk_details[0].source, "claude");
    assert_eq!(center.safety.risk_details[0].missing_file_count, 1);
    assert_eq!(center.safety.risk_details[0].protected_event_count, 1);
    Ok(())
}

#[test]
fn sync_command_center_keeps_failed_last_run_past_serve_noise() -> Result<()> {
    let fixture = Fixture::new()?;
    let conn = fixture.store().open_connection()?;
    conn.execute(
        r#"
        INSERT INTO source_sync_status(
            source, files_processed, changed_files, bytes_scanned,
            events_seen, events_replayed, events_inserted, stored_events,
            parse_ms, write_ms, lock_wait_ms, updated_at
        ) VALUES ('codex', 1, 1, 10, 2, 0, 2, 2, 1, 1, 0, '2026-08-19T00:00:00Z')
        "#,
        [],
    )?;
    conn.execute(
        r#"
        INSERT INTO run_log(command, status, error, started_at, finished_at)
        VALUES ('sync', 'failed', 'parser exploded', '2026-08-19T00:00:00Z', '2026-08-19T00:00:01Z')
        "#,
        [],
    )?;
    for idx in 0..12 {
        conn.execute(
            r#"
            INSERT INTO run_log(command, status, error, started_at, finished_at)
            VALUES ('serve', 'aborted', 'recovered stale running record', ?1, ?2)
            "#,
            rusqlite::params![
                format!("2026-08-19T01:{idx:02}:00Z"),
                format!("2026-08-19T01:{idx:02}:30Z"),
            ],
        )?;
    }
    drop(conn);

    let center = Dashboard::open(fixture.store())?.sync_command_center(&Default::default())?;
    assert_eq!(center.headline_key, "syncCenter.headline.failed");
    assert_eq!(center.reason_key, "syncCenter.reason.lastRunFailed");
    let last_run = center.last_run.as_ref().expect("last run");
    assert_eq!(last_run.status, "failed");
    assert_eq!(
        last_run.error_key.as_deref(),
        Some("syncCenter.reason.lastRunFailed")
    );
    assert_eq!(center.safety.recent_failures, 1);
    Ok(())
}

#[test]
fn sync_command_center_pairs_failed_headline_ahead_of_rebuild_risk() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:center-failed:1",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-08-19T00:00:00Z",
        input_tokens: 10,
        total_tokens: 10,
        ..Default::default()
    })?;
    let conn = fixture.store().open_connection()?;
    conn.execute(
        r#"
        INSERT INTO source_sync_status(
            source, files_processed, changed_files, bytes_scanned,
            events_seen, events_replayed, events_inserted, stored_events,
            parse_ms, write_ms, lock_wait_ms, updated_at
        ) VALUES ('codex', 1, 1, 10, 2, 0, 2, 2, 1, 1, 0, '2026-08-19T00:00:00Z')
        "#,
        [],
    )?;
    conn.execute(
        "INSERT INTO source_file(source, file_path, state, last_state_change_at) VALUES ('codex', ?1, 'missing', '2026-08-19T00:00:00Z')",
        [fixture
            .paths()
            .root_dir
            .join("gone-codex.jsonl")
            .display()
            .to_string()],
    )?;
    conn.execute(
        r#"
        INSERT INTO run_log(command, status, error, started_at, finished_at)
        VALUES ('sync', 'failed', 'parser exploded', '2026-08-19T00:00:00Z', '2026-08-19T00:00:01Z')
        "#,
        [],
    )?;
    drop(conn);

    let center = Dashboard::open(fixture.store())?.sync_command_center(&Default::default())?;
    assert_eq!(center.headline_key, "syncCenter.headline.failed");
    assert_eq!(center.reason_key, "syncCenter.reason.lastRunFailed");
    assert!(center.safety.lossy_rebuild_risk);
    assert_eq!(center.last_run.as_ref().unwrap().status, "failed");
    Ok(())
}

#[test]
fn dashboard_snapshot_uses_single_connection_and_matches_individual_methods() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(180)?;

    Store::reset_open_connection_counter();
    let dashboard = Dashboard::open(fixture.store())?;
    let snapshot = dashboard.snapshot(&Default::default())?;
    assert_eq!(Store::open_connection_count(), 1);

    let mut snapshot_overview = serde_json::to_value(&snapshot.overview)?;
    let mut method_overview = serde_json::to_value(dashboard.overview(&Default::default())?)?;
    snapshot_overview["generated_at"] = serde_json::Value::String("same".to_string());
    method_overview["generated_at"] = serde_json::Value::String("same".to_string());
    assert_eq!(snapshot_overview, method_overview);
    assert_eq!(
        serde_json::to_value(&snapshot.day_trends)?,
        serde_json::to_value(dashboard.trends("day", &Default::default())?)?
    );
    assert_eq!(
        serde_json::to_value(&snapshot.week_trends)?,
        serde_json::to_value(dashboard.trends("week", &Default::default())?)?
    );
    assert_eq!(
        serde_json::to_value(&snapshot.month_trends)?,
        serde_json::to_value(dashboard.trends("month", &Default::default())?)?
    );
    assert_eq!(
        serde_json::to_value(&snapshot.all_trends)?,
        serde_json::to_value(dashboard.trends("all", &Default::default())?)?
    );
    assert_eq!(
        serde_json::to_value(&snapshot.models)?,
        serde_json::to_value(dashboard.model_breakdown(&Default::default())?)?
    );
    assert_eq!(
        serde_json::to_value(&snapshot.sources)?,
        serde_json::to_value(dashboard.source_breakdown(&Default::default())?)?
    );
    assert_eq!(
        serde_json::to_value(&snapshot.projects)?,
        serde_json::to_value(dashboard.project_breakdown(&Default::default())?)?
    );
    assert_eq!(
        serde_json::to_value(&snapshot.costs)?,
        serde_json::to_value(dashboard.cost_breakdown(&Default::default())?)?
    );
    assert_eq!(
        serde_json::to_value(&snapshot.health)?,
        serde_json::to_value(dashboard.health()?)?
    );
    let serialized = serde_json::to_value(&snapshot)?;
    assert!(serialized.get("home_overview").is_some());
    assert!(serialized.get("heatmap").is_some());
    assert!(serialized.get("trends_daily").is_some());

    let old_snapshot: ReadyWidgetsSnapshotCompatibility =
        serde_json::from_str(r#"{"overview":{}}"#)?;
    assert!(old_snapshot.home_overview.is_none());
    assert!(old_snapshot.heatmap.is_none());
    assert!(old_snapshot.trends_daily.is_none());

    assert!(snapshot.overview.bucket_count >= 180);
    assert_eq!(snapshot.sources.len(), 3);
    assert!(!snapshot.models.is_empty());
    assert!(!snapshot.projects.is_empty());
    Ok(())
}
