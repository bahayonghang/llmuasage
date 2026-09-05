use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use llmusage::{app::AppContext, commands, integrations, query::Dashboard, store::Store, web};
use rusqlite::Connection;
use tempfile::TempDir;

#[test]
fn local_flow_bootstraps_and_syncs_without_installing_integrations() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex()?;
    fixture.seed_claude()?;
    fixture.seed_opencode()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;

        commands::init::run(&app).await?;
        assert!(app.paths.db_path.is_file());
        assert!(!app.paths.hook_cmd_path.exists());
        assert!(!app.paths.hook_sh_path.exists());
        assert_eq!(
            fs::read_to_string(fixture.codex_home.join("config.toml"))?,
            "notify = [\"echo\", \"hello\"]\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.home.join(".claude").join("settings.json"))?,
            "{}"
        );
        assert!(
            !fixture
                .opencode_config
                .join("plugin")
                .join("llmusage-tracker.js")
                .exists()
        );

        commands::sync::run(&app).await?;
        let store = Store::new(&app.paths)?;
        let dashboard = Dashboard::open(&store)?;
        let overview = dashboard.overview(&Default::default())?;
        assert_eq!(overview.source_count, 3);
        assert!(overview.total.total_tokens >= 344);

        let projects = dashboard.project_breakdown(&Default::default())?;
        assert!(!projects.is_empty());

        let html_out = fixture.root.path().join("html-out");
        commands::export::run_html(&app, Some(html_out.clone())).await?;
        assert!(html_out.join("index.html").is_file());
        assert!(html_out.join("snapshot.json").is_file());
        assert!(html_out.join("assets").join("base.css").is_file());
        assert!(html_out.join("assets").join("layout.css").is_file());
        assert!(html_out.join("assets").join("components.css").is_file());
        assert!(html_out.join("assets").join("charts.css").is_file());
        assert!(html_out.join("assets").join("app.js").is_file());
        assert!(html_out.join("assets").join("copy.js").is_file());
        assert!(html_out.join("assets").join("data.js").is_file());
        assert!(
            html_out
                .join("assets")
                .join("data")
                .join("fetch.js")
                .is_file()
        );
        assert!(
            html_out
                .join("assets")
                .join("data")
                .join("format.js")
                .is_file()
        );
        assert!(
            html_out
                .join("assets")
                .join("data")
                .join("derive.js")
                .is_file()
        );
        assert!(
            html_out
                .join("assets")
                .join("render")
                .join("hero.js")
                .is_file()
        );
        assert!(
            html_out
                .join("assets")
                .join("render")
                .join("explorer.js")
                .is_file()
        );
        for logo in [
            "codex.svg",
            "claude.svg",
            "opencode.svg",
            "antigravity.svg",
            "kimi_code.svg",
            "pi.svg",
            "omp.svg",
            "grok.svg",
            "zcode.svg",
            "deepseek_harness.svg",
            "fallback.svg",
        ] {
            assert!(
                html_out
                    .join("assets")
                    .join("agent-logos")
                    .join(logo)
                    .is_file(),
                "missing exported Agent logo: {logo}"
            );
        }
        let exported_index = fs::read_to_string(html_out.join("index.html"))?;
        assert!(exported_index.contains("data-mode=\"snapshot\""));
        assert!(exported_index.contains("id=\"source-badge-catalog\""));
        assert!(exported_index.contains("type=\"module\""));
        assert!(exported_index.contains("assets/app.js"));
        assert!(exported_index.contains("assets/base.css"));
        assert!(exported_index.contains("assets/layout.css"));
        assert!(exported_index.contains("assets/components.css"));
        assert!(exported_index.contains("assets/charts.css"));
        assert!(exported_index.contains("<title>llmusage · 本地用量概览</title>"));
        assert!(exported_index.contains(">本地用量概览</strong>"));
        assert!(exported_index.contains("用量趋势"));
        assert!(exported_index.contains("用量分析"));
        let snapshot_json = fs::read_to_string(html_out.join("snapshot.json"))?;
        let snapshot: serde_json::Value = serde_json::from_str(&snapshot_json)?;
        assert!(snapshot["explorer"].is_object());
        assert!(!exported_index.contains("llmusage 本地账本"));
        assert!(web::live_index_html().contains("data-mode=\"live\""));
        assert!(web::snapshot_index_html().contains("data-mode=\"snapshot\""));
        assert!(web::live_index_html().contains("type=\"module\""));

        commands::diagnostics::run(
            &app,
            Some(fixture.root.path().join("diagnostics.json")),
            None,
            None,
        )
        .await?;
        commands::doctor::run(&app, true, None).await?;

        let historical_backup = app.paths.backups_dir.join("historical.bak");
        fs::write(&historical_backup, "keep")?;
        commands::uninstall::run(&app, false).await?;
        commands::uninstall::run(&app, false).await?;
        assert_eq!(fs::read_to_string(historical_backup)?, "keep");
        assert!(
            store
                .integration_state()
                .load_integration_states()?
                .is_empty()
        );

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn legacy_cleanup_handles_all_owned_artifacts_and_is_idempotent() -> Result<()> {
    let fixture = Fixture::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::init::run(&app).await?;

        let claude_path = fixture.home.join(".claude").join("settings.json");
        let user_claude = serde_json::json!({ "type": "command", "command": "notify-user" });
        fs::write(
            &claude_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "hooks": {
                    "Stop": [
                        { "hooks": [
                            user_claude.clone(),
                            { "type": "command", "command": "cmd /c \"C:\\\\old\\\\llmusage-hook.cmd --source claude\"" }
                        ] },
                        { "hooks": [
                            { "type": "command", "command": "cmd /c \"\"C:\\\\new\\\\llmusage-hook.cmd\" --source claude\"" }
                        ] }
                    ],
                    "SessionEnd": [
                        { "hooks": [{ "type": "command", "command": "/tmp/llmusage-hook --source claude" }] }
                    ]
                },
                "user": { "value": 7 }
            }))?,
        )?;

        fs::write(
            fixture.codex_home.join("config.toml"),
            "model = \"gpt-5\"\nnotify = [\"cmd\", \"/c\", \"llmusage-hook.cmd\"]\n",
        )?;
        fs::write(
            app.paths.backups_dir.join("codex_notify_original.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "notify": ["echo", "user-notify"]
            }))?,
        )?;

        let antigravity_path = fixture
            .home
            .join(".gemini")
            .join("config")
            .join("hooks.json");
        fs::create_dir_all(antigravity_path.parent().expect("parent"))?;
        let antigravity_seed = serde_json::to_vec_pretty(&serde_json::json!({
            "Stop": [
                { "type": "command", "command": "notify-antigravity-user" },
                { "type": "command", "command": "llmusage-hook --source antigravity" },
                { "type": "command", "command": "llmusage-hook --source gemini" }
            ]
        }))?;
        fs::write(&antigravity_path, &antigravity_seed)?;
        fs::write(
            fixture.home.join(".gemini").join("settings.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "hooks": {
                    "SessionEnd": [{ "hooks": [
                        { "type": "command", "command": "notify-gemini-user" },
                        { "type": "command", "command": "llmusage-hook --source gemini" }
                    ] }]
                }
            }))?,
        )?;

        let plugin_path = fixture
            .opencode_config
            .join("plugin")
            .join("llmusage-tracker.js");
        fs::write(&plugin_path, "// LLMUSAGE_LOCAL_PLUGIN\nexport default {};\n")?;
        fs::create_dir_all(&app.paths.bin_dir)?;
        fs::write(&app.paths.hook_cmd_path, "legacy")?;
        fs::write(&app.paths.hook_sh_path, "legacy")?;

        let antigravity_name = antigravity_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("file name");
        let antigravity_dir = antigravity_path.parent().expect("parent");
        fs::write(
            antigravity_dir.join(format!(".{antigravity_name}.llmusage-pending")),
            "present\n",
        )?;
        fs::write(
            antigravity_dir.join(format!(".{antigravity_name}.llmusage-recovery")),
            &antigravity_seed,
        )?;
        fs::write(
            antigravity_dir.join(format!(".{antigravity_name}.llmusage-tmp.1.2.3")),
            "stale",
        )?;

        let historical_backup = app.paths.backups_dir.join("keep-history.bak");
        let database_backup = app.paths.backups_dir.join("llmusage.db.pre-0.5.0");
        fs::write(&historical_backup, "keep")?;
        fs::write(&database_backup, "keep-db")?;

        commands::uninstall::run(&app, false).await?;

        let claude: serde_json::Value = serde_json::from_slice(&fs::read(&claude_path)?)?;
        assert_eq!(claude["hooks"]["Stop"], serde_json::json!([{ "hooks": [user_claude] }]));
        assert_eq!(claude["hooks"]["SessionEnd"], serde_json::json!([]));
        assert_eq!(claude["user"], serde_json::json!({ "value": 7 }));

        let codex = fs::read_to_string(fixture.codex_home.join("config.toml"))?;
        assert!(codex.contains("model = \"gpt-5\""));
        assert!(codex.contains("notify = [\"echo\", \"user-notify\"]"));
        assert!(!app.paths.backups_dir.join("codex_notify_original.json").exists());

        let antigravity: serde_json::Value =
            serde_json::from_slice(&fs::read(&antigravity_path)?)?;
        assert_eq!(
            antigravity["Stop"],
            serde_json::json!([{ "type": "command", "command": "notify-antigravity-user" }])
        );
        let gemini: serde_json::Value = serde_json::from_slice(&fs::read(
            fixture.home.join(".gemini").join("settings.json"),
        )?)?;
        assert_eq!(
            gemini["hooks"]["SessionEnd"][0]["hooks"],
            serde_json::json!([{ "type": "command", "command": "notify-gemini-user" }])
        );
        assert!(!plugin_path.exists());
        assert!(!app.paths.hook_cmd_path.exists());
        assert!(!app.paths.hook_sh_path.exists());
        assert_eq!(fs::read_to_string(&historical_backup)?, "keep");
        assert_eq!(fs::read_to_string(&database_backup)?, "keep-db");
        assert!(
            fs::read_dir(antigravity_dir)?
                .filter_map(|entry| entry.ok())
                .all(|entry| !entry.file_name().to_string_lossy().contains("llmusage-"))
        );

        fs::write(&plugin_path, "// user-owned plugin\n")?;
        let plugin_name = plugin_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("plugin file name");
        let plugin_residue = plugin_path
            .parent()
            .expect("plugin parent")
            .join(format!(".{plugin_name}.llmusage-tmp.4.5.6"));
        fs::write(&plugin_residue, "stale")?;
        let store = Store::new(&app.paths)?;
        let before_rows: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM integration_install",
            [],
            |row| row.get(0),
        )?;
        let before_opencode_state = store
            .integration_state()
            .load_integration_states()?
            .into_iter()
            .find(|state| state.source == "opencode")
            .expect("OpenCode cleanup audit state");
        assert!(
            store
                .integration_state()
                .load_integration_states()?
                .iter()
                .any(|state| state.source == "legacy_hook_wrappers")
        );
        let before_backups = fs::read_dir(&app.paths.backups_dir)?.count();
        let claude_before = fs::read(&claude_path)?;
        let codex_before = fs::read(fixture.codex_home.join("config.toml"))?;
        let antigravity_before = fs::read(&antigravity_path)?;
        let gemini_before = fs::read(fixture.home.join(".gemini").join("settings.json"))?;

        commands::uninstall::run(&app, false).await?;

        let after_rows: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM integration_install",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(after_rows, before_rows);
        let after_opencode_state = store
            .integration_state()
            .load_integration_states()?
            .into_iter()
            .find(|state| state.source == "opencode")
            .expect("updated OpenCode cleanup audit state");
        assert_ne!(
            after_opencode_state.details_json,
            before_opencode_state.details_json
        );
        assert!(
            after_opencode_state
                .details_json
                .as_deref()
                .is_some_and(|detail| detail.contains("recovered residue"))
        );
        let after_opencode_state_json = serde_json::to_value(&after_opencode_state)?;
        assert!(!plugin_residue.exists());
        assert_eq!(fs::read_dir(&app.paths.backups_dir)?.count(), before_backups);
        assert_eq!(fs::read(&claude_path)?, claude_before);
        assert_eq!(fs::read(fixture.codex_home.join("config.toml"))?, codex_before);
        assert_eq!(fs::read(&antigravity_path)?, antigravity_before);
        assert_eq!(
            fs::read(fixture.home.join(".gemini").join("settings.json"))?,
            gemini_before
        );
        assert_eq!(fs::read_to_string(&plugin_path)?, "// user-owned plugin\n");
        let before_noop_states =
            serde_json::to_value(store.integration_state().load_integration_states()?)?;

        commands::uninstall::run(&app, false).await?;
        let final_rows: i64 = store.open_connection()?.query_row(
            "SELECT COUNT(*) FROM integration_install",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(final_rows, after_rows);
        let final_opencode_state = store
            .integration_state()
            .load_integration_states()?
            .into_iter()
            .find(|state| state.source == "opencode")
            .expect("final OpenCode cleanup audit state");
        assert_eq!(
            serde_json::to_value(final_opencode_state)?,
            after_opencode_state_json
        );
        assert_eq!(fs::read_dir(&app.paths.backups_dir)?.count(), before_backups);
        assert_eq!(fs::read_to_string(plugin_path)?, "// user-owned plugin\n");
        assert_eq!(
            serde_json::to_value(store.integration_state().load_integration_states()?)?,
            before_noop_states
        );

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn wrapper_only_cleanup_is_audited_once_and_then_becomes_a_noop() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = AppContext::discover()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    fs::write(&app.paths.hook_cmd_path, "legacy wrapper")?;

    integrations::cleanup_all(&app, &store)?;
    assert!(!app.paths.hook_cmd_path.exists());
    let after_cleanup = store.integration_state().load_integration_states()?;
    assert_eq!(after_cleanup.len(), 1);
    assert_eq!(after_cleanup[0].source, "legacy_hook_wrappers");
    assert_eq!(after_cleanup[0].status, "restored");
    let backup_count = fs::read_dir(&app.paths.backups_dir)?.count();

    integrations::cleanup_all(&app, &store)?;
    assert_eq!(
        serde_json::to_value(store.integration_state().load_integration_states()?)?,
        serde_json::to_value(after_cleanup)?
    );
    assert_eq!(fs::read_dir(&app.paths.backups_dir)?.count(), backup_count);

    fixture.restore_env();
    Ok(())
}

