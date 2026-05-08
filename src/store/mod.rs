use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::{
    models::{SourceKind, UsageEvent, UsageTokens},
    paths::AppPaths,
};

mod connection;
mod cursor;
mod integration;
mod lock;
mod migrations;
mod run_log;
mod schema;
mod source_file;
mod sync_status;
mod sync_writer;
mod trigger;

pub use cursor::CursorStore;
pub use integration::IntegrationStateStore;
pub use migrations::{
    MigrationProgress, MigrationProgressEvent, latest_schema_version, read_schema_version,
};
pub use run_log::RunLog;
pub use source_file::{SourceFileStateCounts, SourceFileStore};
pub use sync_status::SyncStatusStore;
pub use trigger::TriggerStore;

const WORKER_LOCK_NAME: &str = "sync-worker";
const WORKER_LOCK_LEASE_MINUTES: i64 = 30;

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

/// Caller family recorded on the global sync worker lock row.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HolderKind {
    /// Interactive/manual CLI command such as `llmusage sync`.
    Cli,
    /// Library/Tauri/HTTP job caller.
    Library,
    /// Tool hook caller; intentionally uses non-blocking acquisition.
    Hook,
}

impl HolderKind {
    /// Stable lowercase identifier persisted to SQLite and rendered in status.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Library => "library",
            Self::Hook => "hook",
        }
    }
}

impl std::fmt::Display for HolderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Metadata for the current global sync worker lock holder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerLockMeta {
    /// Process id that acquired the lock.
    pub holder_pid: i64,
    /// Caller family that acquired the lock.
    pub holder_kind: String,
    /// RFC 3339 acquisition timestamp.
    pub acquired_at: String,
    /// RFC 3339 lease expiry timestamp.
    pub lease_expires_at: String,
    /// Last refresh timestamp.
    pub updated_at: String,
}

impl WorkerLockMeta {
    /// Compact identity string used by `LlmusageError::LockBusy`.
    pub fn holder_identity(&self) -> String {
        format!(
            "{}:{}@{}",
            self.holder_kind, self.holder_pid, self.acquired_at
        )
    }
}

/// Guard object that owns the global sync worker lock until drop.
pub struct WorkerLock {
    store: Store,
    lock_name: String,
    owner_id: String,
    meta: WorkerLockMeta,
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

    /// Recomputes and persists per-event cost columns from the embedded
    /// static pricing catalog (D6/F1.3). Returns the number of `usage_event`
    /// rows updated. Single transaction so a partial run never leaves the
    /// table half-priced.
    pub fn recompute_costs(&self) -> crate::error::Result<usize> {
        self.recompute_costs_with(crate::query::pricing_catalog::PricingCatalog::static_v1())
    }

    /// Recomputes costs against a caller-supplied catalog. doctor uses
    /// this to drive a recompute through a litellm snapshot loaded from
    /// `~/.llmusage/pricing/`.
    pub fn recompute_costs_with(
        &self,
        catalog: &crate::query::pricing_catalog::PricingCatalog,
    ) -> crate::error::Result<usize> {
        use rusqlite::params;

        let mut conn = self.open_connection()?;
        let tx = conn.transaction()?;

        let rows: Vec<(String, String, String, i64, i64, i64, i64)> = {
            let mut stmt = tx.prepare(
                r#"
                SELECT event_key, source, model,
                       COALESCE(input_tokens, 0),
                       COALESCE(cache_read_tokens, 0),
                       COALESCE(output_tokens, 0),
                       COALESCE(reasoning_output_tokens, 0)
                FROM usage_event
                "#,
            )?;
            let mapped = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?;
            mapped.collect::<rusqlite::Result<Vec<_>>>()?
        };

        let mut updated = 0usize;
        {
            let mut update_stmt = tx.prepare(
                r#"
                UPDATE usage_event
                SET cost_with_cache_usd = ?2,
                    cost_without_cache_usd = ?3,
                    pricing_status = ?4,
                    pricing_source = ?5,
                    pricing_rate = ?6
                WHERE event_key = ?1
                "#,
            )?;
            for (event_key, source, model, input, cache_read, output, reasoning) in rows {
                let breakdown = crate::query::pricing::compute_cost_with(
                    catalog, &source, &model, input, cache_read, output, reasoning,
                );
                update_stmt.execute(params![
                    event_key,
                    breakdown.cost_with_cache_usd,
                    breakdown.cost_without_cache_usd,
                    breakdown.pricing_status.as_str(),
                    breakdown.pricing_source,
                    breakdown.pricing_rate,
                ])?;
                updated += 1;
            }
        }
        tx.commit()?;
        Ok(updated)
    }
}

