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
}
