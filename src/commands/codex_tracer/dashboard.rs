//! Dashboard generation for codex-tracer.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::models::CodexTracerEvent;
use super::store::CodexTracerStore;

/// Dashboard payload structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct DashboardPayload {
    /// List of calls
    pub calls: Vec<CodexTracerEvent>,
    /// Metadata
    pub metadata: DashboardMetadata,
}

/// Dashboard metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct DashboardMetadata {
    /// Generation timestamp
    pub generated_at: String,
    /// Schema version
    pub schema: String,
    /// Total events count
    pub total_events: usize,
}

/// Generate a static dashboard HTML file.
pub fn generate_dashboard(store: &CodexTracerStore, output_dir: &Path) -> Result<PathBuf> {
    // 1. Query all events (for now, we'll add filtering later)
    let calls = store.query_calls(&super::store::CallFilters {
        limit: Some(10000), // Limit to avoid huge payloads
        ..Default::default()
    })?;

    // 2. Create payload
    let payload = DashboardPayload {
        calls: calls.clone(),
        metadata: DashboardMetadata {
            generated_at: chrono::Utc::now().to_rfc3339(),
            schema: "codex-tracer-v1".to_string(),
            total_events: calls.len(),
        },
    };

    // 3. Serialize payload to JSON
    let payload_json =
        serde_json::to_string(&payload).context("Failed to serialize dashboard payload")?;

    // 4. Load HTML template
    let template = include_str!("dashboard/dashboard_template.html");

    // 5. Replace placeholders
    let html = template
        .replace("__HTML_LANG__", "en")
        .replace("__HTML_DIR__", "ltr")
        .replace("__TITLE__", "Codex Tracer Dashboard")
        .replace("__STYLESHEET_LINKS__", &inline_css())
        .replace(
            "__BODY_ATTRS__",
            &format!(" data-dashboard-payload='{}'", escape_html(&payload_json)),
        )
        .replace("__GUIDE_LINK__", "");

    // 6. Write HTML file
    fs::create_dir_all(output_dir).context("Failed to create output directory")?;
    let dashboard_path = output_dir.join("dashboard.html");
    fs::write(&dashboard_path, html)
        .with_context(|| format!("Failed to write dashboard to {}", dashboard_path.display()))?;

    // 7. Copy JS assets
    copy_dashboard_assets(output_dir)?;

    Ok(dashboard_path)
}

/// Inline CSS styles into the HTML.
fn inline_css() -> String {
    let css = include_str!("dashboard/dashboard.css");
    format!("<style>{}</style>", css)
}

/// Escape HTML special characters for attribute values.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Copy JavaScript assets to the output directory.
fn copy_dashboard_assets(output_dir: &Path) -> Result<()> {
    let js_files = [
        ("dashboard.js", include_str!("dashboard/dashboard.js")),
        (
            "dashboard_actions.js",
            include_str!("dashboard/dashboard_actions.js"),
        ),
        (
            "dashboard_analysis.js",
            include_str!("dashboard/dashboard_analysis.js"),
        ),
        (
            "dashboard_call_diagnostics.js",
            include_str!("dashboard/dashboard_call_diagnostics.js"),
        ),
        (
            "dashboard_call_investigator.js",
            include_str!("dashboard/dashboard_call_investigator.js"),
        ),
        (
            "dashboard_cells.js",
            include_str!("dashboard/dashboard_cells.js"),
        ),
        (
            "dashboard_data.js",
            include_str!("dashboard/dashboard_data.js"),
        ),
        (
            "dashboard_details.js",
            include_str!("dashboard/dashboard_details.js"),
        ),
        (
            "dashboard_events.js",
            include_str!("dashboard/dashboard_events.js"),
        ),
        (
            "dashboard_filters.js",
            include_str!("dashboard/dashboard_filters.js"),
        ),
        (
            "dashboard_format.js",
            include_str!("dashboard/dashboard_format.js"),
        ),
        (
            "dashboard_insights.js",
            include_str!("dashboard/dashboard_insights.js"),
        ),
        (
            "dashboard_i18n.js",
            include_str!("dashboard/dashboard_i18n.js"),
        ),
        (
            "dashboard_live.js",
            include_str!("dashboard/dashboard_live.js"),
        ),
        (
            "dashboard_payload_cache.js",
            include_str!("dashboard/dashboard_payload_cache.js"),
        ),
        (
            "dashboard_state.js",
            include_str!("dashboard/dashboard_state.js"),
        ),
        (
            "dashboard_status.js",
            include_str!("dashboard/dashboard_status.js"),
        ),
        (
            "dashboard_tables.js",
            include_str!("dashboard/dashboard_tables.js"),
        ),
        (
            "dashboard_tooltips.js",
            include_str!("dashboard/dashboard_tooltips.js"),
        ),
    ];

    for (filename, content) in &js_files {
        let file_path = output_dir.join(filename);
        fs::write(&file_path, content).with_context(|| format!("Failed to write {}", filename))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("hello"), "hello");
        assert_eq!(escape_html("<div>"), "&lt;div&gt;");
        assert_eq!(escape_html("\"quote\""), "&quot;quote&quot;");
        assert_eq!(escape_html("a&b"), "a&amp;b");
    }

    #[test]
    fn test_generate_dashboard_basic() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = CodexTracerStore::open(&db_path).unwrap();

        let output_dir = tempdir().unwrap();
        let dashboard_path = generate_dashboard(&store, output_dir.path()).unwrap();

        assert!(dashboard_path.exists());
        assert_eq!(dashboard_path.file_name().unwrap(), "dashboard.html");

        // Verify JS files were copied
        assert!(output_dir.path().join("dashboard.js").exists());
        assert!(output_dir.path().join("dashboard_actions.js").exists());
    }
}
