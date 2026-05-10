use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;

use super::pricing::PricingStatus;
use crate::error::{LlmusageError, Result};

/// Embedded baseline catalog shipped with the binary.
///
/// `pricing/static-v1.json` is the only authoritative source of static-v1
/// rates: keep it in sync with the matchers expected by tests under
/// `query/pricing.rs`. Litellm snapshots replace it at runtime via
/// [`PricingCatalog::load_snapshot`].
const STATIC_V1_JSON: &str = include_str!("../../pricing/static-v1.json");

/// One rate row inside a [`PricingCatalog`]. Matchers run as
/// case-insensitive exact or dash-delimited prefix checks against the
/// normalized model name.
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
            PricingCatalog::from_file(file, PricingStatus::Static)
                .expect("pricing/static-v1.json must contain non-overlapping matchers")
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
        let file: CatalogFile =
            serde_json::from_str(&raw).map_err(|source| LlmusageError::Parse {
                context: "pricing snapshot",
                source,
            })?;
        PricingCatalog::from_file(file, PricingStatus::Snapshot)
    }

    /// Finds the most specific entry whose matcher is either an exact model
    /// match or a dash-delimited prefix for the requested source. This avoids
    /// accidental substring matches such as `gpt` matching `not-gpt` or `gpt2`,
    /// while still letting `gpt-5-mini` override the broader `gpt-5` row.
    pub fn find(&self, source: &str, model: &str) -> Option<&PricingEntry> {
        let normalized = model.to_ascii_lowercase();
        self.models
            .iter()
            .filter(|entry| entry.source.eq_ignore_ascii_case(source))
            .filter_map(|entry| {
                entry
                    .matchers
                    .iter()
                    .filter(|matcher| matcher_matches(matcher, &normalized))
                    .map(|matcher| (entry, matcher.len()))
                    .max_by_key(|(_, matcher_len)| *matcher_len)
            })
            .max_by_key(|(_, matcher_len)| *matcher_len)
            .map(|(entry, _)| entry)
    }

    fn from_file(file: CatalogFile, status: PricingStatus) -> Result<PricingCatalog> {
        let catalog = PricingCatalog {
            version: file.version,
            status,
            models: file.models,
        };
        catalog.validate_non_overlapping_matchers()?;
        Ok(catalog)
    }

    fn validate_non_overlapping_matchers(&self) -> Result<()> {
        for (left_index, left) in self.models.iter().enumerate() {
            for (right_index, right) in self.models.iter().enumerate().skip(left_index + 1) {
                if !left.source.eq_ignore_ascii_case(&right.source) {
                    continue;
                }

                for left_matcher in &left.matchers {
                    for right_matcher in &right.matchers {
                        if same_matcher(left_matcher, right_matcher) {
                            return Err(LlmusageError::ConfigInvalid {
                                detail: format!(
                                    "pricing catalog {} has duplicate matcher for source '{}': '{}' in entry {} duplicates '{}' in entry {}",
                                    self.version,
                                    left.source,
                                    left_matcher,
                                    left_index,
                                    right_matcher,
                                    right_index
                                ),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn matcher_matches(matcher: &str, normalized_model: &str) -> bool {
    let matcher = matcher.to_ascii_lowercase();
    if matcher.is_empty() {
        return false;
    }
    normalized_model == matcher || normalized_model.starts_with(&format!("{matcher}-"))
}

fn same_matcher(left: &str, right: &str) -> bool {
    let left = left.to_ascii_lowercase();
    let right = right.to_ascii_lowercase();
    !left.is_empty() && left == right
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::PricingCatalog;
    use crate::error::LlmusageError;
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
        assert!(catalog.find("codex", "not-gpt-5").is_none());
        assert!(catalog.find("opencode", "gpt2").is_none());
    }

    #[test]
    fn pricing_catalog_prefix_matches_minor_variants() {
        let catalog = PricingCatalog::static_v1();
        let entry = catalog
            .find("codex", "gpt-5-mini-2026-05-01")
            .expect("dash-delimited minor variant should match gpt-5-mini");
        assert_eq!(entry.input_per_mtok, 0.25);

        let base = catalog
            .find("codex", "gpt-5")
            .expect("exact matcher should still match base model");
        assert_eq!(base.input_per_mtok, 1.25);
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

    #[test]
    fn pricing_catalog_duplicate_matchers_rejected() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{
                "version": "overlap-test",
                "models": [
                    {{
                        "source": "codex",
                        "matchers": ["gpt-5"],
                        "input_per_mtok": 1.25,
                        "cached_per_mtok": 0.125,
                        "output_per_mtok": 10.0
                    }},
                    {{
                        "source": "codex",
                        "matchers": ["gpt-5"],
                        "input_per_mtok": 0.25,
                        "cached_per_mtok": 0.025,
                        "output_per_mtok": 2.0
                    }}
                ]
            }}"#
        )?;
        tmp.flush()?;

        let err = PricingCatalog::load_snapshot(tmp.path())
            .expect_err("duplicate pricing matchers should be rejected at load time");
        assert!(matches!(err, LlmusageError::ConfigInvalid { .. }));
        assert!(err.to_string().contains("duplicate matcher"));
        Ok(())
    }

    #[test]
    fn pricing_catalog_invalid_snapshot_returns_parse_context() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        write!(tmp, "{{not-json")?;
        tmp.flush()?;

        let err = PricingCatalog::load_snapshot(tmp.path())
            .expect_err("malformed pricing snapshot should return Parse");
        assert!(matches!(
            err,
            LlmusageError::Parse {
                context: "pricing snapshot",
                ..
            }
        ));
        Ok(())
    }
}
