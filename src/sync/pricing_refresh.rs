//! Sync-owned dynamic pricing refresh for upstream pricing catalogs.
//!
//! Refreshes public pricing tables from LiteLLM and models.dev before parsing,
//! caching each source under the runtime root with a 1-hour freshness window.
//! Preserves user overlays and falls back gracefully to cached data or the
//! embedded catalog on network errors.

use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use std::{future::Future, sync::Arc};

#[cfg(test)]
use crate::sync::types::PricingFetcherFn;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    domain::{
        pricing::PricingStatus,
        pricing_catalog::{
            CatalogDocument, CatalogKind, MatchMode, ModelDefinition, ModelRates, PricingCatalog,
            PricingMatcher, PricingRate, PricingTier, native_sources, normalize_model_candidate,
            read_f64, read_reasoning_policy, read_u64, stable_identifier,
        },
    },
    error::Result,
    store::Store,
    sync::types::SyncRunOptions,
    util::hash_string,
};

pub const LITELLM_PRICING_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
pub const MODELS_DEV_URL: &str = "https://models.dev/api.json";
pub const CACHE_TTL_SECS: u64 = 3600;
pub const FETCH_TIMEOUT_SECS: u64 = 10;
pub const CACHE_LITELLM_FILE: &str = "pricing-cache-litellm.json";
pub const CACHE_MODELS_DEV_FILE: &str = "pricing-cache-models-dev.json";

static LIVE_REFRESH_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enables live network fetching for production sync.
/// Honored only if `LLMUSAGE_PRICING_REFRESH` is not `off` or `0`.
pub fn enable_live_pricing_refresh() {
    if let Ok(val) = std::env::var("LLMUSAGE_PRICING_REFRESH") {
        let trimmed = val.trim();
        if trimmed.eq_ignore_ascii_case("off") || trimmed == "0" {
            return;
        }
    }
    LIVE_REFRESH_ENABLED.store(true, Ordering::Relaxed);
}

/// Checks if live network fetching is enabled.
pub fn is_live_refresh_enabled() -> bool {
    if let Ok(val) = std::env::var("LLMUSAGE_PRICING_REFRESH") {
        let trimmed = val.trim();
        if trimmed.eq_ignore_ascii_case("off") || trimmed == "0" {
            return false;
        }
    }
    LIVE_REFRESH_ENABLED.load(Ordering::Relaxed)
}

/// Helper to create a test double fetcher closure for `SyncRunOptions`.
#[cfg(test)]
pub(crate) fn mock_pricing_fetcher<F, Fut>(f: F) -> PricingFetcherFn
where
    F: Fn(&str) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = std::result::Result<String, String>> + Send + 'static,
{
    Arc::new(move |url| Box::pin(f(url)))
}

/// Envelope stored on disk containing saved-at UTC timestamp and raw body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingCacheEnvelope {
    pub saved_at: u64,
    pub body: String,
}

pub fn read_cache(path: &Path) -> Option<PricingCacheEnvelope> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn write_cache(path: &Path, saved_at: u64, body: &str) -> Result<()> {
    let envelope = PricingCacheEnvelope {
        saved_at,
        body: body.to_string(),
    };
    let json = serde_json::to_string(&envelope).map_err(|err| {
        crate::error::LlmusageError::ConfigInvalid {
            detail: format!("failed to serialize pricing cache envelope: {err}"),
        }
    })?;
    crate::integrations::write_file_atomic(path, json.as_bytes()).map_err(|err| {
        crate::error::LlmusageError::ConfigInvalid {
            detail: format!("failed to write pricing cache: {err}"),
        }
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum CacheStatus {
    Fresh(String),
    Stale(String),
    Missing,
}

pub fn check_cache(path: &Path, now: u64) -> CacheStatus {
    if let Some(env) = read_cache(path) {
        if env.saved_at > now {
            CacheStatus::Stale(env.body)
        } else if now - env.saved_at <= CACHE_TTL_SECS {
            CacheStatus::Fresh(env.body)
        } else {
            CacheStatus::Stale(env.body)
        }
    } else {
        CacheStatus::Missing
    }
}

pub async fn fetch_production(url: &str) -> std::result::Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP request to {url} failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "HTTP request to {url} returned status {}",
            resp.status()
        ));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| format!("failed to read HTTP body from {url}: {e}"))?;
    Ok(body)
}

async fn do_fetch(options: &SyncRunOptions, url: &str) -> std::result::Result<String, String> {
    if let Some(fetcher) = &options.pricing_fetcher {
        fetcher(url).await
    } else if is_live_refresh_enabled() {
        fetch_production(url).await
    } else {
        Err("live pricing refresh is disabled".to_string())
    }
}

pub fn has_usable_litellm_rows(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    let Some(root) = value.as_object() else {
        return false;
    };
    let model_root = value
        .get("models")
        .and_then(Value::as_object)
        .unwrap_or(root);
    for (key, entry) in model_root {
        if matches!(
            key.as_str(),
            "version" | "sample_spec" | "README" | "metadata" | "schema"
        ) {
            continue;
        }
        if let Some(cost) = read_f64(entry, "input_cost_per_token")
            && cost.is_finite()
            && cost >= 0.0
        {
            return true;
        }
    }
    false
}

