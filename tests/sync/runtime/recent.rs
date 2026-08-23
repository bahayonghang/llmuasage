use super::*;

#[tokio::test]
async fn recent_ready_emitted_per_source_when_recent_days_set() -> Result<()> {
    let _env = SourceEnvFixture::new()?;
    let (_tmp, store) = make_store()?;
    let app = llmusage::app::AppContext {
        paths: store.paths.clone(),
        current_exe: std::env::current_exe()?,
    };
    let (mut tx, mut rx) = tokio::sync::mpsc::channel(32);

    let summary = llmusage::commands::sync::run_once_with_options(
        &app,
        &store,
        0,
        &llmusage::commands::sync::SyncRunOptions {
            source: Some(SourceKind::Codex),
            recent_days: Some(30),
            ..Default::default()
        },
        Some(&mut tx),
    )
    .await?;
    drop(tx);

    assert_eq!(summary.sources.len(), 1);
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    assert!(events.iter().any(|event| {
        matches!(
            event,
            SyncEvent::RecentReady {
                source: SourceKind::Codex
            }
        )
    }));
    let source_finished = events
        .iter()
        .position(|event| matches!(event, SyncEvent::SourceFinished { .. }))
        .expect("source finished event");
    let recent_ready = events
        .iter()
        .position(|event| matches!(event, SyncEvent::RecentReady { .. }))
        .expect("recent ready event");
    assert!(
        recent_ready > source_finished,
        "RecentReady must follow completion of the requested bounded stage"
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            SyncEvent::SourceFinished {
                source: SourceKind::Codex,
                ..
            }
        )
    }));

    let diagnostics = Dashboard::open(&store)?.diagnostics()?;
    let codex = diagnostics
        .by_source
        .iter()
        .find(|row| row.source == "codex")
        .expect("codex diagnostics row");
    assert!(codex.recent_completed_at.is_some());
    Ok(())
}

#[tokio::test]
async fn recent_window_filters_old_events_without_advancing_full_history_cursor() -> Result<()> {
    let _env = SourceEnvFixture::new()?;
    let codex_home = PathBuf::from(std::env::var("CODEX_HOME")?);
    let old_directory = codex_home.join("sessions/2020/01/01");
    fs::create_dir_all(&old_directory)?;
    let rollout = old_directory.join("rollout-old-file.jsonl");
    let old_at = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
    let recent_at = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    let token_line = |timestamp: &str, total: i64| {
        serde_json::json!({
            "timestamp": timestamp,
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "last_token_usage": {
                        "input_tokens": 10,
                        "output_tokens": 0,
                        "total_tokens": 10
                    },
                    "total_token_usage": {
                        "input_tokens": total,
                        "output_tokens": 0,
                        "total_tokens": total
                    }
                }
            }
        })
        .to_string()
    };
    fs::write(&rollout, format!("{}\n", token_line(&old_at, 10)))?;

    let (_tmp, store) = make_store()?;
    let app = llmusage::app::AppContext {
        paths: store.paths.clone(),
        current_exe: std::env::current_exe()?,
    };
    let bounded = llmusage::commands::sync::SyncRunOptions {
        source: Some(SourceKind::Codex),
        recent_days: Some(30),
        ..Default::default()
    };
    llmusage::commands::sync::run_once_with_options(&app, &store, 0, &bounded, None).await?;
    assert_eq!(
        count_rows(&store, "usage_event", "")?,
        0,
        "old event must be outside the window"
    );
    assert!(
        store
            .cursors()
            .load_file_cursors(SourceKind::Codex, "local")?
            .is_empty(),
        "bounded scan must not advance the sole full-history cursor"
    );

    fs::OpenOptions::new()
        .append(true)
        .open(&rollout)?
        .write_all(format!("{}\n", token_line(&recent_at, 20)).as_bytes())?;
    llmusage::commands::sync::run_once_with_options(&app, &store, 0, &bounded, None).await?;
    assert_eq!(
        count_rows(&store, "usage_event", "")?,
        1,
        "recent append must be imported"
    );
    assert!(
        store
            .cursors()
            .load_file_cursors(SourceKind::Codex, "local")?
            .is_empty()
    );

    let full = llmusage::commands::sync::SyncRunOptions {
        source: Some(SourceKind::Codex),
        ..Default::default()
    };
    llmusage::commands::sync::run_once_with_options(&app, &store, 0, &full, None).await?;
    assert_eq!(
        count_rows(&store, "usage_event", "")?,
        2,
        "later full sync must recover the older event"
    );
    assert_eq!(
        store
            .cursors()
            .load_file_cursors(SourceKind::Codex, "local")?
            .len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn source_filtered_sync_keeps_other_sources_intact() -> Result<()> {
    let _env = SourceEnvFixture::new()?;
    let (_tmp, store) = make_store()?;
    seed_source_file(&store, SourceKind::Codex, "/codex/stale.jsonl")?;
    seed_source_file(&store, SourceKind::Claude, "/claude/keep.jsonl")?;
    assert_eq!(
        store
            .source_files()
            .counts(SourceKind::Codex, "local")?
            .live,
        1
    );
    assert_eq!(
        store
            .source_files()
            .counts(SourceKind::Claude, "local")?
            .live,
        1
    );

    let app = llmusage::app::AppContext {
        paths: store.paths.clone(),
        current_exe: std::env::current_exe()?,
    };
    let summary = llmusage::commands::sync::run_once_with_options(
        &app,
        &store,
        0,
        &llmusage::commands::sync::SyncRunOptions {
            source: Some(SourceKind::Codex),
            ..Default::default()
        },
        None,
    )
    .await?;
    assert_eq!(summary.sources.len(), 1);

    let codex = store.source_files().counts(SourceKind::Codex, "local")?;
    let claude = store.source_files().counts(SourceKind::Claude, "local")?;
    assert_eq!(codex.missing, 1);
    assert_eq!(claude.live, 1);
    assert_eq!(claude.missing, 0);
    Ok(())
}
