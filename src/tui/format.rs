//! Shared formatting helpers for terminal presentation.

/// Formats an integer with comma-separated thousands groups.
pub fn grouped(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let raw = value.unsigned_abs().to_string();
    let mut reversed = String::with_capacity(raw.len() + raw.len() / 3);
    for (index, ch) in raw.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            reversed.push(',');
        }
        reversed.push(ch);
    }
    format!("{sign}{}", reversed.chars().rev().collect::<String>())
}

/// Formats dashboard token counts using the historical one-decimal k/M form.
pub fn tokens(value: i64) -> String {
    if value.unsigned_abs() >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value.unsigned_abs() >= 10_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        grouped(value)
    }
}

/// Formats compact footer counts without grouping small values.
pub fn footer_compact(value: i64) -> String {
    if value.unsigned_abs() >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value.unsigned_abs() >= 10_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

/// Formats trend-axis counts using whole-number k/M suffixes.
pub fn axis_compact(value: i64) -> String {
    if value.unsigned_abs() >= 1_000_000 {
        format!("{:.0}M", value as f64 / 1_000_000.0)
    } else if value.unsigned_abs() >= 10_000 {
        format!("{:.0}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

/// Formats interactive TUI statistics using one-decimal K/M/B/T units.
pub fn stat_compact(value: i64) -> String {
    const UNITS: [(u64, &str); 4] = [
        (1_000, "K"),
        (1_000_000, "M"),
        (1_000_000_000, "B"),
        (1_000_000_000_000, "T"),
    ];

    let abs = value.unsigned_abs();
    let Some(mut unit_index) = UNITS.iter().rposition(|(divisor, _)| abs >= *divisor) else {
        return value.to_string();
    };

    let mut rounded_tenths = round_to_tenths(abs, UNITS[unit_index].0);
    if rounded_tenths >= 10_000 && unit_index + 1 < UNITS.len() {
        unit_index += 1;
        rounded_tenths = round_to_tenths(abs, UNITS[unit_index].0);
    }

    let sign = if value < 0 { "-" } else { "" };
    let whole = rounded_tenths / 10;
    let fraction = rounded_tenths % 10;
    let suffix = UNITS[unit_index].1;
    if fraction == 0 {
        format!("{sign}{whole}{suffix}")
    } else {
        format!("{sign}{whole}.{fraction}{suffix}")
    }
}

fn round_to_tenths(value: u64, divisor: u64) -> u128 {
    (u128::from(value) * 10 + u128::from(divisor) / 2) / u128::from(divisor)
}

/// Formats report-table token counts using two-decimal K/M/B suffixes.
pub fn token_compact(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let abs = value.unsigned_abs() as f64;
    let (scaled, suffix) = if abs >= 1_000_000_000.0 {
        (abs / 1_000_000_000.0, "B")
    } else if abs >= 1_000_000.0 {
        (abs / 1_000_000.0, "M")
    } else if abs >= 1_000.0 {
        (abs / 1_000.0, "K")
    } else {
        return value.to_string();
    };
    format!("{sign}{scaled:.2}{suffix}")
}

pub fn cost(value: f64) -> String {
    format!("${value:.2}")
}

pub fn percent_ratio(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

pub fn metric_value(value: f64) -> String {
    if (0.0..=1.0).contains(&value) {
        percent_ratio(value)
    } else {
        format!("{value:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_existing_terminal_formats() {
        assert_eq!(grouped(1_234_567), "1,234,567");
        assert_eq!(grouped(-1_234), "-1,234");
        assert_eq!(tokens(9_999), "9,999");
        assert_eq!(tokens(10_000), "10.0k");
        assert_eq!(footer_compact(9_999), "9999");
        assert_eq!(axis_compact(12_500), "12k");
        assert_eq!(token_compact(978_050), "978.05K");
        assert_eq!(token_compact(5_370_000), "5.37M");
        assert_eq!(token_compact(40_330_000_000), "40.33B");
        assert_eq!(cost(12.345), "$12.35");
        assert_eq!(percent_ratio(0.125), "12.5%");
        assert_eq!(metric_value(42.0), "42.00");
    }

    #[test]
    fn stat_compact_uses_one_decimal_uppercase_units() {
        for (value, expected) in [
            (0, "0"),
            (999, "999"),
            (1_000, "1K"),
            (1_050, "1.1K"),
            (12_500, "12.5K"),
            (1_000_000, "1M"),
            (288_694_891, "288.7M"),
            (1_000_000_000, "1B"),
            (18_214_785_227, "18.2B"),
            (1_000_000_000_000, "1T"),
        ] {
            assert_eq!(stat_compact(value), expected, "value={value}");
        }
    }

    #[test]
    fn stat_compact_promotes_rounded_values_and_handles_signed_extremes() {
        for (value, expected) in [
            (999_949, "999.9K"),
            (999_950, "1M"),
            (999_949_999, "999.9M"),
            (999_950_000, "1B"),
            (999_949_999_999, "999.9B"),
            (999_950_000_000, "1T"),
            (-12_500, "-12.5K"),
            (i64::MAX, "9223372T"),
            (i64::MIN, "-9223372T"),
        ] {
            assert_eq!(stat_compact(value), expected, "value={value}");
        }
    }

    #[test]
    fn analytical_panels_use_compact_stats_while_sync_counts_stay_exact() {
        for (name, source) in [
            ("overview", include_str!("panels/overview.rs")),
            ("models", include_str!("panels/models.rs")),
            ("daily", include_str!("panels/daily.rs")),
            ("hourly", include_str!("panels/hourly.rs")),
            ("cost", include_str!("panels/cost.rs")),
            ("stats", include_str!("panels/stats.rs")),
            ("behavior", include_str!("panels/behavior.rs")),
            ("blocks", include_str!("panels/blocks.rs")),
        ] {
            assert!(
                source.contains("stat_compact"),
                "{name} must use the shared compact statistic formatter"
            );
            assert!(
                !source.contains("format::grouped") && !source.contains("grouped as"),
                "{name} must not fall back to exact grouped analytics counts"
            );
        }

        let usage = include_str!("panels/usage.rs");
        assert!(usage.contains("grouped as format_number"));
        assert!(!usage.contains("stat_compact"));
    }
}
