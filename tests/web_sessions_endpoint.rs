use anyhow::Result;
use chrono::NaiveDate;
use llmusage::{
    AppPaths, Dashboard, QueryFilter, ReportTimezone, TopSessionsQuery, TopSessionsSort,
    models::SourceKind, store::Store,
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

#[allow(clippy::too_many_arguments)]
fn insert_edge_event(
    store: &Store,
    event_key: &str,
    source: &str,
    model: &str,
    event_at: &str,
    session_id: Option<&str>,
    source_path_hash: Option<&str>,
    session_label: Option<&str>,
    project_hash: Option<&str>,
    project_label: Option<&str>,
    host_id: &str,
    total_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    cost_usd: f64,
) -> Result<()> {
    store.open_connection()?.execute(
        r#"
        INSERT INTO usage_event(
            event_key, source, model, event_at, hour_start,
            input_tokens, cache_creation_tokens, cache_read_tokens,
            output_tokens, reasoning_output_tokens, total_tokens,
            project_hash, project_label, path_hash, session_id, session_label,
            source_path_hash, host_id, created_at, cost_with_cache_usd,
            cost_without_cache_usd, pricing_status
        ) VALUES (?1, ?2, ?3, ?4, ?4, 0, 0, 0, ?12, ?13, ?11,
                  ?8, ?9, ?1, ?5, ?7, ?6, ?10, ?4, ?14, ?14, 'static')
        "#,
        rusqlite::params![
            event_key,
            source,
            model,
            event_at,
            session_id,
            source_path_hash,
            session_label,
            project_hash,
            project_label,
            host_id,
            total_tokens,
            output_tokens,
            reasoning_output_tokens,
            cost_usd,
        ],
    )?;
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

#[test]
fn top_sessions_keeps_complete_serialized_row_and_identity_fallbacks() -> Result<()> {
    let (_temp, store) = fixture()?;
    for event in [
        (
            "explicit-2",
            "codex",
            "gpt-a",
            "2026-05-02T00:30:00Z",
            Some("  explicit  "),
            Some("ignored-path"),
            Some("Zulu"),
            Some("project-a"),
            Some("Project Z"),
            "host-a",
            7,
            2,
            3,
            1.0,
        ),
        (
            "explicit-1",
            "codex",
            "gpt-a",
            "2026-05-02T00:00:00Z",
            Some("explicit"),
            None,
            Some("Alpha"),
            Some("project-a"),
            Some("Project A"),
            "host-a",
            5,
            1,
            1,
            1.0e16,
        ),
        (
            "explicit-3",
            "codex",
            "gpt-a",
            "2026-05-02T01:31:00Z",
            Some("explicit"),
            None,
            None,
            Some("project-a"),
            None,
            "host-a",
            9,
            4,
            0,
            -1.0e16,
        ),
        (
            "path-event",
            "claude",
            "gpt-b",
            "2026-05-03T00:00:00Z",
            Some("  "),
            Some("path-fallback"),
            Some("Path label"),
            None,
            None,
            "host-b",
            2,
            1,
            0,
            0.0,
        ),
        (
            "codex:thread-x:turn-y:event-1",
            "codex",
            "gpt-c",
            "2026-05-04T00:00:00Z",
            None,
            None,
            None,
            None,
            None,
            "host-c",
            3,
            1,
            0,
            0.0,
        ),
        (
            "opencode:event:fallback",
            "opencode",
            "gpt-d",
            "2026-05-05T00:00:00Z",
            None,
            None,
            None,
            None,
            None,
            "host-d",
            4,
            1,
            0,
            0.0,
        ),
    ] {
        insert_edge_event(
            &store, event.0, event.1, event.2, event.3, event.4, event.5, event.6, event.7,
            event.8, event.9, event.10, event.11, event.12, event.13,
        )?;
    }

    let dashboard = Dashboard::open(&store)?;
    let filter = QueryFilter {
        source: Some(SourceKind::Codex),
        model: Some("gpt-a".to_string()),
        project_hash: Some("project-a".to_string()),
        host_id: Some("host-a".to_string()),
        since: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
        until: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
        timezone: ReportTimezone::Utc,
    };
    for sort in [
        TopSessionsSort::Tokens,
        TopSessionsSort::Duration,
        TopSessionsSort::Cost,
    ] {
        let rows = dashboard.top_sessions(&TopSessionsQuery {
            filter: filter.clone(),
            sort,
            limit: 50,
        })?;
        assert_eq!(
            serde_json::to_string(&rows)?,
            concat!(
                "[{\"session_id\":\"codex:explicit\",\"session_label\":\"Alpha\",",
                "\"project_label\":\"Project A\",\"source\":\"codex\",",
                "\"first_event_at\":\"2026-05-02T00:00:00Z\",",
                "\"last_event_at\":\"2026-05-02T01:31:00Z\",",
                "\"total_tokens\":21,\"output_tokens\":11,\"cost_usd\":1.0,",
                "\"span_minutes\":91,\"active_minutes\":30,\"event_count\":3}]"
            ),
            "complete serialized row changed for sort={sort:?}"
        );
    }

    let ids = dashboard
        .top_sessions(&TopSessionsQuery {
            limit: 50,
            ..TopSessionsQuery::default()
        })?
        .into_iter()
        .map(|row| row.session_id)
        .collect::<Vec<_>>();
    assert!(ids.contains(&"claude:path-fallback".to_string()));
    assert!(ids.contains(&"codex:thread-x:turn-y".to_string()));
    assert!(ids.contains(&"opencode:event:fallback".to_string()));
    Ok(())
}
