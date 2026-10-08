//! Passive CLI and IDE SQLite readers, with one atomic replay group per product.
//!
//! Native ModelUsageStats uses #1 as a model enum, #2 fresh input, #4 cache
//! write, #5 cache read, #9 thinking and #10 visible output. The decoder reads
//! only usage/trajectory metadata and keeps SQLite WAL snapshots consistent.
mod decode;
#[cfg(test)]
mod tests;

use crate::{
    domain::source_diagnostics::{SourceIssue, SourceIssueCode, SourceIssues},
    models::{ParseIssueKind, ParseIssues, SourceKind},
    parsers::{ProgressSink, SourceParser, SourceSyncStats, SyncEvent, source_files},
    store::{FileCursor, Store, SyncRunWriter, SyncShard},
    util::hash_string,
};
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    pin::Pin,
    time::{Instant, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

const FAMILY: [SourceKind; 2] = [SourceKind::Antigravity, SourceKind::AntigravityIde];
pub struct AntigravityParser;
pub struct AntigravityIdeParser;

macro_rules! parser {
    ($parser:ty, $source:expr) => {
        impl SourceParser for $parser {
            fn source(&self) -> SourceKind {
                $source
            }
            fn parse<'a>(
                &'a self,
                store: &'a Store,
                writer: &'a mut SyncRunWriter,
                parallelism: usize,
                recent_cutoff: Option<DateTime<Utc>>,
                cancel: &'a CancellationToken,
                progress: Option<ProgressSink<'a>>,
            ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>> {
                Box::pin(async move {
                    let mut result = sync_antigravity_family(
                        store,
                        writer,
                        &[$source],
                        false,
                        false,
                        parallelism,
                        recent_cutoff,
                        cancel,
                        progress,
                    )
                    .await?;
                    Ok(result.stats.remove(0))
                })
            }
        }
    };
}
parser!(AntigravityParser, SourceKind::Antigravity);
parser!(AntigravityIdeParser, SourceKind::AntigravityIde);

pub(super) fn group_hash(source: SourceKind) -> String {
    hash_string(&format!("antigravity-snapshot:{source}"))
}

struct FamilyFile {
    path: PathBuf,
    root_source: SourceKind,
    cursor: FileCursor,
}

fn snapshot(path: &Path) -> Result<FileCursor> {
    let metadata = std::fs::metadata(path)?;
    let mtime = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)?
        .as_nanos()
        .min(i64::MAX as u128) as i64;
    let mut digest = Sha256::new();
    // WAL size/time plus its header and tail catch appended frames and reused
    // WAL generations even when the main database has not changed.
    for file_path in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.display())),
    ] {
        let mut file = match std::fs::File::open(&file_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && file_path != path => {
                digest.update(b"no-wal");
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let meta = file.metadata()?;
        // SQLite may create or touch a zero-byte WAL when opening a WAL-mode
        // database read-only. It contains no frames and is equivalent to an
        // absent WAL; only a WAL with bytes can affect the usage snapshot.
        if file_path != path && meta.len() == 0 {
            digest.update(b"no-wal");
            continue;
        }
        digest.update(meta.len().to_le_bytes());
        digest.update(
            meta.modified()?
                .duration_since(UNIX_EPOCH)?
                .as_nanos()
                .to_le_bytes(),
        );
        let mut buffer = [0u8; 4096];
        let count = file.read(&mut buffer)?;
        digest.update(&buffer[..count]);
        if meta.len() > 4096 {
            file.seek(SeekFrom::End(-4096))?;
            let count = file.read(&mut buffer)?;
            digest.update(&buffer[..count]);
        }
    }
    let path = path.to_string_lossy().into_owned();
    Ok(FileCursor {
        cursor_key: path.clone(),
        file_path: path,
        file_fingerprint: digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        file_size: metadata.len(),
        file_mtime_ns: mtime,
        offset: metadata.len(),
        tail_signature: String::new(),
        last_total: None,
        last_model: None,
        updated_at: Utc::now().to_rfc3339(),
    })
}

fn record_failure(
    stats: &mut [SourceSyncStats],
    source_issues: &mut SourceIssues,
    source: SourceKind,
    code: SourceIssueCode,
    count: u64,
) {
    for stat in stats.iter_mut().filter(|stat| stat.source == source) {
        let issues = source_issues.entry(source).or_default();
        SourceIssue::record(issues, code, count);
        stat.last_error = Some(
            issues
                .iter()
                .map(|issue| issue.cli_line(source))
                .collect::<Vec<_>>()
                .join("; "),
        );
    }
}
pub(crate) struct FamilySyncResult {
    pub(crate) stats: Vec<SourceSyncStats>,
    pub(crate) source_issues: SourceIssues,
}

/// Read-only preflight coverage facts for one Antigravity product.
///
/// Discovered counts are partitioned across product roots by first appearance.
/// Missing and out-of-scope counts are derived purely from filesystem metadata
/// of tracked paths without decoding usage or writing cursors/inventory/markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravityProductCoverage {
    pub source: SourceKind,
    pub discovered_count: usize,
    pub new_files_count: usize,
    pub tracked_count: usize,
    pub stored_events: usize,
    pub missing_count: usize,
    pub out_of_scope_count: usize,
    pub unreadable_count: usize,
    pub discovery_incomplete: bool,
}

