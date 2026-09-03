#[test]
fn behavior_queries_return_activity_and_tool_breakdowns() -> Result<()> {
    let fixture = Fixture::new()?;
    let conn = fixture.store().open_connection()?;

    fixture.seed_event(crate::testing::SeedEvent {
        event_key: "codex:behavior:multi-tool",
        event_at: "2026-05-01T00:00:00Z",
        hour_start: Some("2026-05-01T00:00:00Z"),
        input_tokens: 120,
        output_tokens: 60,
        total_tokens: 180,
        cost_with_cache_usd: 1.00,
        cost_without_cache_usd: 1.00,
        pricing_status: "static",
        pricing_source: Some("static-v1"),
        session_id: Some("session-behavior"),
        source_path_hash: Some("path-behavior"),
        ..Default::default()
    })?;
    fixture.seed_event(crate::testing::SeedEvent {
        event_key: "codex:behavior:non-tool",
        event_at: "2026-05-02T01:00:00Z",
        hour_start: Some("2026-05-02T01:00:00Z"),
        input_tokens: 80,
        output_tokens: 20,
        total_tokens: 100,
        cost_with_cache_usd: 0.25,
        cost_without_cache_usd: 0.25,
        pricing_status: "static",
        pricing_source: Some("static-v1"),
        session_id: Some("session-behavior"),
        source_path_hash: Some("path-behavior"),
        ..Default::default()
    })?;
    conn.execute(
        r#"
        INSERT INTO usage_turn(
            turn_key, source, session_id, source_path_hash, project_hash,
            primary_model, started_at, category, has_edits, retries,
            one_shot, call_count, input_tokens, cache_read_tokens,
            cache_creation_tokens, output_tokens, reasoning_output_tokens,
            total_tokens, created_at
        ) VALUES ('turn:codex:behavior:multi-tool', 'codex', 'session-behavior',
            'path-behavior', 'project-test', 'gpt-5', '2026-05-01T00:00:00Z',
            'coding', 1, 0, 1, 1, 100, 0, 0, 50, 0, 150, '2026-05-01T00:00:00Z')
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
        ) VALUES ('turn:codex:behavior:non-tool', 'codex', 'session-behavior',
            'path-behavior', 'project-test', 'gpt-5', '2026-05-02T01:00:00Z',
            'coding', 0, 0, 0, 1, 80, 0, 0, 20, 0, 100, '2026-05-02T01:00:00Z')
        "#,
        [],
    )?;
    conn.execute(
        r#"
        INSERT INTO usage_tool_call(
            tool_call_key, turn_key, event_key, source, session_id,
            source_path_hash, project_hash, model, occurred_at, tool_name,
            tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
        ) VALUES ('tool:codex:behavior:multi-tool:edit',
            'turn:codex:behavior:multi-tool', 'codex:behavior:multi-tool', 'codex',
            'session-behavior', 'path-behavior', 'project-test', 'gpt-5',
            '2026-05-01T00:00:00Z', 'Edit', 'edit',
            NULL, NULL, 'fp-edit', 'Edit src/lib.rs', '2026-05-01T00:00:00Z')
        "#,
        [],
    )?;
    conn.execute(
        r#"
        INSERT INTO usage_tool_call(
            tool_call_key, turn_key, event_key, source, session_id,
            source_path_hash, project_hash, model, occurred_at, tool_name,
            tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
        ) VALUES ('tool:codex:behavior:multi-tool:read',
            'turn:codex:behavior:multi-tool', 'codex:behavior:multi-tool', 'codex',
            'session-behavior', 'path-behavior', 'project-test', 'gpt-5',
            '2026-05-01T00:00:00Z', 'Read', 'read',
            NULL, NULL, 'fp-read', 'Read src/lib.rs', '2026-05-01T00:00:00Z')
        "#,
        [],
    )?;
    drop(conn);

    let dashboard = Dashboard::open(fixture.store())?;
    let activity = dashboard.activity_breakdown(&QueryFilter {
        source: Some(SourceKind::Codex),
        model: Some("gpt-5".to_string()),
        ..Default::default()
    })?;
    assert!(activity.support.supported);
    assert_eq!(activity.breakdown.len(), 1);
    assert_eq!(activity.breakdown[0].category, "coding");
    assert_eq!(activity.breakdown[0].turns, 2);
    assert_eq!(activity.breakdown[0].one_shot_rate, 1.0);
    assert_eq!(activity.breakdown[0].estimated_cost_usd, 1.25);

    let tools = dashboard.tool_breakdown(&QueryFilter {
        source: Some(SourceKind::Codex),
        model: Some("gpt-5".to_string()),
        ..Default::default()
    })?;
    assert!(tools.support.supported);
    assert_eq!(tools.breakdown.len(), 3);
    let total_cost: f64 = tools
        .breakdown
        .iter()
        .map(|row| row.estimated_cost_usd)
        .sum();
    assert!((total_cost - 1.25).abs() < f64::EPSILON);

    let edit = tools
        .breakdown
        .iter()
        .find(|row| row.tool_name == "Edit")
        .expect("edit row");
    assert_eq!(edit.tool_kind, "edit");
    assert_eq!(edit.calls, 1);
    assert_eq!(edit.turn_count, 1);
    assert_eq!(edit.session_count, 1);
    assert_eq!(edit.call_share, 0.5);
    assert_eq!(edit.estimated_cost_usd, 0.5);

    let read = tools
        .breakdown
        .iter()
        .find(|row| row.tool_name == "Read")
        .expect("read row");
    assert_eq!(read.tool_kind, "read");
    assert_eq!(read.calls, 1);
    assert_eq!(read.turn_count, 1);
    assert_eq!(read.session_count, 1);
    assert_eq!(read.call_share, 0.5);
    assert_eq!(read.estimated_cost_usd, 0.5);

    let non_tool = tools
        .breakdown
        .iter()
        .find(|row| row.tool_name == "(non-tool)")
        .expect("non-tool row");
    assert_eq!(non_tool.tool_kind, "(non-tool)");
    assert_eq!(non_tool.calls, 0);
    assert_eq!(non_tool.turn_count, 1);
    assert_eq!(non_tool.session_count, 1);
    assert_eq!(non_tool.call_share, 0.0);
    assert_eq!(non_tool.estimated_cost_usd, 0.25);

    let day_one = dashboard.tool_breakdown(&QueryFilter {
        source: Some(SourceKind::Codex),
        model: Some("gpt-5".to_string()),
        since: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
        until: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
        timezone: ReportTimezone::Utc,
        ..Default::default()
    })?;
    assert_eq!(day_one.breakdown.len(), 2);
    let day_one_cost: f64 = day_one
        .breakdown
        .iter()
        .map(|row| row.estimated_cost_usd)
        .sum();
    assert!((day_one_cost - 1.0).abs() < f64::EPSILON);
    assert!(
        day_one
            .breakdown
            .iter()
            .all(|row| row.tool_name != "(non-tool)")
    );

    let day_two = dashboard.tool_breakdown(&QueryFilter {
        source: Some(SourceKind::Codex),
        model: Some("gpt-5".to_string()),
        since: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
        until: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
        timezone: ReportTimezone::Utc,
        ..Default::default()
    })?;
    assert_eq!(day_two.breakdown.len(), 1);
    let day_two_cost: f64 = day_two
        .breakdown
        .iter()
        .map(|row| row.estimated_cost_usd)
        .sum();
    assert!((day_two_cost - 0.25).abs() < f64::EPSILON);
    assert_eq!(day_two.breakdown[0].tool_name, "(non-tool)");

    let equivalence_filters = [
        QueryFilter::default(),
        QueryFilter {
            source: Some(SourceKind::Codex),
            model: Some("gpt-5".to_string()),
            ..Default::default()
        },
        QueryFilter {
            source: Some(SourceKind::Codex),
            model: Some("gpt-5".to_string()),
            since: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
            timezone: ReportTimezone::Utc,
            ..Default::default()
        },
    ];
    for filter in &equivalence_filters {
        assert_eq!(
            serde_json::to_value(dashboard.activity_breakdown(filter)?)?,
            serde_json::to_value(dashboard.legacy_activity_breakdown(filter)?)?,
            "Activity JSON must match the pre-SQL hashmap oracle for {filter:?}"
        );
        assert_eq!(
            serde_json::to_value(dashboard.tool_attribution_rows(filter)?)?,
            serde_json::to_value(dashboard.legacy_tool_attribution_rows(filter)?)?,
            "Tools attribution must match the pre-SQL rust oracle for {filter:?}"
        );
    }
    Ok(())
}

