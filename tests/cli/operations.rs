use super::*;

#[test]
fn logging_runtime_writes_ndjson_file() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let output = fixture.output_with_env(
        &["doctor"],
        &[("LLMUSAGE_LOG", "info"), ("RUST_LOG", "off")],
    )?;
    assert!(output.status.success(), "{output:?}");

    let entries = read_recent_log_entries(&fixture.paths, 100, Some("info"), None)?;
    let status = runtime_status(&fixture.paths)?;
    assert!(
        entries.iter().any(|entry| {
            entry.level == "INFO"
                && entry.fields["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("doctor"))
        }),
        "expected INFO doctor event in {}: {entries:#?}",
        status.path
    );
    Ok(())
}

#[test]
fn report_stdout_is_not_polluted_by_logging() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:stdout-clean:1",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-05-01T00:00:00Z",
        input_tokens: 10,
        total_tokens: 10,
        project_hash: "project-a",
        project_label: "Project A",
        session_id: Some("stdout-clean-session"),
        source_path_hash: Some("stdout-clean-source"),
        ..SeedEvent::default()
    })?;

    let output = fixture.output_with_env(
        &["daily", "--json", "--timezone", "UTC"],
        &[("LLMUSAGE_LOG", "info"), ("RUST_LOG", "off")],
    )?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout)?;
    let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
    assert!(parsed["daily"].is_array());
    assert!(!stdout.contains("INFO"), "{stdout}");
    assert!(!stdout.contains("开始初始化本地目录"), "{stdout}");
    assert!(runtime_status(&fixture.paths)?.exists);
    Ok(())
}

#[test]
fn logs_command_filters_level_and_command() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fs::create_dir_all(&fixture.paths.logs_dir)?;
    fs::write(
        &fixture.paths.log_file_path,
        [
            r#"{"timestamp":"2026-06-02T00:00:00Z","level":"INFO","target":"test","fields":{"message":"sync info","command":"sync","run_id":1}}"#,
            r#"{"timestamp":"2026-06-02T00:01:00Z","level":"WARN","target":"test","fields":{"message":"sync warn","command":"sync","source":"codex","run_id":2,"error":"warning only"}}"#,
            r#"{"timestamp":"2026-06-02T00:02:00Z","level":"ERROR","target":"test","fields":{"message":"doctor error","command":"doctor","run_id":3,"error":"doctor failed"}}"#,
        ]
        .join("\n")
            + "\n",
    )?;

    let store = Store::new(&fixture.paths)?;
    let sync_run = store.run_log().record_run_start("sync")?;
    store
        .run_log()
        .finish_run(sync_run, "success", Some("human sync summary"), None)?;
    let doctor_run = store.run_log().record_run_start("doctor")?;
    store
        .run_log()
        .finish_run(doctor_run, "failed", None, Some("doctor failed"))?;

    let payload = fixture.json_with_env(
        &[
            "logs",
            "--limit",
            "10",
            "--level",
            "warn",
            "--command",
            "sync",
            "--json",
        ],
        &[("LLMUSAGE_LOG", "off"), ("RUST_LOG", "off")],
    )?;
    let entries = payload["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 1, "{payload:#}");
    assert_eq!(entries[0]["level"], "WARN");
    assert_eq!(entries[0]["command"], "sync");
    assert_eq!(entries[0]["source"], "codex");
    assert_eq!(entries[0]["error"], "warning only");

    let runs = payload["recent_runs"]
        .as_array()
        .expect("recent_runs array");
    assert_eq!(runs.len(), 1, "{payload:#}");
    assert_eq!(runs[0]["command"], "sync");
    assert_eq!(runs[0]["summary"], "human sync summary");
    Ok(())
}

#[test]
fn diagnostics_includes_logs_summary_without_dumping_entries() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fs::create_dir_all(&fixture.paths.logs_dir)?;
    fs::write(
        &fixture.paths.log_file_path,
        r#"{"timestamp":"2026-06-02T00:02:00Z","level":"ERROR","target":"test","fields":{"message":"sync failed","command":"sync","error":"redacted summary"}}"#,
    )?;

    let payload = fixture.json_with_env(
        &["diagnostics"],
        &[("LLMUSAGE_LOG", "off"), ("RUST_LOG", "off")],
    )?;
    let expected_log_path = fixture.paths.log_file_path.to_string_lossy().to_string();
    assert_eq!(
        payload["paths"]["log_file_path"].as_str(),
        Some(expected_log_path.as_str())
    );
    assert_eq!(payload["logs"]["exists"].as_bool(), Some(true));
    assert_eq!(payload["logs"]["recent_error_count"].as_u64(), Some(1));
    assert_eq!(payload["logs"]["retained_files"].as_u64(), Some(1));
    assert!(payload["logs"]["total_size_bytes"].as_u64().is_some());
    assert!(payload["logs"]["dropped_event_count"].as_u64().is_some());
    assert!(
        payload["logs"]["maintenance_error_count"]
            .as_u64()
            .is_some()
    );
    assert!(
        payload["logs"].get("entries").is_none(),
        "diagnostics should expose only log summary, not dump log contents"
    );
    Ok(())
}

