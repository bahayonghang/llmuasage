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

#[test]
fn bilingual_sync_keeps_human_and_machine_streams_separate_in_plain_modes() -> Result<()> {
    for (language, title, lock, header, total) in [
        (
            "en",
            "Sync finished:",
            "SQLite sync worker lock acquired",
            "SOURCE",
            "TOTAL",
        ),
        (
            "zh",
            "同步完成：",
            "已获取 SQLite sync worker 锁",
            "来源",
            "合计",
        ),
    ] {
        for plain_mode in ["NO_COLOR", "LLMUSAGE_NO_COLOR", "LLMUSAGE_PROGRESS"] {
            let temp = TempDir::new()?;
            let home = temp.path().join("home");
            fs::create_dir_all(&home)?;
            let root = temp.path().join("data");
            llmusage::runtime::preferences::write(
                &root,
                llmusage::common::i18n::Language::parse(language).unwrap(),
            )?;
            let run = |machine: Option<&str>| -> Result<std::process::Output> {
                let mut command = crate::test_process::llmusage_command();
                command
                    .arg("--home")
                    .arg(&root)
                    .args(["sync", "--source", "codex"])
                    .env("HOME", &home)
                    .env("USERPROFILE", &home)
                    .env("CODEX_HOME", home.join(".codex"))
                    .env("GEMINI_CLI_HOME", home.join(".gemini"))
                    .env("LLMUSAGE_PRICING_REFRESH", "off")
                    .env("RUST_LOG", "off")
                    .env("LLMUSAGE_LOG", "info")
                    .env(plain_mode, "off");
                if let Some(machine) = machine {
                    command.arg(machine);
                }
                Ok(command.output()?)
            };
            let human = run(None)?;
            assert!(human.status.success(), "{human:?}");
            let stdout = String::from_utf8(human.stdout)?;
            let stderr = String::from_utf8(human.stderr)?;
            assert!(stdout.starts_with(title), "{stdout}");
            assert!(
                stdout.contains(header) && stdout.contains(total),
                "{stdout}"
            );
            assert!(stdout.contains('┌') && stdout.contains('┘'), "{stdout}");
            let cells = stdout.lines().find(|line| line.contains(header)).unwrap();
            assert_eq!(cells.matches('│').count(), 11, "{cells}");
            assert!(stderr.contains(lock), "{stderr}");
            assert!(!stdout.contains(lock), "{stdout}");
            for stream in [&stdout, &stderr] {
                assert!(
                    !stream.contains('\u{1b}') && !stream.contains('\r'),
                    "{stream}"
                );
            }
            for machine in ["--json-events", "--emit-shards"] {
                let output = run(Some(machine))?;
                assert!(output.status.success(), "{output:?}");
                let stdout = String::from_utf8(output.stdout)?;
                let values = stdout
                    .lines()
                    .map(serde_json::from_str::<serde_json::Value>)
                    .collect::<serde_json::Result<Vec<_>>>()?;
                assert!(!values.is_empty());
                assert!(!stdout.contains(title) && !stdout.contains('┌'), "{stdout}");
                if machine == "--json-events" {
                    assert_eq!(values.first().unwrap()["event"], "started");
                    assert_eq!(values.last().unwrap()["event"], "finished");
                } else {
                    assert_eq!(values.first().unwrap()["kind"], "header");
                    assert_eq!(values.last().unwrap()["kind"], "trailer");
                    assert_eq!(values[0]["shard_protocol"], 2);
                    assert_eq!(values[0]["source_accounting_versions"]["codex"], 3);
                }
                assert!(!output.stderr.contains(&0x1b));
            }
        }
    }
    Ok(())
}

fn sync_failure_command(
    root: &std::path::Path,
    home: &std::path::Path,
    source: &str,
) -> std::process::Command {
    let mut command = crate::test_process::llmusage_command();
    command
        .arg("--home")
        .arg(root)
        .args(["sync", "--source", source])
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("CODEX_HOME", home.join("codex"))
        .env("GEMINI_CLI_HOME", home.join("gemini"))
        .env("OPENCODE_HOME", home.join("opencode"))
        .env("ZCODE_HOME", home.join("zcode"))
        .env("LLMUSAGE_PRICING_REFRESH", "off")
        .env("LLMUSAGE_LOG", "error")
        .env("RUST_LOG", "off");
    command
}

