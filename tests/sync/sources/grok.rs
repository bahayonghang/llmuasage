use super::super::*;

#[test]
fn grok_session_replay_converges_and_protects_missing_sidecars() -> Result<()> {
    let fixture = Fixture::new()?;
    let updates = concat!(
        "{\"params\":{\"update\":{\"sessionUpdate\":\"available_commands_update\"},\"_meta\":{\"totalTokens\":100,\"agentTimestampMs\":1700000000000}}}\n",
        "{\"params\":{\"update\":{\"sessionUpdate\":\"user_message_chunk\",\"_meta\":{\"modelId\":\"grok-4.5\"}},\"_meta\":{\"agentTimestampMs\":1700000001000}}}\n",
        "{\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\"},\"_meta\":{\"totalTokens\":300,\"agentTimestampMs\":1700000003000}}}\n"
    );
    let session_dir = fixture.seed_grok(
        "session-replay",
        updates,
        Some("{\"current_model_id\":\"grok-4.5\",\"updated_at\":\"2023-11-14T22:13:20Z\"}"),
        Some("{\"primaryModelId\":\"grok-4.5\",\"contextTokensUsed\":500}"),
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Grok),
            ..Default::default()
        };

        let first = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(first.sources[0].source, SourceKind::Grok);
        assert_eq!(first.sources[0].changed_files, 3);
        assert_eq!(first.sources[0].events_replayed, 0);
        assert_eq!(first.sources[0].events_inserted, 2);
        assert_grok_totals(&app.paths.db_path, 2, 500)?;
        assert!(grok_event_rows(&app.paths.db_path)?.iter().all(|row| {
            row.model == "grok-4.5"
                && row.input_tokens == 0
                && row.cache_read_tokens == 0
                && row.cache_creation_tokens == 0
                && row.output_tokens == 0
                && row.reasoning_tokens == 0
                && row.pricing_status == "unpriced"
                && row.provider_label.is_empty()
                && row.project_label.as_deref() == Some("demo")
        }));
        assert_eq!(
            llmusage::registry::source_descriptor(SourceKind::Grok)
                .expect("grok descriptor")
                .quality,
            llmusage::domain::source_descriptor::UsageQuality::Precise
        );
        assert_eq!(expected_token_accounting_version(SourceKind::Grok), 3);
        assert_eq!(expected_token_accounting_version(SourceKind::Codex), 3);
        assert_eq!(
            source_capability_status(&app, &store, SourceKind::Grok)?,
            "passive_ready"
        );
        let grok_monitor = llmusage::registry::registered_platform_monitors()
            .iter()
            .find(|monitor| monitor.platform_id == "grok")
            .expect("grok monitor");
        assert_eq!(
            grok_monitor.quality,
            Some(llmusage::domain::source_descriptor::UsageQuality::Precise)
        );
        let probe = llmusage::domain::platform_monitor::probe_platform_descriptor(
            grok_monitor,
            &fixture.home,
        );
        assert_eq!(
            probe.parser_status,
            llmusage::domain::platform_monitor::ParserSupportStatus::Registered
        );
        assert_eq!(
            probe.status,
            llmusage::domain::platform_monitor::PlatformProbeStatus::Detected
        );

        let second = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(second.sources[0].changed_files, 0);
        assert_eq!(second.sources[0].events_inserted, 0);
        assert_eq!(second.sources[0].bytes_scanned, 0);
        assert_grok_totals(&app.paths.db_path, 2, 500)?;

        fixture.append_grok_updates(
            "session-replay",
            concat!(
                "{\"params\":{\"update\":{\"sessionUpdate\":\"user_message_chunk\"},\"_meta\":{\"agentTimestampMs\":1700000004000}}}\n",
                "{\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\"},\"_meta\":{\"totalTokens\":500,\"agentTimestampMs\":1700000005000}}}\n"
            ),
        )?;
        let appended =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(appended.sources[0].changed_files, 3);
        assert_eq!(appended.sources[0].events_replayed, 3);
        assert_grok_totals(&app.paths.db_path, 3, 500)?;

        fixture.write_grok_sidecar(
            "session-replay",
            "signals.json",
            "{\"primaryModelId\":\"grok-4.5\",\"contextTokensUsed\":700,\"revision\":2}",
        )?;
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_grok_totals(&app.paths.db_path, 3, 700)?;

        // Later updates can cover the complete signals total. Session replay
        // removes the old reconciliation row instead of double counting it.
        fixture.write_grok_sidecar(
            "session-replay",
            "updates.jsonl",
            concat!(
                "{\"params\":{\"update\":{\"sessionUpdate\":\"user_message_chunk\",\"_meta\":{\"modelId\":\"grok-4.5\"}},\"_meta\":{\"agentTimestampMs\":1700000010000}}}\n",
                "{\"params\":{\"update\":{\"sessionUpdate\":\"agent_message_chunk\"},\"_meta\":{\"totalTokens\":700,\"agentTimestampMs\":1700000011000}}}\n"
            ),
        )?;
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_grok_totals(&app.paths.db_path, 1, 700)?;
        assert_eq!(
            store.token_accounting_version(SourceKind::Grok)?,
            Some(expected_token_accounting_version(SourceKind::Grok))
        );

        let signals_path = session_dir.join("signals.json");
        fs::remove_file(&signals_path)?;
        let missing =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(missing.sources[0].changed_files, 0);
        assert_grok_totals(&app.paths.db_path, 1, 700)?;
        assert_eq!(store.source_files().counts(SourceKind::Grok, "local")?.missing, 1);

        let blocked = commands::sync::run_with_options(
            &app,
            commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::Grok),
                ..Default::default()
            },
        )
        .await;
        assert!(
            blocked
                .expect_err("missing Grok sidecar must block rebuild")
                .to_string()
                .contains("Refusing lossy sync --rebuild")
        );

        fixture.write_grok_sidecar(
            "session-replay",
            "signals.json",
            "{\"primaryModelId\":\"grok-4.5\",\"contextTokensUsed\":800,\"revision\":3}",
        )?;
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_grok_totals(&app.paths.db_path, 2, 800)?;
        assert_eq!(store.source_files().counts(SourceKind::Grok, "local")?.missing, 0);
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn grok_missing_root_reports_passive_no_data() -> Result<()> {
    let fixture = Fixture::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let result = commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                source: Some(SourceKind::Grok),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_eq!(result.sources[0].files_processed, 0);
        assert_eq!(result.sources[0].events_inserted, 0);
        assert_eq!(
            source_capability_status(&app, &store, SourceKind::Grok)?,
            "passive_no_data"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    fixture.restore_env();
    Ok(())
}

#[test]
fn grok_home_override_is_honored() -> Result<()> {
    let fixture = Fixture::new()?;
    let custom_root = fixture.home.join("custom-grok");
    fixture.seed_grok_under(
        &custom_root,
        "session-override",
        "{\"timestamp\":1700000000,\"totalTokens\":123}\n",
        Some("{\"current_model_id\":\"grok-future\",\"updated_at\":\"2023-11-14T22:13:20Z\"}"),
        None,
    )?;
    unsafe {
        std::env::set_var("GROK_HOME", &custom_root);
    }

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
                source: Some(SourceKind::Grok),
                ..Default::default()
            },
            None,
        )
        .await?;

        assert_grok_totals(&app.paths.db_path, 1, 123)?;
        assert_eq!(grok_event_rows(&app.paths.db_path)?[0].model, "grok-future");
        Ok::<_, anyhow::Error>(())
    })?;
    fixture.restore_env();
    Ok(())
}

