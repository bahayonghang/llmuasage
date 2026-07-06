//! Codex tracer - detailed Codex usage tracking and analysis.
//!
//! This module provides enhanced tracking for Codex usage with fine-grained
//! token accounting, thread tracking, and a dedicated dashboard.

use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing::info;

use crate::app::AppContext;

pub mod dashboard;
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

        // Parse all JSONL files
        let mut all_events = Vec::new();
        let mut file_count = 0;
        let mut error_count = 0;

        for entry in walkdir::WalkDir::new(&rollout_dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }

            file_count += 1;
            match parse_codex_jsonl_for_tracer(path) {
                Ok(events) => {
                    info!(file = %path.display(), events = events.len(), "Parsed JSONL file");
                    all_events.extend(events);
                }
                Err(err) => {
                    tracing::warn!(file = %path.display(), error = %err, "Failed to parse JSONL file");
                    error_count += 1;
                }
            }
        }

        info!(
            files = file_count,
            events = all_events.len(),
            errors = error_count,
            "Finished parsing JSONL files"
        );

        if all_events.is_empty() {
            anyhow::bail!(
                "No events found in {}\n\
                 Please ensure you have used Codex at least once.",
                rollout_dir.display()
            );
        }

        // Insert events into database
        let inserted = store.upsert_events(&all_events)?;
        info!(inserted = inserted, "Inserted events into database");

        println!(
            "Parsed {} files, found {} events",
            file_count,
            all_events.len()
        );
        if error_count > 0 {
            println!("Warning: {} files failed to parse", error_count);
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
