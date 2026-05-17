pub mod app;
pub mod commands;
pub mod error;
pub mod export;
pub mod integrations;
pub mod logging;
pub mod models;
pub mod parsers;
pub mod paths;
pub mod project;
pub mod query;
pub mod sources;
pub mod store;
pub mod sync;
#[cfg(any(feature = "testing", test))]
pub mod testing;
pub mod tui;
pub mod util;
pub mod web;

pub use error::{LlmusageError, Result};
pub use paths::AppPaths;
pub use query::{
    ActivityBreakdown, ActivityPayload, BehaviorSupport, CategoryCompareRow, CompareMetric,
    CompareModelCandidate, DailyTrendPoint, Dashboard, DashboardCoreSnapshot, DiagnosticsPayload,
    HomeOverviewPayload, LogRecord, LogsPage, LogsQuery, ModelBreakdown, ModelComparePayload,
    ModelCompareStats, OptimizeFinding, OptimizePayload, OverviewPayload, ProjectBreakdown,
    QueryFilter, ReportTimezone, SourceDiagnostics, ToolBreakdown, ToolsPayload,
};

use anyhow::Result as AnyhowResult;
use clap::Parser;

pub async fn run() -> AnyhowResult<()> {
    logging::init_logging()?;
    let cli = commands::Cli::parse();
    let app = app::AppContext::with_cli_home(cli.home.clone())?;
    commands::dispatch(app, cli).await
}