impl AntigravityProductCoverage {
    /// True when the product has recoverable loss (missing or out of scope)
    /// and has no unreadable files or incomplete discovery.
    pub(crate) fn can_prompt_for_loss(&self) -> bool {
        (self.missing_count > 0 || self.out_of_scope_count > 0)
            && self.unreadable_count == 0
            && !self.discovery_incomplete
    }
}

pub(crate) fn check_antigravity_coverage(
    store: &Store,
) -> Result<HashMap<SourceKind, AntigravityProductCoverage>> {
    check_antigravity_coverage_with_inputs(
        store,
        [
            (
                SourceKind::Antigravity,
                source_files::list_antigravity_conversation_files(),
            ),
            (
                SourceKind::AntigravityIde,
                source_files::list_antigravity_ide_conversation_files(),
            ),
        ],
        |path| std::fs::metadata(path),
    )
}

pub(crate) fn check_antigravity_coverage_with_inputs(
    store: &Store,
    listings: [(SourceKind, source_files::SourceFileListing); 2],
    metadata: impl Fn(&Path) -> std::io::Result<std::fs::Metadata>,
) -> Result<HashMap<SourceKind, AntigravityProductCoverage>> {
    let mut cursors: HashMap<SourceKind, HashMap<String, FileCursor>> = HashMap::new();
    let mut tracked: HashMap<SourceKind, HashSet<PathBuf>> = HashMap::new();
    for source in FAMILY {
        let normalized = store
            .cursors()
            .load_file_cursors(source, "local")?
            .into_values()
            .map(|mut cursor| {
                cursor.file_path = std::fs::canonicalize(&cursor.file_path)
                    .unwrap_or_else(|_| PathBuf::from(&cursor.file_path))
                    .to_string_lossy()
                    .into_owned();
                (cursor.file_path.clone(), cursor)
            })
            .collect();
        cursors.insert(source, normalized);
        let mut paths: HashSet<_> = store
            .source_files()
            .tracked_paths(source, "local")?
            .into_iter()
            .map(|path| std::fs::canonicalize(&path).unwrap_or_else(|_| PathBuf::from(path)))
            .collect();
        paths.extend(cursors[&source].keys().map(PathBuf::from));
        tracked.insert(source, paths);
    }

    let mut discovery_incomplete = false;
    for (_source, listing) in &listings {
        let root_failed = match metadata(&listing.root) {
            Ok(meta) => !meta.is_dir(),
            Err(error) => error.kind() != std::io::ErrorKind::NotFound,
        };
        if listing.error_summary().is_some() || root_failed {
            discovery_incomplete = true;
        }
    }

    let mut discovered = HashSet::new();
    let mut discovered_by_root: HashMap<SourceKind, Vec<PathBuf>> = HashMap::new();
    for (source, listing) in listings {
        for path in listing.paths {
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            if discovered.insert(path.clone()) {
                discovered_by_root.entry(source).or_default().push(path);
            }
        }
    }

    let mut result = HashMap::new();
    for source in FAMILY {
        let discovered_paths = discovered_by_root.remove(&source).unwrap_or_default();
        let discovered_count = discovered_paths.len();
        let source_tracked = &tracked[&source];
        let new_files_count = discovered_paths
            .iter()
            .filter(|path| !source_tracked.contains(*path))
            .count();
        let tracked_count = source_tracked.len();

        let mut missing_count = 0;
        let mut out_of_scope_count = 0;
        let mut unreadable_count = 0;

        for path in source_tracked
            .iter()
            .filter(|path| !discovered.contains(*path))
        {
            match metadata(path) {
                Ok(_) => out_of_scope_count += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing_count += 1,
                Err(_) => unreadable_count += 1,
            }
        }

        let stored_events = {
            let conn = store.open_connection()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM usage_event WHERE source = ?1",
                [source.as_str()],
                |row| row.get(0),
            )?;
            count.max(0) as usize
        };

        result.insert(
            source,
            AntigravityProductCoverage {
                source,
                discovered_count,
                new_files_count,
                tracked_count,
                stored_events,
                missing_count,
                out_of_scope_count,
                unreadable_count,
                discovery_incomplete,
            },
        );
    }

    Ok(result)
}

