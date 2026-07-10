use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use llmusage::{
    app::AppContext, commands, integrations, models::SourceKind, query::Dashboard, store::Store,
    web,
};
use rusqlite::Connection;
use tempfile::TempDir;

#[test]
fn local_flow_installs_syncs_exports_and_uninstalls() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex()?;
    fixture.seed_claude()?;
    fixture.seed_opencode()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;

        commands::init::run(&app).await?;
        assert!(app.paths.db_path.is_file());
        assert!(app.paths.hook_cmd_path.is_file());
        assert!(app.paths.hook_sh_path.is_file());

        let codex_config = fs::read_to_string(fixture.codex_home.join("config.toml"))?;
        assert!(codex_config.contains("llmusage-hook"));
        let claude_settings =
            fs::read_to_string(fixture.home.join(".claude").join("settings.json"))?;
        assert!(claude_settings.contains("SessionEnd"));
        assert!(
            fixture
                .opencode_config
                .join("plugin")
                .join("llmusage-tracker.js")
                .is_file()
        );

        commands::sync::run(&app).await?;
        let store = llmusage::store::Store::new(&app.paths)?;
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
        let exported_index = fs::read_to_string(html_out.join("index.html"))?;
        assert!(exported_index.contains("data-mode=\"snapshot\""));
        assert!(exported_index.contains("type=\"module\""));
        assert!(exported_index.contains("assets/app.js"));
        assert!(exported_index.contains("assets/base.css"));
        assert!(exported_index.contains("assets/layout.css"));
        assert!(exported_index.contains("assets/components.css"));
        assert!(exported_index.contains("assets/charts.css"));
        assert!(exported_index.contains("<title>llmusage · 本地用量概览</title>"));
        assert!(exported_index.contains(">本地用量概览</strong>"));
        assert!(exported_index.contains("用量趋势"));
        assert!(exported_index.contains("Cost Explorer"));
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

        commands::uninstall::run(&app, false).await?;
        let codex_restored = fs::read_to_string(fixture.codex_home.join("config.toml"))?;
        assert!(codex_restored.contains("echo"));
        let claude_restored =
            fs::read_to_string(fixture.home.join(".claude").join("settings.json"))?;
        assert!(!claude_restored.contains("llmusage-hook"));
        assert!(
            !fixture
                .opencode_config
                .join("plugin")
                .join("llmusage-tracker.js")
                .exists()
        );

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn claude_install_reports_invalid_settings_shapes() -> Result<()> {
    let cases = [
        ("top-level", "[]", "顶层必须是 object"),
        (
            "hooks-shape",
            "{\"hooks\":\"invalid\"}",
            "hooks 字段必须是 object",
        ),
        (
            "event-shape",
            "{\"hooks\":{\"Stop\":{}}}",
            "Claude hooks.Stop 必须是数组",
        ),
    ];

    for (_name, raw, expected) in cases {
        let fixture = Fixture::new()?;
        fs::write(fixture.home.join(".claude").join("settings.json"), raw)?;
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;

        let err = integrations::claude::install(&app, &store).expect_err("shape should fail");
        assert!(
            err.to_string().contains(expected),
            "unexpected error: {err:#}"
        );

        fixture.restore_env();
    }

    Ok(())
}

