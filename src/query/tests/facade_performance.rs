#[test]
fn home_overview_includes_all_sources_in_by_platform() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(12)?;
    let payload = Dashboard::open(fixture.store())?.home_overview(&Default::default())?;

    for descriptor in crate::domain::source_descriptor::registered_source_descriptors() {
        assert!(
            payload.by_platform.contains_key(descriptor.stable_id),
            "home overview must keep a card for registered source {}",
            descriptor.stable_id
        );
    }
    for source in [
        "claude",
        "codex",
        "antigravity",
        "opencode",
        "pi",
        "omp",
        "grok",
        "kimi_code",
    ] {
        assert!(payload.by_platform.contains_key(source));
    }
    assert!(payload.by_platform["codex"].requests > 0);
    assert!(payload.by_platform["claude"].requests > 0);
    assert!(payload.by_platform["opencode"].requests > 0);
    assert_eq!(payload.by_platform["antigravity"].requests, 0);
    assert!(!payload.series.is_empty());
    Ok(())
}

#[test]
fn home_overview_sql_aggregates_without_full_event_projection() -> Result<()> {
    use std::sync::Mutex;

    static SQL: Mutex<Vec<String>> = Mutex::new(Vec::new());
    fn capture(event: rusqlite::trace::TraceEvent<'_>) {
        if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
            SQL.lock().expect("home SQL lock").push(sql.to_string());
        }
    }

    let fixture = Fixture::new()?;
    fixture.seed_dashboard(48)?;
    let dashboard = Dashboard::open(fixture.store())?;
    SQL.lock().expect("home SQL lock").clear();
    dashboard.conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(capture),
    );
    let payload = dashboard.home_overview(&QueryFilter::default())?;
    let compact = dashboard.home_overview_compact(&QueryFilter::default())?;
    dashboard
        .conn
        .trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, None);
    assert_home_overview_projection_equivalent(&compact, &payload)?;

    let sqls = SQL.lock().expect("home SQL lock");
    let usage_event_sqls = sqls
        .iter()
        .filter(|sql| {
            sql.to_ascii_lowercase().contains("from usage_event")
                && !sql.to_ascii_lowercase().contains("explain")
        })
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        usage_event_sqls
            .iter()
            .any(|sql| sql.contains("COUNT(DISTINCT")),
        "home overview must aggregate sessions in SQL: {usage_event_sqls:?}"
    );
    assert!(
        usage_event_sqls.iter().any(|sql| sql.contains("GROUP BY")),
        "home overview must group in SQL instead of folding every event: {usage_event_sqls:?}"
    );
    assert!(
        usage_event_sqls.iter().all(|sql| {
            let compact = sql
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase();
            compact.contains("count(") || compact.contains("sum(") || compact.contains("group by")
        }),
        "home overview must not project every usage_event row: {usage_event_sqls:?}"
    );
    assert!(
        usage_event_sqls.iter().all(|sql| {
            let compact = sql
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase();
            !compact.contains("select source,") || compact.contains("count(")
        }),
        "home overview must not load a per-event source/session projection: {usage_event_sqls:?}"
    );
    Ok(())
}

#[test]
fn home_overview_archive_by_source_empty_when_source_file_unseeded() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(3)?;
    let payload = Dashboard::open(fixture.store())?.home_overview(&Default::default())?;

    assert_eq!(
        payload.archive.archive_root,
        fixture.store().paths.root_dir.display().to_string()
    );
    assert!(payload.archive.by_source.is_empty());
    assert_eq!(payload.archive.recent_failures.len(), 1);
    Ok(())
}

