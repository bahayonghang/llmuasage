use serde::{Deserialize, Serialize};

use super::pricing_catalog::{PricingCatalog, ReasoningPolicy};

pub const PRICING_MIXED: &str = "mixed";
pub const PRICING_UNPRICED: &str = "unpriced";

/// Pricing status reported alongside a [`CostBreakdown`] (D6/F1.3).
///
/// `Static` matches succeed against the embedded catalog; `Snapshot`
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
            Self::Unpriced => PRICING_UNPRICED,
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
    /// Catalog version label (e.g. `static-v2` or
    /// `litellm-snapshot-2026-05`) when matched.
    pub pricing_source: Option<String>,
    /// JSON-encoded rate row used for the calculation, when matched.
    pub pricing_rate: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CostTokens {
    pub input: i64,
    pub cache_read: i64,
    pub cache_creation: i64,
    pub output: i64,
    pub reasoning_output: i64,
}

/// Computes a cost breakdown using the embedded catalog.
///
/// Equivalent to [`compute_cost_with`] keyed off [`PricingCatalog::embedded`].
pub fn compute_cost(source: &str, model: &str, tokens: CostTokens) -> CostBreakdown {
    compute_cost_with(PricingCatalog::embedded(), source, model, tokens)
}

/// Computes a cost breakdown against a caller-supplied catalog. Used by
/// `Store::recompute_costs_with` and catalog activation paths so persisted
/// events can be recalculated against an explicit catalog.
pub fn compute_cost_with(
    catalog: &PricingCatalog,
    source: &str,
    model: &str,
    tokens: CostTokens,
) -> CostBreakdown {
    let Some(pricing) = catalog.find(source, model) else {
        return CostBreakdown {
            pricing_status: PricingStatus::Unpriced,
            ..CostBreakdown::default()
        };
    };

    let prompt_tokens = tokens
        .input
        .max(0)
        .saturating_add(tokens.cache_read.max(0))
        .saturating_add(tokens.cache_creation.max(0)) as u64;
    let selected = pricing.rate_for_prompt_tokens(prompt_tokens);
    let rate = selected.rate;

    let input_mtok = tokens.input as f64 / 1_000_000.0;
    let cache_read_mtok = tokens.cache_read as f64 / 1_000_000.0;
    let cache_creation_mtok = tokens.cache_creation as f64 / 1_000_000.0;
    let output_mtok = tokens.output as f64 / 1_000_000.0;
    let reasoning_mtok = tokens.reasoning_output as f64 / 1_000_000.0;
    let reasoning_cost_usd = match rate.reasoning_policy {
        ReasoningPolicy::IncludedInOutput => 0.0,
        ReasoningPolicy::Separate => reasoning_mtok * rate.reasoning_per_mtok(),
    };

    let cost_with_cache_usd = input_mtok * rate.input_per_mtok
        + cache_read_mtok * rate.cached_per_mtok
        + cache_creation_mtok * rate.cache_creation_per_mtok()
        + output_mtok * rate.output_per_mtok
        + reasoning_cost_usd;
    let cost_without_cache_usd = (input_mtok + cache_read_mtok + cache_creation_mtok)
        * rate.input_per_mtok
        + output_mtok * rate.output_per_mtok
        + reasoning_cost_usd;

    CostBreakdown {
        cost_with_cache_usd,
        cost_without_cache_usd,
        pricing_status: catalog.status,
        pricing_source: Some(catalog.version.clone()),
        pricing_rate: serde_json::to_string(&serde_json::json!({
            "model_id": pricing.id,
            "tier": selected.tier,
            "prompt_tokens": prompt_tokens,
            "prompt_tokens_above": selected.prompt_tokens_above,
            "input_per_mtok": rate.input_per_mtok,
            "cached_per_mtok": rate.cached_per_mtok,
            "cache_creation_per_mtok": rate.cache_creation_per_mtok(),
            "output_per_mtok": rate.output_per_mtok,
            "reasoning_per_mtok": rate.reasoning_per_mtok().max(0.0),
            "reasoning_policy": rate.reasoning_policy.as_str(),
        }))
        .ok(),
    }
}

/// Backwards-compatible scalar API used by the legacy report aggregates
/// in `query/reports.rs` and `query/mod.rs::cost_breakdown`. New surfaces
/// should call [`compute_cost`] / [`compute_cost_with`] for the full
/// breakdown.
pub fn estimate_cost_usd(source: &str, model: &str, tokens: CostTokens) -> f64 {
    compute_cost(source, model, tokens).cost_with_cache_usd
}

