use anyhow::Result;
use llmusage::{AppPaths, Dashboard, QueryFilter, ReportTimezone, store::Store};
use tempfile::TempDir;

fn fixture() -> Result<(TempDir, Store)> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    Ok((temp, store))
}

#[test]
fn hour_of_week_zero_fills_and_applies_iana_timezone() -> Result<()> {
    let (_temp, store) = fixture()?;
    let empty = Dashboard::open(&store)?.hour_of_week(&QueryFilter {
        timezone: ReportTimezone::Utc,
        ..Default::default()
    })?;
    assert_eq!(empty.len(), 168);
    assert!(
        empty
            .iter()
            .all(|cell| cell.total_tokens == 0 && cell.event_count == 0)
    );

    store.open_connection()?.execute(
        r#"
        INSERT INTO usage_bucket_30m(
            source, model, hour_start, project_hash, input_tokens,
            cache_creation_tokens, cache_read_tokens, output_tokens,
            reasoning_output_tokens, total_tokens, event_count, updated_at
        ) VALUES ('codex', 'gpt-5', '2026-05-04T16:30:00Z', 'p1',
                  0, 0, 0, 0, 0, 42, 2, '2026-05-04T16:30:00Z')
        "#,
        [],
    )?;

    let dashboard = Dashboard::open(&store)?;
    let utc = dashboard.hour_of_week(&QueryFilter {
        timezone: ReportTimezone::Utc,
        ..Default::default()
    })?;
    let shanghai = dashboard.hour_of_week(&QueryFilter {
        timezone: ReportTimezone::Iana("Asia/Shanghai".parse().unwrap()),
        ..Default::default()
    })?;
    assert_eq!(
        utc.iter()
            .find(|cell| cell.total_tokens == 42)
            .map(|c| (c.dow, c.hour)),
        Some((0, 16))
    );
    assert_eq!(
        shanghai
            .iter()
            .find(|cell| cell.total_tokens == 42)
            .map(|c| (c.dow, c.hour)),
        Some((1, 0))
    );

    let filtered = dashboard.hour_of_week(&QueryFilter {
        model: Some("other".into()),
        timezone: ReportTimezone::Utc,
        ..Default::default()
    })?;
    assert!(filtered.iter().all(|cell| cell.total_tokens == 0));
    Ok(())
}

#[test]
fn hour_of_week_folds_both_sides_of_a_dst_fallback_into_the_same_local_hour() -> Result<()> {
    let (_temp, store) = fixture()?;
    let conn = store.open_connection()?;
    for (hour_start, tokens) in [("2026-11-01T05:30:00Z", 10), ("2026-11-01T06:30:00Z", 20)] {
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, input_tokens, cache_creation_tokens,
                cache_read_tokens, output_tokens, reasoning_output_tokens,
                total_tokens, event_count, updated_at
            ) VALUES ('codex', 'gpt-5', ?1, 0, 0, 0, 0, 0, ?2, 1, ?1)
            "#,
            rusqlite::params![hour_start, tokens],
        )?;
    }

    let rows = Dashboard::open(&store)?.hour_of_week(&QueryFilter {
        timezone: ReportTimezone::Iana("America/New_York".parse().unwrap()),
        ..Default::default()
    })?;
    let repeated_hour = rows
        .iter()
        .find(|cell| cell.dow == 6 && cell.hour == 1)
        .unwrap();
    assert_eq!(repeated_hour.total_tokens, 30);
    assert_eq!(repeated_hour.event_count, 2);
    Ok(())
}
