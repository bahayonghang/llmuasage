pub mod app;
pub mod commands;
pub mod export;
pub mod integrations;
pub mod logging;
pub mod models;
pub mod parsers;
pub mod paths;
pub mod project;
pub mod query;
pub mod store;
pub mod tui;
pub mod web;

use anyhow::Result;
use clap::Parser;

pub async fn run() -> Result<()> {
    logging::init_logging()?;
    let cli = commands::Cli::parse();
    let app = app::AppContext::discover()?;
    commands::dispatch(app, cli).await
}
