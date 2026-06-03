//! Local-first usage analytics for AI coding CLIs.
//!
//! The stable adapter surface for downstream desktop apps and tests is the
//! root-level façade: [`AppPaths`], [`Store`], [`Dashboard`], [`QueryFilter`],
//! [`ReportTimezone`], [`JobRegistry`], sync job payloads, and the shared
//! [`Result`] / [`LlmusageError`] types. Broad implementation modules remain
//! public for 0.7.x compatibility, but callers should prefer the façade or the
//! documented `paths`, `store`, `query`, `sync`, `models`, and `error`
//! namespaces when embedding the crate.

#[doc = "Compatibility module for error internals. Prefer root `LlmusageError`/`Result` or `llmusage::error`."]
pub mod api;
#[doc = "Compatibility module for CLI command internals. Not the preferred embedding surface."]
pub mod commands;
#[doc = "Compatibility module for shared internal utilities. Prefer documented root façade types."]
pub mod common;
#[doc = "Compatibility module for domain internals. Prefer `llmusage::models` and `llmusage::project`."]
pub mod domain;
#[doc = "Compatibility module for export internals. Prefer CLI/export commands or `Dashboard` snapshots."]
pub mod export;
#[doc = "Compatibility module for integration internals. Prefer `sources` descriptors unless installing hooks."]
pub mod integrations;
#[doc = "Compatibility module for parser internals. Parser APIs may change between releases."]
pub mod parsers;
pub mod query;
#[doc = "Compatibility module for source registry internals. Prefer `llmusage::sources`."]
pub mod registry;
#[doc = "Compatibility module for runtime internals. Prefer `app`, `paths`, and `logging`."]
pub mod runtime;
pub mod store;
pub mod sync;
#[cfg(any(feature = "testing", test))]
pub mod testing;
#[doc = "Compatibility module for terminal UI internals. Rendering internals may change between releases."]
pub mod tui;
#[doc = "Compatibility module for web server internals. Prefer `Dashboard` for library queries."]
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
pub use domain::models::SourceKind;
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
pub use store::{BootstrapOptions, HolderKind, Store, WorkerLock};
pub use sync::{
    JobEvent, JobId, JobRegistry, JobSnapshot, JobStartRejected, JobStatus, SyncOptions,
};

#[cfg(any(feature = "testing", test))]
pub use testing::{Fixture, SeedEvent};

use anyhow::Result as AnyhowResult;
use clap::Parser;

pub async fn run() -> AnyhowResult<()> {
    if let Some(language) = commands::help::is_top_level_help_request(std::env::args().skip(1)) {
        print!("{}", commands::help::top_level_help(language));
        return Ok(());
    }
    let cli = commands::Cli::parse();
    let app = runtime::app::AppContext::with_cli_home(cli.home.clone())?;
    runtime::logging::init_logging_for_paths(&app.paths)?;
    commands::dispatch(app, cli).await
}
