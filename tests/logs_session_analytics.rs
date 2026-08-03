use anyhow::Result;
use llmusage::{AppPaths, Dashboard, LogsQuery, store::Store};
use tempfile::TempDir;

fn fixture() -> Result<(TempDir, Store)> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    let conn = store.open_connection()?;
    for (key, session, label, at) in [
        ("event-a", "session-a", "Alpha Work", "2026-05-01T00:00:00Z"),
        ("event-b", "session-b", "Beta Work", "2026-05-01T01:00:00Z"),
    ] {
        conn.execute(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start, input_tokens,
                cache_creation_tokens, cache_read_tokens, output_tokens,
                reasoning_output_tokens, total_tokens, path_hash, session_id,
                session_label, created_at, cost_with_cache_usd,
                cost_without_cache_usd, pricing_status
            ) VALUES (?1, 'codex', 'gpt-5', ?4, ?4, 1, 0, 0, 1, 0, 2,
                      ?1, ?2, ?3, ?4, 0.0, 0.0, 'static')
            "#,
            rusqlite::params![key, session, label, at],
        )?;
        conn.execute(
            "INSERT INTO usage_event_raw(event_key, raw_json, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![key, format!(r#"{{"event":"{key}"}}"#), at],
        )?;
    }
    Ok((temp, store))
}

#[test]
fn logs_support_session_and_single_event_detail() -> Result<()> {
    let (_temp, store) = fixture()?;
    let dashboard = Dashboard::open(&store)?;

    let exact = dashboard.logs(&LogsQuery {
        session: Some("SESSION-A".into()),
        ..Default::default()
    })?;
    assert_eq!(exact.records.len(), 1);
    assert_eq!(exact.records[0].event_key, "event-a");

    let canonical = dashboard.logs(&LogsQuery {
        session: Some("CODEX:SESSION-A".into()),
        ..Default::default()
    })?;
    assert_eq!(canonical.records.len(), 1);
    assert_eq!(canonical.records[0].event_key, "event-a");

    let label = dashboard.logs(&LogsQuery {
        session: Some("beta".into()),
        ..Default::default()
    })?;
    assert_eq!(label.records.len(), 1);
    assert_eq!(label.records[0].event_key, "event-b");

    let detail = dashboard.logs(&LogsQuery {
        page_size: 500,
        cursor: Some("ignored-in-detail-mode".into()),
        include_total: true,
        event_key: Some("event-a".into()),
        ..Default::default()
    })?;
    assert_eq!(detail.records.len(), 1);
    assert_eq!(detail.total, None);
    assert_eq!(detail.next_cursor, None);
    assert_eq!(
        detail.records[0].raw_json.as_deref(),
        Some(r#"{"event":"event-a"}"#)
    );
    Ok(())
}
