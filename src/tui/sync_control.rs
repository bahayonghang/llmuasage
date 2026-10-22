use crate::common::i18n::tui::format as tr_format;
use crate::common::i18n::tui::text as tr;
use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc},
    time::Duration,
};

use anyhow::Result;

use crate::common::i18n::{
    self,
    commands::{DiagnosticFacts, DomainDiagnostic},
};
use crate::sync::{
    failure::{FailureKind, SyncFailure},
    job_registry::{JobPresentation, RegistryFailure},
    notices::RunNotice,
};
use crate::{
    models::SourceKind,
    parsers::SyncEvent,
    store::Store,
    sync::{JobRegistry, JobStatus, SyncOptions},
};

// Match JobRegistry's bounded event channel; yield even if producers replenish it.
const SYNC_DRAIN_BATCH: usize = 128;

#[derive(Debug, Clone)]
enum FailurePresentation {
    Sync(FailureKind),
    Diagnostic(DiagnosticFacts),
    Registry {
        command: &'static str,
        status: &'static str,
        finish: DiagnosticFacts,
        original: Option<Box<Self>>,
    },
    Original(String),
}

impl FailurePresentation {
    fn capture(
        error: &anyhow::Error,
        pricing: Option<&crate::domain::pricing_catalog::diagnostic::DiagnosticFacts>,
    ) -> Self {
        if let Some(error) = error.downcast_ref::<RegistryFailure>() {
            return Self::Registry {
                command: error.command,
                status: error.status,
                finish: DiagnosticFacts::Domain(DomainDiagnostic::from_error(&error.finish_error)),
                original: error
                    .original_error
                    .as_ref()
                    .map(|error| Box::new(Self::capture(error, pricing))),
            };
        }
        if let Some(facts) = pricing {
            return Self::Diagnostic(DiagnosticFacts::Pricing(facts.clone()));
        }
        if let Some(failure) = error.downcast_ref::<SyncFailure>() {
            return Self::Sync(failure.kind.clone());
        }
        if let Some(error) = error.downcast_ref::<crate::LlmusageError>() {
            return Self::Diagnostic(DiagnosticFacts::Domain(DomainDiagnostic::from_error(error)));
        }
        Self::Original(format!("{error:#}"))
    }

    fn render(&self) -> String {
        match self {
            Self::Sync(kind) => i18n::sync::failure(kind, i18n::current()),
            Self::Diagnostic(facts) => facts.render(i18n::current()).unwrap_or_default(),
            Self::Registry {
                command,
                status,
                finish,
                original,
            } => i18n::sync::registry_failure(
                command,
                status,
                &finish.render(i18n::current()).unwrap_or_default(),
                original.as_ref().map(|facts| facts.render()).as_deref(),
                i18n::current(),
            ),
            Self::Original(detail) => i18n::tui::original(detail),
        }
    }
}

enum PresentationUpdate {
    Notice(RunNotice),
    Failed(FailurePresentation),
}

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
    presentation: mpsc::Receiver<(String, PresentationUpdate)>,
    failure: Option<FailurePresentation>,
    remote: BTreeMap<String, crate::remote::diagnostic::DiagnosticFacts>,
}

impl SyncController {
    pub(super) fn new() -> Result<Self> {
        let (tx, presentation) = mpsc::channel();
        let registry = JobRegistry::with_presentation_observer(Arc::new(move |job, update| {
            let update = match update {
                JobPresentation::Notice(notice) => PresentationUpdate::Notice(notice.clone()),
                JobPresentation::Failed { error, pricing } => {
                    PresentationUpdate::Failed(FailurePresentation::capture(error, pricing))
                }
            };
            let _ = tx.send((job.to_string(), update));
        }));
        Ok(Self {
            runtime: tokio::runtime::Handle::try_current().map_err(|err| {
                anyhow::anyhow!(
                    "{} {err}",
                    i18n::text("TUI requires a Tokio runtime:", "TUI 需要 Tokio 运行时：")
                )
            })?,
            registry,
            active_job_id: None,
            events: None,
            presentation,
            failure: None,
            remote: BTreeMap::new(),
        })
    }