#[test]
fn activity_breakdown_sql_joins_or_filters_usage_event() -> Result<()> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    static SQL: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static UNFILTERED: AtomicBool = AtomicBool::new(false);

    fn is_unfiltered_usage_event_scan(sql: &str) -> bool {
        let compact = sql
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();
        compact.contains("from usage_event")
            && !compact.contains("join")
            && !compact.contains("where")
            && !compact.contains("group by")
    }

    fn capture(event: rusqlite::trace::TraceEvent<'_>) {
        if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
            if is_unfiltered_usage_event_scan(sql) {
                UNFILTERED.store(true, Ordering::Relaxed);
            }
            SQL.lock().expect("activity SQL lock").push(sql.to_string());
        }
    }

    let fixture = Fixture::new()?;
    fixture.seed_event(crate::testing::SeedEvent {
        event_key: "codex:activity-sql:1",
        event_at: "2026-05-01T00:00:00Z",
        hour_start: Some("2026-05-01T00:00:00Z"),
        total_tokens: 10,
        cost_with_cache_usd: 1.0,
        cost_without_cache_usd: 1.0,
        session_id: Some("session-sql"),
        ..Default::default()
    })?;
    let conn = fixture.store().open_connection()?;
    conn.execute(
        r#"
        INSERT INTO usage_turn(
            turn_key, source, session_id, source_path_hash, project_hash,
            primary_model, started_at, category, has_edits, retries,
            one_shot, call_count, input_tokens, cache_read_tokens,
            cache_creation_tokens, output_tokens, reasoning_output_tokens,
            total_tokens, created_at
        ) VALUES ('turn:codex:activity-sql:1', 'codex', 'session-sql',
            'path-sql', 'project-test', 'gpt-5', '2026-05-01T00:00:00Z',
            'coding', 1, 0, 1, 1, 10, 0, 0, 0, 0, 10, '2026-05-01T00:00:00Z')
        "#,
        [],
    )?;
    drop(conn);

    let dashboard = Dashboard::open(fixture.store())?;
    SQL.lock().expect("activity SQL lock").clear();
    UNFILTERED.store(false, Ordering::Relaxed);
    dashboard.conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(capture),
    );
    let payload = dashboard.activity_breakdown(&QueryFilter::default())?;
    dashboard
        .conn
        .trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, None);

    assert!(payload.support.supported);
    assert_eq!(payload.breakdown.len(), 1);
    assert_eq!(payload.breakdown[0].estimated_cost_usd, 1.0);
    assert!(
        !UNFILTERED.load(Ordering::Relaxed),
        "activity_breakdown must not scan usage_event without JOIN or WHERE: {:?}",
        SQL.lock().expect("activity SQL lock")
    );
    let sqls = SQL.lock().expect("activity SQL lock");
    assert!(
        sqls.iter().any(|sql| {
            let compact = sql
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase();
            compact.contains("from usage_turn") && compact.contains("join usage_event")
        }),
        "activity_breakdown must join filtered turns to events: {sqls:?}"
    );
    Ok(())
}

