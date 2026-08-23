use super::*;

#[derive(Debug)]
struct SyncStatusRow {
    source: String,
    files_processed: i64,
    changed_files: i64,
    events_seen: i64,
    events_inserted: i64,
    stored_events: i64,
    updated_at: String,
    last_error: Option<String>,
    parse_issues: ParseIssues,
}

fn load_sync_statuses_with_conn(
    conn: &Connection,
    filter: &QueryFilter,
) -> Result<Vec<SyncStatusRow>> {
    let source = filter.source.map(|source| source.as_str().to_string());
    let mut stmt = conn.prepare(
        r#"
        SELECT source, files_processed, changed_files, events_seen, events_inserted,
               stored_events, updated_at, parse_issues_json
        FROM source_sync_status
        WHERE (?1 IS NULL OR source = ?1)
        ORDER BY stored_events DESC, source ASC
        "#,
    )?;
    let rows = stmt.query_map([source], |row| {
        let parse_issues_raw = row.get::<_, String>(7)?;
        let parse_issues = serde_json::from_str(&parse_issues_raw).map_err(|source| {
            rusqlite::Error::FromSqlConversionFailure(7, Type::Text, Box::new(source))
        })?;
        Ok(SyncStatusRow {
            source: row.get(0)?,
            files_processed: row.get(1)?,
            changed_files: row.get(2)?,
            events_seen: row.get(3)?,
            events_inserted: row.get(4)?,
            stored_events: row.get(5)?,
            updated_at: row.get(6)?,
            last_error: None,
            parse_issues,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Loads `SourceDiagnostics` rows by joining the per-source state counts in
/// `source_file` with the recent/history completion timestamps in
/// `source_sync_status`. Rows show up for any source that appears in either
/// table, sorted by source identifier.
#[cfg(test)]
static DIAGNOSTICS_STAT_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_diagnostics_stat_counter() {
    DIAGNOSTICS_STAT_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn diagnostics_stat_calls() -> usize {
    DIAGNOSTICS_STAT_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn load_source_diagnostics(conn: &Connection) -> Result<Vec<SourceDiagnostics>> {
    // Pre-load all source_file paths to avoid N+1 queries in the main loop
    let mut file_paths_by_source: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT source, file_path FROM source_file")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (source, path) = row?;
            file_paths_by_source.entry(source).or_default().push(path);
        }
    }

    let missing_file_counts: std::collections::HashMap<String, u64> = file_paths_by_source
        .iter()
        .filter_map(|(source, paths)| {
            let missing = paths
                .iter()
                .filter(|path| {
                    #[cfg(test)]
                    DIAGNOSTICS_STAT_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    !std::path::Path::new(path).exists()
                })
                .count() as u64;
            (missing > 0).then_some((source.clone(), missing))
        })
        .collect();

    // `event_count` is maintained transactionally with usage_event writes, so
    // diagnostics can avoid scanning the full fact table on every dashboard load.
    // It is only needed for sources with missing files; querying all buckets when
    // every source file is live makes the common dashboard path needlessly scan
    // the entire aggregate projection.
    let mut event_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    if !missing_file_counts.is_empty() {
        let placeholders = std::iter::repeat_n("?", missing_file_counts.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT source, COALESCE(SUM(event_count), 0) FROM usage_bucket_30m WHERE source IN ({placeholders}) GROUP BY source"
        );
        let sources = missing_file_counts.keys().collect::<Vec<_>>();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sources), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?.max(0) as u64,
            ))
        })?;
        for row in rows {
            let (source, count) = row?;
            event_counts.insert(source, count);
        }
    }

    let mut stmt = conn.prepare(
        r#"
        WITH file_states AS (
            SELECT
                source,
                SUM(CASE state WHEN 'live' THEN 1 ELSE 0 END) AS live_files,
                SUM(CASE state WHEN 'missing' THEN 1 ELSE 0 END) AS missing_files,
                SUM(CASE state WHEN 'deleted_by_user' THEN 1 ELSE 0 END) AS deleted_files
            FROM source_file
            GROUP BY source
        ),
        sources AS (
            SELECT source FROM file_states
            UNION
            SELECT source FROM source_sync_status
        )
        SELECT
            s.source,
            COALESCE(fs.live_files, 0),
            COALESCE(fs.missing_files, 0),
            COALESCE(fs.deleted_files, 0),
            ss.recent_completed_at,
            ss.history_completed_at
        FROM sources s
        LEFT JOIN file_states fs ON fs.source = s.source
        LEFT JOIN source_sync_status ss ON ss.source = s.source
        ORDER BY s.source ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        let source = row.get::<_, String>(0)?;
        let missing_file_count = missing_file_counts.get(&source).copied().unwrap_or(0);
        let total_events = if missing_file_count > 0 {
            *event_counts.get(&source).unwrap_or(&0)
        } else {
            0
        };
        Ok(SourceDiagnostics {
            source,
            live_files: row.get::<_, Option<i64>>(1)?.unwrap_or_default().max(0) as u64,
            missing_files: row.get::<_, Option<i64>>(2)?.unwrap_or_default().max(0) as u64,
            deleted_files: row.get::<_, Option<i64>>(3)?.unwrap_or_default().max(0) as u64,
            missing_file_count,
            protected_event_count: if missing_file_count > 0 {
                total_events
            } else {
                0
            },
            lossy_rebuild_risk: missing_file_count > 0 && total_events > 0,
            recent_completed_at: row.get(4)?,
            history_completed_at: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Cursor freshness row used in health views.
#[derive(Debug, Clone, Serialize)]
pub struct CursorHealth {
    /// Source identifier.
    pub source: String,
    /// Cursor key within the source.
    pub cursor_key: String,
    /// Last cursor update time, if any.
    pub updated_at: Option<String>,
    /// Source-specific SQLite status field, mainly for OpenCode.
    pub sqlite_status: Option<String>,
}

/// Health payload combining cursors and recent failures.
#[derive(Debug, Clone, Serialize)]
pub struct HealthPayload {
    /// Cursor freshness/health rows.
    pub cursors: Vec<CursorHealth>,
    /// Recent non-success command runs.
    pub recent_failures: Vec<RunRecord>,
}

/// Compact health projection used by latency-sensitive live dashboard reads.
#[derive(Debug, Clone, Serialize)]
pub struct HealthSummaryPayload {
    /// Number of persisted cursors without serializing every cursor key.
    pub cursor_count: i64,
    /// Recent non-success command runs.
    pub recent_failures: Vec<RunRecord>,
}

/// Per-source archive state diagnostics, derived from the `source_file`
/// state machine (D15 / ADR 0006).
///
/// Field semantics:
/// - `live_files` — files seen by the most recent sync run for this source.
/// - `missing_files` — previously seen, not in the latest run.
/// - `deleted_files` — explicitly forgotten via the diagnostics forget entry.
/// - `recent_completed_at` — last run that finished its `recent_days` window.
///   `None` until `RecentReady` is wired in 4.4.
/// - `history_completed_at` — last run that drove cursors back to the earliest
///   file. `None` until full-history sweeps are tracked.
#[derive(Debug, Clone, Serialize)]
pub struct SourceDiagnostics {
    /// Stable source identifier such as `codex`, `kimi_code`, `pi`, or `grok`.
    pub source: String,
    /// Number of `source_file` rows currently in `live` state.
    pub live_files: u64,
    /// Number of `source_file` rows currently in `missing` state.
    pub missing_files: u64,
    /// Number of `source_file` rows currently in `deleted_by_user` state.
    pub deleted_files: u64,
    /// Number of tracked source files that are currently absent on disk.
    ///
    /// This is an immediate filesystem check used by the lossy rebuild guard;
    /// it can be non-zero before a normal sync has swept `state='missing'`.
    pub missing_file_count: u64,
    /// Number of imported usage rows that would be protected from a default
    /// lossy `sync --rebuild` because at least one source file is absent.
    pub protected_event_count: u64,
    /// True when the default `sync --rebuild` guard would refuse this source
    /// until files are restored or `--allow-lossy-rebuild` is passed.
    pub lossy_rebuild_risk: bool,
    /// Last RFC 3339 time the recent-window scan reached the cutoff.
    pub recent_completed_at: Option<String>,
    /// Last RFC 3339 time the history scan reached the earliest file.
    pub history_completed_at: Option<String>,
}

/// Top-level diagnostics payload returned by [`Dashboard::diagnostics`] and
/// embedded into `HomeOverviewPayload.archive` (F4.4 / F5.3).
///
/// `archive_root` is `paths.root_dir` as a display string (D28); ccr-ui keeps
/// the legacy field name but the value now points at the llmusage runtime
/// root rather than the old ccr-db archive root.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsPayload {
    /// Absolute path to the llmusage runtime root.
    pub archive_root: String,
    /// One row per source, ordered by source identifier.
    pub by_source: Vec<SourceDiagnostics>,
    /// Most recent failed usage-import records, including historical hook runs.
    pub recent_failures: Vec<RunRecord>,
}

/// Top dashboard sync command-center payload. It answers ordinary sync safety
/// separately from lossy rebuild risk and exposes only structured facts.
#[derive(Debug, Clone, Serialize)]
pub struct SyncCommandCenterPayload {
    pub mode: String,
    pub tone: String,
    pub headline_key: String,
    pub reason_key: String,
    pub generated_at: String,
    pub current_job: Option<SyncCurrentJobPayload>,
    pub last_run: Option<SyncLastRunPayload>,
    pub safety: SyncSafetyPayload,
    pub metrics: SyncMetricsPayload,
    pub sources: Vec<SyncSourcePayload>,
    pub actions: Vec<SyncActionPayload>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncCurrentJobPayload {
    pub job_id: String,
    pub status: String,
    pub last_event: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncLastRunPayload {
    pub status: String,
    pub command: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncSafetyPayload {
    pub ordinary_sync_safe: bool,
    pub worker_lock: String,
    pub worker_lock_holder: Option<String>,
    pub lossy_rebuild_risk: bool,
    pub risk_sources: Vec<String>,
    pub risk_details: Vec<SyncRiskSourcePayload>,
    pub recent_failures: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncRiskSourcePayload {
    pub source: String,
    pub missing_file_count: u64,
    pub protected_event_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncMetricsPayload {
    pub events_seen: i64,
    pub inserted_delta: i64,
    pub stored_events: i64,
    pub sources_ready: i64,
    pub sources_total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncSourcePayload {
    pub source: String,
    pub status: String,
    pub tone: String,
    pub files_processed: i64,
    pub changed_files: i64,
    pub skipped_files: i64,
    pub events_seen: i64,
    pub events_inserted: i64,
    pub stored_events: i64,
    #[serde(default)]
    pub malformed_lines: u64,
    #[serde(default)]
    pub oversized_lines: u64,
    #[serde(default)]
    pub skipped_lines: u64,
    #[serde(default)]
    pub accounting_anomaly_lines: u64,
    pub updated_at: Option<String>,
    pub share: f64,
    pub error_key: Option<String>,
    pub lossy_rebuild_risk: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncActionPayload {
    pub id: String,
    pub label_key: String,
    pub primary: bool,
    pub disabled: bool,
    pub reason_key: Option<String>,
}

impl Dashboard {
    /// Loads per-source archive diagnostics (F4.4 / F5.3).
    ///
    /// Reads the `source_file` state-machine counts plus
    /// `source_sync_status.{recent,history}_completed_at` columns. The
    /// completion timestamps are populated once 4.4 (RecentReady) lands;
    /// until then they are surfaced as `None`.
    pub fn diagnostics(&self) -> Result<DiagnosticsPayload> {
        let archive_root = self.store.paths.root_dir.display().to_string();
        let by_source = load_source_diagnostics(&self.conn)?;
        let recent_failures = self
            .store
            .run_log()
            .recent_runs_with_conn(&self.conn, 10)?
            .into_iter()
            .filter(crate::store::RunRecord::counts_as_failure)
            .collect();
        Ok(DiagnosticsPayload {
            archive_root,
            by_source,
            recent_failures,
        })
    }

    /// Loads cursor and recent failure health signals.
    pub fn health(&self) -> Result<HealthPayload> {
        let recent_failures = self
            .store
            .run_log()
            .recent_runs_with_conn(&self.conn, 10)?
            .into_iter()
            .filter(crate::store::RunRecord::counts_as_failure)
            .collect::<Vec<_>>();

        let mut stmt = self.conn.prepare(
            r#"
            SELECT source, cursor_key, updated_at, sqlite_status
            FROM source_cursor
            ORDER BY source ASC, cursor_key ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CursorHealth {
                source: row.get(0)?,
                cursor_key: row.get(1)?,
                updated_at: row.get(2)?,
                sqlite_status: row.get(3)?,
            })
        })?;

        Ok(HealthPayload {
            cursors: rows.collect::<rusqlite::Result<Vec<_>>>()?,
            recent_failures,
        })
    }

    /// Loads the health fields used by the live web shell without returning
    /// thousands of cursor keys that the shell only counts.
    pub fn health_summary(&self) -> Result<HealthSummaryPayload> {
        let recent_failures = self
            .store
            .run_log()
            .recent_runs_with_conn(&self.conn, 10)?
            .into_iter()
            .filter(crate::store::RunRecord::counts_as_failure)
            .collect();
        let cursor_count = scalar_i64(&self.conn, "SELECT COUNT(*) FROM source_cursor", [])?;

        Ok(HealthSummaryPayload {
            cursor_count,
            recent_failures,
        })
    }

    /// Builds the top-of-dashboard sync command center payload.
    pub fn sync_command_center(&self, filter: &QueryFilter) -> Result<SyncCommandCenterPayload> {
        let diagnostics = self.diagnostics()?;
        self.sync_command_center_with_diagnostics(filter, &diagnostics)
    }

    pub(super) fn sync_command_center_with_diagnostics(
        &self,
        filter: &QueryFilter,
        diagnostics: &DiagnosticsPayload,
    ) -> Result<SyncCommandCenterPayload> {
        let statuses = load_sync_statuses_with_conn(&self.conn, filter)?;
        let recent_runs = self
            .store
            .run_log()
            .recent_usage_import_runs_with_conn(&self.conn, 10)?;
        let current_lock = Store::current_worker_lock_with_conn(&self.conn)?;
        // Failed headlines follow the newest usage-import row. Recovered
        // `aborted` rows stay visible in details but do not count here.
        let recent_failures = recent_runs
            .iter()
            .filter(|run| run.status == "failed")
            .count();
        let selected_source = filter.source.map(|source| source.as_str().to_string());
        let risk_details = diagnostics
            .by_source
            .iter()
            .filter(|source| {
                selected_source
                    .as_deref()
                    .is_none_or(|selected| source.source == selected)
            })
            .filter(|source| source.lossy_rebuild_risk)
            .map(|source| SyncRiskSourcePayload {
                source: source.source.clone(),
                missing_file_count: source.missing_file_count,
                protected_event_count: source.protected_event_count,
            })
            .collect::<Vec<_>>();
        let risk_sources = risk_details
            .iter()
            .map(|detail| detail.source.clone())
            .collect::<Vec<_>>();
        let risk_set = risk_sources.iter().cloned().collect::<BTreeSet<_>>();
        let inserted_total = statuses.iter().map(|row| row.events_inserted).sum::<i64>();
        let seen_total = statuses.iter().map(|row| row.events_seen).sum::<i64>();
        let stored_total = statuses.iter().map(|row| row.stored_events).sum::<i64>();
        let ready_total = statuses
            .iter()
            .filter(|row| row.last_error.is_none() && row.stored_events > 0)
            .count() as i64;
        let sources_total = statuses.len() as i64;
        let max_stored = statuses
            .iter()
            .map(|row| row.stored_events)
            .max()
            .unwrap_or_default()
            .max(1);
        // Newest usage-import row, including historical `hook-run` labels.
        let last_run = recent_runs.first().map(|run| SyncLastRunPayload {
            status: run.status.clone(),
            command: run.command.clone(),
            started_at: run.started_at.clone(),
            finished_at: run.finished_at.clone(),
            error_key: (run.status == "failed")
                .then(|| "syncCenter.reason.lastRunFailed".to_string()),
        });
        let last_run_failed = last_run.as_ref().is_some_and(|run| run.status == "failed");
        let worker_lock = if current_lock.is_some() {
            "busy"
        } else {
            "available"
        }
        .to_string();
        let worker_lock_holder = current_lock.as_ref().map(|lock| lock.holder_identity());
        let lossy_rebuild_risk = !risk_sources.is_empty();
        let tone = if worker_lock == "busy" || last_run_failed {
            "warn"
        } else {
            "good"
        };
        let headline_key = if worker_lock == "busy" {
            "syncCenter.headline.busy"
        } else if last_run_failed {
            "syncCenter.headline.failed"
        } else if statuses.is_empty() {
            "syncCenter.headline.empty"
        } else {
            "syncCenter.headline.ready"
        };
        let reason_key = if last_run_failed && worker_lock != "busy" {
            "syncCenter.reason.lastRunFailed"
        } else if statuses.is_empty() {
            "syncCenter.reason.empty"
        } else {
            "syncCenter.reason.ready"
        };

        Ok(SyncCommandCenterPayload {
            mode: "live".to_string(),
            tone: tone.to_string(),
            headline_key: headline_key.to_string(),
            reason_key: reason_key.to_string(),
            generated_at: now_utc(),
            current_job: None,
            last_run,
            safety: SyncSafetyPayload {
                ordinary_sync_safe: worker_lock != "busy",
                worker_lock: worker_lock.clone(),
                worker_lock_holder,
                lossy_rebuild_risk,
                risk_sources,
                risk_details,
                recent_failures,
            },
            metrics: SyncMetricsPayload {
                events_seen: seen_total,
                inserted_delta: inserted_total,
                stored_events: stored_total,
                sources_ready: ready_total,
                sources_total,
            },
            sources: statuses
                .into_iter()
                .map(|row| {
                    let source_risk = risk_set.contains(&row.source);
                    let status = if row.last_error.is_some() {
                        "error"
                    } else if row.stored_events > 0 || row.events_seen > 0 {
                        "ok"
                    } else {
                        "idle"
                    };
                    let tone = match status {
                        "error" => "warn",
                        _ if row.parse_issues.total() > 0 => "warn",
                        "ok" => "good",
                        _ => "neutral",
                    };
                    SyncSourcePayload {
                        source: row.source,
                        status: status.to_string(),
                        tone: tone.to_string(),
                        files_processed: row.files_processed,
                        changed_files: row.changed_files,
                        skipped_files: (row.files_processed - row.changed_files).max(0),
                        events_seen: row.events_seen,
                        events_inserted: row.events_inserted,
                        stored_events: row.stored_events,
                        malformed_lines: row.parse_issues.malformed_lines,
                        oversized_lines: row.parse_issues.oversized_lines,
                        skipped_lines: row.parse_issues.skipped_lines,
                        accounting_anomaly_lines: row.parse_issues.accounting_anomaly_lines,
                        updated_at: Some(row.updated_at),
                        share: (row.stored_events as f64 / max_stored as f64).clamp(0.0, 1.0),
                        error_key: row
                            .last_error
                            .is_some()
                            .then(|| "syncCenter.reason.sourceError".to_string()),
                        lossy_rebuild_risk: source_risk,
                    }
                })
                .collect(),
            actions: vec![SyncActionPayload {
                id: "sync".to_string(),
                label_key: "syncCenter.action.sync".to_string(),
                primary: true,
                disabled: worker_lock == "busy",
                reason_key: if worker_lock == "busy" {
                    Some("syncCenter.action.busy".to_string())
                } else {
                    None
                },
            }],
        })
    }
}
