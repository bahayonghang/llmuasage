use std::{collections::HashSet, path::Path, sync::OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::pricing::PricingStatus;
use crate::error::{LlmusageError, Result};

/// Embedded baseline catalog shipped with the binary.
const STATIC_V2_JSON: &str = include_str!("../../pricing/static-v2.json");
const CATALOG_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, Default, Deserialize, Hash, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, Copy, Default, Deserialize, Hash, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    Exact,
    #[default]
    Family,
}

impl MatchMode {
    fn priority(self) -> u8 {
        match self {
            Self::Exact => 1,
            Self::Family => 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct PricingMatcher {
    pub value: String,
    #[serde(default)]
    pub mode: MatchMode,
}

impl PricingMatcher {
    pub fn exact(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            mode: MatchMode::Exact,
        }
    }

    pub fn family(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            mode: MatchMode::Family,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct PricingRate {
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

impl PricingRate {
    pub fn new(input_per_mtok: f64, cached_per_mtok: f64, output_per_mtok: f64) -> Self {
        Self {
            input_per_mtok,
            cached_per_mtok,
            cache_creation_per_mtok: None,
            output_per_mtok,
            reasoning_per_mtok: None,
            reasoning_policy: ReasoningPolicy::default(),
        }
    }

    pub fn cache_creation_per_mtok(&self) -> f64 {
        self.cache_creation_per_mtok.unwrap_or(self.input_per_mtok)
    }

    pub fn reasoning_per_mtok(&self) -> f64 {
        self.reasoning_per_mtok.unwrap_or(self.output_per_mtok)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct PricingTier {
    pub name: String,
    pub prompt_tokens_above: u64,
    #[serde(flatten)]
    pub rate: PricingRate,
}

#[derive(Debug, Clone, Copy)]
pub struct SelectedPricingRate<'a> {
    pub tier: &'a str,
    pub prompt_tokens_above: Option<u64>,
    pub rate: &'a PricingRate,
}

/// One compiled source-specific rule inside a [`PricingCatalog`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct PricingEntry {
    pub id: String,
    pub source: String,
    pub matches: Vec<PricingMatcher>,
    pub default_rate: PricingRate,
    pub tiers: Vec<PricingTier>,
    /// Maximum context window (prompt-side capacity) in tokens, when known.
    /// Drives context-window utilization; `None` degrades to "unknown".
    pub context_window: Option<u64>,
}

impl PricingEntry {
    pub fn new(
        id: impl Into<String>,
        source: impl Into<String>,
        matches: Vec<PricingMatcher>,
        default_rate: PricingRate,
    ) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            matches,
            default_rate,
            tiers: Vec::new(),
            context_window: None,
        }
    }

    pub fn rate_for_prompt_tokens(&self, prompt_tokens: u64) -> SelectedPricingRate<'_> {
        self.tiers
            .iter()
            .rev()
            .find(|tier| prompt_tokens > tier.prompt_tokens_above)
            .map(|tier| SelectedPricingRate {
                tier: tier.name.as_str(),
                prompt_tokens_above: Some(tier.prompt_tokens_above),
                rate: &tier.rate,
            })
            .unwrap_or(SelectedPricingRate {
                tier: "default",
                prompt_tokens_above: None,
                rate: &self.default_rate,
            })
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CatalogKind {
    Base,
    Overlay,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub(crate) struct ModelRates {
    pub default: PricingRate,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tiers: Vec<PricingTier>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub(crate) struct ModelDefinition {
    pub id: String,
    pub sources: Vec<String>,
    pub matches: Vec<PricingMatcher>,
    pub rates: ModelRates,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub(crate) struct CatalogDocument {
    pub schema_version: u32,
    pub kind: CatalogKind,
    pub version: String,
    #[serde(default)]
    pub models: Vec<ModelDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove_models: Vec<String>,
}

impl CatalogDocument {
    pub(crate) fn canonical_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|source| LlmusageError::Parse {
            context: "pricing catalog serialization",
            source,
        })
    }

    pub(crate) fn model_count(&self) -> usize {
        self.models.len()
    }

    pub(crate) fn source_rule_count(&self) -> usize {
        self.models.iter().map(|model| model.sources.len()).sum()
    }
}

/// In-memory pricing catalog compiled from the embedded base, a complete
/// snapshot, or a base plus user overlay.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct PricingCatalog {
    pub version: String,
    pub status: PricingStatus,
    pub models: Vec<PricingEntry>,
    document: CatalogDocument,
}

#[derive(Debug, Deserialize)]
struct LegacyCatalogFile {
    version: Option<String>,
    models: Vec<LegacyPricingEntry>,
}

#[derive(Debug, Deserialize)]
struct LegacyPricingEntry {
    source: String,
    matchers: Vec<String>,
    input_per_mtok: f64,
    cached_per_mtok: f64,
    #[serde(default)]
    cache_creation_per_mtok: Option<f64>,
    output_per_mtok: f64,
    #[serde(default)]
    reasoning_per_mtok: Option<f64>,
    #[serde(default)]
    reasoning_policy: ReasoningPolicy,
    #[serde(default)]
    context_window: Option<u64>,
}

impl PricingCatalog {
    /// Returns the embedded static-v2 catalog. The file is parsed at most
    /// once per process; subsequent callers share the cached value.
    pub fn embedded() -> &'static PricingCatalog {
        static CATALOG: OnceLock<PricingCatalog> = OnceLock::new();
        CATALOG.get_or_init(|| {
            let document = catalog_document_from_str(STATIC_V2_JSON, Some("static-v2"))
                .expect("pricing/static-v2.json must be valid JSON");
            PricingCatalog::from_document(document, PricingStatus::Static)
                .expect("pricing/static-v2.json must contain valid non-overlapping rules")
        })
    }

    /// Compatibility wrapper retained for downstream callers compiled against
    /// the old API name. It now returns the current embedded catalog.
    #[deprecated(note = "use PricingCatalog::embedded()")]
    pub fn static_v1() -> &'static PricingCatalog {
        Self::embedded()
    }

    pub fn new(
        version: impl Into<String>,
        status: PricingStatus,
        models: Vec<PricingEntry>,
    ) -> Result<Self> {
        let version = version.into();
        let document = document_from_entries(&version, &models)?;
        let catalog = Self {
            version,
            status,
            models,
            document,
        };
        catalog.validate_compiled_matchers()?;
        Ok(catalog)
    }

    /// Loads a litellm-style snapshot from a local path. URLs are
    /// rejected upstream by `doctor::refresh_pricing_catalog`.
    ///
    /// The accepted format is either the legacy internal-v1 catalog shape,
    /// catalog v2, or a direct LiteLLM
    /// `model_prices_and_context_window.json` snapshot. Native LiteLLM
    /// per-token fields are converted to llmusage MTok rows on load.
    pub fn load_snapshot(path: &Path) -> Result<PricingCatalog> {
        let raw = std::fs::read_to_string(path)?;
        let fallback_version = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("litellm-snapshot");
        let document = catalog_document_from_str(&raw, Some(fallback_version))?;
        if document.kind != CatalogKind::Base {
            return Err(config_invalid("pricing snapshot must have kind `base`"));
        }
        PricingCatalog::from_document(document, PricingStatus::Snapshot)
    }

    pub(crate) fn load_overlay(path: &Path) -> Result<CatalogDocument> {
        let raw = std::fs::read_to_string(path)?;
        let document = catalog_document_from_str(&raw, None)?;
        if document.kind != CatalogKind::Overlay {
            return Err(config_invalid("pricing overlay must have kind `overlay`"));
        }
        validate_document(&document)?;
        Ok(document)
    }

    pub(crate) fn merge_overlay(&self, overlay: CatalogDocument) -> Result<Self> {
        if overlay.kind != CatalogKind::Overlay {
            return Err(config_invalid("pricing overlay must have kind `overlay`"));
        }
        validate_document(&overlay)?;

        let mut models = self.document.models.clone();
        for removed in &overlay.remove_models {
            let Some(index) = models.iter().position(|model| model.id == *removed) else {
                return Err(config_invalid(format!(
                    "pricing overlay removes unknown model id `{removed}`"
                )));
            };
            models.remove(index);
        }
        for replacement in overlay.models {
            if let Some(index) = models.iter().position(|model| model.id == replacement.id) {
                models[index] = replacement;
            } else {
                models.push(replacement);
            }
        }

        let document = CatalogDocument {
            schema_version: CATALOG_SCHEMA_VERSION,
            kind: CatalogKind::Base,
            version: format!("{}+{}", self.document.version, overlay.version),
            models,
            remove_models: Vec::new(),
        };
        Self::from_document(document, PricingStatus::Snapshot)
    }

    pub(crate) fn document(&self) -> &CatalogDocument {
        &self.document
    }

    pub(crate) fn declared_version(&self) -> &str {
        &self.document.version
    }

    pub(crate) fn set_runtime_identity(&mut self, version: String, status: PricingStatus) {
        self.version = version;
        self.status = status;
    }

    /// Finds the most specific entry whose matcher is either an exact model
    /// match, a dash-delimited prefix, or a controlled dot-delimited suffix
    /// for the requested source. This avoids accidental substring matches such
    /// as `gpt` matching `not-gpt` or `gpt2`, while still letting
    /// `gpt-5-mini` override the broader `gpt-5` row and `gpt-5` cover
    /// current dotted variants such as `gpt-5.5`.
    pub fn find(&self, source: &str, model: &str) -> Option<&PricingEntry> {
        let normalized = normalize_model_candidate(model);
        self.models
            .iter()
            .filter(|entry| entry.source.eq_ignore_ascii_case(source))
            .filter_map(|entry| {
                entry
                    .matches
                    .iter()
                    .filter(|matcher| matcher_matches(matcher, &normalized))
                    .map(|matcher| {
                        (
                            entry,
                            (
                                matcher.mode.priority(),
                                normalize_model_candidate(&matcher.value).len(),
                            ),
                        )
                    })
                    .max_by_key(|(_, score)| *score)
            })
            .max_by_key(|(_, score)| *score)
            .map(|(entry, _)| entry)
    }

    /// Returns the known maximum context window (in tokens) for the given
    /// source/model, or `None` when the catalog has no window for it.
    pub fn context_window(&self, source: &str, model: &str) -> Option<u64> {
        self.find(source, model)
            .and_then(|entry| entry.context_window)
            .filter(|window| *window > 0)
    }

    fn from_document(document: CatalogDocument, status: PricingStatus) -> Result<PricingCatalog> {
        validate_document(&document)?;
        if document.kind != CatalogKind::Base {
            return Err(config_invalid("only a base catalog can be compiled"));
        }

        let mut models = Vec::new();
        for model in &document.models {
            for source in &model.sources {
                models.push(PricingEntry {
                    id: model.id.clone(),
                    source: source.clone(),
                    matches: model.matches.clone(),
                    default_rate: model.rates.default.clone(),
                    tiers: model.rates.tiers.clone(),
                    context_window: model.context_window,
                });
            }
        }
        let catalog = PricingCatalog {
            version: document.version.clone(),
            status,
            models,
            document,
        };
        catalog.validate_compiled_matchers()?;
        Ok(catalog)
    }

    fn validate_compiled_matchers(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for entry in &self.models {
            for matcher in &entry.matches {
                let normalized = normalize_model_candidate(&matcher.value);
                let key = (entry.source.to_ascii_lowercase(), matcher.mode, normalized);
                if !seen.insert(key) {
                    return Err(config_invalid(format!(
                        "pricing catalog {} has duplicate {:?} matcher for source '{}': '{}'",
                        self.version, matcher.mode, entry.source, matcher.value
                    )));
                }
            }
        }
        Ok(())
    }
}

fn catalog_document_from_str(raw: &str, fallback_version: Option<&str>) -> Result<CatalogDocument> {
    let value: Value = serde_json::from_str(raw).map_err(|source| LlmusageError::Parse {
        context: "pricing snapshot",
        source,
    })?;
    catalog_document_from_value(value, fallback_version)
}

fn catalog_document_from_value(
    value: Value,
    fallback_version: Option<&str>,
) -> Result<CatalogDocument> {
    if value.get("schema_version").is_some() {
        let document: CatalogDocument =
            serde_json::from_value(value).map_err(|source| LlmusageError::Parse {
                context: "pricing catalog v2",
                source,
            })?;
        validate_document(&document)?;
        return Ok(document);
    }

    let version = value
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| fallback_version.map(str::to_string))
        .unwrap_or_else(|| "litellm-snapshot".to_string());

    if value.get("models").and_then(Value::as_array).is_some() {
        let file: LegacyCatalogFile =
            serde_json::from_value(value).map_err(|source| LlmusageError::Parse {
                context: "pricing snapshot",
                source,
            })?;
        let version = file
            .version
            .clone()
            .or_else(|| fallback_version.map(str::to_string))
            .unwrap_or_else(|| "litellm-snapshot".to_string());
        let models = file
            .models
            .into_iter()
            .enumerate()
            .map(|(index, entry)| legacy_model_definition(index, entry))
            .collect::<Result<Vec<_>>>()?;
        let document = CatalogDocument {
            schema_version: CATALOG_SCHEMA_VERSION,
            kind: CatalogKind::Base,
            version,
            models,
            remove_models: Vec::new(),
        };
        validate_document(&document)?;
        return Ok(document);
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
        let Some(entry) = native_litellm_model(model_id, raw_entry) else {
            continue;
        };
        models.push(entry);
    }

    if models.is_empty() {
        return Err(LlmusageError::ConfigInvalid {
            detail: "pricing snapshot did not contain any LiteLLM rows with input_cost_per_token"
                .to_string(),
        });
    }

    let document = CatalogDocument {
        schema_version: CATALOG_SCHEMA_VERSION,
        kind: CatalogKind::Base,
        version,
        models,
        remove_models: Vec::new(),
    };
    validate_document(&document)?;
    Ok(document)
}

fn legacy_model_definition(index: usize, entry: LegacyPricingEntry) -> Result<ModelDefinition> {
    let first_matcher = entry
        .matchers
        .first()
        .map(|matcher| normalize_model_candidate(matcher))
        .filter(|matcher| !matcher.is_empty())
        .ok_or_else(|| config_invalid(format!("legacy pricing entry {index} has no matcher")))?;
    let mut matches = entry
        .matchers
        .into_iter()
        .map(PricingMatcher::family)
        .collect::<Vec<_>>();
    if matches
        .iter()
        .any(|matcher| normalize_model_candidate(&matcher.value) == "gemini-3-pro-preview")
    {
        matches.push(PricingMatcher::exact("gemini-3-pro-high"));
    }
    Ok(ModelDefinition {
        id: format!(
            "legacy-{}-{first_matcher}",
            entry.source.to_ascii_lowercase()
        ),
        sources: vec![entry.source],
        matches,
        rates: ModelRates {
            default: PricingRate {
                input_per_mtok: entry.input_per_mtok,
                cached_per_mtok: entry.cached_per_mtok,
                cache_creation_per_mtok: entry.cache_creation_per_mtok,
                output_per_mtok: entry.output_per_mtok,
                reasoning_per_mtok: entry.reasoning_per_mtok,
                reasoning_policy: entry.reasoning_policy,
            },
            tiers: Vec::new(),
        },
        context_window: entry.context_window,
    })
}

fn native_litellm_model(model_id: &str, raw_entry: &Value) -> Option<ModelDefinition> {
    let object = raw_entry.as_object()?;
    let input = read_f64(raw_entry, "input_cost_per_token")?;
    let output = read_f64(raw_entry, "output_cost_per_token").unwrap_or_default();
    let cached = read_f64(raw_entry, "cache_read_input_token_cost").unwrap_or(input);
    let cache_creation = read_f64(raw_entry, "cache_creation_input_token_cost");
    let reasoning = read_f64(raw_entry, "output_cost_per_reasoning_token");
    let reasoning_policy = read_reasoning_policy(raw_entry);
    let context_window = read_u64(raw_entry, "max_input_tokens")
        .or_else(|| read_u64(raw_entry, "max_tokens"))
        .filter(|window| *window > 0);
    let provider = object
        .get("litellm_provider")
        .and_then(Value::as_str)
        .or_else(|| object.get("provider").and_then(Value::as_str));
    let matches = native_matchers(model_id, raw_entry);
    if matches.is_empty() {
        return None;
    }

    let sources = native_sources(model_id, provider);
    if sources.is_empty() {
        return None;
    }

    Some(ModelDefinition {
        id: format!("litellm-{}", stable_identifier(model_id)),
        sources,
        matches,
        rates: ModelRates {
            default: PricingRate {
                input_per_mtok: input * 1_000_000.0,
                cached_per_mtok: cached * 1_000_000.0,
                cache_creation_per_mtok: cache_creation.map(|rate| rate * 1_000_000.0),
                output_per_mtok: output * 1_000_000.0,
                reasoning_per_mtok: reasoning.map(|rate| rate * 1_000_000.0),
                reasoning_policy,
            },
            tiers: Vec::new(),
        },
        context_window,
    })
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

fn native_matchers(model_id: &str, raw_entry: &Value) -> Vec<PricingMatcher> {
    let mut values = Vec::new();
    push_normalized_matcher(&mut values, model_id);
    if let Some(model) = raw_entry.get("model").and_then(Value::as_str) {
        push_normalized_matcher(&mut values, model);
    }
    let mut normalized = HashSet::new();
    values
        .into_iter()
        .filter(|value| normalized.insert(normalize_model_candidate(value)))
        .map(PricingMatcher::family)
        .collect()
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

fn read_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_f64().map(|number| number as u64))
            .or_else(|| value.as_str().and_then(|raw| raw.parse().ok()))
    })
}

