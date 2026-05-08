use std::future::Future;
use std::pin::Pin;

use anyhow::Result;
use tokio_util::sync::CancellationToken;

use crate::{
    models::SourceKind,
    parsers::{SourceSyncStats, SyncEvent},
    store::{Store, SyncRunWriter},
};

/// Parser-local progress callback supplied by the sync driver.
pub type ProgressSink<'a> = &'a mut (dyn FnMut(SyncEvent) + Send);

/// Erased async parser interface used by the sync driver.
///
/// Each implementation owns one source's parse → commit pipeline. The driver
/// iterates over a fixed list of parsers per sync run, so additional sources
/// can be added by registering one new implementation rather than by editing
/// the dispatch site in `commands/sync.rs`.
///
/// `parse` returns [`SourceSyncStats`] directly because each parser already
/// computes parse_ms / write_ms / events_seen internally; the driver only
/// injects `lock_wait_ms` afterwards. Streaming and batched parsers share the
/// same signature because the [`SyncRunWriter::commit_shard`] protocol erases
/// the underlying shape — both `OpencodeParser` (page-stream) and
/// `CodexParser` / `ClaudeParser` (file-batch) push committed shards through
/// the same writer.
///
/// The future is returned as a `Pin<Box<dyn Future + Send>>` to keep the trait
/// object-safe without pulling in `async-trait`. Implementations typically
/// `Box::pin` an inner `async fn` that does the real work.
pub trait SourceParser: Send + Sync {
    /// Source kind this parser is responsible for. Used by registry / probe
    /// flows in later phases; the driver itself relies on
    /// [`SourceSyncStats::source`] to label outputs.
    fn source(&self) -> SourceKind;

    /// Run one parse + commit cycle for this source.
    ///
    /// The driver hands over the shared store and writer plus the requested
    /// parallelism. The returned [`SourceSyncStats`] already carries this
    /// parser's metrics; the driver overrides `lock_wait_ms` after the call.
    fn parse<'a>(
        &'a self,
        store: &'a Store,
        writer: &'a mut SyncRunWriter,
        parallelism: usize,
        cancel: &'a CancellationToken,
        progress: Option<ProgressSink<'a>>,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>>;
}
