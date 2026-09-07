fn seed_recent_event(fixture: &Fixture, event_key: &str, tokens: i64) -> Result<()> {
    let event_at = crate::util::now_utc();
    fixture.seed_event(SeedEvent {
        event_key,
        event_at: &event_at,
        hour_start: Some(&event_at),
        input_tokens: tokens,
        total_tokens: tokens,
        ..Default::default()
    })?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotDbVersion {
    total_tokens: i64,
    total_events: i64,
    last_24h_tokens: i64,
    model_tokens: i64,
    source_tokens: i64,
    host_tokens: i64,
    project_tokens: i64,
    trend_tokens: i64,
}

impl SnapshotDbVersion {
    fn aligned(total_tokens: i64, total_events: i64) -> Self {
        Self {
            total_tokens,
            total_events,
            last_24h_tokens: total_tokens,
            model_tokens: total_tokens,
            source_tokens: total_tokens,
            host_tokens: total_tokens,
            project_tokens: total_tokens,
            trend_tokens: total_tokens,
        }
        .assert_grouped()
    }

    fn from_overview(
        overview: &super::OverviewPayload,
        grouping: [i64; 5],
    ) -> Self {
        let [model_tokens, source_tokens, host_tokens, project_tokens, trend_tokens] = grouping;
        Self {
            total_tokens: overview.total.total_tokens,
            total_events: overview.total_events,
            last_24h_tokens: overview.last_24h.total_tokens,
            model_tokens,
            source_tokens,
            host_tokens,
            project_tokens,
            trend_tokens,
        }
        .assert_grouped()
    }

    fn assert_grouped(self) -> Self {
        assert_eq!(
            self.model_tokens, self.total_tokens,
            "model grouping must match overview tokens"
        );
        assert_eq!(
            self.source_tokens, self.total_tokens,
            "source grouping must match overview tokens"
        );
        assert_eq!(
            self.host_tokens, self.total_tokens,
            "host grouping must match overview tokens"
        );
        assert_eq!(
            self.project_tokens, self.total_tokens,
            "project grouping must match overview tokens"
        );
        assert_eq!(
            self.trend_tokens, self.total_tokens,
            "all-window trends must match overview tokens"
        );
        self
    }
}

fn sum_tokens<T>(rows: &[T], token: impl Fn(&T) -> i64) -> i64 {
    rows.iter().map(token).sum()
}

fn run_barrier_snapshot<T>(
    extra_key: &'static str,
    load: impl Fn(&Dashboard) -> crate::error::Result<T>,
    version_of: impl Fn(&T) -> SnapshotDbVersion,
) -> Result<()> {
    const BASE_TOKENS: i64 = 40;
    const EXTRA_TOKENS: i64 = 100_000;

    let fixture = Fixture::new()?;
    seed_recent_event(&fixture, "codex:snapshot-consistency:base", BASE_TOKENS)?;
    let dashboard = Dashboard::open(fixture.store())?;

    std::thread::scope(|scope| -> Result<()> {
        let (pause_tx, pause_rx) = std::sync::mpsc::sync_channel::<()>(0);
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel::<()>(0);
        scope.spawn(move || {
            if pause_rx.recv().is_err() {
                return;
            }
            seed_recent_event(&fixture, extra_key, EXTRA_TOKENS).expect("writer commit");
            done_tx.send(()).expect("writer finished");
        });
        let _hooks = super::snapshot::set_snapshot_section_barrier(move || {
            pause_tx.send(()).expect("signal writer");
            done_rx.recv().expect("writer committed");
        });
        let during = load(&dashboard).expect("composite snapshot during writer commit");
        let during_version = version_of(&during);
        assert_eq!(
            during_version,
            SnapshotDbVersion::aligned(BASE_TOKENS, 1),
            "overview, grouping, and trends must stay on the pre-commit SQLite version"
        );
        Ok(())
    })?;

    let after = load(&dashboard)?;
    assert_eq!(
        version_of(&after),
        SnapshotDbVersion::aligned(BASE_TOKENS + EXTRA_TOKENS, 2),
        "the next composite snapshot must observe the committed event"
    );
    Ok(())
}

#[test]
fn composite_snapshot_keeps_one_sqlite_version_when_writer_commits_between_sections() -> Result<()>
{
    run_barrier_snapshot(
        "codex:snapshot-consistency:full-extra",
        |dashboard| dashboard.snapshot(&Default::default()),
        |snap| {
            SnapshotDbVersion::from_overview(
                &snap.overview,
                [
                    sum_tokens(&snap.models, |row| row.total_tokens),
                    sum_tokens(&snap.sources, |row| row.total_tokens),
                    sum_tokens(&snap.hosts, |row| row.total_tokens),
                    sum_tokens(&snap.projects, |row| row.total_tokens),
                    sum_tokens(&snap.all_trends, |row| row.total_tokens),
                ],
            )
        },
    )?;
    run_barrier_snapshot(
        "codex:snapshot-consistency:core-extra",
        |dashboard| dashboard.core_snapshot(&Default::default()),
        |snap| {
            SnapshotDbVersion::from_overview(
                &snap.overview,
                [
                    sum_tokens(&snap.models, |row| row.total_tokens),
                    sum_tokens(&snap.sources, |row| row.total_tokens),
                    sum_tokens(&snap.hosts, |row| row.total_tokens),
                    sum_tokens(&snap.projects, |row| row.total_tokens),
                    sum_tokens(&snap.all_trends, |row| row.total_tokens),
                ],
            )
        },
    )?;
    run_barrier_snapshot(
        "codex:snapshot-consistency:interactive-extra",
        |dashboard| dashboard.interactive_snapshot(&Default::default(), "all"),
        |snap| {
            SnapshotDbVersion::from_overview(
                &snap.overview,
                [
                    sum_tokens(&snap.models, |row| row.total_tokens),
                    sum_tokens(&snap.sources, |row| row.total_tokens),
                    sum_tokens(&snap.hosts, |row| row.total_tokens),
                    sum_tokens(&snap.projects, |row| row.total_tokens),
                    sum_tokens(&snap.trends, |row| row.total_tokens),
                ],
            )
        },
    )?;
    Ok(())
}

#[test]
fn snapshot_query_error_and_interrupt_end_read_transaction() -> Result<()> {
    let fixture = Fixture::new()?;
    seed_recent_event(&fixture, "codex:snapshot-consistency:error", 7)?;
    let dashboard = Dashboard::open(fixture.store())?;

    {
        let _hooks = super::snapshot::fail_next_snapshot_section();
        let err = dashboard
            .core_snapshot(&Default::default())
            .expect_err("forced section query must fail");
        assert!(
            matches!(err, crate::error::LlmusageError::Db(_)),
            "section query error should surface as a database error: {err}"
        );
    }
    assert!(
        dashboard.connection().is_autocommit(),
        "query error must end the snapshot read transaction"
    );
    dashboard.overview(&Default::default())?;
    dashboard.core_snapshot(&Default::default())?;

    {
        let _hooks = super::snapshot::interrupt_next_snapshot_section();
        let err = dashboard
            .interactive_snapshot(&Default::default(), "all")
            .expect_err("SQLite interrupt must fail the composite snapshot");
        match err {
            crate::error::LlmusageError::Db(db) => {
                assert_eq!(
                    db.sqlite_error_code(),
                    Some(rusqlite::ErrorCode::OperationInterrupted)
                );
            }
            other => panic!("expected SQLite interrupt, got {other}"),
        }
    }
    assert!(
        dashboard.connection().is_autocommit(),
        "SQLite interrupt must end the snapshot read transaction"
    );
    dashboard.snapshot(&Default::default())?;
    Ok(())
}

#[test]
fn composite_snapshot_without_writer_matches_and_keeps_one_connection() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(24)?;
    seed_recent_event(&fixture, "codex:snapshot-consistency:stable", 12)?;

    Store::reset_open_connection_counter();
    let dashboard = Dashboard::open(fixture.store())?;
    assert_eq!(Store::open_connection_count(), 1);

    let first = dashboard.snapshot(&Default::default())?;
    let second = dashboard.snapshot(&Default::default())?;
    let core = dashboard.core_snapshot(&Default::default())?;
    let interactive = dashboard.interactive_snapshot(&Default::default(), "all")?;
    assert_eq!(Store::open_connection_count(), 1);
    assert!(
        dashboard.connection().is_autocommit(),
        "a successful composite snapshot must end the read transaction"
    );

    let first_version = SnapshotDbVersion::from_overview(
        &first.overview,
        [
            sum_tokens(&first.models, |row| row.total_tokens),
            sum_tokens(&first.sources, |row| row.total_tokens),
            sum_tokens(&first.hosts, |row| row.total_tokens),
            sum_tokens(&first.projects, |row| row.total_tokens),
            sum_tokens(&first.all_trends, |row| row.total_tokens),
        ],
    );
    let second_version = SnapshotDbVersion::from_overview(
        &second.overview,
        [
            sum_tokens(&second.models, |row| row.total_tokens),
            sum_tokens(&second.sources, |row| row.total_tokens),
            sum_tokens(&second.hosts, |row| row.total_tokens),
            sum_tokens(&second.projects, |row| row.total_tokens),
            sum_tokens(&second.all_trends, |row| row.total_tokens),
        ],
    );
    let core_version = SnapshotDbVersion::from_overview(
        &core.overview,
        [
            sum_tokens(&core.models, |row| row.total_tokens),
            sum_tokens(&core.sources, |row| row.total_tokens),
            sum_tokens(&core.hosts, |row| row.total_tokens),
            sum_tokens(&core.projects, |row| row.total_tokens),
            sum_tokens(&core.all_trends, |row| row.total_tokens),
        ],
    );
    let interactive_version = SnapshotDbVersion::from_overview(
        &interactive.overview,
        [
            sum_tokens(&interactive.models, |row| row.total_tokens),
            sum_tokens(&interactive.sources, |row| row.total_tokens),
            sum_tokens(&interactive.hosts, |row| row.total_tokens),
            sum_tokens(&interactive.projects, |row| row.total_tokens),
            sum_tokens(&interactive.trends, |row| row.total_tokens),
        ],
    );
    assert_eq!(first_version, second_version);
    assert_eq!(first_version, core_version);
    assert_eq!(first_version, interactive_version);
    assert_eq!(
        serde_json::to_value(&first.models)?,
        serde_json::to_value(&second.models)?
    );
    assert_eq!(
        serde_json::to_value(&first.sources)?,
        serde_json::to_value(&second.sources)?
    );
    assert_eq!(
        serde_json::to_value(&first.all_trends)?,
        serde_json::to_value(&second.all_trends)?
    );
    Ok(())
}

