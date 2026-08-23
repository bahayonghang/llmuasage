use super::*;

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
fn host_filter_keeps_totals_consistent_and_lists_unknown_labels() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fixture.upsert_host("devbox", "devbox")?;
    fixture.seed_event(SeedEvent {
        event_key: "local:codex:host-filter:1",
        host_id: "local",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-08-20T10:00:00Z",
        input_tokens: 100,
        total_tokens: 100,
        project_hash: "project-local",
        project_label: "Local",
        session_id: Some("session-local"),
        source_path_hash: Some("path-local"),
        ..SeedEvent::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "devbox:codex:host-filter:1",
        host_id: "devbox",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-08-20T11:00:00Z",
        input_tokens: 40,
        total_tokens: 40,
        project_hash: "project-devbox",
        project_label: "Devbox",
        session_id: Some("session-devbox"),
        source_path_hash: Some("path-devbox"),
        ..SeedEvent::default()
    })?;

    let all = fixture.json(&[
        "daily",
        "--json",
        "--by-agent",
        "--since",
        "20260820",
        "--until",
        "20260820",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(all["totals"]["totalTokens"].as_i64(), Some(140));
    let hosts = all["hosts"].as_array().expect("hosts rows");
    assert_eq!(hosts.len(), 2);
    assert!(all["daily"][0]["agents"].as_array().is_some());
    let host_sum = hosts
        .iter()
        .map(|row| row["totalTokens"].as_i64().unwrap())
        .sum::<i64>();
    assert_eq!(host_sum, 140);
    assert!(
        hosts
            .iter()
            .any(|row| row["host"] == "local" && row["totalTokens"] == 100)
    );
    assert!(
        hosts
            .iter()
            .any(|row| row["host"] == "devbox" && row["totalTokens"] == 40)
    );

    let local = fixture.json(&[
        "daily",
        "--json",
        "--host",
        "local",
        "--since",
        "20260820",
        "--until",
        "20260820",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(local["totals"]["totalTokens"].as_i64(), Some(100));
    assert_eq!(local["hosts"].as_array().map(Vec::len), Some(1));
    assert_eq!(local["hosts"][0]["host"], "local");

    let remote = fixture.json(&[
        "daily",
        "--json",
        "--host",
        "devbox",
        "--since",
        "20260820",
        "--until",
        "20260820",
        "--timezone",
        "UTC",
    ])?;
    assert_eq!(remote["totals"]["totalTokens"].as_i64(), Some(40));

    let unknown = fixture.output(&[
        "daily",
        "--json",
        "--host",
        "missing-host",
        "--timezone",
        "UTC",
    ])?;
    assert!(!unknown.status.success());
    let stderr = String::from_utf8(unknown.stderr)?;
    assert!(
        stderr.contains("unknown host label 'missing-host'"),
        "{stderr}"
    );
    assert!(stderr.contains("registered labels:"), "{stderr}");
    assert!(stderr.contains("local"), "{stderr}");
    assert!(stderr.contains("devbox"), "{stderr}");

    let store = Store::new(&fixture.paths)?;
    let dashboard = Dashboard::open(&store)?;
    let filter = QueryFilter {
        since: Some(chrono::NaiveDate::from_ymd_opt(2026, 8, 20).unwrap()),
        until: Some(chrono::NaiveDate::from_ymd_opt(2026, 8, 20).unwrap()),
        timezone: ReportTimezone::Utc,
        ..Default::default()
    };
    let host_rows = dashboard.host_breakdown(&filter)?;
    assert_eq!(host_rows.len(), 2);
    let dashboard_sum = host_rows.iter().map(|row| row.total_tokens).sum::<i64>();
    assert_eq!(dashboard_sum, 140);
    let local_dash = host_rows.iter().find(|row| row.label == "local").unwrap();
    assert_eq!(local_dash.total_tokens, 100);
    assert_eq!(
        local_dash.total_tokens,
        local["totals"]["totalTokens"].as_i64().unwrap()
    );
    Ok(())
}

#[test]
fn host_filter_applies_to_activity_and_tools() -> Result<()> {
    let fixture = ReportCliFixture::new()?;
    fixture.upsert_host("devbox", "devbox")?;
    fixture.seed_event(SeedEvent {
        event_key: "local:codex:behavior:1",
        host_id: "local",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-08-20T10:00:00Z",
        input_tokens: 10,
        total_tokens: 10,
        project_hash: "project-a",
        project_label: "Project A",
        session_id: Some("session-a"),
        source_path_hash: Some("path-a"),
        ..SeedEvent::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "devbox:codex:behavior:1",
        host_id: "devbox",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-08-20T11:00:00Z",
        input_tokens: 20,
        total_tokens: 20,
        project_hash: "project-b",
        project_label: "Project B",
        session_id: Some("session-b"),
        source_path_hash: Some("path-b"),
        ..SeedEvent::default()
    })?;

    let conn = Connection::open(&fixture.paths.db_path)?;
    conn.execute(
        r#"
        INSERT INTO usage_turn(
            turn_key, host_id, source, session_id, source_path_hash, project_hash,
            primary_model, started_at, category, has_edits, retries,
            one_shot, call_count, input_tokens, cache_read_tokens,
            cache_creation_tokens, output_tokens, reasoning_output_tokens,
            total_tokens, created_at
        ) VALUES
            ('turn:local:codex:behavior:1', 'local', 'codex', 'session-a', 'path-a', 'project-a',
             'gpt-5', '2026-08-20T10:00:00Z', 'coding', 1, 0, 1, 1, 10, 0, 0, 0, 0, 10,
             '2026-08-20T10:00:00Z'),
            ('turn:devbox:codex:behavior:1', 'devbox', 'codex', 'session-b', 'path-b', 'project-b',
             'gpt-5', '2026-08-20T11:00:00Z', 'debugging', 0, 0, 0, 1, 20, 0, 0, 0, 0, 20,
             '2026-08-20T11:00:00Z')
        "#,
        [],
    )?;
    conn.execute(
        r#"
        INSERT INTO usage_tool_call(
            tool_call_key, host_id, turn_key, event_key, source, session_id,
            source_path_hash, project_hash, model, occurred_at, tool_name,
            tool_kind, created_at
        ) VALUES
            ('tool:local:1', 'local', 'turn:local:codex:behavior:1', 'local:codex:behavior:1',
             'codex', 'session-a', 'path-a', 'project-a', 'gpt-5', '2026-08-20T10:00:00Z',
             'Read', 'read', '2026-08-20T10:00:00Z'),
            ('tool:devbox:1', 'devbox', 'turn:devbox:codex:behavior:1', 'devbox:codex:behavior:1',
             'codex', 'session-b', 'path-b', 'project-b', 'gpt-5', '2026-08-20T11:00:00Z',
             'Edit', 'edit', '2026-08-20T11:00:00Z')
        "#,
        [],
    )?;
    drop(conn);

    let store = Store::new(&fixture.paths)?;
    let dashboard = Dashboard::open(&store)?;
    let local = QueryFilter {
        host_id: Some("local".to_string()),
        timezone: ReportTimezone::Utc,
        ..Default::default()
    };
    let activity = dashboard.activity_breakdown(&local)?;
    assert!(activity.support.supported);
    assert_eq!(activity.breakdown.len(), 1);
    assert_eq!(activity.breakdown[0].category, "coding");
    assert_eq!(activity.breakdown[0].turns, 1);

    let tools = dashboard.tool_breakdown(&local)?;
    assert!(tools.support.supported);
    assert!(
        tools.breakdown.iter().any(|row| row.tool_name == "Read"),
        "{tools:?}"
    );
    assert!(
        tools.breakdown.iter().all(|row| row.tool_name != "Edit"),
        "{tools:?}"
    );
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
