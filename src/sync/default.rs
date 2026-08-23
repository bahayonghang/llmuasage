//! Default sync executor and JobRegistry composition.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    app::AppContext,
    parsers::SyncEvent,
    store::Store,
    sync::{
        engine::run_once_locked,
        executor::{BoxFuture, SyncExecutor},
        job_registry::JobRegistry,
        types::{SyncRunOptions, SyncSummary},
    },
};

/// Canonical executor for the in-process sync application engine.
#[derive(Debug, Default)]
pub struct DefaultSyncExecutor;

impl SyncExecutor for DefaultSyncExecutor {
    fn run_once<'a>(
        &'a self,
        _app: &'a AppContext,
        store: &'a Store,
        lock_wait_ms: u64,
        options: &'a SyncRunOptions,
        sender: Option<&'a mut mpsc::Sender<SyncEvent>>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, anyhow::Result<SyncSummary>> {
        Box::pin(run_once_locked(
            store,
            lock_wait_ms,
            options,
            sender,
            cancel,
        ))
    }
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new(Arc::new(DefaultSyncExecutor))
    }
}
