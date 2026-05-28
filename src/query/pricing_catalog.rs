use std::path::Path;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
/// case-insensitive exact, dash-delimited prefix, or controlled dot-delimited
/// suffix checks against the normalized model name.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningPolicy {
    /// Reasoning tokens are already represented in the provider output token
    /// charge. Keep them as a display/audit sub-channel and do not bill them
    /// a second time.
    #[default]
    IncludedInOutput,
    /// Reasoning tokens are billed through a dedicated catalog rate.
    Separate,
}

impl ReasoningPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IncludedInOutput => "included_in_output",
            Self::Separate => "separate",
        }
    }
}

/// One rate row inside a [`PricingCatalog`]. Matchers run as
/// case-insensitive exact, dash-delimited prefix, or controlled dot-delimited
/// suffix checks against the normalized model name.
#[derive(Debug, Clone, Deserialize)]
pub struct PricingEntry {
    pub source: String,
    pub matchers: Vec<String>,
    pub input_per_mtok: f64,
    pub cached_per_mtok: f64,
    #[serde(default)]
    pub cache_creation_per_mtok: Option<f64>,
    pub output_per_mtok: f64,
    #[serde(default)]
    pub reasoning_per_mtok: Option<f64>,
    #[serde(default)]
    pub reasoning_policy: ReasoningPolicy,
}

impl PricingEntry {
    pub fn cache_creation_per_mtok(&self) -> f64 {
        self.cache_creation_per_mtok.unwrap_or(self.input_per_mtok)
    }