#[test]
fn run_tracked_records_failure_for_sync_rebuild() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:rebuild-failure:1",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-05-01T00:00:00Z",
        input_tokens: 10,
        total_tokens: 10,
        project_hash: "project-a",
        project_label: "Project A",
        session_id: Some("rebuild-failure-session"),
        source_path_hash: Some("rebuild-failure-source"),
        ..SeedEvent::default()
    })?;
    let missing_source = fixture.home.join("missing-codex-source.jsonl");
    let conn = Connection::open(&fixture.paths.db_path)?;
    conn.execute(
        r#"
        INSERT INTO source_file(source, file_path, state, last_seen_at, last_state_change_at)
        VALUES ('codex', ?1, 'missing', NULL, '2026-06-02T00:00:00Z')
        "#,
        [missing_source.to_string_lossy().to_string()],
    )?;
    drop(conn);

    let output = fixture.output_with_env(
        &["sync", "--rebuild", "--source", "codex"],
        &[("LLMUSAGE_LOG", "off"), ("RUST_LOG", "off")],
    )?;
    assert!(!output.status.success(), "{output:?}");

    let store = Store::new(&fixture.paths)?;
    let recent = store.run_log().recent_runs(5)?;
    let failed = recent
        .iter()
        .find(|run| run.command == "sync --rebuild")
        .expect("sync --rebuild run should be recorded");
    assert_eq!(failed.status, "failed");
    assert!(
        failed
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Refusing lossy sync --rebuild")),
        "{failed:#?}"
    );
    Ok(())
}

#[test]
fn run_tracked_records_failure_for_daily() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let output = fixture.output_with_env(
        &["daily", "--all", "--since", "2026-01-01"],
        &[("LLMUSAGE_LOG", "error"), ("RUST_LOG", "off")],
    )?;
    assert!(!output.status.success(), "{output:?}");

    let store = Store::new(&fixture.paths)?;
    let recent = store.run_log().recent_runs(5)?;
    let failed = recent
        .iter()
        .find(|run| run.command == "daily")
        .expect("daily run should be recorded");
    assert_eq!(failed.status, "failed");
    assert!(
        failed.error.as_deref().is_some_and(|error| {
            error.contains("--all cannot be combined with --since or --until")
        }),
        "{failed:#?}"
    );

    let entries = read_recent_log_entries(&fixture.paths, 100, Some("error"), Some("daily"))?;
    assert!(
        entries.iter().any(|entry| {
            entry.level == "ERROR"
                && entry.command.as_deref() == Some("daily")
                && entry
                    .message
                    .as_deref()
                    .is_some_and(|message| message.contains("run failed"))
        }),
        "expected ERROR daily run_tracked event in logs: {entries:#?}"
    );
    Ok(())
}

#[test]
fn status_does_not_record_run_log() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let output = fixture.output_with_env(
        &["status"],
        &[("LLMUSAGE_LOG", "error"), ("RUST_LOG", "off")],
    )?;
    assert!(output.status.success(), "{output:?}");

    let store = Store::new(&fixture.paths)?;
    let recent = store.run_log().recent_runs(5)?;
    assert!(
        recent.iter().all(|run| run.command != "status"),
        "read-only status must not write run_log: {recent:#?}"
    );
    Ok(())
}

#[test]
fn status_failure_emits_error_without_run_log() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let conn = Connection::open(&fixture.paths.db_path)?;
    conn.execute(
        "UPDATE meta SET value = '99999' WHERE key = 'schema_version'",
        [],
    )?;
    drop(conn);

    let output = fixture.output_with_env(
        &["status"],
        &[("LLMUSAGE_LOG", "error"), ("RUST_LOG", "off")],
    )?;
    assert!(!output.status.success(), "{output:?}");

    let store = Store::new(&fixture.paths)?;
    let recent = store.run_log().recent_runs(5)?;
    assert!(
        recent.iter().all(|run| run.command != "status"),
        "failed status must not write run_log: {recent:#?}"
    );

    let entries = read_recent_log_entries(&fixture.paths, 100, Some("error"), Some("status"))?;
    assert!(
        entries.iter().any(|entry| {
            entry.level == "ERROR"
                && entry.command.as_deref() == Some("status")
                && entry
                    .message
                    .as_deref()
                    .is_some_and(|message| message.contains("run failed"))
        }),
        "expected ERROR status event in logs: {entries:#?}"
    );
    Ok(())
}

