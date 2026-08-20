use chrono::{DateTime, Duration, SecondsFormat, Utc};

use crate::{
    error::{LlmusageError, Result},
    parsers::SourceSyncStats,
    store::{Host, SourceSyncStatus, Store, SyncRunWriter, SyncShard},
    util::now_utc,
};

use super::protocol::{ShardDecoder, ShardRecord};
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
        let skipped_lines;
        let saw_header;
        let saw_trailer;
        {
            let mut decoder = ShardDecoder::new(session.reader());
            while let Some(record) = decoder.next_record()? {
                match record {
                    ShardRecord::Header { .. } => {}
                    ShardRecord::Shard { shard } => {
                        let shard = bind_shard_to_host(shard, &host.host_id);
                        for event in &shard.events {
                            if let Ok(parsed) = DateTime::parse_from_rfc3339(&event.event_at) {
                                let utc = parsed.with_timezone(&Utc);
                                max_event_at =
                                    Some(max_event_at.map_or(utc, |current| current.max(utc)));
                            }
                        }
                        writer.commit_shard(shard)?;
                        shards_committed += 1;
                    }
                    ShardRecord::Trailer { sources, .. } => {
                        trailer_sources = Some(sources);
                    }
                }
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
        remote::protocol::{SHARD_PROTOCOL_VERSION, ShardRecord, encode_record},
        remote::transport::MemoryShardSource,
        store::{FileCursor, HolderKind, LOCAL_HOST_ID, Store},
    };
    use tempfile::TempDir;

    fn event(key: &str, at: &str) -> UsageEvent {
        UsageEvent {
            event_key: key.to_string(),
            source: SourceKind::Codex,
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
                ShardRecord::Header {
                    shard_protocol: SHARD_PROTOCOL_VERSION,
                    llmusage_version: "1.2.0".to_string(),
                    schema_version: 23,
                    emitted_at: "2026-08-20T02:00:00Z".to_string(),
                },
                ShardRecord::Shard { shard },
                ShardRecord::Trailer {
                    sources: vec![SourceSyncStats {
                        source: SourceKind::Codex,
                        events_seen: 1,
                        events_inserted: 1,
                        ..SourceSyncStats::default()
                    }],
                    parse_issues: ParseIssues::default(),
                },
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
            &[
                ShardRecord::Header {
                    shard_protocol: SHARD_PROTOCOL_VERSION,
                    llmusage_version: "1.2.0".to_string(),
                    schema_version: 23,
                    emitted_at: "2026-08-20T02:00:00Z".to_string(),
                },
                ShardRecord::Shard { shard },
            ],
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
                ShardRecord::Header {
                    shard_protocol: SHARD_PROTOCOL_VERSION,
                    llmusage_version: "1.2.0".to_string(),
                    schema_version: 23,
                    emitted_at: "2026-08-20T02:00:00Z".to_string(),
                },
                ShardRecord::Shard {
                    shard: remote_shard,
                },
                ShardRecord::Trailer {
                    sources: vec![SourceSyncStats::default()],
                    parse_issues: ParseIssues::default(),
                },
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
}
