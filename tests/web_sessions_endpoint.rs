use anyhow::Result;
use llmusage::{
    AppPaths, Dashboard, QueryFilter, TopSessionsQuery, TopSessionsSort, models::SourceKind,
    store::Store,
};
use tempfile::TempDir;

fn fixture() -> Result<(TempDir, Store)> {
    let temp = TempDir::new()?;
    let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    Ok((temp, store))
}

fn seed(store: &Store) -> Result<()> {
    let conn = store.open_connection()?;
    for row in [
        (
            "a1",
            "codex",
            "gpt-a",
            "2026-05-01T00:00:00Z",
            "alpha",
            "Alpha",
            "p1",
            "Project 1",
            50,
            5,
            0.5,
        ),
        (
            "a2",
            "codex",
            "gpt-a",
            "2026-05-01T00:10:00Z",
            "alpha",
            "Alpha",
            "p1",
            "Project 1",
            50,
            5,
            0.5,
        ),
        (
            "b1",
            "claude",
            "gpt-b",
            "2026-05-01T00:00:00Z",
            "beta",
            "Beta",
            "p2",
            "Project 2",
            50,
            4,
            0.5,
        ),
        (
            "b2",
            "claude",
            "gpt-b",
            "2026-05-01T00:10:00Z",
            "beta",
            "Beta",
            "p2",
            "Project 2",
            50,
            4,
            0.5,
        ),
        (
            "c1",
            "codex",
            "gpt-a",
            "2026-05-01T01:00:00Z",
            "gamma",
            "Gamma",
            "p1",
            "Project 1",
            20,
            2,
            3.0,
        ),
        (
            "c2",
            "codex",
            "gpt-a",
            "2026-05-01T01:30:00Z",
            "gamma",
            "Gamma",
            "p1",
            "Project 1",
            20,
            2,
            3.0,
        ),
    ] {
        conn.execute(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cache_creation_tokens, cache_read_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                project_hash, project_label, path_hash, session_id, session_label,
                source_path_hash, created_at, cost_with_cache_usd,
                cost_without_cache_usd, pricing_status
            ) VALUES (?1, ?2, ?3, ?4, ?4, 0, 0, 0, ?10, 0, ?9,
                      ?7, ?8, ?1, ?5, ?6, NULL, ?4, ?11, ?11, 'static')
            "#,
            rusqlite::params![
                row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9, row.10
            ],
        )?;
    }
    Ok(())
}

#[test]
fn top_sessions_is_empty_for_an_empty_store() -> Result<()> {
    let (_temp, store) = fixture()?;
    assert!(
        Dashboard::open(&store)?
            .top_sessions(&Default::default())?
            .is_empty()
    );
    Ok(())
}

#[test]
fn top_sessions_filters_and_sorts_stably() -> Result<()> {
    let (_temp, store) = fixture()?;
    seed(&store)?;
    let dashboard = Dashboard::open(&store)?;

    let tokens = dashboard.top_sessions(&TopSessionsQuery {
        sort: TopSessionsSort::Tokens,
        limit: 10,
        ..Default::default()
    })?;
    assert_eq!(
        tokens
            .iter()
            .map(|r| r.session_id.as_str())
            .collect::<Vec<_>>(),
        ["claude:beta", "codex:alpha", "codex:gamma"]
    );

    let duration = dashboard.top_sessions(&TopSessionsQuery {
        sort: TopSessionsSort::Duration,
        limit: 10,
        ..Default::default()
    })?;
    assert_eq!(duration[0].session_id, "codex:gamma");
    assert_eq!(duration[0].active_minutes, 30);
    assert_eq!(duration[0].first_event_at, "2026-05-01T01:00:00Z");
    assert_eq!(duration[0].last_event_at, "2026-05-01T01:30:00Z");
    assert_eq!(duration[1].session_id, "claude:beta");
    assert_eq!(duration[2].session_id, "codex:alpha");
    assert_eq!(duration[1].active_minutes, duration[2].active_minutes);

    let cost = dashboard.top_sessions(&TopSessionsQuery {
        sort: TopSessionsSort::Cost,
        limit: 10,
        ..Default::default()
    })?;
    assert_eq!(cost[0].session_id, "codex:gamma");
    assert_eq!(cost[1].session_id, "claude:beta");
    assert_eq!(cost[2].session_id, "codex:alpha");
    assert_eq!(cost[1].cost_usd, cost[2].cost_usd);

    let filtered = dashboard.top_sessions(&TopSessionsQuery {
        filter: QueryFilter {
            source: Some(SourceKind::Codex),
            model: Some("gpt-a".into()),
            project_hash: Some("p1".into()),
            ..Default::default()
        },
        limit: 1,
        ..Default::default()
    })?;
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].session_id, "codex:alpha");
    assert_eq!(filtered[0].first_event_at, "2026-05-01T00:00:00Z");
    assert_eq!(filtered[0].last_event_at, "2026-05-01T00:10:00Z");

    let clamped = dashboard.top_sessions(&TopSessionsQuery {
        limit: 500,
        ..Default::default()
    })?;
    assert_eq!(clamped.len(), 3);
    Ok(())
}

#[test]
fn duration_sort_does_not_drop_active_sessions_outside_a_span_prefilter() -> Result<()> {
    let (_temp, store) = fixture()?;
    let conn = store.open_connection()?;
    for (key, session, at) in [
        ("idle-a-1", "idle-a", "2026-05-01T00:00:00Z"),
        ("idle-a-2", "idle-a", "2026-05-01T03:00:00Z"),
        ("idle-b-1", "idle-b", "2026-05-01T00:00:00Z"),
        ("idle-b-2", "idle-b", "2026-05-01T02:00:00Z"),
        ("idle-c-1", "idle-c", "2026-05-01T00:00:00Z"),
        ("idle-c-2", "idle-c", "2026-05-01T01:00:00Z"),
        ("active-1", "active", "2026-05-01T00:00:00Z"),
        ("active-2", "active", "2026-05-01T00:30:00Z"),
    ] {
        conn.execute(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start, input_tokens,
                cache_creation_tokens, cache_read_tokens, output_tokens,
                reasoning_output_tokens, total_tokens, path_hash, session_id,
                created_at, cost_with_cache_usd, cost_without_cache_usd, pricing_status
            ) VALUES (?1, 'codex', 'gpt-5', ?3, ?3, 1, 0, 0, 1, 0, 2,
                      ?1, ?2, ?3, 0.0, 0.0, 'static')
            "#,
            rusqlite::params![key, session, at],
        )?;
    }

    let rows = Dashboard::open(&store)?.top_sessions(&TopSessionsQuery {
        sort: TopSessionsSort::Duration,
        limit: 1,
        ..Default::default()
    })?;
    assert_eq!(rows[0].session_id, "codex:active");
    assert_eq!(rows[0].active_minutes, 30);
    Ok(())
}