    pub(super) fn start_or_cancel(&mut self, store: &Store, source: Option<SourceKind>) -> String {
        if let Some(job_id) = self.active_job_id.as_deref() {
            return if self.registry.cancel(job_id) {
                tr("sync_cancellation_requested").to_string()
            } else {
                tr("sync_job_is_no_longer_available").to_string()
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
                tr("sync_running_press_x_to_cancel").to_string()
            }
            Err(err) => {
                use crate::sync::job_registry::JobStartError;
                let detail = match &err {
                    JobStartError::Active(active) => format!(
                        "{} {}",
                        i18n::text("Sync job already active:", "已有活动同步任务："),
                        active.active_job_id
                    ),
                    JobStartError::InvalidRequest(request) => {
                        use crate::sync::SyncRequestErrorCode;
                        let explanation = match request.code {
                            SyncRequestErrorCode::UnknownSource => {
                                i18n::text("Unknown sync source", "未知同步来源")
                            }
                            SyncRequestErrorCode::InvalidParallelism => {
                                i18n::text("Invalid sync parallelism", "同步并发数无效")
                            }
                            SyncRequestErrorCode::InvalidRecentDays => {
                                i18n::text("Invalid recent-day window", "近期天数范围无效")
                            }
                        };
                        format!("{explanation} [{}]", request.code.as_str())
                    }
                };
                tr_format!(sync_start_rejected_err, err = detail)
            }
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.active_job_id.is_some()
    }

    pub(super) fn drain_updates(&mut self) -> Vec<SyncUpdate> {
        let mut updates = Vec::new();
        let mut received = Vec::new();
        let mut disconnected = false;
        if let Some(events) = self.events.as_mut() {
            for _ in 0..SYNC_DRAIN_BATCH {
                match events.try_recv() {
                    Ok(event) => received.push(event),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        // Facts are emitted before their public event. Drain after collecting events
        // so a terminal event cannot race ahead of its same-job typed explanation.
        while let Ok((job, presentation)) = self.presentation.try_recv() {
            if self.active_job_id.as_deref() != Some(job.as_str()) {
                continue;
            }
            match presentation {
                PresentationUpdate::Failed(facts) => self.failure = Some(facts),
                PresentationUpdate::Notice(notice) => {
                    if let RunNotice::RemoteSkipped {
                        host_id,
                        facts: Some(facts),
                        ..
                    } = &notice
                    {
                        self.remote.insert(host_id.clone(), *facts.clone());
                    }
                    if let Some(message) = i18n::sync::notice(&notice, i18n::current()) {
                        updates.push(SyncUpdate::Progress(message));
                    }
                }
            }
        }
        for event in received {
            let update = match &event {
                SyncEvent::Failed { error } => SyncUpdate::Failed(
                    self.failure
                        .as_ref()
                        .map(|facts| facts.render())
                        .unwrap_or_else(|| i18n::tui::original(error)),
                ),
                SyncEvent::RemoteHostSkipped { host_id, .. } => SyncUpdate::Progress(
                    i18n::sync::progress_with_remote(
                        &event,
                        self.remote.remove(host_id).as_ref(),
                        i18n::current(),
                    )
                    .unwrap_or_else(|| sync_progress_message(&event)),
                ),
                _ => sync_update(event),
            };
            updates.push(update);
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
                self.failure
                    .as_ref()
                    .map(|facts| facts.render())
                    .unwrap_or_else(|| {
                        snapshot
                            .error
                            .map(|error| i18n::tui::original(&error))
                            .unwrap_or_else(|| {
                                i18n::text("Sync job failed", "同步任务失败").to_string()
                            })
                    }),
            )),
            JobStatus::Cancelled => Some(SyncUpdate::Cancelled),
            _ => None,
        }
    }

    fn clear_active(&mut self) {
        self.active_job_id = None;
        self.events = None;
        self.failure = None;
        self.remote.clear();
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
        SyncEvent::Started { .. } => tr("sync_started").to_string(),
        SyncEvent::BootstrapStarted => tr("preparing_local_database").to_string(),
        SyncEvent::MigrationStarted {
            version,
            latest_version,
            ..
        } => tr_format!(
            migrating_database_v_version_latest_version,
            version = version,
            latest_version = latest_version
        ),
        SyncEvent::MigrationFinished {
            version,
            elapsed_ms,
            ..
        } => tr_format!(
            database_migration_v_version_complete_elapsed_ms_ms,
            version = version,
            elapsed_ms = elapsed_ms
        ),
        SyncEvent::PricingUpgradeStarted { total_events, .. } => {
            tr_format!(
                updating_prices_for_total_events_events,
                total_events = total_events
            )
        }
        SyncEvent::PricingUpgradeProgress {
            processed_events,
            total_events,
            ..
        } => tr_format!(
            updating_prices_processed_events_total_events,
            processed_events = processed_events,
            total_events = total_events
        ),
        SyncEvent::PricingBucketReconcileStarted { bucket_count, .. } => {
            tr_format!(
                reconciling_bucket_count_pricing_buckets,
                bucket_count = bucket_count
            )
        }
        SyncEvent::PricingUpgradeFinished {
            updated_events,
            bucket_count,
            ..
        } => tr_format!(
            prices_updated_updated_events_events_bucket_count_buckets,
            updated_events = updated_events,
            bucket_count = bucket_count
        ),
        SyncEvent::LockWaiting { .. } => tr("waiting_for_sync_worker_lock").to_string(),
        SyncEvent::LockAcquired { wait_ms } => {
            tr_format!(
                sync_worker_lock_acquired_after_wait_ms_ms,
                wait_ms = wait_ms
            )
        }
        SyncEvent::TokenAccountingRepairStarted { sources } => tr_format!(
            repairing_legacy_token_accounting,
            sources
                .iter()
                .map(|source| source.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyncEvent::TokenAccountingRepairFinished { sources } => tr_format!(
            token_accounting_repair_complete,
            sources
                .iter()
                .map(|source| source.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyncEvent::SourceStarted {
            source,
            files_total,
        } => {
            let action = if matches!(source, SourceKind::Codex | SourceKind::Claude) {
                tr("replaying")
            } else {
                tr("scanning")
            };
            tr_format!(
                action_files_total_files,
                source.as_str(),
                action = action,
                files_total = files_total
            )
        }
        SyncEvent::Progress {
            source,
            files_scanned,
            records_imported,
            ..
        } => {
            let work = if matches!(source, SourceKind::Codex | SourceKind::Claude) {
                tr_format!(files_scanned_files_replayed, files_scanned = files_scanned)
            } else {
                tr_format!(files_scanned_rows_scanned, files_scanned = files_scanned)
            };
            tr_format!(
                work_records_imported_records,
                source.as_str(),
                work = work,
                records_imported = records_imported
            )
        }
        SyncEvent::RecentReady { source } => {
            tr_format!(recent_usage_ready, source.as_str())
        }
        SyncEvent::SourceFinished { source, stats } => tr_format!(
            files_skipped_stored,
            source.as_str(),
            stats.files_processed,
            stats.skipped_files,
            stats.stored_events
        ),
        SyncEvent::RemoteHostStarted { label, .. } => {
            tr_format!(importing_remote_host_label, label = label)
        }
        SyncEvent::RemoteHostFinished { label, stats, .. } => {
            tr_format!(
                remote_host_label_imported_sources,
                stats.len(),
                label = label
            )
        }
        SyncEvent::RemoteHostSkipped { label, reason, .. } => {
            tr_format!(
                skipped_remote_host_label_reason,
                label = label,
                reason = reason
            )
        }
        SyncEvent::Finished { summary } => tr_format!(
            sync_complete_inserted_stored,
            summary.total_inserted,
            summary.stored_events
        ),
        SyncEvent::Failed { error } => tr_format!(sync_failed_error, error = error),
        SyncEvent::Cancelled => tr("sync_cancelled").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parsers::SourceSyncStats, paths::AppPaths, store::HolderKind};
    use tempfile::TempDir;

    #[tokio::test]
    async fn sync_drain_yields_at_one_event_batch_and_keeps_the_remainder() -> Result<()> {
        let mut controller = SyncController::new()?;
        let (tx, rx) = tokio::sync::mpsc::channel(SYNC_DRAIN_BATCH * 2);
        controller.active_job_id = Some("fixture-job".to_string());
        controller.events = Some(rx);
        for _ in 0..SYNC_DRAIN_BATCH * 2 {
            tx.try_send(SyncEvent::BootstrapStarted)?;
        }
        assert_eq!(controller.drain_updates().len(), SYNC_DRAIN_BATCH);
        assert_eq!(controller.drain_updates().len(), SYNC_DRAIN_BATCH);
        assert!(controller.drain_updates().is_empty());
        assert!(controller.is_active());
        Ok(())
    }

    #[tokio::test]
    async fn terminal_failure_uses_same_job_facts_and_ui_thread_language() -> Result<()> {
        let mut controller = SyncController::new()?;
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        let (presentation_tx, presentation_rx) = mpsc::channel();
        controller.presentation = presentation_rx;
        controller.active_job_id = Some("active".into());
        controller.events = Some(rx);
        let typed = anyhow::Error::new(SyncFailure::new(
            FailureKind::OmpBeforePiSplit,
            "CANONICAL_MACHINE_REFUSAL".into(),
        ));
        let facts = FailurePresentation::capture(&typed, None);
        presentation_tx.send((
            "old".into(),
            PresentationUpdate::Failed(FailurePresentation::Original("STALE_ERROR".into())),
        ))?;
        presentation_tx.send(("active".into(), PresentationUpdate::Failed(facts)))?;
        tx.try_send(SyncEvent::Failed {
            error: typed.to_string(),
        })?;
        let updates = i18n::with_language(i18n::Language::Chinese, || controller.drain_updates());
        let Some(SyncUpdate::Failed(message)) = updates.last() else {
            panic!("missing terminal update: {updates:?}")
        };
        assert!(
            message.contains("拒绝同步 --source omp")
                && message.contains("`llmusage sync --rebuild --source pi`"),
            "{message}"
        );
        assert!(
            !message.contains("CANONICAL_MACHINE_REFUSAL") && !message.contains("STALE_ERROR"),
            "{message}"
        );
        assert_eq!(typed.to_string(), "CANONICAL_MACHINE_REFUSAL");
        assert!(!controller.is_active());
        Ok(())
    }

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

    #[test]
    fn progress_message_distinguishes_replay_files_from_stream_rows() {
        assert_eq!(
            sync_progress_message(&SyncEvent::SourceStarted {
                source: SourceKind::Codex,
                files_total: 2,
            }),
            "codex: replaying 2 files"
        );
        assert_eq!(
            sync_progress_message(&SyncEvent::Progress {
                source: SourceKind::Claude,
                files_scanned: 2,
                records_imported: 4,
                current_file: None,
            }),
            "claude: 2 files replayed, 4 records"
        );
        assert_eq!(
            sync_progress_message(&SyncEvent::Progress {
                source: SourceKind::Opencode,
                files_scanned: 10,
                records_imported: 3,
                current_file: None,
            }),
            "opencode: 10 rows scanned, 3 records"
        );
    }

    #[test]
    fn progress_message_reports_token_accounting_repair_boundaries() {
        let sources = vec![SourceKind::Codex, SourceKind::Claude];
        assert_eq!(
            sync_progress_message(&SyncEvent::TokenAccountingRepairStarted {
                sources: sources.clone(),
            }),
            "Repairing legacy token accounting: codex, claude"
        );
        assert_eq!(
            sync_progress_message(&SyncEvent::TokenAccountingRepairFinished { sources }),
            "Token accounting repair complete: codex, claude"
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
