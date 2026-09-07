use anyhow::Result;

use chrono::NaiveDate;
use rusqlite::Connection;
use serde::Deserialize;

use super::{
    DailyTrendPoint, Dashboard, HeatmapPoint, HomeOverviewSnapshot, QueryFilter, ReportTimezone,
    context_pressure_event_filter, home_overview,
};
use crate::{
    models::SourceKind,
    store::Store,
    testing::{Fixture, SeedEvent},
};

const EPSILON: f64 = 1e-9;

fn assert_home_overview_projection_equivalent(
    compact: &HomeOverviewSnapshot,
    full: &home_overview::HomeOverviewPayload,
) -> Result<()> {
    assert_eq!(compact.summary.total_sessions, full.summary.total_sessions);
    assert_eq!(compact.summary.total_requests, full.summary.total_requests);
    assert_eq!(compact.summary.total_tokens, full.summary.total_tokens);
    assert_eq!(compact.summary.active_days, full.summary.active_days);
    assert_eq!(compact.summary.platforms, full.summary.platforms);
    assert!(
        (compact.summary.total_cost_usd - full.summary.total_cost_usd).abs() <= EPSILON,
        "compact/full total cost delta exceeded {EPSILON}: compact={} full={}",
        compact.summary.total_cost_usd,
        full.summary.total_cost_usd
    );
    assert!(
        (compact.summary.cache_efficiency - full.summary.cache_efficiency).abs() <= EPSILON,
        "compact/full cache efficiency delta exceeded {EPSILON}: compact={} full={}",
        compact.summary.cache_efficiency,
        full.summary.cache_efficiency
    );
    assert_eq!(
        serde_json::to_value(&compact.by_platform)?,
        serde_json::to_value(&full.by_platform)?,
        "compact/full platform keys and integer aggregates must match exactly"
    );
    Ok(())
}

#[derive(Deserialize)]
struct ReadyWidgetsSnapshotCompatibility {
    #[serde(default)]
    home_overview: Option<HomeOverviewSnapshot>,
    #[serde(default)]
    heatmap: Option<Vec<HeatmapPoint>>,
    #[serde(default)]
    trends_daily: Option<Vec<DailyTrendPoint>>,
}

include!("diagnostics_snapshot.rs");
include!("snapshot_consistency.rs");
include!("overview_breakdowns.rs");
include!("behavior.rs");
include!("comparison.rs");
include!("trends.rs");
include!("pricing.rs");
include!("facade_performance.rs");
include!("report_facade.rs");