#[test]
fn tool_breakdown_sql_joins_and_groups_attribution() -> Result<()> {
    use std::sync::Mutex;

    static SQL: Mutex<Vec<String>> = Mutex::new(Vec::new());
    fn capture(event: rusqlite::trace::TraceEvent<'_>) {
        if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
            SQL.lock().expect("tool SQL lock").push(sql.to_string());
        }
    }

    let fixture = Fixture::new()?;
    fixture.seed_event(crate::testing::SeedEvent {
        event_key: "codex:tool-sql:1",
        event_at: "2026-05-01T00:00:00Z",
        hour_start: Some("2026-05-01T00:00:00Z"),
        total_tokens: 10,
        cost_with_cache_usd: 1.0,
        cost_without_cache_usd: 1.0,
        session_id: Some("session-sql"),
        ..Default::default()
    })?;
    let conn = fixture.store().open_connection()?;
    conn.execute(
        r#"
        INSERT INTO usage_tool_call(
            tool_call_key, turn_key, event_key, source, session_id,
            source_path_hash, project_hash, model, occurred_at, tool_name,
            tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
        ) VALUES ('tool:codex:tool-sql:1', 'turn:codex:tool-sql:1',
            'codex:tool-sql:1', 'codex', 'session-sql', 'path-sql',
            'project-test', 'gpt-5', '2026-05-01T00:00:00Z', 'Read', 'read',
            NULL, NULL, 'fp-read', 'Read', '2026-05-01T00:00:00Z')
        "#,
        [],
    )?;
    drop(conn);

    let dashboard = Dashboard::open(fixture.store())?;
    SQL.lock().expect("tool SQL lock").clear();
    dashboard.conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(capture),
    );
    let payload = dashboard.tool_breakdown(&QueryFilter::default())?;
    dashboard
        .conn
        .trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, None);

    assert!(payload.support.supported);
    assert_eq!(payload.breakdown.len(), 1);
    assert_eq!(payload.breakdown[0].tool_name, "Read");
    assert_eq!(payload.breakdown[0].estimated_cost_usd, 1.0);
    let sqls = SQL.lock().expect("tool SQL lock");
    assert!(
        sqls.iter().any(|sql| {
            let compact = sql
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase();
            compact.contains("from usage_tool_call")
                && compact.contains("join usage_event")
                && compact.contains("group by")
        }),
        "tool_breakdown must join tools to events and aggregate in SQL: {sqls:?}"
    );
    Ok(())
}