#[test]
fn cli_home_flag_overrides_llmusage_home_env() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let other = TempDir::new()?;
    let output = test_process::llmusage_command()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("--home")
        .arg(&fixture.paths.root_dir)
        .arg("statusline")
        .arg("--no-cache")
        .env("LLMUSAGE_HOME", other.path())
        .env("HOME", &fixture.home)
        .env("USERPROFILE", &fixture.home)
        .env("RUST_LOG", "off")
        .output()
        .context("spawn llmusage --home statusline")?;

    assert!(output.status.success(), "{output:?}");
    assert!(fixture.paths.db_path.is_file());
    assert!(!other.path().join("llmusage.db").exists());
    Ok(())
}

#[test]
fn doctor_refresh_pricing_writes_catalog_version_meta() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:pricing-meta:1",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-05-01T00:00:00Z",
        input_tokens: 500_000,
        cache_read_tokens: 0,
        output_tokens: 100_000,
        reasoning_output_tokens: 0,
        total_tokens: 600_000,
        project_hash: "project-a",
        project_label: "Project A",
        project_ref: Some("example/project-a"),
        session_id: Some("pricing-meta-session"),
        source_path_hash: Some("pricing-meta-source"),
        ..SeedEvent::default()
    })?;
    let snapshot = fixture.home.join("pricing-snapshot.json");
    std::fs::write(
        &snapshot,
        r#"{
            "version": "litellm-snapshot-2026-05",
            "models": [
                {
                    "source": "codex",
                    "matchers": ["gpt-5"],
                    "input_per_mtok": 2.0,
                    "cached_per_mtok": 0.2,
                    "output_per_mtok": 20.0
                }
            ]
        }"#,
    )?;

    let output = fixture.output(&["doctor", "--refresh-pricing", snapshot.to_str().unwrap()])?;
    assert!(output.status.success(), "{output:?}");

    let store = Store::new(&fixture.paths)?;
    assert_eq!(
        store.meta_value("pricing_catalog_version")?.as_deref(),
        Some("litellm-snapshot-2026-05")
    );
    let catalog_file = store
        .meta_value("pricing_catalog_file")?
        .expect("snapshot file metadata");
    assert!(catalog_file.starts_with("base-"));
    assert!(catalog_file.ends_with(".json"));
    assert!(
        fixture
            .paths
            .root_dir
            .join("pricing")
            .join(catalog_file)
            .is_file()
    );

    let conn = Connection::open(&fixture.paths.db_path)?;
    let (event_status, event_source, event_cost): (String, String, f64) = conn.query_row(
        r#"
        SELECT pricing_status, COALESCE(pricing_source, ''), cost_with_cache_usd
        FROM usage_event
        WHERE event_key = 'codex:pricing-meta:1'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(event_status, "snapshot");
    assert_eq!(event_source, "litellm-snapshot-2026-05");
    assert!((event_cost - 3.0).abs() < 1e-6);

    let (bucket_status, bucket_source, bucket_cost): (String, String, f64) = conn.query_row(
        r#"
        SELECT pricing_status, COALESCE(pricing_source, ''), cost_with_cache_usd
        FROM usage_bucket_30m
        WHERE source = 'codex' AND model = 'gpt-5'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(bucket_status, "snapshot");
    assert_eq!(bucket_source, "litellm-snapshot-2026-05");
    assert!((bucket_cost - 3.0).abs() < 1e-6);
    Ok(())
}

#[test]
fn doctor_refresh_pricing_accepts_native_litellm_snapshot() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:native-pricing:1",
        source: "codex",
        model: "gpt-5.5",
        event_at: "2026-05-01T00:00:00Z",
        input_tokens: 1_000_000,
        cache_read_tokens: 2_000_000,
        output_tokens: 3_000_000,
        reasoning_output_tokens: 4_000_000,
        total_tokens: 10_000_000,
        project_hash: "project-native",
        project_label: "Project Native",
        session_id: Some("native-pricing-session"),
        source_path_hash: Some("native-pricing-source"),
        ..SeedEvent::default()
    })?;

    let snapshot = fixture.home.join("native-litellm.json");
    std::fs::write(
        &snapshot,
        r#"{
            "models": {
                "gpt-5": {
                    "litellm_provider": "openai",
                    "input_cost_per_token": 0.00000125,
                    "output_cost_per_token": 0.000010,
                    "cache_creation_input_token_cost": 0.00000125,
                    "cache_read_input_token_cost": 0.000000125,
                    "output_cost_per_reasoning_token": 0.000010
                }
            }
        }"#,
    )?;

    let output = fixture.output(&["doctor", "--refresh-pricing", snapshot.to_str().unwrap()])?;
    assert!(output.status.success(), "{output:?}");

    let store = Store::new(&fixture.paths)?;
    assert_eq!(
        store.meta_value("pricing_catalog_version")?.as_deref(),
        Some("native-litellm")
    );

    let conn = Connection::open(&fixture.paths.db_path)?;
    let (status, source, event_cost, bucket_cost): (String, String, f64, f64) = conn.query_row(
        r#"
        SELECT
            e.pricing_status,
            COALESCE(e.pricing_source, ''),
            e.cost_with_cache_usd,
            b.cost_with_cache_usd
        FROM usage_event e
        JOIN usage_bucket_30m b
          ON b.source = e.source
         AND b.model = e.model
         AND b.hour_start = e.hour_start
         AND b.project_hash = COALESCE(e.project_hash, '')
        WHERE e.event_key = 'codex:native-pricing:1'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    assert_eq!(status, "snapshot");
    assert_eq!(source, "native-litellm");
    assert!((event_cost - 31.5).abs() < 1e-6);
    assert!((bucket_cost - event_cost).abs() < 1e-9);
    Ok(())
}