#[cfg(test)]
mod tests {
    use super::{CostTokens, PricingStatus, compute_cost, compute_cost_with};
    use crate::query::pricing_catalog::{
        PricingCatalog, PricingEntry, PricingMatcher, PricingRate, ReasoningPolicy,
    };

    fn tokens(
        input: i64,
        cache_read: i64,
        cache_creation: i64,
        output: i64,
        reasoning_output: i64,
    ) -> CostTokens {
        CostTokens {
            input,
            cache_read,
            cache_creation,
            output,
            reasoning_output,
        }
    }

    /// Validates D6: a Codex/`gpt-5` event picks up the embedded rate row,
    /// produces non-zero cost columns, and stamps `pricing_source` for
    /// downstream re-keying.
    #[test]
    fn pricing_static_v2_hits_known_model() {
        let cost = compute_cost("codex", "gpt-5", tokens(1_000_000, 200_000, 0, 500_000, 0));
        assert_eq!(cost.pricing_status, PricingStatus::Static);
        assert_eq!(cost.pricing_source.as_deref(), Some("static-v2"));
        assert!(cost.cost_with_cache_usd > 0.0);
        // Without-cache lower-bounds: cache_read priced at full input rate.
        assert!(cost.cost_without_cache_usd > cost.cost_with_cache_usd);
        assert!(cost.pricing_rate.is_some());
    }

    #[test]
    fn pricing_static_v2_hits_current_gpt5_dotted_variants() {
        for model in ["gpt-5.5", "gpt-5.4", "gpt-5.4-mini"] {
            let cost = compute_cost("codex", model, tokens(1_000_000, 200_000, 0, 500_000, 0));

            assert_eq!(cost.pricing_status, PricingStatus::Static, "{model}");
            assert_eq!(cost.pricing_source.as_deref(), Some("static-v2"));
            assert!(cost.cost_with_cache_usd > 0.0, "{model}");
        }
    }

    #[test]
    fn pricing_static_v2_hits_claude_fable_and_mythos_5() {
        for model in ["claude-fable-5", "claude-mythos-5"] {
            let cost = compute_cost(
                "claude",
                model,
                tokens(1_000_000, 200_000, 300_000, 400_000, 0),
            );

            assert_eq!(cost.pricing_status, PricingStatus::Static, "{model}");
            assert_eq!(cost.pricing_source.as_deref(), Some("static-v2"));
            assert!((cost.cost_with_cache_usd - 33.95).abs() < 1e-9, "{model}");
            assert!((cost.cost_without_cache_usd - 35.0).abs() < 1e-9, "{model}");
            let pricing_rate = cost
                .pricing_rate
                .as_deref()
                .expect("matched Fable/Mythos rows should carry pricing_rate");
            assert!(pricing_rate.contains("\"input_per_mtok\":10.0"), "{model}");
            assert!(
                pricing_rate.contains("\"cache_creation_per_mtok\":12.5"),
                "{model}"
            );
            assert!(pricing_rate.contains("\"output_per_mtok\":50.0"), "{model}");
        }
    }

    #[test]
    fn pricing_gpt_5_6_uses_request_scoped_short_and_long_tiers() {
        let cases = [
            ("gpt-5.6-luna", 0.8, 1.300_002_5),
            ("gpt-5.6-terra", 2.0, 3.250_006_25),
            ("gpt-5.6-sol", 4.0, 6.500_012_5),
        ];

        for (model, expected_short, expected_long) in cases {
            let short = compute_cost("codex", model, tokens(100_000, 100_000, 72_000, 100_000, 0));
            assert!(
                (short.cost_with_cache_usd - expected_short).abs() < 1e-9,
                "{model}"
            );
            let short_rate = short.pricing_rate.expect("short tier audit row");
            assert!(short_rate.contains("\"tier\":\"default\""), "{short_rate}");
            assert!(
                short_rate.contains("\"prompt_tokens\":272000"),
                "{short_rate}"
            );

            let long = compute_cost("codex", model, tokens(100_000, 100_000, 72_001, 100_000, 0));
            assert!(
                (long.cost_with_cache_usd - expected_long).abs() < 1e-9,
                "{model}"
            );
            let long_rate = long.pricing_rate.expect("long tier audit row");
            assert!(
                long_rate.contains("\"tier\":\"long_context\""),
                "{long_rate}"
            );
            assert!(
                long_rate.contains("\"prompt_tokens_above\":272000"),
                "{long_rate}"
            );
        }
    }