#[test]
fn activity_serialization_is_identical_before_and_after_v19_index() -> Result<()> {
    let fixture = Fixture::new()?;
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(
        r#"
        CREATE TABLE usage_event (
            event_key TEXT PRIMARY KEY,
            cost_with_cache_usd REAL
        );
        CREATE TABLE usage_turn (
            turn_key TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            project_hash TEXT,
            primary_model TEXT NOT NULL,
            started_at TEXT NOT NULL,
            category TEXT NOT NULL,
            has_edits INTEGER NOT NULL,
            retries INTEGER NOT NULL,
            one_shot INTEGER NOT NULL,
            call_count INTEGER NOT NULL,
            total_tokens INTEGER NOT NULL
        );
        INSERT INTO usage_event(event_key, cost_with_cache_usd) VALUES
            ('codex:activity-index:coding', 1.0),
            ('codex:activity-index:planning', 1.0),
            ('claude:activity-index:null-cost', NULL);
        INSERT INTO usage_turn(
            turn_key, source, project_hash, primary_model, started_at,
            category, has_edits, retries, one_shot, call_count, total_tokens
        ) VALUES
            ('turn:codex:activity-index:coding', 'codex', 'project-a', 'gpt-5',
             '2026-05-01T01:00:00Z', 'coding', 1, 0, 1, 1, 100),
            ('turn:codex:activity-index:planning', 'codex', 'project-a', 'gpt-5',
             '2026-05-01T02:00:00Z', 'planning', 0, 0, 0, 1, 100),
            ('turn:claude:activity-index:null-cost', 'claude', 'project-b',
             'claude-sonnet-4', '2026-05-02T01:00:00Z', 'review', 0, 1, 0, 1, 50),
            ('turn:missing:activity-index:event', 'codex', 'project-a', 'gpt-5',
             '2026-05-01T03:00:00Z', 'exploration', 0, 0, 0, 1, 20);
        "#,
    )?;
    let dashboard = Dashboard {
        store: fixture.store().clone(),
        conn,
    };

    let day_one = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
    let filters = vec![
        ("default", QueryFilter::default()),
        (
            "source",
            QueryFilter {
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
        ),
        (
            "model",
            QueryFilter {
                model: Some("gpt-5".to_string()),
                ..Default::default()
            },
        ),
        (
            "project",
            QueryFilter {
                project_hash: Some("project-a".to_string()),
                ..Default::default()
            },
        ),
        (
            "date",
            QueryFilter {
                since: Some(day_one),
                until: Some(day_one),
                timezone: ReportTimezone::Utc,
                ..Default::default()
            },
        ),
        (
            "no-data",
            QueryFilter {
                model: Some("missing-model".to_string()),
                ..Default::default()
            },
        ),
    ];

    let default_payload = dashboard.activity_breakdown(&filters[0].1)?;
    assert_eq!(
        default_payload
            .breakdown
            .iter()
            .take(2)
            .map(|row| row.category.as_str())
            .collect::<Vec<_>>(),
        vec!["coding", "planning"],
        "equal aggregates must retain the category tie-break"
    );
    assert_eq!(
        default_payload
            .breakdown
            .iter()
            .find(|row| row.category == "planning")
            .expect("planning category")
            .edit_turns,
        0
    );

    let mut baseline = Vec::with_capacity(filters.len());
    for (label, filter) in &filters {
        let current = serde_json::to_vec(&dashboard.activity_breakdown(filter)?)?;
        let legacy = serde_json::to_vec(&dashboard.legacy_activity_breakdown(filter)?)?;
        assert_eq!(current, legacy, "pre-index legacy mismatch for {label}");
        baseline.push(current);
    }
    dashboard.conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_usage_event_activity_cost
            ON usage_event(event_key, cost_with_cache_usd);
        "#,
    )?;
    for ((label, filter), expected) in filters.iter().zip(baseline) {
        let current = serde_json::to_vec(&dashboard.activity_breakdown(filter)?)?;
        let legacy = serde_json::to_vec(&dashboard.legacy_activity_breakdown(filter)?)?;
        assert_eq!(current, legacy, "post-index legacy mismatch for {label}");
        assert_eq!(
            current, expected,
            "Activity serialization changed after creating v19 index for {label}"
        );
    }
    Ok(())
}

