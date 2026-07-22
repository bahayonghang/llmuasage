use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Result, bail};
use chrono::{Duration, Utc};
use rusqlite::{Connection, params};
use tempfile::TempDir;

use llmusage::{paths::AppPaths, query::Dashboard, store::Store};

#[test]
fn report_commands_emit_unified_camel_case_json_from_sqlite() -> Result<()> {
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
        cache_creation_tokens: 30,
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
        daily["daily"][0]["period"].as_str(),
        Some(today_display.as_str())
    );
    assert_eq!(daily["daily"][0]["agent"].as_str(), Some("all"));
    assert_eq!(daily["totals"]["cacheCreationTokens"].as_i64(), Some(30));
    assert_eq!(daily["daily"][0]["cacheCreationTokens"].as_i64(), Some(30));
    assert_eq!(daily["totals"]["totalTokens"].as_i64(), Some(385));
    assert!(daily["daily"][0].get("date").is_none());
    assert!(daily["daily"][0].get("cache_creation_tokens").is_none());

    let daily_by_agent = fixture.json(&[
        "daily",
        "--by-agent",
        "--json",
        "--since",
        &today_arg,
        "--until",
        &today_arg,
        "--timezone",
        "UTC",
    ])?;
    let agents = daily_by_agent["daily"][0]["agents"]
        .as_array()
        .expect("daily by-agent rows");
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0]["agent"].as_str(), Some("codex"));
    assert_eq!(agents[1]["agent"].as_str(), Some("claude"));
    assert_eq!(
        agents
            .iter()
            .map(|row| row["totalTokens"].as_i64().unwrap())
            .sum::<i64>(),
        daily_by_agent["daily"][0]["totalTokens"].as_i64().unwrap()
    );

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
        monthly["monthly"][0]["period"].as_str(),
        Some(today_month.as_str())
    );
    assert_eq!(monthly["monthly"][0]["agent"].as_str(), Some("all"));
    assert_eq!(monthly["totals"]["cacheCreationTokens"].as_i64(), Some(30));
    assert_eq!(monthly["totals"]["totalTokens"].as_i64(), Some(385));

    let session = fixture.json(&[
        "session",
        "--id",
        "session-a",
        "--json",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(
        session["session"][0]["period"].as_str(),
        Some("codex:session-a")
    );
    assert_eq!(session["session"][0]["agent"].as_str(), Some("codex"));
    assert_eq!(
        session["session"][0]["cacheCreationTokens"].as_i64(),
        Some(30)
    );
    assert_eq!(session["totals"]["totalTokens"].as_i64(), Some(135));
    assert!(session["session"][0].get("agents").is_none());

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

    assert_json_has_no_camel_case_keys(&blocks);

    Ok(())
}

#[test]
fn weekly_command_uses_monday_periods_and_shared_agent_json() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    for (event_key, source, model, event_at, total_tokens) in [
        (
            "codex:weekly-monday:1",
            "codex",
            "gpt-5",
            "2025-12-29T10:00:00Z",
            10,
        ),
        (
            "claude:weekly-sunday:1",
            "claude",
            "claude-sonnet-4",
            "2026-01-04T10:00:00Z",
            20,
        ),
    ] {
        fixture.seed_event(SeedEvent {
            event_key,
            source,
            model,
            event_at,
            input_tokens: total_tokens,
            total_tokens,
            project_hash: event_key,
            project_label: event_key,
            session_id: Some(event_key),
            source_path_hash: Some(event_key),
            ..SeedEvent::default()
        })?;
    }

    let json = fixture.json(&[
        "weekly",
        "--by-agent",
        "--json",
        "--since",
        "20251229",
        "--until",
        "20260104",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(json["weekly"].as_array().map(Vec::len), Some(1));
    assert_eq!(json["weekly"][0]["period"].as_str(), Some("2025-12-29"));
    assert_eq!(json["weekly"][0]["agent"].as_str(), Some("all"));
    assert_eq!(json["weekly"][0]["totalTokens"].as_i64(), Some(30));
    assert_eq!(
        json["weekly"][0]["agents"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(json["totals"]["totalTokens"].as_i64(), Some(30));

    let output = fixture.output_with_env(
        &[
            "weekly",
            "--since",
            "20251229",
            "--until",
            "20260104",
            "--timezone",
            "UTC",
        ],
        &[("COLUMNS", "160"), ("NO_COLOR", "1")],
    )?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("Coding (Agent) CLI Usage Report - Weekly"));
    assert!(stdout.contains("Week"));
    assert!(stdout.contains("All"));
    assert!(stdout.contains("- Codex"));
    assert!(stdout.contains("- Claude"));
    assert!(!stdout.contains("2026-W01"));
    Ok(())
}

#[test]
fn no_cost_projects_all_report_output_without_changing_tokens() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    for (event_key, source, model, event_at, total_tokens) in [
        (
            "codex:no-cost:1",
            "codex",
            "gpt-5",
            "2026-05-05T10:00:00Z",
            10,
        ),
        (
            "claude:no-cost:1",
            "claude",
            "claude-sonnet-4",
            "2026-05-05T11:00:00Z",
            20,
        ),
    ] {
        fixture.seed_event(SeedEvent {
            event_key,
            source,
            model,
            event_at,
            input_tokens: total_tokens,
            total_tokens,
            cost_with_cache_usd: 1.25,
            project_hash: event_key,
            project_label: event_key,
            session_id: Some(event_key),
            source_path_hash: Some(event_key),
            ..SeedEvent::default()
        })?;
    }

    let normal = fixture.json(&[
        "daily",
        "--json",
        "--since",
        "20260505",
        "--until",
        "20260505",
        "--timezone",
        "UTC",
    ])?;
    let hidden = fixture.json(&[
        "daily",
        "--by-agent",
        "--breakdown",
        "--no-cost",
        "--json",
        "--since",
        "20260505",
        "--until",
        "20260505",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(
        normal["totals"]["totalTokens"],
        hidden["totals"]["totalTokens"]
    );
    assert_eq!(
        hidden["daily"][0]["agents"].as_array().map(Vec::len),
        Some(2)
    );
    assert_json_has_no_cost_keys(&hidden);

    for command in ["weekly", "monthly", "session", "blocks"] {
        let json = fixture.json(&[
            command,
            "--no-cost",
            "--json",
            "--since",
            "20260505",
            "--until",
            "20260505",
            "--timezone",
            "UTC",
        ])?;
        assert_json_has_no_cost_keys(&json);
    }

    let instances = fixture.json(&[
        "daily",
        "--instances",
        "--no-cost",
        "--json",
        "--since",
        "20260505",
        "--until",
        "20260505",
        "--timezone",
        "UTC",
    ])?;
    assert_json_has_no_cost_keys(&instances);

    let daily_text = fixture.output_with_env(
        &[
            "daily",
            "--no-cost",
            "--since",
            "20260505",
            "--until",
            "20260505",
            "--timezone",
            "UTC",
        ],
        &[("COLUMNS", "160"), ("NO_COLOR", "1")],
    )?;
    let daily_stdout = String::from_utf8(daily_text.stdout)?;
    assert!(!daily_stdout.contains("Cost (USD)"));
    assert!(daily_stdout.contains("Total Tokens"));
    assert!(daily_stdout.contains("- Codex"));

    let weekly_text = fixture.output_with_env(
        &[
            "weekly",
            "--compact",
            "--no-cost",
            "--since",
            "20260505",
            "--until",
            "20260505",
            "--timezone",
            "UTC",
        ],
        &[("NO_COLOR", "1")],
    )?;
    let weekly_stdout = String::from_utf8(weekly_text.stdout)?;
    assert!(!weekly_stdout.contains("Cost (USD)"));
    assert!(weekly_stdout.contains("Agent"));
    assert!(weekly_stdout.contains("Input"));
    Ok(())
}

#[test]
fn sections_output_keeps_current_period_first_and_flattens_json() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    for (event_key, source, model, total_tokens) in [
        ("codex:sections:1", "codex", "gpt-5", 10),
        ("claude:sections:1", "claude", "claude-sonnet-4", 20),
    ] {
        fixture.seed_event(SeedEvent {
            event_key,
            source,
            model,
            event_at: "2026-05-05T12:00:00Z",
            input_tokens: total_tokens,
            total_tokens,
            cost_with_cache_usd: 1.0,
            project_hash: event_key,
            project_label: event_key,
            session_id: Some(event_key),
            source_path_hash: Some(event_key),
            ..SeedEvent::default()
        })?;
    }

    let output = fixture.output(&[
        "monthly",
        "--sections",
        "daily,daily,session",
        "--by-agent",
        "--json",
        "--since",
        "20260505",
        "--until",
        "20260505",
        "--timezone",
        "UTC",
    ])?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout)?;
    let monthly_index = stdout.find("\"monthly\"").unwrap();
    let daily_index = stdout.find("\"daily\"").unwrap();
    let session_index = stdout.find("\"session\"").unwrap();
    let totals_index = stdout.rfind("\"totals\"").unwrap();
    assert!(
        monthly_index < daily_index && daily_index < session_index && session_index < totals_index
    );
    assert_eq!(stdout.matches("\"daily\"").count(), 1);

    let json: serde_json::Value = serde_json::from_str(&stdout)?;
    assert_eq!(json["monthly"][0]["period"].as_str(), Some("2026-05"));
    assert_eq!(json["daily"][0]["period"].as_str(), Some("2026-05-05"));
    assert_eq!(json["totals"]["totalTokens"].as_i64(), Some(30));
    assert!(json["monthly"][0]["agents"].is_array());
    assert!(json["daily"][0]["agents"].is_array());
    assert!(json["session"][0].get("agents").is_none());
    assert!(json["monthly"][0].get("totals").is_none());

    let no_cost = fixture.json(&[
        "daily",
        "--sections",
        "monthly,session",
        "--by-agent",
        "--no-cost",
        "--json",
        "--since",
        "20260505",
        "--until",
        "20260505",
        "--timezone",
        "UTC",
    ])?;
    assert_json_has_no_cost_keys(&no_cost);
    assert_eq!(no_cost["totals"]["totalTokens"].as_i64(), Some(30));

    let text = fixture.output_with_env(
        &[
            "daily",
            "--sections",
            "daily,monthly,session",
            "--since",
            "20260505",
            "--until",
            "20260505",
            "--timezone",
            "UTC",
        ],
        &[("COLUMNS", "160"), ("NO_COLOR", "1")],
    )?;
    assert!(text.status.success(), "{text:?}");
    let text = String::from_utf8(text.stdout)?;
    let daily_title = text.find("Report - Daily").unwrap();
    let monthly_title = text.find("Report - Monthly").unwrap();
    let session_title = text.find("Report - Session").unwrap();
    assert!(daily_title < monthly_title && monthly_title < session_title);

    let invalid = fixture.output(&["daily", "--sections", "invalid"])?;
    assert!(!invalid.status.success());
    let invalid_stderr = String::from_utf8(invalid.stderr)?;
    assert!(
        invalid_stderr.contains("possible values"),
        "{invalid_stderr}"
    );
    Ok(())
}

#[test]
fn focused_source_reports_match_source_filters_without_comparison_fields() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    for (event_key, source, model, total_tokens) in [
        ("claude:focused:1", "claude", "claude-sonnet-4", 20),
        ("codex:focused:1", "codex", "gpt-5", 10),
        ("opencode:focused:1", "opencode", "gpt-5-mini", 30),
        ("antigravity:focused:1", "antigravity", "gemini-2.5-pro", 40),
    ] {
        fixture.seed_event(SeedEvent {
            event_key,
            source,
            model,
            event_at: "2026-05-05T12:00:00Z",
            input_tokens: total_tokens,
            total_tokens,
            cost_with_cache_usd: 1.25,
            project_hash: event_key,
            project_label: event_key,
            session_id: Some(event_key),
            source_path_hash: Some(event_key),
            ..SeedEvent::default()
        })?;
    }

    let filtered = fixture.json(&[
        "daily",
        "--source",
        "claude",
        "--by-agent",
        "--json",
        "--since",
        "20260505",
        "--until",
        "20260505",
        "--timezone",
        "UTC",
    ])?;
    let focused = fixture.json(&[
        "claude",
        "daily",
        "--json",
        "--since",
        "20260505",
        "--until",
        "20260505",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(focused["totals"], filtered["totals"]);
    assert_eq!(
        focused["daily"][0]["totalTokens"],
        filtered["daily"][0]["totalTokens"]
    );
    assert_json_has_no_agent_keys(&focused);

    for (source, total_tokens) in [
        ("claude", 20),
        ("codex", 10),
        ("opencode", 30),
        ("antigravity", 40),
    ] {
        for period in ["daily", "weekly", "monthly", "session"] {
            let json = fixture.json(&[
                source,
                period,
                "--json",
                "--since",
                "20260505",
                "--until",
                "20260505",
                "--timezone",
                "UTC",
            ])?;
            assert_eq!(
                json["totals"]["totalTokens"].as_i64(),
                Some(total_tokens),
                "{source} {period} should apply its source filter"
            );
            assert_json_has_no_agent_keys(&json);
        }
    }

    let sections = fixture.json(&[
        "codex",
        "monthly",
        "--sections",
        "daily,weekly,session",
        "--no-cost",
        "--json",
        "--since",
        "20260505",
        "--until",
        "20260505",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(sections["totals"]["totalTokens"].as_i64(), Some(10));
    assert_json_has_no_agent_keys(&sections);
    assert_json_has_no_cost_keys(&sections);

    let text = fixture.output_with_env(
        &[
            "claude",
            "daily",
            "--no-cost",
            "--since",
            "20260505",
            "--until",
            "20260505",
            "--timezone",
            "UTC",
        ],
        &[("COLUMNS", "160"), ("NO_COLOR", "1")],
    )?;
    assert!(text.status.success(), "{text:?}");
    let text = String::from_utf8(text.stdout)?;
    assert!(text.contains("Claude Usage Report - Daily"));
    assert!(!text.contains("Agent"));
    assert!(!text.contains("Detected:"));
    assert!(!text.contains("Cost (USD)"));

    let same_source = fixture.output(&[
        "claude",
        "daily",
        "--source",
        "claude",
        "--json",
        "--since",
        "20260505",
        "--until",
        "20260505",
        "--timezone",
        "UTC",
    ])?;
    assert!(same_source.status.success(), "{same_source:?}");

    let conflict = fixture.output(&[
        "claude",
        "daily",
        "--source",
        "codex",
        "--json",
        "--since",
        "20260505",
        "--until",
        "20260505",
        "--timezone",
        "UTC",
    ])?;
    assert!(!conflict.status.success());
    assert!(
        String::from_utf8(conflict.stderr)?.contains("conflicts with `--source codex`"),
        "focused source conflict should explain the incompatible filter"
    );

    let instances = fixture.output(&["codex", "daily", "--instances"])?;
    assert!(!instances.status.success());
    assert!(
        String::from_utf8(instances.stderr)?.contains("--instances is not supported"),
        "focused daily instances should be rejected explicitly"
    );
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
    assert_eq!(default_daily["totals"]["totalTokens"].as_i64(), Some(70));

    let all_daily = fixture.json(&["--all", "--json", "--timezone", "UTC"])?;
    assert_eq!(all_daily["daily"].as_array().map(Vec::len), Some(8));
    assert_eq!(all_daily["totals"]["totalTokens"].as_i64(), Some(80));

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
        explicit_old_range["totals"]["totalTokens"].as_i64(),
        Some(10)
    );

    Ok(())
}

#[test]
fn report_date_filters_accept_iso_and_compact_forms_equivalently() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:date-format:1",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-04-25T12:00:00Z",
        input_tokens: 10,
        total_tokens: 10,
        project_hash: "date-format",
        project_label: "Date Format",
        session_id: Some("date-format"),
        source_path_hash: Some("date-format"),
        ..SeedEvent::default()
    })?;

    let compact = fixture.json(&[
        "daily",
        "--json",
        "--since",
        "20260425",
        "--until",
        "20260425",
        "--timezone",
        "UTC",
    ])?;
    let iso = fixture.json(&[
        "daily",
        "--json",
        "--since",
        "2026-04-25",
        "--until",
        "2026-04-25",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(compact, iso);

    let invalid = fixture.output(&["daily", "--since", "2026/04/25", "--timezone", "UTC"])?;
    assert!(!invalid.status.success());
    let stderr = String::from_utf8(invalid.stderr)?;
    assert!(stderr.contains("YYYY-MM-DD or YYYYMMDD"), "{stderr}");

    let help = fixture.output(&["daily", "--help"])?;
    let help_stdout = String::from_utf8(help.stdout)?;
    assert!(help_stdout.contains("YYYY-MM-DD|YYYYMMDD"));
    Ok(())
}

#[test]
fn daily_human_output_uses_aggregate_ccusage_style_columns_and_no_default_info_logs() -> Result<()>
{
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
        cache_creation_tokens: 333_333,
        cache_read_tokens: 5_370_000,
        output_tokens: 40_330_000_000,
        reasoning_output_tokens: 12_345,
        total_tokens: 40_336_693_728,
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
        &[("COLUMNS", "160"), ("NO_COLOR", "1")],
    )?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stdout.contains("Coding (Agent) CLI Usage Report - Daily"));
    assert!(stdout.contains("Detected: Codex, Claude"));
    assert!(!stdout.contains("Codex daily usage"));
    assert!(!stdout.contains("Claude daily usage"));
    assert!(!stdout.contains("---\nClaude daily usage"));
    assert!(stdout.contains(&today_display));
    assert!(stdout.contains(&six_days_ago_display));
    assert!(stdout.contains('\u{250C}'));
    assert!(stdout.contains("Agent"));
    assert!(stdout.contains("All"));
    assert!(stdout.contains("- Codex"));
    assert!(stdout.contains("- Claude"));
    assert!(stdout.contains("Cache Create"));
    assert!(stdout.contains("Cache Read"));
    assert!(stdout.contains("Total Tokens"));
    assert!(stdout.contains("Cost (USD)"));
    assert!(!stdout.contains("Conv"));
    assert!(!stdout.contains("Reason"));
    assert!(!stdout.contains("Notes"));
    assert!(!stdout.contains("unpriced"));
    assert!(!stdout.contains("reason not reported"));
    assert!(stdout.contains("gpt-5.4"));
    assert!(stdout.contains("sonnet-4"));
    assert!(stdout.contains("6.35M"));
    assert!(stdout.contains("333.33K"));
    assert!(stdout.contains("40.33B"));
    assert!(stdout.contains("40.34B"));
    assert!(!stdout.contains("40,343,165,778"));
    assert!(!stdout.contains("40,343,167,778"));
    assert!(stdout.contains("Total"));
    assert!(stdout.contains('\u{255E}'));
    assert!(stdout.contains('\u{2550}'));
    assert!(stdout.contains('\u{2561}'));
    assert!(!stdout.contains("Total:"));
    assert!(stdout.contains("978.05K"));
    assert!(stdout.contains("5.37M"));
    assert!(!stdout.contains("\u{1b}["));
    assert!(!stderr.contains("INFO"));
    assert!(!stderr.contains("开始初始化本地目录与 SQLite schema"));

    let colored =
        fixture.output_with_env(&["--timezone", "UTC"], &[("LLMUSAGE_FORCE_COLOR", "1")])?;
    assert!(colored.status.success(), "{colored:?}");
    let colored_stdout = String::from_utf8(colored.stdout)?;
    assert!(colored_stdout.contains("\u{1b}["));
    assert!(colored_stdout.contains("Coding (Agent) CLI Usage Report - Daily"));

    Ok(())
}

#[test]
fn logging_runtime_writes_ndjson_file() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    let output = fixture.output_with_env(
        &["doctor"],
        &[("LLMUSAGE_LOG", "info"), ("RUST_LOG", "off")],
    )?;
    assert!(output.status.success(), "{output:?}");

    let entries = read_log_json_lines(&fixture.paths.log_file_path)?;
    assert!(
        entries.iter().any(|entry| {
            entry["level"] == "INFO"
                && entry["fields"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("doctor"))
        }),
        "expected INFO doctor event in {}: {entries:#?}",
        fixture.paths.log_file_path.display()
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
    assert!(fixture.paths.log_file_path.is_file());
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
        ..SeedEvent::default()
    })?;

    let daily = fixture.json(&["daily", "--all", "--json", "--timezone", "UTC"])?;
    assert_eq!(daily["totals"]["totalCost"].as_f64(), Some(42.5));
    assert_eq!(daily["daily"][0]["totalCost"].as_f64(), Some(42.5));
    Ok(())
}

