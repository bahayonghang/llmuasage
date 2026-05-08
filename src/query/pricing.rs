use serde::{Deserialize, Serialize};

use super::pricing_catalog::PricingCatalog;

/// Pricing status reported alongside a [`CostBreakdown`] (D6/F1.3).
///
/// `Static` matches succeed against the embedded v1 catalog; `Snapshot`
/// is stamped when [`PricingCatalog::load_snapshot`] supplied the rates;
/// `Unpriced` means no catalog entry fired so the cost columns stay at 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PricingStatus {
    Static,
    Snapshot,
    #[default]
    Unpriced,
}

impl PricingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Snapshot => "snapshot",
            Self::Unpriced => "unpriced",
        }
    }
}

/// Per-event cost breakdown (D10/F1.3) persisted on `usage_event` so that
/// `cost_with_cache_usd` and `cost_without_cache_usd` are stable across
/// downstream UI without re-deriving them at query time.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CostBreakdown {
    /// Estimated USD cost charging cache reads at their cache-read rate.
    pub cost_with_cache_usd: f64,
    /// Estimated USD cost as if every cache-read were billed at the full
    /// input rate (lower bound on hypothetical "no-cache" usage).
    pub cost_without_cache_usd: f64,
    /// Catalog match outcome.
    pub pricing_status: PricingStatus,
    /// Catalog version label (e.g. `static-v1` or
    /// `litellm-snapshot-2026-05`) when matched.
    pub pricing_source: Option<String>,
    /// JSON-encoded rate row used for the calculation, when matched.
    pub pricing_rate: Option<String>,
}

/// Computes a cost breakdown using the embedded static-v1 catalog.
///
/// Equivalent to [`compute_cost_with`] keyed off [`PricingCatalog::static_v1`].
pub fn compute_cost(
    source: &str,
    model: &str,
    input_tokens: i64,
    cache_read_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
) -> CostBreakdown {
    compute_cost_with(
        PricingCatalog::static_v1(),
        source,
        model,
        input_tokens,
        cache_read_tokens,
        output_tokens,
        reasoning_output_tokens,
    )
}

/// Computes a cost breakdown against a caller-supplied catalog. Used by
/// `Store::recompute_costs_with` so doctor can drive recompute through a
/// litellm snapshot without mutating shared state.
pub fn compute_cost_with(
    catalog: &PricingCatalog,
    source: &str,
    model: &str,
    input_tokens: i64,
    cache_read_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
) -> CostBreakdown {
    let Some(pricing) = catalog.find(source, model) else {
        return CostBreakdown {
            pricing_status: PricingStatus::Unpriced,
            ..CostBreakdown::default()
        };
    };

    let input_mtok = input_tokens as f64 / 1_000_000.0;
    let cached_mtok = cache_read_tokens as f64 / 1_000_000.0;
    let output_mtok = (output_tokens + reasoning_output_tokens) as f64 / 1_000_000.0;

    let cost_with_cache_usd = input_mtok * pricing.input_per_mtok
        + cached_mtok * pricing.cached_per_mtok
        + output_mtok * pricing.output_per_mtok;
    let cost_without_cache_usd =
        (input_mtok + cached_mtok) * pricing.input_per_mtok + output_mtok * pricing.output_per_mtok;

    CostBreakdown {
        cost_with_cache_usd,
        cost_without_cache_usd,
        pricing_status: catalog.status,
        pricing_source: Some(catalog.version.clone()),
        pricing_rate: Some(format!(
            r#"{{"input_per_mtok":{:.4},"cached_per_mtok":{:.4},"output_per_mtok":{:.4}}}"#,
            pricing.input_per_mtok, pricing.cached_per_mtok, pricing.output_per_mtok
        )),
    }
}

/// Backwards-compatible scalar API used by the legacy report aggregates
/// in `query/reports.rs` and `query/mod.rs::cost_breakdown`. New surfaces
/// should call [`compute_cost`] / [`compute_cost_with`] for the full
/// breakdown.
pub fn estimate_cost_usd(
    source: &str,
    model: &str,
    input_tokens: i64,
    cache_read_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
) -> f64 {
    compute_cost(
        source,
        model,
        input_tokens,
        cache_read_tokens,
        output_tokens,
        reasoning_output_tokens,
    )
    .cost_with_cache_usd
}

#[cfg(test)]
mod tests {
    use super::{PricingStatus, compute_cost};

    /// Validates D6: a Codex/`gpt-5` event picks up the static-v1 rate row,
    /// produces non-zero cost columns, and stamps `pricing_source` for
    /// downstream re-keying.
    #[test]
    fn pricing_static_v1_hits_known_model() {
        let cost = compute_cost("codex", "gpt-5", 1_000_000, 200_000, 500_000, 0);
        assert_eq!(cost.pricing_status, PricingStatus::Static);
        assert_eq!(cost.pricing_source.as_deref(), Some("static-v1"));
        assert!(cost.cost_with_cache_usd > 0.0);
        // Without-cache lower-bounds: cache_read priced at full input rate.
        assert!(cost.cost_without_cache_usd > cost.cost_with_cache_usd);
        assert!(cost.pricing_rate.is_some());
    }

    /// Validates D6 fallthrough: an unknown model returns 0 cost and
    /// `Unpriced` status so dashboards can render the row instead of
    /// hiding the spend behind a fake number.
    #[test]
    fn pricing_unpriced_when_no_match() {
        let cost = compute_cost("codex", "made-up-model", 1_000, 0, 0, 0);
        assert_eq!(cost.pricing_status, PricingStatus::Unpriced);
        assert!(cost.pricing_source.is_none());
        assert_eq!(cost.cost_with_cache_usd, 0.0);
        assert_eq!(cost.cost_without_cache_usd, 0.0);
    }
}
