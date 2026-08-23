use super::*;

#[tokio::test]
async fn start_run_complete_lifecycle_observable_via_snapshot() -> Result<()> {
    let _env = SourceEnvFixture::new()?;
    let (_tmp, store) = make_store()?;
    let registry = JobRegistry::new(Arc::new(llmusage::commands::sync::CommandSyncExecutor));
    let (job_id, mut rx) = registry.start(
        &store,
        SyncOptions {
            source: Some("codex".to_string()),
            ..Default::default()
        },
    );

    let mut saw_finished = false;
    while let Some(event) = rx.recv().await {
        if matches!(event, SyncEvent::Finished { .. }) {
            saw_finished = true;
            break;
        }
    }
    assert!(saw_finished, "job should forward Finished event");

    let snapshot = registry.snapshot(&job_id).expect("job snapshot");
    assert_eq!(snapshot.status, JobStatus::Completed);
    assert!(snapshot.summary.is_some());
    assert!(snapshot.finished_at.is_some());
    assert!(matches!(
        snapshot.last_event,
        Some(SyncEvent::Finished { .. }) | Some(SyncEvent::SourceFinished { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn cancel_within_1500ms() -> Result<()> {
    let (_tmp, store) = make_store()?;
    let blocker = store
        .acquire_worker_lock_with(Duration::from_secs(0), llmusage::store::HolderKind::Library)?;
    let registry = JobRegistry::new(Arc::new(llmusage::commands::sync::CommandSyncExecutor));
    let (job_id, mut rx) = registry.start(
        &store,
        SyncOptions {
            source: Some("antigravity".to_string()),
            ..Default::default()
        },
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(event) = rx.recv().await {
            if matches!(event, SyncEvent::LockWaiting { .. }) {
                return;
            }
        }
    })
    .await?;

    let started = std::time::Instant::now();
    assert!(registry.cancel(&job_id));
    let snapshot = registry.snapshot(&job_id).expect("job snapshot");
    assert_eq!(snapshot.status, JobStatus::Cancelling);
    assert!(snapshot.finished_at.is_none());
    drop(blocker);
    loop {
        let snapshot = registry.snapshot(&job_id).expect("job snapshot");
        if snapshot.status == JobStatus::Cancelled {
            break;
        }
        if started.elapsed() >= std::time::Duration::from_millis(1500) {
            anyhow::bail!("job did not reach cancelled state within 1500ms");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(started.elapsed() < std::time::Duration::from_millis(1500));
    Ok(())
}

#[tokio::test]
async fn file_boundary_cancel_preserves_written_events() -> Result<()> {
    let (_tmp, store) = make_store()?;
    let cancel = CancellationToken::new();
    let parser = CancelAfterFilesParser {
        total_files: 10,
        cancel_after_files: 3,
        per_file_delay: Duration::from_millis(0),
    };
    let parsers: Vec<Box<dyn SourceParser>> = vec![Box::new(parser)];
    let (mut tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut writer = store.begin_sync_run()?;

    let stats = driver::drive_with_events(driver::DriveContext {
        parsers: &parsers,
        store: &store,
        writer: &mut writer,
        parallelism: 1,
        lock_wait_ms: 0,
        recent_cutoff: None,
        sender: Some(&mut tx),
        cancel: &cancel,
        sweep_host_ids: vec!["local".to_string()],
    })
    .await?;
    writer.finish_sync_run()?;
    drop(tx);

    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].files_processed, 3);
    assert_eq!(stats[0].events_inserted, 3);
    assert!(cancel.is_cancelled());

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SyncEvent::SourceFinished { .. })),
        "driver should finish the source with partial stats"
    );

    let conn = store.open_connection()?;
    let event_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
        [],
        |row| row.get(0),
    )?;
    let cursor_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM source_cursor WHERE source = 'codex'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(event_count, 3);
    assert_eq!(cursor_count, 3);
    assert!(
        store
            .source_files()
            .counts(SourceKind::Codex, "local")?
            .live
            >= 3
    );

    let imported_keys = (0..10)
        .filter_map(|index| {
            conn.query_row(
                "SELECT event_key FROM usage_event WHERE event_key = ?1",
                [format!("local:codex:cancel-file-{index}")],
                |row| row.get::<_, String>(0),
            )
            .ok()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        imported_keys,
        vec![
            "local:codex:cancel-file-0".to_string(),
            "local:codex:cancel-file-1".to_string(),
            "local:codex:cancel-file-2".to_string()
        ]
    );
    Ok(())
}

#[tokio::test]
async fn cancel_within_1500ms_with_5_pending_files() -> Result<()> {
    let (_tmp, store) = make_store()?;
    let cancel = CancellationToken::new();
    let parser = CancelAfterFilesParser {
        total_files: 10,
        cancel_after_files: 5,
        per_file_delay: Duration::from_millis(20),
    };
    let started = Instant::now();
    let mut writer = store.begin_sync_run()?;
    let stats = parser
        .parse(&store, &mut writer, 1, None, &cancel, None)
        .await?;
    writer.finish_sync_run()?;

    assert!(started.elapsed() < Duration::from_millis(1500));
    assert!(cancel.is_cancelled());
    assert_eq!(stats.files_processed, 5);
    assert_eq!(stats.events_inserted, 5);

    let conn = store.open_connection()?;
    let event_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(event_count, 5);
    Ok(())
}