#[test]
fn report_help_and_legacy_help_still_parse() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    for args in [
        vec!["--help"],
        vec!["-h"],
        vec!["help"],
        vec!["help", "--zh"],
        vec!["help", "daily"],
        vec!["daily", "--help"],
        vec!["monthly", "--help"],
        vec!["session", "--help"],
        vec!["blocks", "--help"],
        vec!["statusline", "--help"],
        vec!["source-status", "--help"],
        vec!["export", "html", "--help"],
    ] {
        let output = fixture.output(&args)?;
        assert!(output.status.success(), "{args:?}: {output:?}");
    }

    for args in [
        ["--help"].as_slice(),
        ["-h"].as_slice(),
        ["help"].as_slice(),
    ] {
        let output = fixture.output(args)?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("┌"), "{args:?}: {stdout}");
        assert!(stdout.contains("│ Command"), "{args:?}: {stdout}");
        assert!(stdout.contains("│ Option"), "{args:?}: {stdout}");
        assert!(stdout.contains("Report options:"), "{args:?}: {stdout}");
        assert!(stdout.contains("│ Goal"), "{args:?}: {stdout}");
        assert!(stdout.contains("llmusage help --zh"), "{args:?}: {stdout}");
        assert!(!stdout.contains("| --- |"), "{args:?}: {stdout}");
    }

    let zh_help = fixture.output(&["help", "--zh"])?;
    let zh_help_stdout = String::from_utf8(zh_help.stdout)?;
    assert!(zh_help_stdout.contains("┌"));
    assert!(zh_help_stdout.contains("│ 命令"));
    assert!(zh_help_stdout.contains("全局参数"));
    assert!(zh_help_stdout.contains("报表参数"));
    assert!(zh_help_stdout.contains("示例"));
    assert!(zh_help_stdout.contains("llmusage help daily"));

    let fresh_home = fixture.home.join("fresh-help-home");
    fs::create_dir_all(&fresh_home)?;
    let help_home = fresh_home.to_string_lossy().into_owned();
    let top_help_without_runtime =
        fixture.output_with_env(&["help"], &[("LLMUSAGE_HOME", &help_home)])?;
    assert!(top_help_without_runtime.status.success());
    assert!(
        !fresh_home.join("llmusage.db").exists(),
        "top-level help should not initialize the database"
    );

    let daily_help = fixture.output(&["daily", "--help"])?;
    let daily_help_stdout = String::from_utf8(daily_help.stdout)?;
    assert!(daily_help_stdout.contains("last 7 days"));
    assert!(daily_help_stdout.contains("Usage: llmusage"));

    let legacy_help = fixture.output(&["help", "daily"])?;
    let legacy_help_stdout = String::from_utf8(legacy_help.stdout)?;
    assert!(legacy_help_stdout.contains("last 7 days"));
    assert!(legacy_help_stdout.contains("Usage: llmusage"));
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

