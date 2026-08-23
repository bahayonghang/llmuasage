#[test]
fn compare_returns_candidates_metrics_and_low_sample_warning() -> Result<()> {
    let fixture = Fixture::new()?;
    for (index, model, category, has_edits, one_shot, retries) in [
        (0, "gpt-5", "coding", 1, 1, 0),
        (1, "gpt-5", "planning", 0, 0, 0),
        (2, "sonnet", "coding", 1, 0, 1),
        (3, "sonnet", "delegation", 1, 1, 0),
    ] {
        let event_key = format!("codex:compare:{index}");
        fixture.seed_event(crate::testing::SeedEvent {
            event_key: &event_key,
            source: "codex",
            model,
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 100 + index * 10,
            cache_read_tokens: 10,
            output_tokens: 50,
            total_tokens: 160 + index * 10,
            cost_with_cache_usd: 0.10 + (index as f64 * 0.01),
            cost_without_cache_usd: 0.10 + (index as f64 * 0.01),
            pricing_status: "static",
            pricing_source: Some("static-v1"),
            session_id: Some("session-compare"),
            source_path_hash: Some("path-compare"),
            created_at: Some("2026-05-01T00:00:00Z"),
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
            ) VALUES (?1, 'codex', 'session-compare', 'path-compare',
                'project-test', ?2, '2026-05-01T00:00:00Z', ?3,
                ?4, ?5, ?6, 1, 100, 10, 0, 50, 0, 160, '2026-05-01T00:00:00Z')
            "#,
            rusqlite::params![
                format!("turn:{event_key}"),
                model,
                category,
                has_edits,
                retries,
                one_shot
            ],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_tool_call(
                tool_call_key, turn_key, event_key, source, session_id,
                source_path_hash, project_hash, model, occurred_at, tool_name,
                tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
            ) VALUES (?1, ?2, ?3, 'codex', 'session-compare', 'path-compare',
                'project-test', ?4, '2026-05-01T00:00:00Z', 'Edit', 'edit',
                NULL, NULL, ?5, 'Edit src/lib.rs', '2026-05-01T00:00:00Z')
            "#,
            rusqlite::params![
                format!("tool:{event_key}"),
                format!("turn:{event_key}"),
                event_key,
                model,
                format!("fp-{index}")
            ],
        )?;
    }

    let dashboard = Dashboard::open(fixture.store())?;
    let candidates = dashboard.compare_models(&QueryFilter::default())?;
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().all(|candidate| candidate.low_sample));

    let compare =
        dashboard.model_compare(&QueryFilter::default(), Some("gpt-5"), Some("sonnet"))?;
    assert!(compare.support.supported);
    assert_eq!(compare.support.level, "low_sample");
    assert!(
        compare
            .warning
            .as_deref()
            .unwrap_or("")
            .contains("Low sample")
    );
    assert_eq!(compare.model_a.as_ref().unwrap().model, "gpt-5");
    assert_eq!(compare.model_b.as_ref().unwrap().model, "sonnet");
    assert!(
        compare
            .metrics
            .iter()
            .any(|metric| metric.id == "one_shot_rate")
    );
    assert!(
        compare
            .working_style
            .iter()
            .any(|metric| metric.id == "delegation_rate")
    );
    assert!(
        compare
            .category_head_to_head
            .iter()
            .any(|row| row.category == "coding")
    );
    assert_eq!(
        serde_json::to_value(&compare)?,
        serde_json::to_value(dashboard.legacy_model_compare(
            &QueryFilter::default(),
            Some("gpt-5"),
            Some("sonnet"),
        )?)?,
        "batched selected-model stats must preserve the complete low-sample payload"
    );

    let missing = dashboard.model_compare(
        &QueryFilter::default(),
        Some("gpt-5"),
        Some("missing-model"),
    )?;
    assert_eq!(missing.support.level, "missing_model");
    assert_eq!(
        serde_json::to_value(&missing)?,
        serde_json::to_value(dashboard.legacy_model_compare(
            &QueryFilter::default(),
            Some("gpt-5"),
            Some("missing-model"),
        )?)?,
        "missing-model warning and empty metric/category fields must remain unchanged"
    );

    let normalized_fixture = Fixture::new()?;
    normalized_fixture.seed_stress_dashboard(0, 0, 2)?;
    let normalized_dashboard = Dashboard::open(normalized_fixture.store())?;
    let normalized = normalized_dashboard.model_compare(
        &QueryFilter::default(),
        Some("stress-model-00"),
        Some("stress-model-01"),
    )?;
    assert_eq!(normalized.support.level, "normalized");
    assert_eq!(
        serde_json::to_value(&normalized)?,
        serde_json::to_value(normalized_dashboard.legacy_model_compare(
            &QueryFilter::default(),
            Some("stress-model-00"),
            Some("stress-model-01"),
        )?)?,
        "normalized metrics, categories, and working style must remain unchanged"
    );
    Ok(())
}
/// Legacy per-candidate N+1 oracle for the grouped turn query in
/// `compare_model_candidates`. Mirrors the pre-refactor implementation:
/// the unchanged bucket top-25 query plus one `usage_turn` aggregate per
/// candidate model.
fn legacy_compare_model_candidates(
    conn: &rusqlite::Connection,
    filter: &QueryFilter,
) -> Result<Vec<super::CompareModelCandidate>> {
    let bucket_filter = filter.bucket_filter(Some("b"));
    let sql = format!(
        r#"
        SELECT
            b.model,
            COALESCE(SUM(b.event_count), 0) AS calls,
            COALESCE(SUM(b.total_tokens), 0) AS total_tokens,
            COALESCE(SUM(b.cost_with_cache_usd), 0.0) AS estimated_cost_usd
        FROM usage_bucket_30m b
        {}
        GROUP BY b.model
        ORDER BY estimated_cost_usd DESC, total_tokens DESC, calls DESC, b.model ASC
        LIMIT 25
        "#,
        bucket_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(bucket_filter.params().iter()),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                row.get::<_, Option<f64>>(3)?.unwrap_or_default(),
            ))
        },
    )?;
    let mut candidates = Vec::new();
    for row in rows {
        let (model, calls, total_tokens, estimated_cost_usd) = row?;
        let mut model_filter = filter.clone();
        model_filter.model = Some(model.clone());
        let turn_filter = model_filter.turn_filter(Some("t"));
        let (turns, edit_turns): (i64, i64) = conn.query_row(
            &format!(
                "SELECT COUNT(*), COALESCE(SUM(t.has_edits), 0) FROM usage_turn t{}",
                turn_filter.where_sql()
            ),
            rusqlite::params_from_iter(turn_filter.params().iter()),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        candidates.push(super::CompareModelCandidate {
            model,
            calls,
            turns,
            edit_turns,
            total_tokens,
            estimated_cost_usd,
            low_sample: calls < 20 || edit_turns < 10,
        });
    }
    Ok(candidates)
}