#[test]
fn independent_health_and_diagnostics_do_not_open_snapshot_transaction() -> Result<()> {
    let fixture = Fixture::new()?;
    seed_recent_event(&fixture, "codex:snapshot-consistency:diag", 3)?;
    let dashboard = Dashboard::open(fixture.store())?;
    assert!(dashboard.connection().is_autocommit());
    dashboard.diagnostics()?;
    assert!(
        dashboard.connection().is_autocommit(),
        "independent diagnostics must not claim the composite snapshot transaction"
    );
    dashboard.health()?;
    dashboard.health_summary()?;
    assert!(dashboard.connection().is_autocommit());
    Ok(())
}

fn insert_missing_source_file(fixture: &Fixture) -> Result<()> {
    let conn = fixture.store().open_connection()?;
    conn.execute(
        "INSERT INTO source_file(source, file_path, state, last_state_change_at) VALUES ('codex', ?1, 'live', '2026-07-11T00:00:00Z')",
        [fixture
            .paths()
            .root_dir
            .join("missing-snapshot.jsonl")
            .display()
            .to_string()],
    )?;
    Ok(())
}

#[test]
fn composite_snapshot_file_stats_match_one_diagnostics_pass() -> Result<()> {
    let fixture = Fixture::new()?;
    seed_recent_event(&fixture, "codex:snapshot-consistency:stats", 5)?;
    insert_missing_source_file(&fixture)?;
    let dashboard = Dashboard::open(fixture.store())?;

    super::reset_diagnostics_stat_counter();
    dashboard.diagnostics()?;
    let diagnostics_scans = super::diagnostics_stat_calls();
    assert!(
        diagnostics_scans > 0,
        "fixture must exercise Path::exists so file-stat counting is meaningful"
    );

    super::reset_diagnostics_stat_counter();
    dashboard.snapshot(&Default::default())?;
    assert_eq!(
        super::diagnostics_stat_calls(),
        diagnostics_scans,
        "full snapshot must file-scan diagnostics once, before the metrics transaction"
    );

    super::reset_diagnostics_stat_counter();
    dashboard.core_snapshot(&Default::default())?;
    assert_eq!(super::diagnostics_stat_calls(), diagnostics_scans);

    super::reset_diagnostics_stat_counter();
    dashboard.interactive_snapshot(&Default::default(), "all")?;
    assert_eq!(super::diagnostics_stat_calls(), diagnostics_scans);

    super::reset_diagnostics_stat_counter();
    let diagnostics = dashboard.diagnostics()?;
    let scans_before_metrics = super::diagnostics_stat_calls();
    dashboard.core_snapshot_with_diagnostics(&Default::default(), &diagnostics)?;
    dashboard.interactive_snapshot_with_diagnostics(&Default::default(), "all", &diagnostics)?;
    assert_eq!(
        super::diagnostics_stat_calls(),
        scans_before_metrics,
        "metrics transaction must not run diagnostics Path::exists scans"
    );
    Ok(())
}