#[test]
fn cli_reports_use_camel_case_without_changing_other_json_surfaces() -> Result<()> {
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
        cache_creation_tokens: 4,
        cache_read_tokens: 1,
        output_tokens: 2,
        reasoning_output_tokens: 3,
        total_tokens: 20,
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
    ] {
        let json = fixture.json(&args)?;
        assert!(
            json["totals"].get("totalTokens").is_some(),
            "{args:?}: {json:#}"
        );
        assert!(
            json["totals"].get("total_tokens").is_none(),
            "{args:?}: {json:#}"
        );
    }

    let blocks = fixture.json(&["blocks", "--json", "--timezone", "UTC"])?;
    assert_json_has_no_camel_case_keys(&blocks);

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

fn assert_json_has_no_cost_keys(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                assert!(
                    !key.to_ascii_lowercase().contains("cost"),
                    "JSON key should not expose cost, got {key}"
                );
                assert_json_has_no_cost_keys(value);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                assert_json_has_no_cost_keys(item);
            }
        }
        _ => {}
    }
}

fn assert_json_has_no_agent_keys(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                assert!(
                    key != "agent" && key != "agents",
                    "focused JSON should not expose comparison field {key}"
                );
                assert_json_has_no_agent_keys(value);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                assert_json_has_no_agent_keys(item);
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
        .map(|row| {
            row["period"]
                .as_str()
                .expect("daily row period")
                .to_string()
        })
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
        std::fs::create_dir_all(&home)?;
        let paths = AppPaths::with_root(root_dir)?;
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
                event_key, source, provider_label, model, event_at, hour_start,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                project_hash, project_label, project_ref, path_hash,
                session_id, session_label, source_path_hash, created_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11,
                ?12, ?13, ?14, ?15,
                ?16, ?17, ?18, ?19,
                ?20, ?20, ?21, ?5
            )
            "#,
            params![
                event.event_key,
                event.source,
                event.provider_label,
                event.model,
                event.event_at,
                event.input_tokens,
                event.cache_read_tokens,
                event.cache_creation_tokens,
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
                source, provider_label, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                event_count, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 1, ?4)
            ON CONFLICT(source, provider_label, model, hour_start, project_hash) DO UPDATE SET
                input_tokens = input_tokens + excluded.input_tokens,
                cache_read_tokens = cache_read_tokens + excluded.cache_read_tokens,
                cache_creation_tokens = cache_creation_tokens + excluded.cache_creation_tokens,
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
                event.provider_label,
                event.model,
                event.event_at,
                event.project_hash,
                event.project_label,
                event.project_ref,
                event.input_tokens,
                event.cache_read_tokens,
                event.cache_creation_tokens,
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
        self.json_with_env(args, &[])
    }

    fn json_with_env(&self, args: &[&str], envs: &[(&str, &str)]) -> Result<serde_json::Value> {
        let output = self.output_with_env(args, envs)?;
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

fn read_log_json_lines(path: &Path) -> Result<Vec<serde_json::Value>> {
    let raw = fs::read_to_string(path)?;
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

struct SeedEvent<'a> {
    event_key: &'a str,
    source: &'a str,
    provider_label: &'a str,
    model: &'a str,
    event_at: &'a str,
    input_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
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
            provider_label: "",
            model: "gpt-5",
            event_at: "2026-05-01T00:00:00Z",
            input_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
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
