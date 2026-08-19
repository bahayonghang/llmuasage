use serde::{Deserialize, Serialize};

use crate::{
    models::SourceKind,
    store::{BootstrapProgressEvent, MigrationProgress, MigrationProgressEvent},
};

pub mod antigravity;
pub(crate) mod behavior;
pub mod claude;
pub mod codex;
pub mod driver;
pub mod dsh;
mod file_progress;
pub mod file_state;
pub mod grok;
pub mod kimi_code;
pub mod opencode;
pub mod pi;
pub(crate) mod source_files;
pub mod source_parser;
pub mod zcode;

pub use crate::models::{ParseIssueKind, ParseIssueSample, ParseIssues};
pub use antigravity::AntigravityParser;
pub use claude::ClaudeParser;
pub use codex::CodexParser;
pub use dsh::DeepseekHarnessParser;
pub use grok::GrokParser;
pub use kimi_code::KimiCodeParser;
pub use opencode::OpencodeParser;
pub use pi::PiParser;
pub use source_parser::{ProgressSink, SourceParser};
pub use zcode::ZcodeParser;

pub(crate) fn timestamp_in_recent_window(
    timestamp: &str,
    cutoff: Option<&chrono::DateTime<chrono::Utc>>,
) -> bool {
    cutoff.is_none_or(|cutoff| {
        chrono::DateTime::parse_from_rfc3339(timestamp)
            .map(|value| value.with_timezone(&chrono::Utc) >= *cutoff)
            .unwrap_or(false)
    })
}

/// Progress and lifecycle events emitted by sync/import flows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "event")]
pub enum SyncEvent {
    /// Sync job has started.
    Started { job_id: String, files_total: u64 },
    /// Runtime directories and SQLite bootstrap are about to run.
    BootstrapStarted,
    /// One SQLite schema migration has started.
    MigrationStarted {
        version: u32,
        name: String,
        latest_version: u32,
    },
    /// One SQLite schema migration has committed.
    MigrationFinished {
        version: u32,
        name: String,
        elapsed_ms: u64,
    },
    /// An embedded pricing catalog upgrade is about to reprice stored events.
    PricingUpgradeStarted {
        from_version: String,
        to_version: String,
        total_events: usize,
    },
    /// A committed pricing page made durable progress.
    PricingUpgradeProgress {
        from_version: String,
        to_version: String,
        processed_events: usize,
        total_events: usize,
        elapsed_ms: u64,
    },
    /// Event repricing finished and bucket pricing reconciliation is starting.
    PricingBucketReconcileStarted {
        to_version: String,
        bucket_count: usize,
    },
    /// Embedded pricing activation and bucket reconciliation committed.
    PricingUpgradeFinished {
        from_version: String,
        to_version: String,
        updated_events: usize,
        bucket_count: usize,
        deleted_orphan_buckets: usize,
        elapsed_ms: u64,
    },
    /// Caller is waiting for the global SQLite sync worker lock.
    LockWaiting { timeout_ms: u64 },
    /// Global SQLite sync worker lock was acquired.
    LockAcquired { wait_ms: u64 },
    /// A normal unbounded sync is about to rebuild safe legacy accounting rows.
    TokenAccountingRepairStarted { sources: Vec<SourceKind> },
    /// Safe legacy accounting rows were rebuilt and their markers advanced.
    TokenAccountingRepairFinished { sources: Vec<SourceKind> },
    /// A source parser is about to run.
    SourceStarted {
        source: SourceKind,
        files_total: u64,
    },
    /// Throttled source progress snapshot. File-backed parsers report completed
    /// replay files during blocking parse work and at commit boundaries;
    /// streaming parsers report committed page/row work.
    Progress {
        source: SourceKind,
        files_scanned: u64,
        records_imported: u64,
        current_file: Option<String>,
    },
    /// Recent-window scan finished for one source (D27).
    RecentReady { source: SourceKind },
    /// One source completed with final stats.
    SourceFinished {
        source: SourceKind,
        stats: SourceSyncStats,
    },
    /// Full sync completed.
    Finished { summary: SyncSummaryEvent },
    /// Sync failed.
    Failed { error: String },
    /// Sync was cancelled.
    Cancelled,
}

impl From<MigrationProgress> for SyncEvent {
    fn from(value: MigrationProgress) -> Self {
        match value.elapsed_ms {
            Some(elapsed_ms) => SyncEvent::MigrationFinished {
                version: value.version,
                name: value.name.to_string(),
                elapsed_ms,
            },
            None => SyncEvent::MigrationStarted {
                version: value.version,
                name: value.name.to_string(),
                latest_version: crate::store::latest_schema_version(),
            },
        }
    }
}

impl From<BootstrapProgressEvent> for SyncEvent {
    fn from(value: BootstrapProgressEvent) -> Self {
        match value {
            BootstrapProgressEvent::Migration(MigrationProgressEvent::Started(item)) => {
                SyncEvent::MigrationStarted {
                    version: item.version,
                    name: item.name.to_string(),
                    latest_version: crate::store::latest_schema_version(),
                }
            }
            BootstrapProgressEvent::Migration(MigrationProgressEvent::Finished(item)) => {
                SyncEvent::MigrationFinished {
                    version: item.version,
                    name: item.name.to_string(),
                    elapsed_ms: item.elapsed_ms.unwrap_or_default(),
                }
            }
            BootstrapProgressEvent::PricingUpgradeStarted {
                from_version,
                to_version,
                total_events,
            } => SyncEvent::PricingUpgradeStarted {
                from_version,
                to_version,
                total_events,
            },
            BootstrapProgressEvent::PricingUpgradeProgress {
                from_version,
                to_version,
                processed_events,
                total_events,
                elapsed_ms,
            } => SyncEvent::PricingUpgradeProgress {
                from_version,
                to_version,
                processed_events,
                total_events,
                elapsed_ms,
            },
            BootstrapProgressEvent::PricingBucketReconcileStarted {
                to_version,
                bucket_count,
            } => SyncEvent::PricingBucketReconcileStarted {
                to_version,
                bucket_count,
            },
            BootstrapProgressEvent::PricingUpgradeFinished {
                from_version,
                to_version,
                updated_events,
                bucket_count,
                deleted_orphan_buckets,
                elapsed_ms,
            } => SyncEvent::PricingUpgradeFinished {
                from_version,
                to_version,
                updated_events,
                bucket_count,
                deleted_orphan_buckets,
                elapsed_ms,
            },
        }
    }
}

