use std::{fs, path::PathBuf};

use anyhow::Result;
use llmusage::{
    app::AppContext,
    commands,
    models::SourceKind,
    parsers::SyncEvent,
    query::{
        Dashboard, QueryFilter, ReportTimezone,
        reports::{ReportFilter, SortOrder, load_daily_report},
    },
    store::{Store, expected_token_accounting_version},
};
use rusqlite::Connection;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

#[test]
fn ccusage_token_semantics_are_consistent_across_sources_and_queries() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;
    fixture.seed_claude_streaming_and_sidechain_replay()?;
    fixture.seed_opencode_authoritative_total()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;

        for source in [SourceKind::Codex, SourceKind::Claude, SourceKind::Opencode] {
            commands::sync::run_once_with_options(
                &app,
                &store,
                0,
                &commands::sync::SyncRunOptions {
                    source: Some(source),
                    ..Default::default()
                },
                None,
            )
            .await?;
        }

        let conn = Connection::open(&app.paths.db_path)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT source, input_tokens, cache_creation_tokens, cache_read_tokens,
                   output_tokens, reasoning_output_tokens, total_tokens
            FROM usage_event
            ORDER BY source
            "#,
        )?;
        let events = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert_eq!(
            events,
            vec![
                ("claude".to_string(), 20, 0, 5, 10, 0, 35),
                ("codex".to_string(), 60, 0, 40, 30, 10, 130),
                ("opencode".to_string(), 100, 40, 20, 30, 7, 250),
            ]
        );

        let event_total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(total_tokens), 0) FROM usage_event",
            [],
            |row| row.get(0),
        )?;
        let bucket_total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(total_tokens), 0) FROM usage_bucket_30m",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(event_total, 415);
        assert_eq!(bucket_total, event_total);

        let overview = Dashboard::open(&store)?.overview(&Default::default())?;
        assert_eq!(overview.total.total_tokens, event_total);
        let filter = ReportFilter {
            filter: QueryFilter {
                timezone: ReportTimezone::Utc,
                ..QueryFilter::default()
            },
            order: SortOrder::Asc,
            locale: "en-US".to_string(),
            project: None,
            breakdown: true,
        };
        let daily = load_daily_report(&store.open_connection()?, &filter)?;
        assert_eq!(daily.totals.total_tokens, event_total);

        let (cost, rate_json): (f64, String) = conn.query_row(
            "SELECT cost_with_cache_usd, pricing_rate FROM usage_event WHERE source = 'codex'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let rate: serde_json::Value = serde_json::from_str(&rate_json)?;
        let expected_cost = (60.0 * rate["input_per_mtok"].as_f64().unwrap()
            + 40.0 * rate["cached_per_mtok"].as_f64().unwrap()
            + 30.0 * rate["output_per_mtok"].as_f64().unwrap())
            / 1_000_000.0;
        assert!((cost - expected_cost).abs() <= 1e-9);
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, Some(3));
        assert_eq!(store.token_accounting_version(SourceKind::Claude)?, Some(2));
        assert_eq!(
            store.token_accounting_version(SourceKind::Opencode)?,
            Some(2)
        );

        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn ordinary_sync_keeps_unparseable_legacy_history_and_warns_for_explicit_rebuild() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_opencode_authoritative_total()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Opencode),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        store.clear_token_accounting_version(SourceKind::Opencode)?;
        fixture.break_opencode_schema()?;
        assert!(store.has_legacy_token_accounting(SourceKind::Opencode)?);
        let before = source_history_snapshot(&store, SourceKind::Opencode)?;
        let marker_before = store.token_accounting_version(SourceKind::Opencode)?;

        let (mut tx, mut rx) = tokio::sync::mpsc::channel(256);
        commands::sync::run_once_with_options(&app, &store, 0, &options, Some(&mut tx)).await?;
        let events = drain_events(&mut rx);

        assert_eq!(
            source_history_snapshot(&store, SourceKind::Opencode)?,
            before
        );
        assert_eq!(
            store.token_accounting_version(SourceKind::Opencode)?,
            marker_before
        );
        assert!(store.has_legacy_token_accounting(SourceKind::Opencode)?);
        assert_no_repair_claim(&events);
        assert!(!events.iter().any(|event| {
            matches!(
                event,
                SyncEvent::SourceStarted {
                    source: SourceKind::Opencode,
                    ..
                }
            )
        }));
        let status = loaded_source_status(&store, "opencode")?;
        assert!(status.legacy_token_accounting);
        assert_explicit_repair_warning(
            status.token_accounting_warning.as_deref(),
            SourceKind::Opencode,
        );

        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn ordinary_sync_skips_every_legacy_source_and_preserves_parserless_history() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;
    fixture.seed_claude_streaming_and_sidechain_replay()?;
    fixture.seed_opencode_authoritative_total()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            None,
        )
        .await?;
        let legacy = [SourceKind::Codex, SourceKind::Claude, SourceKind::Opencode];
        let mut before = Vec::new();
        for source in legacy {
            store.clear_token_accounting_version(source)?;
            before.push(source_history_snapshot(&store, source)?);
        }
        seed_antigravity_history(&store)?;

        let (mut tx, mut rx) = tokio::sync::mpsc::channel(256);
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            Some(&mut tx),
        )
        .await?;
        let events = drain_events(&mut rx);

        assert_no_repair_claim(&events);
        for (index, source) in legacy.into_iter().enumerate() {
            assert_eq!(source_history_snapshot(&store, source)?, before[index]);
            assert!(store.has_legacy_token_accounting(source)?);
            assert_eq!(store.token_accounting_version(source)?, None);
            assert!(!events.iter().any(|event| {
                matches!(
                    event,
                    SyncEvent::SourceStarted {
                        source: actual,
                        ..
                    } if *actual == source
                )
            }));
            let status = loaded_source_status(&store, source.as_str())?;
            assert!(status.legacy_token_accounting);
            assert_explicit_repair_warning(status.token_accounting_warning.as_deref(), source);
        }
        assert_eq!(
            source_row_count(&store, "usage_event", SourceKind::Antigravity)?,
            1
        );
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(expected_token_accounting_version(SourceKind::Antigravity))
        );
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn ordinary_sync_skips_only_legacy_in_a_mixed_run_and_stays_idempotent() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;
    fixture.seed_claude_streaming_and_sidechain_replay()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            None,
        )
        .await?;
        store.set_meta_value("token_accounting_version.codex", "2")?;
        let codex_before = source_history_snapshot(&store, SourceKind::Codex)?;
        let claude_before = source_history_snapshot(&store, SourceKind::Claude)?;

        let (mut tx, mut rx) = tokio::sync::mpsc::channel(256);
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            Some(&mut tx),
        )
        .await?;
        let events = drain_events(&mut rx);
        assert_no_repair_claim(&events);
        assert!(!events.iter().any(|event| {
            matches!(
                event,
                SyncEvent::SourceStarted {
                    source: SourceKind::Codex,
                    ..
                }
            )
        }));
        let claude_stats = events
            .iter()
            .find_map(|event| match event {
                SyncEvent::SourceFinished {
                    source: SourceKind::Claude,
                    stats,
                } => Some(stats.clone()),
                _ => None,
            })
            .expect("claude stats");
        assert_eq!(claude_stats.changed_files, 0);
        assert!(claude_stats.skipped_files > 0);
        assert_eq!(
            source_history_snapshot(&store, SourceKind::Codex)?,
            codex_before
        );
        assert_usage_rows_eq(
            &source_history_snapshot(&store, SourceKind::Claude)?,
            &claude_before,
        );
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, Some(2));
        assert_eq!(store.token_accounting_version(SourceKind::Claude)?, Some(2));
        assert!(store.has_legacy_token_accounting(SourceKind::Codex)?);
        assert!(!store.has_legacy_token_accounting(SourceKind::Claude)?);

        let (mut tx, mut rx) = tokio::sync::mpsc::channel(256);
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            Some(&mut tx),
        )
        .await?;
        let second = drain_events(&mut rx);
        let claude_second = second
            .iter()
            .find_map(|event| match event {
                SyncEvent::SourceFinished {
                    source: SourceKind::Claude,
                    stats,
                } => Some(stats),
                _ => None,
            })
            .expect("claude second stats");
        assert_eq!(claude_second.changed_files, 0);
        assert_eq!(claude_second.events_inserted, 0);
        assert!(claude_second.skipped_files > 0);
        assert_eq!(
            source_history_snapshot(&store, SourceKind::Codex)?,
            codex_before
        );
        assert_usage_rows_eq(
            &source_history_snapshot(&store, SourceKind::Claude)?,
            &claude_before,
        );
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, Some(2));
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn bounded_sync_skips_legacy_history_instead_of_resetting() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let source_options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Codex),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &source_options, None).await?;
        store.set_meta_value("token_accounting_version.codex", "2")?;
        let before = source_history_snapshot(&store, SourceKind::Codex)?;

        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Codex),
                recent_days: Some(30),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_eq!(source_history_snapshot(&store, SourceKind::Codex)?, before);
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, Some(2));
        let status = loaded_source_status(&store, "codex")?;
        assert!(status.legacy_token_accounting);
        assert_explicit_repair_warning(
            status.token_accounting_warning.as_deref(),
            SourceKind::Codex,
        );
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn ordinary_sync_ignores_lossy_opt_in_and_skips_legacy_writes() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let source_options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Codex),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &source_options, None).await?;
        fixture.remove_codex_inputs()?;
        commands::sync::run_once_with_options(&app, &store, 0, &source_options, None).await?;
        store.clear_token_accounting_version(SourceKind::Codex)?;
        let before = source_history_snapshot(&store, SourceKind::Codex)?;

        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Codex),
                allow_lossy_rebuild: true,
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_eq!(source_history_snapshot(&store, SourceKind::Codex)?, before);
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, None);
        assert!(store.has_legacy_token_accounting(SourceKind::Codex)?);
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn ordinary_sync_skips_lossy_and_safe_legacy_sources_without_resetting_either() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;
    fixture.seed_claude_streaming_and_sidechain_replay()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            None,
        )
        .await?;
        fixture.remove_codex_inputs()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            None,
        )
        .await?;
        store.clear_token_accounting_version(SourceKind::Codex)?;
        store.clear_token_accounting_version(SourceKind::Claude)?;
        let codex_before = source_history_snapshot(&store, SourceKind::Codex)?;
        let claude_before = source_history_snapshot(&store, SourceKind::Claude)?;

        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            None,
        )
        .await?;

        assert_eq!(
            source_history_snapshot(&store, SourceKind::Codex)?,
            codex_before
        );
        assert_eq!(
            source_history_snapshot(&store, SourceKind::Claude)?,
            claude_before
        );
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, None);
        assert_eq!(store.token_accounting_version(SourceKind::Claude)?, None);
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn targeted_current_sync_ignores_unselected_legacy_source() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;
    fixture.seed_claude_streaming_and_sidechain_replay()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            None,
        )
        .await?;
        store.clear_token_accounting_version(SourceKind::Claude)?;
        let claude_before = source_row_count(&store, "usage_event", SourceKind::Claude)?;

        let (mut tx, mut rx) = tokio::sync::mpsc::channel(256);
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            Some(&mut tx),
        )
        .await?;
        drop(tx);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }

        assert_no_repair_claim(&events);
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, Some(3));
        assert_eq!(store.token_accounting_version(SourceKind::Claude)?, None);
        assert_eq!(
            source_row_count(&store, "usage_event", SourceKind::Claude)?,
            claude_before
        );
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn empty_and_preset_current_sources_are_not_skipped_as_legacy() -> Result<()> {
    let _fixture = Fixture::new()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        seed_antigravity_history(&store)?;

        for source in [SourceKind::Codex, SourceKind::Antigravity] {
            let (mut tx, rx) = tokio::sync::mpsc::channel(256);
            commands::sync::run_once_with_options(
                &app,
                &store,
                0,
                &commands::sync::SyncRunOptions {
                    source: Some(source),
                    ..Default::default()
                },
                Some(&mut tx),
            )
            .await?;
            drop(tx);
            let events = drain_recv_events(rx).await;
            assert_no_repair_claim(&events);
        }

        assert_eq!(
            source_row_count(&store, "usage_event", SourceKind::Codex)?,
            0
        );
        assert_eq!(
            store.token_accounting_version(SourceKind::Codex)?,
            Some(expected_token_accounting_version(SourceKind::Codex)),
            "first sync of an empty source with no marker must still be allowed"
        );
        assert_eq!(
            source_row_count(&store, "usage_event", SourceKind::Antigravity)?,
            1
        );
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(expected_token_accounting_version(SourceKind::Antigravity))
        );
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn cancel_after_legacy_detect_still_preserves_skipped_history() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;
    fixture.seed_claude_streaming_and_sidechain_replay()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            None,
        )
        .await?;
        store.clear_token_accounting_version(SourceKind::Codex)?;
        let before = source_history_snapshot(&store, SourceKind::Codex)?;
        let marker_before = store.token_accounting_version(SourceKind::Codex)?;

        let cancel = CancellationToken::new();
        cancel.cancel();
        let (mut tx, rx) = tokio::sync::mpsc::channel(256);
        commands::sync::run_once_with_cancel(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            Some(&mut tx),
            &cancel,
        )
        .await?;
        drop(tx);
        let events = drain_recv_events(rx).await;

        assert_eq!(source_history_snapshot(&store, SourceKind::Codex)?, before);
        assert_eq!(
            store.token_accounting_version(SourceKind::Codex)?,
            marker_before
        );
        assert!(store.has_legacy_token_accounting(SourceKind::Codex)?);
        assert_no_repair_claim(&events);
        assert!(!events.iter().any(|event| {
            matches!(
                event,
                SyncEvent::SourceStarted {
                    source: SourceKind::Codex,
                    ..
                }
            )
        }));
        let status = loaded_source_status(&store, "codex")?;
        assert!(status.legacy_token_accounting);
        assert_explicit_repair_warning(
            status.token_accounting_warning.as_deref(),
            SourceKind::Codex,
        );
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn cancel_after_current_source_starts_still_skips_legacy() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;
    fixture.seed_claude_streaming_and_sidechain_replay()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            None,
        )
        .await?;
        store.set_meta_value("token_accounting_version.codex", "2")?;
        let before = source_history_snapshot(&store, SourceKind::Codex)?;

        let cancel = CancellationToken::new();
        let watcher_cancel = cancel.clone();
        let (mut tx, mut rx) = tokio::sync::mpsc::channel(256);
        let watcher = tokio::spawn(async move {
            let mut events = Vec::new();
            while let Some(event) = rx.recv().await {
                if matches!(
                    event,
                    SyncEvent::SourceStarted {
                        source: SourceKind::Claude,
                        ..
                    }
                ) {
                    watcher_cancel.cancel();
                }
                events.push(event);
            }
            events
        });

        commands::sync::run_once_with_cancel(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            Some(&mut tx),
            &cancel,
        )
        .await?;
        drop(tx);
        let events = watcher.await?;

        assert_eq!(source_history_snapshot(&store, SourceKind::Codex)?, before);
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, Some(2));
        assert_no_repair_claim(&events);
        assert!(!events.iter().any(|event| {
            matches!(
                event,
                SyncEvent::SourceStarted {
                    source: SourceKind::Codex,
                    ..
                }
            )
        }));
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn serve_repair_keeps_legacy_history_and_does_not_implicit_rebuild() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Codex),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        store.clear_token_accounting_version(SourceKind::Codex)?;
        let before = source_history_snapshot(&store, SourceKind::Codex)?;
        let totals_before = Dashboard::open(&store)?.overview(&Default::default())?;

        let report = commands::serve::repair_legacy_token_accounting(&app, &store).await?;
        assert!(report.rebuilt_sources.is_empty());
        assert_eq!(report.blocked_sources.len(), 1);
        assert_eq!(report.blocked_sources[0].source, SourceKind::Codex);
        assert_eq!(source_history_snapshot(&store, SourceKind::Codex)?, before);
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, None);
        assert!(store.has_legacy_token_accounting(SourceKind::Codex)?);
        let totals_after = Dashboard::open(&store)?.overview(&Default::default())?;
        assert_eq!(
            totals_after.total.total_tokens,
            totals_before.total.total_tokens
        );
        let status = loaded_source_status(&store, "codex")?;
        assert!(status.legacy_token_accounting);
        assert_explicit_repair_warning(
            status.token_accounting_warning.as_deref(),
            SourceKind::Codex,
        );

        let repeated = commands::serve::repair_legacy_token_accounting(&app, &store).await?;
        assert!(repeated.rebuilt_sources.is_empty());
        assert_eq!(repeated.blocked_sources[0].source, SourceKind::Codex);
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn serve_repair_records_every_legacy_source_as_not_rebuilt() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;
    fixture.seed_claude_streaming_and_sidechain_replay()?;
    fixture.seed_opencode_authoritative_total()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            None,
        )
        .await?;
        let legacy = [SourceKind::Codex, SourceKind::Claude, SourceKind::Opencode];
        let mut before = Vec::new();
        for source in legacy {
            store.clear_token_accounting_version(source)?;
            before.push(source_history_snapshot(&store, source)?);
        }
        seed_antigravity_history(&store)?;

        let report = commands::serve::repair_legacy_token_accounting(&app, &store).await?;
        assert!(report.rebuilt_sources.is_empty());
        let blocked = report
            .blocked_sources
            .iter()
            .map(|row| row.source)
            .collect::<Vec<_>>();
        assert_eq!(blocked, legacy.to_vec());
        for (index, source) in legacy.into_iter().enumerate() {
            assert_eq!(source_history_snapshot(&store, source)?, before[index]);
            assert!(store.has_legacy_token_accounting(source)?);
        }
        assert_eq!(
            source_row_count(&store, "usage_event", SourceKind::Antigravity)?,
            1
        );
        assert_eq!(
            store.token_accounting_version(SourceKind::Antigravity)?,
            Some(expected_token_accounting_version(SourceKind::Antigravity))
        );
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn serve_repair_skips_lossy_legacy_source_without_deleting_history() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Codex),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        fixture.remove_codex_inputs()?;
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        store.clear_token_accounting_version(SourceKind::Codex)?;
        let before = source_history_snapshot(&store, SourceKind::Codex)?;
        let event_count = source_row_count(&store, "usage_event", SourceKind::Codex)?;

        let report = commands::serve::repair_legacy_token_accounting(&app, &store).await?;
        assert!(report.rebuilt_sources.is_empty());
        assert_eq!(report.blocked_sources.len(), 1);
        let blocked = &report.blocked_sources[0];
        assert_eq!(blocked.source, SourceKind::Codex);
        assert_eq!(blocked.missing_file_count, 2);
        assert_eq!(blocked.protected_event_count, event_count as u64);
        assert_eq!(source_history_snapshot(&store, SourceKind::Codex)?, before);
        assert!(store.has_legacy_token_accounting(SourceKind::Codex)?);
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, None);
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn serve_repair_succeeds_when_legacy_source_is_unparseable() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_opencode_authoritative_total()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Opencode),
                ..Default::default()
            },
            None,
        )
        .await?;
        store.clear_token_accounting_version(SourceKind::Opencode)?;
        fixture.break_opencode_schema()?;
        let before = source_history_snapshot(&store, SourceKind::Opencode)?;
        let totals_before = Dashboard::open(&store)?.overview(&Default::default())?;

        let report = commands::serve::repair_legacy_token_accounting(&app, &store).await?;
        assert!(report.rebuilt_sources.is_empty());
        assert_eq!(report.blocked_sources.len(), 1);
        assert_eq!(report.blocked_sources[0].source, SourceKind::Opencode);
        assert_eq!(
            source_history_snapshot(&store, SourceKind::Opencode)?,
            before
        );
        assert_eq!(store.token_accounting_version(SourceKind::Opencode)?, None);
        let totals_after = Dashboard::open(&store)?.overview(&Default::default())?;
        assert_eq!(
            totals_after.total.total_tokens,
            totals_before.total.total_tokens
        );
        let status = loaded_source_status(&store, "opencode")?;
        assert!(status.legacy_token_accounting);
        assert_explicit_repair_warning(
            status.token_accounting_warning.as_deref(),
            SourceKind::Opencode,
        );
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn explicit_rebuild_repairs_selected_legacy_source() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Codex),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        store.set_meta_value("token_accounting_version.codex", "2")?;
        assert!(store.has_legacy_token_accounting(SourceKind::Codex)?);

        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, Some(3));
        assert!(!store.has_legacy_token_accounting(SourceKind::Codex)?);
        assert_eq!(
            source_row_count(&store, "usage_event", SourceKind::Codex)?,
            1
        );
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn full_rebuild_refused_while_unattributed_antigravity_history_exists() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            None,
        )
        .await?;
        seed_antigravity_history(&store)?;

        // Hook-era rows carry no file attribution and cannot be reconstructed
        // from conversations/*.db, so any rebuild that would delete them is
        // refused — even with --allow-lossy-rebuild and even for a full
        // no-source rebuild.
        let error = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                rebuild: true,
                allow_lossy_rebuild: true,
                ..Default::default()
            },
            None,
        )
        .await
        .expect_err("full rebuild must refuse while unattributed history exists");
        assert!(
            error
                .to_string()
                .contains("hook-era history without file attribution")
        );

        for table in [
            "usage_event",
            "usage_bucket_30m",
            "usage_turn",
            "usage_tool_call",
            "source_cursor",
            "source_file",
        ] {
            assert_eq!(
                source_row_count(&store, table, SourceKind::Antigravity)?,
                1,
                "refused rebuild must preserve Antigravity rows in {table}"
            );
        }
        let risk = store
            .source_files()
            .lossy_rebuild_risk(SourceKind::Antigravity, "local")?;
        assert_eq!(risk.missing_file_count, 1);
        assert_eq!(risk.protected_event_count, 1);
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn full_rebuild_preserves_parserless_rows_across_all_owned_tables() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            None,
        )
        .await?;
        seed_parserless_history(&store)?;

        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                rebuild: true,
                ..Default::default()
            },
            None,
        )
        .await?;

        let conn = store.open_connection()?;
        assert_eq!(
            conn.query_row(
                "SELECT event_key FROM usage_event WHERE source = 'parserless_fixture'",
                [],
                |row| row.get::<_, String>(0),
            )?,
            "parserless:event"
        );
        assert_eq!(
            conn.query_row(
                "SELECT model, hour_start, project_hash, total_tokens, event_count \
                 FROM usage_bucket_30m WHERE source = 'parserless_fixture'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )?,
            (
                "parserless-model".to_string(),
                "2026-01-02T03:00:00Z".to_string(),
                "parserless-project".to_string(),
                29,
                1,
            )
        );
        assert_eq!(
            conn.query_row(
                "SELECT turn_key FROM usage_turn WHERE source = 'parserless_fixture'",
                [],
                |row| row.get::<_, String>(0),
            )?,
            "parserless:turn"
        );
        assert_eq!(
            conn.query_row(
                "SELECT tool_call_key FROM usage_tool_call WHERE source = 'parserless_fixture'",
                [],
                |row| row.get::<_, String>(0),
            )?,
            "parserless:tool"
        );
        assert_eq!(
            conn.query_row(
                "SELECT cursor_key, file_path FROM source_cursor \
                 WHERE source = 'parserless_fixture'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?,
            (
                "parserless:cursor".to_string(),
                "/virtual/parserless/history.jsonl".to_string(),
            )
        );
        assert_eq!(
            conn.query_row(
                "SELECT file_path, state FROM source_file \
                 WHERE source = 'parserless_fixture'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?,
            (
                "/virtual/parserless/history.jsonl".to_string(),
                "missing".to_string(),
            )
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
                [],
                |row| row.get::<_, i64>(0),
            )?,
            1,
            "successful full rebuild must replay the parser-backed fixture"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn full_rebuild_checks_all_parser_risks_before_resetting_any_source() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;
    fixture.seed_claude_streaming_and_sidechain_replay()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions::default(),
            None,
        )
        .await?;
        fixture.remove_codex_inputs()?;
        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            None,
        )
        .await?;
        let codex_before = source_row_count(&store, "usage_event", SourceKind::Codex)?;
        let claude_before = source_row_count(&store, "usage_event", SourceKind::Claude)?;

        let error = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                rebuild: true,
                ..Default::default()
            },
            None,
        )
        .await
        .expect_err("full rebuild must reject any parser-backed lossy source");
        assert!(error.to_string().contains("Refusing lossy sync --rebuild"));
        assert_eq!(
            source_row_count(&store, "usage_event", SourceKind::Codex)?,
            codex_before
        );
        assert_eq!(
            source_row_count(&store, "usage_event", SourceKind::Claude)?,
            claude_before
        );
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

