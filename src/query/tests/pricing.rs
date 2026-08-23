/// Validates D6/F1.3: `Store::recompute_costs` rewrites the per-event
/// cost columns using the embedded catalog, so a `usage_event` seeded
/// with zero cost now carries non-zero `cost_with_cache_usd` and a
/// `pricing_status = 'static'` row tag.
#[test]
fn refresh_pricing_recomputes_all_costs() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_event(crate::testing::SeedEvent {
        event_key: "codex:k1",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-05-01T00:00:00Z",
        hour_start: Some("2026-05-01T00:00:00Z"),
        input_tokens: 1_000_000,
        cache_read_tokens: 200_000,
        output_tokens: 500_000,
        reasoning_output_tokens: 0,
        total_tokens: 1_700_000,
        created_at: Some("2026-05-01T00:00:00Z"),
        ..Default::default()
    })?;
    let conn = fixture.store().open_connection()?;

    let updated = fixture.store().recompute_costs()?;
    assert_eq!(updated, 1);

    let (cost_with, cost_without, status, source): (f64, f64, String, String) = conn
        .query_row(
            r#"
            SELECT cost_with_cache_usd, cost_without_cache_usd,
                   pricing_status, COALESCE(pricing_source, '')
            FROM usage_event WHERE event_key = 'codex:k1'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    assert!(cost_with > 0.0);
    assert!(cost_without > cost_with);
    assert_eq!(status, "static");
    assert_eq!(source, "static-v2");
    let (bucket_cost_with, bucket_status, bucket_source): (f64, String, String) = conn
        .query_row(
            r#"
            SELECT cost_with_cache_usd, pricing_status, COALESCE(pricing_source, '')
            FROM usage_bucket_30m
            WHERE source = 'codex' AND model = 'gpt-5' AND hour_start = '2026-05-01T00:00:00Z'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    assert!((bucket_cost_with - cost_with).abs() < 1e-9);
    assert_eq!(bucket_status, "static");
    assert_eq!(bucket_source, "static-v2");
    Ok(())
}

#[test]
fn recompute_costs_prices_codex_and_claude_cache_channels() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_event(crate::testing::SeedEvent {
        event_key: "codex:cache",
        source: "codex",
        model: "gpt-5.5",
        event_at: "2026-05-01T00:00:00Z",
        hour_start: Some("2026-05-01T00:00:00Z"),
        input_tokens: 1_000_000,
        cache_read_tokens: 2_000_000,
        output_tokens: 3_000_000,
        reasoning_output_tokens: 4_000_000,
        total_tokens: 10_000_000,
        created_at: Some("2026-05-01T00:00:00Z"),
        ..Default::default()
    })?;
    fixture.seed_event(crate::testing::SeedEvent {
        event_key: "claude:cache",
        source: "claude",
        model: "claude-sonnet-4-5",
        event_at: "2026-05-01T00:00:00Z",
        hour_start: Some("2026-05-01T00:00:00Z"),
        input_tokens: 1_000_000,
        cache_read_tokens: 2_000_000,
        cache_creation_tokens: 3_000_000,
        output_tokens: 4_000_000,
        reasoning_output_tokens: 5_000_000,
        total_tokens: 15_000_000,
        created_at: Some("2026-05-01T00:00:00Z"),
        ..Default::default()
    })?;

    let updated = fixture.store().recompute_costs()?;
    assert_eq!(updated, 2);

    let conn = fixture.store().open_connection()?;
    let (codex_cost, codex_without, codex_status): (f64, f64, String) = conn.query_row(
        r#"
        SELECT cost_with_cache_usd, cost_without_cache_usd, pricing_status
        FROM usage_event
        WHERE event_key = 'codex:cache'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert!((codex_cost - 31.5).abs() < EPSILON);
    assert!((codex_without - 33.75).abs() < EPSILON);
    assert_eq!(codex_status, "static");

    let (claude_cost, claude_without, claude_status): (f64, f64, String) = conn.query_row(
        r#"
        SELECT cost_with_cache_usd, cost_without_cache_usd, pricing_status
        FROM usage_event
        WHERE event_key = 'claude:cache'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert!((claude_cost - 72.6).abs() < EPSILON);
    assert!((claude_without - 78.0).abs() < EPSILON);
    assert_eq!(claude_status, "static");

    let (bucket_cost, bucket_tokens, bucket_status): (f64, i64, String) = conn.query_row(
        r#"
        SELECT cost_with_cache_usd, total_tokens, pricing_status
        FROM usage_bucket_30m
        WHERE source = 'claude' AND model = 'claude-sonnet-4-5'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert!((bucket_cost - claude_cost).abs() < EPSILON);
    assert_eq!(bucket_tokens, 15_000_000);
    assert_eq!(bucket_status, "static");

    Ok(())
}

/// Validates F1.3 snapshot path: `Store::recompute_costs_with` driven by
/// a litellm-shaped catalog stamps `pricing_status = 'snapshot'` and
/// the catalog's version label so dashboards can tell static vs
/// snapshot-priced rows apart even after the same recompute pass.
#[test]
fn recompute_costs_with_snapshot_catalog_marks_rows_as_snapshot() -> Result<()> {
    use crate::query::PricingCatalog;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let fixture = Fixture::new()?;
    fixture.seed_event(crate::testing::SeedEvent {
        event_key: "codex:snap",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-05-01T00:00:00Z",
        hour_start: Some("2026-05-01T00:00:00Z"),
        input_tokens: 500_000,
        output_tokens: 100_000,
        total_tokens: 600_000,
        created_at: Some("2026-05-01T00:00:00Z"),
        ..Default::default()
    })?;
    let conn = fixture.store().open_connection()?;

    let mut tmp = NamedTempFile::new()?;
    writeln!(
        tmp,
        r#"{{
            "version": "litellm-snapshot-2026-05",
            "models": [
                {{
                    "source": "codex",
                    "matchers": ["gpt-5"],
                    "input_per_mtok": 2.0,
                    "cached_per_mtok": 0.2,
                    "output_per_mtok": 20.0
                }}
            ]
        }}"#
    )?;
    tmp.flush()?;

    let catalog = PricingCatalog::load_snapshot(tmp.path())?;
    let updated = fixture.store().recompute_costs_with(&catalog)?;
    assert_eq!(updated, 1);

    let (status, source, cost_with): (String, String, f64) = conn.query_row(
        r#"
        SELECT pricing_status, COALESCE(pricing_source, ''), cost_with_cache_usd
        FROM usage_event WHERE event_key = 'codex:snap'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(status, "snapshot");
    assert_eq!(source, "litellm-snapshot-2026-05");
    // 0.5M input @ 2.0 + 0 cache_read + 0.1M output @ 20.0 = 1.0 + 2.0 = 3.0
    assert!((cost_with - 3.0).abs() < 1e-6);
    let (bucket_status, bucket_source, bucket_cost): (String, String, f64) = conn.query_row(
        r#"
        SELECT pricing_status, COALESCE(pricing_source, ''), cost_with_cache_usd
        FROM usage_bucket_30m
        WHERE source = 'codex' AND model = 'gpt-5' AND hour_start = '2026-05-01T00:00:00Z'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(bucket_status, "snapshot");
    assert_eq!(bucket_source, "litellm-snapshot-2026-05");
    assert!((bucket_cost - 3.0).abs() < 1e-6);
    Ok(())
}

/// Validates C2 on the recompute path: the no-arg recompute entrypoint uses
/// the same active catalog resolver as sync, so an active local snapshot
/// does not diverge back to the embedded static catalog.
#[test]
fn recompute_costs_uses_active_pricing_catalog() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_event(crate::testing::SeedEvent {
        event_key: "codex:active-snap",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-05-01T00:00:00Z",
        hour_start: Some("2026-05-01T00:00:00Z"),
        input_tokens: 500_000,
        output_tokens: 100_000,
        total_tokens: 600_000,
        created_at: Some("2026-05-01T00:00:00Z"),
        ..Default::default()
    })?;
    let pricing_dir = fixture.paths().root_dir.join("pricing");
    std::fs::create_dir_all(&pricing_dir)?;
    std::fs::write(
        pricing_dir.join("litellm-snapshot-2026-05.json"),
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
    fixture
        .store()
        .set_meta_value("pricing_catalog_version", "litellm-snapshot-2026-05")?;

    let updated = fixture.store().recompute_costs()?;
    assert_eq!(updated, 1);

    let conn = fixture.store().open_connection()?;
    let (status, source, cost): (String, String, f64) = conn.query_row(
        r#"
        SELECT pricing_status, COALESCE(pricing_source, ''), cost_with_cache_usd
        FROM usage_event WHERE event_key = 'codex:active-snap'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(status, "snapshot");
    assert_eq!(source, "litellm-snapshot-2026-05");
    assert!((cost - 3.0).abs() < 1e-6);
    Ok(())
}

#[test]
fn recompute_costs_deletes_orphan_buckets() -> Result<()> {
    let fixture = Fixture::new()?;
    const LIVE_BUCKETS: usize = 128;
    for index in 0..LIVE_BUCKETS {
        let event_key = format!("codex:live-bucket-{index:03}");
        let day = index / 24 + 1;
        let hour = index % 24;
        let event_at = format!("2026-05-{day:02}T{hour:02}:00:00Z");
        fixture.seed_event(crate::testing::SeedEvent {
            event_key: &event_key,
            source: "codex",
            model: "gpt-5",
            event_at: &event_at,
            hour_start: Some(&event_at),
            input_tokens: 1_000,
            output_tokens: 500,
            total_tokens: 1_500,
            created_at: Some(&event_at),
            ..Default::default()
        })?;
    }
    let conn = fixture.store().open_connection()?;
    conn.execute(
        r#"
        INSERT INTO usage_bucket_30m(
            source, model, hour_start, project_hash, project_label, project_ref,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens,
            cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source, pricing_rate,
            event_count, updated_at
        ) VALUES ('codex', 'gpt-5', '2026-06-01T00:00:00Z', '', NULL, NULL,
            0, 0, 0, 0, 0, 0,
            42.0, 42.0, 'static', 'static-v1', '{}',
            0, '2026-06-01T00:00:00Z')
        "#,
        [],
    )?;

    let before: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_bucket_30m WHERE source = 'codex'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(before, LIVE_BUCKETS as i64 + 1);

    let updated = fixture.store().recompute_costs()?;
    assert_eq!(updated, LIVE_BUCKETS);

    let after: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_bucket_30m WHERE source = 'codex'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        after, LIVE_BUCKETS as i64,
        "orphan bucket should be deleted without affecting live buckets"
    );
    let event_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(event_count, LIVE_BUCKETS as i64);
    let orphan_count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*) FROM usage_bucket_30m
        WHERE source = 'codex' AND model = 'gpt-5' AND hour_start = '2026-06-01T00:00:00Z'
        "#,
        [],
        |row| row.get(0),
    )?;
    assert_eq!(orphan_count, 0);
    Ok(())
}

fn seed_source_reported(
    fixture: &Fixture,
    event_key: &str,
    model: &str,
    hour_start: &str,
    cost_with: f64,
    cost_without: f64,
) -> Result<()> {
    fixture.seed_event(SeedEvent {
        event_key,
        source: "omp",
        model,
        event_at: hour_start,
        hour_start: Some(hour_start),
        input_tokens: 1_000,
        output_tokens: 200,
        total_tokens: 1_200,
        cost_with_cache_usd: cost_with,
        cost_without_cache_usd: cost_without,
        pricing_status: "source_reported",
        pricing_source: Some("source-reported"),
        pricing_rate: Some(r#"{"source":"pi_usage_cost"}"#),
        created_at: Some(hour_start),
        ..SeedEvent::default()
    })?;
    Ok(())
}

#[test]
fn recompute_costs_leaves_source_reported_event_amounts_unchanged() -> Result<()> {
    let fixture = Fixture::new()?;
    seed_source_reported(
        &fixture,
        "omp:reported",
        "deepseek-v4-flash",
        "2026-05-01T00:00:00Z",
        0.0661,
        0.07,
    )?;
    let conn = fixture.store().open_connection()?;
    let before: (f64, f64, String, String, Option<String>) = conn.query_row(
        r#"
        SELECT cost_with_cache_usd, cost_without_cache_usd,
               pricing_status, COALESCE(pricing_source, ''), pricing_rate
        FROM usage_event WHERE event_key = 'omp:reported'
        "#,
        [],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )?;

    let updated = fixture.store().recompute_costs()?;
    assert_eq!(updated, 0);

    let after: (f64, f64, String, String, Option<String>) = conn.query_row(
        r#"
        SELECT cost_with_cache_usd, cost_without_cache_usd,
               pricing_status, COALESCE(pricing_source, ''), pricing_rate
        FROM usage_event WHERE event_key = 'omp:reported'
        "#,
        [],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )?;
    assert_eq!(before, after);
    assert_eq!(after.2, "source_reported");
    assert_eq!(after.3, "source-reported");
    Ok(())
}

#[test]
fn recompute_keeps_source_reported_buckets_and_mixes_with_unpriced() -> Result<()> {
    let fixture = Fixture::new()?;
    seed_source_reported(
        &fixture,
        "omp:pure",
        "deepseek-v4-flash",
        "2026-05-01T00:00:00Z",
        0.125,
        0.125,
    )?;
    seed_source_reported(
        &fixture,
        "omp:mixed-paid",
        "stealth/ox-alpha",
        "2026-05-01T00:30:00Z",
        0.3756,
        0.4,
    )?;
    fixture.seed_event(SeedEvent {
        event_key: "omp:mixed-free",
        source: "omp",
        model: "stealth/ox-alpha",
        event_at: "2026-05-01T00:30:00Z",
        hour_start: Some("2026-05-01T00:30:00Z"),
        input_tokens: 10,
        output_tokens: 5,
        total_tokens: 15,
        cost_with_cache_usd: 0.0,
        cost_without_cache_usd: 0.0,
        pricing_status: "unpriced",
        created_at: Some("2026-05-01T00:30:00Z"),
        ..SeedEvent::default()
    })?;

    let updated = fixture.store().recompute_costs()?;
    assert_eq!(updated, 1, "only the unpriced event is catalog-repriced");

    let conn = fixture.store().open_connection()?;
    let (pure_cost, pure_status, pure_count): (f64, String, i64) = conn.query_row(
        r#"
        SELECT cost_with_cache_usd, pricing_status, COUNT(*)
        FROM usage_bucket_30m
        WHERE source = 'omp' AND model = 'deepseek-v4-flash'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(pure_count, 1);
    assert_eq!(pure_status, "source_reported");
    assert!((pure_cost - 0.125).abs() < EPSILON);

    let (mixed_cost, mixed_status, mixed_count): (f64, String, i64) = conn.query_row(
        r#"
        SELECT cost_with_cache_usd, pricing_status, COUNT(*)
        FROM usage_bucket_30m
        WHERE source = 'omp' AND model = 'stealth/ox-alpha'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(mixed_count, 1);
    assert_eq!(mixed_status, "mixed");
    assert!((mixed_cost - 0.3756).abs() < EPSILON);

    let paid: (f64, String) = conn.query_row(
        r#"
        SELECT cost_with_cache_usd, pricing_status
        FROM usage_event WHERE event_key = 'omp:mixed-paid'
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert!((paid.0 - 0.3756).abs() < EPSILON);
    assert_eq!(paid.1, "source_reported");
    Ok(())
}

#[test]
fn source_reported_events_are_not_counted_as_unpriced() -> Result<()> {
    let fixture = Fixture::new()?;
    seed_source_reported(
        &fixture,
        "omp:priced",
        "deepseek-v4-flash",
        "2026-05-01T00:00:00Z",
        0.0125,
        0.0125,
    )?;

    let models = Dashboard::open(fixture.store())?.model_breakdown(&QueryFilter::default())?;
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].pricing_status, "source_reported");

    let filter = super::reports::ReportFilter {
        since: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
        until: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
        order: super::reports::SortOrder::Asc,
        timezone: ReportTimezone::Utc,
        locale: "en-US".to_string(),
        source: Some(SourceKind::Omp),
        project: None,
        breakdown: false,
        host_id: None,
    };
    let report = super::reports::load_daily_report(fixture.store(), &filter)?;
    assert_eq!(report.daily.len(), 1);
    assert!(!report.daily[0].notes.unpriced);
    assert!((report.daily[0].totals.estimated_cost_usd - 0.0125).abs() < EPSILON);
    Ok(())
}