#[test]
fn grok_recent_window_preserves_full_history_state_and_later_recovers_old_event() -> Result<()> {
    let fixture = Fixture::new()?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let old_ms = now_ms - chrono::Duration::days(90).num_milliseconds();
    let recent_ms = now_ms - chrono::Duration::days(1).num_milliseconds();
    let old_usage = r#"{"inputTokens":11,"cachedReadTokens":0,"cacheCreationTokens":0,"outputTokens":0,"reasoningTokens":0,"totalTokens":11,"modelUsage":{"grok-recent-test":{}}}"#;
    let recent_usage = r#"{"inputTokens":22,"cachedReadTokens":0,"cacheCreationTokens":0,"outputTokens":0,"reasoningTokens":0,"totalTokens":22,"modelUsage":{"grok-recent-test":{}}}"#;
    fixture.seed_grok(
        "session-recent-window",
        &format!(
            "{}{}",
            grok_turn_usage_line("prompt-old", old_ms, old_usage),
            grok_turn_usage_line("prompt-recent", recent_ms, recent_usage),
        ),
        Some("{\"current_model_id\":\"grok-recent-test\",\"updated_at\":\"2026-01-01T00:00:00Z\"}"),
        None,
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let bounded = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Grok),
            recent_days: Some(30),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &bounded, None).await?;
        assert_eq!(
            source_token_totals(&app.paths.db_path, SourceKind::Grok)?,
            vec![22]
        );
        assert!(
            store
                .cursors()
                .load_file_cursors(SourceKind::Grok, "local")?
                .is_empty(),
            "bounded Grok sync must not advance the full-history sidecar state"
        );

        let full = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Grok),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &full, None).await?;
        assert_eq!(
            source_token_totals(&app.paths.db_path, SourceKind::Grok)?,
            vec![11, 22]
        );
        assert!(
            !store
                .cursors()
                .load_file_cursors(SourceKind::Grok, "local")?
                .is_empty()
        );

        commands::sync::run_once_with_options(&app, &store, 0, &full, None).await?;
        assert_eq!(
            source_token_totals(&app.paths.db_path, SourceKind::Grok)?,
            vec![11, 22]
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn grok_turn_usage_is_precise_idempotent_and_replays() -> Result<()> {
    let fixture = Fixture::new()?;
    let usage_a = r#"{"inputTokens":1000,"cachedReadTokens":400,"cacheCreationTokens":0,"outputTokens":50,"reasoningTokens":20,"totalTokens":1050,"modelUsage":{"grok-4.6-build":{}}}"#;
    let usage_b = r#"{"inputTokens":20,"cachedReadTokens":0,"cacheCreationTokens":0,"outputTokens":5,"reasoningTokens":0,"totalTokens":100,"modelUsage":{"grok-4.6-build":{}}}"#;
    fixture.seed_grok(
        "session-usage",
        &format!(
            "{}{}",
            grok_turn_usage_line("p1", 1_700_000_001_000, usage_a),
            grok_turn_usage_line("p2", 1_700_000_002_000, usage_b),
        ),
        Some("{\"current_model_id\":\"custom\",\"updated_at\":\"2023-11-14T22:13:20Z\"}"),
        Some("{\"primaryModelId\":\"grok-4.6\",\"contextTokensUsed\":50000}"),
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Grok),
            ..Default::default()
        };

        let first = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(first.sources[0].events_inserted, 2);
        assert_grok_totals(&app.paths.db_path, 2, 1150)?;
        let rows = grok_event_rows(&app.paths.db_path)?;
        assert_eq!(rows[0].event_key, "local:grok:session-usage:usage:p1");
        assert_eq!(rows[0].model, "grok-4.6-build");
        assert_eq!(rows[0].input_tokens, 600);
        assert_eq!(rows[0].cache_read_tokens, 400);
        assert_eq!(rows[0].cache_creation_tokens, 0);
        assert_eq!(rows[0].output_tokens, 50);
        assert_eq!(rows[0].reasoning_tokens, 20);
        assert_eq!(rows[0].total_tokens, 1050);
        assert_eq!(rows[0].pricing_status, "unpriced");
        assert!(rows[0].provider_label.is_empty());
        assert_eq!(rows[1].event_key, "local:grok:session-usage:usage:p2");
        assert_eq!(rows[1].total_tokens, 100);
        assert!(rows.iter().all(|row| !row.event_key.ends_with(":signals")));
        assert_eq!(
            store.token_accounting_version(SourceKind::Grok)?,
            Some(3)
        );

        let second = commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(second.sources[0].changed_files, 0);
        assert_eq!(second.sources[0].events_inserted, 0);
        assert_grok_totals(&app.paths.db_path, 2, 1150)?;

        let usage_c = r#"{"inputTokens":30,"cachedReadTokens":0,"cacheCreationTokens":0,"outputTokens":10,"reasoningTokens":0,"totalTokens":200,"modelUsage":{"grok-4.6-build":{}}}"#;
        fixture.append_grok_updates(
            "session-usage",
            &grok_turn_usage_line("p3", 1_700_000_003_000, usage_c),
        )?;
        let appended =
            commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert!(appended.sources[0].events_replayed >= 3);
        assert_grok_totals(&app.paths.db_path, 3, 1350)?;
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}

