//! Feature-gated test fixtures for downstream adapters.
//!
//! Enable with `features = ["testing"]` in dev-dependencies. The helpers create
//! isolated tempdir-backed stores and seed only local SQLite rows; they never
//! inspect a real user home directory.

use rusqlite::params;
use tempfile::TempDir;

use crate::{
    error::Result,
    paths::AppPaths,
    query::pricing,
    store::{BootstrapOptions, Store},
};

/// Isolated llmusage test runtime.
pub struct Fixture {
    _tempdir: TempDir,
    store: Store,
}

impl Fixture {
    /// Creates a tempdir-backed runtime root and bootstraps the SQLite store.
    pub fn new() -> Result<Self> {
        Self::with_bootstrap_options(BootstrapOptions::default())
    }

    /// Creates a fixture while applying bootstrap-time options such as raw
    /// archive opt-in.
    pub fn with_bootstrap_options(options: BootstrapOptions) -> Result<Self> {
        let tempdir = TempDir::new()?;
        let paths = AppPaths::with_root(tempdir.path().join(".llmusage"))?;
        let store = Store::new(&paths)?;
        store.bootstrap_with(options)?;
        Ok(Self {
            _tempdir: tempdir,
            store,
        })
    }

    /// Returns the isolated runtime paths.
    pub fn paths(&self) -> &AppPaths {
        &self.store.paths
    }

    /// Returns the bootstrapped store.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Inserts one normalized usage event row directly into SQLite.
    pub fn seed_event(&self, event: SeedEvent<'_>) -> Result<()> {
        let conn = self.store.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source, pricing_rate,
                project_hash, project_label, project_ref, path_hash,
                session_id, session_label, source_path_hash, created_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11,
                ?12, ?13, ?14, ?15, ?16,
                ?17, ?18, ?19, ?20,
                ?21, ?22, ?23, ?24
            )
            "#,
            params![
                event.event_key,
                event.source,
                event.model,
                event.event_at,
                event.hour_start.unwrap_or(event.event_at),
                event.input_tokens,
                event.cache_read_tokens,
                event.cache_creation_tokens,
                event.output_tokens,
                event.reasoning_output_tokens,
                event.total_tokens,
                event.cost_with_cache_usd,
                event.cost_without_cache_usd,
                event.pricing_status,
                event.pricing_source,
                event.pricing_rate,
                event.project_hash,
                event.project_label,
                event.project_ref,
                event.path_hash,
                event.session_id,
                event.session_label,
                event.source_path_hash,
                event.created_at.unwrap_or(event.event_at),
            ],
        )?;
        let breakdown = if event.pricing_status == "unpriced"
            && event.pricing_source.is_none()
            && event.pricing_rate.is_none()
            && event.cost_with_cache_usd == 0.0
            && event.cost_without_cache_usd == 0.0
        {
            pricing::compute_cost(
                event.source,
                event.model,
                event.input_tokens,
                event.cache_read_tokens,
                event.output_tokens,
                event.reasoning_output_tokens,
            )
        } else {
            pricing::CostBreakdown {
                cost_with_cache_usd: event.cost_with_cache_usd,
                cost_without_cache_usd: event.cost_without_cache_usd,
                pricing_status: parse_seed_pricing_status(event.pricing_status),
                pricing_source: event.pricing_source.map(str::to_string),
                pricing_rate: event.pricing_rate.map(str::to_string),
            }
        };
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source, pricing_rate,
                event_count, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 1, ?18)
            ON CONFLICT(source, model, hour_start, project_hash) DO UPDATE SET
                input_tokens = input_tokens + excluded.input_tokens,
                cache_read_tokens = cache_read_tokens + excluded.cache_read_tokens,
                cache_creation_tokens = cache_creation_tokens + excluded.cache_creation_tokens,
                output_tokens = output_tokens + excluded.output_tokens,
                reasoning_output_tokens = reasoning_output_tokens + excluded.reasoning_output_tokens,
                total_tokens = total_tokens + excluded.total_tokens,
                cost_with_cache_usd = cost_with_cache_usd + excluded.cost_with_cache_usd,
                cost_without_cache_usd = cost_without_cache_usd + excluded.cost_without_cache_usd,
                pricing_status = CASE
                    WHEN pricing_status = excluded.pricing_status THEN pricing_status
                    ELSE 'mixed'
                END,
                pricing_source = CASE
                    WHEN pricing_source IS excluded.pricing_source THEN pricing_source
                    ELSE 'mixed'
                END,
                pricing_rate = CASE
                    WHEN pricing_rate IS excluded.pricing_rate THEN pricing_rate
                    ELSE 'mixed'
                END,
                event_count = event_count + excluded.event_count,
                updated_at = excluded.updated_at
            "#,
            params![
                event.source,
                event.model,
                event.hour_start.unwrap_or(event.event_at),
                event.project_hash,
                event.project_label,
                event.project_ref,
                event.input_tokens,
                event.cache_read_tokens,
                event.cache_creation_tokens,
                event.output_tokens,
                event.reasoning_output_tokens,
                event.total_tokens,
                breakdown.cost_with_cache_usd,
                breakdown.cost_without_cache_usd,
                breakdown.pricing_status.as_str(),
                breakdown.pricing_source,
                breakdown.pricing_rate,
                event.created_at.unwrap_or(event.event_at),
            ],
        )?;
        Ok(())
    }

    /// Seeds dashboard-heavy tables with deterministic synthetic rows.
    pub fn seed_dashboard(&self, rows: usize) -> Result<()> {
        let conn = self.store.open_connection()?;
        conn.execute(
            "INSERT OR IGNORE INTO integration_install(source, install_type, status, config_path, backup_path, details_json, updated_at) VALUES ('codex', 'probe', 'ready', NULL, NULL, NULL, '2026-05-05T00:00:00Z')",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO run_log(command, status, summary, error, started_at, finished_at, duration_ms) VALUES ('sync', 'aborted', NULL, 'recovered stale running record', '2026-05-05T00:00:00Z', '2026-05-05T00:01:00Z', 60000)",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO source_cursor(source, cursor_key, sqlite_status, updated_at) VALUES ('opencode', 'main', 'ok', '2026-05-05T00:00:00Z')",
            [],
        )?;

        let models = ["gpt-5", "claude-sonnet-4", "o3"];
        let sources = ["codex", "claude", "opencode"];
        for idx in 0..rows {
            let source = sources[idx % sources.len()];
            let model = models[idx % models.len()];
            let event_key = format!("{source}:fixture:{idx}");
            let project_hash = format!("project-{}-{}", idx % 4, idx / (28 * 24));
            let project_label = format!("Project {}", idx % 4);
            let project_ref = format!("ref-{}", idx % 4);
            let path_hash = format!("path-{idx}");
            let day = 1 + ((idx / 24) % 28);
            let hour = idx % 24;
            let hour_start = format!("2026-04-{day:02}T{hour:02}:00:00Z");
            let event_at = format!("2026-04-{day:02}T{hour:02}:15:00Z");
            let input_tokens = 10 + (idx % 7) as i64;
            let cache_read_tokens = (idx % 3) as i64;
            let output_tokens = 5 + (idx % 5) as i64;
            let reasoning_output_tokens = (idx % 2) as i64;
            let total_tokens =
                input_tokens + cache_read_tokens + output_tokens + reasoning_output_tokens;
            let updated_at = format!("2026-05-05T00:{:02}:00Z", idx % 60);
            let seed = SeedEvent {
                event_key: &event_key,
                source,
                model,
                event_at: &event_at,
                hour_start: Some(&hour_start),
                input_tokens,
                cache_read_tokens,
                output_tokens,
                reasoning_output_tokens,
                total_tokens,
                project_hash: &project_hash,
                project_label: &project_label,
                project_ref: Some(&project_ref),
                path_hash: &path_hash,
                created_at: Some(&updated_at),
                ..SeedEvent::default()
            };
            self.seed_event(seed)?;
        }
        Ok(())
    }
}

