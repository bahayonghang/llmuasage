use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;

use super::pricing::PricingStatus;
use crate::error::Result;

/// Embedded baseline catalog shipped with the binary.
///
/// `pricing/static-v1.json` is the only authoritative source of static-v1
/// rates: keep it in sync with the matchers expected by tests under
/// `query/pricing.rs`. Litellm snapshots replace it at runtime via
/// [`PricingCatalog::load_snapshot`].
const STATIC_V1_JSON: &str = include_str!("../../pricing/static-v1.json");

/// One rate row inside a [`PricingCatalog`]. Matchers run as
/// case-insensitive `contains` checks against the normalized model name.
#[derive(Debug, Clone, Deserialize)]
pub struct PricingEntry {
    pub source: String,
    pub matchers: Vec<String>,
    pub input_per_mtok: f64,
    pub cached_per_mtok: f64,
    pub output_per_mtok: f64,
}

/// In-memory pricing catalog (D6/F1.3) loaded either from the embedded
/// static-v1 JSON or a litellm snapshot file written by
/// `llmusage doctor --refresh-pricing`.
#[derive(Debug, Clone)]
pub struct PricingCatalog {
    pub version: String,
    pub status: PricingStatus,
    pub models: Vec<PricingEntry>,
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    version: String,
    models: Vec<PricingEntry>,
}

impl PricingCatalog {
    /// Returns the embedded static-v1 catalog. The file is parsed at most
    /// once per process; subsequent callers share the cached value.
    pub fn static_v1() -> &'static PricingCatalog {
        static CATALOG: OnceLock<PricingCatalog> = OnceLock::new();
        CATALOG.get_or_init(|| {
            let file: CatalogFile = serde_json::from_str(STATIC_V1_JSON)
                .expect("pricing/static-v1.json must be valid JSON");
            PricingCatalog {
                version: file.version,
                status: PricingStatus::Static,
                models: file.models,
            }
        })
    }

    /// Loads a litellm-style snapshot from a local path. URLs are
    /// rejected upstream by `doctor::refresh_pricing_catalog`.
    ///
    /// The accepted format mirrors the embedded static catalog (see
    /// `pricing/static-v1.json`). Direct upstream litellm
    /// `model_prices_and_context_window.json` adaptation is a follow-up
    /// patch — point users at the project README for the schema.
    pub fn load_snapshot(path: &Path) -> Result<PricingCatalog> {
        let raw = std::fs::read_to_string(path)?;
        let file: CatalogFile = serde_json::from_str(&raw)?;
        Ok(PricingCatalog {
            version: file.version,
            status: PricingStatus::Snapshot,
            models: file.models,
        })
    }

    /// Finds the first entry whose matchers contain the (case-insensitive)
    /// model substring for the requested source.
    pub fn find(&self, source: &str, model: &str) -> Option<&PricingEntry> {
        let normalized = model.to_ascii_lowercase();
        self.models.iter().find(|entry| {
            entry.source.eq_ignore_ascii_case(source)
                && entry
                    .matchers
                    .iter()
                    .any(|matcher| !matcher.is_empty() && normalized.contains(matcher))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::PricingCatalog;
    use crate::query::pricing::PricingStatus;

    /// The shipped static-v1 catalog must parse cleanly and cover the
    /// matchers the test suite asserts against (`gpt-5`, `claude-sonnet`,
    /// `opus`, `o3`).
    #[test]
    fn pricing_catalog_loads_static_v1() {
        let catalog = PricingCatalog::static_v1();
        assert_eq!(catalog.version, "static-v1");
        assert_eq!(catalog.status, PricingStatus::Static);
        assert!(catalog.find("codex", "gpt-5").is_some());
        assert!(catalog.find("codex", "o3-mini").is_some());
        assert!(catalog.find("claude", "claude-opus-4-1").is_some());
        assert!(catalog.find("claude", "claude-sonnet-4-5").is_some());
        assert!(catalog.find("opencode", "gpt-5").is_some());
        assert!(catalog.find("codex", "made-up-model").is_none());
    }

    /// Litellm snapshots loaded from disk inherit the snapshot status and
    /// the version label baked into the file. doctor uses this to mark
    /// `pricing_source` so dashboards can tell static vs snapshot rows
    /// apart even after `recompute_costs` lands the new rates.
    #[test]
    fn pricing_catalog_loads_litellm_snapshot() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{
                "version": "litellm-snapshot-2026-05",
                "models": [
                    {{
                        "source": "codex",
                        "matchers": ["gpt-5"],
                        "input_per_mtok": 1.5,
                        "cached_per_mtok": 0.15,
                        "output_per_mtok": 12.0
                    }}
                ]
            }}"#
        )?;
        tmp.flush()?;

        let catalog = PricingCatalog::load_snapshot(tmp.path())?;
        assert_eq!(catalog.version, "litellm-snapshot-2026-05");
        assert_eq!(catalog.status, PricingStatus::Snapshot);
        let entry = catalog.find("codex", "gpt-5").expect("gpt-5 present");
        assert!((entry.input_per_mtok - 1.5).abs() < f64::EPSILON);
        Ok(())
    }
}
