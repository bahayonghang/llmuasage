use std::time::Duration;

use anyhow::Result;

use crate::{
    models::SourceKind,
    parsers::SyncEvent,
    store::Store,
    sync::{JobRegistry, JobStatus, SyncOptions},
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum SyncUpdate {
    Progress(String),
    Completed { inserted: usize, stored: usize },
    Failed(String),
    Cancelled,
}

pub(super) struct SyncController {
    runtime: tokio::runtime::Handle,
    registry: JobRegistry,
    active_job_id: Option<String>,
    events: Option<tokio::sync::mpsc::Receiver<SyncEvent>>,
}

impl SyncController {
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            runtime: tokio::runtime::Handle::try_current()
                .map_err(|err| anyhow::anyhow!("TUI requires a Tokio runtime: {err}"))?,
            registry: JobRegistry::default(),
            active_job_id: None,
            events: None,
        })
    }

    pub(super) fn start_or_cancel(&mut self, store: &Store, source: Option<SourceKind>) -> String {
        if let Some(job_id) = self.active_job_id.as_deref() {
            return if self.registry.cancel(job_id) {
                "Sync cancellation requested".to_string()
            } else {
                "Sync job is no longer available".to_string()
            };
        }

        let _runtime_guard = self.runtime.enter();
        match self.registry.try_start(
            store,
            SyncOptions {
                source: source.map(|value| value.as_str().to_string()),
                ..SyncOptions::default()
            },
        ) {
            Ok((job_id, events)) => {
                self.active_job_id = Some(job_id);
                self.events = Some(events);
                "Sync running... press x to cancel".to_string()
            }
            Err(err) => format!("Sync already running: {}", err.active_job_id),
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.active_job_id.is_some()
    }

    pub(super) fn drain_updates(&mut self) -> Vec<SyncUpdate> {
        let mut updates = Vec::new();
        let mut disconnected = false;
        if let Some(events) = self.events.as_mut() {
            loop {
                match events.try_recv() {
                    Ok(event) => updates.push(sync_update(event)),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        if updates.iter().any(is_terminal_sync_update) {
            self.clear_active();
        } else if disconnected {
            if let Some(update) = self.snapshot_terminal_update() {
                updates.push(update);
            }
            self.clear_active();
        }
        updates
    }

    pub(super) fn shutdown(&mut self, timeout: Duration) {
        let Some(job_id) = self.active_job_id.clone() else {
            return;
        };
        self.registry.cancel(&job_id);
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            let Some(snapshot) = self.registry.snapshot(&job_id) else {
                break;
            };
            if !matches!(snapshot.status, JobStatus::Running | JobStatus::Cancelling) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.clear_active();
    }

    fn snapshot_terminal_update(&self) -> Option<SyncUpdate> {
        let snapshot = self
            .active_job_id
            .as_deref()
            .and_then(|job_id| self.registry.snapshot(job_id))?;
        match snapshot.status {
            JobStatus::Failed => Some(SyncUpdate::Failed(
                snapshot
                    .error
                    .unwrap_or_else(|| "sync job failed".to_string()),
            )),
            JobStatus::Cancelled => Some(SyncUpdate::Cancelled),
            _ => None,
        }
    }

    fn clear_active(&mut self) {
        self.active_job_id = None;
        self.events = None;
    }
}

fn sync_update(event: SyncEvent) -> SyncUpdate {
    match event {
        SyncEvent::Finished { summary } => SyncUpdate::Completed {
            inserted: summary.total_inserted,
            stored: summary.stored_events,
        },
        SyncEvent::Failed { error } => SyncUpdate::Failed(error),
        SyncEvent::Cancelled => SyncUpdate::Cancelled,
        event => SyncUpdate::Progress(sync_progress_message(&event)),
    }
}

fn is_terminal_sync_update(update: &SyncUpdate) -> bool {
    matches!(
        update,
        SyncUpdate::Completed { .. } | SyncUpdate::Failed(_) | SyncUpdate::Cancelled
    )
}

fn sync_progress_message(event: &SyncEvent) -> String {
    match event {
        SyncEvent::Started { .. } => "Sync started".to_string(),
        SyncEvent::BootstrapStarted => "Preparing local database".to_string(),
        SyncEvent::MigrationStarted {
            version,
            latest_version,
            ..
        } => format!("Migrating database v{version}/{latest_version}"),
        SyncEvent::MigrationFinished {
            version,
            elapsed_ms,
            ..
        } => format!("Database migration v{version} complete ({elapsed_ms}ms)"),
        SyncEvent::PricingUpgradeStarted { total_events, .. } => {
            format!("Updating prices for {total_events} events")
        }
        SyncEvent::PricingUpgradeProgress {
            processed_events,
            total_events,
            ..
        } => format!("Updating prices {processed_events}/{total_events}"),
        SyncEvent::PricingBucketReconcileStarted { bucket_count, .. } => {
            format!("Reconciling {bucket_count} pricing buckets")
        }
        SyncEvent::PricingUpgradeFinished {
            updated_events,
            bucket_count,
            ..
        } => format!("Prices updated: {updated_events} events, {bucket_count} buckets"),
        SyncEvent::LockWaiting { .. } => "Waiting for sync worker lock".to_string(),
        SyncEvent::LockAcquired { wait_ms } => {
            format!("Sync worker lock acquired after {wait_ms}ms")
        }
        SyncEvent::SourceStarted {
            source,
            files_total,
        } => format!("{}: scanning {files_total} files", source.as_str()),
        SyncEvent::Progress {
            source,
            files_scanned,
            records_imported,
            ..
        } => format!(
            "{}: {files_scanned} files, {records_imported} records",
            source.as_str()
        ),
        SyncEvent::RecentReady { source } => {
            format!("{}: recent usage ready", source.as_str())
        }
        SyncEvent::SourceFinished { source, stats } => format!(
            "{}: {} files, {} skipped, {} stored",
            source.as_str(),
            stats.files_processed,
            stats.skipped_files,
            stats.stored_events
        ),
        SyncEvent::Finished { summary } => format!(
            "Sync complete: {} inserted, {} stored",
            summary.total_inserted, summary.stored_events
        ),
        SyncEvent::Failed { error } => format!("Sync failed: {error}"),
        SyncEvent::Cancelled => "Sync cancelled".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parsers::SourceSyncStats, paths::AppPaths, store::HolderKind};
    use tempfile::TempDir;

    #[test]
    fn progress_message_reports_source_counts() {
        let event = SyncEvent::SourceFinished {
            source: SourceKind::Codex,
            stats: SourceSyncStats {
                source: SourceKind::Codex,
                files_processed: 12,
                skipped_files: 7,
                stored_events: 42,
                ..SourceSyncStats::default()
            },
        };

        assert_eq!(
            sync_progress_message(&event),
            "codex: 12 files, 7 skipped, 42 stored"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sync_action_inside_runtime_starts_once_and_second_trigger_cancels() -> Result<()> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        let blocker =
            store.acquire_worker_lock_with(Duration::from_secs(0), HolderKind::Library)?;
        let mut controller = SyncController::new()?;

        assert_eq!(
            controller.start_or_cancel(&store, Some(SourceKind::Antigravity)),
            "Sync running... press x to cancel"
        );
        assert!(controller.active_job_id.is_some());
        assert_eq!(
            controller.start_or_cancel(&store, Some(SourceKind::Antigravity)),
            "Sync cancellation requested"
        );
        assert_eq!(
            controller
                .active_job_id
                .as_deref()
                .and_then(|job_id| controller.registry.snapshot(job_id))
                .map(|snapshot| snapshot.status),
            Some(JobStatus::Cancelling)
        );

        drop(blocker);
        controller.shutdown(Duration::from_secs(2));
        assert!(controller.active_job_id.is_none());
        Ok(())
    }

    #[test]
    fn terminal_updates_are_identified() {
        assert!(is_terminal_sync_update(&SyncUpdate::Completed {
            inserted: 1,
            stored: 2,
        }));
        assert!(is_terminal_sync_update(&SyncUpdate::Failed(
            "locked".to_string()
        )));
        assert!(is_terminal_sync_update(&SyncUpdate::Cancelled));
        assert!(!is_terminal_sync_update(&SyncUpdate::Progress(
            "running".to_string()
        )));
    }
}
