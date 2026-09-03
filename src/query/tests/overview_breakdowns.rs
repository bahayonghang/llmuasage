#[test]
fn overview_filter_by_source_excludes_others() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(12)?;
    let dashboard = Dashboard::open(fixture.store())?;

    let all = dashboard.overview(&QueryFilter::default())?;
    let codex = dashboard.overview(&QueryFilter {
        source: Some(SourceKind::Codex),
        ..Default::default()
    })?;

    assert!(all.total.total_tokens > codex.total.total_tokens);
    assert_eq!(codex.source_count, 1);
    assert_eq!(
        dashboard
            .source_breakdown(&QueryFilter {
                source: Some(SourceKind::Codex),
                ..Default::default()
            })?
            .len(),
        1
    );
    Ok(())
}
#[test]
fn source_breakdown_preserves_filtered_latest_event_time() -> Result<()> {
    let fixture = Fixture::new()?;
    for event in [
        SeedEvent {
            event_key: "codex:source-breakdown:early",
            event_at: "2026-05-01T01:00:00Z",
            input_tokens: 10,
            total_tokens: 10,
            project_hash: "project-a",
            ..Default::default()
        },
        SeedEvent {
            event_key: "codex:source-breakdown:latest-matching",
            event_at: "2026-05-02T23:00:00Z",
            input_tokens: 20,
            total_tokens: 20,
            project_hash: "project-a",
            ..Default::default()
        },
        SeedEvent {
            event_key: "codex:source-breakdown:other-model",
            model: "gpt-other",
            event_at: "2026-05-03T00:00:00Z",
            input_tokens: 30,
            total_tokens: 30,
            project_hash: "project-a",
            ..Default::default()
        },
        SeedEvent {
            event_key: "codex:source-breakdown:other-project",
            event_at: "2026-05-04T00:00:00Z",
            input_tokens: 40,
            total_tokens: 40,
            project_hash: "project-b",
            ..Default::default()
        },
        SeedEvent {
            event_key: "claude:source-breakdown:latest",
            source: "claude",
            model: "claude-sonnet-4-5",
            event_at: "2026-05-05T00:00:00Z",
            input_tokens: 50,
            total_tokens: 50,
            project_hash: "project-a",
            ..Default::default()
        },
    ] {
        fixture.seed_event(event)?;
    }

    let dashboard = Dashboard::open(fixture.store())?;
    let filtered = dashboard.source_breakdown(&QueryFilter {
        source: Some(SourceKind::Codex),
        model: Some("gpt-5".to_string()),
        since: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
        until: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
        project_hash: Some("project-a".to_string()),
        timezone: ReportTimezone::Utc,
        ..Default::default()
    })?;

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].source, "codex");
    assert_eq!(filtered[0].total_tokens, 30);
    assert_eq!(filtered[0].event_count, 2);
    assert_eq!(
        filtered[0].last_event_at.as_deref(),
        Some("2026-05-02T23:00:00Z")
    );

    let all = dashboard.source_breakdown(&QueryFilter::default())?;
    assert_eq!(
        all.iter()
            .find(|row| row.source == "codex")
            .and_then(|row| row.last_event_at.as_deref()),
        Some("2026-05-04T00:00:00Z")
    );
    assert_eq!(
        all.iter()
            .find(|row| row.source == "claude")
            .and_then(|row| row.last_event_at.as_deref()),
        Some("2026-05-05T00:00:00Z")
    );
    Ok(())
}

