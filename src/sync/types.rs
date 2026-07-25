//! Core sync domain types shared between the sync engine and adapters.
//!
//! These types live here rather than in `commands::sync` so the application
//! layer (`sync::`) does not need to import CLI-adapter code — fixing the
//! ARCH-002 reverse dependency.

use std::path::PathBuf;

use crate::{models::SourceKind, parsers::SourceSyncStats};

/// Summary returned by a completed sync run.
#[derive(Debug, Clone)]
pub struct SyncSummary {
    pub sources: Vec<SourceSyncStats>,
    pub total_seen: usize,
    pub total_inserted: usize,
    pub stored_events: usize,
}

/// Options accepted by a sync run.
#[derive(Debug, Clone, Default)]
pub struct SyncRunOptions {
    pub rebuild: bool,
    pub source: Option<SourceKind>,
    pub recent_days: Option<u32>,
    pub parallelism: Option<usize>,
    pub provider_map: Option<PathBuf>,
    pub json_events: bool,
    pub allow_lossy_rebuild: bool,
}