static SNAPSHOT_BEGIN_STATEMENTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn count_snapshot_begin_statements(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
        let sql = sql.trim_start();
        if sql.len() >= 5 && sql[..5].eq_ignore_ascii_case("begin") {
            SNAPSHOT_BEGIN_STATEMENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

#[test]
fn composite_snapshot_starts_one_deferred_transaction() -> Result<()> {
    let fixture = Fixture::new()?;
    seed_recent_event(&fixture, "codex:snapshot-consistency:begin", 9)?;
    let dashboard = Dashboard::open(fixture.store())?;
    dashboard.conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(count_snapshot_begin_statements),
    );

    SNAPSHOT_BEGIN_STATEMENTS.store(0, std::sync::atomic::Ordering::Relaxed);
    dashboard.snapshot(&Default::default())?;
    assert_eq!(
        SNAPSHOT_BEGIN_STATEMENTS.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "snapshot must BEGIN once and reuse that transaction for nested core composition"
    );

    SNAPSHOT_BEGIN_STATEMENTS.store(0, std::sync::atomic::Ordering::Relaxed);
    dashboard.core_snapshot(&Default::default())?;
    assert_eq!(
        SNAPSHOT_BEGIN_STATEMENTS.load(std::sync::atomic::Ordering::Relaxed),
        1
    );

    SNAPSHOT_BEGIN_STATEMENTS.store(0, std::sync::atomic::Ordering::Relaxed);
    dashboard.interactive_snapshot(&Default::default(), "all")?;
    assert_eq!(
        SNAPSHOT_BEGIN_STATEMENTS.load(std::sync::atomic::Ordering::Relaxed),
        1
    );

    dashboard
        .conn
        .trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, None);
    dashboard.diagnostics()?;
    dashboard.health()?;
    assert!(dashboard.connection().is_autocommit());
    Ok(())
}