pub(crate) fn format_antigravity_notice(
    source: SourceKind,
    coverage: &AntigravityProductCoverage,
    is_interactive: bool,
    has_window: bool,
    user_chose_keep: bool,
) -> String {
    let mut gap_parts = Vec::new();
    if coverage.missing_count > 0 {
        gap_parts.push(format!(
            "{} tracked database(s) missing",
            coverage.missing_count
        ));
    }
    if coverage.out_of_scope_count > 0 {
        gap_parts.push(format!(
            "{} tracked input(s) out of scope",
            coverage.out_of_scope_count
        ));
    }
    let gap_text = if gap_parts.is_empty() {
        "tracked inputs incomplete".to_string()
    } else {
        gap_parts.join(", ")
    };

    let action = if has_window {
        if is_interactive {
            "run `llmusage sync` without `--recent-days` to choose recovery".to_string()
        } else {
            format!(
                "run `llmusage sync --rebuild --source {} --allow-lossy-rebuild` (do not pass `--recent-days`) to repair",
                source.as_str()
            )
        }
    } else if user_chose_keep {
        "run `llmusage sync` again and accept loss to repair".to_string()
    } else {
        format!(
            "run `llmusage sync --rebuild --source {} --allow-lossy-rebuild` to repair",
            source.as_str()
        )
    };

    format!("history preserved; {gap_text}; {action}")
}

/// Notice for a gap that this rebuild would still refuse.
///
/// Unreadable files and incomplete discovery must not offer `--allow-lossy-rebuild`.
pub(crate) fn format_antigravity_blocked_notice(coverage: &AntigravityProductCoverage) -> String {
    let mut parts = Vec::new();
    if coverage.unreadable_count > 0 {
        parts.push(format!(
            "{} tracked database(s) unreadable",
            coverage.unreadable_count
        ));
    }
    if coverage.discovery_incomplete {
        parts.push("discovery incomplete".to_string());
    }
    if coverage.missing_count > 0 {
        parts.push(format!(
            "{} tracked database(s) missing",
            coverage.missing_count
        ));
    }
    if coverage.out_of_scope_count > 0 {
        parts.push(format!(
            "{} tracked input(s) out of scope",
            coverage.out_of_scope_count
        ));
    }
    let detail = if parts.is_empty() {
        "source blocked".to_string()
    } else {
        parts.join(", ")
    };
    format!("group writes skipped; {detail}; restore access and retry sync")
}

pub(crate) fn format_prompt_question(
    source: SourceKind,
    coverage: &AntigravityProductCoverage,
) -> String {
    let mut parts = Vec::new();
    if coverage.missing_count > 0 {
        parts.push(format!(
            "{} tracked database(s) are missing from disk ({} new file(s) found); rebuilding will discard events from missing files",
            coverage.missing_count, coverage.new_files_count
        ));
    }
    if coverage.out_of_scope_count > 0 {
        parts.push(format!(
            "{} tracked input(s) exist outside current discovery coverage; rebuilding will only keep currently parseable files",
            coverage.out_of_scope_count
        ));
    }
    format!(
        "\nSource '{}' has {} preserved events.\n{}\nAccept data loss and rebuild now? [k]eep / [r]ebuild (default: keep): ",
        source.as_str(),
        coverage.stored_events,
        parts.join("\n")
    )
}