    pub fn reasoning_per_mtok(&self) -> f64 {
        self.reasoning_per_mtok.unwrap_or(self.output_per_mtok)
    }
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
            let file = catalog_file_from_str(STATIC_V1_JSON, Some("static-v1"))
                .expect("pricing/static-v1.json must be valid JSON");
            PricingCatalog::from_file(file, PricingStatus::Static)
                .expect("pricing/static-v1.json must contain non-overlapping matchers")
        })
    }

    /// Loads a litellm-style snapshot from a local path. URLs are
    /// rejected upstream by `doctor::refresh_pricing_catalog`.
    ///
    /// The accepted format is either the embedded static catalog shape (see
    /// `pricing/static-v1.json`) or a direct LiteLLM
    /// `model_prices_and_context_window.json` snapshot. Native LiteLLM
    /// per-token fields are converted to llmusage MTok rows on load.
    pub fn load_snapshot(path: &Path) -> Result<PricingCatalog> {
        let raw = std::fs::read_to_string(path)?;
        let fallback_version = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("litellm-snapshot");
        let file = catalog_file_from_str(&raw, Some(fallback_version))?;
        PricingCatalog::from_file(file, PricingStatus::Snapshot)
    }

    /// Finds the most specific entry whose matcher is either an exact model
    /// match, a dash-delimited prefix, or a controlled dot-delimited suffix
    /// for the requested source. This avoids accidental substring matches such
    /// as `gpt` matching `not-gpt` or `gpt2`, while still letting
    /// `gpt-5-mini` override the broader `gpt-5` row and `gpt-5` cover
    /// current dotted variants such as `gpt-5.5`.
    pub fn find(&self, source: &str, model: &str) -> Option<&PricingEntry> {
        let candidates = model_candidates(model);
        self.models
            .iter()
            .filter(|entry| entry.source.eq_ignore_ascii_case(source))
            .filter_map(|entry| {
                entry
                    .matchers
                    .iter()
                    .filter_map(|matcher| {
                        candidates
                            .iter()
                            .any(|candidate| matcher_matches(matcher, candidate))
                            .then_some((entry, matcher.len()))
                    })
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

fn catalog_file_from_str(raw: &str, fallback_version: Option<&str>) -> Result<CatalogFile> {
    let value: Value = serde_json::from_str(raw).map_err(|source| LlmusageError::Parse {
        context: "pricing snapshot",
        source,
    })?;
    catalog_file_from_value(value, fallback_version)
}

fn catalog_file_from_value(value: Value, fallback_version: Option<&str>) -> Result<CatalogFile> {
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| fallback_version.map(str::to_string))
        .unwrap_or_else(|| "litellm-snapshot".to_string());

    if value.get("models").and_then(Value::as_array).is_some() {
        #[derive(Deserialize)]
        struct InternalCatalogFile {
            version: Option<String>,
            models: Vec<PricingEntry>,
        }

        let file: InternalCatalogFile =
            serde_json::from_value(value).map_err(|source| LlmusageError::Parse {
                context: "pricing snapshot",
                source,
            })?;
        return Ok(CatalogFile {
            version: file
                .version
                .or_else(|| fallback_version.map(str::to_string))
                .unwrap_or_else(|| "litellm-snapshot".to_string()),
            models: file.models,
        });
    }

    let Some(root) = value.as_object() else {
        return Err(LlmusageError::ConfigInvalid {
            detail: "pricing snapshot must be a JSON object".to_string(),
        });
    };
    let model_root = value
        .get("models")
        .and_then(Value::as_object)
        .unwrap_or(root);

    let mut models = Vec::new();
    for (model_id, raw_entry) in model_root {
        if matches!(
            model_id.as_str(),
            "version" | "sample_spec" | "README" | "schema" | "metadata"
        ) {
            continue;
        }
        let Some(entry) = native_litellm_entry(model_id, raw_entry) else {
            continue;
        };
        models.extend(entry);
    }

    if models.is_empty() {
        return Err(LlmusageError::ConfigInvalid {
            detail: "pricing snapshot did not contain any LiteLLM rows with input_cost_per_token"
                .to_string(),
        });
    }

    Ok(CatalogFile { version, models })
}

fn native_litellm_entry(model_id: &str, raw_entry: &Value) -> Option<Vec<PricingEntry>> {
    let object = raw_entry.as_object()?;
    let input = read_f64(raw_entry, "input_cost_per_token")?;
    let output = read_f64(raw_entry, "output_cost_per_token").unwrap_or_default();
    let cached = read_f64(raw_entry, "cache_read_input_token_cost").unwrap_or(input);
    let cache_creation = read_f64(raw_entry, "cache_creation_input_token_cost");
    let reasoning = read_f64(raw_entry, "output_cost_per_reasoning_token");
    let reasoning_policy = read_reasoning_policy(raw_entry);
    let provider = object
        .get("litellm_provider")
        .and_then(Value::as_str)
        .or_else(|| object.get("provider").and_then(Value::as_str));
    let matchers = native_matchers(model_id, raw_entry);
    if matchers.is_empty() {
        return None;
    }

    let sources = native_sources(model_id, provider);
    if sources.is_empty() {
        return None;
    }

    Some(
        sources
            .into_iter()
            .map(|source| PricingEntry {
                source,
                matchers: matchers.clone(),
                input_per_mtok: input * 1_000_000.0,
                cached_per_mtok: cached * 1_000_000.0,
                cache_creation_per_mtok: cache_creation.map(|rate| rate * 1_000_000.0),
                output_per_mtok: output * 1_000_000.0,
                reasoning_per_mtok: reasoning.map(|rate| rate * 1_000_000.0),
                reasoning_policy,
            })
            .collect(),
    )
}

fn read_reasoning_policy(value: &Value) -> ReasoningPolicy {
    let raw = value
        .get("reasoning_policy")
        .or_else(|| value.get("llmusage_reasoning_policy"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match raw.as_str() {
        "separate" | "extra" | "additional" => ReasoningPolicy::Separate,
        _ => ReasoningPolicy::IncludedInOutput,
    }
}

fn native_matchers(model_id: &str, raw_entry: &Value) -> Vec<String> {
    let mut values = Vec::new();
    push_normalized_matcher(&mut values, model_id);
    if let Some(model) = raw_entry.get("model").and_then(Value::as_str) {
        push_normalized_matcher(&mut values, model);
    }
    values
}

fn push_matcher(values: &mut Vec<String>, raw: &str) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    for candidate in [
        trimmed.to_string(),
        trimmed
            .rsplit_once('/')
            .map(|(_, model)| model.to_string())
            .unwrap_or_else(|| trimmed.to_string()),
    ] {
        let normalized = candidate.to_ascii_lowercase();
        if !normalized.is_empty() && !values.iter().any(|value| value == &normalized) {
            values.push(normalized);
        }
    }
}

fn push_normalized_matcher(values: &mut Vec<String>, raw: &str) {
    push_matcher(values, raw);
    let normalized = normalize_litellm_model_id(raw);
    if normalized != raw.trim().to_ascii_lowercase() {
        push_matcher(values, &normalized);
    }
    for alias in pricing_aliases(&normalized) {
        push_matcher(values, alias);
    }
}

fn native_sources(model_id: &str, provider: Option<&str>) -> Vec<String> {
    let model = model_id.to_ascii_lowercase();
    let provider = provider.unwrap_or_default().to_ascii_lowercase();
    if provider.contains("anthropic") || model.contains("claude") {
        return vec!["claude".to_string(), "opencode".to_string()];
    }
    if provider.contains("google") || provider.contains("gemini") || model.contains("gemini") {
        return vec!["antigravity".to_string(), "opencode".to_string()];
    }
    if provider.contains("openai")
        || model.starts_with("gpt")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
    {
        return vec!["codex".to_string(), "opencode".to_string()];
    }
    Vec::new()
}

fn read_f64(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str().and_then(|raw| raw.parse().ok()))
    })
}