#[test]
fn home_overview_preserves_exact_session_day_and_filter_semantics() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:cross-day:1",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-04-01T23:30:00Z",
        hour_start: Some("2026-04-01T23:00:00Z"),
        session_id: Some("session-shared"),
        project_hash: "project-a",
        input_tokens: 10,
        total_tokens: 10,
        ..Default::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "codex:cross-day:2",
        source: "codex",
        model: "gpt-5",
        event_at: "2026-04-02T00:30:00Z",
        hour_start: Some("2026-04-02T00:00:00Z"),
        session_id: Some("session-shared"),
        project_hash: "project-a",
        input_tokens: 20,
        total_tokens: 20,
        ..Default::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "claude:fallback:1",
        source: "claude",
        model: "claude-sonnet-4",
        event_at: "2026-04-02T01:00:00Z",
        hour_start: Some("2026-04-02T01:00:00Z"),
        source_path_hash: Some("claude-path"),
        project_hash: "project-b",
        input_tokens: 30,
        total_tokens: 30,
        ..Default::default()
    })?;
    fixture.seed_event(SeedEvent {
        event_key: "opencode:fallback:1",
        source: "opencode",
        model: "gpt-5",
        event_at: "2026-04-02T16:00:00Z",
        hour_start: Some("2026-04-02T16:00:00Z"),
        source_path_hash: Some("opencode-path"),
        project_hash: "project-c",
        input_tokens: 40,
        total_tokens: 40,
        ..Default::default()
    })?;
    fixture.store().open_connection()?.execute(
        "INSERT INTO run_log(command, status, started_at, finished_at) VALUES ('sync', 'success', '2026-04-03T00:00:00Z', '2026-04-03T00:01:00Z')",
        [],
    )?;

    let dashboard = Dashboard::open(fixture.store())?;
    let all_filter = QueryFilter {
        timezone: ReportTimezone::Iana(chrono_tz::Asia::Shanghai),
        ..Default::default()
    };
    let payload = dashboard.home_overview(&all_filter)?;
    let compact = dashboard.home_overview_compact(&all_filter)?;
    let (profiled_payload, _) = home_overview::load_profile(&dashboard, &all_filter)?;
    assert_eq!(
        serde_json::to_value(&payload)?,
        serde_json::to_value(&profiled_payload)?
    );
    assert_home_overview_projection_equivalent(&compact, &payload)?;
    assert_eq!(payload.summary.total_sessions, 3);
    assert_eq!(payload.summary.total_requests, 4);
    assert_eq!(payload.summary.total_tokens, 100);
    assert_eq!(payload.summary.active_days, 2);
    assert_eq!(payload.by_platform["codex"].sessions, 1);
    assert_eq!(payload.by_platform["codex"].requests, 2);
    assert_eq!(payload.by_platform["claude"].sessions, 1);
    assert_eq!(payload.by_platform["opencode"].sessions, 1);
    assert_eq!(payload.series.len(), 2);
    assert_eq!(payload.series[0].date, "2026-04-02");
    assert_eq!(payload.series[0].codex.sessions, 1);
    assert_eq!(payload.series[0].codex.requests, 2);
    assert_eq!(payload.series[0].claude.sessions, 1);
    assert_eq!(payload.series[1].date, "2026-04-03");
    assert_eq!(payload.series[1].opencode.sessions, 1);
    assert!(payload.bootstrap.usage_import_attempted);
    assert!(payload.bootstrap.is_warm);
    assert_eq!(payload.last_updated, "2026-04-03T00:01:00Z");

    let filtered_filter = QueryFilter {
        source: Some(SourceKind::Codex),
        model: Some("gpt-5".to_string()),
        project_hash: Some("project-a".to_string()),
        since: Some(NaiveDate::from_ymd_opt(2026, 4, 2).expect("valid date")),
        until: Some(NaiveDate::from_ymd_opt(2026, 4, 2).expect("valid date")),
        timezone: ReportTimezone::Iana(chrono_tz::Asia::Shanghai),
        ..Default::default()
    };
    let filtered = dashboard.home_overview(&filtered_filter)?;
    let filtered_compact = dashboard.home_overview_compact(&filtered_filter)?;
    assert_home_overview_projection_equivalent(&filtered_compact, &filtered)?;
    assert_eq!(filtered.summary.total_sessions, 1);
    assert_eq!(filtered.summary.total_requests, 2);
    assert_eq!(filtered.summary.total_tokens, 30);
    assert_eq!(filtered.summary.active_days, 1);
    assert_eq!(filtered.by_platform["codex"].requests, 2);
    assert_eq!(filtered.by_platform["claude"].requests, 0);
    assert_eq!(filtered.series.len(), 1);
    assert_eq!(filtered.series[0].date, "2026-04-02");
    assert_eq!(filtered.series[0].codex.sessions, 1);
    Ok(())
}

#[test]
fn home_overview_under_80ms_with_seeded_10k_events() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(10_000)?;
    let dashboard = Dashboard::open(fixture.store())?;
    let started = std::time::Instant::now();

    let (payload, timing) = home_overview::load_profile(&dashboard, &Default::default())?;
    eprintln!("home_overview profile: {timing:?}");
    let elapsed = started.elapsed();
    let limit = if std::env::var_os("CI").is_some() {
        std::time::Duration::from_millis(500)
    } else {
        std::time::Duration::from_millis(150)
    };

    assert_eq!(payload.summary.total_requests, 10_000);
    assert!(
        elapsed < limit,
        "home_overview should stay below {limit:?} with 10k seeded events, got {elapsed:?}"
    );
    Ok(())
}

