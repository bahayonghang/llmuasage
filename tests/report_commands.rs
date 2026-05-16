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
        ..SeedEvent::default()
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
        ..SeedEvent::default()
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
    assert!(daily["daily"][0].get("conversation_count").is_none());
    assert!(daily["daily"][0].get("conversationCount").is_none());

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
fn daily_defaults_to_last_7_days_and_all_restores_history() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let today = Utc::now().date_naive();
    let six_days_ago = today - Duration::days(6);
    let seven_days_ago = today - Duration::days(7);
    let today_arg = today.format("%Y%m%d").to_string();
    let six_days_ago_arg = six_days_ago.format("%Y%m%d").to_string();
    let expected_default_dates = (0..=6)
        .map(|offset| {
            (today - Duration::days(offset))
                .format("%Y-%m-%d")
                .to_string()
        })
        .collect::<Vec<_>>();

    for offset in 0..=7 {
        let date = today - Duration::days(offset);
        let date_display = date.format("%Y-%m-%d").to_string();
        let event_at = format!("{date_display}T12:00:00Z");
        let event_key = format!("codex:day-{offset}:1");
        let session_id = format!("session-day-{offset}");
        let source_path_hash = format!("source-day-{offset}");
        fixture.seed_event(SeedEvent {
            event_key: &event_key,
            source: "codex",
            model: "gpt-5",
            event_at: &event_at,
            input_tokens: 10,
            cache_read_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 10,
            project_hash: "project-a",
            project_label: "Project A",
            project_ref: Some("example/project-a"),
            session_id: Some(&session_id),
            source_path_hash: Some(&source_path_hash),
            ..SeedEvent::default()
        })?;
    }

    let default_daily = fixture.json(&["--json", "--timezone", "UTC"])?;
    let default_dates = daily_dates(&default_daily);
    assert_eq!(default_dates, expected_default_dates);
    assert!(!default_dates.contains(&seven_days_ago.format("%Y-%m-%d").to_string()));
    assert_eq!(default_daily["totals"]["total_tokens"].as_i64(), Some(70));

    let all_daily = fixture.json(&["--all", "--json", "--timezone", "UTC"])?;
    assert_eq!(all_daily["daily"].as_array().map(Vec::len), Some(8));
    assert_eq!(all_daily["totals"]["total_tokens"].as_i64(), Some(80));

    let range_daily = fixture.json(&[
        "daily",
        "--since",
        &six_days_ago_arg,
        "--until",
        &today_arg,
        "--json",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(daily_dates(&range_daily), expected_default_dates);

    let explicit_old_range = fixture.json(&[
        "daily",
        "--since",
        &seven_days_ago.format("%Y%m%d").to_string(),
        "--until",
        &seven_days_ago.format("%Y%m%d").to_string(),
        "--json",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(
        daily_dates(&explicit_old_range),
        vec![seven_days_ago.format("%Y-%m-%d").to_string()]
    );
    assert_eq!(
        explicit_old_range["totals"]["total_tokens"].as_i64(),
        Some(10)
    );

    Ok(())
}

#[test]
fn daily_human_output_uses_source_tables_compact_tokens_and_no_default_info_logs() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let today = Utc::now().date_naive();
    let six_days_ago = today - Duration::days(6);
    let today_display = today.format("%Y-%m-%d").to_string();
    let six_days_ago_display = six_days_ago.format("%Y-%m-%d").to_string();
    let today_event = format!("{today_display}T12:00:00Z");
    let six_days_ago_event = format!("{six_days_ago_display}T12:00:00Z");
    fixture.seed_event(SeedEvent {
        event_key: "codex:today:human",
        source: "codex",
        model: "gpt-5.4",
        event_at: &today_event,
        input_tokens: 978_050,
        cache_read_tokens: 5_370_000,
        output_tokens: 40_330_000_000,
        reasoning_output_tokens: 12_345,
        total_tokens: 40_336_360_395,
        project_hash: "project-a",
        project_label: "Project A",
        project_ref: Some("example/project-a"),
        session_id: Some("session-a"),
        source_path_hash: Some("source-a"),
        ..SeedEvent::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:today:human-second",
        source: "codex",
        model: "gpt-5.4",
        event_at: &today_event,
        input_tokens: 1_000,
        cache_read_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: 1_000,
        project_hash: "project-a",
        project_label: "Project A",
        project_ref: Some("example/project-a"),
        session_id: Some("session-b"),
        source_path_hash: Some("source-b"),
        ..SeedEvent::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:six-days-ago:human",
        source: "codex",
        model: "gpt-5.4",
        event_at: &six_days_ago_event,
        input_tokens: 2_000,
        cache_read_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: 2_000,
        project_hash: "project-a",
        project_label: "Project A",
        project_ref: Some("example/project-a"),
        session_id: Some("session-old"),
        source_path_hash: Some("source-old"),
        ..SeedEvent::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "claude:today:human",
        source: "claude",
        model: "claude-sonnet-4-20250514",
        event_at: &today_event,
        input_tokens: 5_370_000,
        cache_read_tokens: 978_050,
        output_tokens: 123_000,
        reasoning_output_tokens: 0,
        total_tokens: 6_471_050,
        project_hash: "project-b",
        project_label: "Project B",
        project_ref: Some("example/project-b"),
        session_id: Some("session-c"),
        source_path_hash: Some("source-c"),
        ..SeedEvent::default()
    })?;

    let output = fixture.output_with_env(
        &["--timezone", "UTC"],
        &[("COLUMNS", "120"), ("NO_COLOR", "1")],
    )?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stdout.contains("Codex daily usage"));
    assert!(stdout.contains("Claude daily usage"));
    assert!(stdout.contains("---\nClaude daily usage"));
    assert!(stdout.contains(&today_display));
    assert!(stdout.contains(&six_days_ago_display));
    assert!(stdout.contains('\u{250C}'));
    assert!(stdout.contains("Conv"));
    assert!(stdout.contains("Cache"));
    assert!(stdout.contains("Reason"));
    assert!(stdout.contains("All"));
    assert!(stdout.contains("Notes"));
    assert!(stdout.contains("unpriced"));
    assert!(stdout.contains("reason not reported"));
    assert!(stdout.contains("gpt-5.4"));
    assert!(stdout.contains("sonnet-4"));
    assert!(stdout.contains("978.05K"));
    assert!(stdout.contains("5.37M"));
    assert!(stdout.contains("40.33B"));
    assert!(stdout.contains("TOTAL"));
    assert!(!stdout.contains("Total:"));
    assert!(!stdout.contains("978,050"));
    assert!(!stdout.contains("5,370,000"));
    assert!(!stdout.contains("\u{1b}["));
    assert!(!stderr.contains("INFO"));
    assert!(!stderr.contains("开始初始化本地目录与 SQLite schema"));

    let colored =
        fixture.output_with_env(&["--timezone", "UTC"], &[("LLMUSAGE_FORCE_COLOR", "1")])?;
    assert!(colored.status.success(), "{colored:?}");
    let colored_stdout = String::from_utf8(colored.stdout)?;
    assert!(colored_stdout.contains("\u{1b}["));
    assert!(colored_stdout.contains("Codex daily usage"));

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
    assert!(
        fixture
            .paths
            .root_dir
            .join("pricing")
            .join("litellm-snapshot-2026-05.json")
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
fn report_commands_use_persisted_cost_columns() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let today = Utc::now().date_naive();
    let today_display = today.format("%Y-%m-%d").to_string();
    let today_event = format!("{today_display}T12:00:00Z");
    fixture.seed_event(SeedEvent {
        event_key: "codex:persisted-cost:1",
        source: "codex",
        model: "gpt-5",
        event_at: &today_event,
        input_tokens: 500_000,
        cache_read_tokens: 0,
        output_tokens: 100_000,
        reasoning_output_tokens: 0,
        total_tokens: 600_000,
        cost_with_cache_usd: 42.5,
        cost_without_cache_usd: 45.0,
        pricing_status: "snapshot",
        pricing_source: Some("manual-test"),
        project_hash: "project-a",
        project_label: "Project A",
        project_ref: Some("example/project-a"),
        session_id: Some("session-a"),
        source_path_hash: Some("source-a"),
    })?;

    let daily = fixture.json(&["daily", "--all", "--json", "--timezone", "UTC"])?;
    assert_eq!(daily["totals"]["estimated_cost_usd"].as_f64(), Some(42.5));
    assert_eq!(daily["daily"][0]["estimated_cost_usd"].as_f64(), Some(42.5));
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
    let daily_help = fixture.output(&["daily", "--help"])?;
    let daily_help_stdout = String::from_utf8(daily_help.stdout)?;
    assert!(daily_help_stdout.contains("last 7 days"));
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
        ..SeedEvent::default()
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

fn daily_dates(value: &serde_json::Value) -> Vec<String> {
    value["daily"]
        .as_array()
        .expect("daily array")
        .iter()
        .map(|row| row["date"].as_str().expect("daily row date").to_string())
        .collect()
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
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                project_hash, project_label, project_ref, path_hash,
                session_id, session_label, source_path_hash, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18, ?19, ?4)
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
                event.cost_with_cache_usd,
                event.cost_without_cache_usd,
                event.pricing_status,
                event.pricing_source,
                event.project_hash,
                event.project_label,
                event.project_ref,
                event.source_path_hash.unwrap_or(event.event_key),
                event.session_id,
                event.source_path_hash,
            ],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                event_count, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 1, ?3)
            ON CONFLICT(source, model, hour_start, project_hash) DO UPDATE SET
                input_tokens = input_tokens + excluded.input_tokens,
                cache_read_tokens = cache_read_tokens + excluded.cache_read_tokens,
                output_tokens = output_tokens + excluded.output_tokens,
                reasoning_output_tokens = reasoning_output_tokens + excluded.reasoning_output_tokens,
                total_tokens = total_tokens + excluded.total_tokens,
                cost_with_cache_usd = cost_with_cache_usd + excluded.cost_with_cache_usd,
                cost_without_cache_usd = cost_without_cache_usd + excluded.cost_without_cache_usd,
                pricing_status = CASE
                    WHEN pricing_status = excluded.pricing_status THEN pricing_status
                    ELSE 'mixed'
                END,
                pricing_source = CASE
                    WHEN pricing_source IS excluded.pricing_source THEN pricing_source
                    ELSE 'mixed'
                END,
                event_count = event_count + excluded.event_count,
                updated_at = excluded.updated_at
            "#,
            params![
                event.source,
                event.model,
                event.event_at,
                event.project_hash,
                event.project_label,
                event.project_ref,
                event.input_tokens,
                event.cache_read_tokens,
                event.output_tokens,
                event.reasoning_output_tokens,
                event.total_tokens,
                event.cost_with_cache_usd,
                event.cost_without_cache_usd,
                event.pricing_status,
                event.pricing_source,
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
            .env("OPENCODE_HOME", self.home.join("opencode"));
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
    cost_with_cache_usd: f64,
    cost_without_cache_usd: f64,
    pricing_status: &'a str,
    pricing_source: Option<&'a str>,
}

impl Default for SeedEvent<'_> {
    fn default() -> Self {
        Self {
            event_key: "codex:test:1",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-01T00:00:00Z",
            input_tokens: 0,
            cache_read_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 0,
            project_hash: "project-a",
            project_label: "Project A",
            project_ref: None,
            session_id: None,
            source_path_hash: None,
            cost_with_cache_usd: 0.0,
            cost_without_cache_usd: 0.0,
            pricing_status: "unpriced",
            pricing_source: None,
        }
    }
}