fn matcher_matches(matcher: &str, normalized_model: &str) -> bool {
    let matcher = normalize_model_candidate(matcher);
    if matcher.is_empty() {
        return false;
    }
    normalized_model == matcher
        || normalized_model.starts_with(&format!("{matcher}-"))
        || dot_suffix_matches(&matcher, normalized_model)
}

fn model_candidates(model: &str) -> Vec<String> {
    let normalized = normalize_model_candidate(model);
    let mut candidates = Vec::new();
    push_candidate(&mut candidates, normalized.as_str());
    for alias in pricing_aliases(&normalized) {
        push_candidate(&mut candidates, alias);
    }
    candidates
}

fn push_candidate(values: &mut Vec<String>, candidate: &str) {
    if !candidate.is_empty() && !values.iter().any(|value| value == candidate) {
        values.push(candidate.to_string());
    }
}

fn normalize_model_candidate(model: &str) -> String {
    let stripped = model
        .trim()
        .to_ascii_lowercase()
        .rsplit_once('/')
        .map(|(_, model)| model.to_string())
        .unwrap_or_else(|| model.trim().to_ascii_lowercase());
    normalize_litellm_model_id(&stripped)
}

fn normalize_litellm_model_id(model: &str) -> String {
    let mut normalized = model.trim().to_ascii_lowercase();
    for suffix in ["-thinking", "-latest"] {
        if let Some(stripped) = normalized.strip_suffix(suffix) {
            normalized = stripped.to_string();
        }
    }
    normalized
        .chars()
        .map(|ch| match ch {
            '.' | '_' | ':' => '-',
            _ => ch,
        })
        .collect()
}

fn pricing_aliases(model: &str) -> &'static [&'static str] {
    match model {
        "gpt-5-codex" => &["gpt-5"],
        "gemini-3-pro-high" => &["gemini-3-pro-preview"],
        _ => &[],
    }
}

fn dot_suffix_matches(matcher: &str, normalized_model: &str) -> bool {
    if normalized_model.starts_with(&format!("{matcher}-"))
        && normalized_model[matcher.len() + 1..]
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_digit())
    {
        return true;
    }
    if !normalized_model.starts_with(&format!("{matcher}.")) {
        return false;
    }
    matcher
        .rsplit_once('-')
        .is_some_and(|(_, suffix)| suffix.chars().all(|ch| ch.is_ascii_digit()))
}