#[test]
fn home_overview_profiles_configured_read_only_backup() -> Result<()> {
    let Some(db_path) = std::env::var_os("LLMUSAGE_HOME_OVERVIEW_BACKUP_DB") else {
        return Ok(());
    };
    let db_path = std::path::PathBuf::from(db_path);
    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let event_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
    let bucket_count: i64 = conn.query_row("SELECT COUNT(*) FROM usage_bucket_30m", [], |row| {
        row.get(0)
    })?;
    drop(conn);

    for run in 0..5 {
        let (_, timing) = home_overview::load_profile_read_only(&db_path, &Default::default())?;
        eprintln!(
            "home_overview real backup run={run} events={event_count} buckets={bucket_count} timing={timing:?}"
        );
    }
    Ok(())
}

static DASHBOARD_STRUCTURE_STATEMENTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn count_dashboard_structure_statements(event: rusqlite::trace::TraceEvent<'_>) {
    if matches!(event, rusqlite::trace::TraceEvent::Stmt(_, _)) {
        DASHBOARD_STRUCTURE_STATEMENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn normalize_dashboard_payload<T: serde::Serialize>(payload: &T) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(payload)?;
    fn normalize(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                if let Some(generated_at) = fields.get_mut("generated_at") {
                    *generated_at = serde_json::Value::String("stable".to_string());
                }
                if let Some(archive_root) = fields.get_mut("archive_root") {
                    *archive_root = serde_json::Value::String("stable".to_string());
                }
                for value in fields.values_mut() {
                    normalize(value);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    normalize(value);
                }
            }
            _ => {}
        }
    }
    normalize(&mut value);
    Ok(serde_json::to_vec(&value)?)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

/// Structural parity harness. Run on the pre-move and post-move trees with:
/// `cargo test --lib measure_dashboard_structure_parity -- --ignored --nocapture --test-threads=1`.
#[test]
#[ignore = "explicit before/after dashboard structure parity harness"]
fn measure_dashboard_structure_parity() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(180)?;
    let dashboard = Dashboard::open(fixture.store())?;
    let filter = QueryFilter::default();
    dashboard.conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(count_dashboard_structure_statements),
    );

    macro_rules! measure {
        ($name:literal, $call:expr) => {{
            let _ = $call?;
            let mut samples = Vec::new();
            let mut statements = 0;
            let mut payload = Vec::new();
            for _ in 0..5 {
                DASHBOARD_STRUCTURE_STATEMENTS.store(0, std::sync::atomic::Ordering::Relaxed);
                let started = std::time::Instant::now();
                let value = $call?;
                samples.push(started.elapsed().as_secs_f64() * 1_000.0);
                statements = DASHBOARD_STRUCTURE_STATEMENTS.load(std::sync::atomic::Ordering::Relaxed);
                payload = normalize_dashboard_payload(&value)?;
            }
            samples.sort_by(f64::total_cmp);
            eprintln!(
                "structure shape={} statements={} payload_bytes={} payload_fnv1a64={:016x} p50_ms={:.3} p95_ms={:.3}",
                $name,
                statements,
                payload.len(),
                fnv1a64(&payload),
                samples[2],
                samples[4],
            );
        }};
    }

    measure!("full", dashboard.snapshot(&filter));
    measure!("core", dashboard.core_snapshot(&filter));
    measure!(
        "interactive",
        dashboard.interactive_snapshot(&filter, "all")
    );
    dashboard
        .conn
        .trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, None);
    Ok(())
}