#[test]
fn init_continues_when_claude_install_fails_and_records_error() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::write(
        fixture.home.join(".claude").join("settings.json"),
        "{\"hooks\":\"invalid\"}",
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::init::run(&app).await?;

        let codex_config = fs::read_to_string(fixture.codex_home.join("config.toml"))?;
        assert!(codex_config.contains("llmusage-hook"));
        assert!(
            fixture
                .opencode_config
                .join("plugin")
                .join("llmusage-tracker.js")
                .is_file()
        );
        assert_eq!(
            fs::read_to_string(fixture.home.join(".claude").join("settings.json"))?,
            "{\"hooks\":\"invalid\"}"
        );

        let store = Store::new(&app.paths)?;
        let states = store.integration_state().load_integration_states()?;
        assert_eq!(
            states
                .iter()
                .find(|item| item.source == "claude")
                .map(|item| item.status.as_str()),
            Some("error")
        );
        assert_eq!(
            states
                .iter()
                .find(|item| item.source == "codex")
                .map(|item| item.status.as_str()),
            Some("ready")
        );
        assert_eq!(
            states
                .iter()
                .find(|item| item.source == "opencode")
                .map(|item| item.status.as_str()),
            Some("ready")
        );

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn init_writes_quoted_windows_string_commands_for_spaced_paths() -> Result<()> {
    let fixture = Fixture::new_with_spaces()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::init::run(&app).await?;

        let expected_stop =
            integrations::HookTarget::current(&app).shell_command(SourceKind::Claude, "Stop");
        let claude_settings: serde_json::Value = serde_json::from_slice(&fs::read(
            fixture.home.join(".claude").join("settings.json"),
        )?)?;
        let stop_commands = claude_settings
            .get("hooks")
            .and_then(|hooks| hooks.get("Stop"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.get("hooks").and_then(serde_json::Value::as_array))
            .flatten()
            .filter_map(|hook| hook.get("command").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>();
        assert!(
            stop_commands
                .iter()
                .any(|command| *command == expected_stop)
        );

        if cfg!(windows) {
            assert!(expected_stop.contains("cmd /c \"\""));
            assert!(
                expected_stop.contains("llmusage-hook.cmd\" --source claude --trigger Stop --auto")
            );
        }

        let plugin_body = fs::read_to_string(
            fixture
                .opencode_config
                .join("plugin")
                .join("llmusage-tracker.js"),
        )?;
        let expected_opencode = integrations::HookTarget::current(&app)
            .shell_command(SourceKind::Opencode, "session.updated");
        assert!(plugin_body.contains(&expected_opencode));
        if cfg!(windows) {
            assert!(plugin_body.contains("cmd /c \"\""));
        }

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn antigravity_install_cleans_legacy_gemini_hooks() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(fixture.home.join(".gemini").join("config"))?;
    let user_antigravity_command = "echo user-antigravity";
    let legacy_antigravity_command = "llmusage-hook --source gemini --trigger Stop --auto";
    fs::write(
        fixture
            .home
            .join(".gemini")
            .join("config")
            .join("hooks.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "Stop": [
                { "type": "command", "command": user_antigravity_command },
                { "type": "command", "command": legacy_antigravity_command }
            ]
        }))?,
    )?;
    let user_legacy_settings_command = "echo user-legacy-gemini";
    let legacy_settings_command = "llmusage-hook --source gemini --trigger SessionEnd --auto";
    fs::write(
        fixture.home.join(".gemini").join("settings.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "hooks": {
                "SessionEnd": [
                    { "hooks": [{ "type": "command", "command": user_legacy_settings_command }] },
                    { "hooks": [{ "type": "command", "command": legacy_settings_command }] }
                ]
            }
        }))?,
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        commands::init::run(&app).await?;

        let expected_stop =
            integrations::HookTarget::current(&app).shell_command(SourceKind::Antigravity, "Stop");
        let hooks: serde_json::Value = serde_json::from_slice(&fs::read(
            fixture
                .home
                .join(".gemini")
                .join("config")
                .join("hooks.json"),
        )?)?;
        let stop_commands = hooks
            .get("Stop")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|hook| hook.get("command").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>();
        assert!(
            stop_commands
                .iter()
                .any(|command| *command == expected_stop)
        );
        assert!(expected_stop.contains("--source antigravity"));
        assert!(!expected_stop.contains("--source gemini"));
        assert!(stop_commands.contains(&user_antigravity_command));
        assert!(!stop_commands.contains(&legacy_antigravity_command));

        let legacy_settings: serde_json::Value = serde_json::from_slice(&fs::read(
            fixture.home.join(".gemini").join("settings.json"),
        )?)?;
        let session_end_commands = legacy_settings
            .get("hooks")
            .and_then(|hooks| hooks.get("SessionEnd"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.get("hooks").and_then(serde_json::Value::as_array))
            .flatten()
            .filter_map(|hook| hook.get("command").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>();
        assert!(session_end_commands.contains(&user_legacy_settings_command));
        assert!(!session_end_commands.contains(&legacy_settings_command));

        commands::uninstall::run(&app, false).await?;
        let restored_hooks: serde_json::Value = serde_json::from_slice(&fs::read(
            fixture
                .home
                .join(".gemini")
                .join("config")
                .join("hooks.json"),
        )?)?;
        let remaining = restored_hooks
            .get("Stop")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            .unwrap_or_default();
        assert_eq!(remaining, 1);
        let restored_commands = restored_hooks
            .get("Stop")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|hook| hook.get("command").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(restored_commands, vec![user_antigravity_command]);

        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn hook_run_syncs_only_triggered_source() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex()?;
    fixture.seed_claude()?;
    fixture.seed_opencode()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;

        commands::hook_run::run(&app, SourceKind::Claude, "Stop", true).await?;

        let store = Store::new(&app.paths)?;
        let dashboard = Dashboard::open(&store)?;
        let sources = dashboard.source_breakdown(&Default::default())?;
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source, "claude");
        assert!(sources[0].total_tokens > 0);

        Ok::<_, anyhow::Error>(())
    })?;

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
        for model in ["claude-fable-5", "claude-mythos-5"] {
            let row = models
                .iter()
                .find(|item| item.model == model)
                .unwrap_or_else(|| panic!("{model} should be present in model breakdown"));
            assert_eq!(row.pricing_status, "static", "{model}");
            assert_eq!(row.pricing_source.as_deref(), Some("static-v2"), "{model}");
            assert!(
                (row.cost_with_cache_usd - 33.95).abs() < 1e-9,
                "{model} cost should use Fable/Mythos embedded rates"
            );
        }
        assert!(
            models
                .iter()
                .filter(|item| matches!(item.model.as_str(), "claude-fable-5" | "claude-mythos-5"))
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
                .all(|row| row.2 == "static" && row.3 == "static-v2")
        );
        assert_eq!(rows[0].0, "codex");
        assert_eq!(rows[0].1, "gpt-5.6-luna");
        assert!(rows[0].4.contains("\"tier\":\"default\""));
        assert!((rows[0].5 - 0.8).abs() < 1e-9);
        assert_eq!(rows[1].1, "gpt-5.6-luna");
        assert!(rows[1].4.contains("\"tier\":\"long_context\""));
        assert!((rows[1].5 - 1.3000025).abs() < 1e-9);
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
        assert!((bucket_cost - 2.1000025).abs() < 1e-9);
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

struct Fixture {
    root: TempDir,
    home: PathBuf,
    codex_home: PathBuf,
    opencode_home: PathBuf,
    opencode_config: PathBuf,
    saved: Vec<(String, Option<String>)>,
}

impl Fixture {
    fn new() -> Result<Self> {
        Self::new_with_names("home", "opencode-home", "opencode-config")
    }

    fn new_with_spaces() -> Result<Self> {
        Self::new_with_names("home with spaces", "opencode home", "opencode config")
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

        let mut saved = Vec::new();
        for key in [
            "HOME",
            "USERPROFILE",
            "CODEX_HOME",
            "OPENCODE_HOME",
            "OPENCODE_CONFIG_DIR",
        ] {
            saved.push((key.to_string(), std::env::var(key).ok()));
        }
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
}

fn write_git_repo(repo_root: &Path) -> Result<()> {
    fs::create_dir_all(repo_root.join(".git"))?;
    fs::write(
        repo_root.join(".git").join("config"),
        "[remote \"origin\"]\n    url = https://github.com/example/demo-repo.git\n",
    )?;
    Ok(())
}