fn same_matcher(left: &str, right: &str) -> bool {
    let left = normalize_model_candidate(left);
    let right = normalize_model_candidate(right);
    !left.is_empty() && left == right
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::{PricingCatalog, ReasoningPolicy};
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
        assert!(catalog.find("codex", "gpt-5.5").is_some());
        assert!(catalog.find("codex", "gpt-5.4").is_some());
        assert!(catalog.find("codex", "gpt-5.4-mini").is_some());
        assert!(catalog.find("codex", "o3-mini").is_some());
        assert!(catalog.find("claude", "claude-opus-4-1").is_some());
        assert!(catalog.find("claude", "claude-sonnet-4-5").is_some());
        assert!(catalog.find("opencode", "gpt-5").is_some());
        assert!(catalog.find("codex", "gpt-5-codex").is_some());
        assert!(catalog.find("opencode", "claude.sonnet.4.5").is_some());
        assert!(
            catalog
                .find("opencode", "anthropic/claude-sonnet-4-5")
                .is_some()
        );
        assert!(catalog.find("opencode", "gemini-3-pro-high").is_some());
        assert!(catalog.find("codex", "made-up-model").is_none());
        assert!(catalog.find("codex", "not-gpt-5").is_none());
        assert!(catalog.find("codex", "gpt-50").is_none());
        assert!(catalog.find("opencode", "gpt2").is_none());
    }

    #[test]
    fn pricing_catalog_prefix_matches_minor_variants() {
        let catalog = PricingCatalog::static_v1();
        let entry = catalog
            .find("codex", "gpt-5-mini-2026-05-01")
            .expect("dash-delimited minor variant should match gpt-5-mini");
        assert_eq!(entry.input_per_mtok, 0.25);

        let dotted = catalog
            .find("codex", "gpt-5.4")
            .expect("dot-delimited GPT-5 variant should match gpt-5");
        assert_eq!(dotted.input_per_mtok, 1.25);

        let dotted_mini = catalog
            .find("codex", "gpt-5.4-mini")
            .expect("dot-delimited GPT-5 mini variant should match gpt-5");
        assert_eq!(dotted_mini.input_per_mtok, 1.25);

        let base = catalog
            .find("codex", "gpt-5")
            .expect("exact matcher should still match base model");
        assert_eq!(base.input_per_mtok, 1.25);
    }

    #[test]
    fn pricing_catalog_matches_known_provider_aliases() {
        let catalog = PricingCatalog::static_v1();
        let codex = catalog
            .find("codex", "gpt-5-codex")
            .expect("Codex-specific GPT-5 alias should match gpt-5");
        assert_eq!(codex.input_per_mtok, 1.25);

        let claude = catalog
            .find("opencode", "anthropic/claude.sonnet.4.5")
            .expect("provider-prefixed dotted Claude IDs should normalize");
        assert_eq!(claude.output_per_mtok, 15.0);

        let gemini = catalog
            .find("opencode", "gemini-3-pro-high")
            .expect("Gemini high alias should match preview pricing");
        assert_eq!(gemini.input_per_mtok, 2.0);
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
    fn pricing_catalog_loads_native_litellm_snapshot() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{
                "gpt-5": {{
                    "litellm_provider": "openai",
                    "input_cost_per_token": 0.00000125,
                    "output_cost_per_token": 0.000010,
                    "cache_creation_input_token_cost": 0.00000125,
                    "cache_read_input_token_cost": 0.000000125,
                    "output_cost_per_reasoning_token": 0.000010
                }},
                "anthropic/claude-sonnet-4-5": {{
                    "litellm_provider": "anthropic",
                    "input_cost_per_token": "0.000003",
                    "output_cost_per_token": "0.000015",
                    "cache_creation_input_token_cost": "0.00000375",
                    "cache_read_input_token_cost": "0.0000003"
                }}
            }}"#
        )?;
        tmp.flush()?;

        let catalog = PricingCatalog::load_snapshot(tmp.path())?;
        assert_eq!(catalog.status, PricingStatus::Snapshot);

        let gpt = catalog.find("codex", "gpt-5").expect("gpt-5 present");
        assert!((gpt.input_per_mtok - 1.25).abs() < 1e-12);
        assert!((gpt.cached_per_mtok - 0.125).abs() < 1e-12);
        assert!((gpt.cache_creation_per_mtok() - 1.25).abs() < 1e-12);
        assert!((gpt.output_per_mtok - 10.0).abs() < 1e-12);
        assert!((gpt.reasoning_per_mtok() - 10.0).abs() < 1e-12);
        assert_eq!(gpt.reasoning_policy, ReasoningPolicy::IncludedInOutput);
        assert!(catalog.find("opencode", "gpt-5").is_some());

        let claude = catalog
            .find("claude", "claude-sonnet-4-5")
            .expect("provider-prefixed matcher should be normalized");
        assert!((claude.input_per_mtok - 3.0).abs() < 1e-12);
        assert!((claude.cached_per_mtok - 0.3).abs() < 1e-12);
        assert!((claude.cache_creation_per_mtok() - 3.75).abs() < 1e-12);
        assert_eq!(claude.reasoning_policy, ReasoningPolicy::IncludedInOutput);
        assert!(
            catalog
                .find("opencode", "anthropic/claude.sonnet.4.5")
                .is_some(),
            "native snapshot provider-prefixed candidates should normalize for OpenCode"
        );

        Ok(())
    }

    #[test]
    fn pricing_catalog_honors_explicit_native_reasoning_policy() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{
                "gpt-reasoning-extra": {{
                    "litellm_provider": "openai",
                    "input_cost_per_token": 0.00000125,
                    "output_cost_per_token": 0.000010,
                    "output_cost_per_reasoning_token": 0.000020,
                    "reasoning_policy": "separate"
                }}
            }}"#
        )?;
        tmp.flush()?;

        let catalog = PricingCatalog::load_snapshot(tmp.path())?;
        let entry = catalog
            .find("codex", "gpt-reasoning-extra")
            .expect("explicit reasoning policy row should load");
        assert!((entry.reasoning_per_mtok() - 20.0).abs() < 1e-12);
        assert_eq!(entry.reasoning_policy, ReasoningPolicy::Separate);
        Ok(())
    }

    #[test]
    fn pricing_catalog_loads_native_litellm_models_object() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{
                "version": "wrapped-litellm",
                "models": {{
                    "gpt-5": {{
                        "litellm_provider": "openai",
                        "input_cost_per_token": 0.00000125,
                        "output_cost_per_token": 0.000010,
                        "cache_read_input_token_cost": 0.000000125
                    }}
                }}
            }}"#
        )?;
        tmp.flush()?;

        let catalog = PricingCatalog::load_snapshot(tmp.path())?;
        assert_eq!(catalog.version, "wrapped-litellm");
        assert!(catalog.find("codex", "gpt-5").is_some());
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
