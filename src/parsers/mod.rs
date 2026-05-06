use serde::Serialize;

use crate::models::SourceKind;

pub mod claude;
pub mod codex;
pub mod driver;
pub mod file_state;
pub mod opencode;
pub mod source_parser;

pub use claude::ClaudeParser;
pub use codex::CodexParser;
pub use opencode::OpencodeParser;
pub use source_parser::SourceParser;

/// Per-source sync metrics reported after a parser + write cycle completes.
#[derive(Debug, Clone, Serialize)]
pub struct SourceSyncStats {
    /// Source these stats belong to.
    pub source: SourceKind,
    /// Number of candidate files or SQLite sources inspected.
    pub files_processed: usize,
    /// Number of files/DBs that required re-parse or incremental scan.
    pub changed_files: usize,
    /// Bytes scanned while parsing the source.
    pub bytes_scanned: u64,
    /// Number of normalized events observed before dedupe.
    pub events_seen: usize,
    /// Number of events replayed because an existing file had to be rebuilt.
    pub events_replayed: usize,
    /// Number of newly inserted events after SQLite dedupe.
    pub events_inserted: usize,
    /// Parser wall-clock time in milliseconds.
    pub parse_ms: u64,
    /// SQLite write wall-clock time in milliseconds.
    pub write_ms: u64,
    /// Time spent waiting for the global sync worker lock in milliseconds.
    pub lock_wait_ms: u64,
    /// Optional last parse error surfaced for diagnostics.
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
