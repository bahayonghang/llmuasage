//! `SyncExecutor` trait — the boundary between the sync application layer and
//! CLI/web adapter code (ARCH-002).

use std::{future::Future, pin::Pin};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    app::AppContext,
    parsers::SyncEvent,
    store::Store,
    sync::types::{SyncRunOptions, SyncSummary},
};

/// Boxed future alias used for dyn-compatible async methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Executes one sync run with cancellation support.
///
/// The application layer depends on this trait; the CLI adapter
/// (`commands::sync`) provides the implementation. Tests can supply a stub.
pub trait SyncExecutor: Send + Sync + 'static {
    fn run_once<'a>(
        &'a self,
        app: &'a AppContext,
        store: &'a Store,
        lock_wait_ms: u64,
        options: &'a SyncRunOptions,
        sender: Option<&'a mut mpsc::Sender<SyncEvent>>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, anyhow::Result<SyncSummary>>;
}
