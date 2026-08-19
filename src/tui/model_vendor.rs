//! Vendor identity and shade ranks for Models table coloring.

use std::collections::HashMap;

use crate::query::ModelBreakdown;

/// Infers a vendor id from a model id. Gateway prefixes still follow the vendor.
pub fn vendor_from_model(model: &str) -> &'static str {
    let lower = model.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    let has_token = |name: &str| tokens.contains(&name);
    let has_token_prefix = |prefix: &str| tokens.iter().any(|token| token.starts_with(prefix));

    if lower.contains("claude")
        || lower.contains("opus")
        || lower.contains("sonnet")
        || lower.contains("haiku")
        || has_token("fable")
    {
        "anthropic"
    } else if lower.contains("gpt")
        || lower.contains("chatgpt")
        || lower.contains("codex")
        || has_token_prefix("o1")
        || has_token_prefix("o3")
    {
        "openai"
    } else if lower.contains("gemini") {
        "google"
    } else if lower.contains("grok") {
        "xai"
    } else if has_token("glm") {
        "zai"
    } else if has_token("kimi") {
        "moonshot"
    } else if lower.contains("deepseek") {
        "deepseek"
    } else if lower.contains("llama") {
        "meta"
    } else {
        "unknown"
    }
}

/// Display name for a vendor id inferred from a model id.
pub fn vendor_display_name(vendor: &str) -> &'static str {
    match vendor {
        "anthropic" => "Anthropic",
        "openai" => "OpenAI",
        "google" => "Google",
        "xai" => "xAI",
        "zai" => "Zhipu",
        "moonshot" => "Moonshot",
        "deepseek" => "DeepSeek",
        "meta" => "Meta",
        _ => "Unknown",
    }
}

/// Builds a `model -> shade rank` map from the full payload.
///
/// Ranks are assigned inside each vendor so scrolling does not change colors.
pub fn build_shade_map(items: &[ModelBreakdown]) -> HashMap<String, usize> {
    let mut by_vendor: HashMap<&str, HashMap<&str, f64>> = HashMap::new();
    for item in items {
        let vendor = vendor_from_model(&item.model);
        let cost = if item.cost_with_cache_usd.is_finite() {
            item.cost_with_cache_usd
        } else {
            0.0
        };
        *by_vendor
            .entry(vendor)
            .or_default()
            .entry(item.model.as_str())
            .or_insert(0.0) += cost;
    }

    let mut map = HashMap::new();
    for (vendor, models) in by_vendor {
        let mut ranked: Vec<(&str, f64)> = models.into_iter().collect();
        ranked.sort_by(|left, right| {
            family_tier(vendor, left.0)
                .cmp(&family_tier(vendor, right.0))
                .then_with(|| model_version(right.0).cmp(&model_version(left.0)))
                .then_with(|| right.1.total_cmp(&left.1))
                .then_with(|| left.0.cmp(right.0))
        });
        for (rank, (name, _)) in ranked.iter().enumerate() {
            map.insert((*name).to_string(), rank.min(6));
        }
    }
    map
}

fn family_tier(vendor: &str, model: &str) -> u8 {
    if vendor != "anthropic" {
        return 0;
    }
    let lower = model.to_lowercase();
    if lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|token| token == "fable")
    {
        0
    } else if lower.contains("opus") {
        1
    } else if lower.contains("sonnet") {
        2
    } else if lower.contains("haiku") {
        3
    } else {
        4
    }
}

fn model_version(model: &str) -> (u32, u32) {
    let tokens: Vec<&str> = model
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    for (index, token) in tokens.iter().enumerate() {
        let Some(major) = leading_number(token) else {
            continue;
        };
        if major >= 1000 {
            return (0, 0);
        }
        let minor = tokens
            .get(index + 1)
            .and_then(|token| token.parse::<u32>().ok())
            .filter(|&value| value < 1000)
            .unwrap_or(0);
        return (major, minor);
    }
    (0, 0)
}

fn leading_number(token: &str) -> Option<u32> {
    let end = token
        .char_indices()
        .find(|(_, ch)| !ch.is_ascii_digit())
        .map_or(token.len(), |(index, _)| index);
    token[..end].parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(model: &str, cost: f64) -> ModelBreakdown {
        ModelBreakdown {
            model: model.to_string(),
            input_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 0,
            event_count: 0,
            cost_with_cache_usd: cost,
            cost_without_cache_usd: 0.0,
            cache_savings_usd: 0.0,
            pricing_status: "static".to_string(),
            pricing_source: None,
            pricing_rate: None,
            sources: Vec::new(),
        }
    }

    fn rank(map: &HashMap<String, usize>, model: &str) -> usize {
        *map.get(model).expect("shade rank")
    }

    #[test]
    fn fable_outranks_higher_cost_opus() {
        let map = build_shade_map(&[item("claude-opus-4-6", 900.0), item("claude-fable-5", 1.0)]);
        assert_eq!(rank(&map, "claude-fable-5"), 0);
        assert_eq!(rank(&map, "claude-opus-4-6"), 1);
    }

    #[test]
    fn unfabled_is_not_anthropic() {
        assert_eq!(vendor_from_model("unfabled-model"), "unknown");
        assert_eq!(vendor_from_model("fableton-1"), "unknown");
        assert_eq!(vendor_from_model("claude-fable-5"), "anthropic");
        assert_eq!(vendor_display_name("anthropic"), "Anthropic");
    }

    #[test]
    fn gateway_names_follow_model_vendor() {
        assert_eq!(
            vendor_from_model("github-copilot/claude-sonnet-4"),
            "anthropic"
        );
        assert_eq!(vendor_from_model("openrouter/gpt-4o"), "openai");
        assert_eq!(vendor_from_model("fireworks/glm-4.7"), "zai");
        assert_eq!(vendor_from_model("kimi-code/k3-256k"), "moonshot");
    }

    #[test]
    fn gpt_4o_version_outranks_older_numeric() {
        assert_eq!(model_version("gpt-4o"), (4, 0));
        assert_eq!(model_version("gpt-3.5-turbo"), (3, 5));
        assert_eq!(model_version("claude-opus-4-6"), (4, 6));
        assert_eq!(model_version("o1-2024-12-17"), (0, 0));
        let map = build_shade_map(&[item("gpt-3.5-turbo", 900.0), item("gpt-4o", 1.0)]);
        assert_eq!(rank(&map, "gpt-4o"), 0);
        assert_eq!(rank(&map, "gpt-3.5-turbo"), 1);
    }
}