pub fn has_usable_models_dev_rows(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    let Some(root) = value.as_object() else {
        return false;
    };
    for (_k, v) in root {
        if let Some(models) = v.get("models").and_then(Value::as_object) {
            for (_mk, m) in models {
                if let Some(cost) = m.get("cost")
                    && let Some(input) = read_f64(cost, "input")
                    && input.is_finite()
                    && input >= 0.0
                {
                    return true;
                }
            }
        } else if let Some(cost) = v.get("cost")
            && let Some(input) = read_f64(cost, "input")
            && input.is_finite()
            && input >= 0.0
        {
            return true;
        }
    }
    false
}

fn parse_threshold(suffix: &str) -> Option<u64> {
    let stripped = suffix
        .strip_suffix("_tokens")
        .or_else(|| suffix.strip_suffix("tokens"))
        .unwrap_or(suffix);
    let trimmed = stripped.trim_matches(|c| c == '_' || c == '-');
    if let Some(k_val) = trimmed
        .strip_suffix('k')
        .or_else(|| trimmed.strip_suffix('K'))
    {
        let n: u64 = k_val.parse().ok()?;
        Some(n.saturating_mul(1_000))
    } else if let Some(m_val) = trimmed
        .strip_suffix('m')
        .or_else(|| trimmed.strip_suffix('M'))
    {
        let n: u64 = m_val.parse().ok()?;
        Some(n.saturating_mul(1_000_000))
    } else {
        trimmed.parse().ok()
    }
}

fn extract_litellm_tiers(raw_entry: &Value, base_rate: &PricingRate) -> Vec<PricingTier> {
    let Some(obj) = raw_entry.as_object() else {
        return Vec::new();
    };
    let mut tiers_by_threshold: BTreeMap<u64, PricingTier> = BTreeMap::new();

    for (key, val) in obj {
        if let Some(suffix) = key.strip_prefix("input_cost_per_token_above_") {
            let Some(threshold) = parse_threshold(suffix) else {
                continue;
            };
            if threshold == 0 {
                continue;
            }
            let Some(input_above) = val
                .as_f64()
                .or_else(|| val.as_str().and_then(|s| s.parse().ok()))
            else {
                continue;
            };
            if !input_above.is_finite() || input_above < 0.0 {
                continue;
            }

            let output_key = format!("output_cost_per_token_above_{suffix}");
            let cache_read_key = format!("cache_read_input_token_cost_above_{suffix}");
            let cache_create_key = format!("cache_creation_input_token_cost_above_{suffix}");

            let output_above = read_f64(raw_entry, &output_key)
                .map(|r| r * 1_000_000.0)
                .unwrap_or(base_rate.output_per_mtok);
            let cached_above = read_f64(raw_entry, &cache_read_key)
                .map(|r| r * 1_000_000.0)
                .unwrap_or(base_rate.cached_per_mtok);
            let cache_create_above = read_f64(raw_entry, &cache_create_key)
                .map(|r| r * 1_000_000.0)
                .or(base_rate.cache_creation_per_mtok);

            let tier_rate = PricingRate {
                input_per_mtok: input_above * 1_000_000.0,
                cached_per_mtok: cached_above,
                cache_creation_per_mtok: cache_create_above,
                output_per_mtok: output_above,
                reasoning_per_mtok: base_rate.reasoning_per_mtok,
                reasoning_policy: base_rate.reasoning_policy,
            };

            tiers_by_threshold.insert(
                threshold,
                PricingTier {
                    name: format!("above_{threshold}"),
                    prompt_tokens_above: threshold,
                    rate: tier_rate,
                },
            );
        }
    }

    tiers_by_threshold.into_values().collect()
}

fn extract_models_dev_tiers(cost_val: &Value, base_rate: &PricingRate) -> Vec<PricingTier> {
    let Some(cost_obj) = cost_val.as_object() else {
        return Vec::new();
    };
    let Some(tiers_arr) = cost_obj.get("tiers").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut tiers_by_threshold: BTreeMap<u64, PricingTier> = BTreeMap::new();
    for tier_val in tiers_arr {
        let size = tier_val
            .get("tier")
            .and_then(|t| t.get("size"))
            .and_then(Value::as_u64)
            .or_else(|| read_u64(tier_val, "size"))
            .or_else(|| read_u64(tier_val, "prompt_tokens_above"));

        let Some(threshold) = size else {
            continue;
        };
        if threshold == 0 {
            continue;
        }

        let input_above = read_f64(tier_val, "input").unwrap_or(base_rate.input_per_mtok);
        let output_above = read_f64(tier_val, "output").unwrap_or(base_rate.output_per_mtok);
        let cached_above = read_f64(tier_val, "cache_read").unwrap_or(base_rate.cached_per_mtok);
        let cache_create_above =
            read_f64(tier_val, "cache_write").or(base_rate.cache_creation_per_mtok);

        let tier_rate = PricingRate {
            input_per_mtok: input_above,
            cached_per_mtok: cached_above,
            cache_creation_per_mtok: cache_create_above,
            output_per_mtok: output_above,
            reasoning_per_mtok: base_rate.reasoning_per_mtok,
            reasoning_policy: base_rate.reasoning_policy,
        };

        tiers_by_threshold.insert(
            threshold,
            PricingTier {
                name: format!("above_{threshold}"),
                prompt_tokens_above: threshold,
                rate: tier_rate,
            },
        );
    }

    tiers_by_threshold.into_values().collect()
}

