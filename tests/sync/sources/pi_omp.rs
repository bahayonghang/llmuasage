use super::super::*;

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
        assert_eq!(stats.files_processed, 1);
        assert_eq!(stats.changed_files, 1);
        assert_eq!(stats.events_seen, 1);
        assert_eq!(stats.events_inserted, 1);
        assert_eq!(stats.stored_events, 1);
        assert_eq!(
            pi_event_rows(&app.paths.db_path)?,
            vec![("pi-future-model".to_string(), 11, 3, 2, 7, 5, 23)]
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
        assert_eq!(models, vec!["pi-future-model"]);
        assert_eq!(
            store.token_accounting_version(SourceKind::Pi)?,
            Some(expected_token_accounting_version(SourceKind::Pi))
        );
        assert_eq!(expected_token_accounting_version(SourceKind::Pi), 3);
        assert_eq!(expected_token_accounting_version(SourceKind::Omp), 2);
        assert!(!store.has_legacy_token_accounting(SourceKind::Pi)?);
        assert_eq!(
            llmusage::registry::source_descriptor(SourceKind::Pi)
                .expect("pi source descriptor")
                .display_name,
            "Pi"
        );
        assert_eq!(
            llmusage::registry::source_descriptor(SourceKind::Omp)
                .expect("omp source descriptor")
                .display_name,
            "Oh My Pi"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn omp_syncs_default_root_and_projects_status() -> Result<()> {
    let fixture = Fixture::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Omp),
            ..Default::default()
        };

        let empty = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(empty.sources[0].files_processed, 0);
        assert_eq!(
            source_capability_status(&app, &store, SourceKind::Omp)?,
            "passive_no_data"
        );

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
        assert_eq!(imported.sources[0].source, SourceKind::Omp);
        assert_eq!(imported.sources[0].files_processed, 1);
        assert_eq!(imported.sources[0].events_inserted, 1);
        assert_eq!(
            source_capability_status(&app, &store, SourceKind::Omp)?,
            "passive_ready"
        );

        let monitor = commands::source_status::build_platform_monitor_statuses()
            .into_iter()
            .find(|status| status.platform_id == "omp")
            .expect("omp platform monitor");
        assert_eq!(monitor.source, Some(SourceKind::Omp));
        assert_eq!(monitor.parser_status, "registered");
        assert_eq!(monitor.roots_checked, 1);
        assert_eq!(monitor.roots_detected, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn omp_stamps_provider_and_project_dimensions() -> Result<()> {
    let fixture = Fixture::new()?;
    let encoded = "--D--Documents-Code-CLI-llmusage--";
    let run_dir = "2026-08-22T16-26-20-289Z_aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let git_repo = fixture.home.join("workspace").join("llmusage");
    write_git_repo_with_url(&git_repo, "https://github.com/example/llmusage.git")?;
    let non_git = fixture
        .home
        .join(".omp")
        .join("agent")
        .join("sessions")
        .join("cwd-trap");
    fs::create_dir_all(&non_git)?;

    fixture.seed_omp_relative(
        Path::new(encoded).join("agent_git.jsonl"),
        &[
            pi_title_line(),
            pi_session_line("header-git-id", Some(&git_repo.to_string_lossy())),
            pi_assistant_line(
                "2026-06-10T00:00:00Z",
                "gpt-5.5",
                Some("openai-codex"),
                10,
                4,
            ),
        ],
    )?;
    fixture.seed_omp_relative(
        Path::new(encoded).join("agent_nongit.jsonl"),
        &[
            pi_session_line("header-nongit-id", Some(&non_git.to_string_lossy())),
            pi_assistant_line(
                "2026-06-10T00:01:00Z",
                "deepseek-v4-flash",
                Some("deepseek"),
                8,
                2,
            ),
        ],
    )?;
    fixture.seed_omp_relative(
        Path::new(encoded).join("agent_noheader.jsonl"),
        &[
            pi_title_line(),
            pi_assistant_line(
                "2026-06-10T00:02:00Z",
                "stealth/ox-alpha",
                Some("openrouter"),
                6,
                1,
            ),
        ],
    )?;
    fixture.seed_omp_relative(
        Path::new(encoded).join(run_dir).join("DiffJudge.jsonl"),
        &[
            pi_title_line(),
            pi_session_line("header-nested-id", None),
            pi_assistant_line("2026-06-10T00:03:00Z", "grok-4.6", Some("xai-oauth"), 5, 3),
        ],
    )?;
    fixture.seed_omp_relative(
        Path::new("agent_orphan.jsonl"),
        &[pi_assistant_line(
            "2026-06-10T00:04:00Z",
            "gpt-5.5",
            Some("openrouter"),
            3,
            1,
        )],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let imported = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Omp),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(imported.sources[0].events_inserted, 5);

        let rows = omp_dimension_rows(&app.paths.db_path)?;
        assert_eq!(rows.len(), 5);

        let git = row_by_session(&rows, "header-git-id");
        assert_eq!(git.provider_label, "openai-codex");
        assert_eq!(git.project_label.as_deref(), Some("example/llmusage"));
        assert_eq!(
            git.project_ref.as_deref(),
            Some("https://github.com/example/llmusage")
        );
        assert!(
            git.project_hash
                .as_ref()
                .is_some_and(|value| !value.is_empty())
        );

        let nongit = row_by_session(&rows, "header-nongit-id");
        assert_eq!(nongit.provider_label, "deepseek");
        assert_eq!(nongit.project_label.as_deref(), Some("llmusage"));
        assert!(nongit.project_ref.is_none());

        let noheader = row_by_session(&rows, "noheader");
        assert_eq!(noheader.provider_label, "openrouter");
        assert_eq!(noheader.project_label.as_deref(), Some("llmusage"));
        assert_eq!(noheader.session_label.as_deref(), Some("noheader"));

        let nested = row_by_session(&rows, "header-nested-id");
        assert_eq!(nested.provider_label, "xai-oauth");
        assert_eq!(nested.project_label.as_deref(), Some("llmusage"));
        assert_eq!(nested.session_label.as_deref(), Some("DiffJudge"));
        assert_eq!(nested.project_hash, noheader.project_hash);
        assert_ne!(
            nested.project_hash.as_deref().unwrap_or(""),
            llmusage::util::hash_string(run_dir)
        );

        let orphan = row_by_session(&rows, "orphan");
        assert_eq!(orphan.provider_label, "openrouter");
        assert!(orphan.project_hash.is_none());
        assert!(orphan.project_label.is_none());

        for row in &rows {
            for value in [
                row.session_id.as_str(),
                row.session_label.as_deref().unwrap_or(""),
                row.project_label.as_deref().unwrap_or(""),
                row.project_ref.as_deref().unwrap_or(""),
                row.project_hash.as_deref().unwrap_or(""),
            ] {
                assert!(
                    !value.contains("agent/sessions"),
                    "privacy: {value:?} must not store agent/sessions"
                );
            }
        }
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn omp_source_reported_cost_survives_recompute() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_omp(
        "project-cost",
        "costed",
        &[
            pi_message_line_with_cost(
                "2026-06-10T00:00:00Z",
                "deepseek-v4-flash",
                1_000,
                200,
                2_000,
                0,
                3_200,
                0,
                serde_json::json!({
                    "input": 0.01,
                    "output": 0.02,
                    "cacheRead": 0.002,
                    "cacheWrite": 0.0,
                    "total": 0.032
                }),
            ),
            pi_message_line_with_cost(
                "2026-06-10T00:01:00Z",
                "grok-4.6",
                10,
                5,
                0,
                0,
                15,
                0,
                serde_json::json!({
                    "input": 0.0,
                    "output": 0.0,
                    "cacheRead": 0.0,
                    "cacheWrite": 0.0,
                    "total": 0.0
                }),
            ),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let imported = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Omp),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(imported.sources[0].events_inserted, 2);

        let rows = omp_pricing_rows(&app.paths.db_path)?;
        assert_eq!(rows.len(), 2);
        let paid = rows
            .iter()
            .find(|row| row.model == "deepseek-v4-flash")
            .expect("paid omp event");
        assert_eq!(paid.pricing_status, "source_reported");
        assert_eq!(paid.pricing_source.as_deref(), Some("source-reported"));
        assert!((paid.cost_with_cache_usd - 0.032).abs() < 1e-12);
        assert!((paid.cost_without_cache_usd - 0.05).abs() < 1e-12);

        let free = rows
            .iter()
            .find(|row| row.model == "grok-4.6")
            .expect("zero-total omp event");
        assert_eq!(free.pricing_status, "unpriced");
        assert_eq!(free.cost_with_cache_usd, 0.0);

        let (bucket_cost, bucket_status): (f64, String) = {
            let conn = Connection::open(&app.paths.db_path)?;
            conn.query_row(
                r#"
                SELECT cost_with_cache_usd, pricing_status
                FROM usage_bucket_30m
                WHERE source = 'omp' AND model = 'deepseek-v4-flash'
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
        };
        assert_eq!(bucket_status, "source_reported");
        assert!((bucket_cost - paid.cost_with_cache_usd).abs() < 1e-12);

        let updated = store.recompute_costs()?;
        assert_eq!(updated, 1, "only the unpriced event is catalog-repriced");

        let after = omp_pricing_rows(&app.paths.db_path)?;
        let paid_after = after
            .iter()
            .find(|row| row.model == "deepseek-v4-flash")
            .expect("paid omp event after recompute");
        assert_eq!(paid_after.pricing_status, paid.pricing_status);
        assert_eq!(paid_after.pricing_source, paid.pricing_source);
        assert!((paid_after.cost_with_cache_usd - paid.cost_with_cache_usd).abs() < 1e-12);
        assert!((paid_after.cost_without_cache_usd - paid.cost_without_cache_usd).abs() < 1e-12);

        let (bucket_cost_after, bucket_status_after): (f64, String) = {
            let conn = Connection::open(&app.paths.db_path)?;
            conn.query_row(
                r#"
                SELECT cost_with_cache_usd, pricing_status
                FROM usage_bucket_30m
                WHERE source = 'omp' AND model = 'deepseek-v4-flash'
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
        };
        assert_eq!(bucket_status_after, "source_reported");
        assert!((bucket_cost_after - bucket_cost).abs() < 1e-12);
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
            source: Some(SourceKind::Omp),
            ..Default::default()
        };

        let first = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(first.sources[0].events_inserted, 2);
        assert_eq!(omp_event_count(&app.paths.db_path)?, 2);

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
        assert_eq!(omp_event_count(&app.paths.db_path)?, 3);

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
        assert_eq!(omp_event_count(&app.paths.db_path)?, 1);
        assert_eq!(
            omp_event_rows(&app.paths.db_path)?,
            vec![("gpt-6-rewrite".to_string(), 42, 0, 0, 8, 6, 50)]
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn omp_behavior_facts_persist_turns_and_tool_calls() -> Result<()> {
    let fixture = Fixture::new()?;
    let secret = "SECRET_TOOL_RESULT_CONTENT";
    let long_command = "x".repeat(200);
    fixture.seed_omp(
        "project-behavior",
        "behavior",
        &[
            pi_session_line("behavior-id", None),
            pi_assistant_behavior_line(
                "2026-08-20T00:00:00Z",
                "gpt-5.5",
                Some("openrouter"),
                vec![
                    pi_tool_call("bash", serde_json::json!({ "command": long_command })),
                    pi_tool_call("read", serde_json::json!({ "file_path": "src/lib.rs" })),
                    pi_tool_call("write", serde_json::json!({ "file_path": "src/main.rs" })),
                ],
                None,
            ),
            pi_tool_result_line(secret),
            pi_assistant_behavior_line(
                "2026-08-20T00:01:00Z",
                "gpt-5.5",
                Some("openrouter"),
                vec![pi_tool_call(
                    "edit",
                    serde_json::json!({ "file_path": "src/lib.rs" }),
                )],
                Some(1),
            ),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let imported = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Omp),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(imported.sources[0].events_inserted, 2);
        assert_eq!(omp_event_count(&app.paths.db_path)?, 2);
        assert_eq!(omp_turn_count(&app.paths.db_path)?, 2);
        assert_eq!(omp_tool_call_count(&app.paths.db_path)?, 4);
        assert_eq!(omp_orphan_turns(&app.paths.db_path)?, 0);
        assert_eq!(omp_orphan_tool_calls(&app.paths.db_path)?, 0);
        assert_eq!(
            omp_tool_kind_counts(&app.paths.db_path)?,
            vec![
                ("bash".to_string(), 1),
                ("edit".to_string(), 2),
                ("read".to_string(), 1),
            ]
        );
        assert_eq!(
            omp_turn_retry_rows(&app.paths.db_path)?,
            vec![(0, 1), (1, 0)]
        );
        assert_eq!(omp_empty_turn_project_hash_count(&app.paths.db_path)?, 0);
        assert_eq!(
            omp_empty_tool_call_project_hash_count(&app.paths.db_path)?,
            0
        );
        assert_eq!(omp_overlong_preview_count(&app.paths.db_path)?, 0);
        assert_eq!(omp_preview_secret_count(&app.paths.db_path, secret)?, 0);

        let dashboard = Dashboard::open(&store)?;
        let filter = QueryFilter {
            source: Some(SourceKind::Omp),
            ..Default::default()
        };
        let activity = dashboard.activity_breakdown(&filter)?;
        assert!(activity.support.supported);
        assert!(!activity.breakdown.is_empty());
        let tools = dashboard.tool_breakdown(&filter)?;
        assert!(tools.support.supported);
        assert!(!tools.breakdown.is_empty());
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn omp_sensitive_tool_values_do_not_persist_in_safe_preview() -> Result<()> {
    let fixture = Fixture::new()?;
    let sentinels = [
        "/private/unix/secret.rs",
        r"C:\Users\secret\private.rs",
        "powershell Get-Secret",
        "curl https://secret.invalid/token",
    ];
    fixture.seed_omp(
        "project-sensitive-preview",
        "sensitive-preview",
        &[pi_assistant_behavior_line(
            &chrono::Utc::now().to_rfc3339(),
            "gpt-5.5",
            Some("openrouter"),
            vec![
                pi_tool_call("read", serde_json::json!({ "file_path": sentinels[0] })),
                pi_tool_call("read", serde_json::json!({ "path": sentinels[1] })),
                pi_tool_call("shell", serde_json::json!({ "cmd": sentinels[2] })),
                pi_tool_call("bash", serde_json::json!({ "command": sentinels[3] })),
            ],
            None,
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
                source: Some(SourceKind::Omp),
                ..Default::default()
            },
            None,
        )
        .await?;

        let conn = Connection::open(&app.paths.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT COALESCE(safe_preview, ''), COALESCE(input_fingerprint, '') \
             FROM usage_tool_call WHERE source = 'omp' ORDER BY tool_call_key",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert_eq!(rows.len(), 4);
        assert!(
            rows.iter().all(|(preview, _)| preview.is_empty()),
            "sensitive-only inputs must persist no partial safe_preview: {rows:?}"
        );
        assert!(rows.iter().all(|(_, fingerprint)| !fingerprint.is_empty()));
        for sentinel in sentinels {
            assert!(
                rows.iter().all(|(preview, _)| !preview.contains(sentinel)),
                "raw sentinel persisted in safe_preview: {sentinel}"
            );
        }
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn omp_rewrite_clears_old_path_hash_behavior_facts() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_omp(
        "project-replay",
        "replay",
        &[pi_assistant_behavior_line(
            "2026-08-20T00:00:00Z",
            "gpt-5.5",
            Some("openrouter"),
            vec![
                pi_tool_call("bash", serde_json::json!({ "command": "echo one" })),
                pi_tool_call("read", serde_json::json!({ "file_path": "old.rs" })),
            ],
            None,
        )],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Omp),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(omp_turn_count(&app.paths.db_path)?, 1);
        assert_eq!(omp_tool_call_count(&app.paths.db_path)?, 2);

        fixture.seed_omp(
            "project-replay",
            "replay",
            &[pi_assistant_behavior_line(
                "2026-08-20T01:00:00Z",
                "gpt-5.5",
                Some("openrouter"),
                vec![pi_tool_call(
                    "grep",
                    serde_json::json!({ "pattern": "fn main" }),
                )],
                None,
            )],
        )?;
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(omp_event_count(&app.paths.db_path)?, 1);
        assert_eq!(omp_turn_count(&app.paths.db_path)?, 1);
        assert_eq!(omp_tool_call_count(&app.paths.db_path)?, 1);
        assert_eq!(
            omp_tool_kind_counts(&app.paths.db_path)?,
            vec![("search".to_string(), 1)]
        );
        assert_eq!(omp_orphan_turns(&app.paths.db_path)?, 0);
        assert_eq!(omp_orphan_tool_calls(&app.paths.db_path)?, 0);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn omp_recent_cutoff_does_not_write_orphan_behavior_facts() -> Result<()> {
    let fixture = Fixture::new()?;
    let now = chrono::Utc::now().to_rfc3339();
    fixture.seed_omp(
        "project-recent",
        "recent",
        &[
            pi_assistant_behavior_line(
                "2020-01-01T00:00:00Z",
                "gpt-5.5",
                Some("openrouter"),
                vec![pi_tool_call(
                    "read",
                    serde_json::json!({ "file_path": "old.rs" }),
                )],
                None,
            ),
            pi_assistant_behavior_line(
                &now,
                "gpt-5.5",
                Some("openrouter"),
                vec![pi_tool_call(
                    "write",
                    serde_json::json!({ "file_path": "new.rs" }),
                )],
                None,
            ),
        ],
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let bounded = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Omp),
                recent_days: Some(1),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(bounded.sources[0].events_inserted, 1);
        assert_eq!(omp_event_count(&app.paths.db_path)?, 1);
        assert_eq!(omp_turn_count(&app.paths.db_path)?, 1);
        assert_eq!(omp_tool_call_count(&app.paths.db_path)?, 1);
        assert_eq!(omp_orphan_turns(&app.paths.db_path)?, 0);
        assert_eq!(omp_orphan_tool_calls(&app.paths.db_path)?, 0);
        assert_eq!(
            omp_tool_kind_counts(&app.paths.db_path)?,
            vec![("edit".to_string(), 1)]
        );
        assert!(
            store
                .cursors()
                .load_file_cursors(SourceKind::Omp, "local")?
                .is_empty(),
            "bounded OMP sync must not advance the full-history cursor"
        );

        let full = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Omp),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &full, None).await?;
        assert_eq!(omp_event_count(&app.paths.db_path)?, 2);
        assert_eq!(omp_turn_count(&app.paths.db_path)?, 2);
        assert_eq!(omp_tool_call_count(&app.paths.db_path)?, 2);
        assert_eq!(
            omp_tool_kind_counts(&app.paths.db_path)?,
            vec![("edit".to_string(), 1), ("read".to_string(), 1)]
        );
        assert_eq!(omp_orphan_turns(&app.paths.db_path)?, 0);
        assert_eq!(omp_orphan_tool_calls(&app.paths.db_path)?, 0);
        assert_eq!(
            store
                .cursors()
                .load_file_cursors(SourceKind::Omp, "local")?
                .len(),
            1
        );

        commands::sync::run_once_with_options(&app, &store, 0, &full, None).await?;
        assert_eq!(omp_event_count(&app.paths.db_path)?, 2);
        assert_eq!(omp_turn_count(&app.paths.db_path)?, 2);
        assert_eq!(omp_tool_call_count(&app.paths.db_path)?, 2);
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
fn pi_token_accounting_bump_replays_omp_identity_set() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.seed_omp(
        "project-omp",
        "omp-first",
        &[pi_message_line(
            "2026-06-01T00:05:00Z",
            "gpt-5.5",
            100,
            50,
            40,
            8,
            333,
            10,
        )],
    )?;
    let canonical = fs::canonicalize(&path)?;
    let path_hash = hash_string(&canonical.to_string_lossy());
    let event_at = chrono::DateTime::parse_from_rfc3339("2026-06-01T00:05:00Z")?
        .with_timezone(&chrono::Utc)
        .to_rfc3339();
    let hour_start =
        llmusage::util::bucket_start_from_rfc3339(&event_at).unwrap_or_else(|| event_at.clone());

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        {
            let lock = store.acquire_worker_lock_with(Duration::from_secs(5), HolderKind::Cli)?;
            let fenced = lock.fenced_store();
            fenced.bootstrap()?;
            let mut shard = SyncShard::new(SourceKind::Pi);
            shard.events.push(UsageEvent {
                event_key: "pi:legacy-split".to_string(),
                source: SourceKind::Pi,
                provider_label: String::new(),
                model: "gpt-5.5".to_string(),
                event_at: event_at.clone(),
                hour_start,
                tokens: UsageTokens {
                    input_tokens: 100,
                    cache_read_tokens: 40,
                    cache_creation_tokens: 8,
                    output_tokens: 50,
                    reasoning_output_tokens: 10,
                    total_tokens: 333,
                },
                project: None,
                session: Some(SessionInfo {
                    session_id: "omp-first".to_string(),
                    session_label: Some("omp-first".to_string()),
                    source_path_hash: Some(path_hash.clone()),
                }),
                source_cost: None,
            });
            shard
                .seen_file_paths
                .push(canonical.to_string_lossy().to_string());
            let mut writer = fenced.begin_sync_run()?;
            writer.commit_shard(shard)?;
            writer.finish_sync_run()?;
            fenced.set_meta_value("token_accounting_version.pi", "2")?;
        }

        let baseline = identity_rows(&app.paths.db_path, "pi")?;
        assert_eq!(baseline.len(), 1);
        assert!(store.has_legacy_token_accounting(SourceKind::Pi)?);

        let summary = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                rebuild: true,
                ..Default::default()
            },
            None,
        )
        .await?;
        assert!(
            summary
                .sources
                .iter()
                .any(|stats| stats.source == SourceKind::Omp && stats.events_inserted == 1),
            "{:?}",
            summary.sources
        );
        assert_eq!(pi_event_count(&app.paths.db_path)?, 0);
        let migrated = identity_rows(&app.paths.db_path, "omp")?;
        assert_eq!(migrated, baseline);
        assert_eq!(store.token_accounting_version(SourceKind::Pi)?, Some(3));
        assert!(!store.has_legacy_token_accounting(SourceKind::Pi)?);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn omp_source_sync_refuses_until_pi_split_migration() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.seed_omp(
        "project-omp",
        "omp-gate",
        &[pi_message_line(
            "2026-06-01T00:05:00Z",
            "gpt-5.5",
            100,
            50,
            40,
            8,
            333,
            10,
        )],
    )?;
    let canonical = fs::canonicalize(&path)?;
    let event_at = chrono::DateTime::parse_from_rfc3339("2026-06-01T00:05:00Z")?
        .with_timezone(&chrono::Utc)
        .to_rfc3339();
    let hour_start =
        llmusage::util::bucket_start_from_rfc3339(&event_at).unwrap_or_else(|| event_at.clone());

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        {
            let lock = store.acquire_worker_lock_with(Duration::from_secs(5), HolderKind::Cli)?;
            let fenced = lock.fenced_store();
            fenced.bootstrap()?;
            let mut shard = SyncShard::new(SourceKind::Pi);
            shard.events.push(UsageEvent {
                event_key: "pi:legacy-gate".to_string(),
                source: SourceKind::Pi,
                provider_label: String::new(),
                model: "gpt-5.5".to_string(),
                event_at,
                hour_start,
                tokens: UsageTokens {
                    input_tokens: 100,
                    cache_read_tokens: 40,
                    cache_creation_tokens: 8,
                    output_tokens: 50,
                    reasoning_output_tokens: 10,
                    total_tokens: 333,
                },
                project: None,
                session: Some(SessionInfo {
                    session_id: "omp-gate".to_string(),
                    session_label: Some("omp-gate".to_string()),
                    source_path_hash: Some(hash_string(&canonical.to_string_lossy())),
                }),
                source_cost: None,
            });
            let mut writer = fenced.begin_sync_run()?;
            writer.commit_shard(shard)?;
            writer.finish_sync_run()?;
            fenced.set_meta_value("token_accounting_version.pi", "2")?;
        }

        let err = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Omp),
                ..Default::default()
            },
            None,
        )
        .await
        .expect_err("omp-only sync must refuse pre-split pi rows");
        assert!(
            err.to_string().contains("pre-split token-accounting"),
            "{err}"
        );
        assert!(
            err.to_string()
                .contains("llmusage sync --rebuild --source pi"),
            "{err}"
        );
        assert!(!err.to_string().contains("no `--source`"), "{err}");
        assert_eq!(omp_event_count(&app.paths.db_path)?, 0);
        assert_eq!(pi_event_count(&app.paths.db_path)?, 1);

        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                rebuild: true,
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(pi_event_count(&app.paths.db_path)?, 0);
        assert_eq!(omp_event_count(&app.paths.db_path)?, 1);

        let retry = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Omp),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(retry.sources[0].source, SourceKind::Omp);
        assert_eq!(omp_event_count(&app.paths.db_path)?, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}