#[test]
fn tool_attribution_preserves_filter_asymmetry_and_excludes_orphans() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_event(crate::testing::SeedEvent {
        event_key: "codex:asymmetry:linked",
        model: "event-model",
        event_at: "2026-05-01T00:00:00Z",
        hour_start: Some("2026-05-01T00:00:00Z"),
        total_tokens: 100,
        cost_with_cache_usd: 1.0,
        cost_without_cache_usd: 1.0,
        pricing_status: "static",
        pricing_source: Some("static-v1"),
        session_id: Some("event-session"),
        ..Default::default()
    })?;
    fixture.seed_event(crate::testing::SeedEvent {
        event_key: "codex:asymmetry:non-tool",
        model: "tool-model",
        event_at: "2026-05-02T00:00:00Z",
        hour_start: Some("2026-05-02T00:00:00Z"),
        total_tokens: 50,
        cost_with_cache_usd: 0.5,
        cost_without_cache_usd: 0.5,
        pricing_status: "static",
        pricing_source: Some("static-v1"),
        session_id: Some("non-tool-session"),
        ..Default::default()
    })?;
    let conn = fixture.store().open_connection()?;
    conn.execute_batch(
        r#"
        INSERT INTO usage_tool_call(
            tool_call_key, turn_key, event_key, source, session_id,
            source_path_hash, project_hash, model, occurred_at, tool_name,
            tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
        ) VALUES
            ('tool:asymmetry:filtered', NULL, 'codex:asymmetry:linked', 'codex', NULL,
             'path-test', 'project-test', 'tool-model', '2026-05-02T00:00:00Z',
             'Read', 'read', NULL, NULL, 'read-filtered', 'Read filtered', '2026-05-02T00:00:00Z'),
            ('tool:asymmetry:sibling', 'turn:sibling', 'codex:asymmetry:linked', 'codex', 'sibling-session',
             'path-test', 'project-test', 'event-model', '2026-05-01T00:00:00Z',
             'Edit', 'edit', NULL, NULL, 'edit-sibling', 'Edit sibling', '2026-05-01T00:00:00Z'),
            ('tool:asymmetry:orphan', NULL, 'codex:asymmetry:missing', 'codex', NULL,
             'path-test', 'project-test', 'tool-model', '2026-05-02T00:00:00Z',
             'Orphan', 'read', NULL, NULL, 'orphan', 'Read orphan', '2026-05-02T00:00:00Z');
        "#,
    )?;
    drop(conn);

    let dashboard = Dashboard::open(fixture.store())?;
    let filter = QueryFilter {
        source: Some(SourceKind::Codex),
        model: Some("tool-model".to_string()),
        since: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
        until: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
        timezone: ReportTimezone::Utc,
        ..Default::default()
    };
    let tools = dashboard.tool_breakdown(&filter)?;
    assert!(tools.support.supported);
    assert_eq!(tools.breakdown.len(), 2);
    assert!(tools.breakdown.iter().all(|row| row.tool_name != "Orphan"));
    let read = tools
        .breakdown
        .iter()
        .find(|row| row.tool_name == "Read")
        .expect("filtered linked tool");
    assert_eq!(read.turn_count, 1);
    assert_eq!(read.session_count, 1);
    assert_eq!(read.estimated_cost_usd, 1.0);
    let non_tool = tools
        .breakdown
        .iter()
        .find(|row| row.tool_name == "(non-tool)")
        .expect("filtered non-tool event");
    assert_eq!(non_tool.estimated_cost_usd, 0.5);
    assert_eq!(
        serde_json::to_value(dashboard.tool_attribution_rows(&filter)?)?,
        serde_json::to_value(dashboard.legacy_tool_attribution_rows(&filter)?)?
    );
    Ok(())
}