fn matcher_matches(matcher: &PricingMatcher, normalized_model: &str) -> bool {
    let normalized_matcher = normalize_model_candidate(&matcher.value);
    if normalized_matcher.is_empty() {
        return false;
    }
    match matcher.mode {
        MatchMode::Exact => normalized_model == normalized_matcher,
        MatchMode::Family => {
            normalized_model == normalized_matcher
                || normalized_model.starts_with(&format!("{normalized_matcher}-"))
        }
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

fn validate_document(document: &CatalogDocument) -> Result<()> {
    if document.schema_version != CATALOG_SCHEMA_VERSION {
        return Err(config_invalid(format!(
            "unsupported pricing catalog schema_version {}; expected {}",
            document.schema_version, CATALOG_SCHEMA_VERSION
        )));
    }
    if document.version.trim().is_empty() {
        return Err(config_invalid("pricing catalog version must not be empty"));
    }
    if document.kind == CatalogKind::Base && !document.remove_models.is_empty() {
        return Err(config_invalid("base pricing catalog cannot remove models"));
    }

    let mut ids = HashSet::new();
    for model in &document.models {
        validate_identifier("model id", &model.id)?;
        if !ids.insert(model.id.to_ascii_lowercase()) {
            return Err(config_invalid(format!(
                "pricing catalog has duplicate model id `{}`",
                model.id
            )));
        }
        if model.sources.is_empty() {
            return Err(config_invalid(format!(
                "pricing model `{}` must declare at least one source",
                model.id
            )));
        }
        let mut sources = HashSet::new();
        for source in &model.sources {
            validate_identifier("source", source)?;
            if !sources.insert(source.to_ascii_lowercase()) {
                return Err(config_invalid(format!(
                    "pricing model `{}` has duplicate source `{source}`",
                    model.id
                )));
            }
        }
        if model.matches.is_empty() {
            return Err(config_invalid(format!(
                "pricing model `{}` must declare at least one matcher",
                model.id
            )));
        }
        let mut matchers = HashSet::new();
        for matcher in &model.matches {
            let normalized = normalize_model_candidate(&matcher.value);
            if normalized.is_empty() {
                return Err(config_invalid(format!(
                    "pricing model `{}` contains an empty matcher",
                    model.id
                )));
            }
            if !matchers.insert((matcher.mode, normalized)) {
                return Err(config_invalid(format!(
                    "pricing model `{}` contains a duplicate {:?} matcher `{}`",
                    model.id, matcher.mode, matcher.value
                )));
            }
        }
        validate_rate(&model.id, "default", &model.rates.default)?;
        let mut threshold = 0;
        let mut tier_names = HashSet::new();
        for tier in &model.rates.tiers {
            if tier.name.trim().is_empty() || !tier_names.insert(tier.name.to_ascii_lowercase()) {
                return Err(config_invalid(format!(
                    "pricing model `{}` has an empty or duplicate tier name",
                    model.id
                )));
            }
            if tier.prompt_tokens_above == 0 || tier.prompt_tokens_above <= threshold {
                return Err(config_invalid(format!(
                    "pricing model `{}` tiers must use strictly increasing positive thresholds",
                    model.id
                )));
            }
            threshold = tier.prompt_tokens_above;
            validate_rate(&model.id, &tier.name, &tier.rate)?;
        }
        if model.context_window == Some(0) {
            return Err(config_invalid(format!(
                "pricing model `{}` context_window must be positive",
                model.id
            )));
        }
    }

    let mut removals = HashSet::new();
    for removed in &document.remove_models {
        validate_identifier("removed model id", removed)?;
        if !removals.insert(removed.to_ascii_lowercase()) {
            return Err(config_invalid(format!(
                "pricing overlay removes model `{removed}` more than once"
            )));
        }
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value != value.trim()
        || !value.chars().all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_' | '.')
        })
    {
        return Err(config_invalid(format!(
            "pricing catalog {label} `{value}` must use lowercase ASCII letters, digits, dot, dash, or underscore"
        )));
    }
    Ok(())
}

fn validate_rate(model_id: &str, tier: &str, rate: &PricingRate) -> Result<()> {
    for (name, value) in [
        ("input_per_mtok", rate.input_per_mtok),
        ("cached_per_mtok", rate.cached_per_mtok),
        ("cache_creation_per_mtok", rate.cache_creation_per_mtok()),
        ("output_per_mtok", rate.output_per_mtok),
        ("reasoning_per_mtok", rate.reasoning_per_mtok()),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(config_invalid(format!(
                "pricing model `{model_id}` tier `{tier}` has invalid {name}"
            )));
        }
    }
    Ok(())
}

