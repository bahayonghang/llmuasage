/// Static model-price helpers used by dashboard and report queries.
pub fn estimate_cost_usd(
    source: &str,
    model: &str,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
) -> f64 {
    let normalized_model = model.to_ascii_lowercase();
    let pricing = PRICE_CATALOG
        .iter()
        .find(|entry| {
            entry.source.eq_ignore_ascii_case(source)
                && entry
                    .matchers
                    .iter()
                    .any(|matcher| normalized_model.contains(matcher))
        })
        .or_else(|| PRICE_CATALOG.iter().find(|entry| entry.source == "*"));

    let Some(pricing) = pricing else {
        return 0.0;
    };

    let input_mtok = input_tokens as f64 / 1_000_000.0;
    let cached_mtok = cached_input_tokens as f64 / 1_000_000.0;
    let output_mtok = (output_tokens + reasoning_output_tokens) as f64 / 1_000_000.0;
    input_mtok * pricing.input_per_mtok
        + cached_mtok * pricing.cached_per_mtok
        + output_mtok * pricing.output_per_mtok
}

struct PriceEntry {
    source: &'static str,
    matchers: &'static [&'static str],
    input_per_mtok: f64,
    cached_per_mtok: f64,
    output_per_mtok: f64,
}

const PRICE_CATALOG: &[PriceEntry] = &[
    PriceEntry {
        source: "codex",
        matchers: &["gpt-5-mini"],
        input_per_mtok: 0.25,
        cached_per_mtok: 0.025,
        output_per_mtok: 2.0,
    },
    PriceEntry {
        source: "codex",
        matchers: &["gpt-5", "o3", "o4"],
        input_per_mtok: 1.25,
        cached_per_mtok: 0.125,
        output_per_mtok: 10.0,
    },
    PriceEntry {
        source: "claude",
        matchers: &["opus"],
        input_per_mtok: 15.0,
        cached_per_mtok: 1.5,
        output_per_mtok: 75.0,
    },
    PriceEntry {
        source: "claude",
        matchers: &["sonnet", "claude-3-7"],
        input_per_mtok: 3.0,
        cached_per_mtok: 0.3,
        output_per_mtok: 15.0,
    },
    PriceEntry {
        source: "opencode",
        matchers: &["gpt", "o3", "o4"],
        input_per_mtok: 1.25,
        cached_per_mtok: 0.125,
        output_per_mtok: 10.0,
    },
    PriceEntry {
        source: "*",
        matchers: &[""],
        input_per_mtok: 0.0,
        cached_per_mtok: 0.0,
        output_per_mtok: 0.0,
    },
];
