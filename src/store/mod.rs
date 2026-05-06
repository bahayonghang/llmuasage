use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::{
    models::{SourceKind, UsageEvent, UsageTokens},
    paths::AppPaths,
};

mod connection;
mod cursor;
mod integration;
mod lease;
mod run_log;
mod schema;
mod sync_status;
mod sync_writer;
mod trigger;

pub use cursor::CursorStore;
pub use integration::IntegrationStateStore;
pub use run_log::RunLog;
pub use sync_status::SyncStatusStore;
pub use trigger::TriggerStore;

const WORKER_LOCK_NAME: &str = "sync-worker";
const WORKER_LEASE_MINUTES: i64 = 30;

/// Incremental cursor for file-backed JSONL sources.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileCursor {
    /// Stable cursor key, usually the raw file path.
    pub cursor_key: String,
    /// Original file path for diagnostics.
    pub file_path: String,
    /// Head/tail-aware file fingerprint used to detect replace vs append.
    pub file_fingerprint: String,
    /// Last observed file size in bytes.
    pub file_size: u64,
    /// Last observed file mtime in nanoseconds since epoch.
    pub file_mtime_ns: i64,
    /// Signature of the tail window near the stored offset.
    pub tail_signature: String,
    /// Next byte offset to resume incremental parsing from.
    pub offset: u64,
    /// Last cumulative token snapshot used to derive Codex deltas.
    pub last_total: Option<UsageTokens>,
    /// Last model observed in the source file.
    pub last_model: Option<String>,
    /// Last cursor refresh time in RFC 3339 format.
    pub updated_at: String,
}

impl FileCursor {
    /// Compares only the persisted material fields used for replay decisions.
    pub fn materially_eq(&self, other: &Self) -> bool {
        self.cursor_key == other.cursor_key
            && self.file_path == other.file_path
            && self.file_fingerprint == other.file_fingerprint
            && self.file_size == other.file_size
            && self.file_mtime_ns == other.file_mtime_ns
            && self.tail_signature == other.tail_signature
            && self.offset == other.offset
            && self.last_total == other.last_total
            && self.last_model == other.last_model
    }
}

/// Incremental cursor for the OpenCode SQLite source.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpencodeCursor {
    /// Persisted DB identity fingerprint used to detect replacement/rotation.
    pub inode: u64,
    /// Highest `time_created` value fully processed so far.
    pub last_time_created: i64,
    /// Message ids already consumed at the high-water timestamp.
    pub last_processed_ids: Vec<String>,
    /// Last observed SQLite status such as `ok` or `missing-db`.
    pub sqlite_status: String,
    /// Last cursor refresh time in RFC 3339 format.
    pub updated_at: String,
}

/// Latest known install/probe state for one integration surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationState {
    /// Source the integration belongs to.
    pub source: String,
    /// Action family that wrote the record, such as `init`, `uninstall`, or `probe`.
    pub install_type: String,
    /// Current state, for example `ready`, `restored`, `skipped`, or `error`.
    pub status: String,
    /// Optional config path touched by the integration.
    pub config_path: Option<String>,
    /// Optional backup path created before mutation.
    pub backup_path: Option<String>,
    /// Optional JSON-encoded extra details for diagnostics.
    pub details_json: Option<String>,
    /// Last update time in RFC 3339 format.
    pub updated_at: String,
}

/// One CLI command execution recorded in `run_log`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    /// Monotonic autoincrement id.
    pub id: i64,
    /// Command label such as `sync`, `init`, or `export html`.
    pub command: String,
    /// Lifecycle status such as `running`, `success`, `failed`, or `aborted`.
    pub status: String,
    /// Optional human-readable success summary.
    pub summary: Option<String>,
    /// Optional failure or recovery detail.
    pub error: Option<String>,
    /// Start time in RFC 3339 format.
    pub started_at: String,
    /// Finish time when the command has closed.
    pub finished_at: Option<String>,
}

impl RunRecord {
    /// Returns whether this run should surface as a recent failure in health/doctor views.
    pub fn counts_as_failure(&self) -> bool {
        self.status != "success" && self.status != "running"
    }
}

/// Hook signal bookkeeping used by `hook-run` workers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerStateRecord {
    /// Source the hook signal belongs to.
    pub source: String,
    /// Last signal time seen for the source.
    pub last_signal_at: String,
    /// Raw trigger/event name reported by the integration.
    pub trigger: String,
    /// Last worker start time for this source.
    pub last_worker_started_at: Option<String>,
    /// Last worker finish time for this source.
    pub last_worker_finished_at: Option<String>,
    /// Last update time in RFC 3339 format.
    pub updated_at: String,
}