#[test]
fn behavior_query_plans_use_v18_indexes() -> Result<()> {
    let fixture = Fixture::new()?;
    let conn = fixture.store().open_connection()?;
    let plans = [
        (
            "idx_usage_event_event_at",
            super::explain_query_plan(
                &conn,
                "SELECT event_key FROM usage_event WHERE event_at >= ?1 AND event_at < ?2",
                ["2026-05-01T00:00:00Z", "2026-05-02T00:00:00Z"],
            )?,
        ),
        (
            "idx_usage_turn_started_at",
            super::explain_query_plan(
                &conn,
                "SELECT category FROM usage_turn WHERE started_at >= ?1 AND started_at < ?2",
                ["2026-05-01T00:00:00Z", "2026-05-02T00:00:00Z"],
            )?,
        ),
        (
            "idx_usage_turn_session_id",
            super::explain_query_plan(
                &conn,
                "SELECT turn_key FROM usage_turn WHERE session_id = ?1",
                ["session-plan"],
            )?,
        ),
        (
            "idx_usage_tool_call_event_key",
            super::explain_query_plan(
                &conn,
                "SELECT tool_call_key FROM usage_tool_call WHERE event_key = ?1",
                ["codex:plan:1"],
            )?,
        ),
        (
            "idx_usage_tool_call_occurred_at",
            super::explain_query_plan(
                &conn,
                "SELECT tool_call_key FROM usage_tool_call WHERE occurred_at >= ?1 AND occurred_at < ?2",
                ["2026-05-01T00:00:00Z", "2026-05-02T00:00:00Z"],
            )?,
        ),
        (
            "idx_usage_tool_call_model_occurred",
            super::explain_query_plan(
                &conn,
                "SELECT COUNT(*) FROM usage_tool_call WHERE model = ?1 AND occurred_at >= ?2",
                ["gpt-5", "2026-05-01T00:00:00Z"],
            )?,
        ),
        (
            "idx_usage_turn_event_key_expr",
            super::explain_query_plan(
                &conn,
                "SELECT turn_key FROM usage_turn WHERE substr(turn_key, 6) = ?1",
                ["codex:plan:1"],
            )?,
        ),
    ];
    for (index, plan) in plans {
        let details = plan.join("\n");
        assert!(
            details.contains(index),
            "expected {index} in query plan, got:\n{details}"
        );
    }
    Ok(())
}