/// Equivalence oracle: the grouped turn query must produce byte-identical
/// candidate JSON to the legacy N+1 across empty, partial-turn, full and
/// filtered scenarios.
#[test]
fn compare_candidates_match_legacy_n_plus_one_output() -> Result<()> {
    // Empty database: no candidates at all.
    let empty = Fixture::new()?;
    let empty_dashboard = Dashboard::open(empty.store())?;
    let new_empty = empty_dashboard.compare_models(&QueryFilter::default())?;
    let legacy_empty =
        legacy_compare_model_candidates(&empty_dashboard.conn, &QueryFilter::default())?;
    assert!(new_empty.is_empty() && legacy_empty.is_empty());

    // 25+ models (LIMIT 25 binds), one of them with buckets but zero
    // turns, plus per-model turn counts that differ.
    let fixture = Fixture::new()?;
    fixture.seed_stress_dashboard(0, 0, 30)?;
    let conn = fixture.store().open_connection()?;
    conn.execute(
        r#"
        INSERT INTO usage_bucket_30m(
            source, model, hour_start, project_hash, project_label, project_ref,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens,
            cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
            event_count, updated_at
        ) VALUES ('codex', 'no-turns-model', '2026-05-01T00:00:00Z', 'project-stress', 'Project Stress', NULL,
            100, 0, 0, 50, 0, 150, 9.99, 9.99, 'static', 'static-v1',
            6, '2026-05-05T00:00:00Z')
        "#,
        [],
    )?;
    drop(conn);

    let dashboard = Dashboard::open(fixture.store())?;
    let filters = [
        QueryFilter::default(),
        QueryFilter {
            source: Some(SourceKind::Codex),
            ..Default::default()
        },
        QueryFilter {
            model: Some("stress-model-03".to_string()),
            ..Default::default()
        },
        QueryFilter {
            project_hash: Some("project-stress".to_string()),
            ..Default::default()
        },
        QueryFilter {
            project_hash: Some("project-absent".to_string()),
            ..Default::default()
        },
        QueryFilter {
            since: Some(NaiveDate::from_ymd_opt(2026, 5, 2).expect("valid date")),
            until: Some(NaiveDate::from_ymd_opt(2026, 5, 3).expect("valid date")),
            timezone: ReportTimezone::Utc,
            ..Default::default()
        },
    ];
    for filter in &filters {
        let new_candidates = dashboard.compare_models(filter)?;
        let legacy_candidates = legacy_compare_model_candidates(&dashboard.conn, filter)?;
        assert_eq!(
            serde_json::to_value(&new_candidates)?,
            serde_json::to_value(&legacy_candidates)?,
            "candidate JSON must match the legacy N+1 output for filter {filter:?}"
        );
    }
    // The grouped result must contain the no-turns model with zeroed turn
    // stats (top bucket cost puts it first), matching legacy semantics.
    let candidates = dashboard.compare_models(&QueryFilter::default())?;
    let no_turns = candidates
        .iter()
        .find(|candidate| candidate.model == "no-turns-model")
        .expect("bucket-only model is a candidate");
    assert_eq!((no_turns.turns, no_turns.edit_turns), (0, 0));
    assert!(no_turns.low_sample);
    // And the full /api/compare payload stays field-equivalent to a
    // payload whose candidates come from the legacy oracle.
    let full = dashboard.model_compare(&QueryFilter::default(), None, None)?;
    let mut legacy_full = serde_json::to_value(&full)?;
    legacy_full["candidates"] = serde_json::to_value(legacy_compare_model_candidates(
        &dashboard.conn,
        &QueryFilter::default(),
    )?)?;
    assert_eq!(serde_json::to_value(&full)?, legacy_full);
    Ok(())
}

