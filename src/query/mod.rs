use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Datelike, Duration, NaiveDate, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, params_from_iter, types::Type};
use serde::{Deserialize, Serialize};

use crate::{
    domain::source_descriptor::registered_source_descriptors,
    error::Result,
    models::ParseIssues,
    store::{RunRecord, Store},
    util::now_utc,
};

mod explorer;
pub mod filter;
mod heatmap;
mod home_overview;
mod hour_of_week;
pub mod inventory;
pub(crate) mod logs;
pub mod pricing;
pub mod pricing_catalog;
pub mod reports;
pub(crate) mod timezone;
mod top_sessions;

pub use explorer::{
    ExplorerDimension, ExplorerFilters, ExplorerGranularity, ExplorerMetric, ExplorerPayload,
    ExplorerQuery, ExplorerRow, ExplorerSeriesPoint, ExplorerSupport, ExplorerTokenType,
    ExplorerTotals,
};
pub use filter::{QueryFilter, ReportTimezone};
pub use heatmap::HeatmapPoint;
pub use home_overview::{
    HomeOverviewBootstrap, HomeOverviewPayload, HomeOverviewPlatformStats, HomeOverviewSeriesItem,
    HomeOverviewSummary,
};
pub use hour_of_week::HourOfWeekCell;
pub use inventory::{InstalledItem, InventoryKind, InventoryRoots, InventorySource};
pub use logs::{LogRecord, LogsPage, LogsQuery};
pub use pricing::{
    CostBreakdown, PRICING_MIXED, PRICING_SOURCE_REPORTED, PRICING_UNPRICED, PricingStatus,
};
pub use pricing_catalog::PricingCatalog;
pub use top_sessions::{TopSessionRow, TopSessionsQuery, TopSessionsSort};

mod activity;
mod breakdowns;
mod comparison;
mod diagnostics;
mod optimize;
mod overview;
mod snapshot;
mod tools;

pub use activity::{ActivityBreakdown, ActivityPayload, BehaviorSupport};
pub use breakdowns::{CostLine, HostBreakdown, ModelBreakdown, ProjectBreakdown, SourceBreakdown};
pub use comparison::{
    CategoryCompareRow, CompareMetric, CompareModelCandidate, ModelComparePayload,
    ModelCompareStats,
};
pub use diagnostics::{
    CursorHealth, DiagnosticsPayload, HealthPayload, HealthSummaryPayload, SourceDiagnostics,
    SyncActionPayload, SyncCommandCenterPayload, SyncCurrentJobPayload, SyncLastRunPayload,
    SyncMetricsPayload, SyncRiskSourcePayload, SyncSafetyPayload, SyncSourcePayload,
};
pub use optimize::{OptimizeFinding, OptimizePayload, ZombieItem, ZombieReport};
pub use overview::{
    ContextPressurePayload, DailyModelPoint, DailyTrendPoint, HourlyTrendPoint, MonthlyTrendPoint,
    OverviewPayload, PeriodDetailRow, TokenSummary, TrendPoint, month_date_bounds,
};
pub use snapshot::{
    DashboardCoreSnapshot, DashboardInteractiveSnapshot, DashboardSnapshot, HomeOverviewSnapshot,
};
pub use tools::{ToolBreakdown, ToolsPayload};

#[cfg(test)]
use breakdowns::context_pressure_event_filter;
#[cfg(test)]
pub(crate) use diagnostics::{
    diagnostics_stat_calls, load_source_diagnostics, reset_diagnostics_stat_counter,
};

/// Read-side façade backed by a single SQLite connection. All eight dashboard
/// queries share the same connection so a snapshot only opens the DB once.
/// Composite snapshot methods wrap database metric reads in a short deferred
/// transaction on that connection; [`Dashboard::open`] does not start it.
pub struct Dashboard {
    pub(super) store: Store,
    pub(super) conn: Connection,
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

    /// Opens a Dashboard whose connection uses a shorter `busy_timeout`.
    ///
    /// Web/API handlers use this so a locked database surfaces as a fast
    /// section error inside the existing timeout/degraded flow instead of
    /// blocking for the writer-oriented 30s default. Sync writers and export
    /// paths keep using [`Dashboard::open`].
    pub fn open_with_busy_timeout(
        store: &Store,
        busy_timeout: std::time::Duration,
    ) -> Result<Self> {
        let conn = store.open_connection_with_busy_timeout(busy_timeout)?;
        Ok(Self {
            store: store.clone(),
            conn,
        })
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn interrupt_handle(&self) -> rusqlite::InterruptHandle {
        self.conn.get_interrupt_handle()
    }

    #[cfg(test)]
    pub(crate) fn test_slow_query(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "WITH RECURSIVE counter(value) AS (VALUES(1) UNION ALL SELECT value + 1 FROM counter WHERE value < 100000000) SELECT SUM(value) FROM counter",
            [],
            |row| row.get(0),
        )?)
    }
}
fn sorted_unique_sources(raw: Option<String>) -> Vec<String> {
    let mut sources: Vec<String> = raw
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    sources.sort_unstable();
    sources.dedup();
    sources
}

fn behavior_support(
    conn: &Connection,
    table: &str,
    filter: crate::query::filter::SqlFilter,
) -> Result<BehaviorSupport> {
    let exists = conn.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM {table}{} LIMIT 1)",
            filter.where_sql()
        ),
        params_from_iter(filter.params().iter()),
        |row| row.get::<_, bool>(0),
    )?;
    Ok(if exists {
        BehaviorSupport {
            supported: true,
            level: "normalized".to_string(),
            reason: None,
        }
    } else {
        BehaviorSupport {
            supported: false,
            level: "no_data".to_string(),
            reason: Some(
                "No normalized behavior facts match this filter; run sync with a parser that emits behavior facts."
                    .to_string(),
            ),
        }
    })
}

fn ratio(numerator: i64, denominator: i64) -> f64 {
    if denominator <= 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
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
fn explain_query_plan<P>(conn: &Connection, sql: &str, params: P) -> Result<Vec<String>>
where
    P: rusqlite::Params,
{
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
    let rows = stmt.query_map(params, |row| row.get::<_, String>(3))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
