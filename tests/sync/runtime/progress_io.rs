use super::*;

#[test]
fn legacy_warning_keeps_human_lines_and_json_stdout_separate() -> Result<()> {
    let temp = TempDir::new()?;
    let home = temp.path().join("home");
    fs::create_dir_all(&home)?;
    let root = temp.path().join("data");
    let paths = AppPaths::with_root(root.clone())?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    store.open_connection()?.execute(
        "INSERT INTO usage_event(event_key, source, model, event_at, hour_start, input_tokens, cache_read_tokens, cache_creation_tokens, output_tokens, reasoning_output_tokens, total_tokens, created_at)
         VALUES ('legacy-warning', 'codex', 'gpt-5', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 10, 0, 0, 0, 0, 10, '2026-09-01T00:00:00Z')",
        [],
    )?;
    store.set_meta_value("token_accounting_version.codex", "2")?;
    for json in [false, true] {
        let mut command = crate::test_process::llmusage_command();
        command
            .arg("--home")
            .arg(&root)
            .args(["sync", "--source", "codex"])
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("CODEX_HOME", home.join(".codex"))
            .env("RUST_LOG", "warn")
            .env("LLMUSAGE_LOG", "warn");
        if json {
            command.arg("--json-events");
        }
        let output = command.output()?;
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout)?;
        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            stderr.contains("skipped legacy token accounting sources"),
            "{stderr}"
        );
        if json {
            let events = stdout
                .lines()
                .map(serde_json::from_str::<serde_json::Value>)
                .collect::<serde_json::Result<Vec<_>>>()?;
            assert!(events.iter().any(|event| event["event"] == "finished"));
        } else {
            assert!(stdout.starts_with("Sync finished:"), "{stdout}");
            assert!(
                stdout.contains("llmusage sync --rebuild --source codex"),
                "{stdout}"
            );
            assert!(
                stderr
                    .lines()
                    .filter(|line| line.contains("已获取 SQLite sync worker 锁"))
                    .all(|line| !line.contains("WARN")),
                "{stderr}"
            );
        }
    }
    assert_eq!(store.token_accounting_version(SourceKind::Codex)?, Some(2));
    assert!(
        read_recent_log_entries(&paths, 100, Some("warn"), None)?
            .iter()
            .any(|entry| {
                entry.message.as_deref().is_some_and(|message| {
                    message.contains("skipped legacy token accounting sources")
                })
            })
    );
    Ok(())
}