pub(crate) fn prompt_recovery_choice<R: std::io::BufRead, W: std::io::Write>(
    reader: &mut R,
    writer: &mut W,
    source: SourceKind,
    coverage: &AntigravityProductCoverage,
) -> crate::sync::types::AntigravityRecoveryChoice {
    let question = format_prompt_question(source, coverage);
    let _ = write!(writer, "{question}");
    let _ = writer.flush();

    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return crate::sync::types::AntigravityRecoveryChoice::Keep;
    }
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("k") {
        return crate::sync::types::AntigravityRecoveryChoice::Keep;
    }
    if trimmed.eq_ignore_ascii_case("r") {
        return crate::sync::types::AntigravityRecoveryChoice::AcceptLoss;
    }

    // Invalid input: prompt once more
    let _ = write!(
        writer,
        "Please enter 'k' to keep or 'r' to rebuild (default: keep): "
    );
    let _ = writer.flush();
    line.clear();
    if reader.read_line(&mut line).is_err() {
        return crate::sync::types::AntigravityRecoveryChoice::Keep;
    }
    let trimmed = line.trim();
    if trimmed.eq_ignore_ascii_case("r") {
        crate::sync::types::AntigravityRecoveryChoice::AcceptLoss
    } else {
        crate::sync::types::AntigravityRecoveryChoice::Keep
    }
}

struct FamilyInputs {
    started: Instant,
    listings: [(SourceKind, source_files::SourceFileListing); 2],
    metadata: fn(&Path) -> std::io::Result<std::fs::Metadata>,
}