fn document_from_entries(version: &str, entries: &[PricingEntry]) -> Result<CatalogDocument> {
    let mut models: Vec<ModelDefinition> = Vec::new();
    for entry in entries {
        if let Some(model) = models.iter_mut().find(|model| model.id == entry.id) {
            if model.matches != entry.matches
                || model.rates.default != entry.default_rate
                || model.rates.tiers != entry.tiers
                || model.context_window != entry.context_window
            {
                return Err(config_invalid(format!(
                    "pricing entries with id `{}` disagree across sources",
                    entry.id
                )));
            }
            model.sources.push(entry.source.clone());
        } else {
            models.push(ModelDefinition {
                id: entry.id.clone(),
                sources: vec![entry.source.clone()],
                matches: entry.matches.clone(),
                rates: ModelRates {
                    default: entry.default_rate.clone(),
                    tiers: entry.tiers.clone(),
                },
                context_window: entry.context_window,
            });
        }
    }
    let document = CatalogDocument {
        schema_version: CATALOG_SCHEMA_VERSION,
        kind: CatalogKind::Base,
        version: version.to_string(),
        models,
        remove_models: Vec::new(),
    };
    validate_document(&document)?;
    Ok(document)
}

fn stable_identifier(raw: &str) -> String {
    let mut value = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    while value.contains("--") {
        value = value.replace("--", "-");
    }
    value.trim_matches('-').to_string()
}