#[test]
fn grok_legacy_marker_2_replays_on_unbounded_sync() -> Result<()> {
    let fixture = Fixture::new()?;
    let usage = r#"{"inputTokens":1000,"cachedReadTokens":400,"cacheCreationTokens":0,"outputTokens":50,"reasoningTokens":20,"totalTokens":1050,"modelUsage":{"grok-4.6-build":{}}}"#;
    fixture.seed_grok(
        "session-legacy",
        &grok_turn_usage_line("p1", 1_700_000_001_000, usage),
        Some("{\"current_model_id\":\"grok-4.6\",\"updated_at\":\"2023-11-14T22:13:20Z\"}"),
        None,
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Grok),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
        assert_eq!(store.token_accounting_version(SourceKind::Grok)?, Some(3));
        store.set_meta_value("token_accounting_version.grok", "2")?;
        assert!(store.has_legacy_token_accounting(SourceKind::Grok)?);

        let (mut tx, mut rx) = tokio::sync::mpsc::channel(256);
        commands::sync::run_once_with_options(&app, &store, 0, &options, Some(&mut tx)).await?;
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        assert_eq!(store.token_accounting_version(SourceKind::Grok)?, Some(3));
        assert!(!store.has_legacy_token_accounting(SourceKind::Grok)?);
        assert_grok_totals(&app.paths.db_path, 1, 1050)?;
        assert!(
            events.iter().any(|event| {
                matches!(
                    event,
                    SyncEvent::TokenAccountingRepairStarted { sources }
                        if sources.as_slice() == [SourceKind::Grok]
                )
            }),
            "legacy grok marker 2 must start token-accounting repair"
        );
        Ok::<_, anyhow::Error>(())
    })?;

    fixture.restore_env();
    Ok(())
}
