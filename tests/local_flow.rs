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
        let store = llmusage::store::Store::new(&app.paths);
        let dashboard = Dashboard::open(&store)?;
        let overview = dashboard.overview()?;
        assert_eq!(overview.source_count, 3);
        assert!(overview.total.total_tokens >= 344);

        let projects = dashboard.project_breakdown()?;
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
        assert!(html_out.join("assets").join("render.js").is_file());
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
                .join("charts.js")
                .is_file()
        );
        assert!(
            html_out
                .join("assets")
                .join("render")
                .join("tables.js")
                .is_file()
        );
        assert!(
            html_out
                .join("assets")
                .join("render")
                .join("health.js")
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
        assert!(exported_index.contains("<strong>本地用量概览</strong>"));
        assert!(exported_index.contains("用量趋势"));
        assert!(!exported_index.contains("llmusage 本地账本"));
        assert!(web::live_index_html().contains("data-mode=\"live\""));
        assert!(web::snapshot_index_html().contains("data-mode=\"snapshot\""));
        assert!(web::live_index_html().contains("type=\"module\""));

        commands::diagnostics::run(&app, Some(fixture.root.path().join("diagnostics.json")))
            .await?;
        commands::doctor::run(&app, true).await?;

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
        let store = Store::new(&app.paths);
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

        let store = Store::new(&app.paths);
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
        assert!(plugin_body.contains("cmd /c \"\""));

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
}

fn write_git_repo(repo_root: &Path) -> Result<()> {
    fs::create_dir_all(repo_root.join(".git"))?;
    fs::write(
        repo_root.join(".git").join("config"),
        "[remote \"origin\"]\n    url = https://github.com/example/demo-repo.git\n",
    )?;
    Ok(())
}