#[test]
fn source_and_host_last_event_at_use_one_grouped_query() -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static GROUPED: AtomicUsize = AtomicUsize::new(0);
    static PER_GROUP_MAX: AtomicUsize = AtomicUsize::new(0);

    fn capture(event: rusqlite::trace::TraceEvent<'_>) {
        let rusqlite::trace::TraceEvent::Stmt(_, sql) = event else {
            return;
        };
        let compact = sql
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();
        let is_event_max = compact.contains("max(event_at)")
            && compact.contains("from usage_event")
            && !compact.contains("explain");
        if is_event_max && compact.contains("group by") {
            GROUPED.fetch_add(1, Ordering::Relaxed);
        }
        if is_event_max && !compact.contains("group by") {
            PER_GROUP_MAX.fetch_add(1, Ordering::Relaxed);
        }
    }

    let fixture = Fixture::new()?;
    for event in [
        SeedEvent {
            event_key: "codex:last-event:1",
            event_at: "2026-05-01T00:00:00Z",
            total_tokens: 10,
            ..Default::default()
        },
        SeedEvent {
            event_key: "claude:last-event:1",
            source: "claude",
            model: "claude-sonnet-4-5",
            event_at: "2026-05-02T00:00:00Z",
            total_tokens: 20,
            ..Default::default()
        },
        SeedEvent {
            event_key: "opencode:last-event:1",
            source: "opencode",
            event_at: "2026-05-03T00:00:00Z",
            total_tokens: 30,
            ..Default::default()
        },
    ] {
        fixture.seed_event(event)?;
    }

    let dashboard = Dashboard::open(fixture.store())?;
    GROUPED.store(0, Ordering::Relaxed);
    PER_GROUP_MAX.store(0, Ordering::Relaxed);
    dashboard.conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(capture),
    );
    let sources = dashboard.source_breakdown(&QueryFilter::default())?;
    let hosts = dashboard.host_breakdown(&QueryFilter::default())?;
    dashboard
        .conn
        .trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, None);

    assert!(
        sources.len() >= 2,
        "need multiple source groups, got {sources:?}"
    );
    assert!(!hosts.is_empty());
    assert_eq!(
        sources
            .iter()
            .find(|row| row.source == "codex")
            .and_then(|row| row.last_event_at.as_deref()),
        Some("2026-05-01T00:00:00Z")
    );
    assert_eq!(
        sources
            .iter()
            .find(|row| row.source == "claude")
            .and_then(|row| row.last_event_at.as_deref()),
        Some("2026-05-02T00:00:00Z")
    );
    assert_eq!(
        sources
            .iter()
            .find(|row| row.source == "opencode")
            .and_then(|row| row.last_event_at.as_deref()),
        Some("2026-05-03T00:00:00Z")
    );
    assert_eq!(
        hosts
            .iter()
            .map(|row| row.last_event_at.as_deref())
            .max()
            .flatten(),
        Some("2026-05-03T00:00:00Z")
    );
    assert_eq!(
        GROUPED.load(Ordering::Relaxed),
        2,
        "source and host last_event_at must each issue one grouped MAX query"
    );
    assert_eq!(
        PER_GROUP_MAX.load(Ordering::Relaxed),
        0,
        "last_event_at must not issue one MAX(event_at) per group"
    );
    Ok(())
}

#[test]
fn dashboard_snapshot_keeps_historical_antigravity_usage() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "antigravity:historical:1",
        source: "antigravity",
        model: "gemini-2.5-pro",
        event_at: "2026-05-06T00:00:00Z",
        input_tokens: 70,
        output_tokens: 7,
        total_tokens: 77,
        ..Default::default()
    })?;

    let dashboard = Dashboard::open(fixture.store())?;
    let snapshot = dashboard.snapshot(&QueryFilter::default())?;
    let historical = snapshot
        .sources
        .iter()
        .find(|row| row.source == "antigravity")
        .expect("dashboard source projection should retain Antigravity history");
    assert_eq!(historical.total_tokens, 77);
    assert_eq!(historical.event_count, 1);
    assert_eq!(
        historical.last_event_at.as_deref(),
        Some("2026-05-06T00:00:00Z")
    );
    Ok(())
}

#[test]
fn context_pressure_ratios_and_unpriced_split() -> Result<()> {
    use crate::testing::SeedEvent;

    let fixture = Fixture::new()?;
    // Priced model (codex gpt-5, window 400_000): peak prompt 200_000 -> 50%.
    fixture.seed_event(SeedEvent {
        event_key: "codex:ctx:1",
        model: "gpt-5",
        input_tokens: 150_000,
        cache_read_tokens: 50_000,
        total_tokens: 200_000,
        ..SeedEvent::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:ctx:2",
        model: "gpt-5",
        input_tokens: 40_000,
        total_tokens: 40_000,
        ..SeedEvent::default()
    })?;
    // Unknown-window model is excluded from ratios but counted as unpriced.
    fixture.seed_event(SeedEvent {
        event_key: "codex:ctx:3",
        model: "mystery-model",
        input_tokens: 999_999,
        total_tokens: 999_999,
        ..SeedEvent::default()
    })?;

    let dashboard = Dashboard::open(fixture.store())?;
    let pressure = dashboard.context_pressure(&Default::default())?;

    assert!((pressure.peak_percent - 0.5).abs() < 1e-9);
    // avg = (200_000 + 40_000) / 400_000 / 2 priced events = 0.30
    assert!((pressure.avg_percent - 0.30).abs() < 1e-9);
    assert_eq!(pressure.priced_events, 2);
    assert_eq!(pressure.unpriced_events, 1);
    assert_eq!(pressure.peak_model.as_deref(), Some("codex:gpt-5"));
    Ok(())
}