/// Lightweight serializable sync summary used in [`SyncEvent::Finished`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SyncSummaryEvent {
    /// Number of source parsers that ran.
    pub sources: usize,
    /// Total normalized events seen before SQLite dedupe.
    pub total_seen: usize,
    /// Total newly inserted events during this incremental sync run.
    pub total_inserted: usize,
    /// Total imported events currently stored in the database after the run.
    #[serde(default)]
    pub stored_events: usize,
}

/// Per-source sync metrics reported after a parser + write cycle completes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceSyncStats {
    /// Source these stats belong to.
    pub source: SourceKind,
    /// Number of candidate files or SQLite sources inspected.
    pub files_processed: usize,
    /// Number of files/DBs that required re-parse or incremental scan.
    pub changed_files: usize,
    /// Number of files/DBs skipped because cursor/fingerprint evidence showed no new work.
    #[serde(default)]
    pub skipped_files: usize,
    /// Bytes scanned while parsing the source.
    pub bytes_scanned: u64,
    /// Number of normalized events observed before dedupe.
    pub events_seen: usize,
    /// Number of events replayed because an existing file had to be rebuilt.
    pub events_replayed: usize,
    /// Number of newly inserted events after SQLite dedupe.
    pub events_inserted: usize,
    /// Total imported events currently stored for this source after the run.
    #[serde(default)]
    pub stored_events: usize,
    /// Parser wall-clock time in milliseconds.
    pub parse_ms: u64,
    /// SQLite write wall-clock time in milliseconds.
    pub write_ms: u64,
    /// Time spent waiting for the global sync worker lock in milliseconds.
    pub lock_wait_ms: u64,
    /// True when an optional local source is absent rather than failed.
    #[serde(default)]
    pub absent: bool,
    /// Optional last parse error surfaced for diagnostics.
    pub last_error: Option<String>,
    /// Privacy-safe parse-issue counters and bounded samples.
    #[serde(default)]
    pub parse_issues: ParseIssues,
}

impl Default for SourceSyncStats {
    fn default() -> Self {
        Self {
            source: SourceKind::Codex,
            files_processed: 0,
            changed_files: 0,
            skipped_files: 0,
            bytes_scanned: 0,
            events_seen: 0,
            events_replayed: 0,
            events_inserted: 0,
            stored_events: 0,
            parse_ms: 0,
            write_ms: 0,
            lock_wait_ms: 0,
            absent: false,
            last_error: None,
            parse_issues: ParseIssues::default(),
        }
    }
}

#[cfg(test)]
mod bounded_jsonl_contract_tests {
    use std::{
        io::{Seek, Write},
        path::Path,
    };

    use anyhow::Result;
    use tempfile::TempDir;

    use super::{ParseIssues, SourceKind, claude, codex, grok, kimi_code, pi};

    type ContractParser = fn(&Path) -> Result<(ParseIssues, u64, bool)>;

    #[test]
    fn file_jsonl_parsers_share_the_bounded_jsonl_contract() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("shared-contract.jsonl");
        let secret = "private prompt must never appear in diagnostics";
        let oversized = vec![b'x'; 10 * 1024 * 1024];
        let mut file = std::fs::File::create(&path)?;
        file.write_all(&oversized)?;
        file.write_all(b"\n")?;
        file.write_all(format!("{{\"prompt\":\"{secret}\"\n").as_bytes())?;
        file.write_all("{\"unicode\":\"界\"}\n".as_bytes())?;
        let durable_offset = file.stream_position()?;
        file.write_all("{\"partial\":\"界\"}".as_bytes())?;
        drop(file);

        let cases: [(SourceKind, ContractParser); 5] = [
            (SourceKind::Codex, codex::bounded_contract_parse),
            (SourceKind::Claude, claude::bounded_contract_parse),
            (SourceKind::KimiCode, kimi_code::bounded_contract_parse),
            (SourceKind::Pi, pi::bounded_contract_parse),
            (SourceKind::Grok, grok::bounded_contract_parse),
        ];

        for (source, parse) in cases {
            let (issues, end_offset, cancelled) = parse(&path)?;
            assert!(!cancelled, "{source} unexpectedly cancelled");
            assert_eq!(end_offset, durable_offset, "{source} cursor boundary");
            assert_eq!(issues.malformed_lines, 1, "{source} malformed count");
            assert_eq!(issues.oversized_lines, 1, "{source} oversized count");
            assert_eq!(issues.samples.len(), 2, "{source} bounded samples");
            assert!(issues.samples.iter().all(|sample| sample.source == source));
            assert!(
                issues
                    .samples
                    .iter()
                    .all(|sample| sample.path_hash == "bounded-contract-path-hash")
            );
            assert!(!serde_json::to_string(&issues)?.contains(secret));
        }
        Ok(())
    }
}