fn config_invalid(detail: impl Into<String>) -> LlmusageError {
    LlmusageError::ConfigInvalid {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::{PricingCatalog, ReasoningPolicy};
    use crate::error::LlmusageError;
    use crate::query::pricing::PricingStatus;

    /// The shipped static-v2 catalog must parse cleanly and cover the
    /// matchers the test suite asserts against (`gpt-5`, `claude-sonnet`,
    /// `opus`, `o3`).
    #[test]
    fn pricing_catalog_loads_static_v2() {
        let catalog = PricingCatalog::embedded();
        assert_eq!(catalog.version, "static-v2");
        assert_eq!(catalog.status, PricingStatus::Static);
        assert!(catalog.find("codex", "gpt-5").is_some());
        assert!(catalog.find("codex", "gpt-5.5").is_some());
        assert!(catalog.find("codex", "gpt-5.4").is_some());
        assert!(catalog.find("codex", "gpt-5.4-mini").is_some());
        assert!(catalog.find("codex", "o3-mini").is_some());
        assert!(catalog.find("claude", "claude-opus-4-1").is_some());
        assert!(catalog.find("claude", "claude-sonnet-4-5").is_some());
        assert!(catalog.find("claude", "claude-fable-5").is_some());
        assert!(catalog.find("claude", "claude-mythos-5").is_some());
        assert!(catalog.find("opencode", "gpt-5").is_some());
        assert!(catalog.find("codex", "gpt-5-codex").is_some());
        assert!(catalog.find("opencode", "claude.sonnet.4.5").is_some());
        assert!(
            catalog
                .find("opencode", "anthropic/claude-sonnet-4-5")
                .is_some()
        );
        assert!(
            catalog
                .find("opencode", "anthropic.claude-fable-5")
                .is_some()
        );
        assert!(
            catalog
                .find("opencode", "anthropic/claude-mythos-5")
                .is_some()
        );
        assert!(catalog.find("opencode", "gemini-3-pro-high").is_some());
        assert!(catalog.find("codex", "made-up-model").is_none());
        assert!(catalog.find("codex", "not-gpt-5").is_none());
        assert!(catalog.find("codex", "gpt-50").is_none());
        assert!(catalog.find("opencode", "gpt2").is_none());
        assert!(catalog.find("claude", "not-fable-5").is_none());
        assert!(catalog.find("claude", "not-mythos-5").is_none());
        assert!(catalog.find("claude", "claude-mythos-preview").is_none());
    }

    #[test]
    fn pricing_catalog_covers_gpt_5_6_exact_models_and_alias() {
        let catalog = PricingCatalog::embedded();
        for source in ["codex", "opencode"] {
            for (model, expected_id, input, cached, write, output) in [
                ("gpt-5.6-luna", "gpt-5.6-luna", 1.0, 0.1, 1.25, 6.0),
                ("gpt-5.6-terra", "gpt-5.6-terra", 2.5, 0.25, 3.125, 15.0),
                ("gpt-5.6-sol", "gpt-5.6-sol", 5.0, 0.5, 6.25, 30.0),
                ("gpt-5.6", "gpt-5.6-sol", 5.0, 0.5, 6.25, 30.0),
            ] {
                let entry = catalog.find(source, model).expect("GPT-5.6 row");
                assert_eq!(entry.id, expected_id, "{source}:{model}");
                assert_eq!(entry.default_rate.input_per_mtok, input, "{model}");
                assert_eq!(entry.default_rate.cached_per_mtok, cached, "{model}");
                assert_eq!(
                    entry.default_rate.cache_creation_per_mtok(),
                    write,
                    "{model}"
                );
                assert_eq!(entry.default_rate.output_per_mtok, output, "{model}");
                assert_eq!(entry.context_window, Some(1_050_000), "{model}");
                assert_eq!(entry.tiers.len(), 1, "{model}");
                assert_eq!(entry.tiers[0].prompt_tokens_above, 272_000, "{model}");
            }
        }

        for model in [
            "not-gpt-5.6-luna",
            "not-gpt-5.6-terra",
            "not-gpt-5.6-sol",
            "gpt-5.6-luna-preview",
        ] {
            let matched = catalog.find("codex", model);
            assert!(
                matched.is_none() || !matched.is_some_and(|entry| entry.id.starts_with("gpt-5.6")),
                "exact GPT-5.6 matchers must not claim {model}"
            );
        }
    }

    #[test]
    fn pricing_catalog_prefix_matches_minor_variants() {
        let catalog = PricingCatalog::embedded();
        let entry = catalog
            .find("codex", "gpt-5-mini-2026-05-01")
            .expect("dash-delimited minor variant should match gpt-5-mini");
        assert_eq!(entry.default_rate.input_per_mtok, 0.25);

        let dotted = catalog
            .find("codex", "gpt-5.4")
            .expect("dot-delimited GPT-5 variant should match gpt-5");
        assert_eq!(dotted.default_rate.input_per_mtok, 1.25);

        let dotted_mini = catalog
            .find("codex", "gpt-5.4-mini")
            .expect("dot-delimited GPT-5 mini variant should match gpt-5");
        assert_eq!(dotted_mini.default_rate.input_per_mtok, 1.25);

        let base = catalog
            .find("codex", "gpt-5")
            .expect("exact matcher should still match base model");
        assert_eq!(base.default_rate.input_per_mtok, 1.25);
    }

    #[test]
    fn pricing_catalog_matches_known_provider_aliases() {
        let catalog = PricingCatalog::embedded();
        let codex = catalog
            .find("codex", "gpt-5-codex")
            .expect("Codex-specific GPT-5 alias should match gpt-5");
        assert_eq!(codex.default_rate.input_per_mtok, 1.25);

        let claude = catalog
            .find("opencode", "anthropic/claude.sonnet.4.5")
            .expect("provider-prefixed dotted Claude IDs should normalize");
        assert_eq!(claude.default_rate.output_per_mtok, 15.0);

        let gemini = catalog
            .find("opencode", "gemini-3-pro-high")
            .expect("Gemini high alias should match preview pricing");
        assert_eq!(gemini.default_rate.input_per_mtok, 2.0);

        let fable = catalog
            .find("opencode", "anthropic.claude-fable-5")
            .expect("dot-normalized Anthropic Fable IDs should match OpenCode pricing");
        assert_eq!(fable.default_rate.input_per_mtok, 10.0);
        assert_eq!(fable.default_rate.cached_per_mtok, 1.0);
        assert_eq!(fable.default_rate.cache_creation_per_mtok(), 12.5);
        assert_eq!(fable.default_rate.output_per_mtok, 50.0);

        let mythos = catalog
            .find("opencode", "anthropic/claude-mythos-5")
            .expect("provider-prefixed Mythos IDs should match OpenCode pricing");
        assert_eq!(mythos.default_rate.input_per_mtok, 10.0);
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
        assert!((entry.default_rate.input_per_mtok - 1.5).abs() < f64::EPSILON);
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
        assert!((gpt.default_rate.input_per_mtok - 1.25).abs() < 1e-12);
        assert!((gpt.default_rate.cached_per_mtok - 0.125).abs() < 1e-12);
        assert!((gpt.default_rate.cache_creation_per_mtok() - 1.25).abs() < 1e-12);
        assert!((gpt.default_rate.output_per_mtok - 10.0).abs() < 1e-12);
        assert!((gpt.default_rate.reasoning_per_mtok() - 10.0).abs() < 1e-12);
        assert_eq!(
            gpt.default_rate.reasoning_policy,
            ReasoningPolicy::IncludedInOutput
        );
        assert!(catalog.find("opencode", "gpt-5").is_some());

        let claude = catalog
            .find("claude", "claude-sonnet-4-5")
            .expect("provider-prefixed matcher should be normalized");
        assert!((claude.default_rate.input_per_mtok - 3.0).abs() < 1e-12);
        assert!((claude.default_rate.cached_per_mtok - 0.3).abs() < 1e-12);
        assert!((claude.default_rate.cache_creation_per_mtok() - 3.75).abs() < 1e-12);
        assert_eq!(
            claude.default_rate.reasoning_policy,
            ReasoningPolicy::IncludedInOutput
        );
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
        assert!((entry.default_rate.reasoning_per_mtok() - 20.0).abs() < 1e-12);
        assert_eq!(
            entry.default_rate.reasoning_policy,
            ReasoningPolicy::Separate
        );
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
        assert!(err.to_string().contains("duplicate"));
        Ok(())
    }

    #[test]
    fn pricing_catalog_exposes_context_window() {
        let catalog = PricingCatalog::embedded();
        assert_eq!(
            catalog.context_window("claude", "claude-opus-4-1"),
            Some(200_000)
        );
        assert_eq!(catalog.context_window("codex", "gpt-5"), Some(400_000));
        assert_eq!(
            catalog.context_window("opencode", "gemini-3-pro-preview"),
            Some(1_000_000)
        );
        assert_eq!(
            catalog.context_window("claude", "claude-fable-5"),
            Some(1_000_000)
        );
        assert_eq!(
            catalog.context_window("claude", "claude-mythos-5"),
            Some(1_000_000)
        );
        assert_eq!(
            catalog.context_window("opencode", "anthropic.claude-fable-5"),
            Some(1_000_000)
        );
        // Unknown model degrades to None rather than panicking.
        assert_eq!(catalog.context_window("codex", "made-up-model"), None);
        assert_eq!(
            catalog.context_window("claude", "claude-mythos-preview"),
            None
        );
    }

    #[test]
    fn pricing_catalog_reads_native_context_window() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{
                "gpt-5": {{
                    "litellm_provider": "openai",
                    "input_cost_per_token": 0.00000125,
                    "output_cost_per_token": 0.000010,
                    "max_input_tokens": 400000
                }}
            }}"#
        )?;
        tmp.flush()?;

        let catalog = PricingCatalog::load_snapshot(tmp.path())?;
        assert_eq!(catalog.context_window("codex", "gpt-5"), Some(400_000));
        Ok(())
    }

    #[test]
    fn pricing_catalog_rejects_invalid_v2_tiers() -> anyhow::Result<()> {
        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{
                "schema_version": 2,
                "kind": "base",
                "version": "invalid-tiers",
                "models": [{{
                    "id": "invalid-model",
                    "sources": ["codex"],
                    "matches": [{{ "value": "invalid-model", "mode": "exact" }}],
                    "rates": {{
                        "default": {{
                            "input_per_mtok": 1.0,
                            "cached_per_mtok": 0.1,
                            "output_per_mtok": 2.0
                        }},
                        "tiers": [
                            {{
                                "name": "long",
                                "prompt_tokens_above": 1000,
                                "input_per_mtok": 2.0,
                                "cached_per_mtok": 0.2,
                                "output_per_mtok": 3.0
                            }},
                            {{
                                "name": "longer",
                                "prompt_tokens_above": 1000,
                                "input_per_mtok": -1.0,
                                "cached_per_mtok": 0.3,
                                "output_per_mtok": 4.0
                            }}
                        ]
                    }}
                }}]
            }}"#
        )?;
        tmp.flush()?;

        let error = PricingCatalog::load_snapshot(tmp.path())
            .expect_err("duplicate tier thresholds must be rejected");
        assert!(error.to_string().contains("strictly increasing"), "{error}");
        Ok(())
    }

    #[test]
    fn pricing_overlay_removal_is_strict_and_replacement_is_whole_model() -> anyhow::Result<()> {
        let mut invalid = NamedTempFile::new()?;
        writeln!(
            invalid,
            r#"{{
                "schema_version": 2,
                "kind": "overlay",
                "version": "invalid-removal",
                "models": [],
                "remove_models": ["does-not-exist"]
            }}"#
        )?;
        invalid.flush()?;
        let invalid_overlay = PricingCatalog::load_overlay(invalid.path())?;
        let error = PricingCatalog::embedded()
            .merge_overlay(invalid_overlay)
            .expect_err("unknown model removal must be rejected");
        assert!(error.to_string().contains("unknown model id"), "{error}");

        let mut replacement = NamedTempFile::new()?;
        writeln!(
            replacement,
            r#"{{
                "schema_version": 2,
                "kind": "overlay",
                "version": "replace-luna",
                "models": [{{
                    "id": "gpt-5.6-luna",
                    "sources": ["codex"],
                    "matches": [{{ "value": "private-luna", "mode": "exact" }}],
                    "rates": {{
                        "default": {{
                            "input_per_mtok": 9.0,
                            "cached_per_mtok": 0.9,
                            "output_per_mtok": 27.0
                        }}
                    }},
                    "context_window": 123456
                }}]
            }}"#
        )?;
        replacement.flush()?;
        let overlay = PricingCatalog::load_overlay(replacement.path())?;
        let merged = PricingCatalog::embedded().merge_overlay(overlay)?;
        let private = merged
            .find("codex", "private-luna")
            .expect("replacement matcher");
        assert_eq!(private.id, "gpt-5.6-luna");
        assert_eq!(private.default_rate.input_per_mtok, 9.0);
        assert_eq!(private.context_window, Some(123_456));
        assert!(private.tiers.is_empty());
        assert_ne!(
            merged
                .find("codex", "gpt-5.6-luna")
                .map(|entry| entry.id.as_str()),
            Some("gpt-5.6-luna"),
            "the old matcher must not survive whole-model replacement"
        );
        Ok(())
    }
}