#[test]
fn bounded_context_pressure_uses_source_time_ranges_without_changing_totals() -> Result<()> {
    let fixture = Fixture::new()?;
    for event in [
        SeedEvent {
            event_key: "codex:bounded:1",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-08T01:00:00Z",
            input_tokens: 200_000,
            total_tokens: 200_000,
            ..Default::default()
        },
        SeedEvent {
            event_key: "claude:bounded:1",
            source: "claude",
            model: "claude-fable-5",
            event_at: "2026-05-08T02:00:00Z",
            input_tokens: 500_000,
            total_tokens: 500_000,
            ..Default::default()
        },
        SeedEvent {
            event_key: "codex:outside",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-07T23:59:59Z",
            input_tokens: 400_000,
            total_tokens: 400_000,
            ..Default::default()
        },
    ] {
        fixture.seed_event(event)?;
    }
    let filter = QueryFilter {
        since: Some(NaiveDate::from_ymd_opt(2026, 5, 8).unwrap()),
        until: Some(NaiveDate::from_ymd_opt(2026, 5, 8).unwrap()),
        timezone: ReportTimezone::Utc,
        ..Default::default()
    };
    let dashboard = Dashboard::open(fixture.store())?;
    let combined = dashboard.context_pressure(&filter)?;
    let codex = dashboard.context_pressure(&QueryFilter {
        source: Some(SourceKind::Codex),
        ..filter.clone()
    })?;
    let claude = dashboard.context_pressure(&QueryFilter {
        source: Some(SourceKind::Claude),
        ..filter.clone()
    })?;

    assert_eq!(
        combined.priced_events,
        codex.priced_events + claude.priced_events
    );
    assert_eq!(
        combined.unpriced_events,
        codex.unpriced_events + claude.unpriced_events
    );
    let expected_avg = (codex.avg_percent * codex.priced_events as f64
        + claude.avg_percent * claude.priced_events as f64)
        / combined.priced_events as f64;
    assert!((combined.avg_percent - expected_avg).abs() < EPSILON);
    assert_eq!(
        combined.peak_percent,
        codex.peak_percent.max(claude.peak_percent)
    );

    let event_filter = context_pressure_event_filter(&filter);
    let sql = format!(
        "EXPLAIN QUERY PLAN SELECT source, model, MAX(input_tokens + cache_read_tokens + cache_creation_tokens), SUM(input_tokens + cache_read_tokens + cache_creation_tokens), COUNT(*) FROM usage_event {} GROUP BY source, model",
        event_filter.where_sql()
    );
    let conn = fixture.store().open_connection()?;
    let mut stmt = conn.prepare(&sql)?;
    let plan = stmt
        .query_map(
            rusqlite::params_from_iter(event_filter.params().iter()),
            |row| row.get::<_, String>(3),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    assert!(
        plan.iter()
            .any(|detail| detail.contains("idx_usage_event_source_event_at")),
        "{plan:?}"
    );
    Ok(())
}

#[test]
fn context_pressure_knows_claude_fable_and_mythos_windows() -> Result<()> {
    use crate::testing::SeedEvent;

    let fixture = Fixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "claude:ctx:fable",
        source: "claude",
        model: "claude-fable-5",
        input_tokens: 450_000,
        cache_read_tokens: 50_000,
        total_tokens: 500_000,
        ..SeedEvent::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "claude:ctx:mythos",
        source: "claude",
        model: "claude-mythos-5",
        input_tokens: 250_000,
        total_tokens: 250_000,
        ..SeedEvent::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "claude:ctx:unknown",
        source: "claude",
        model: "claude-mythos-preview",
        input_tokens: 1_000_000,
        total_tokens: 1_000_000,
        ..SeedEvent::default()
    })?;

    let dashboard = Dashboard::open(fixture.store())?;
    let pressure = dashboard.context_pressure(&Default::default())?;

    assert!((pressure.peak_percent - 0.5).abs() < 1e-9);
    assert!((pressure.avg_percent - 0.375).abs() < 1e-9);
    assert_eq!(pressure.priced_events, 2);
    assert_eq!(pressure.unpriced_events, 1);
    assert_eq!(
        pressure.peak_model.as_deref(),
        Some("claude:claude-fable-5")
    );
    Ok(())
}

#[test]
fn context_pressure_empty_is_zero() -> Result<()> {
    let fixture = Fixture::new()?;
    let dashboard = Dashboard::open(fixture.store())?;
    let pressure = dashboard.context_pressure(&Default::default())?;
    assert_eq!(pressure.priced_events, 0);
    assert_eq!(pressure.unpriced_events, 0);
    assert_eq!(pressure.peak_percent, 0.0);
    assert_eq!(pressure.avg_percent, 0.0);
    Ok(())
}

#[test]
fn overview_filter_by_date_range_clamps_correctly() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(72)?;
    let dashboard = Dashboard::open(fixture.store())?;

    let one_day = dashboard.overview(&QueryFilter {
        since: Some(NaiveDate::from_ymd_opt(2026, 4, 2).unwrap()),
        until: Some(NaiveDate::from_ymd_opt(2026, 4, 2).unwrap()),
        ..Default::default()
    })?;
    assert_eq!(one_day.bucket_count, 24);
    assert!(one_day.total.total_tokens > 0);
    assert!(
        one_day.total.total_tokens < dashboard.overview(&Default::default())?.total.total_tokens
    );
    Ok(())
}