#[test]
fn cleanup_continues_after_one_integration_fails() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = AppContext::discover()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;

    fs::write(fixture.home.join(".claude").join("settings.json"), "{")?;
    let plugin_path = fixture
        .opencode_config
        .join("plugin")
        .join("llmusage-tracker.js");
    fs::write(&plugin_path, "// LLMUSAGE_LOCAL_PLUGIN\n")?;
    let antigravity_path = fixture
        .home
        .join(".gemini")
        .join("config")
        .join("hooks.json");
    fs::create_dir_all(antigravity_path.parent().expect("parent"))?;
    fs::write(
        &antigravity_path,
        serde_json::to_vec(&serde_json::json!({
            "Stop": [
                { "type": "command", "command": "llmusage-hook --source antigravity" },
                { "type": "command", "command": "notify-user" }
            ]
        }))?,
    )?;
    fs::write(&app.paths.hook_sh_path, "legacy wrapper")?;

    let error = integrations::cleanup_all(&app, &store)
        .expect_err("invalid Claude settings should fail the aggregate cleanup");
    assert!(error.to_string().contains("claude"));
    assert!(!plugin_path.exists());
    assert!(!app.paths.hook_sh_path.exists());
    let antigravity: serde_json::Value = serde_json::from_slice(&fs::read(antigravity_path)?)?;
    assert_eq!(
        antigravity["Stop"],
        serde_json::json!([{ "type": "command", "command": "notify-user" }])
    );
    let states = store.integration_state().load_integration_states()?;
    assert!(
        states
            .iter()
            .any(|state| state.source == "claude" && state.status == "error")
    );
    assert!(states.iter().any(|state| state.source == "opencode"));
    assert!(states.iter().any(|state| state.source == "antigravity"));
    assert!(
        states
            .iter()
            .any(|state| state.source == "legacy_hook_wrappers")
    );

    fixture.restore_env();
    Ok(())
}