#[test]
fn bilingual_lossy_refusal_translates_all_human_sinks_and_keeps_canonical_wire_error() -> Result<()>
{
    let temp = TempDir::new()?;
    let home = temp.path().join("home");
    fs::create_dir_all(&home)?;
    let mut canonical_errors = Vec::new();
    for language in [
        llmusage::common::i18n::Language::English,
        llmusage::common::i18n::Language::Chinese,
    ] {
        let root = temp.path().join(language.code());
        llmusage::runtime::preferences::write(&root, language)?;
        let paths = AppPaths::with_root(root.clone())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        store.mark_current_token_accounting(SourceKind::Codex)?;
        store.open_connection()?.execute_batch("INSERT INTO usage_event(event_key, source, model, event_at, hour_start, input_tokens, cache_read_tokens, cache_creation_tokens, output_tokens, reasoning_output_tokens, total_tokens, created_at) VALUES ('protected', 'codex', 'gpt-5', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 12, 0, 0, 0, 0, 12, '2026-01-01T00:00:00Z');")?;
        store.open_connection()?.execute("INSERT INTO source_file(source, file_path, state, last_state_change_at) VALUES ('codex', ?1, 'missing', '2026-01-01T00:00:00Z')", [home.join("missing-rollout.jsonl").to_string_lossy().as_ref()])?;
        for level in ["off", "error"] {
            let output = sync_failure_command(&root, &home, "codex")
                .arg("--rebuild")
                .env("RUST_LOG", level)
                .output()?;
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(output.stdout.is_empty());
            let stderr = String::from_utf8(output.stderr)?;
            assert!(
                stderr.contains(language.select("Refusing lossy", "拒绝有损重建")),
                "{stderr}"
            );
            assert!(
                stderr.contains("--allow-lossy-rebuild") && stderr.contains("codex"),
                "{stderr}"
            );
            if language == llmusage::common::i18n::Language::Chinese {
                assert!(
                    !stderr.contains("Refusing lossy") && !stderr.contains("Original detail"),
                    "{stderr}"
                );
            }
        }
        let output = sync_failure_command(&root, &home, "codex")
            .args(["--rebuild", "--json-events"])
            .output()?;
        assert_eq!(output.status.code(), Some(1));
        let values = String::from_utf8(output.stdout)?
            .lines()
            .map(serde_json::from_str::<serde_json::Value>)
            .collect::<serde_json::Result<Vec<_>>>()?;
        let error = values
            .iter()
            .find(|value| value["event"] == "failed")
            .expect("canonical failed event")["error"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(error.starts_with("Refusing lossy sync --rebuild"));
        canonical_errors.push(error);
        assert_eq!(
            store.open_connection()?.query_row(
                "SELECT total_tokens FROM usage_event WHERE event_key='protected'",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            12
        );
        let entries = read_recent_log_entries(&paths, 100, Some("error"), None)?;
        assert!(entries.iter().any(|entry| {
            entry
                .error
                .as_deref()
                .is_some_and(|error| error.starts_with("Refusing lossy sync --rebuild"))
        }));
    }
    assert_eq!(canonical_errors[0], canonical_errors[1]);
    Ok(())
}

#[test]
fn bilingual_sqlite_open_failure_keeps_success_exit_and_canonical_stats_and_shards() -> Result<()> {
    let temp = TempDir::new()?;
    let home = temp.path().join("home");
    fs::create_dir_all(home.join("opencode"))?;
    fs::create_dir_all(home.join("zcode/cli/db"))?;
    fs::write(home.join("opencode/opencode.db"), b"not sqlite")?;
    fs::write(home.join("zcode/cli/db/db.sqlite"), b"not sqlite")?;
    for (source, canonical) in [
        (SourceKind::Opencode, "OpenCode SQLite DB 打开失败"),
        (SourceKind::Zcode, "ZCode SQLite DB 打开失败"),
    ] {
        let mut machine_stats = Vec::new();
        let mut shard_trailers = Vec::new();
        for language in [
            llmusage::common::i18n::Language::English,
            llmusage::common::i18n::Language::Chinese,
        ] {
            let root = temp.path().join(format!("{source}-{}", language.code()));
            llmusage::runtime::preferences::write(&root, language)?;
            let paths = AppPaths::with_root(root.clone())?;
            let store = Store::new(&paths)?;
            store.bootstrap()?;
            store.mark_current_token_accounting(source)?;
            let output = sync_failure_command(&root, &home, source.as_str()).output()?;
            assert!(output.status.success(), "{output:?}");
            let stdout = String::from_utf8(output.stdout)?;
            assert!(
                stdout.contains(language.select(
                    "Cannot open source SQLite database",
                    "无法打开来源 SQLite 数据库"
                )),
                "{stdout}"
            );
            assert!(!stdout.contains(canonical), "{stdout}");
            for mode in ["--json-events", "--emit-shards"] {
                let output = sync_failure_command(&root, &home, source.as_str())
                    .arg(mode)
                    .output()?;
                assert!(output.status.success(), "{output:?}");
                let values = String::from_utf8(output.stdout)?
                    .lines()
                    .map(serde_json::from_str::<serde_json::Value>)
                    .collect::<serde_json::Result<Vec<_>>>()?;
                let mut stats = if mode == "--json-events" {
                    values
                        .iter()
                        .find(|value| value["event"] == "source_finished")
                        .expect("finished source")["stats"]
                        .clone()
                } else {
                    values.last().unwrap()["sources"][0].clone()
                };
                assert_eq!(stats["last_error"], canonical);
                assert_eq!(stats["absent"], false);
                stats.as_object_mut().unwrap().remove("lock_wait_ms");
                if mode == "--json-events" {
                    machine_stats.push(stats);
                } else {
                    shard_trailers.push(stats);
                }
            }
            assert_eq!(
                store.token_accounting_version(source)?,
                Some(llmusage::store::expected_token_accounting_version(source))
            );
        }
        assert_eq!(machine_stats[0], machine_stats[1]);
        assert_eq!(shard_trailers[0], shard_trailers[1]);
    }
    Ok(())
}

#[test]
fn bilingual_omp_preflight_and_invalid_since_preserve_exit_and_safe_ordering() -> Result<()> {
    let temp = TempDir::new()?;
    let home = temp.path().join("home");
    fs::create_dir_all(&home)?;
    let mut canonical_errors = Vec::new();
    for language in [
        llmusage::common::i18n::Language::English,
        llmusage::common::i18n::Language::Chinese,
    ] {
        let root = temp.path().join(language.code());
        llmusage::runtime::preferences::write(&root, language)?;
        let paths = AppPaths::with_root(root.clone())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        store.open_connection()?.execute_batch("INSERT INTO usage_event(event_key, source, model, event_at, hour_start, input_tokens, cache_read_tokens, cache_creation_tokens, output_tokens, reasoning_output_tokens, total_tokens, created_at) VALUES ('old-pi', 'pi', 'model', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 12, 0, 0, 0, 0, 12, '2026-01-01T00:00:00Z');")?;
        store.set_meta_value("token_accounting_version.pi", "2")?;
        for level in ["off", "error"] {
            let output = sync_failure_command(&root, &home, "omp")
                .env("RUST_LOG", level)
                .output()?;
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
            let stderr = String::from_utf8(output.stderr)?;
            assert!(
                stderr.contains(language.select("Refusing --source omp", "拒绝同步 --source omp")),
                "{stderr}"
            );
            assert!(stderr.contains("llmusage sync --rebuild --source pi"));
            if language == llmusage::common::i18n::Language::Chinese {
                assert!(
                    !stderr.contains("pre-split token-accounting contract"),
                    "{stderr}"
                );
            }
        }
        let output = sync_failure_command(&root, &home, "omp")
            .arg("--json-events")
            .output()?;
        assert_eq!(output.status.code(), Some(1));
        let values = String::from_utf8(output.stdout)?
            .lines()
            .map(serde_json::from_str::<serde_json::Value>)
            .collect::<serde_json::Result<Vec<_>>>()?;
        canonical_errors.push(
            values
                .iter()
                .find(|value| value["event"] == "failed")
                .unwrap()["error"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        assert_eq!(store.token_accounting_version(SourceKind::Pi)?, Some(2));
        assert_eq!(
            store.open_connection()?.query_row(
                "SELECT total_tokens FROM usage_event WHERE event_key='old-pi'",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            12
        );
        let output = sync_failure_command(&root, &home, "codex")
            .args(["--emit-shards", "--since", "不是时间"])
            .output()?;
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            stderr.contains(language.select(
                "invalid --since RFC3339 timestamp",
                "--since 不是有效的 RFC3339 时间戳"
            )),
            "{stderr}"
        );
        if language == llmusage::common::i18n::Language::Chinese {
            assert!(
                !stderr.contains("invalid --since RFC3339 timestamp"),
                "{stderr}"
            );
        }
    }
    assert_eq!(canonical_errors[0], canonical_errors[1]);
    assert!(canonical_errors[0].starts_with("Refusing `--source omp`"));
    Ok(())
}

#[test]
fn bilingual_inventory_root_fault_is_localized_without_private_path_or_wire_changes() -> Result<()>
{
    let temp = TempDir::new()?;
    let home = temp.path().join("private-home");
    fs::create_dir_all(home.join("codex"))?;
    fs::write(home.join("codex/sessions"), b"regular file")?;
    let mut canonical_stats = Vec::new();
    for language in [
        llmusage::common::i18n::Language::English,
        llmusage::common::i18n::Language::Chinese,
    ] {
        let root = temp.path().join(language.code());
        llmusage::runtime::preferences::write(&root, language)?;
        let paths = AppPaths::with_root(root.clone())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        store.mark_current_token_accounting(SourceKind::Codex)?;
        let output = sync_failure_command(&root, &home, "codex").output()?;
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout)?;
        assert!(
            stdout
                .contains(language.select("Source root is not a directory", "来源根路径不是目录")),
            "{stdout}"
        );
        assert!(
            !stdout.contains("private-home") && !stdout.contains("source file inventory error"),
            "{stdout}"
        );
        let output = sync_failure_command(&root, &home, "codex")
            .arg("--json-events")
            .output()?;
        assert!(output.status.success());
        let values = String::from_utf8(output.stdout)?
            .lines()
            .map(serde_json::from_str::<serde_json::Value>)
            .collect::<serde_json::Result<Vec<_>>>()?;
        let stats = values
            .iter()
            .find(|value| value["event"] == "source_finished")
            .unwrap()["stats"]
            .clone();
        assert_eq!(
            stats["last_error"],
            "source file inventory error: root is not a directory"
        );
        canonical_stats.push(stats["last_error"].clone());
    }
    assert_eq!(canonical_stats[0], canonical_stats[1]);
    Ok(())
}
