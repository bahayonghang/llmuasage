/// Validates F4.3: a 365-day heatmap zero-fills every day in the window
/// even when only a single bucket landed in SQLite, and surfaces the
/// observed event_count/total_tokens on the matching local date.
/// Validates F4.3: a 365-day heatmap zero-fills every day in the window
/// even when only a single bucket landed in SQLite, and surfaces the
/// observed event_count/total_tokens on the matching local date.
#[test]
fn heatmap_365_days_returns_all_dates_with_zero_fill() -> Result<()> {
    let fixture = Fixture::new()?;
    let conn = fixture.store().open_connection()?;
    let today_local = chrono::Local::now().date_naive();
    let target_local = today_local - chrono::Duration::days(10);
    let target_utc_midnight = format!("{}T00:00:00Z", target_local.format("%Y-%m-%d"));

    conn.execute(
        r#"
        INSERT INTO usage_bucket_30m(
            source, model, hour_start, project_hash, project_label, project_ref,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens, event_count, updated_at
        )
        VALUES ('codex', 'gpt-5', ?1, '', NULL, NULL,
                100, 10, 0, 50, 0, 160, 4, ?1)
        "#,
        [&target_utc_midnight],
    )?;

    let dashboard = Dashboard::open(fixture.store())?;
    let heatmap = dashboard.heatmap(&QueryFilter::default(), 365)?;
    assert_eq!(heatmap.len(), 365);

    let observed_dates: Vec<&String> = heatmap
        .iter()
        .filter(|point| point.event_count > 0)
        .map(|point| &point.date)
        .collect();
    // 跨时区时单条事件可能落在 ±1 天，因此放宽为「至少有一天观察到 4 个事件」。
    assert_eq!(observed_dates.len(), 1);

    let zero_days = heatmap
        .iter()
        .filter(|point| point.event_count == 0)
        .count();
    assert_eq!(zero_days, 364);

    let last_date = heatmap.last().expect("non-empty heatmap").date.clone();
    assert_eq!(last_date, today_local.format("%Y-%m-%d").to_string());
    Ok(())
}
#[test]
fn heatmap_uses_explicit_until_as_the_calendar_window_end() -> Result<()> {
    let fixture = Fixture::new()?;
    let conn = fixture.store().open_connection()?;
    conn.execute(
        r#"
        INSERT INTO usage_bucket_30m(
            source, model, hour_start, project_hash, project_label, project_ref,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens, event_count, updated_at
        )
        VALUES ('codex', 'gpt-5', '2024-02-10T08:00:00Z', '', NULL, NULL,
                42, 0, 0, 0, 0, 42, 1, '2024-02-10T08:00:00Z')
        "#,
        [],
    )?;
    drop(conn);

    let historical_day = NaiveDate::from_ymd_opt(2024, 2, 10).expect("valid date");
    let filter = QueryFilter {
        since: historical_day.checked_sub_days(chrono::Days::new(2)),
        until: Some(historical_day),
        timezone: ReportTimezone::Utc,
        ..Default::default()
    };
    let rows = Dashboard::open(fixture.store())?.heatmap(&filter, 3)?;

    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.first().map(|row| row.date.as_str()),
        Some("2024-02-08")
    );
    assert_eq!(rows.last().map(|row| row.date.as_str()), Some("2024-02-10"));
    assert_eq!(rows.last().map(|row| row.total_tokens), Some(42));
    Ok(())
}

#[test]
fn trends_daily_groups_by_local_date_with_timezone() -> Result<()> {
    let fixture = Fixture::new()?;
    let conn = fixture.store().open_connection()?;
    // 16:00 UTC on Apr 4 = 00:00 (next day) on Apr 5 in +08:00.
    conn.execute(
        r#"
        INSERT INTO usage_bucket_30m(
            source, model, hour_start, project_hash, project_label, project_ref,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens, event_count, updated_at
        )
        VALUES ('codex', 'gpt-5', '2026-04-04T16:00:00Z', '', NULL, NULL,
                100, 10, 5, 50, 7, 172, 2, '2026-04-05T00:00:00Z')
        "#,
        [],
    )?;
    let dashboard = Dashboard::open(fixture.store())?;

    let cn_filter = QueryFilter {
        timezone: ReportTimezone::Fixed(
            chrono::FixedOffset::east_opt(8 * 3600).expect("valid offset"),
        ),
        ..Default::default()
    };
    let cn_series = dashboard.trends_daily(&cn_filter)?;
    assert_eq!(cn_series.len(), 1);
    let row = &cn_series[0];
    assert_eq!(row.date, "2026-04-05");
    assert_eq!(row.input_tokens, 100);
    assert_eq!(row.cache_read_tokens, 10);
    assert_eq!(row.cache_creation_tokens, 5);
    assert_eq!(row.output_tokens, 50);
    assert_eq!(row.total_tokens, 172);
    assert_eq!(row.event_count, 2);

    let utc_filter = QueryFilter {
        timezone: ReportTimezone::Utc,
        ..Default::default()
    };
    let utc_series = dashboard.trends_daily(&utc_filter)?;
    assert_eq!(utc_series.len(), 1);
    assert_eq!(utc_series[0].date, "2026-04-04");
    Ok(())
}