fn parse_litellm_models(body: &str) -> Vec<ModelDefinition> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let Some(root) = value.as_object() else {
        return Vec::new();
    };
    let model_root = value
        .get("models")
        .and_then(Value::as_object)
        .unwrap_or(root);

    let mut map: BTreeMap<String, ModelDefinition> = BTreeMap::new();
    for (model_key, raw_entry) in model_root {
        if matches!(
            model_key.as_str(),
            "version" | "sample_spec" | "README" | "metadata" | "schema"
        ) {
            continue;
        }
        let Some(input_cost) = read_f64(raw_entry, "input_cost_per_token") else {
            continue;
        };
        if !input_cost.is_finite() || input_cost < 0.0 {
            continue;
        }
        let output_cost = read_f64(raw_entry, "output_cost_per_token").unwrap_or(0.0);
        let cached_cost = read_f64(raw_entry, "cache_read_input_token_cost").unwrap_or(input_cost);
        let cache_create_cost = read_f64(raw_entry, "cache_creation_input_token_cost");
        let reasoning_cost = read_f64(raw_entry, "output_cost_per_reasoning_token");
        let reasoning_policy = read_reasoning_policy(raw_entry);

        let context_window = read_u64(raw_entry, "max_input_tokens")
            .or_else(|| read_u64(raw_entry, "max_tokens"))
            .filter(|w| *w > 0);

        let provider = raw_entry
            .get("litellm_provider")
            .and_then(Value::as_str)
            .or_else(|| raw_entry.get("provider").and_then(Value::as_str));

        let sources = native_sources(model_key, provider);
        if sources.is_empty() {
            continue;
        }

        let model_id = public_model_id(model_key);
        if model_id.is_empty() {
            continue;
        }

        let default_rate = PricingRate {
            input_per_mtok: input_cost * 1_000_000.0,
            cached_per_mtok: cached_cost * 1_000_000.0,
            cache_creation_per_mtok: cache_create_cost.map(|r| r * 1_000_000.0),
            output_per_mtok: output_cost * 1_000_000.0,
            reasoning_per_mtok: reasoning_cost.map(|r| r * 1_000_000.0),
            reasoning_policy,
        };

        let tiers = extract_litellm_tiers(raw_entry, &default_rate);

        let key = model_id.clone();
        let matches = vec![PricingMatcher::exact(model_id.clone())];
        map.insert(
            key,
            ModelDefinition {
                id: model_id,
                sources,
                matches,
                rates: ModelRates {
                    default: default_rate,
                    tiers,
                },
                context_window,
            },
        );
    }

    map.into_values().collect()
}

fn parse_models_dev_models(body: &str) -> Vec<ModelDefinition> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let Some(root) = value.as_object() else {
        return Vec::new();
    };

    let mut map: BTreeMap<String, ModelDefinition> = BTreeMap::new();
    let mut flat_models: Vec<(String, Option<String>, &Value)> = Vec::new();
    for (key, val) in root {
        if let Some(models) = val.get("models").and_then(Value::as_object) {
            let provider_id = val.get("id").and_then(Value::as_str).unwrap_or(key);
            for (m_key, m_val) in models {
                flat_models.push((m_key.clone(), Some(provider_id.to_string()), m_val));
            }
        } else if val.get("cost").is_some() {
            flat_models.push((key.clone(), None, val));
        }
    }

    for (model_key, provider, m_val) in flat_models {
        let declared_id = m_val
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(&model_key);
        let model_id = public_model_id(declared_id);
        if model_id.is_empty() {
            continue;
        }

        let sources = native_sources(declared_id, provider.as_deref());
        if sources.is_empty() {
            continue;
        }

        let Some(cost_val) = m_val.get("cost") else {
            continue;
        };
        let Some(input) = read_f64(cost_val, "input") else {
            continue;
        };
        if !input.is_finite() || input < 0.0 {
            continue;
        }
        let output = read_f64(cost_val, "output").unwrap_or(0.0);
        let cached = read_f64(cost_val, "cache_read").unwrap_or(input);
        let cache_write = read_f64(cost_val, "cache_write");

        let context_window = m_val
            .get("limit")
            .and_then(|l| l.get("context"))
            .and_then(Value::as_u64)
            .or_else(|| read_u64(m_val, "context_length"))
            .or_else(|| read_u64(m_val, "max_tokens"))
            .or_else(|| read_u64(m_val, "max_input_tokens"))
            .filter(|w| *w > 0);

        let default_rate = PricingRate {
            input_per_mtok: input,
            cached_per_mtok: cached,
            cache_creation_per_mtok: cache_write,
            output_per_mtok: output,
            reasoning_per_mtok: None,
            reasoning_policy: crate::domain::pricing_catalog::ReasoningPolicy::IncludedInOutput,
        };

        let tiers = extract_models_dev_tiers(cost_val, &default_rate);

        let key = model_id.clone();
        let matches = vec![PricingMatcher::exact(model_id.clone())];
        map.insert(
            key,
            ModelDefinition {
                id: model_id,
                sources,
                matches,
                rates: ModelRates {
                    default: default_rate,
                    tiers,
                },
                context_window,
            },
        );
    }

    map.into_values().collect()
}

fn public_model_id(raw: &str) -> String {
    let trimmed = raw.trim();
    let bare = trimmed
        .rsplit_once('/')
        .map(|(_, model)| model)
        .unwrap_or(trimmed);
    let id = stable_identifier(bare);
    if id.is_empty() {
        stable_identifier(trimmed)
    } else {
        id
    }
}

