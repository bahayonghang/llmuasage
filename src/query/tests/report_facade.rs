#[test]
fn blocks_report_reuses_dashboard_connection() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:blocks-facade:active",
        event_at: "2026-05-10T10:15:00Z",
        total_tokens: 10,
        ..Default::default()
    })?;

    crate::store::Store::reset_open_connection_counter();
    let dashboard = Dashboard::open(fixture.store())?;
    assert_eq!(crate::store::Store::open_connection_count(), 1);
    let _blocks = dashboard.blocks_report()?;
    assert_eq!(
        crate::store::Store::open_connection_count(),
        1,
        "Dashboard::blocks_report must reuse the dashboard connection"
    );
    Ok(())
}

#[test]
fn dashboard_and_daily_report_share_dst_date_bounds() -> Result<()> {
    let fixture = Fixture::new()?;
    for event in [
        SeedEvent {
            event_key: "codex:dst-bounds:before",
            event_at: "2026-03-07T05:30:00Z",
            input_tokens: 10,
            total_tokens: 10,
            ..Default::default()
        },
        SeedEvent {
            event_key: "codex:dst-bounds:start",
            event_at: "2026-03-08T05:30:00Z",
            input_tokens: 20,
            total_tokens: 20,
            ..Default::default()
        },
        SeedEvent {
            event_key: "codex:dst-bounds:end",
            event_at: "2026-03-09T03:30:00Z",
            input_tokens: 30,
            total_tokens: 30,
            ..Default::default()
        },
        SeedEvent {
            event_key: "codex:dst-bounds:after",
            event_at: "2026-03-09T04:30:00Z",
            input_tokens: 40,
            total_tokens: 40,
            ..Default::default()
        },
    ] {
        fixture.seed_event(event)?;
    }

    let query = QueryFilter {
        since: Some(NaiveDate::from_ymd_opt(2026, 3, 8).unwrap()),
        until: Some(NaiveDate::from_ymd_opt(2026, 3, 8).unwrap()),
        timezone: ReportTimezone::Iana(chrono_tz::America::New_York),
        ..QueryFilter::default()
    };
    // NY springs forward 2026-03-08 02:00 EST -> 03:00 EDT, so Mar 8 is
    // [2026-03-08T05:00:00Z, 2026-03-09T04:00:00Z). A second date SQL generator
    // that kept a fixed EST-5 until bound (05:00Z) would also count the 04:30Z row.
    assert_eq!(
        query.bucket_filter(None).params(),
        &[
            rusqlite::types::Value::Text("2026-03-08T05:00:00Z".to_string()),
            rusqlite::types::Value::Text("2026-03-09T04:00:00Z".to_string()),
        ]
    );

    let filter = super::reports::ReportFilter {
        filter: query,
        order: super::reports::SortOrder::Asc,
        locale: "en-US".to_string(),
        project: None,
        breakdown: false,
    };
    let dashboard = Dashboard::open(fixture.store())?;
    let overview = dashboard.overview(filter.query())?;
    let trends = dashboard.trends_daily(filter.query())?;
    let report = super::reports::load_unified_report(
        dashboard.connection(),
        &filter,
        super::reports::PeriodKind::Daily,
    )?;

    assert_eq!(overview.total.total_tokens, 50);
    assert_eq!(report.totals().total_tokens, 50);
    assert_eq!(
        trends
            .iter()
            .map(|point| (point.date.as_str(), point.total_tokens))
            .collect::<Vec<_>>(),
        vec![("2026-03-08", 50)]
    );
    assert_eq!(
        report
            .rows
            .iter()
            .map(|row| (row.period.as_str(), row.totals.total_tokens))
            .collect::<Vec<_>>(),
        vec![("2026-03-08", 50)]
    );
    Ok(())
}