static COMPARE_TURN_STATEMENTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
static COMPARE_BUCKET_STATEMENTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
static COMPARE_TOOL_STATEMENTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn count_compare_turn_statements(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event
        && sql.contains("usage_turn")
    {
        COMPARE_TURN_STATEMENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn count_full_compare_statements(event: rusqlite::trace::TraceEvent<'_>) {
    let rusqlite::trace::TraceEvent::Stmt(_, sql) = event else {
        return;
    };
    if sql.contains("FROM usage_bucket_30m b") {
        COMPARE_BUCKET_STATEMENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    if sql.contains("FROM usage_turn t") {
        COMPARE_TURN_STATEMENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    if sql.contains("FROM usage_tool_call tc") {
        COMPARE_TOOL_STATEMENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// The grouped turn query runs exactly once regardless of candidate count
/// (previously once per candidate, up to 25).
#[test]
fn compare_candidates_use_constant_turn_query_count() -> Result<()> {
    for models in [2_usize, 25] {
        let fixture = Fixture::new()?;
        fixture.seed_stress_dashboard(0, 0, models)?;
        let dashboard = Dashboard::open(fixture.store())?;
        dashboard.conn.trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(count_compare_turn_statements),
        );
        COMPARE_TURN_STATEMENTS.store(0, std::sync::atomic::Ordering::Relaxed);
        let candidates = dashboard.compare_models(&QueryFilter::default())?;
        assert_eq!(candidates.len(), models);
        assert_eq!(
            COMPARE_TURN_STATEMENTS.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "compare candidates must run exactly one usage_turn query with {models} models"
        );
        dashboard
            .conn
            .trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, None);
    }
    Ok(())
}

#[test]
fn model_compare_batches_each_selected_model_query_family() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_stress_dashboard(0, 0, 2)?;
    let dashboard = Dashboard::open(fixture.store())?;
    dashboard.conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(count_full_compare_statements),
    );
    COMPARE_BUCKET_STATEMENTS.store(0, std::sync::atomic::Ordering::Relaxed);
    COMPARE_TURN_STATEMENTS.store(0, std::sync::atomic::Ordering::Relaxed);
    COMPARE_TOOL_STATEMENTS.store(0, std::sync::atomic::Ordering::Relaxed);

    let payload = dashboard.model_compare(
        &QueryFilter::default(),
        Some("stress-model-00"),
        Some("stress-model-01"),
    )?;
    assert_eq!(payload.support.level, "normalized");
    assert_eq!(
        COMPARE_BUCKET_STATEMENTS.load(std::sync::atomic::Ordering::Relaxed),
        2,
        "candidate selection and selected-model stats each use one bucket query"
    );
    assert_eq!(
        COMPARE_TURN_STATEMENTS.load(std::sync::atomic::Ordering::Relaxed),
        3,
        "candidate, selected-model, and category stats each use one turn query"
    );
    assert_eq!(
        COMPARE_TOOL_STATEMENTS.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "both selected models share one tool-count query"
    );
    dashboard
        .conn
        .trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, None);
    Ok(())
}