fn drain_events(rx: &mut tokio::sync::mpsc::Receiver<SyncEvent>) -> Vec<SyncEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

async fn drain_recv_events(mut rx: tokio::sync::mpsc::Receiver<SyncEvent>) -> Vec<SyncEvent> {
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    events
}

fn loaded_source_status(store: &Store, source: &str) -> Result<llmusage::store::SourceSyncStatus> {
    Ok(store
        .sync_status()
        .load_source_sync_statuses("local")?
        .into_iter()
        .find(|status| status.source == source)
        .unwrap_or_else(|| panic!("{source} sync status")))
}

fn assert_explicit_repair_warning(warning: Option<&str>, source: SourceKind) {
    let warning = warning.expect("legacy warning");
    assert!(
        warning.contains(&format!(
            "llmusage sync --rebuild --source {}",
            source.as_str()
        )),
        "{warning}"
    );
    assert!(warning.contains("--allow-lossy-rebuild"), "{warning}");
    assert!(
        !warning.to_ascii_lowercase().contains("automatic"),
        "{warning}"
    );
    assert!(!warning.contains("repaired"), "{warning}");
    assert!(!warning.contains("unbounded"), "{warning}");
}

fn assert_usage_rows_eq(actual: &SourceHistorySnapshot, expected: &SourceHistorySnapshot) {
    assert_eq!(actual.events, expected.events);
    assert_eq!(actual.raw, expected.raw);
    assert_eq!(actual.buckets, expected.buckets);
    assert_eq!(actual.turns, expected.turns);
    assert_eq!(actual.tools, expected.tools);
    assert_eq!(actual.cursors, expected.cursors);
}