#[test]
fn cache_efficiency_zero_when_no_input() {
    assert_eq!(super::TokenSummary::default().cache_efficiency(), 0.0);
}

#[test]
fn sorted_unique_sources_trims_sorts_and_dedups() {
    assert_eq!(
        super::sorted_unique_sources(Some("codex, claude,codex,".to_string())),
        vec!["claude".to_string(), "codex".to_string()]
    );
    assert_eq!(super::sorted_unique_sources(None), Vec::<String>::new());
}

#[test]
fn model_breakdown_keeps_one_row_and_sorted_sources() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:shared-model:1",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-04-01T00:00:00Z",
        hour_start: Some("2026-04-01T00:00:00Z"),
        input_tokens: 10,
        total_tokens: 10,
        ..Default::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "claude:shared-model:1",
        source: "claude",
        model: "gpt-5",
        event_at: "2026-04-01T01:00:00Z",
        hour_start: Some("2026-04-01T01:00:00Z"),
        input_tokens: 5,
        total_tokens: 5,
        ..Default::default()
    })?;

    let models = Dashboard::open(fixture.store())?.model_breakdown(&Default::default())?;
    let matches: Vec<_> = models.iter().filter(|row| row.model == "gpt-5").collect();
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0].sources,
        vec!["claude".to_string(), "codex".to_string()]
    );
    assert_eq!(matches[0].total_tokens, 15);
    Ok(())
}

/// Validates the 0.5.1 ccr-ui field contract: overview, daily trends,
/// model/project breakdowns, and logs all expose persisted cost/cache/
/// pricing fields without requiring downstream adapters to re-SUM them.
#[test]
fn dashboard_ccr_ui_contract_exposes_cost_cache_and_pricing_fields() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(12)?;
    let dashboard = Dashboard::open(fixture.store())?;

    let overview = dashboard.overview(&QueryFilter::default())?;
    assert!(overview.total_cost_usd > 0.0);
    assert_eq!(overview.cache_efficiency, overview.total.cache_efficiency());

    let trend = dashboard
        .trends_daily(&QueryFilter::default())?
        .into_iter()
        .next()
        .expect("seeded trend");
    assert!(trend.cost_with_cache_usd > 0.0);

    let model = dashboard
        .model_breakdown(&QueryFilter::default())?
        .into_iter()
        .find(|row| row.model == "gpt-5")
        .expect("gpt-5 model row");
    assert!(model.cost_with_cache_usd > 0.0);
    assert!(model.cost_without_cache_usd >= model.cost_with_cache_usd);
    assert!(model.cache_savings_usd >= 0.0);
    assert_eq!(model.pricing_status, "static");
    assert_eq!(model.pricing_source.as_deref(), Some("static-v2"));
    assert!(model.pricing_rate.is_some());

    let project = dashboard
        .project_breakdown(&QueryFilter::default())?
        .into_iter()
        .next()
        .expect("seeded project row");
    assert!(project.total_cost_usd > 0.0);
    assert!(project.project_path.is_some());

    let logs = dashboard.logs(&crate::LogsQuery {
        page_size: 1,
        ..Default::default()
    })?;
    let record = logs.records.first().expect("seeded log row");
    assert_eq!(record.id, record.event_key);
    assert!(!record.recorded_at.is_empty());
    assert!(record.cost_usd >= 0.0);
    assert_eq!(record.cost_usd, record.cost_with_cache_usd);
    assert!(!record.pricing_status.is_empty());

    Ok(())
}

/// Validates D24/F1.4: overview surfaces an event count that matches the
/// number of seeded usage events, and breakdown rows expose the same
/// totals so dashboards no longer need to re-COUNT in the UI layer.
#[test]
fn overview_event_count_matches_row_count() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(48)?;
    let dashboard = Dashboard::open(fixture.store())?;

    let overview = dashboard.overview(&QueryFilter::default())?;
    assert_eq!(overview.total_events, 48);
    assert!(overview.total_cost_usd > 0.0);

    let model_total: i64 = dashboard
        .model_breakdown(&QueryFilter::default())?
        .iter()
        .map(|row| row.event_count)
        .sum();
    assert_eq!(model_total, 48);

    let source_total: i64 = dashboard
        .source_breakdown(&QueryFilter::default())?
        .iter()
        .map(|row| row.event_count)
        .sum();
    assert_eq!(source_total, 48);

    let cost_total: i64 = dashboard
        .cost_breakdown(&QueryFilter::default())?
        .iter()
        .map(|row| row.event_count)
        .sum();
    assert_eq!(cost_total, 48);
    Ok(())
}
