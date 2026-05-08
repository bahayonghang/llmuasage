use std::{path::PathBuf, process::Command};

use anyhow::{Result, bail};
use chrono::{Duration, Utc};
use rusqlite::{Connection, params};
use tempfile::TempDir;

use llmusage::{paths::AppPaths, store::Store};

#[test]
fn report_commands_emit_stable_json_from_sqlite() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let today = Utc::now().date_naive();
    let today_arg = today.format("%Y%m%d").to_string();
    let today_display = today.format("%Y-%m-%d").to_string();
    let today_month = today.format("%Y-%m").to_string();
    let today_first_event = format!("{today_display}T00:15:00Z");
    let today_second_event = format!("{today_display}T03:00:00Z");
    fixture.seed_event(SeedEvent {
        event_key: "codex:source-a:fingerprint-a:1",
        source: "codex",
        model: "gpt-5",
        event_at: &today_first_event,
        input_tokens: 100,
        cache_read_tokens: 10,
        output_tokens: 20,
        reasoning_output_tokens: 5,
        total_tokens: 135,
        project_hash: "project-a",
        project_label: "Project A",
        project_ref: Some("example/project-a"),
        session_id: Some("session-a"),
        source_path_hash: Some("source-a"),
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "claude:source-b:fingerprint-b:1",
        source: "claude",
        model: "claude-sonnet-4",
        event_at: &today_second_event,
        input_tokens: 200,
        cache_read_tokens: 0,
        output_tokens: 50,
        reasoning_output_tokens: 0,
        total_tokens: 250,
        project_hash: "project-b",
        project_label: "Project B",
        project_ref: Some("example/project-b"),
        session_id: Some("session-b"),
        source_path_hash: Some("source-b"),
    })?;

    let daily = fixture.json(&[
        "--json",
        "--since",
        &today_arg,
        "--until",
        &today_arg,
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(daily["daily"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        daily["daily"][0]["date"].as_str(),
        Some(today_display.as_str())
    );
    assert_eq!(daily["totals"]["total_tokens"].as_i64(), Some(385));

    let projects = fixture.json(&[
        "daily",
        "--instances",
        "--all",
        "--json",
        "--timezone",
        "UTC",
    ])?;
    assert!(projects["projects"].get("example/project-a").is_some());
    assert!(projects["projects"].get("example/project-b").is_some());

    let monthly = fixture.json(&["monthly", "--json", "--timezone", "UTC"])?;
    assert_eq!(
        monthly["monthly"][0]["month"].as_str(),
        Some(today_month.as_str())
    );
    assert_eq!(monthly["totals"]["total_tokens"].as_i64(), Some(385));

    let session = fixture.json(&[
        "session",
        "--id",
        "session-a",
        "--json",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(
        session["session"]["session_id"].as_str(),
        Some("codex:session-a")
    );
    assert_eq!(session["session"]["total_tokens"].as_i64(), Some(135));

    let blocks = fixture.json(&[
        "blocks",
        "--json",
        "--token-limit",
        "max",
        "--timezone",
        "UTC",
    ])?;
    assert!(
        blocks["blocks"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );

    for payload in [&daily, &monthly, &session, &blocks] {
        assert_json_has_no_camel_case_keys(payload);
    }

    Ok(())
}

#[test]
fn daily_defaults_to_today_and_all_restores_history() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let today = Utc::now().date_naive();
    let yesterday = today - Duration::days(1);
    let today_display = today.format("%Y-%m-%d").to_string();
    let yesterday_arg = yesterday.format("%Y%m%d").to_string();
    let today_arg = today.format("%Y%m%d").to_string();
    let today_event = format!("{today_display}T12:00:00Z");
    let yesterday_event = format!("{}T12:00:00Z", yesterday.format("%Y-%m-%d"));

    fixture.seed_event(SeedEvent {
        event_key: "codex:today:1",
        source: "codex",
        model: "gpt-5",
        event_at: &today_event,
        input_tokens: 10,
        cache_read_tokens: 1,
        output_tokens: 2,
        reasoning_output_tokens: 3,
        total_tokens: 16,
        project_hash: "project-a",
        project_label: "Project A",
        project_ref: Some("example/project-a"),
        session_id: Some("session-today"),
        source_path_hash: Some("today-source"),
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:yesterday:1",
        source: "codex",
        model: "gpt-5",
        event_at: &yesterday_event,
        input_tokens: 20,
        cache_read_tokens: 2,
        output_tokens: 4,
        reasoning_output_tokens: 6,
        total_tokens: 32,
        project_hash: "project-a",
        project_label: "Project A",
        project_ref: Some("example/project-a"),
        session_id: Some("session-yesterday"),
        source_path_hash: Some("yesterday-source"),
    })?;

    let default_daily = fixture.json(&["--json", "--timezone", "UTC"])?;
    assert_eq!(default_daily["daily"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        default_daily["daily"][0]["date"].as_str(),
        Some(today_display.as_str())
    );
    assert_eq!(default_daily["totals"]["total_tokens"].as_i64(), Some(16));

    let all_daily = fixture.json(&["--all", "--json", "--timezone", "UTC"])?;
    assert_eq!(all_daily["daily"].as_array().map(Vec::len), Some(2));
    assert_eq!(all_daily["totals"]["total_tokens"].as_i64(), Some(48));

    let range_daily = fixture.json(&[
        "daily",
        "--since",
        &yesterday_arg,
        "--until",
        &today_arg,
        "--json",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(range_daily["daily"].as_array().map(Vec::len), Some(2));

    Ok(())
}

#[test]
fn daily_human_output_uses_box_table_and_compact_columns() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let today = Utc::now().date_naive();
    let today_display = today.format("%Y-%m-%d").to_string();
    let today_event = format!("{today_display}T12:00:00Z");
    fixture.seed_event(SeedEvent {
        event_key: "codex:today:human",
        source: "codex",
        model: "claude-sonnet-4-20250514",
        event_at: &today_event,
        input_tokens: 1234,
        cache_read_tokens: 890,
        output_tokens: 56,
        reasoning_output_tokens: 7,
        total_tokens: 2187,
        project_hash: "project-a",
        project_label: "Project A",
        project_ref: Some("example/project-a"),
        session_id: Some("session-a"),
        source_path_hash: Some("source-a"),
    })?;

    let output = fixture.output(&["--timezone", "UTC"])?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains('\u{250C}'));
    assert!(stdout.contains("Cache Read"));
    assert!(stdout.contains("Total Tokens"));
    assert!(stdout.contains("- sonnet-4"));
    assert!(stdout.contains("Total"));
    assert!(!stdout.contains("Total:"));

    let compact = fixture.output_with_env(&["--timezone", "UTC"], &[("COLUMNS", "80")])?;
    assert!(compact.status.success(), "{compact:?}");
    let compact_stdout = String::from_utf8(compact.stdout)?;
    assert!(compact_stdout.contains('\u{250C}'));
    assert!(!compact_stdout.contains("Cache Read"));
    assert!(!compact_stdout.contains("Total Tokens"));
    assert!(compact_stdout.contains("Cost (USD)"));

    Ok(())
}

#[test]
fn cli_home_flag_overrides_llmusage_home_env() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let other = TempDir::new()?;
    let output = Command::new(env!("CARGO_BIN_EXE_llmusage"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("--home")
        .arg(&fixture.paths.root_dir)
        .arg("statusline")
        .arg("--no-cache")
        .env("LLMUSAGE_HOME", other.path())
        .env("HOME", &fixture.home)
        .env("USERPROFILE", &fixture.home)
        .env("RUST_LOG", "off")
        .output()?;

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
    Ok(())
}

#[test]
fn report_help_and_legacy_help_still_parse() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    for args in [
        vec!["--help"],
        vec!["daily", "--help"],
        vec!["monthly", "--help"],
        vec!["session", "--help"],
        vec!["blocks", "--help"],
        vec!["statusline", "--help"],
        vec!["export", "html", "--help"],
    ] {
        let output = fixture.output(&args)?;
        assert!(output.status.success(), "{args:?}: {output:?}");
    }
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

#[test]
fn cli_json_outputs_all_snake_case() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let today = Utc::now().date_naive();
    let today_display = today.format("%Y-%m-%d").to_string();
    let today_event = format!("{today_display}T12:00:00Z");
    fixture.seed_event(SeedEvent {
        event_key: "codex:snake-case:1",
        source: "codex",
        model: "gpt-5",
        event_at: &today_event,
        input_tokens: 10,
        cache_read_tokens: 1,
        output_tokens: 2,
        reasoning_output_tokens: 3,
        total_tokens: 16,
        project_hash: "project-a",
        project_label: "Project A",
        project_ref: Some("example/project-a"),
        session_id: Some("session-a"),
        source_path_hash: Some("source-a"),
    })?;

    for args in [
        vec!["daily", "--json", "--timezone", "UTC"],
        vec!["monthly", "--json", "--timezone", "UTC"],
        vec!["session", "--json", "--timezone", "UTC"],
        vec!["blocks", "--json", "--timezone", "UTC"],
    ] {
        let json = fixture.json(&args)?;
        assert_json_has_no_camel_case_keys(&json);
    }

    Ok(())
}

fn assert_json_has_no_camel_case_keys(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                assert!(
                    !has_camel_case_boundary(key),
                    "JSON key should be snake_case, got {key}"
                );
                assert_json_has_no_camel_case_keys(value);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                assert_json_has_no_camel_case_keys(item);
            }
        }
        _ => {}
    }
}

fn has_camel_case_boundary(value: &str) -> bool {
    let mut prev_lower = false;
    for ch in value.chars() {
        if prev_lower && ch.is_ascii_uppercase() {
            return true;
        }
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    false
}

struct ReportCliFixture {
    _temp: TempDir,
    home: PathBuf,
    paths: AppPaths,
}

impl ReportCliFixture {
    fn new() -> Result<Self> {
        let temp = TempDir::new()?;
        let home = temp.path().join("home");
        let root_dir = home.join(".llmusage");
        let bin_dir = root_dir.join("bin");
        std::fs::create_dir_all(&home)?;
        let paths = AppPaths {
            db_path: root_dir.join("llmusage.db"),
            hook_cmd_path: bin_dir.join("llmusage-hook.cmd"),
            hook_sh_path: bin_dir.join("llmusage-hook.sh"),
            lock_path: root_dir.join("worker.lock"),
            backups_dir: root_dir.join("backups"),
            exports_dir: root_dir.join("exports"),
            root_dir,
            bin_dir,
        };
        Store::new(&paths)?.bootstrap()?;
        Ok(Self {
            _temp: temp,
            home,
            paths,
        })
    }

    fn seed_event(&self, event: SeedEvent<'_>) -> Result<()> {
        let conn = Connection::open(&self.paths.db_path)?;
        conn.execute(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cache_read_tokens, output_tokens, reasoning_output_tokens, total_tokens,
                project_hash, project_label, project_ref, path_hash,
                session_id, session_label, source_path_hash, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14, ?15, ?4)
            "#,
            params![
                event.event_key,
                event.source,
                event.model,
                event.event_at,
                event.input_tokens,
                event.cache_read_tokens,
                event.output_tokens,
                event.reasoning_output_tokens,
                event.total_tokens,
                event.project_hash,
                event.project_label,
                event.project_ref,
                event.source_path_hash.unwrap_or(event.event_key),
                event.session_id,
                event.source_path_hash,
            ],
        )?;
        Ok(())
    }

    fn json(&self, args: &[&str]) -> Result<serde_json::Value> {
        let output = self.output(args)?;
        if !output.status.success() {
            bail!("command failed: {output:?}");
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    fn output(&self, args: &[&str]) -> Result<std::process::Output> {
        self.output_with_env(args, &[])
    }

    fn output_with_env(
        &self,
        args: &[&str],
        envs: &[(&str, &str)],
    ) -> Result<std::process::Output> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_llmusage"));
        command
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(args)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("CODEX_HOME", self.home.join(".codex"))
            .env("OPENCODE_HOME", self.home.join("opencode"))
            .env("RUST_LOG", "off");
        for (key, value) in envs {
            command.env(key, value);
        }
        Ok(command.output()?)
    }
}

struct SeedEvent<'a> {
    event_key: &'a str,
    source: &'a str,
    model: &'a str,
    event_at: &'a str,
    input_tokens: i64,
    cache_read_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
    project_hash: &'a str,
    project_label: &'a str,
    project_ref: Option<&'a str>,
    session_id: Option<&'a str>,
    source_path_hash: Option<&'a str>,
}