fn assert_no_repair_claim(events: &[SyncEvent]) {
    assert!(
        !events.iter().any(|event| matches!(
            event,
            SyncEvent::TokenAccountingRepairStarted { .. }
                | SyncEvent::TokenAccountingRepairFinished { .. }
        )),
        "{events:?}"
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceHistorySnapshot {
    events: Vec<Vec<String>>,
    raw: Vec<Vec<String>>,
    buckets: Vec<Vec<String>>,
    turns: Vec<Vec<String>>,
    tools: Vec<Vec<String>>,
    cursors: Vec<Vec<String>>,
    source_files: Vec<Vec<String>>,
}

fn source_history_snapshot(store: &Store, source: SourceKind) -> Result<SourceHistorySnapshot> {
    let conn = store.open_connection()?;
    let source_id = source.as_str();
    Ok(SourceHistorySnapshot {
        events: dump_query(
            &conn,
            "SELECT * FROM usage_event WHERE source = ?1 ORDER BY event_key, event_at, rowid",
            source_id,
        )?,
        raw: dump_query(
            &conn,
            r#"
            SELECT r.event_key, r.raw_json, r.created_at
            FROM usage_event_raw r
            INNER JOIN usage_event e ON e.event_key = r.event_key
            WHERE e.source = ?1
            ORDER BY r.event_key
            "#,
            source_id,
        )?,
        buckets: dump_query(
            &conn,
            "SELECT * FROM usage_bucket_30m WHERE source = ?1 ORDER BY hour_start, model, project_hash, rowid",
            source_id,
        )?,
        turns: dump_query(
            &conn,
            "SELECT * FROM usage_turn WHERE source = ?1 ORDER BY turn_key, rowid",
            source_id,
        )?,
        tools: dump_query(
            &conn,
            "SELECT * FROM usage_tool_call WHERE source = ?1 ORDER BY tool_call_key, rowid",
            source_id,
        )?,
        cursors: dump_query(
            &conn,
            "SELECT * FROM source_cursor WHERE source = ?1 ORDER BY cursor_key, rowid",
            source_id,
        )?,
        source_files: dump_query(
            &conn,
            "SELECT * FROM source_file WHERE source = ?1 ORDER BY file_path, rowid",
            source_id,
        )?,
    })
}

fn dump_query(conn: &Connection, sql: &str, source: &str) -> Result<Vec<Vec<String>>> {
    let mut stmt = conn.prepare(sql)?;
    let col_count = stmt.column_count();
    let mut rows = stmt.query([source])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut values = Vec::with_capacity(col_count);
        for index in 0..col_count {
            values.push(match row.get_ref(index)? {
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
                rusqlite::types::ValueRef::Integer(value) => value.to_string(),
                rusqlite::types::ValueRef::Real(value) => value.to_string(),
                rusqlite::types::ValueRef::Text(value) => {
                    String::from_utf8_lossy(value).into_owned()
                }
                rusqlite::types::ValueRef::Blob(value) => format!("blob:{}", value.len()),
            });
        }
        out.push(values);
    }
    Ok(out)
}

fn seed_antigravity_history(store: &Store) -> Result<()> {
    let conn = store.open_connection()?;
    let timestamp = "2026-07-15T03:00:00Z";
    conn.execute_batch(&format!(
        r#"
        INSERT INTO usage_event(
            event_key, source, model, event_at, hour_start,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens, created_at
        ) VALUES ('antigravity:test:event', 'antigravity', 'gemini-2.5-pro', '{timestamp}', '{timestamp}',
                  20, 0, 0, 5, 0, 25, '{timestamp}');
        INSERT INTO usage_bucket_30m(
            source, provider_label, model, hour_start, project_hash,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens,
            event_count, updated_at
        ) VALUES ('antigravity', '', 'gemini-2.5-pro', '{timestamp}', '',
                  20, 0, 0, 5, 0, 25, 1, '{timestamp}');
        INSERT INTO usage_turn(
            turn_key, source, primary_model, started_at, category,
            input_tokens, output_tokens, total_tokens, created_at
        ) VALUES ('turn:antigravity:test', 'antigravity', 'gemini-2.5-pro', '{timestamp}',
                  'tooling', 20, 5, 25, '{timestamp}');
        INSERT INTO usage_tool_call(
            tool_call_key, turn_key, event_key, source, occurred_at,
            tool_name, tool_kind, created_at
        ) VALUES ('tool:antigravity:test', 'turn:antigravity:test', 'antigravity:test:event',
                  'antigravity', '{timestamp}', 'read_file', 'builtin', '{timestamp}');
        INSERT INTO source_cursor(source, cursor_key, file_path, updated_at)
        VALUES ('antigravity', 'antigravity:test', '/missing/antigravity-history.jsonl', '{timestamp}');
        INSERT INTO source_file(source, file_path, state, last_state_change_at)
        VALUES ('antigravity', '/missing/antigravity-history.jsonl', 'missing', '{timestamp}');
        "#
    ))?;
    Ok(())
}

fn seed_parserless_history(store: &Store) -> Result<()> {
    let conn = store.open_connection()?;
    let timestamp = "2026-01-02T03:00:00Z";
    conn.execute_batch(&format!(
        r#"
        INSERT INTO usage_event(
            event_key, source, source_path_hash, model, event_at, hour_start,
            project_hash, input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens, created_at
        ) VALUES ('parserless:event', 'parserless_fixture', 'parserless:path',
                  'parserless-model', '{timestamp}', '{timestamp}', 'parserless-project',
                  23, 0, 0, 6, 0, 29, '{timestamp}');
        INSERT INTO usage_bucket_30m(
            source, provider_label, model, hour_start, project_hash,
            input_tokens, cache_read_tokens, cache_creation_tokens,
            output_tokens, reasoning_output_tokens, total_tokens,
            event_count, updated_at
        ) VALUES ('parserless_fixture', '', 'parserless-model', '{timestamp}',
                  'parserless-project', 23, 0, 0, 6, 0, 29, 1, '{timestamp}');
        INSERT INTO usage_turn(
            turn_key, source, project_hash, primary_model, started_at, category,
            input_tokens, output_tokens, total_tokens, created_at
        ) VALUES ('parserless:turn', 'parserless_fixture', 'parserless-project',
                  'parserless-model', '{timestamp}', 'tooling', 23, 6, 29, '{timestamp}');
        INSERT INTO usage_tool_call(
            tool_call_key, turn_key, event_key, source, project_hash, occurred_at,
            tool_name, tool_kind, created_at
        ) VALUES ('parserless:tool', 'parserless:turn', 'parserless:event',
                  'parserless_fixture', 'parserless-project', '{timestamp}',
                  'read_file', 'builtin', '{timestamp}');
        INSERT INTO source_cursor(source, cursor_key, file_path, updated_at)
        VALUES ('parserless_fixture', 'parserless:cursor',
                '/virtual/parserless/history.jsonl', '{timestamp}');
        INSERT INTO source_file(source, file_path, state, last_state_change_at)
        VALUES ('parserless_fixture', '/virtual/parserless/history.jsonl',
                'missing', '{timestamp}');
        "#
    ))?;
    Ok(())
}

fn source_row_count(store: &Store, table: &str, source: SourceKind) -> Result<i64> {
    let conn = store.open_connection()?;
    Ok(conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE source = ?1"),
        [source.as_str()],
        |row| row.get(0),
    )?)
}