    #[test]
    fn pricing_gpt_5_6_alias_resolves_to_sol() {
        let cost = compute_cost(
            "opencode",
            "gpt-5.6",
            tokens(100_000, 100_000, 72_000, 100_000, 0),
        );
        assert!((cost.cost_with_cache_usd - 4.0).abs() < 1e-9);
        let rate = cost.pricing_rate.expect("alias should be auditable");
        assert!(rate.contains("\"model_id\":\"gpt-5.6-sol\""), "{rate}");
    }

    #[test]
    fn pricing_rate_preserves_low_precision_rates() {
        let catalog = PricingCatalog::new(
            "precision-test",
            PricingStatus::Snapshot,
            vec![PricingEntry::new(
                "low-cache",
                "codex",
                vec![PricingMatcher::family("low-cache")],
                PricingRate {
                    input_per_mtok: 0.0005,
                    cached_per_mtok: 0.00005,
                    cache_creation_per_mtok: None,
                    output_per_mtok: 0.005,
                    reasoning_per_mtok: None,
                    reasoning_policy: Default::default(),
                },
            )],
        )
        .expect("test pricing catalog");
        let cost = compute_cost_with(
            &catalog,
            "codex",
            "low-cache",
            tokens(1_000, 1_000, 0, 1_000, 0),
        );
        let pricing_rate = cost
            .pricing_rate
            .expect("matched row should carry pricing_rate");
        assert!(
            pricing_rate.contains("0.00005"),
            "pricing_rate should not round low precision rates to zero: {pricing_rate}"
        );
    }

    #[test]
    fn pricing_counts_cache_creation_and_keeps_reasoning_included_by_default() {
        let catalog = PricingCatalog::new(
            "cache-test",
            PricingStatus::Snapshot,
            vec![PricingEntry::new(
                "claude-test",
                "claude",
                vec![PricingMatcher::family("claude-test")],
                PricingRate {
                    input_per_mtok: 3.0,
                    cached_per_mtok: 0.3,
                    cache_creation_per_mtok: Some(3.75),
                    output_per_mtok: 15.0,
                    reasoning_per_mtok: Some(99.0),
                    reasoning_policy: ReasoningPolicy::IncludedInOutput,
                },
            )],
        )
        .expect("test pricing catalog");

        let cost = compute_cost_with(
            &catalog,
            "claude",
            "claude-test",
            tokens(1_000_000, 2_000_000, 3_000_000, 4_000_000, 5_000_000),
        );

        assert!((cost.cost_with_cache_usd - 74.85).abs() < 1e-9);
        assert!((cost.cost_without_cache_usd - 78.0).abs() < 1e-9);
    }

    #[test]
    fn pricing_can_bill_reasoning_separately_when_catalog_requests_it() {
        let catalog = PricingCatalog::new(
            "reasoning-test",
            PricingStatus::Snapshot,
            vec![PricingEntry::new(
                "reasoning-model",
                "codex",
                vec![PricingMatcher::family("reasoning-model")],
                PricingRate {
                    input_per_mtok: 1.0,
                    cached_per_mtok: 0.1,
                    cache_creation_per_mtok: None,
                    output_per_mtok: 2.0,
                    reasoning_per_mtok: Some(4.0),
                    reasoning_policy: ReasoningPolicy::Separate,
                },
            )],
        )
        .expect("test pricing catalog");

        let cost = compute_cost_with(
            &catalog,
            "codex",
            "reasoning-model",
            tokens(1_000_000, 0, 0, 1_000_000, 1_000_000),
        );

        assert!((cost.cost_with_cache_usd - 7.0).abs() < 1e-9);
        assert!(
            cost.pricing_rate
                .as_deref()
                .is_some_and(|rate| rate.contains("\"reasoning_policy\":\"separate\""))
        );
    }

    /// Validates D6 fallthrough: an unknown model returns 0 cost and
    /// `Unpriced` status so dashboards can render the row instead of
    /// hiding the spend behind a fake number.
    #[test]
    fn pricing_unpriced_when_no_match() {
        let cost = compute_cost("codex", "made-up-model", tokens(1_000, 0, 0, 0, 0));
        assert_eq!(cost.pricing_status, PricingStatus::Unpriced);
        assert!(cost.pricing_source.is_none());
        assert_eq!(cost.cost_with_cache_usd, 0.0);
        assert_eq!(cost.cost_without_cache_usd, 0.0);
    }
}
