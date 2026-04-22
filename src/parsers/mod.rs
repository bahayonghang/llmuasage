use serde::Serialize;

use crate::{
    models::{SourceKind, UsageEvent},
    store::{FileCursor, OpencodeCursor},
};

pub mod claude;
pub mod codex;
pub mod file_state;
pub mod opencode;

#[derive(Debug, Clone, Serialize)]
pub struct SourceSyncStats {
    pub source: SourceKind,
    pub files_processed: usize,
    pub changed_files: usize,
    pub bytes_scanned: u64,
    pub events_seen: usize,
    pub events_replayed: usize,
    pub events_inserted: usize,
    pub parse_ms: u64,
    pub write_ms: u64,
    pub lock_wait_ms: u64,
    pub last_error: Option<String>,
}

impl Default for SourceSyncStats {
    fn default() -> Self {
        Self {
            source: SourceKind::Codex,
            files_processed: 0,
            changed_files: 0,
            bytes_scanned: 0,
            events_seen: 0,
            events_replayed: 0,
            events_inserted: 0,
            parse_ms: 0,
            write_ms: 0,
            lock_wait_ms: 0,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceParseOutput {
    pub source: SourceKind,
    pub events: Vec<UsageEvent>,
    pub cursors: Vec<FileCursor>,
    pub opencode_cursor: Option<OpencodeCursor>,
    pub reset_path_hashes: Vec<String>,
    pub stats: SourceSyncStats,
}