#[test]
fn behavior_queries_return_explicit_no_data_support() -> Result<()> {
    let fixture = Fixture::new()?;
    let dashboard = Dashboard::open(fixture.store())?;

    let activity = dashboard.activity_breakdown(&QueryFilter::default())?;
    let tools = dashboard.tool_breakdown(&QueryFilter::default())?;

    assert!(!activity.support.supported);
    assert_eq!(activity.support.level, "no_data");
    assert!(activity.breakdown.is_empty());
    assert!(!tools.support.supported);
    assert_eq!(tools.support.level, "no_data");
    assert!(tools.breakdown.is_empty());
    Ok(())
}

#[test]
fn zombie_report_diffs_installed_against_used() -> Result<()> {
    use super::InventoryRoots;

    let fixture = Fixture::new()?;
    let conn = fixture.store().open_connection()?;
    let seed = |source: &str, kind: &str, name: &str, server: Option<&str>| -> Result<()> {
        conn.execute(
            r#"INSERT INTO usage_tool_call(
                tool_call_key, source, occurred_at, tool_name, tool_kind, mcp_server, created_at
            ) VALUES (?1, ?2, '2026-05-01T00:00:00Z', ?3, ?4, ?5, '2026-05-01T00:00:00Z')"#,
            rusqlite::params![
                format!("tc:{source}:{kind}:{name}"),
                source,
                name,
                kind,
                server
            ],
        )?;
        Ok(())
    };
    // Used set: claude skill alpha, claude mcp context7, opencode skill gamma.
    seed("claude", "skill", "alpha", None)?;
    seed("claude", "mcp", "context7/search", Some("context7"))?;
    seed("opencode", "skill", "gamma", None)?;

    // Installed roots in a temp tree (superset of the used set).
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    let write = |rel: &str, body: &str| {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    };
    write("claude/skills/alpha/SKILL.md", "x");
    write("claude/skills/beta/SKILL.md", "x");
    write(
        "claude.json",
        r#"{"mcpServers":{"context7":{},"playwright":{}}}"#,
    );
    write("opencode/skills/gamma/SKILL.md", "x");
    write("opencode/skills/delta/SKILL.md", "x");
    write("opencode/opencode.json", r#"{"mcp":{}}"#);
    let roots = InventoryRoots {
        claude_skills: root.join("claude/skills"),
        claude_mcp_config: root.join("claude.json"),
        codex_skills: root.join("codex/skills"),
        codex_mcp_config: root.join("codex/config.toml"),
        opencode_skills: root.join("opencode/skills"),
        opencode_mcp_config: root.join("opencode/opencode.json"),
    };

    let report = Dashboard::open(fixture.store())?.zombie_report(&roots)?;
    let zombies: std::collections::BTreeSet<(String, String, String)> = report
        .zombies
        .iter()
        .map(|item| (item.source.clone(), item.kind.clone(), item.name.clone()))
        .collect();

    // Installed but never called → zombie candidates.
    assert!(zombies.contains(&("claude".into(), "skill".into(), "beta".into())));
    assert!(zombies.contains(&("claude".into(), "mcp".into(), "playwright".into())));
    assert!(zombies.contains(&("opencode".into(), "skill".into(), "delta".into())));
    // Actually-used items are never flagged.
    assert!(!zombies.iter().any(|item| item.2 == "alpha"));
    assert!(!zombies.iter().any(|item| item.2 == "context7"));
    assert!(!zombies.iter().any(|item| item.2 == "gamma"));
    Ok(())
}

