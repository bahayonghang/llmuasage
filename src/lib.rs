pub mod api;
pub mod commands;
pub mod common;
pub mod domain;
pub mod export;
pub mod integrations;
pub mod parsers;
pub mod query;
pub mod registry;
pub mod runtime;
pub mod store;
pub mod sync;
#[cfg(any(feature = "testing", test))]
pub mod testing;
pub mod tui;
pub mod web;

pub mod app {
    pub use crate::runtime::app::*;
}

pub mod error {
    pub use crate::api::error::*;
}

pub mod logging {
    pub use crate::runtime::logging::*;
}

pub mod models {
    pub use crate::domain::models::*;
}

pub mod paths {
    pub use crate::runtime::paths::*;
}

pub mod project {
    pub use crate::domain::project::*;
}

pub mod sources {
    pub use crate::registry::*;
}

pub mod util {
    pub use crate::common::util::*;
}

pub use api::error::{LlmusageError, Result};
pub use query::{
    ActivityBreakdown, ActivityPayload, BehaviorSupport, CategoryCompareRow, CompareMetric,
    CompareModelCandidate, DailyTrendPoint, Dashboard, DashboardCoreSnapshot, DiagnosticsPayload,
    ExplorerDimension, ExplorerFilters, ExplorerGranularity, ExplorerMetric, ExplorerPayload,
    ExplorerQuery, ExplorerRow, ExplorerSeriesPoint, ExplorerSupport, ExplorerTokenType,
    ExplorerTotals, HomeOverviewPayload, LogRecord, LogsPage, LogsQuery, ModelBreakdown,
    ModelComparePayload, ModelCompareStats, OptimizeFinding, OptimizePayload, OverviewPayload,
    ProjectBreakdown, QueryFilter, ReportTimezone, SourceDiagnostics, ToolBreakdown, ToolsPayload,
};
pub use runtime::paths::AppPaths;

use anyhow::Result as AnyhowResult;
use clap::Parser;

pub async fn run() -> AnyhowResult<()> {
    runtime::logging::init_logging()?;
    if let Some(language) = commands::help::is_top_level_help_request(std::env::args().skip(1)) {
        print!("{}", commands::help::top_level_help(language));
        return Ok(());
    }
    let cli = commands::Cli::parse();
    let app = runtime::app::AppContext::with_cli_home(cli.home.clone())?;
    commands::dispatch(app, cli).await
}
