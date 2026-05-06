use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{
    store::{IntegrationState, RunRecord, Store},
    util::now_utc,
};

pub mod pricing;
pub mod reports;

/// Aggregated token counters returned by overview and trend queries.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TokenSummary {
    /// Sum of non-cached input tokens.
    pub input_tokens: i64,
    /// Sum of cached/reused input tokens.
    pub cached_input_tokens: i64,
    /// Sum of non-reasoning output tokens.
    pub output_tokens: i64,
    /// Sum of separately reported reasoning tokens.
    pub reasoning_output_tokens: i64,
    /// Total normalized tokens across all categories.
    pub total_tokens: i64,
}

/// Top-level dashboard numbers shown in status, web, and export views.
#[derive(Debug, Clone, Serialize)]
pub struct OverviewPayload {
    /// Snapshot generation time in RFC 3339 format.
    pub generated_at: String,
    /// Lifetime totals across the entire dataset.
    pub total: TokenSummary,
    /// Totals restricted to the last 24 hours.
    pub last_24h: TokenSummary,
    /// Distinct source count present in aggregated buckets.
    pub source_count: i64,
    /// Number of persisted 30-minute buckets.
    pub bucket_count: i64,
    /// Last successful sync/hook-run finish time.
    pub last_sync_at: Option<String>,
    /// Last successful HTML export finish time.
    pub last_export_at: Option<String>,
}

/// One plotted point in a trend series.
#[derive(Debug, Clone, Serialize)]
pub struct TrendPoint {
    /// Display label for the time window bucket.
    pub label: String,
    /// Total tokens in the bucket.
    pub total_tokens: i64,
}

/// Per-model aggregate shown in dashboard breakdowns.
#[derive(Debug, Clone, Serialize)]
pub struct ModelBreakdown {
    /// Normalized model name.
    pub model: String,
    /// Summed non-cached input tokens.
    pub input_tokens: i64,
    /// Summed cached input tokens.
    pub cached_input_tokens: i64,
    /// Summed output tokens.
    pub output_tokens: i64,
    /// Summed reasoning-only output tokens.
    pub reasoning_output_tokens: i64,
    /// Summed total tokens.
    pub total_tokens: i64,
}

/// Per-source aggregate plus freshest observed event time.
#[derive(Debug, Clone, Serialize)]
pub struct SourceBreakdown {
    /// Source identifier.
    pub source: String,
    /// Summed total tokens for the source.
    pub total_tokens: i64,
    /// Latest raw event timestamp observed for the source.
    pub last_event_at: Option<String>,
}

/// Per-project aggregate shown in rankings.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectBreakdown {
    /// Stable hashed project key.
    pub project_hash: String,
    /// Human-readable project label.
    pub project_label: String,
    /// Optional repo/project reference.
    pub project_ref: Option<String>,
    /// Summed total tokens for the project.
    pub total_tokens: i64,
}

/// Cost estimate line for one `(source, model)` pair.
#[derive(Debug, Clone, Serialize)]
pub struct CostLine {
    /// Source identifier.
    pub source: String,
    /// Normalized model name.
    pub model: String,
    /// Summed total tokens for the pair.
    pub total_tokens: i64,
    /// Estimated USD cost using the built-in static price catalog.
    pub estimated_cost_usd: f64,
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

/// Health payload combining integrations, cursors, and recent failures.
#[derive(Debug, Clone, Serialize)]
pub struct HealthPayload {
    /// Latest install/probe states for known integrations.
    pub integrations: Vec<IntegrationState>,
    /// Cursor freshness/health rows.
    pub cursors: Vec<CursorHealth>,
    /// Recent non-success command runs.
    pub recent_failures: Vec<RunRecord>,
}

/// Full snapshot embedded into exported HTML bundles.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardSnapshot {
    /// Headline overview metrics.
    pub overview: OverviewPayload,
    /// Last 24 hours trend series.
    pub day_trends: Vec<TrendPoint>,
    /// Last 7 days trend series.
    pub week_trends: Vec<TrendPoint>,
    /// Last 30 days trend series.
    pub month_trends: Vec<TrendPoint>,
    /// Lifetime/month-grouped trend series.
    pub all_trends: Vec<TrendPoint>,
    /// Per-model breakdown table.
    pub models: Vec<ModelBreakdown>,
    /// Per-source breakdown table.
    pub sources: Vec<SourceBreakdown>,
    /// Per-project ranking table.
    pub projects: Vec<ProjectBreakdown>,
    /// Per-source/model cost estimate table.
    pub costs: Vec<CostLine>,
    /// Integration/cursor/run health payload.
    pub health: HealthPayload,
}