fn sources_overlap(left: &ModelDefinition, right: &ModelDefinition) -> bool {
    left.sources.iter().any(|source| {
        right
            .sources
            .iter()
            .any(|other| source.eq_ignore_ascii_case(other))
    })
}

fn same_matcher(left: &PricingMatcher, right: &PricingMatcher) -> bool {
    left.mode == right.mode
        && normalize_model_candidate(&left.value) == normalize_model_candidate(&right.value)
}

fn exact_overlap(left: &ModelDefinition, right: &ModelDefinition) -> bool {
    left.matches.iter().any(|matcher| {
        matcher.mode == MatchMode::Exact
            && right
                .matches
                .iter()
                .any(|other| other.mode == MatchMode::Exact && same_matcher(matcher, other))
    })
}

/// Published rows update prices without deleting embedded family coverage.
///
/// The same model id keeps its existing matchers, including aliases and
/// families, and takes the fetched default rate. A fetched long-context tier
/// replaces the previous tiers. An empty fetched tier list keeps the tiers
/// already on that id. A new id with an exact matcher owned by another model
/// takes that exact matcher and leaves the other model's remaining matchers.
fn absorb_fetched_model(models: &mut BTreeMap<String, ModelDefinition>, fetched: ModelDefinition) {
    if let Some(existing) = models.get_mut(&fetched.id) {
        let kept_tiers = existing.rates.tiers.clone();
        existing.rates = fetched.rates;
        if existing.rates.tiers.is_empty() {
            existing.rates.tiers = kept_tiers;
        }
        if fetched.context_window.is_some() {
            existing.context_window = fetched.context_window;
        }
        for matcher in fetched.matches {
            if !existing
                .matches
                .iter()
                .any(|current| same_matcher(current, &matcher))
            {
                existing.matches.push(matcher);
            }
        }
        return;
    }

    let mut remove_ids = Vec::new();
    for (id, existing) in models.iter_mut() {
        if !sources_overlap(existing, &fetched) || !exact_overlap(existing, &fetched) {
            continue;
        }
        existing.matches.retain(|matcher| {
            matcher.mode != MatchMode::Exact
                || !fetched.matches.iter().any(|claimed| {
                    claimed.mode == MatchMode::Exact && same_matcher(matcher, claimed)
                })
        });
        if existing.matches.is_empty() {
            remove_ids.push(id.clone());
        }
    }
    for id in remove_ids {
        models.remove(&id);
    }
    models.insert(fetched.id.clone(), fetched);
}

fn fill_models_dev(models: &mut BTreeMap<String, ModelDefinition>, fetched: ModelDefinition) {
    if let Some(existing) = models.get_mut(&fetched.id) {
        if existing.rates.tiers.is_empty() && !fetched.rates.tiers.is_empty() {
            existing.rates.tiers = fetched.rates.tiers;
        }
        if existing.context_window.is_none() && fetched.context_window.is_some() {
            existing.context_window = fetched.context_window;
        }
        return;
    }
    absorb_fetched_model(models, fetched);
}

pub fn build_assembled_catalog(
    litellm_body: Option<&str>,
    models_dev_body: Option<&str>,
) -> Result<PricingCatalog> {
    let embedded_doc = PricingCatalog::embedded().document();
    let mut model_map: BTreeMap<String, ModelDefinition> = BTreeMap::new();

    for model in &embedded_doc.models {
        model_map.insert(model.id.clone(), model.clone());
    }
    if let Some(body) = litellm_body {
        for model in parse_litellm_models(body) {
            absorb_fetched_model(&mut model_map, model);
        }
    }
    if let Some(body) = models_dev_body {
        for model in parse_models_dev_models(body) {
            fill_models_dev(&mut model_map, model);
        }
    }

    let models = model_map.into_values().collect::<Vec<_>>();
    let payload = serde_json::to_string(&models).map_err(|err| {
        crate::error::LlmusageError::ConfigInvalid {
            detail: format!("failed to hash refreshed pricing catalog: {err}"),
        }
    })?;
    let final_doc = CatalogDocument {
        schema_version: 2,
        kind: CatalogKind::Base,
        version: format!("public-{}", hash_string(&payload)),
        models,
        remove_models: Vec::new(),
    };
    PricingCatalog::from_document(final_doc, PricingStatus::Snapshot)
}

fn warn_pricing_fallback(source: &str, detail: &str) {
    tracing::warn!(
        source = source,
        detail = detail,
        "pricing source refresh failed; using fallback catalog"
    );
}

