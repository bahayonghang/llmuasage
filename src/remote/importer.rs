use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, SecondsFormat, Utc};

use crate::{
    error::{LlmusageError, Result},
    models::SourceKind,
    parsers::SourceSyncStats,
    store::{
        Host, SourceSyncStatus, Store, SyncRunWriter, SyncShard, expected_token_accounting_version,
    },
    util::now_utc,
};

use super::protocol::{SHARD_PROTOCOL_VERSION, ShardDecoder, ShardRecord};
use super::transport::ShardSource;

pub const IMPORT_WATERMARK_OVERLAP_HOURS: i64 = 48;

#[derive(Debug, Clone)]
pub struct ImportOutcome {
    pub host_id: String,
    pub skipped_lines: u64,
    pub shards_committed: usize,
    pub warnings: Vec<String>,
    pub sources: Vec<SourceSyncStats>,
}

pub struct RemoteImporter;

impl RemoteImporter {
    pub fn import(
        host: &Host,
        store: &Store,
        writer: &mut SyncRunWriter,
        source: &dyn ShardSource,
    ) -> Result<ImportOutcome> {
        let since = since_from_watermark(host.import_watermark.as_deref());
        let mut session = source.open(host, since.as_deref())?;
        let mut max_event_at: Option<DateTime<Utc>> = None;
        let mut shards_committed = 0usize;
        let mut trailer_sources: Option<Vec<SourceSyncStats>> = None;
        let mut listed_sources: Option<BTreeMap<SourceKind, u32>> = None;
        let mut empty_sources = BTreeSet::new();
        let mut pi_awaiting_omp_reset = false;
        let skipped_lines;
        let saw_header;
        let saw_trailer;
        {
            let mut decoder = ShardDecoder::new(session.reader());
            let mut deferred_pi = Vec::new();
            let mut omp_split_migrated = store
                .meta_value(&format!("omp_split_migrated.{}", host.host_id))?
                .is_some();
            while let Some(record) = decoder.next_record()? {
                match record {
                    ShardRecord::Header {
                        source_accounting_versions,
                        ..
                    } => {
                        if listed_sources.is_some() {
                            continue;
                        }
                        let (parsed, empty, awaiting_reset) = validate_header_accounting(
                            host,
                            store,
                            &source_accounting_versions,
                            omp_split_migrated,
                        )?;
                        listed_sources = Some(parsed);
                        empty_sources = empty;
                        pi_awaiting_omp_reset = awaiting_reset;
                    }
                    ShardRecord::Shard { shard } => {
                        let listed = listed_sources.as_ref().ok_or_else(|| {
                            LlmusageError::ConfigInvalid {
                                detail: format!(
                                    "remote host {} emitted a shard before a validated header",
                                    host.host_id
                                ),
                            }
                        })?;
                        if !listed.contains_key(&shard.source) {
                            return Err(unlisted_shard_source_error(&host.host_id, shard.source));
                        }
                        let shard = bind_shard_to_host(shard, &host.host_id);
                        for event in &shard.events {
                            if let Ok(parsed) = DateTime::parse_from_rfc3339(&event.event_at) {
                                let utc = parsed.with_timezone(&Utc);
                                max_event_at =
                                    Some(max_event_at.map_or(utc, |current| current.max(utc)));
                            }
                        }
                        // First Omp commit resets pre-split Pi for this host.
                        // Defer Pi only when this stream listed Omp, otherwise a
                        // Pi-then-Omp stream would delete the post-split Pi rows.
                        if shard.source == SourceKind::Pi
                            && listed.contains_key(&SourceKind::Omp)
                            && !omp_split_migrated
                        {
                            deferred_pi.push(shard);
                            continue;
                        }
                        let shard_source = shard.source;
                        writer.commit_shard(shard)?;
                        shards_committed += 1;
                        if shard_source == SourceKind::Omp {
                            omp_split_migrated = true;
                            if pi_awaiting_omp_reset {
                                empty_sources.insert(SourceKind::Pi);
                                pi_awaiting_omp_reset = false;
                            }
                            for pi_shard in deferred_pi.drain(..) {
                                writer.commit_shard(pi_shard)?;
                                shards_committed += 1;
                            }
                        }
                    }
                    ShardRecord::Trailer { sources, .. } => {
                        trailer_sources = Some(sources);
                    }
                }
            }
            if pi_awaiting_omp_reset && !deferred_pi.is_empty() {
                return Err(historical_restore_error(
                    &host.host_id,
                    SourceKind::Pi,
                    store.token_accounting_version_for_host(&host.host_id, SourceKind::Pi)?,
                    expected_token_accounting_version(SourceKind::Pi),
                ));
            }
            for pi_shard in deferred_pi {
                writer.commit_shard(pi_shard)?;
                shards_committed += 1;
            }
            skipped_lines = decoder.skipped_lines();
            saw_header = decoder.saw_header();
            saw_trailer = decoder.saw_trailer();
        }
        let exit = session.finish()?;
        if !saw_header {
            return Err(LlmusageError::ConfigInvalid {
                detail: format!(
                    "remote shard stream from host {} had no header record: {}",
                    host.host_id,
                    exit.stderr.trim()
                ),
            });
        }
        let Some(sources) = trailer_sources else {
            return Err(LlmusageError::ConfigInvalid {
                detail: format!(
                    "remote shard stream from host {} ended without a trailer; imported events were kept but the host watermark was not advanced{}",
                    host.host_id,
                    format_stderr_suffix(&exit.stderr)
                ),
            });
        };
        if exit.status != 0 && !saw_trailer {
            return Err(LlmusageError::ConfigInvalid {
                detail: format!(
                    "remote sync --emit-shards exited {} without a trailer: {}",
                    exit.status,
                    exit.stderr.trim()
                ),
            });
        }

        let statuses = sources.iter().map(status_from_stats).collect::<Vec<_>>();
        store
            .sync_status()
            .save_source_sync_statuses(&host.host_id, &statuses)?;
        store.hosts().record_contact(&host.host_id, None)?;
        if since.is_none()
            && let Some(listed) = listed_sources.as_ref()
        {
            establish_host_source_markers(store, &host.host_id, listed, &empty_sources, &sources)?;
        }
        if let Some(watermark) = max_event_at {
            store.hosts().set_watermark(
                &host.host_id,
                Some(&watermark.to_rfc3339_opts(SecondsFormat::Secs, true)),
            )?;
        }

        let mut warnings = Vec::new();
        if skipped_lines > 0 {
            let warning = format!(
                "skipped {skipped_lines} non-JSON shard lines from host {}",
                host.label
            );
            tracing::warn!(
                host_id = %host.host_id,
                skipped_lines,
                "skipped non-JSON remote shard lines"
            );
            warnings.push(warning);
        }

        Ok(ImportOutcome {
            host_id: host.host_id.clone(),
            skipped_lines,
            shards_committed,
            warnings,
            sources,
        })
    }
}