/// Single-connection writer used by sync to batch event/cursor updates transactionally.
pub struct SyncRunWriter {
    conn: Connection,
    run_started_at: String,
    raw_archive_enabled: bool,
}

impl SyncRunWriter {
    /// Returns the RFC 3339 timestamp captured when this writer was started.
    ///
    /// Used by the `source_file` state machine: every shard commit stamps
    /// `last_seen_at = run_started_at` so a single later
    /// `update_missing_with_conn(source, run_started_at)` call can flip stale
    /// `live` rows to `missing` without race conditions across parsers.
    pub fn run_started_at(&self) -> &str {
        &self.run_started_at
    }

    /// Returns whether this writer captures raw archive payloads at commit
    /// time. The flag is snapshotted from `meta('raw_archive_enabled')` once
    /// at [`Store::begin_sync_run`]; mid-run toggles only take effect on the
    /// next sync run, so a single sync either persists every raw record or
    /// none.
    pub fn raw_archive_enabled(&self) -> bool {
        self.raw_archive_enabled
    }
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
    /// File paths observed during the parser pass, regardless of whether they
    /// produced new events. The writer marks each one `state='live'` in the
    /// `source_file` table so the driver can later flip unseen files to
    /// `missing`. Empty for streaming sources without per-file identity.
    pub seen_file_paths: Vec<String>,
    /// Optional raw payloads keyed by `event_key`. Only consumed when
    /// `Store::raw_archive_enabled` is true; otherwise dropped silently
    /// (D11 / F1.5). Parsers that never serialize raw rows leave this empty.
    pub raw_records: Vec<RawRecord>,
}

impl SyncShard {
    /// Builds an empty shard scoped to one source. Caller fills the vecs.
    pub fn new(source: SourceKind) -> Self {
        Self {
            source,
            reset_path_hashes: Vec::new(),
            events: Vec::new(),
            cursors: Vec::new(),
            seen_file_paths: Vec::new(),
            raw_records: Vec::new(),
        }
    }
}

/// One opt-in raw archive entry written to `usage_event_raw`.
///
/// `event_key` matches `usage_event.event_key` 1:1 so consumers can join back
/// to the normalized row. `raw_json` is the parser-specific serialization of
/// the upstream record (e.g. an OpenCode SQLite row rendered as JSON).
#[derive(Debug, Clone)]
pub struct RawRecord {
    /// Same `event_key` value as the corresponding `usage_event` row.
    pub event_key: String,
    /// JSON-encoded payload describing the upstream row. Parser-specific
    /// shape; consumers should treat it as opaque text.
    pub raw_json: String,
}

/// Options controlling one [`Store::bootstrap_with`] call.
///
/// Currently exposes only the raw archive opt-in (D11 / F1.5). The struct is
/// `#[non_exhaustive]` so adding new bootstrap-time toggles in 0.5.x patches
/// stays SemVer-additive.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct BootstrapOptions {
    /// When `Some(true)` / `Some(false)`, persist the corresponding
    /// `meta('raw_archive_enabled', …)` value during bootstrap. `None` keeps
    /// whatever value the meta row already holds (or the migration-installed
    /// default `'0'` on a freshly created database).
    pub enable_raw_archive: Option<bool>,
}

impl BootstrapOptions {
    /// Toggles the raw archive flag during bootstrap.
    pub fn with_raw_archive(mut self, enabled: bool) -> Self {
        self.enable_raw_archive = Some(enabled);
        self
    }
}

/// Outcome of [`SyncRunWriter::commit_shard`] reported back to the caller.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShardCommitStats {
    /// Newly inserted events after SQLite dedupe.
    pub events_inserted: usize,
    /// Wall-clock milliseconds spent inside the commit (reset + events + cursors).
    pub write_ms: u64,
    /// Number of source files observed and marked live by this shard.
    pub files_seen: usize,
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
    event_count: i64,
}