/// Read-side façade backed by a single SQLite connection. All eight dashboard
/// queries share the same connection so a snapshot only opens the DB once.
pub struct Dashboard {
    store: Store,
    conn: Connection,
}

impl Dashboard {
    /// Opens a fresh connection bound to `store` and returns a Dashboard ready
    /// to answer any of the dashboard queries.
    pub fn open(store: &Store) -> Result<Self> {
        let conn = store.open_connection()?;
        Ok(Self {
            store: store.clone(),
            conn,
        })
    }

    /// Loads top-level lifetime/24h overview metrics plus recent sync/export timestamps.
    pub fn overview(&self) -> Result<OverviewPayload> {
        let total = query_token_summary(&self.conn, None)?;
        let cutoff = (Utc::now() - Duration::hours(24)).to_rfc3339();
        let last_24h = query_token_summary(&self.conn, Some(&cutoff))?;
        let source_count = scalar_i64(
            &self.conn,
            "SELECT COUNT(DISTINCT source) FROM usage_bucket_30m",
            [],
        )?;
        let bucket_count = scalar_i64(&self.conn, "SELECT COUNT(*) FROM usage_bucket_30m", [])?;
        let last_sync_at = scalar_optional_string(
            &self.conn,
            "SELECT MAX(finished_at) FROM run_log WHERE command IN ('sync', 'hook-run') AND status = 'success'",
            [],
        )?;
        let last_export_at = scalar_optional_string(
            &self.conn,
            "SELECT MAX(finished_at) FROM run_log WHERE command = 'export html' AND status = 'success'",
            [],
        )?;

        Ok(OverviewPayload {
            generated_at: now_utc(),
            total,
            last_24h,
            source_count,
            bucket_count,
            last_sync_at,
            last_export_at,
        })
    }