fn bind_shard_to_host(mut shard: SyncShard, host_id: &str) -> SyncShard {
    shard.host_id = host_id.to_string();
    shard.host_prefix_applied = false;
    shard
}

fn since_from_watermark(watermark: Option<&str>) -> Option<String> {
    let watermark = watermark?;
    let parsed = DateTime::parse_from_rfc3339(watermark).ok()?;
    let cutoff = parsed.with_timezone(&Utc) - Duration::hours(IMPORT_WATERMARK_OVERLAP_HOURS);
    Some(cutoff.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn status_from_stats(stats: &SourceSyncStats) -> SourceSyncStatus {
    SourceSyncStatus {
        source: stats.source.as_str().to_string(),
        files_processed: stats.files_processed as i64,
        changed_files: stats.changed_files as i64,
        bytes_scanned: stats.bytes_scanned as i64,
        events_seen: stats.events_seen as i64,
        events_replayed: stats.events_replayed as i64,
        events_inserted: stats.events_inserted as i64,
        stored_events: stats.stored_events as i64,
        token_accounting_version: None,
        legacy_token_accounting: false,
        token_accounting_warning: None,
        parse_ms: stats.parse_ms as i64,
        write_ms: stats.write_ms as i64,
        lock_wait_ms: stats.lock_wait_ms as i64,
        parse_issues: stats.parse_issues.clone(),
        updated_at: now_utc(),
    }
}

fn validate_header_accounting(
    host: &Host,
    store: &Store,
    versions: &BTreeMap<String, u32>,
    omp_split_migrated: bool,
) -> Result<(BTreeMap<SourceKind, u32>, BTreeSet<SourceKind>, bool)> {
    if versions.is_empty() {
        return Err(LlmusageError::ConfigInvalid {
            detail: format!(
                "remote host {} shard header is missing source token-accounting versions; \
                 upgrade llmusage on that host so both sides share shard_protocol {SHARD_PROTOCOL_VERSION} \
                 and the header lists each source's token_accounting_version",
                host.host_id
            ),
        });
    }

    let mut parsed = BTreeMap::new();
    for (name, version) in versions {
        let Some(source) = SourceKind::parse_id(name) else {
            return Err(LlmusageError::ConfigInvalid {
                detail: format!(
                    "remote host {} shard header lists unknown source {name}; \
                     upgrade llmusage so both sides share shard_protocol {SHARD_PROTOCOL_VERSION}",
                    host.host_id
                ),
            });
        };
        let expected = expected_token_accounting_version(source);
        if *version != expected {
            return Err(accounting_mismatch_error(
                &host.host_id,
                source,
                Some(*version),
                expected,
            ));
        }
        parsed.insert(source, *version);
    }

    let omp_split_pending = parsed.contains_key(&SourceKind::Omp) && !omp_split_migrated;
    let mut empty_sources = BTreeSet::new();
    let mut pi_awaiting_omp_reset = false;
    for source in parsed.keys().copied() {
        let rows = store.host_source_event_count(&host.host_id, source)?;
        if source == SourceKind::Pi && omp_split_pending {
            if rows == 0 {
                empty_sources.insert(source);
            } else {
                // First Omp shard will reset these rows. Do not certify Pi
                // until that reset commits; do not mix new Pi in before it.
                pi_awaiting_omp_reset = true;
            }
            continue;
        }
        if rows == 0 {
            empty_sources.insert(source);
            continue;
        }
        let marker = store.token_accounting_version_for_host(&host.host_id, source)?;
        let expected = expected_token_accounting_version(source);
        if marker != Some(expected) {
            return Err(historical_restore_error(
                &host.host_id,
                source,
                marker,
                expected,
            ));
        }
    }
    Ok((parsed, empty_sources, pi_awaiting_omp_reset))
}

fn establish_host_source_markers(
    store: &Store,
    host_id: &str,
    listed: &BTreeMap<SourceKind, u32>,
    empty_sources: &BTreeSet<SourceKind>,
    trailer_sources: &[SourceSyncStats],
) -> Result<()> {
    for source in listed.keys().copied() {
        if !empty_sources.contains(&source) {
            continue;
        }
        let Some(stats) = trailer_sources.iter().find(|stats| stats.source == source) else {
            continue;
        };
        if stats.parse_issues.total() > 0 || stats.last_error.is_some() {
            continue;
        }
        store.mark_current_token_accounting_for_host(host_id, source)?;
    }
    Ok(())
}

fn accounting_mismatch_error(
    host_id: &str,
    source: SourceKind,
    remote_version: Option<u32>,
    expected: u32,
) -> LlmusageError {
    let remote =
        remote_version.map_or_else(|| "<missing>".to_string(), |version| version.to_string());
    LlmusageError::ConfigInvalid {
        detail: format!(
            "token accounting mismatch for host {host_id} source {}: local={expected} remote={remote}; \
             upgrade llmusage on that host so source {} reports token_accounting_version {expected} \
             in shard protocol {SHARD_PROTOCOL_VERSION}",
            source.as_str(),
            source.as_str()
        ),
    }
}

fn historical_restore_error(
    host_id: &str,
    source: SourceKind,
    marker: Option<u32>,
    expected: u32,
) -> LlmusageError {
    let historical = marker.map_or_else(|| "unknown".to_string(), |version| version.to_string());
    LlmusageError::ConfigInvalid {
        detail: format!(
            "remote host {host_id} source {} already has imported rows without a trusted \
             current token-accounting marker (historical={historical}, current={expected}); \
             a full restore is required (not implemented); existing events and watermarks were left unchanged",
            source.as_str()
        ),
    }
}

fn unlisted_shard_source_error(host_id: &str, source: SourceKind) -> LlmusageError {
    LlmusageError::ConfigInvalid {
        detail: format!(
            "remote host {host_id} shard source {} is not listed in the header source_accounting_versions map; \
             upgrade llmusage on that host so the header lists every shard source and its token_accounting_version",
            source.as_str()
        ),
    }
}

fn format_stderr_suffix(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!(": {trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{ParseIssues, SourceKind, UsageEvent, UsageTokens},
        paths::AppPaths,
        remote::protocol::{ShardRecord, encode_record, source_accounting_versions},
        remote::transport::MemoryShardSource,
        store::{FileCursor, HolderKind, LOCAL_HOST_ID, Store},
    };
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn event(key: &str, at: &str) -> UsageEvent {
        event_for(SourceKind::Codex, key, at)
    }

    fn event_for(source: SourceKind, key: &str, at: &str) -> UsageEvent {
        UsageEvent {
            event_key: key.to_string(),
            source,
            provider_label: String::new(),
            model: "gpt-5".to_string(),
            event_at: at.to_string(),
            hour_start: at.to_string(),
            tokens: UsageTokens {
                input_tokens: 1,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 1,
                reasoning_output_tokens: 0,
                total_tokens: 2,
            },
            project: None,
            session: None,
            source_cost: None,
        }
    }

    fn ssh_host(watermark: Option<&str>) -> Host {
        Host {
            host_id: "devbox".to_string(),
            label: "devbox".to_string(),
            transport: "ssh".to_string(),
            ssh_target: Some("me@devbox".to_string()),
            command: "llmusage".to_string(),
            added_at: "2026-08-20T00:00:00Z".to_string(),
            last_contacted_at: None,
            last_error: None,
            import_watermark: watermark.map(str::to_string),
        }
    }

    fn stream(records: &[ShardRecord], prefix: &str) -> String {
        let mut out = String::from(prefix);
        for record in records {
            out.push_str(&encode_record(record).expect("record"));
            out.push('\n');
        }
        out
    }

    fn header(sources: &[SourceKind]) -> ShardRecord {
        ShardRecord::header(
            "2026-08-20T02:00:00Z",
            source_accounting_versions(sources.iter().copied()),
        )
    }

    fn trailer(sources: Vec<SourceSyncStats>) -> ShardRecord {
        ShardRecord::Trailer {
            sources,
            parse_issues: ParseIssues::default(),
        }
    }

    fn source_stats(source: SourceKind, events: usize) -> SourceSyncStats {
        SourceSyncStats {
            source,
            events_seen: events,
            events_inserted: events,
            ..SourceSyncStats::default()
        }
    }

    fn fenced_store() -> anyhow::Result<(TempDir, Store, crate::store::WorkerLock)> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        let lock =
            store.acquire_worker_lock_with(std::time::Duration::from_secs(5), HolderKind::Cli)?;
        let fenced = lock.fenced_store();
        fenced.bootstrap()?;
        Ok((temp, fenced, lock))
    }

    #[test]
    fn motd_is_skipped_and_counted_as_a_warning() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard
            .events
            .push(event("codex:a:1", "2026-08-20T01:00:00Z"));
        let stdout = stream(
            &[
                header(&[SourceKind::Codex]),
                ShardRecord::Shard { shard },
                trailer(vec![source_stats(SourceKind::Codex, 1)]),
            ],
            "Welcome to devbox\n",
        );
        let source = MemoryShardSource {
            stdout,
            stderr: String::new(),
            status: 0,
        };
        let mut writer = store.begin_sync_run()?;
        let outcome = RemoteImporter::import(&host, &store, &mut writer, &source)?;
        assert_eq!(outcome.skipped_lines, 1);
        assert!(
            outcome
                .warnings
                .iter()
                .any(|warning| warning.contains("skipped 1")),
            "{:?}",
            outcome.warnings
        );
        let refreshed = store.hosts().get_by_label("devbox")?.expect("host");
        assert_eq!(
            refreshed.import_watermark.as_deref(),
            Some("2026-08-20T01:00:00Z")
        );
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            Some(crate::store::expected_token_accounting_version(
                SourceKind::Codex
            ))
        );
        Ok(())
    }

    #[test]
    fn missing_trailer_keeps_shards_and_does_not_advance_watermark() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard
            .events
            .push(event("codex:a:1", "2026-08-20T01:00:00Z"));
        shard.cursors.push(FileCursor {
            cursor_key: "/same/path.jsonl".to_string(),
            file_path: "/same/path.jsonl".to_string(),
            file_fingerprint: "fp".to_string(),
            file_size: 10,
            file_mtime_ns: 0,
            tail_signature: "tail".to_string(),
            offset: 10,
            last_total: None,
            last_model: None,
            updated_at: "2026-08-20T01:00:00Z".to_string(),
        });
        shard.seen_file_paths.push("/same/path.jsonl".to_string());
        let stdout = stream(
            &[header(&[SourceKind::Codex]), ShardRecord::Shard { shard }],
            "",
        );
        let source = MemoryShardSource {
            stdout,
            stderr: "killed".to_string(),
            status: 1,
        };
        let mut writer = store.begin_sync_run()?;
        let err = RemoteImporter::import(&host, &store, &mut writer, &source)
            .expect_err("missing trailer");
        assert!(err.to_string().contains("without a trailer"), "{err}");
        let conn = store.open_connection()?;
        let events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE host_id = 'devbox'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(events, 1);
        let refreshed = store.hosts().get_by_label("devbox")?.expect("host");
        assert!(refreshed.import_watermark.is_none());
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            None,
            "mid-stream failure must not establish a host/source marker"
        );
        Ok(())
    }

    #[test]
    fn same_path_on_two_hosts_keeps_independent_cursors() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        let mut local = SyncShard::new(SourceKind::Codex);
        local.seen_file_paths.push("/same/path.jsonl".to_string());
        local.cursors.push(FileCursor {
            cursor_key: "/same/path.jsonl".to_string(),
            file_path: "/same/path.jsonl".to_string(),
            file_fingerprint: "local-fp".to_string(),
            file_size: 4,
            file_mtime_ns: 1,
            tail_signature: "local".to_string(),
            offset: 4,
            last_total: None,
            last_model: None,
            updated_at: "2026-08-20T00:00:00Z".to_string(),
        });
        local
            .events
            .push(event("codex:local:1", "2026-08-20T00:00:00Z"));
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(local)?;

        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut remote_shard = SyncShard::new(SourceKind::Codex);
        remote_shard
            .seen_file_paths
            .push("/same/path.jsonl".to_string());
        remote_shard.cursors.push(FileCursor {
            cursor_key: "/same/path.jsonl".to_string(),
            file_path: "/same/path.jsonl".to_string(),
            file_fingerprint: "remote-fp".to_string(),
            file_size: 8,
            file_mtime_ns: 2,
            tail_signature: "remote".to_string(),
            offset: 8,
            last_total: None,
            last_model: None,
            updated_at: "2026-08-20T01:00:00Z".to_string(),
        });
        remote_shard
            .events
            .push(event("codex:remote:1", "2026-08-20T01:00:00Z"));
        let stdout = stream(
            &[
                header(&[SourceKind::Codex]),
                ShardRecord::Shard {
                    shard: remote_shard,
                },
                trailer(vec![SourceSyncStats::default()]),
            ],
            "",
        );
        let source = MemoryShardSource {
            stdout,
            stderr: String::new(),
            status: 0,
        };
        RemoteImporter::import(&host, &store, &mut writer, &source)?;

        let conn = store.open_connection()?;
        let files: i64 = conn.query_row(
            "SELECT COUNT(*) FROM source_file WHERE file_path = '/same/path.jsonl'",
            [],
            |row| row.get(0),
        )?;
        let cursors: i64 = conn.query_row(
            "SELECT COUNT(*) FROM source_cursor WHERE cursor_key = '/same/path.jsonl'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(files, 2);
        assert_eq!(cursors, 2);
        let local_fp: String = conn.query_row(
            "SELECT file_fingerprint FROM source_cursor WHERE host_id = ?1 AND cursor_key = '/same/path.jsonl'",
            [LOCAL_HOST_ID],
            |row| row.get(0),
        )?;
        let remote_fp: String = conn.query_row(
            "SELECT file_fingerprint FROM source_cursor WHERE host_id = 'devbox' AND cursor_key = '/same/path.jsonl'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(local_fp, "local-fp");
        assert_eq!(remote_fp, "remote-fp");
        Ok(())
    }

    #[test]
    fn since_from_watermark_subtracts_overlap() {
        let since = since_from_watermark(Some("2026-08-20T12:00:00Z")).expect("since");
        assert_eq!(since, "2026-08-18T12:00:00Z");
        assert!(since_from_watermark(None).is_none());
    }

    #[test]
    fn non_header_first_record_commits_no_shards() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard
            .events
            .push(event("codex:a:1", "2026-08-20T01:00:00Z"));
        let stdout = stream(&[ShardRecord::Shard { shard }], "banner\n");
        let source = MemoryShardSource {
            stdout,
            stderr: String::new(),
            status: 0,
        };
        let mut writer = store.begin_sync_run()?;
        let err = RemoteImporter::import(&host, &store, &mut writer, &source)
            .expect_err("header required");
        assert!(
            err.to_string()
                .contains("first deserialized shard record must be a header"),
            "{err}"
        );
        let conn = store.open_connection()?;
        let events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE host_id = 'devbox'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(events, 0);
        Ok(())
    }

    fn count_source(store: &Store, source: SourceKind, host_id: &str) -> anyhow::Result<i64> {
        let conn = store.open_connection()?;
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = ?1 AND host_id = ?2",
            [source.as_str(), host_id],
            |row| row.get(0),
        )?)
    }

    #[test]
    fn first_omp_shard_resets_host_pi_rows_once() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut pi_shard = SyncShard::new_for_host(SourceKind::Pi, "devbox");
        pi_shard
            .events
            .push(event_for(SourceKind::Pi, "pi:old", "2026-08-20T00:00:00Z"));
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(pi_shard)?;
        assert_eq!(count_source(&store, SourceKind::Pi, "devbox")?, 1);

        let mut omp_shard = SyncShard::new(SourceKind::Omp);
        omp_shard.events.push(event_for(
            SourceKind::Omp,
            "omp:new",
            "2026-08-20T01:00:00Z",
        ));
        let stdout = stream(
            &[
                header(&[SourceKind::Omp]),
                ShardRecord::Shard { shard: omp_shard },
                trailer(vec![source_stats(SourceKind::Omp, 1)]),
            ],
            "",
        );
        let source = MemoryShardSource {
            stdout,
            stderr: String::new(),
            status: 0,
        };
        RemoteImporter::import(&host, &store, &mut writer, &source)?;
        assert_eq!(count_source(&store, SourceKind::Pi, "devbox")?, 0);
        assert_eq!(count_source(&store, SourceKind::Omp, "devbox")?, 1);
        let flag = store.meta_value(&format!("omp_split_migrated.{}", host.host_id))?;
        assert!(flag.is_some(), "migration flag should be set");

        let mut extra_pi = SyncShard::new_for_host(SourceKind::Pi, "devbox");
        extra_pi.events.push(event_for(
            SourceKind::Pi,
            "pi:after-flag",
            "2026-08-20T03:00:00Z",
        ));
        writer.commit_shard(extra_pi)?;
        assert_eq!(count_source(&store, SourceKind::Pi, "devbox")?, 1);

        let mut second_omp = SyncShard::new(SourceKind::Omp);
        second_omp.events.push(event_for(
            SourceKind::Omp,
            "omp:second",
            "2026-08-20T04:00:00Z",
        ));
        let stdout = stream(
            &[
                header(&[SourceKind::Omp]),
                ShardRecord::Shard { shard: second_omp },
                trailer(vec![SourceSyncStats::default()]),
            ],
            "",
        );
        let source = MemoryShardSource {
            stdout,
            stderr: String::new(),
            status: 0,
        };
        RemoteImporter::import(&host, &store, &mut writer, &source)?;
        assert_eq!(
            count_source(&store, SourceKind::Pi, "devbox")?,
            1,
            "second omp shard must not reset pi again"
        );
        assert_eq!(count_source(&store, SourceKind::Omp, "devbox")?, 2);
        Ok(())
    }

    #[test]
    fn mixed_pi_omp_stream_keeps_post_split_pi_rows() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut old_pi = SyncShard::new_for_host(SourceKind::Pi, "devbox");
        old_pi
            .events
            .push(event_for(SourceKind::Pi, "pi:old", "2026-08-20T00:00:00Z"));
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(old_pi)?;
        assert_eq!(count_source(&store, SourceKind::Pi, "devbox")?, 1);

        let mut live_pi = SyncShard::new(SourceKind::Pi);
        live_pi
            .events
            .push(event_for(SourceKind::Pi, "pi:true", "2026-08-20T01:00:00Z"));
        let mut omp_shard = SyncShard::new(SourceKind::Omp);
        omp_shard.events.push(event_for(
            SourceKind::Omp,
            "omp:new",
            "2026-08-20T02:00:00Z",
        ));
        let stdout = stream(
            &[
                header(&[SourceKind::Pi, SourceKind::Omp]),
                ShardRecord::Shard { shard: live_pi },
                ShardRecord::Shard { shard: omp_shard },
                trailer(vec![
                    source_stats(SourceKind::Pi, 1),
                    source_stats(SourceKind::Omp, 1),
                ]),
            ],
            "",
        );
        let source = MemoryShardSource {
            stdout,
            stderr: String::new(),
            status: 0,
        };
        RemoteImporter::import(&host, &store, &mut writer, &source)?;
        assert_eq!(count_source(&store, SourceKind::Pi, "devbox")?, 1);
        assert_eq!(count_source(&store, SourceKind::Omp, "devbox")?, 1);
        let conn = store.open_connection()?;
        let pi_at: String = conn.query_row(
            "SELECT event_at FROM usage_event WHERE source = 'pi' AND host_id = 'devbox'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(pi_at, "2026-08-20T01:00:00Z");
        assert!(
            store
                .meta_value(&format!("omp_split_migrated.{}", host.host_id))?
                .is_some()
        );
        Ok(())
    }

    fn import_stream(store: &Store, host: &Host, stdout: String) -> Result<ImportOutcome> {
        let source = MemoryShardSource {
            stdout,
            stderr: String::new(),
            status: 0,
        };
        let mut writer = store.begin_sync_run()?;
        RemoteImporter::import(host, store, &mut writer, &source)
    }

    fn event_count(store: &Store, host_id: &str, source: SourceKind) -> anyhow::Result<i64> {
        count_source(store, source, host_id)
    }

    #[test]
    fn missing_accounting_versions_refuse_before_any_shard_commit() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard
            .events
            .push(event("codex:a:1", "2026-08-20T01:00:00Z"));
        let stdout = stream(
            &[
                ShardRecord::header("2026-08-20T02:00:00Z", BTreeMap::new()),
                ShardRecord::Shard { shard },
                trailer(vec![source_stats(SourceKind::Codex, 1)]),
            ],
            "",
        );
        let err = import_stream(&store, &host, stdout).expect_err("missing versions");
        let text = err.to_string();
        assert!(
            text.contains("codex") || text.contains("token-accounting"),
            "{text}"
        );
        assert!(text.contains("upgrade"), "{text}");
        assert_eq!(event_count(&store, "devbox", SourceKind::Codex)?, 0);
        let refreshed = store.hosts().get_by_label("devbox")?.expect("host");
        assert!(refreshed.import_watermark.is_none());
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            None
        );
        Ok(())
    }

    #[test]
    fn mismatched_accounting_version_refuse_before_any_shard_commit() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard
            .events
            .push(event("codex:a:1", "2026-08-20T01:00:00Z"));
        let mut versions = BTreeMap::new();
        versions.insert("codex".to_string(), 2);
        let stdout = stream(
            &[
                ShardRecord::header("2026-08-20T02:00:00Z", versions),
                ShardRecord::Shard { shard },
                trailer(vec![source_stats(SourceKind::Codex, 1)]),
            ],
            "",
        );
        let err = import_stream(&store, &host, stdout).expect_err("mismatch");
        let text = err.to_string();
        assert!(text.contains("codex"), "{text}");
        assert!(text.contains("upgrade"), "{text}");
        assert!(text.contains("local=3"), "{text}");
        assert!(text.contains("remote=2"), "{text}");
        assert_eq!(event_count(&store, "devbox", SourceKind::Codex)?, 0);
        let refreshed = store.hosts().get_by_label("devbox")?.expect("host");
        assert!(refreshed.import_watermark.is_none());
        Ok(())
    }

    #[test]
    fn unlisted_shard_source_is_refused_before_that_commit() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut shard = SyncShard::new(SourceKind::Claude);
        shard.events.push(event_for(
            SourceKind::Claude,
            "claude:a:1",
            "2026-08-20T01:00:00Z",
        ));
        let stdout = stream(
            &[
                header(&[SourceKind::Codex]),
                ShardRecord::Shard { shard },
                trailer(vec![source_stats(SourceKind::Claude, 1)]),
            ],
            "",
        );
        let err = import_stream(&store, &host, stdout).expect_err("unlisted");
        let text = err.to_string();
        assert!(text.contains("claude"), "{text}");
        assert!(text.contains("upgrade"), "{text}");
        assert_eq!(event_count(&store, "devbox", SourceKind::Claude)?, 0);
        Ok(())
    }

    #[test]
    fn existing_remote_rows_without_marker_refuse_current_incremental() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store
            .hosts()
            .upsert(&ssh_host(Some("2026-08-20T01:00:00Z")))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut existing = SyncShard::new_for_host(SourceKind::Codex, "devbox");
        existing
            .events
            .push(event("codex:old:1", "2026-08-20T00:00:00Z"));
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(existing)?;
        assert_eq!(event_count(&store, "devbox", SourceKind::Codex)?, 1);
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            None
        );

        let mut shard = SyncShard::new(SourceKind::Codex);
        shard
            .events
            .push(event("codex:new:1", "2026-08-20T03:00:00Z"));
        let stdout = stream(
            &[
                header(&[SourceKind::Codex]),
                ShardRecord::Shard { shard },
                trailer(vec![source_stats(SourceKind::Codex, 1)]),
            ],
            "",
        );
        let err = import_stream(&store, &host, stdout).expect_err("mix-in");
        let text = err.to_string();
        assert!(text.contains("codex"), "{text}");
        assert!(text.contains("full restore"), "{text}");
        assert_eq!(event_count(&store, "devbox", SourceKind::Codex)?, 1);
        let refreshed = store.hosts().get_by_label("devbox")?.expect("host");
        assert_eq!(
            refreshed.import_watermark.as_deref(),
            Some("2026-08-20T01:00:00Z")
        );
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            None
        );
        Ok(())
    }

    #[test]
    fn empty_source_no_since_establishes_marker_and_survives_reopen() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard
            .events
            .push(event("codex:a:1", "2026-08-20T01:00:00Z"));
        let stdout = stream(
            &[
                header(&[SourceKind::Codex]),
                ShardRecord::Shard { shard },
                trailer(vec![source_stats(SourceKind::Codex, 1)]),
            ],
            "",
        );
        import_stream(&store, &host, stdout)?;
        let expected = crate::store::expected_token_accounting_version(SourceKind::Codex);
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            Some(expected)
        );
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, None);

        let paths = store.paths.clone();
        drop(store);
        let reopened = Store::new(&paths)?;
        assert_eq!(
            reopened.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            Some(expected)
        );
        let loaded = reopened.sync_status().load_source_sync_statuses("devbox")?;
        let codex = loaded
            .iter()
            .find(|status| status.source == "codex")
            .expect("codex status");
        assert_eq!(codex.token_accounting_version, Some(expected));
        assert!(!codex.legacy_token_accounting);
        Ok(())
    }

    #[test]
    fn matching_version_replay_is_idempotent_and_isolated_across_hosts() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        store.hosts().upsert(&Host {
            host_id: "other".to_string(),
            label: "other".to_string(),
            transport: "ssh".to_string(),
            ssh_target: Some("me@other".to_string()),
            command: "llmusage".to_string(),
            added_at: "2026-08-20T00:00:00Z".to_string(),
            last_contacted_at: None,
            last_error: None,
            import_watermark: None,
        })?;
        store.mark_current_token_accounting(SourceKind::Codex)?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard
            .events
            .push(event("codex:a:1", "2026-08-20T01:00:00Z"));
        let stdout = stream(
            &[
                header(&[SourceKind::Codex]),
                ShardRecord::Shard {
                    shard: shard.clone(),
                },
                trailer(vec![source_stats(SourceKind::Codex, 1)]),
            ],
            "",
        );
        import_stream(&store, &host, stdout.clone())?;
        import_stream(&store, &host, stdout)?;
        assert_eq!(event_count(&store, "devbox", SourceKind::Codex)?, 1);
        let expected = crate::store::expected_token_accounting_version(SourceKind::Codex);
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            Some(expected)
        );
        assert_eq!(
            store.token_accounting_version_for_host("other", SourceKind::Codex)?,
            None
        );
        assert_eq!(
            store.token_accounting_version(SourceKind::Codex)?,
            Some(expected)
        );
        assert_eq!(event_count(&store, "other", SourceKind::Codex)?, 0);
        assert_eq!(event_count(&store, LOCAL_HOST_ID, SourceKind::Codex)?, 0);
        Ok(())
    }

    #[test]
    fn parse_error_trailer_does_not_establish_marker() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard
            .events
            .push(event("codex:a:1", "2026-08-20T01:00:00Z"));
        let mut stats = source_stats(SourceKind::Codex, 1);
        stats.parse_issues.malformed_lines = 1;
        let stdout = stream(
            &[
                header(&[SourceKind::Codex]),
                ShardRecord::Shard { shard },
                trailer(vec![stats]),
            ],
            "",
        );
        import_stream(&store, &host, stdout)?;
        assert_eq!(event_count(&store, "devbox", SourceKind::Codex)?, 1);
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            None
        );
        Ok(())
    }

    #[test]
    fn protocol_v1_header_refuses_before_any_shard_commit() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard
            .events
            .push(event("codex:a:1", "2026-08-20T01:00:00Z"));
        let stdout = format!(
            "{}\n{}\n{}\n",
            r#"{"kind":"header","shard_protocol":1,"llmusage_version":"1.2.0","schema_version":23,"emitted_at":"2026-08-20T00:00:00Z"}"#,
            encode_record(&ShardRecord::Shard { shard })?,
            encode_record(&trailer(vec![source_stats(SourceKind::Codex, 1)]))?,
        );
        let err = import_stream(&store, &host, stdout).expect_err("old protocol");
        let text = err.to_string();
        assert!(text.contains("remote=1"), "{text}");
        assert!(text.contains("upgrade"), "{text}");
        assert_eq!(event_count(&store, "devbox", SourceKind::Codex)?, 0);
        let refreshed = store.hosts().get_by_label("devbox")?.expect("host");
        assert!(refreshed.import_watermark.is_none());
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            None
        );
        Ok(())
    }

    #[test]
    fn since_set_does_not_establish_host_source_marker() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store
            .hosts()
            .upsert(&ssh_host(Some("2026-08-20T01:00:00Z")))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard
            .events
            .push(event("codex:a:1", "2026-08-20T03:00:00Z"));
        let stdout = stream(
            &[
                header(&[SourceKind::Codex]),
                ShardRecord::Shard { shard },
                trailer(vec![source_stats(SourceKind::Codex, 1)]),
            ],
            "",
        );
        import_stream(&store, &host, stdout)?;
        assert_eq!(event_count(&store, "devbox", SourceKind::Codex)?, 1);
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            None,
            "incremental since must not certify historical accounting"
        );
        Ok(())
    }

    #[test]
    fn matching_host_marker_allows_incremental_when_since_is_set() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut first = SyncShard::new(SourceKind::Codex);
        first
            .events
            .push(event("codex:a:1", "2026-08-20T01:00:00Z"));
        let first_stdout = stream(
            &[
                header(&[SourceKind::Codex]),
                ShardRecord::Shard { shard: first },
                trailer(vec![source_stats(SourceKind::Codex, 1)]),
            ],
            "",
        );
        import_stream(&store, &host, first_stdout)?;
        let expected = crate::store::expected_token_accounting_version(SourceKind::Codex);
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            Some(expected)
        );

        let host = store.hosts().get_by_label("devbox")?.expect("host");
        assert!(host.import_watermark.is_some());
        let mut second = SyncShard::new(SourceKind::Codex);
        second
            .events
            .push(event("codex:b:1", "2026-08-20T03:00:00Z"));
        let second_stdout = stream(
            &[
                header(&[SourceKind::Codex]),
                ShardRecord::Shard { shard: second },
                trailer(vec![source_stats(SourceKind::Codex, 1)]),
            ],
            "",
        );
        import_stream(&store, &host, second_stdout)?;
        assert_eq!(event_count(&store, "devbox", SourceKind::Codex)?, 2);
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Codex)?,
            Some(expected)
        );
        Ok(())
    }

    #[test]
    fn pi_without_omp_reset_does_not_mix_into_existing_unmarked_rows() -> anyhow::Result<()> {
        let (_temp, store, _lock) = fenced_store()?;
        store.hosts().upsert(&ssh_host(None))?;
        let host = store.hosts().get_by_label("devbox")?.expect("host");
        let mut old_pi = SyncShard::new_for_host(SourceKind::Pi, "devbox");
        old_pi
            .events
            .push(event_for(SourceKind::Pi, "pi:old", "2026-08-20T00:00:00Z"));
        let mut writer = store.begin_sync_run()?;
        writer.commit_shard(old_pi)?;
        drop(writer);
        assert_eq!(count_source(&store, SourceKind::Pi, "devbox")?, 1);

        let mut live_pi = SyncShard::new(SourceKind::Pi);
        live_pi
            .events
            .push(event_for(SourceKind::Pi, "pi:new", "2026-08-20T01:00:00Z"));
        let stdout = stream(
            &[
                header(&[SourceKind::Pi, SourceKind::Omp]),
                ShardRecord::Shard { shard: live_pi },
                trailer(vec![
                    source_stats(SourceKind::Pi, 1),
                    source_stats(SourceKind::Omp, 0),
                ]),
            ],
            "",
        );
        let err = import_stream(&store, &host, stdout).expect_err("mix-in");
        let text = err.to_string();
        assert!(text.contains("pi"), "{text}");
        assert!(text.contains("full restore"), "{text}");
        assert_eq!(count_source(&store, SourceKind::Pi, "devbox")?, 1);
        assert_eq!(count_source(&store, SourceKind::Omp, "devbox")?, 0);
        let conn = store.open_connection()?;
        let pi_at: String = conn.query_row(
            "SELECT event_at FROM usage_event WHERE source = 'pi' AND host_id = 'devbox'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(pi_at, "2026-08-20T00:00:00Z");
        assert_eq!(
            store.token_accounting_version_for_host("devbox", SourceKind::Pi)?,
            None
        );
        Ok(())
    }
}