#[test]
fn catalog_cli_applies_reports_and_resets_overlay_across_processes() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:catalog-overlay:1",
        source: "codex",
        model: "private-cli-model",
        event_at: "2026-05-01T00:00:00Z",
        input_tokens: 1_000_000,
        cache_read_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: 1_000_000,
        project_hash: "project-catalog",
        project_label: "Project Catalog",
        session_id: Some("catalog-overlay-session"),
        source_path_hash: Some("catalog-overlay-source"),
        ..SeedEvent::default()
    })?;
    let overlay = fixture.home.join("pricing-overlay.json");
    fs::write(
        &overlay,
        r#"{
  "schema_version": 2,
  "kind": "overlay",
  "version": "team-catalog-1",
  "models": [
    {
      "id": "private-cli-model",
      "sources": ["codex"],
      "matches": [{ "value": "private-cli-model", "mode": "exact" }],
      "rates": {
        "default": {
          "input_per_mtok": 3.0,
          "cached_per_mtok": 0.3,
          "output_per_mtok": 18.0
        }
      },
      "context_window": 2000000
    }
  ]
}"#,
    )?;

    let applied = fixture.output(&["catalog", "apply", overlay.to_str().unwrap()])?;
    assert!(applied.status.success(), "{applied:?}");
    let status = fixture.json(&["catalog", "status", "--json"])?;
    assert_eq!(status["base"]["identity"].as_str(), Some("static-v2"));
    assert_eq!(
        status["overlay"]["version"].as_str(),
        Some("team-catalog-1")
    );
    assert!(
        status["effective"]["identity"]
            .as_str()
            .is_some_and(|identity| identity.starts_with("effective-"))
    );
    assert_eq!(status["rebase_available"].as_bool(), Some(false));

    let conn = Connection::open(&fixture.paths.db_path)?;
    let (priced, source): (f64, String) = conn.query_row(
        "SELECT cost_with_cache_usd, pricing_source FROM usage_event WHERE event_key = 'codex:catalog-overlay:1'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert!((priced - 3.0).abs() < 1e-9);
    assert!(source.starts_with("effective-"));
    drop(conn);

    let store = Store::new(&fixture.paths)?;
    let pressure = Dashboard::open(&store)?.context_pressure(&Default::default())?;
    assert!((pressure.peak_percent - 0.5).abs() < 1e-9);
    assert_eq!(
        pressure.peak_model.as_deref(),
        Some("codex:private-cli-model")
    );

    let reset = fixture.output(&["catalog", "reset"])?;
    assert!(reset.status.success(), "{reset:?}");
    let reset_status = fixture.json(&["catalog", "status", "--json"])?;
    assert!(reset_status["overlay"].is_null());
    assert_eq!(
        reset_status["effective"]["identity"].as_str(),
        Some("static-v2")
    );

    let conn = Connection::open(&fixture.paths.db_path)?;
    let (cost, pricing_status): (f64, String) = conn.query_row(
        "SELECT cost_with_cache_usd, pricing_status FROM usage_event WHERE event_key = 'codex:catalog-overlay:1'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(cost, 0.0);
    assert_eq!(pricing_status, "unpriced");
    Ok(())
}

#[test]
fn source_status_command_executes_against_fresh_runtime() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let output = fixture.output_with_env(
        &["source-status"],
        &[("LLMUSAGE_LOG", "off"), ("RUST_LOG", "off")],
    )?;
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("Source status:"), "{stdout}");
    assert!(stdout.contains("Host local:"), "{stdout}");
    assert!(stdout.contains("- Source status codex:"), "{stdout}");
    assert!(stdout.contains("- Platform monitor"), "{stdout}");
    Ok(())
}

#[test]
fn statusline_outputs_single_line_without_stdin() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let output = fixture.output(&["statusline", "--no-cache"])?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.lines().count() <= 1);
    assert!(stdout.contains("today") || stdout.contains("unavailable"));
    Ok(())
}
