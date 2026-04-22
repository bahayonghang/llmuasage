use serde::Serialize;

use crate::models::SourceKind;

pub mod claude;
pub mod codex;
pub mod opencode;

#[derive(Debug, Clone, Serialize)]
pub struct SourceSyncStats {
    pub source: SourceKind,
    pub files_processed: usize,
    pub events_seen: usize,
    pub events_inserted: usize,
    pub last_error: Option<String>,
}

impl Default for SourceSyncStats {
    fn default() -> Self {
        Self {
            source: SourceKind::Codex,
            files_processed: 0,
            events_seen: 0,
            events_inserted: 0,
            last_error: None,
        }
    }
}