#[test]
fn json_events_subprocess_emits_ndjson_per_event() -> Result<()> {
    let temp = TempDir::new()?;
    let home = temp.path().join("home");
    let root = temp.path().join(".llmusage");
    fs::create_dir_all(&home)?;
    let paths = AppPaths::with_root(root.clone())?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    let conn = store.open_connection()?;
    conn.execute(
        r#"
        INSERT INTO usage_event(
            event_key, source, model, event_at, hour_start,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens, created_at
        ) VALUES ('json-pricing-progress', 'codex', 'gpt-5', ?1, ?1,
                  100, 0, 0, 10, 0, 110, ?1)
        "#,
        ["2026-07-16T00:00:00Z"],
    )?;
    conn.execute(
        r#"
        INSERT INTO usage_bucket_30m(
            source, provider_label, model, hour_start, project_hash,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens,
            event_count, updated_at
        ) VALUES ('codex', '', 'gpt-5', ?1, '', 100, 0, 0, 10, 0, 110, 1, ?1)
        "#,
        ["2026-07-16T00:00:00Z"],
    )?;
    drop(conn);
    store.mark_current_token_accounting(SourceKind::Codex)?;
    store.set_meta_value("pricing_catalog_version", "static-v1")?;

    let output = crate::test_process::llmusage_command()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("--home")
        .arg(&root)
        .args(["sync", "--source", "codex", "--json-events"])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("CODEX_HOME", home.join(".codex"))
        .env("OPENCODE_HOME", home.join("opencode"))
        .env("LLMUSAGE_LOG", "info")
        .env("RUST_LOG", "off")
        .output()
        .context("spawn llmusage sync --json-events")?;
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8(output.stdout)?;
    let stdout_lines = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    assert!(
        stdout_lines
            .iter()
            .all(|line| line.trim_start().starts_with('{')),
        "sync --json-events stdout must contain only NDJSON lifecycle events: {stdout}"
    );
    let json_lines = stdout_lines
        .iter()
        .map(|line| serde_json::from_str::<serde_json::Value>(line))
        .collect::<serde_json::Result<Vec<_>>>()?;
    assert!(json_lines.iter().any(|line| line["event"] == "started"));
    assert!(
        json_lines
            .iter()
            .any(|line| line["event"] == "bootstrap_started")
    );
    let event_names = json_lines
        .iter()
        .filter_map(|line| line["event"].as_str())
        .collect::<Vec<_>>();
    let pricing_started = event_names
        .iter()
        .position(|event| *event == "pricing_upgrade_started")
        .expect("pricing upgrade start event");
    let pricing_progress = event_names
        .iter()
        .position(|event| *event == "pricing_upgrade_progress")
        .expect("pricing upgrade progress event");
    let bucket_reconcile = event_names
        .iter()
        .position(|event| *event == "pricing_bucket_reconcile_started")
        .expect("pricing bucket reconcile event");
    let pricing_finished = event_names
        .iter()
        .position(|event| *event == "pricing_upgrade_finished")
        .expect("pricing upgrade finished event");
    assert!(pricing_started < pricing_progress);
    assert!(pricing_progress < bucket_reconcile);
    assert!(bucket_reconcile < pricing_finished);
    assert_eq!(json_lines[pricing_progress]["processed_events"], 1);
    assert_eq!(json_lines[pricing_progress]["total_events"], 1);
    assert!(
        json_lines
            .iter()
            .any(|line| line["event"] == "lock_waiting")
    );
    assert!(
        json_lines
            .iter()
            .any(|line| line["event"] == "source_started")
    );
    assert!(
        json_lines
            .iter()
            .any(|line| line["event"] == "lock_acquired")
    );
    assert!(json_lines.iter().any(|line| line["event"] == "finished"));

    let log_entries = read_recent_log_entries(&paths, 100, Some("info"), None)?;
    for phase in ["started", "bucket_reconcile", "finished"] {
        assert!(log_entries.iter().any(|entry| {
            entry.fields["operation"] == "pricing_recompute" && entry.fields["phase"] == phase
        }));
    }
    Ok(())
}

#[test]
fn human_sync_subprocess_stderr_contains_no_ansi_escapes() -> Result<()> {
    for progress_env in [None, Some("off")] {
        let temp = TempDir::new()?;
        let home = temp.path().join("home");
        let root = temp.path().join(".llmusage");
        fs::create_dir_all(&home)?;

        let mut command = crate::test_process::llmusage_command();
        command
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .arg("--home")
            .arg(&root)
            .args(["sync", "--source", "codex"])
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("CODEX_HOME", home.join(".codex"))
            .env("OPENCODE_HOME", home.join("opencode"))
            .env("LLMUSAGE_LOG", "info")
            .env("RUST_LOG", "off");
        if let Some(value) = progress_env {
            command.env("LLMUSAGE_PROGRESS", value);
        }
        let output = command
            .output()
            .context("spawn llmusage non-TTY sync subprocess")?;
        assert!(output.status.success(), "{output:?}");
        assert!(
            !output.stderr.contains(&0x1B),
            "sync human stderr must not contain ANSI escapes (LLMUSAGE_PROGRESS={progress_env:?}): {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !output.stdout.contains(&0x1B),
            "sync summary stdout must not contain ANSI escapes (LLMUSAGE_PROGRESS={progress_env:?})"
        );
    }
    Ok(())
}