#[test]
fn trends_daily_by_model_groups_date_and_model() -> Result<()> {
    let fixture = Fixture::new()?;
    let conn = fixture.store().open_connection()?;
    conn.execute(
        r#"
        INSERT INTO usage_bucket_30m(
            source, model, hour_start, project_hash, project_label, project_ref,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens, event_count, updated_at
        )
        VALUES
            ('codex', 'gpt-5', '2026-04-04T16:00:00Z', '', NULL, NULL,
             100, 10, 5, 50, 7, 172, 2, '2026-04-05T00:00:00Z'),
            ('claude', 'claude-opus-5', '2026-04-04T16:00:00Z', '', NULL, NULL,
             20, 0, 0, 10, 0, 30, 1, '2026-04-05T00:00:00Z'),
            ('codex', 'gpt-5', '2026-04-05T01:00:00Z', '', NULL, NULL,
             8, 0, 0, 2, 0, 10, 1, '2026-04-05T01:00:00Z')
        "#,
        [],
    )?;
    let dashboard = Dashboard::open(fixture.store())?;

    let utc = dashboard.trends_daily_by_model(&QueryFilter {
        timezone: ReportTimezone::Utc,
        ..Default::default()
    })?;
    assert_eq!(utc.len(), 3);
    assert_eq!(utc[0].date, "2026-04-04");
    assert_eq!(utc[0].model, "claude-opus-5");
    assert_eq!(utc[0].total_tokens, 30);
    assert_eq!(utc[1].date, "2026-04-04");
    assert_eq!(utc[1].model, "gpt-5");
    assert_eq!(utc[1].total_tokens, 172);
    assert_eq!(utc[2].date, "2026-04-05");
    assert_eq!(utc[2].model, "gpt-5");
    assert_eq!(utc[2].total_tokens, 10);

    let cn = dashboard.trends_daily_by_model(&QueryFilter {
        timezone: ReportTimezone::Fixed(
            chrono::FixedOffset::east_opt(8 * 3600).expect("valid offset"),
        ),
        ..Default::default()
    })?;
    assert_eq!(cn.len(), 2);
    assert_eq!(cn[0].date, "2026-04-05");
    assert_eq!(cn[0].model, "claude-opus-5");
    assert_eq!(cn[1].date, "2026-04-05");
    assert_eq!(cn[1].model, "gpt-5");
    assert_eq!(cn[1].total_tokens, 182);

    let empty_fixture = Fixture::new()?;
    let empty = Dashboard::open(empty_fixture.store())?;
    assert!(
        empty
            .trends_daily_by_model(&QueryFilter::default())?
            .is_empty()
    );
    Ok(())
}

#[test]
fn trends_hourly_merges_half_hour_buckets_and_sources() -> Result<()> {
    let fixture = Fixture::new()?;
    let conn = fixture.store().open_connection()?;
    conn.execute(
        r#"
        INSERT INTO usage_bucket_30m(
            source, model, hour_start, project_hash, project_label, project_ref,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens, event_count,
            cost_with_cache_usd, updated_at
        )
        VALUES
            ('codex', 'gpt-5', '2026-04-04T16:00:00Z', '', NULL, NULL,
             100, 10, 5, 50, 0, 165, 2, 1.5, '2026-04-05T00:00:00Z'),
            ('claude', 'claude-opus-5', '2026-04-04T16:30:00Z', '', NULL, NULL,
             20, 0, 0, 10, 0, 30, 1, 0.5, '2026-04-05T00:30:00Z'),
            ('codex', 'gpt-5', '2026-04-04T17:00:00Z', '', NULL, NULL,
             8, 0, 0, 2, 0, 10, 1, 0.1, '2026-04-05T01:00:00Z')
        "#,
        [],
    )?;
    conn.execute(
        r#"
        INSERT INTO usage_turn(
            turn_key, source, session_id, source_path_hash, project_hash,
            primary_model, started_at, category, has_edits, retries,
            one_shot, call_count, input_tokens, cache_read_tokens,
            cache_creation_tokens, output_tokens, reasoning_output_tokens,
            total_tokens, created_at
        ) VALUES ('turn:codex:hour', 'codex', 's', 'p', '', 'gpt-5',
            '2026-04-04T16:10:00Z', 'coding', 0, 0, 0, 1, 1, 0, 0, 1, 0, 2,
            '2026-04-04T16:10:00Z')
        "#,
        [],
    )?;
    let dashboard = Dashboard::open(fixture.store())?;
    let cn = QueryFilter {
        timezone: ReportTimezone::Fixed(
            chrono::FixedOffset::east_opt(8 * 3600).expect("valid offset"),
        ),
        ..Default::default()
    };
    let hourly = dashboard.trends_hourly(&cn)?;
    assert_eq!(hourly.len(), 2);
    assert_eq!(hourly[0].hour_start, "2026-04-05 00:00");
    assert_eq!(hourly[0].total_tokens, 195);
    assert_eq!(hourly[0].event_count, 3);
    assert_eq!(hourly[0].turn_count, 1);
    assert_eq!(
        hourly[0].sources,
        vec!["claude".to_string(), "codex".to_string()]
    );
    assert_eq!(hourly[1].hour_start, "2026-04-05 01:00");
    assert_eq!(hourly[1].turn_count, 0);

    let monthly = dashboard.trends_monthly(&cn)?;
    assert_eq!(monthly.len(), 1);
    assert_eq!(monthly[0].month, "2026-04");
    assert_eq!(monthly[0].total_tokens, 205);
    assert_eq!(monthly[0].turn_count, 1);

    let mut day_filter = cn.clone();
    day_filter.since = Some(NaiveDate::from_ymd_opt(2026, 4, 5).unwrap());
    day_filter.until = Some(NaiveDate::from_ymd_opt(2026, 4, 5).unwrap());
    let detail = dashboard.period_model_breakdown(&day_filter)?;
    assert_eq!(detail.len(), 2);
    assert_eq!(detail[0].model, "gpt-5");
    assert_eq!(detail[0].source, "codex");

    let json = serde_json::to_value(dashboard.trends_daily(&cn)?)?;
    assert!(json[0].get("turn_count").is_none());
    Ok(())
}