#[test]
fn optimize_returns_read_only_findings_from_behavior_facts() -> Result<()> {
    let fixture = Fixture::new()?;
    for index in 0..8 {
        let event_key = format!("codex:optimize:{index}");
        fixture.seed_event(crate::testing::SeedEvent {
            event_key: &event_key,
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            cost_with_cache_usd: 0.10,
            cost_without_cache_usd: 0.10,
            pricing_status: "static",
            pricing_source: Some("static-v1"),
            session_id: Some("session-optimize"),
            source_path_hash: Some("path-optimize"),
            created_at: Some("2026-05-01T00:00:00Z"),
            ..Default::default()
        })?;
    }
    let conn = fixture.store().open_connection()?;
    for index in 0..8 {
        conn.execute(
            r#"
            INSERT INTO usage_turn(
                turn_key, source, session_id, source_path_hash, project_hash,
                primary_model, started_at, category, has_edits, retries,
                one_shot, call_count, input_tokens, cache_read_tokens,
                cache_creation_tokens, output_tokens, reasoning_output_tokens,
                total_tokens, created_at
            ) VALUES (?1, 'codex', 'session-optimize', 'path-optimize',
                'project-test', 'gpt-5', '2026-05-01T00:00:00Z', 'coding',
                1, 0, 1, 1, 100, 0, 0, 50, 0, 150, '2026-05-01T00:00:00Z')
            "#,
            [format!("turn:codex:optimize:{index}")],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_tool_call(
                tool_call_key, turn_key, event_key, source, session_id,
                source_path_hash, project_hash, model, occurred_at, tool_name,
                tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
            ) VALUES (?1, ?2, ?3, 'codex', 'session-optimize', 'path-optimize',
                'project-test', 'gpt-5', '2026-05-01T00:00:00Z', 'Edit', 'edit',
                NULL, NULL, ?4, 'Edit src/lib.rs', '2026-05-01T00:00:00Z')
            "#,
            rusqlite::params![
                format!("tool:edit:{index}"),
                format!("turn:codex:optimize:{index}"),
                format!("codex:optimize:{index}"),
                format!("fp-edit-{index}")
            ],
        )?;
    }
    for index in 0..3 {
        conn.execute(
            r#"
            INSERT INTO usage_tool_call(
                tool_call_key, turn_key, event_key, source, session_id,
                source_path_hash, project_hash, model, occurred_at, tool_name,
                tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
            ) VALUES (?1, 'turn:codex:optimize:0', 'codex:optimize:0',
                'codex', 'session-optimize', 'path-optimize', 'project-test',
                'gpt-5', '2026-05-01T00:00:00Z', 'Read', 'read',
                NULL, NULL, 'fp-node-modules', 'Read node_modules/pkg/index.js',
                '2026-05-01T00:00:00Z')
            "#,
            [format!("tool:read:{index}")],
        )?;
    }
    drop(conn);

    let dashboard = Dashboard::open(fixture.store())?;
    let filtered = QueryFilter {
        source: Some(SourceKind::Codex),
        model: Some("gpt-5".to_string()),
        ..Default::default()
    };
    let optimize = dashboard.optimize(&filtered)?;

    assert!(optimize.support.supported);
    assert!(optimize.score < 100);
    assert!(
        optimize
            .findings
            .iter()
            .any(|finding| finding.id == "low_read_edit_ratio")
    );
    assert!(
        optimize
            .findings
            .iter()
            .any(|finding| finding.id == "duplicate_reads")
    );
    assert!(
        optimize
            .findings
            .iter()
            .any(|finding| finding.id == "junk_reads")
    );
    assert!(optimize.estimated_savings_tokens > 0);
    assert!(optimize.findings.iter().all(|finding| {
        !finding
            .recommendation
            .to_ascii_lowercase()
            .contains("delete")
    }));
    assert_eq!(
        serde_json::to_value(&optimize)?,
        serde_json::to_value(dashboard.legacy_optimize(&filtered)?)?,
        "optimized detectors must preserve the complete filtered payload"
    );

    let no_match = QueryFilter {
        project_hash: Some("project-with-no-behavior".to_string()),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(dashboard.optimize(&no_match)?)?,
        serde_json::to_value(dashboard.legacy_optimize(&no_match)?)?,
        "negative/no-data detector output must remain unchanged"
    );
    Ok(())
}
