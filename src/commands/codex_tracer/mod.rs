//! Codex tracer - detailed Codex usage tracking and analysis.
//!
//! This module provides enhanced tracking for Codex usage with fine-grained
//! token accounting, thread tracking, and a dedicated dashboard.

use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::app::AppContext;

use self::ingest::{CodexTracerIngestOptions, ingest_rollout_dir};

pub mod dashboard;
pub(crate) mod ingest;
pub mod models;
pub mod parser;
pub mod server;
pub mod store;

pub use dashboard::generate_dashboard;
pub use models::{CodexTracerEvent, ThreadSummary};
pub use parser::{FileParseState, parse_codex_jsonl_for_tracer, parse_codex_jsonl_with_state};
pub use server::serve_dashboard;
pub use store::{CallFilters, CodexTracerStore};

/// Run the codex-tracer command.
pub async fn run(app: &AppContext, port: u16, open_browser: bool, rebuild: bool) -> Result<()> {
    info!(port, open_browser, rebuild, "Starting codex-tracer");

    // Database path
    let db_path = app.paths.root_dir.join("codex-tracer.db");

    // If rebuild is requested, delete the existing database
    if rebuild && db_path.exists() {
        info!("Rebuild requested, removing existing database");
        std::fs::remove_file(&db_path).context("Failed to remove existing database")?;
    }

    // Open or create the database
    let mut store =
        CodexTracerStore::open(&db_path).context("Failed to open codex-tracer database")?;

    // Parse JSONL files if database is empty or rebuild was requested
    let event_count = store.count_events()?;
    if event_count == 0 || rebuild {
        info!("Parsing Codex JSONL files");

        // Determine Codex rollout directory
        let codex_home = match std::env::var("CODEX_HOME") {
            Ok(path) => PathBuf::from(path),
            Err(_) => {
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                home.join(".codex")
            }
        };

        let rollout_dir = codex_home.join("rollout");

        if !rollout_dir.exists() {
            anyhow::bail!(
                "Codex rollout directory not found: {}\n\
                 Please ensure Codex is installed and has been used at least once.\n\
                 You can set CODEX_HOME to specify a custom location.",
                rollout_dir.display()
            );
        }

        let stats = ingest_rollout_dir(
            &mut store,
            &rollout_dir,
            &CancellationToken::new(),
            CodexTracerIngestOptions::default(),
        )?;

        info!(
            files = stats.files_seen,
            records = stats.records_read,
            events = stats.events_found,
            rows_written = stats.rows_written,
            batch_peak = stats.batch_peak,
            errors = stats.errors,
            "Finished parsing JSONL files"
        );

        if store.count_events()? == 0 {
            anyhow::bail!(
                "No events found in {}\n\
                 Please ensure you have used Codex at least once.",
                rollout_dir.display()
            );
        }

        println!(
            "Parsed {} files, found {} events",
            stats.files_seen, stats.events_found
        );
        if stats.errors > 0 {
            println!("Warning: {} files failed to parse", stats.errors);
        }
    } else {
        info!(events = event_count, "Database already contains events");
        println!("Database contains {} events", event_count);
    }

    // Start the web server
    println!("Starting Codex Tracer dashboard on port {}...", port);
    serve_dashboard(db_path, port, open_browser).await?;

    Ok(())
}