/// Latest sync metrics persisted per source for status and diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSyncStatus {
    /// Source that produced the metrics.
    pub source: String,
    /// Number of candidate files/DBs inspected.
    pub files_processed: i64,
    /// Number of changed files requiring parse work.
    pub changed_files: i64,
    /// Bytes scanned while parsing the source.
    pub bytes_scanned: i64,
    /// Normalized events observed before dedupe.
    pub events_seen: i64,
    /// Events replayed because a file had to be rebuilt.
    pub events_replayed: i64,
    /// Newly inserted events after SQLite dedupe.
    pub events_inserted: i64,
    /// Parser wall-clock time in milliseconds.
    pub parse_ms: i64,
    /// SQLite write wall-clock time in milliseconds.
    pub write_ms: i64,
    /// Time spent waiting on the global worker lock in milliseconds.
    pub lock_wait_ms: i64,
    /// Last update time in RFC 3339 format.
    pub updated_at: String,
}

/// Guard object that owns the global sync worker lease until drop.
pub struct WorkerLock {
    store: Store,
    lock_name: String,
    owner_id: String,
}

/// Main SQLite-backed store façade used across commands, parsers, and queries.
#[derive(Debug, Clone)]
pub struct Store {
    /// Runtime paths that locate the DB, wrappers, backups, and exports.
    pub paths: AppPaths,
}

impl Store {
    /// Borrowed view onto the `source_cursor` surface.
    pub fn cursors(&self) -> CursorStore<'_> {
        CursorStore::new(self)
    }

    /// Borrowed view onto the `integration_install` surface.
    pub fn integration_state(&self) -> IntegrationStateStore<'_> {
        IntegrationStateStore::new(self)
    }

    /// Borrowed view onto the `run_log` surface.
    pub fn run_log(&self) -> RunLog<'_> {
        RunLog::new(self)
    }

    /// Borrowed view onto the `source_sync_status` surface.
    pub fn sync_status(&self) -> SyncStatusStore<'_> {
        SyncStatusStore::new(self)
    }

    /// Borrowed view onto the `trigger_state` surface.
    pub fn triggers(&self) -> TriggerStore<'_> {
        TriggerStore::new(self)
    }
}

/// Single-connection writer used by sync to batch event/cursor updates transactionally.
pub struct SyncRunWriter {
    conn: Connection,
}

/// Atomic per-shard payload committed by [`SyncRunWriter::commit_shard`].
///
/// Bundles the implicit reset → write_event → write_cursor protocol that every
/// file-backed parser used to inline. Parsers produce one shard per chunk of
/// candidate files; the writer enforces ordering and chunking. Streaming
/// sources (e.g. OpenCode) submit shards with empty `reset_path_hashes` and
/// `cursors`, retaining their own custom cursor persistence.
#[derive(Debug)]
pub struct SyncShard {
    /// Source the shard belongs to. Used by reset/cursor SQL keys.
    pub source: SourceKind,
    /// Path hashes whose existing events must be cleared before re-inserting.
    pub reset_path_hashes: Vec<String>,
    /// Normalized usage events to upsert in chunked transactions.
    pub events: Vec<UsageEvent>,
    /// File cursors to persist after events land. Empty for streaming sources.
    pub cursors: Vec<FileCursor>,
}

impl SyncShard {
    /// Builds an empty shard scoped to one source. Caller fills the vecs.
    pub fn new(source: SourceKind) -> Self {
        Self {
            source,
            reset_path_hashes: Vec::new(),
            events: Vec::new(),
            cursors: Vec::new(),
        }
    }
}

/// Outcome of [`SyncRunWriter::commit_shard`] reported back to the caller.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShardCommitStats {
    /// Newly inserted events after SQLite dedupe.
    pub events_inserted: usize,
    /// Wall-clock milliseconds spent inside the commit (reset + events + cursors).
    pub write_ms: u64,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct BucketKey {
    source: String,
    model: String,
    hour_start: String,
    project_hash: String,
}

#[derive(Debug, Clone)]
struct BucketRollup {
    project_label: Option<String>,
    project_ref: Option<String>,
    tokens: UsageTokens,
}