fn parse_seed_pricing_status(raw: &str) -> pricing::PricingStatus {
    match raw {
        "static" => pricing::PricingStatus::Static,
        "snapshot" => pricing::PricingStatus::Snapshot,
        _ => pricing::PricingStatus::Unpriced,
    }
}

/// Direct seed row accepted by [`Fixture::seed_event`].
#[derive(Debug, Clone)]
pub struct SeedEvent<'a> {
    pub event_key: &'a str,
    pub source: &'a str,
    pub model: &'a str,
    pub event_at: &'a str,
    pub hour_start: Option<&'a str>,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
    pub cost_with_cache_usd: f64,
    pub cost_without_cache_usd: f64,
    pub pricing_status: &'a str,
    pub pricing_source: Option<&'a str>,
    pub pricing_rate: Option<&'a str>,
    pub project_hash: &'a str,
    pub project_label: &'a str,
    pub project_ref: Option<&'a str>,
    pub path_hash: &'a str,
    pub session_id: Option<&'a str>,
    pub session_label: Option<&'a str>,
    pub source_path_hash: Option<&'a str>,
    pub created_at: Option<&'a str>,
}

impl Default for SeedEvent<'_> {
    fn default() -> Self {
        Self {
            event_key: "test:event",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-08T00:00:00Z",
            hour_start: None,
            input_tokens: 1,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 1,
            cost_with_cache_usd: 0.0,
            cost_without_cache_usd: 0.0,
            pricing_status: "unpriced",
            pricing_source: None,
            pricing_rate: None,
            project_hash: "project-test",
            project_label: "Project Test",
            project_ref: None,
            path_hash: "path-test",
            session_id: None,
            session_label: None,
            source_path_hash: None,
            created_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{query::Dashboard, testing::SeedEvent};

    use super::Fixture;

    #[test]
    fn fixture_bootstraps_store_and_seeds_event() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_event(SeedEvent {
            event_key: "codex:test:1",
            total_tokens: 42,
            ..SeedEvent::default()
        })?;

        let overview = Dashboard::open(fixture.store())?.overview(&Default::default())?;
        assert_eq!(overview.total.total_tokens, 42);
        Ok(())
    }

    #[test]
    fn seed_dashboard_populates_dashboard_snapshot() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_dashboard(12)?;

        let snapshot = Dashboard::open(fixture.store())?.snapshot(&Default::default())?;
        assert_eq!(snapshot.overview.bucket_count, 12);
        assert!(!snapshot.models.is_empty());
        Ok(())
    }
}