struct Fixture {
    _root: TempDir,
    home: PathBuf,
    codex_home: PathBuf,
    opencode_home: PathBuf,
    _env: crate::test_env::ScopedEnv,
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = TempDir::new()?;
        let home = root.path().join("home");
        let codex_home = home.join(".codex");
        let opencode_home = root.path().join("opencode-home");
        fs::create_dir_all(home.join(".claude/projects/demo"))?;
        fs::create_dir_all(&codex_home)?;
        fs::create_dir_all(&opencode_home)?;

        let env = crate::test_env::ScopedEnv::capture(&[
            "HOME",
            "USERPROFILE",
            "CODEX_HOME",
            "OPENCODE_HOME",
        ]);
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("USERPROFILE", &home);
            std::env::set_var("CODEX_HOME", &codex_home);
            std::env::set_var("OPENCODE_HOME", &opencode_home);
        }

        Ok(Self {
            _root: root,
            home,
            codex_home,
            opencode_home,
            _env: env,
        })
    }

    fn seed_codex_copied_event(&self) -> Result<()> {
        let dir = self.codex_home.join("sessions/2026/07/15");
        fs::create_dir_all(&dir)?;
        let usage = serde_json::json!({
            "input_tokens": 100,
            "cached_input_tokens": 40,
            "output_tokens": 30,
            "reasoning_output_tokens": 10,
            "total_tokens": 130
        });
        let contents = [
            serde_json::json!({
                "type": "session_meta",
                "payload": {"id": "session-a", "model": "gpt-5"}
            })
            .to_string(),
            serde_json::json!({
                "timestamp": "2026-07-15T01:00:00Z",
                "payload": {
                    "type": "token_count",
                    "info": {"last_token_usage": usage, "total_token_usage": usage}
                }
            })
            .to_string(),
        ]
        .join("\n");
        fs::write(dir.join("rollout-a.jsonl"), &contents)?;
        fs::write(dir.join("rollout-copy.jsonl"), contents)?;
        Ok(())
    }

    fn seed_claude_streaming_and_sidechain_replay(&self) -> Result<()> {
        let dir = self.home.join(".claude/projects/demo");
        let partial = claude_line("req-a", false, 10, 2, 4);
        let complete = claude_line("req-a", false, 20, 5, 10);
        fs::write(
            dir.join("session.jsonl"),
            format!("{partial}\n{complete}\n"),
        )?;
        fs::write(
            dir.join("sidechain.jsonl"),
            format!("{}\n", claude_line("req-side", true, 20, 5, 10)),
        )?;
        Ok(())
    }

    fn seed_opencode_authoritative_total(&self) -> Result<()> {
        let conn = Connection::open(self.opencode_home.join("opencode.db"))?;
        conn.execute_batch(
            r#"
            CREATE TABLE project(id TEXT PRIMARY KEY, worktree TEXT);
            CREATE TABLE session(id TEXT PRIMARY KEY, project_id TEXT);
            CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
            "#,
        )?;
        conn.execute(
            "INSERT INTO session(id, project_id) VALUES ('session-1', NULL)",
            [],
        )?;
        let message = serde_json::json!({
            "id": "msg-open",
            "role": "assistant",
            "modelID": "gpt-5",
            "tokens": {
                "input": 100,
                "output": 30,
                "reasoning": 7,
                "total": 250,
                "cache": {"read": 20, "write": 40}
            },
            "time": {"created": 1784077200000i64, "completed": 1784077200000i64}
        });
        conn.execute(
            "INSERT INTO message(id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (
                "msg-open",
                "session-1",
                1784077200000i64,
                message.to_string(),
            ),
        )?;
        Ok(())
    }

    fn remove_codex_inputs(&self) -> Result<()> {
        let dir = self.codex_home.join("sessions/2026/07/15");
        fs::remove_file(dir.join("rollout-a.jsonl"))?;
        fs::remove_file(dir.join("rollout-copy.jsonl"))?;
        Ok(())
    }

    fn break_opencode_schema(&self) -> Result<()> {
        let conn = Connection::open(self.opencode_home.join("opencode.db"))?;
        conn.execute("DROP TABLE message", [])?;
        Ok(())
    }
}

fn claude_line(
    request_id: &str,
    is_sidechain: bool,
    input: i64,
    cache_read: i64,
    output: i64,
) -> String {
    serde_json::json!({
        "timestamp": "2026-07-15T02:00:00Z",
        "sessionId": "session-claude",
        "requestId": request_id,
        "isSidechain": is_sidechain,
        "message": {
            "id": "msg-claude",
            "model": "claude-sonnet-4",
            "usage": {
                "input_tokens": input,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": cache_read,
                "output_tokens": output
            }
        }
    })
    .to_string()
}