/// The driver invokes this once for the selected Antigravity family. Both
/// roots are inspected before filtering so copied artifacts retain ownership.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn sync_antigravity_family(
    store: &Store,
    writer: &mut SyncRunWriter,
    selected_sources: &[SourceKind],
    rebuild: bool,
    allow_lossy_rebuild: bool,
    _parallelism: usize,
    recent_cutoff: Option<DateTime<Utc>>,
    cancel: &CancellationToken,
    progress: Option<ProgressSink<'_>>,
) -> Result<FamilySyncResult> {
    let started = Instant::now();
    sync_family_with_inputs(
        store,
        writer,
        selected_sources,
        rebuild,
        allow_lossy_rebuild,
        recent_cutoff,
        cancel,
        progress,
        FamilyInputs {
            started,
            listings: [
                (
                    SourceKind::Antigravity,
                    source_files::list_antigravity_conversation_files(),
                ),
                (
                    SourceKind::AntigravityIde,
                    source_files::list_antigravity_ide_conversation_files(),
                ),
            ],
            metadata: |path| std::fs::metadata(path),
        },
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn sync_family_with_inputs(
    store: &Store,
    writer: &mut SyncRunWriter,
    selected_sources: &[SourceKind],
    rebuild: bool,
    allow_lossy_rebuild: bool,
    recent_cutoff: Option<DateTime<Utc>>,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
    inputs: FamilyInputs,
) -> Result<FamilySyncResult> {
    let started = inputs.started;
    let mut source_issues = SourceIssues::new();
    let mut stats: Vec<_> = selected_sources
        .iter()
        .map(|source| SourceSyncStats {
            source: *source,
            ..Default::default()
        })
        .collect();
    if selected_sources.is_empty() {
        return Ok(FamilySyncResult {
            stats,
            source_issues,
        });
    }
    let mut cursors: HashMap<SourceKind, HashMap<String, FileCursor>> = HashMap::new();
    let mut tracked: HashMap<SourceKind, HashSet<PathBuf>> = HashMap::new();
    for source in FAMILY {
        let normalized = store
            .cursors()
            .load_file_cursors(source, "local")?
            .into_values()
            .map(|mut cursor| {
                cursor.file_path = std::fs::canonicalize(&cursor.file_path)
                    .unwrap_or_else(|_| PathBuf::from(&cursor.file_path))
                    .to_string_lossy()
                    .into_owned();
                (cursor.file_path.clone(), cursor)
            })
            .collect();
        cursors.insert(source, normalized);
        let mut paths: HashSet<_> = store
            .source_files()
            .tracked_paths(source, "local")?
            .into_iter()
            .map(|path| std::fs::canonicalize(&path).unwrap_or_else(|_| PathBuf::from(path)))
            .collect();
        paths.extend(cursors[&source].keys().map(PathBuf::from));
        tracked.insert(source, paths);
    }
    let mut files = Vec::new();
    let mut discovered = HashSet::new();
    let mut discovered_by_root: HashMap<SourceKind, usize> = HashMap::new();
    let mut failed = HashSet::new();
    for (source, listing) in inputs.listings {
        // Shared discovery treats a missing root as empty. Check metadata here
        // because Path::exists also hides access errors and non-directory roots.
        let root_failed = match (inputs.metadata)(&listing.root) {
            Ok(metadata) => !metadata.is_dir(),
            Err(error) => error.kind() != std::io::ErrorKind::NotFound,
        };
        if listing.error_summary().is_some() || root_failed {
            // An unseen database can belong to either product or carry a
            // stronger identity. Root names do not prove product ownership.
            for owner in FAMILY {
                record_failure(
                    &mut stats,
                    &mut source_issues,
                    owner,
                    SourceIssueCode::DiscoveryIncomplete,
                    1,
                );
                failed.insert(owner);
            }
        }
        for path in listing.paths {
            if !discovered.insert(path.clone()) {
                continue;
            }
            *discovered_by_root.entry(source).or_default() += 1;
            match snapshot(&path) {
                Ok(cursor) => files.push(FamilyFile {
                    path,
                    root_source: source,
                    cursor,
                }),
                Err(_) => {
                    for owner in failed_file_owners(&path, source, &tracked) {
                        record_failure(
                            &mut stats,
                            &mut source_issues,
                            owner,
                            SourceIssueCode::FingerprintUnavailable,
                            1,
                        );
                        failed.insert(owner);
                    }
                }
            }
        }
    }
    // Only local membership is loaded above. A path absent from discovery may
    // still exist outside the current roots or supported file extensions.
    for source in FAMILY {
        for path in tracked[&source]
            .iter()
            .filter(|path| !discovered.contains(*path))
        {
            let code = match (inputs.metadata)(path) {
                Ok(_) => SourceIssueCode::TrackedMemberOutOfScope,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    SourceIssueCode::TrackedMemberMissing
                }
                Err(_) => SourceIssueCode::TrackedMemberUnreadable,
            };
            let accepted_loss = rebuild
                && allow_lossy_rebuild
                && selected_sources.contains(&source)
                && matches!(
                    code,
                    SourceIssueCode::TrackedMemberMissing
                        | SourceIssueCode::TrackedMemberOutOfScope
                );
            if !accepted_loss {
                record_failure(&mut stats, &mut source_issues, source, code, 1);
                failed.insert(source);
            }
        }
    }
    let any_changed = rebuild
        || recent_cutoff.is_some()
        || files.iter().any(|file| {
            !cursors.values().any(|map| {
                map.get(&file.cursor.file_path)
                    .is_some_and(|old| old.file_fingerprint == file.cursor.file_fingerprint)
            })
        });
    if !any_changed && failed.is_empty() {
        for stat in &mut stats {
            let paths: Vec<String> = cursors[&stat.source].keys().cloned().collect();
            store.source_files().mark_inventory_seen(
                stat.source,
                "local",
                &paths,
                writer.run_started_at(),
            )?;
            stat.files_processed = paths.len();
            stat.skipped_files = paths.len();
        }
        return Ok(FamilySyncResult {
            stats,
            source_issues,
        });
    }
    if cancel.is_cancelled() {
        return Ok(FamilySyncResult {
            stats,
            source_issues,
        });
    }

    for source in selected_sources {
        if let Some(sink) = progress.as_mut() {
            sink(SyncEvent::SourceStarted {
                source: *source,
                files_total: discovered.len() as u64,
            });
        }
    }
    if selected_sources
        .iter()
        .all(|source| failed.contains(source))
    {
        let elapsed = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        for stat in &mut stats {
            stat.parse_ms = elapsed;
            stat.files_processed = discovered_by_root.get(&stat.source).copied().unwrap_or(0);
            stat.changed_files = 0;
            stat.bytes_scanned = 0;
            stat.write_ms = 0;
            stat.events_seen = 0;
            stat.events_replayed = 0;
            stat.events_inserted = 0;
        }
        return Ok(FamilySyncResult {
            stats,
            source_issues,
        });
    }
    let mut decoded = Vec::new();
    let mut new_cursors: HashMap<SourceKind, Vec<FileCursor>> = HashMap::new();
    let mut issues: HashMap<SourceKind, ParseIssues> = HashMap::new();
    // Do blocking SQLite reads off the async executor, one consistent transaction
    // per DB; no store mutation occurs until every selected snapshot is staged.
    for mut file in files {
        if cancel.is_cancelled() {
            return Ok(FamilySyncResult {
                stats,
                source_issues,
            });
        }
        // Discovery can precede this read by many databases. Capture this
        // member immediately before its transaction, not at inventory time.
        match snapshot(&file.path) {
            Ok(before) => file.cursor = before,
            Err(_) => {
                for owner in failed_file_owners(&file.path, file.root_source, &tracked) {
                    failed.insert(owner);
                    record_failure(
                        &mut stats,
                        &mut source_issues,
                        owner,
                        SourceIssueCode::FingerprintUnavailable,
                        1,
                    );
                }
                continue;
            }
        }
        let path = file.path.clone();
        let read_cancel = cancel.clone();
        let root_source = file.root_source;
        let parsed = tokio::task::spawn_blocking(move || {
            decode::read_file(&path, root_source, &read_cancel)
        })
        .await?;
        if cancel.is_cancelled() {
            return Ok(FamilySyncResult {
                stats,
                source_issues,
            });
        }
        match parsed {
            Ok(parsed) => {
                let fingerprint_error = match snapshot(&file.path) {
                    Ok(after) if after.file_fingerprint == file.cursor.file_fingerprint => None,
                    Ok(_) => Some(SourceIssueCode::SnapshotChanged),
                    Err(error) => {
                        tracing::warn!(error = %error, path_hash = hash_string(&file.path.to_string_lossy()), "Antigravity post-read fingerprint unavailable");
                        Some(SourceIssueCode::FingerprintUnavailable)
                    }
                };
                if let Some(code) = fingerprint_error {
                    for owner in failed_file_owners(&file.path, parsed.source, &tracked) {
                        failed.insert(owner);
                        record_failure(&mut stats, &mut source_issues, owner, code, 1);
                    }
                    continue;
                }
                if parsed.root_attribution {
                    issues.entry(parsed.source).or_default().record(
                        parsed.source,
                        "",
                        0,
                        ParseIssueKind::AccountingAnomaly,
                        "product_metadata_absent_root_attribution",
                    );
                }
                for stat in stats.iter_mut().filter(|stat| stat.source == parsed.source) {
                    stat.files_processed += 1;
                    stat.changed_files += 1;
                    stat.bytes_scanned += file.cursor.file_size;
                }
                decoded.extend(parsed.observations);
                new_cursors
                    .entry(parsed.source)
                    .or_default()
                    .push(file.cursor);
            }
            Err(error) => {
                let record_error = error.downcast_ref::<decode::RecordFailure>();
                let source = record_error.map_or(root_source, |error| error.source_kind);
                let code = if let Some(error) = record_error {
                    issues.entry(source).or_default().record(
                        source,
                        &error.path_hash,
                        0,
                        ParseIssueKind::Malformed,
                        error.reason,
                    );
                    SourceIssueCode::IncompleteSnapshot
                } else {
                    SourceIssueCode::MetadataUnreadable
                };
                for owner in failed_file_owners(&file.path, source, &tracked) {
                    failed.insert(owner);
                    record_failure(&mut stats, &mut source_issues, owner, code, 1);
                }
            }
        }
    }

    // Product metadata makes sibling groups independent. If a failed group had
    // any known copied identity in the successful group's history, the transfer
    // check below blocks that write as well.
    let events = match decode::normalize(decoded, &mut issues) {
        Ok(events) => events,
        Err(_) => {
            for stat in &mut stats {
                stat.parse_issues
                    .merge(issues.remove(&stat.source).unwrap_or_default());
            }
            for source in selected_sources {
                record_failure(
                    &mut stats,
                    &mut source_issues,
                    *source,
                    SourceIssueCode::IncompleteSnapshot,
                    1,
                );
            }
            return Ok(FamilySyncResult {
                stats,
                source_issues,
            });
        }
    };
    let old_owners = if store.emit_only() {
        HashMap::new()
    } else {
        let connection = store.open_connection()?;
        let mut existing = connection.prepare(
            "SELECT event_key, source FROM usage_event WHERE host_id='local' AND source IN ('antigravity','antigravity_ide') AND COALESCE(source_path_hash,'')<>''")?;
        existing
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?
    };
    let mut transferred_sources = HashSet::new();
    for event in &events {
        let old = old_owners.get(&format!("local:{}", event.event_key));
        if let Some(old) = old
            && old != event.source.as_str()
        {
            if selected_sources.len() != 2 || recent_cutoff.is_some() || !failed.is_empty() {
                bail!(
                    "Antigravity request ownership changed; run an unfiltered full sync to transfer CLI/IDE history atomically"
                );
            }
            transferred_sources.extend(FAMILY);
        }
    }
    if recent_cutoff.is_some() {
        let current_keys: HashSet<_> = events
            .iter()
            .map(|event| format!("local:{}", event.event_key))
            .collect();
        if old_owners.iter().any(|(key, source)| {
            selected_sources
                .iter()
                .any(|selected| selected.as_str() == source)
                && !current_keys.contains(key)
        }) {
            bail!(
                "Antigravity request identities changed; run a full sync before importing a bounded window"
            );
        }
    }

    let mut shards = Vec::new();
    for stat in &mut stats {
        stat.parse_issues
            .merge(issues.remove(&stat.source).unwrap_or_default());
        if failed.contains(&stat.source) {
            continue;
        }
        let member_cursors = new_cursors.remove(&stat.source).unwrap_or_default();
        let old_keys: HashSet<_> = old_owners
            .iter()
            .filter(|(_, source)| *source == stat.source.as_str())
            .map(|(key, _)| key.clone())
            .collect();
        let current_keys: HashSet<_> = events
            .iter()
            .filter(|event| event.source == stat.source)
            .map(|event| format!("local:{}", event.event_key))
            .collect();
        let needs_replay = rebuild
            || recent_cutoff.is_some()
            || transferred_sources.contains(&stat.source)
            || old_keys != current_keys
            || member_cursors.len() != cursors[&stat.source].len()
            || member_cursors.iter().any(|cursor| {
                cursors[&stat.source]
                    .get(&cursor.file_path)
                    .is_none_or(|old| old.file_fingerprint != cursor.file_fingerprint)
            });
        if !needs_replay {
            let paths: Vec<_> = member_cursors
                .iter()
                .map(|cursor| cursor.file_path.clone())
                .collect();
            store.source_files().mark_inventory_seen(
                stat.source,
                "local",
                &paths,
                writer.run_started_at(),
            )?;
            continue;
        }
        let mut source_events: Vec<_> = events
            .iter()
            .filter(|event| event.source == stat.source)
            .cloned()
            .collect();
        stat.events_seen = source_events.len();
        if let Some(cutoff) = recent_cutoff.as_ref() {
            source_events.retain(|event| {
                crate::parsers::timestamp_in_recent_window(&event.event_at, Some(cutoff))
            });
            stat.events_seen = source_events.len();
        }
        let mut shard = SyncShard::new(stat.source);
        shard.events = source_events;
        // Bounded runs must still remember replay membership, without claiming
        // a full-history cursor. Otherwise the next full run could lose a file
        // that supplied bounded historical events and disappeared meanwhile.
        shard.seen_file_paths = member_cursors
            .iter()
            .map(|cursor| cursor.file_path.clone())
            .collect();
        if recent_cutoff.is_none() {
            shard.cursors = member_cursors;
            shard.reset_path_hashes.push(group_hash(stat.source));
            if !cursors[&stat.source].is_empty() {
                stat.events_replayed = stat.events_seen;
            }
        }
        shards.push(shard);
    }
    if cancel.is_cancelled() {
        return Ok(FamilySyncResult {
            stats,
            source_issues,
        });
    }
    if !shards.is_empty() {
        let sources: Vec<_> = shards.iter().map(|shard| shard.source).collect();
        let committed =
            writer.commit_antigravity_snapshot(shards, rebuild && recent_cutoff.is_none())?;
        for (source, committed) in sources.into_iter().zip(committed) {
            let stat = stats
                .iter_mut()
                .find(|stat| stat.source == source)
                .expect("selected source");
            stat.events_inserted = committed.events_inserted;
            stat.write_ms = committed.write_ms;
        }
    }
    let elapsed = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    for stat in &mut stats {
        stat.parse_ms = elapsed.saturating_sub(stat.write_ms);
    }
    Ok(FamilySyncResult {
        stats,
        source_issues,
    })
}

fn failed_file_owners(
    path: &Path,
    fallback: SourceKind,
    tracked: &HashMap<SourceKind, HashSet<PathBuf>>,
) -> Vec<SourceKind> {
    let known: Vec<_> = FAMILY
        .into_iter()
        .filter(|source| tracked[source].contains(path))
        .collect();
    if known.is_empty() {
        // An unreadable newly discovered DB could be a copy belonging to either
        // product. Do not certify another product's incomplete ownership scan.
        FAMILY.to_vec()
    } else {
        let mut owners = known;
        if !owners.contains(&fallback) {
            owners.push(fallback);
        }
        owners
    }
}