    /// Loads aggregated trend points for the requested window (`day`, `week`, `month`, or `all`).
    pub fn trends(&self, window: &str) -> Result<Vec<TrendPoint>> {
        let (sql, cutoff): (&str, Option<String>) = match window {
            "day" => (
                "SELECT hour_start AS label, SUM(total_tokens) AS total_tokens FROM usage_bucket_30m WHERE hour_start >= ?1 GROUP BY hour_start ORDER BY hour_start ASC",
                Some((Utc::now() - Duration::hours(24)).to_rfc3339()),
            ),
            "week" => (
                "SELECT substr(hour_start, 1, 10) AS label, SUM(total_tokens) AS total_tokens FROM usage_bucket_30m WHERE hour_start >= ?1 GROUP BY substr(hour_start, 1, 10) ORDER BY label ASC",
                Some((Utc::now() - Duration::days(7)).to_rfc3339()),
            ),
            "month" => (
                "SELECT substr(hour_start, 1, 10) AS label, SUM(total_tokens) AS total_tokens FROM usage_bucket_30m WHERE hour_start >= ?1 GROUP BY substr(hour_start, 1, 10) ORDER BY label ASC",
                Some((Utc::now() - Duration::days(30)).to_rfc3339()),
            ),
            _ => (
                "SELECT substr(hour_start, 1, 7) AS label, SUM(total_tokens) AS total_tokens FROM usage_bucket_30m GROUP BY substr(hour_start, 1, 7) ORDER BY label ASC",
                None,
            ),
        };

        let mut stmt = self.conn.prepare(sql)?;
        if let Some(cutoff) = cutoff {
            let rows = stmt.query_map(params![cutoff], |row| {
                Ok(TrendPoint {
                    label: row.get(0)?,
                    total_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        } else {
            let rows = stmt.query_map([], |row| {
                Ok(TrendPoint {
                    label: row.get(0)?,
                    total_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        }
    }

    /// Loads total token usage grouped by normalized model.
    pub fn model_breakdown(&self) -> Result<Vec<ModelBreakdown>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                model,
                SUM(input_tokens),
                SUM(cached_input_tokens),
                SUM(output_tokens),
                SUM(reasoning_output_tokens),
                SUM(total_tokens)
            FROM usage_bucket_30m
            GROUP BY model
            ORDER BY SUM(total_tokens) DESC, model ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ModelBreakdown {
                model: row.get(0)?,
                input_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                cached_input_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                output_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                reasoning_output_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads total token usage grouped by source plus each source's freshest event time.
    pub fn source_breakdown(&self) -> Result<Vec<SourceBreakdown>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                totals.source,
                totals.total_tokens,
                last_event.last_event_at
            FROM (
                SELECT source, SUM(total_tokens) AS total_tokens
                FROM usage_bucket_30m
                GROUP BY source
            ) totals
            LEFT JOIN (
                SELECT source, MAX(event_at) AS last_event_at
                FROM usage_event
                GROUP BY source
            ) last_event
                ON last_event.source = totals.source
            ORDER BY totals.total_tokens DESC, totals.source ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SourceBreakdown {
                source: row.get(0)?,
                total_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                last_event_at: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads ranked project totals derived from aggregated buckets.
    pub fn project_breakdown(&self) -> Result<Vec<ProjectBreakdown>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                project_hash,
                MAX(project_label),
                MAX(project_ref),
                SUM(total_tokens)
            FROM usage_bucket_30m
            WHERE project_hash <> ''
            GROUP BY project_hash
            ORDER BY SUM(total_tokens) DESC, MAX(project_label) ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProjectBreakdown {
                project_hash: row.get(0)?,
                project_label: row
                    .get::<_, Option<String>>(1)?
                    .unwrap_or_else(|| "unknown-project".to_string()),
                project_ref: row.get(2)?,
                total_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads estimated cost totals for each `(source, model)` pair.
    pub fn cost_breakdown(&self) -> Result<Vec<CostLine>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                source,
                model,
                SUM(input_tokens),
                SUM(cached_input_tokens),
                SUM(output_tokens),
                SUM(reasoning_output_tokens),
                SUM(total_tokens)
            FROM usage_bucket_30m
            GROUP BY source, model
            ORDER BY SUM(total_tokens) DESC, source ASC, model ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            let source: String = row.get(0)?;
            let model: String = row.get(1)?;
            let input_tokens = row.get::<_, Option<i64>>(2)?.unwrap_or_default();
            let cached_input_tokens = row.get::<_, Option<i64>>(3)?.unwrap_or_default();
            let output_tokens = row.get::<_, Option<i64>>(4)?.unwrap_or_default();
            let reasoning_output_tokens = row.get::<_, Option<i64>>(5)?.unwrap_or_default();
            let total_tokens = row.get::<_, Option<i64>>(6)?.unwrap_or_default();
            let estimated_cost_usd = pricing::estimate_cost_usd(
                &source,
                &model,
                input_tokens,
                cached_input_tokens,
                output_tokens,
                reasoning_output_tokens,
            );

            Ok(CostLine {
                source,
                model,
                total_tokens,
                estimated_cost_usd,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads integration, cursor, and recent failure health signals.
    pub fn health(&self) -> Result<HealthPayload> {
        let integrations = self
            .store
            .integration_state()
            .load_integration_states_with_conn(&self.conn)?;
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
            integrations,
            cursors: rows.collect::<rusqlite::Result<Vec<_>>>()?,
            recent_failures,
        })
    }

    /// Builds the full dashboard snapshot used by static HTML export.
    pub fn snapshot(&self) -> Result<DashboardSnapshot> {
        Ok(DashboardSnapshot {
            overview: self.overview()?,
            day_trends: self.trends("day")?,
            week_trends: self.trends("week")?,
            month_trends: self.trends("month")?,
            all_trends: self.trends("all")?,
            models: self.model_breakdown()?,
            sources: self.source_breakdown()?,
            projects: self.project_breakdown()?,
            costs: self.cost_breakdown()?,
            health: self.health()?,
        })
    }
}

fn query_token_summary(conn: &Connection, cutoff: Option<&str>) -> Result<TokenSummary> {
    let sql = if cutoff.is_some() {
        r#"
        SELECT
            COALESCE(SUM(input_tokens), 0),
            COALESCE(SUM(cached_input_tokens), 0),
            COALESCE(SUM(output_tokens), 0),
            COALESCE(SUM(reasoning_output_tokens), 0),
            COALESCE(SUM(total_tokens), 0)
        FROM usage_bucket_30m
        WHERE hour_start >= ?1
        "#
    } else {
        r#"
        SELECT
            COALESCE(SUM(input_tokens), 0),
            COALESCE(SUM(cached_input_tokens), 0),
            COALESCE(SUM(output_tokens), 0),
            COALESCE(SUM(reasoning_output_tokens), 0),
            COALESCE(SUM(total_tokens), 0)
        FROM usage_bucket_30m
        "#
    };

    let mut stmt = conn.prepare(sql)?;
    let summary = if let Some(cutoff) = cutoff {
        stmt.query_row(params![cutoff], map_token_summary)?
    } else {
        stmt.query_row([], map_token_summary)?
    };
    Ok(summary)
}

fn map_token_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<TokenSummary> {
    Ok(TokenSummary {
        input_tokens: row.get(0)?,
        cached_input_tokens: row.get(1)?,
        output_tokens: row.get(2)?,
        reasoning_output_tokens: row.get(3)?,
        total_tokens: row.get(4)?,
    })
}

fn scalar_i64<P>(conn: &Connection, sql: &str, params: P) -> Result<i64>
where
    P: rusqlite::Params,
{
    Ok(conn
        .query_row(sql, params, |row| row.get::<_, Option<i64>>(0))?
        .unwrap_or_default())
}

fn scalar_optional_string<P>(conn: &Connection, sql: &str, params: P) -> Result<Option<String>>
where
    P: rusqlite::Params,
{
    Ok(conn
        .query_row(sql, params, |row| row.get(0))
        .unwrap_or(None))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::TempDir;

    use super::Dashboard;
    use crate::{paths::AppPaths, store::Store};

    #[test]
    fn dashboard_snapshot_uses_single_connection_and_matches_individual_methods() -> Result<()> {
        let fixture = QueryFixture::new()?;
        fixture.seed_dashboard_rows(180)?;

        Store::reset_open_connection_counter();
        let dashboard = Dashboard::open(&fixture.store)?;
        let snapshot = dashboard.snapshot()?;
        assert_eq!(Store::open_connection_count(), 1);

        let mut snapshot_overview = serde_json::to_value(&snapshot.overview)?;
        let mut method_overview = serde_json::to_value(dashboard.overview()?)?;
        snapshot_overview["generated_at"] = serde_json::Value::String("same".to_string());
        method_overview["generated_at"] = serde_json::Value::String("same".to_string());
        assert_eq!(snapshot_overview, method_overview);
        assert_eq!(
            serde_json::to_value(&snapshot.day_trends)?,
            serde_json::to_value(dashboard.trends("day")?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.week_trends)?,
            serde_json::to_value(dashboard.trends("week")?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.month_trends)?,
            serde_json::to_value(dashboard.trends("month")?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.all_trends)?,
            serde_json::to_value(dashboard.trends("all")?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.models)?,
            serde_json::to_value(dashboard.model_breakdown()?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.sources)?,
            serde_json::to_value(dashboard.source_breakdown()?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.projects)?,
            serde_json::to_value(dashboard.project_breakdown()?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.costs)?,
            serde_json::to_value(dashboard.cost_breakdown()?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.health)?,
            serde_json::to_value(dashboard.health()?)?
        );

        assert!(snapshot.overview.bucket_count >= 180);
        assert_eq!(snapshot.sources.len(), 3);
        assert!(!snapshot.models.is_empty());
        assert!(!snapshot.projects.is_empty());
        Ok(())
    }

    struct QueryFixture {
        _tempdir: TempDir,
        store: Store,
    }

    impl QueryFixture {
        fn new() -> Result<Self> {
            let tempdir = TempDir::new()?;
            let root_dir = tempdir.path().join(".llmusage");
            let paths = AppPaths {
                root_dir: root_dir.clone(),
                db_path: root_dir.join("llmusage.db"),
                bin_dir: root_dir.join("bin"),
                backups_dir: root_dir.join("backups"),
                exports_dir: root_dir.join("exports"),
                hook_cmd_path: root_dir.join("bin").join("llmusage-hook.cmd"),
                hook_sh_path: root_dir.join("bin").join("llmusage-hook.sh"),
                lock_path: root_dir.join("worker.lock"),
            };
            let store = Store::new(&paths);
            store.bootstrap()?;
            Ok(Self {
                _tempdir: tempdir,
                store,
            })
        }

        fn seed_dashboard_rows(&self, rows: usize) -> Result<()> {
            let conn = self.store.open_connection()?;
            conn.execute(
                "INSERT INTO integration_install(source, install_type, status, config_path, backup_path, details_json, updated_at) VALUES ('codex', 'probe', 'ready', NULL, NULL, NULL, '2026-05-05T00:00:00Z')",
                [],
            )?;
            conn.execute(
                "INSERT INTO run_log(command, status, summary, error, started_at, finished_at, duration_ms) VALUES ('sync', 'aborted', NULL, 'recovered stale running record', '2026-05-05T00:00:00Z', '2026-05-05T00:01:00Z', 60000)",
                [],
            )?;
            conn.execute(
                "INSERT INTO source_cursor(source, cursor_key, sqlite_status, updated_at) VALUES ('opencode', 'main', 'ok', '2026-05-05T00:00:00Z')",
                [],
            )?;

            let models = ["gpt-5", "claude-sonnet-4", "o3"];
            let sources = ["codex", "claude", "opencode"];
            for idx in 0..rows {
                let source = sources[idx % sources.len()];
                let model = models[idx % models.len()];
                let project_hash = format!("project-{}", idx % 4);
                let project_label = format!("Project {}", idx % 4);
                let project_ref = format!("ref-{}", idx % 4);
                let day = 1 + ((idx / 24) % 28);
                let hour = idx % 24;
                let hour_start = format!("2026-04-{day:02}T{hour:02}:00:00Z");
                let event_at = format!("2026-04-{day:02}T{hour:02}:15:00Z");
                let input_tokens = 10 + (idx % 7) as i64;
                let cached_input_tokens = (idx % 3) as i64;
                let output_tokens = 5 + (idx % 5) as i64;
                let reasoning_output_tokens = (idx % 2) as i64;
                let total_tokens =
                    input_tokens + cached_input_tokens + output_tokens + reasoning_output_tokens;
                let updated_at = format!("2026-05-05T00:{:02}:00Z", idx % 60);

                conn.execute(
                    r#"
                    INSERT INTO usage_bucket_30m(
                        source, model, hour_start, project_hash, project_label, project_ref,
                        input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                    "#,
                    rusqlite::params![
                        source,
                        model,
                        hour_start,
                        project_hash,
                        project_label,
                        project_ref,
                        input_tokens,
                        cached_input_tokens,
                        output_tokens,
                        reasoning_output_tokens,
                        total_tokens,
                        updated_at,
                    ],
                )?;
                conn.execute(
                    r#"
                    INSERT INTO usage_event(
                        event_key, source, model, event_at, hour_start,
                        input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens,
                        project_hash, project_label, project_ref, path_hash, created_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                    "#,
                    rusqlite::params![
                        format!("{source}:{idx}"),
                        source,
                        model,
                        event_at,
                        hour_start,
                        input_tokens,
                        cached_input_tokens,
                        output_tokens,
                        reasoning_output_tokens,
                        total_tokens,
                        project_hash,
                        project_label,
                        project_ref,
                        format!("path-{idx}"),
                        updated_at,
                    ],
                )?;
            }
            Ok(())
        }
    }
}
