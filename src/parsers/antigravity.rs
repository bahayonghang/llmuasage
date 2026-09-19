//! Passive CLI and IDE SQLite readers, with one atomic replay group per product.
//!
//! Native ModelUsageStats uses #1 as a model enum, #2 fresh input, #4 cache
//! write, #5 cache read, #9 thinking and #10 visible output. The decoder reads
//! only usage/trajectory metadata and keeps SQLite WAL snapshots consistent.
mod decode;
#[cfg(test)]
mod tests;

use crate::{
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
                    Ok(result.remove(0))
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

fn record_failure(stats: &mut [SourceSyncStats], source: SourceKind, reason: &str) {
    for stat in stats.iter_mut().filter(|stat| stat.source == source) {
        stat.last_error = Some(reason.to_owned());
        stat.parse_issues
            .record(source, "", 0, ParseIssueKind::Malformed, reason);
    }
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
    mut progress: Option<ProgressSink<'_>>,
) -> Result<Vec<SourceSyncStats>> {
    let started = Instant::now();
    let mut stats: Vec<_> = selected_sources
        .iter()
        .map(|source| SourceSyncStats {
            source: *source,
            ..Default::default()
        })
        .collect();
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
    let mut failed = HashSet::new();
    for (source, listing) in [
        (
            SourceKind::Antigravity,
            source_files::list_antigravity_conversation_files(),
        ),
        (
            SourceKind::AntigravityIde,
            source_files::list_antigravity_ide_conversation_files(),
        ),
    ] {
        if let Some(error) = listing.error_summary() {
            record_failure(&mut stats, source, &error);
            failed.insert(source);
        }
        for path in listing.paths {
            if !discovered.insert(path.clone()) {
                continue;
            }
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
                            owner,
                            "Antigravity database fingerprint failed; history preserved",
                        );
                        failed.insert(owner);
                    }
                }
            }
        }
    }
    // A missing tracked member cannot silently remove its historical events.
    for source in FAMILY {
        if tracked[&source]
            .iter()
            .any(|path| !discovered.contains(path))
            && !(rebuild && allow_lossy_rebuild && selected_sources.contains(&source))
        {
            record_failure(
                &mut stats,
                source,
                "Antigravity tracked database missing; history preserved (restore it or use explicit --rebuild --allow-lossy-rebuild)",
            );
            failed.insert(source);
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
        return Ok(stats);
    }
    if cancel.is_cancelled() {
        return Ok(stats);
    }

    let mut decoded = Vec::new();
    let mut new_cursors: HashMap<SourceKind, Vec<FileCursor>> = HashMap::new();
    let mut issues: HashMap<SourceKind, ParseIssues> = HashMap::new();
    for source in selected_sources {
        if let Some(sink) = progress.as_mut() {
            sink(SyncEvent::SourceStarted {
                source: *source,
                files_total: discovered.len() as u64,
            });
        }
    }
    // Do blocking SQLite reads off the async executor, one consistent transaction
    // per DB; no store mutation occurs until every selected snapshot is staged.
    for mut file in files {
        if cancel.is_cancelled() {
            return Ok(stats);
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
                        owner,
                        "Antigravity database fingerprint failed; history preserved",
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
            return Ok(stats);
        }
        match parsed {
            Ok(parsed) => {
                let fingerprint_error = match snapshot(&file.path) {
                    Ok(after) if after.file_fingerprint == file.cursor.file_fingerprint => None,
                    Ok(_) => Some("Antigravity database changed while reading; retry sync"),
                    Err(error) => {
                        tracing::warn!(error = %error, path_hash = hash_string(&file.path.to_string_lossy()), "Antigravity post-read fingerprint unavailable");
                        Some("Antigravity post-read fingerprint unavailable; history preserved")
                    }
                };
                if let Some(reason) = fingerprint_error {
                    for owner in failed_file_owners(&file.path, parsed.source, &tracked) {
                        failed.insert(owner);
                        record_failure(&mut stats, owner, reason);
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
            Err(_) => {
                for owner in failed_file_owners(&file.path, root_source, &tracked) {
                    failed.insert(owner);
                    record_failure(
                        &mut stats,
                        owner,
                        "Antigravity SQLite metadata unreadable or malformed; history preserved",
                    );
                }
            }
        }
    }

    // Product metadata makes sibling groups independent. If a failed group had
    // any known copied identity in the successful group's history, the transfer
    // check below blocks that write as well.
    let events = match decode::normalize(decoded, &mut issues) {
        Ok(events) => events,
        Err(error) => {
            for stat in &mut stats {
                stat.parse_issues
                    .merge(issues.remove(&stat.source).unwrap_or_default());
                stat.parse_issues.record(
                    stat.source,
                    "",
                    0,
                    ParseIssueKind::Malformed,
                    "incomplete_antigravity_usage_snapshot",
                );
                stat.last_error = Some(error.to_string());
            }
            return Ok(stats);
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
        return Ok(stats);
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
    Ok(stats)
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