/// Measurement-only baseline for the serve dashboard query path task.
/// Run explicitly: `cargo test --lib measure_stress_diagnostics_and_full_sections -- --ignored --nocapture --test-threads=1`
#[test]
#[ignore = "measurement test; run explicitly for baseline/after reports"]
fn measure_stress_diagnostics_and_full_sections() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_stress_dashboard(4_000, 1_000, 25)?;
    let conn = fixture.store().open_connection()?;
    let source_file_rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM source_file", [], |row| row.get(0))?;
    let turn_rows: i64 = conn.query_row("SELECT COUNT(*) FROM usage_turn", [], |row| row.get(0))?;
    let model_rows: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT model) FROM usage_bucket_30m",
        [],
        |row| row.get(0),
    )?;
    drop(conn);
    eprintln!(
        "stress scale: source_file={source_file_rows} usage_turn={turn_rows} models={model_rows}"
    );

    let dashboard = Dashboard::open(fixture.store())?;
    let filter = QueryFilter::default();
    for run in 0..3 {
        super::reset_diagnostics_stat_counter();
        let started = std::time::Instant::now();
        let diagnostics = dashboard.diagnostics()?;
        eprintln!(
            "diagnostics run={run} elapsed={:?} stat_calls={} by_source={}",
            started.elapsed(),
            super::diagnostics_stat_calls(),
            diagnostics.by_source.len()
        );
    }

    #[allow(clippy::type_complexity)]
    let timed: [(&str, &dyn Fn(&Dashboard) -> crate::error::Result<()>); 7] = [
        ("core_snapshot", &|d| d.core_snapshot(&filter).map(|_| ())),
        ("activity", &|d| d.activity_breakdown(&filter).map(|_| ())),
        ("tools", &|d| d.tool_breakdown(&filter).map(|_| ())),
        ("optimize", &|d| d.optimize(&filter).map(|_| ())),
        ("compare", &|d| {
            d.model_compare(&filter, None, None).map(|_| ())
        }),
        ("explorer", &|d| {
            d.explorer(&super::ExplorerQuery {
                filter: filter.clone(),
                ..Default::default()
            })
            .map(|_| ())
        }),
        ("full_snapshot_export", &|d| d.snapshot(&filter).map(|_| ())),
    ];
    for (section, run_fn) in timed {
        let started = std::time::Instant::now();
        run_fn(&dashboard)?;
        eprintln!("section {section} elapsed={:?}", started.elapsed());
    }
    Ok(())
}

/// Measurement against a read-only copy of a representative database.
/// Set `LLMUSAGE_MEASURE_HOME` to the copied runtime root (the directory
/// that contains `llmusage.db`) and run with `--ignored --nocapture`.
#[test]
#[ignore = "measurement test; requires LLMUSAGE_MEASURE_HOME copy"]
fn measure_real_copy_diagnostics_and_full_sections() -> Result<()> {
    let Some(root) = std::env::var_os("LLMUSAGE_MEASURE_HOME") else {
        eprintln!("LLMUSAGE_MEASURE_HOME not set; skipping real-copy measurement");
        return Ok(());
    };
    let paths = crate::paths::AppPaths::with_root(std::path::PathBuf::from(root))?;
    let db_path = paths.db_path.clone();
    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let scale: Vec<(&str, i64)> = [
        "source_file",
        "usage_event",
        "usage_bucket_30m",
        "usage_turn",
    ]
    .iter()
    .map(|table| {
        let count = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })?;
        Ok((*table, count))
    })
    .collect::<Result<Vec<_>>>()?;
    let models: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT model) FROM usage_bucket_30m",
        [],
        |row| row.get(0),
    )?;
    eprintln!("real copy scale: {scale:?} models={models}");

    for run in 0..5 {
        super::reset_diagnostics_stat_counter();
        let started = std::time::Instant::now();
        let rows = super::load_source_diagnostics(&conn)?;
        eprintln!(
            "real diagnostics run={run} elapsed={:?} stat_calls={} by_source={}",
            started.elapsed(),
            super::diagnostics_stat_calls(),
            rows.len()
        );
    }
    drop(conn);

    // Section breakdown through the normal Dashboard facade. This is a
    // file copy, so opening it read-write here never touches the original.
    let store = crate::store::Store::new(&paths)?;
    let dashboard = Dashboard::open(&store)?;
    let filter = QueryFilter::default();
    #[allow(clippy::type_complexity)]
    let timed: [(&str, &dyn Fn(&Dashboard) -> crate::error::Result<()>); 6] = [
        ("core_snapshot", &|d| d.core_snapshot(&filter).map(|_| ())),
        ("activity", &|d| d.activity_breakdown(&filter).map(|_| ())),
        ("tools", &|d| d.tool_breakdown(&filter).map(|_| ())),
        ("optimize", &|d| d.optimize(&filter).map(|_| ())),
        ("compare", &|d| {
            d.model_compare(&filter, None, None).map(|_| ())
        }),
        ("explorer", &|d| {
            d.explorer(&super::ExplorerQuery {
                filter: filter.clone(),
                ..Default::default()
            })
            .map(|_| ())
        }),
    ];
    for (section, run_fn) in timed {
        let started = std::time::Instant::now();
        run_fn(&dashboard)?;
        eprintln!("real section {section} elapsed={:?}", started.elapsed());
    }
    Ok(())
}