#[test]
fn sync_prices_claude_fable_and_mythos_usage() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_claude_fable_mythos()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;

        commands::sync::run(&app).await?;

        let store = Store::new(&app.paths)?;
        let dashboard = Dashboard::open(&store)?;
        let models = dashboard.model_breakdown(&Default::default())?;
        for (model, expected_cost) in [
            ("claude-fable-5", 33.95),
            ("claude-mythos-5", 33.95),
            ("claude-fable-5-1", 33.80),
            ("claude-mythos-5-1", 33.80),
        ] {
            let row = models
                .iter()
                .find(|item| item.model == model)
                .unwrap_or_else(|| panic!("{model} should be present in model breakdown"));
            assert_eq!(row.pricing_status, "static", "{model}");
            assert_eq!(row.pricing_source.as_deref(), Some("static-v3"), "{model}");
            assert!(
                (row.cost_with_cache_usd - expected_cost).abs() < 1e-9,
                "{model} cost should use Fable/Mythos embedded rates"
            );
        }
        assert!(
            models
                .iter()
                .filter(|item| matches!(
                    item.model.as_str(),
                    "claude-fable-5" | "claude-mythos-5" | "claude-fable-5-1" | "claude-mythos-5-1"
                ))
                .all(|item| item.pricing_status != "unpriced")
        );

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn sync_prices_gpt_5_6_per_request_for_codex_and_opencode() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_gpt_5_6()?;
    fixture.seed_opencode_gpt_5_6()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::sync::run(&app).await?;

        let store = Store::new(&app.paths)?;
        let conn = store.open_connection()?;
        let rows = {
            let mut stmt = conn.prepare(
                r#"
                SELECT source, model, pricing_status, pricing_source, pricing_rate,
                       cost_with_cache_usd
                FROM usage_event
                WHERE model LIKE 'gpt-5.6%'
                ORDER BY source, model, event_at
                "#,
            )?;
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, f64>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        assert_eq!(rows.len(), 4);
        assert!(
            rows.iter()
                .all(|row| row.2 == "static" && row.3 == "static-v3")
        );
        assert_eq!(rows[0].0, "codex");
        assert_eq!(rows[0].1, "gpt-5.6-luna");
        assert!(rows[0].4.contains("\"tier\":\"default\""));
        assert!((rows[0].5 - 0.7).abs() < 1e-9);
        assert_eq!(rows[1].1, "gpt-5.6-luna");
        assert!(rows[1].4.contains("\"tier\":\"default\""));
        assert!((rows[1].5 - 0.70000125).abs() < 1e-9);
        assert_eq!(rows[2].0, "opencode");
        assert_eq!(rows[2].1, "gpt-5.6-sol");
        assert!(rows[2].4.contains("\"tier\":\"long_context\""));
        assert!((rows[2].5 - 6.5000125).abs() < 1e-9);
        assert_eq!(rows[3].1, "gpt-5.6-terra");
        assert!(rows[3].4.contains("\"tier\":\"default\""));
        assert!((rows[3].5 - 2.0).abs() < 1e-9);

        let (bucket_cost, bucket_rate): (f64, String) = conn.query_row(
            r#"
            SELECT cost_with_cache_usd, pricing_rate
            FROM usage_bucket_30m
            WHERE source = 'codex' AND model = 'gpt-5.6-luna'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert!((bucket_cost - 1.40000125).abs() < 1e-9);
        assert_eq!(bucket_rate, "mixed");
        drop(conn);

        let dashboard = Dashboard::open(&store)?;
        let pressure = dashboard.context_pressure(&Default::default())?;
        assert_eq!(pressure.priced_events, 4);
        assert_eq!(pressure.unpriced_events, 0);
        assert!((pressure.peak_percent - (272_001.0 / 1_050_000.0)).abs() < 1e-9);
        assert!(
            pressure
                .peak_model
                .as_deref()
                .is_some_and(|model| model.contains("gpt-5.6"))
        );

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn sync_prices_gpt_6_astra_per_request_for_codex_and_opencode() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_gpt_6_astra()?;
    fixture.seed_opencode_gpt_6_astra()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::sync::run(&app).await?;

        let store = Store::new(&app.paths)?;
        let conn = store.open_connection()?;
        let rows = {
            let mut stmt = conn.prepare(
                r#"
                SELECT source, model, pricing_status, pricing_source, pricing_rate,
                       cost_with_cache_usd
                FROM usage_event
                WHERE model = 'gpt-6-astra'
                ORDER BY source, event_at
                "#,
            )?;
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, f64>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        assert_eq!(rows.len(), 4);
        assert!(
            rows.iter()
                .all(|row| row.1 == "gpt-6-astra" && row.2 == "static" && row.3 == "static-v3")
        );
        assert_eq!(rows[0].0, "codex");
        assert!(rows[0].4.contains("\"tier\":\"default\""));
        assert!((rows[0].5 - 7.0).abs() < 1e-9);
        assert!(rows[1].4.contains("\"tier\":\"long_context\""));
        assert!((rows[1].5 - 11.500025).abs() < 1e-9);
        assert_eq!(rows[2].0, "opencode");
        assert!(rows[2].4.contains("\"tier\":\"default\""));
        assert!((rows[2].5 - 7.0).abs() < 1e-9);
        assert!(rows[3].4.contains("\"tier\":\"long_context\""));
        assert!((rows[3].5 - 11.500025).abs() < 1e-9);

        drop(conn);

        let dashboard = Dashboard::open(&store)?;
        let pressure = dashboard.context_pressure(&Default::default())?;
        assert_eq!(pressure.priced_events, 4);
        assert_eq!(pressure.unpriced_events, 0);
        assert!((pressure.peak_percent - (272_001.0 / 1_050_000.0)).abs() < 1e-9);
        assert!(
            pressure
                .peak_model
                .as_deref()
                .is_some_and(|model| model.ends_with(":gpt-6-astra"))
        );

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

struct Fixture {
    root: TempDir,
    home: PathBuf,
    codex_home: PathBuf,
    opencode_home: PathBuf,
    opencode_config: PathBuf,
    env: crate::test_env::ScopedEnv,
}

impl Fixture {
    fn new() -> Result<Self> {
        Self::new_with_names("home", "opencode-home", "opencode-config")
    }

    fn new_with_names(
        home_name: &str,
        opencode_home_name: &str,
        opencode_config_name: &str,
    ) -> Result<Self> {
        let root = TempDir::new()?;
        let home = root.path().join(home_name);
        let codex_home = home.join(".codex");
        let opencode_home = root.path().join(opencode_home_name);
        let opencode_config = root.path().join(opencode_config_name);
        fs::create_dir_all(&home)?;
        fs::create_dir_all(&codex_home)?;
        fs::create_dir_all(&opencode_home)?;
        fs::create_dir_all(&opencode_config)?;

        let env = crate::test_env::ScopedEnv::capture(&[
            "HOME",
            "USERPROFILE",
            "CODEX_HOME",
            "OPENCODE_HOME",
            "OPENCODE_CONFIG_DIR",
        ]);
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("USERPROFILE", &home);
            std::env::set_var("CODEX_HOME", &codex_home);
            std::env::set_var("OPENCODE_HOME", &opencode_home);
            std::env::set_var("OPENCODE_CONFIG_DIR", &opencode_config);
        }

        fs::create_dir_all(home.join(".claude").join("projects").join("demo"))?;
        fs::create_dir_all(opencode_config.join("plugin"))?;
        fs::write(
            codex_home.join("config.toml"),
            "notify = [\"echo\", \"hello\"]\n",
        )?;
        fs::write(home.join(".claude").join("settings.json"), "{}")?;
        Ok(Self {
            root,
            home,
            codex_home,
            opencode_home,
            opencode_config,
            env,
        })
    }

    fn restore_env(&self) {
        self.env.restore();
    }

    fn seed_codex(&self) -> Result<()> {
        let repo_root = self.home.join("workspace").join("demo-repo");
        write_git_repo(&repo_root)?;
        let sessions_dir = self
            .codex_home
            .join("sessions")
            .join("2026")
            .join("04")
            .join("22");
        fs::create_dir_all(&sessions_dir)?;
        let payload = [
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"model\":\"gpt-5\",\"cwd\":\"{}\"}}}}",
                repo_root.to_string_lossy().replace('\\', "\\\\")
            ),
            "{\"timestamp\":\"2026-04-22T01:12:00Z\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"input_tokens\":100,\"cached_input_tokens\":20,\"output_tokens\":60,\"reasoning_output_tokens\":10,\"total_tokens\":190},\"total_token_usage\":{\"input_tokens\":100,\"cached_input_tokens\":20,\"output_tokens\":60,\"reasoning_output_tokens\":10,\"total_tokens\":190}}}}".to_string(),
        ]
        .join("\n");
        fs::write(sessions_dir.join("rollout-test.jsonl"), payload)?;
        Ok(())
    }

    fn seed_claude(&self) -> Result<()> {
        let claude_file = self
            .home
            .join(".claude")
            .join("projects")
            .join("demo")
            .join("session.jsonl");
        fs::write(
            claude_file,
            "{\"timestamp\":\"2026-04-22T02:00:00Z\",\"message\":{\"model\":\"claude-sonnet-4\",\"usage\":{\"input_tokens\":60,\"cache_creation_input_tokens\":10,\"cache_read_input_tokens\":5,\"output_tokens\":20,\"total_tokens\":90}}}\n",
        )?;
        Ok(())
    }

    fn seed_codex_gpt_5_6(&self) -> Result<()> {
        let repo_root = self.home.join("workspace").join("demo-repo");
        write_git_repo(&repo_root)?;
        let sessions_dir = self
            .codex_home
            .join("sessions")
            .join("2026")
            .join("07")
            .join("10");
        fs::create_dir_all(&sessions_dir)?;
        for (file, timestamp, cache_creation) in [
            (
                "rollout-gpt-5-6-short.jsonl",
                "2026-07-10T01:12:00Z",
                72_000,
            ),
            ("rollout-gpt-5-6-long.jsonl", "2026-07-10T01:13:00Z", 72_001),
        ] {
            let total_tokens = 300_000 + cache_creation;
            let usage = serde_json::json!({
                "input_tokens": 100_000,
                "cached_input_tokens": 100_000,
                "cache_creation_tokens": cache_creation,
                "output_tokens": 100_000,
                "reasoning_output_tokens": 0,
                "total_tokens": total_tokens
            });
            let payload = [
                serde_json::json!({
                    "type": "session_meta",
                    "payload": {
                        "model": "gpt-5.6-luna",
                        "cwd": repo_root.to_string_lossy()
                    }
                })
                .to_string(),
                serde_json::json!({
                    "timestamp": timestamp,
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "last_token_usage": usage,
                            "total_token_usage": usage
                        }
                    }
                })
                .to_string(),
            ]
            .join("\n");
            fs::write(sessions_dir.join(file), payload)?;
        }
        Ok(())
    }

    fn seed_claude_fable_mythos(&self) -> Result<()> {
        let claude_file = self
            .home
            .join(".claude")
            .join("projects")
            .join("demo")
            .join("session.jsonl");
        let rows = [
            "{\"timestamp\":\"2026-07-03T02:00:00Z\",\"message\":{\"model\":\"claude-fable-5\",\"usage\":{\"input_tokens\":1000000,\"cache_creation_input_tokens\":300000,\"cache_read_input_tokens\":200000,\"output_tokens\":400000,\"total_tokens\":1900000}}}",
            "{\"timestamp\":\"2026-07-03T02:30:00Z\",\"message\":{\"model\":\"claude-mythos-5\",\"usage\":{\"input_tokens\":1000000,\"cache_creation_input_tokens\":300000,\"cache_read_input_tokens\":200000,\"output_tokens\":400000,\"total_tokens\":1900000}}}",
            "{\"timestamp\":\"2026-09-01T02:00:00Z\",\"message\":{\"model\":\"claude-fable-5-1\",\"usage\":{\"input_tokens\":1000000,\"cache_creation_input_tokens\":300000,\"cache_read_input_tokens\":200000,\"output_tokens\":400000,\"total_tokens\":1900000}}}",
            "{\"timestamp\":\"2026-09-01T02:30:00Z\",\"message\":{\"model\":\"claude-mythos-5-1\",\"usage\":{\"input_tokens\":1000000,\"cache_creation_input_tokens\":300000,\"cache_read_input_tokens\":200000,\"output_tokens\":400000,\"total_tokens\":1900000}}}",
        ]
        .join("\n");
        fs::write(claude_file, format!("{rows}\n"))?;
        Ok(())
    }

    fn seed_opencode(&self) -> Result<()> {
        let repo_root = self.home.join("workspace").join("demo-repo");
        let db_path = self.opencode_home.join("opencode.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE project(id TEXT PRIMARY KEY, worktree TEXT);
            CREATE TABLE session(id TEXT PRIMARY KEY, project_id TEXT);
            CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
            "#,
        )?;
        conn.execute(
            "INSERT INTO project(id, worktree) VALUES (?1, ?2)",
            (&"project-1", &repo_root.to_string_lossy().to_string()),
        )?;
        conn.execute(
            "INSERT INTO session(id, project_id) VALUES (?1, ?2)",
            (&"session-1", &"project-1"),
        )?;
        let message = serde_json::json!({
            "id": "msg-1",
            "role": "assistant",
            "modelID": "gpt-5",
            "tokens": {
                "input": 40,
                "output": 15,
                "reasoning": 4,
                "cache": { "read": 6, "write": 5 }
            },
            "time": {
                "created": 1776823200000i64,
                "completed": 1776823200000i64
            }
        });
        conn.execute(
            "INSERT INTO message(id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (
                &"msg-1",
                &"session-1",
                &1776823200000i64,
                &message.to_string(),
            ),
        )?;
        Ok(())
    }

    fn seed_opencode_gpt_5_6(&self) -> Result<()> {
        let repo_root = self.home.join("workspace").join("demo-repo");
        write_git_repo(&repo_root)?;
        let db_path = self.opencode_home.join("opencode.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE project(id TEXT PRIMARY KEY, worktree TEXT);
            CREATE TABLE session(id TEXT PRIMARY KEY, project_id TEXT);
            CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
            "#,
        )?;
        conn.execute(
            "INSERT INTO project(id, worktree) VALUES (?1, ?2)",
            (&"project-1", &repo_root.to_string_lossy().to_string()),
        )?;
        conn.execute(
            "INSERT INTO session(id, project_id) VALUES (?1, ?2)",
            (&"session-1", &"project-1"),
        )?;
        for (id, model, cache_write, time_created) in [
            ("msg-terra", "gpt-5.6-terra", 72_000, 1_783_649_640_000_i64),
            ("msg-sol", "gpt-5.6-sol", 72_001, 1_783_649_700_000_i64),
        ] {
            let message = serde_json::json!({
                "id": id,
                "role": "assistant",
                "modelID": model,
                "tokens": {
                    "input": 100_000,
                    "output": 100_000,
                    "reasoning": 0,
                    "cache": { "read": 100_000, "write": cache_write }
                },
                "time": {
                    "created": time_created,
                    "completed": time_created
                }
            });
            conn.execute(
                "INSERT INTO message(id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
                (&id, &"session-1", &time_created, &message.to_string()),
            )?;
        }
        Ok(())
    }

    fn seed_codex_gpt_6_astra(&self) -> Result<()> {
        let repo_root = self.home.join("workspace").join("demo-repo");
        write_git_repo(&repo_root)?;
        let sessions_dir = self
            .codex_home
            .join("sessions")
            .join("2026")
            .join("09")
            .join("03");
        fs::create_dir_all(&sessions_dir)?;
        for (file, timestamp, cache_creation) in [
            (
                "rollout-gpt-6-astra-short.jsonl",
                "2026-09-03T01:12:00Z",
                72_000,
            ),
            (
                "rollout-gpt-6-astra-long.jsonl",
                "2026-09-03T01:13:00Z",
                72_001,
            ),
        ] {
            let total_tokens = 300_000 + cache_creation;
            let usage = serde_json::json!({
                "input_tokens": 200_000,
                "cached_input_tokens": 100_000,
                "cache_creation_tokens": cache_creation,
                "output_tokens": 100_000,
                "reasoning_output_tokens": 0,
                "total_tokens": total_tokens
            });
            let payload = [
                serde_json::json!({
                    "type": "session_meta",
                    "payload": {
                        "model": "gpt-6-astra",
                        "cwd": repo_root.to_string_lossy()
                    }
                })
                .to_string(),
                serde_json::json!({
                    "timestamp": timestamp,
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "last_token_usage": usage,
                            "total_token_usage": usage
                        }
                    }
                })
                .to_string(),
            ]
            .join("\n");
            fs::write(sessions_dir.join(file), payload)?;
        }
        Ok(())
    }

    fn seed_opencode_gpt_6_astra(&self) -> Result<()> {
        let repo_root = self.home.join("workspace").join("demo-repo");
        write_git_repo(&repo_root)?;
        let db_path = self.opencode_home.join("opencode.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE project(id TEXT PRIMARY KEY, worktree TEXT);
            CREATE TABLE session(id TEXT PRIMARY KEY, project_id TEXT);
            CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
            "#,
        )?;
        conn.execute(
            "INSERT INTO project(id, worktree) VALUES (?1, ?2)",
            (&"project-1", &repo_root.to_string_lossy().to_string()),
        )?;
        conn.execute(
            "INSERT INTO session(id, project_id) VALUES (?1, ?2)",
            (&"session-1", &"project-1"),
        )?;
        for (id, cache_write, time_created) in [
            ("msg-astra-short", 72_000, 1_788_408_720_000_i64),
            ("msg-astra-long", 72_001, 1_788_408_780_000_i64),
        ] {
            let message = serde_json::json!({
                "id": id,
                "role": "assistant",
                "modelID": "gpt-6-astra",
                "tokens": {
                    "input": 100_000,
                    "output": 100_000,
                    "reasoning": 0,
                    "cache": { "read": 100_000, "write": cache_write }
                },
                "time": {
                    "created": time_created,
                    "completed": time_created
                }
            });
            conn.execute(
                "INSERT INTO message(id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
                (&id, &"session-1", &time_created, &message.to_string()),
            )?;
        }
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.restore_env();
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