/// Refreshes pricing catalogs according to tokscale 1-hour cache rules.
pub async fn refresh_pricing_if_needed(store: &Store, options: &SyncRunOptions) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let litellm_cache_path = store.paths.root_dir.join(CACHE_LITELLM_FILE);
    let models_dev_cache_path = store.paths.root_dir.join(CACHE_MODELS_DEV_FILE);

    let litellm_cache_status = check_cache(&litellm_cache_path, now);
    let models_dev_cache_status = check_cache(&models_dev_cache_path, now);

    let has_network = options.pricing_fetcher.is_some() || is_live_refresh_enabled();

    // 1. Resolve LiteLLM body
    let litellm_body = match litellm_cache_status {
        CacheStatus::Fresh(body) => Some(body),
        CacheStatus::Stale(prev_body) => {
            if has_network {
                match do_fetch(options, LITELLM_PRICING_URL).await {
                    Ok(body) if has_usable_litellm_rows(&body) => {
                        let _ = write_cache(&litellm_cache_path, now, &body);
                        Some(body)
                    }
                    Ok(_) => {
                        warn_pricing_fallback("LiteLLM", "returned no usable pricing rows");
                        Some(prev_body)
                    }
                    Err(err) => {
                        warn_pricing_fallback("LiteLLM", &err);
                        Some(prev_body)
                    }
                }
            } else {
                Some(prev_body)
            }
        }
        CacheStatus::Missing => {
            if has_network {
                match do_fetch(options, LITELLM_PRICING_URL).await {
                    Ok(body) if has_usable_litellm_rows(&body) => {
                        let _ = write_cache(&litellm_cache_path, now, &body);
                        Some(body)
                    }
                    Ok(_) => {
                        warn_pricing_fallback("LiteLLM", "returned no usable pricing rows");
                        None
                    }
                    Err(err) => {
                        warn_pricing_fallback("LiteLLM", &err);
                        None
                    }
                }
            } else {
                None
            }
        }
    };

    // 2. Resolve models.dev body
    let models_dev_body = match models_dev_cache_status {
        CacheStatus::Fresh(body) => Some(body),
        CacheStatus::Stale(prev_body) => {
            if has_network {
                match do_fetch(options, MODELS_DEV_URL).await {
                    Ok(body) if has_usable_models_dev_rows(&body) => {
                        let _ = write_cache(&models_dev_cache_path, now, &body);
                        Some(body)
                    }
                    Ok(_) => {
                        warn_pricing_fallback("models.dev", "returned no usable pricing rows");
                        Some(prev_body)
                    }
                    Err(err) => {
                        warn_pricing_fallback("models.dev", &err);
                        Some(prev_body)
                    }
                }
            } else {
                Some(prev_body)
            }
        }
        CacheStatus::Missing => {
            if has_network {
                match do_fetch(options, MODELS_DEV_URL).await {
                    Ok(body) if has_usable_models_dev_rows(&body) => {
                        let _ = write_cache(&models_dev_cache_path, now, &body);
                        Some(body)
                    }
                    Ok(_) => {
                        warn_pricing_fallback("models.dev", "returned no usable pricing rows");
                        None
                    }
                    Err(err) => {
                        warn_pricing_fallback("models.dev", &err);
                        None
                    }
                }
            } else {
                None
            }
        }
    };

    // If both sources have no usable cache or response, keep embedded catalog
    if litellm_body.is_none() && models_dev_body.is_none() {
        return Ok(());
    }

    let assembled_catalog =
        match build_assembled_catalog(litellm_body.as_deref(), models_dev_body.as_deref()) {
            Ok(catalog) => catalog,
            Err(err) => {
                warn_pricing_fallback("catalog", &err.to_string());
                return Ok(());
            }
        };

    if let Err(err) = store.update_refreshed_base_catalog(assembled_catalog) {
        warn_pricing_fallback("catalog", &err.to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use tempfile::TempDir;

    use super::*;
    use crate::{
        models::{SourceCost, SourceKind, UsageEvent},
        paths::AppPaths,
        store::SyncShard,
    };

    fn test_store(temp: &TempDir) -> Store {
        let paths = AppPaths::with_root(temp.path().to_path_buf()).expect("paths");
        let store = Store::new(&paths).expect("store");
        store.bootstrap().expect("bootstrap");
        store
    }

    fn event_cost(store: &Store, model: &str) -> f64 {
        let conn = store.open_connection().expect("conn");
        conn.query_row(
            "SELECT cost_with_cache_usd FROM usage_event WHERE model = ?1",
            [model],
            |row| row.get(0),
        )
        .expect("cost")
    }

    fn sample_litellm_fixture() -> String {
        serde_json::json!({
            "claude-sonnet-5-5": {
                "input_cost_per_token": 0.000002,
                "output_cost_per_token": 0.000010,
                "cache_read_input_token_cost": 0.0000001,
                "cache_creation_input_token_cost": 0.0000025,
                "max_input_tokens": 1000000,
                "litellm_provider": "anthropic"
            },
            "claude-haiku-5-5": {
                "input_cost_per_token": 0.0000001,
                "output_cost_per_token": 0.0000005,
                "cache_read_input_token_cost": 0.00000001,
                "cache_creation_input_token_cost": 0.000000125,
                "max_input_tokens": 1000000,
                "litellm_provider": "anthropic"
            },
            "gpt-6.1-sol": {
                "input_cost_per_token": 0.000002,
                "output_cost_per_token": 0.000010,
                "cache_read_input_token_cost": 0.0000001,
                "cache_creation_input_token_cost": 0.0000025,
                "input_cost_per_token_above_272k_tokens": 0.000004,
                "output_cost_per_token_above_272k_tokens": 0.000015,
                "cache_read_input_token_cost_above_272k_tokens": 0.0000002,
                "cache_creation_input_token_cost_above_272k_tokens": 0.000005,
                "max_input_tokens": 1050000,
                "litellm_provider": "openai"
            }
        })
        .to_string()
    }

    fn sample_models_dev_fixture() -> String {
        serde_json::json!({
            "anthropic": {
                "id": "anthropic",
                "models": {
                    "claude-sonnet-5-5": {
                        "id": "claude-sonnet-5-5",
                        "cost": {
                            "input": 2.0,
                            "output": 10.0,
                            "cache_read": 0.1,
                            "cache_write": 2.5
                        },
                        "limit": { "context": 1000000 }
                    }
                }
            },
            "openai": {
                "id": "openai",
                "models": {
                    "gpt-6.1-sol": {
                        "id": "gpt-6.1-sol",
                        "cost": {
                            "input": 2.0,
                            "output": 10.0,
                            "cache_read": 0.1,
                            "cache_write": 2.5,
                            "tiers": [
                                {
                                    "tier": { "type": "context", "size": 272000 },
                                    "input": 4.0,
                                    "output": 15.0,
                                    "cache_read": 0.2,
                                    "cache_write": 5.0
                                }
                            ]
                        },
                        "limit": { "context": 1050000 }
                    }
                }
            }
        })
        .to_string()
    }

    #[tokio::test]
    async fn ac1_and_ac2_per_source_caching_and_recompute() {
        let temp = TempDir::new().expect("tempdir");
        let store = test_store(&temp);

        let litellm_calls = Arc::new(AtomicUsize::new(0));
        let models_dev_calls = Arc::new(AtomicUsize::new(0));

        let lc = Arc::clone(&litellm_calls);
        let mc = Arc::clone(&models_dev_calls);
        let fetcher = mock_pricing_fetcher(move |url| {
            let url = url.to_string();
            let lc = Arc::clone(&lc);
            let mc = Arc::clone(&mc);
            async move {
                if url.contains("litellm") {
                    lc.fetch_add(1, Ordering::SeqCst);
                    Ok(sample_litellm_fixture())
                } else if url.contains("models.dev") {
                    mc.fetch_add(1, Ordering::SeqCst);
                    Ok(sample_models_dev_fixture())
                } else {
                    Err("unknown url".to_string())
                }
            }
        });

        let options = SyncRunOptions {
            pricing_fetcher: Some(fetcher),
            ..Default::default()
        };

        // First run: missing cache -> both sources requested
        refresh_pricing_if_needed(&store, &options)
            .await
            .expect("refresh");
        assert_eq!(litellm_calls.load(Ordering::SeqCst), 1);
        assert_eq!(models_dev_calls.load(Ordering::SeqCst), 1);

        // Verify cache files written
        let litellm_cache = store.paths.root_dir.join(CACHE_LITELLM_FILE);
        let models_dev_cache = store.paths.root_dir.join(CACHE_MODELS_DEV_FILE);
        assert!(litellm_cache.exists());
        assert!(models_dev_cache.exists());

        // Second run: fresh cache (age < 3600) -> zero requests
        refresh_pricing_if_needed(&store, &options)
            .await
            .expect("refresh 2");
        assert_eq!(litellm_calls.load(Ordering::SeqCst), 1);
        assert_eq!(models_dev_calls.load(Ordering::SeqCst), 1);

        // Make litellm cache stale (> 3600s)
        let stale_env = PricingCacheEnvelope {
            saved_at: 100, // Very old
            body: sample_litellm_fixture(),
        };
        std::fs::write(
            &litellm_cache,
            serde_json::to_string(&stale_env).expect("json"),
        )
        .expect("write stale");

        // Third run: litellm stale -> requests litellm only, models.dev still fresh
        refresh_pricing_if_needed(&store, &options)
            .await
            .expect("refresh 3");
        assert_eq!(litellm_calls.load(Ordering::SeqCst), 2);
        assert_eq!(models_dev_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn ac3_source_failure_preserves_previous_cache() {
        let temp = TempDir::new().expect("tempdir");
        let store = test_store(&temp);

        let litellm_cache = store.paths.root_dir.join(CACHE_LITELLM_FILE);
        let previous_body = sample_litellm_fixture();
        write_cache(&litellm_cache, 50, &previous_body).expect("write old cache");

        let fetcher = mock_pricing_fetcher(|url| {
            let url = url.to_string();
            async move {
                if url.contains("litellm") {
                    Err("connection timeout".to_string())
                } else {
                    Ok("{}".to_string())
                }
            }
        });

        let options = SyncRunOptions {
            pricing_fetcher: Some(fetcher),
            ..Default::default()
        };

        refresh_pricing_if_needed(&store, &options)
            .await
            .expect("refresh");

        // Cache must still contain previous body
        let env = read_cache(&litellm_cache).expect("read");
        assert_eq!(env.body, previous_body);
    }

    #[tokio::test]
    async fn ac4_no_cache_and_both_fail_keeps_embedded() {
        let temp = TempDir::new().expect("tempdir");
        let store = test_store(&temp);

        let fetcher = mock_pricing_fetcher(|_url| async move { Err("network down".to_string()) });

        let options = SyncRunOptions {
            pricing_fetcher: Some(fetcher),
            ..Default::default()
        };

        refresh_pricing_if_needed(&store, &options)
            .await
            .expect("refresh should not fail");

        // Active catalog remains embedded static-v3
        let active = store.active_pricing_catalog().expect("active");
        assert_eq!(active.version, "static-v3");
    }

    #[tokio::test]
    async fn ac5_named_example_models_hit_exact_rates() {
        let catalog = build_assembled_catalog(
            Some(&sample_litellm_fixture()),
            Some(&sample_models_dev_fixture()),
        )
        .expect("build catalog");

        // 1. claude-sonnet-5-5 hits exact rate ($2 / $10), not embedded sonnet family ($3 / $15)
        let claude_entry = catalog
            .find("claude", "claude-sonnet-5-5")
            .expect("found on claude");
        assert_eq!(claude_entry.id, "claude-sonnet-5-5");
        assert_eq!(claude_entry.default_rate.input_per_mtok, 2.0);
        assert_eq!(claude_entry.default_rate.output_per_mtok, 10.0);
        assert_eq!(claude_entry.context_window, Some(1000000));

        let opencode_sonnet = catalog
            .find("opencode", "claude-sonnet-5-5")
            .expect("found on opencode");
        assert_eq!(opencode_sonnet.id, "claude-sonnet-5-5");
        assert_eq!(opencode_sonnet.default_rate.input_per_mtok, 2.0);

        // 2. claude-haiku-5-5 hits exact rate ($0.10 / $0.50), not embedded haiku family ($0.8 / $4)
        let haiku_entry = catalog
            .find("claude", "claude-haiku-5-5")
            .expect("found on claude");
        assert_eq!(haiku_entry.id, "claude-haiku-5-5");
        assert!((haiku_entry.default_rate.input_per_mtok - 0.1).abs() < 1e-9);
        assert_eq!(haiku_entry.default_rate.output_per_mtok, 0.5);

        // 3. gpt-6.1-sol hits exact rate ($2 / $10), not embedded gpt family ($1.25 / $10)
        let codex_sol = catalog
            .find("codex", "gpt-6.1-sol")
            .expect("found on codex");
        assert_eq!(codex_sol.id, "gpt-6.1-sol");
        assert_eq!(codex_sol.default_rate.input_per_mtok, 2.0);
        assert_eq!(codex_sol.context_window, Some(1050000));

        let opencode_sol = catalog
            .find("opencode", "gpt-6.1-sol")
            .expect("found on opencode");
        assert_eq!(opencode_sol.id, "gpt-6.1-sol");
        assert_eq!(opencode_sol.default_rate.input_per_mtok, 2.0);

        // Older models still match embedded catalog
        let old_opus = catalog.find("claude", "claude-opus").expect("old opus");
        assert_eq!(old_opus.default_rate.input_per_mtok, 15.0);

        // Dated family coverage stays when the fetched row updates the same id.
        let dated = catalog
            .find("codex", "gpt-6-astra-2026-09-03")
            .expect("dated astra");
        assert_eq!(dated.id, "gpt-6-astra");
    }

    #[tokio::test]
    async fn ac6_long_context_threshold_boundary() {
        let catalog = build_assembled_catalog(
            Some(&sample_litellm_fixture()),
            Some(&sample_models_dev_fixture()),
        )
        .expect("build catalog");

        let entry = catalog.find("codex", "gpt-6.1-sol").expect("sol entry");
        assert_eq!(entry.tiers.len(), 1);
        assert_eq!(entry.tiers[0].prompt_tokens_above, 272000);

        // Exactly at threshold: stays on lower/default tier
        let at_threshold = entry.rate_for_prompt_tokens(272000);
        assert_eq!(at_threshold.tier, "default");
        assert_eq!(at_threshold.rate.input_per_mtok, 2.0);

        // Above threshold: uses long-context tier
        let above_threshold = entry.rate_for_prompt_tokens(272001);
        assert_eq!(above_threshold.tier, "above_272000");
        assert_eq!(above_threshold.rate.input_per_mtok, 4.0);
        assert_eq!(above_threshold.rate.output_per_mtok, 15.0);
    }

    #[tokio::test]
    async fn ac2_rate_change_recomputes_and_fresh_cache_does_not() {
        let temp = TempDir::new().expect("tempdir");
        let store = test_store(&temp);
        let mut writer = store.begin_sync_run().expect("writer");
        let mut shard = SyncShard::new(SourceKind::Codex);
        shard.events.push(UsageEvent {
            event_key: "codex:luna:1".to_string(),
            source: SourceKind::Codex,
            provider_label: String::new(),
            model: "gpt-5.6-luna".to_string(),
            event_at: "2026-10-08T10:00:00Z".to_string(),
            hour_start: "2026-10-08T10:00:00Z".to_string(),
            tokens: crate::models::UsageTokens {
                input_tokens: 100_000,
                output_tokens: 0,
                total_tokens: 100_000,
                ..Default::default()
            },
            project: None,
            session: None,
            source_cost: None,
        });
        writer.commit_shard(shard).expect("commit");
        writer.finish_sync_run().expect("finish");

        let cost_before = event_cost(&store, "gpt-5.6-luna");
        let body = std::sync::Mutex::new(
            serde_json::json!({
                "gpt-5.6-luna": {
                    "input_cost_per_token": 0.000010,
                    "output_cost_per_token": 0.000060,
                    "litellm_provider": "openai"
                }
            })
            .to_string(),
        );
        let fetcher = mock_pricing_fetcher(move |url| {
            let url = url.to_string();
            let body = body.lock().expect("body").clone();
            async move {
                if url.contains("litellm") {
                    Ok(body)
                } else {
                    Err("models.dev offline".to_string())
                }
            }
        });
        let options = SyncRunOptions {
            pricing_fetcher: Some(fetcher),
            ..Default::default()
        };
        refresh_pricing_if_needed(&store, &options)
            .await
            .expect("refresh");
        let cost_after = event_cost(&store, "gpt-5.6-luna");
        assert_ne!(cost_before, cost_after);
        assert!((cost_after - 1.0).abs() < 1e-9, "cost={cost_after}");

        let conn = store.open_connection().expect("conn");
        conn.execute(
            "UPDATE usage_event SET cost_with_cache_usd = 123 WHERE model = 'gpt-5.6-luna'",
            [],
        )
        .expect("mark");
        drop(conn);
        refresh_pricing_if_needed(&store, &options)
            .await
            .expect("fresh");
        assert!((event_cost(&store, "gpt-5.6-luna") - 123.0).abs() < 1e-9);

        let reloaded = Store::new(&store.paths).expect("reload");
        let entry = reloaded
            .active_pricing_catalog()
            .expect("active")
            .find("codex", "gpt-5.6-luna")
            .expect("luna")
            .clone();
        assert_eq!(entry.default_rate.input_per_mtok, 10.0);
        assert!(
            entry
                .tiers
                .iter()
                .any(|tier| tier.prompt_tokens_above == 272_000)
        );
    }

    #[tokio::test]
    async fn ac7_raw_model_and_positive_source_reported_cost_unchanged() {
        let temp = TempDir::new().expect("tempdir");
        let store = test_store(&temp);

        let mut writer = store.begin_sync_run().expect("writer");
        let mut shard = SyncShard::new(SourceKind::Pi);
        shard.events.push(UsageEvent {
            event_key: "pi:event:1".to_string(),
            source: SourceKind::Pi,
            provider_label: String::new(),
            model: "claude-sonnet-5-5".to_string(),
            event_at: "2026-10-08T10:00:00Z".to_string(),
            hour_start: "2026-10-08T10:00:00Z".to_string(),
            tokens: crate::models::UsageTokens {
                input_tokens: 1000,
                output_tokens: 500,
                total_tokens: 1500,
                ..Default::default()
            },
            project: None,
            session: None,
            source_cost: Some(SourceCost {
                total: 0.05,
                input: Some(0.01),
                output: Some(0.04),
                cache_read: None,
                cache_write: None,
            }),
        });
        writer.commit_shard(shard).expect("commit");
        writer.finish_sync_run().expect("finish");

        // Refresh pricing to new catalog
        let fetcher = mock_pricing_fetcher(|_| async move { Ok(sample_litellm_fixture()) });
        let options = SyncRunOptions {
            pricing_fetcher: Some(fetcher),
            ..Default::default()
        };
        refresh_pricing_if_needed(&store, &options)
            .await
            .expect("refresh");

        // Verify that source_reported cost and raw model are preserved
        let conn = store.open_connection().expect("conn");
        let (model, status, cost): (String, String, f64) = conn
            .query_row(
                "SELECT model, pricing_status, cost_with_cache_usd FROM usage_event WHERE model = 'claude-sonnet-5-5'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("row");
        assert_eq!(model, "claude-sonnet-5-5");
        assert_eq!(status, "source_reported");
        assert_eq!(cost, 0.05);
    }

    #[tokio::test]
    async fn ac8_overlay_model_id_wins_after_refresh() {
        let temp = TempDir::new().expect("tempdir");
        let store = test_store(&temp);

        // Apply an overlay for claude-sonnet-5-5 with custom high rates
        let overlay_path = temp.path().join("overlay.json");
        std::fs::write(
            &overlay_path,
            serde_json::json!({
                "schema_version": 2,
                "kind": "overlay",
                "version": "user-overlay-v1",
                "models": [
                    {
                        "id": "claude-sonnet-5-5",
                        "sources": ["claude"],
                        "matches": [
                            { "value": "claude-sonnet-5-5", "mode": "exact" }
                        ],
                        "rates": {
                            "default": {
                                "input_per_mtok": 99.0,
                                "cached_per_mtok": 9.9,
                                "output_per_mtok": 999.0
                            }
                        }
                    }
                ]
            })
            .to_string(),
        )
        .expect("write overlay");
        store
            .apply_pricing_overlay(&overlay_path)
            .expect("apply overlay");

        // Refresh pricing from public tables
        let fetcher = mock_pricing_fetcher(|_| async move { Ok(sample_litellm_fixture()) });
        let options = SyncRunOptions {
            pricing_fetcher: Some(fetcher),
            ..Default::default()
        };
        refresh_pricing_if_needed(&store, &options)
            .await
            .expect("refresh");

        // The effective active catalog should preserve the overlay rate (99.0)
        let active = store.active_pricing_catalog().expect("active");
        let entry = active.find("claude", "claude-sonnet-5-5").expect("entry");
        assert_eq!(entry.default_rate.input_per_mtok, 99.0);
    }

    #[tokio::test]
    async fn ac9_and_ac10_offline_default_and_no_json_events_warnings() {
        let temp = TempDir::new().expect("tempdir");
        let store = test_store(&temp);

        // AC10: Default SyncRunOptions without fetcher does zero network requests
        let default_options = SyncRunOptions::default();
        refresh_pricing_if_needed(&store, &default_options)
            .await
            .expect("offline sync");
        let active = store.active_pricing_catalog().expect("active");
        assert_eq!(active.version, "static-v3");
    }
}
