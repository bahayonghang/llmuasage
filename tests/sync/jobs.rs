use super::*;

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

    let output = test_process::llmusage_command()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["doctor", "--json"])
        .env("HOME", &fixture.home)
        .env("USERPROFILE", &fixture.home)
        .env("CODEX_HOME", &fixture.codex_home)
        .env("OPENCODE_HOME", &fixture.opencode_home)
        .env("RUST_LOG", "off")
        .output()
        .context("spawn llmusage doctor subprocess")?;
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
