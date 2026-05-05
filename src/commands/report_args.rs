use anyhow::{Result, anyhow};
use chrono::{FixedOffset, NaiveDate};
use clap::{Args, ValueEnum};

use crate::{models::SourceKind, query::reports};

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum ReportOrderArg {
    Asc,
    #[default]
    Desc,
}

#[derive(Debug, Clone, Args, Default)]
pub struct ReportCommonArgs {
    /// Inclusive start date in YYYYMMDD format.
    #[arg(short = 's', long, value_name = "YYYYMMDD", value_parser = parse_report_date)]
    pub since: Option<String>,

    /// Inclusive end date in YYYYMMDD format.
    #[arg(short = 'u', long, value_name = "YYYYMMDD", value_parser = parse_report_date)]
    pub until: Option<String>,

    /// Emit stable JSON and suppress human-readable tables.
    #[arg(short = 'j', long)]
    pub json: bool,

    /// Include per-model breakdown rows/payloads where supported.
    #[arg(short = 'b', long)]
    pub breakdown: bool,

    /// Sort report rows by period/activity.
    #[arg(long, value_enum, default_value_t = ReportOrderArg::Desc)]
    pub order: ReportOrderArg,

    /// Report timezone: UTC, local, or a fixed offset such as +08:00.
    #[arg(short = 'z', long, default_value = "local", value_parser = parse_timezone)]
    pub timezone: String,

    /// Lightweight locale selector for titles/number formatting.
    #[arg(short = 'l', long, default_value = "en-US", value_parser = parse_locale)]
    pub locale: String,

    /// Use a narrower table layout.
    #[arg(long)]
    pub compact: bool,

    /// Restrict reports to one local source.
    #[arg(long, value_enum)]
    pub source: Option<SourceKind>,
}

impl ReportCommonArgs {
    pub fn to_filter(&self, project: Option<String>) -> Result<reports::ReportFilter> {
        Ok(reports::ReportFilter {
            since: self.since.as_deref().map(parse_date_value).transpose()?,
            until: self.until.as_deref().map(parse_date_value).transpose()?,
            order: match self.order {
                ReportOrderArg::Asc => reports::SortOrder::Asc,
                ReportOrderArg::Desc => reports::SortOrder::Desc,
            },
            timezone: parse_timezone_value(&self.timezone)?,
            locale: self.locale.clone(),
            source: self.source,
            project,
            breakdown: self.breakdown,
        })
    }
}

#[derive(Debug, Clone, Args, Default)]
pub struct DailyArgs {
    #[command(flatten)]
    pub common: ReportCommonArgs,

    /// Show the full daily history instead of defaulting to today.
    #[arg(long)]
    pub all: bool,

    /// Group daily rows by project/instance.
    #[arg(short = 'i', long)]
    pub instances: bool,

    /// Filter by project label, hash, or reference.
    #[arg(short = 'p', long)]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Args, Default)]
pub struct MonthlyArgs {
    #[command(flatten)]
    pub common: ReportCommonArgs,
}

#[derive(Debug, Clone, Args, Default)]
pub struct SessionArgs {
    #[command(flatten)]
    pub common: ReportCommonArgs,

    /// Show one session by exact or partial session id.
    #[arg(short = 'i', long = "id")]
    pub id: Option<String>,

    /// Filter by project label, hash, or reference.
    #[arg(short = 'p', long)]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Args, Default)]
pub struct BlocksArgs {
    #[command(flatten)]
    pub common: ReportCommonArgs,

    /// Only display the currently active block.
    #[arg(short = 'a', long)]
    pub active: bool,

    /// Only display recent blocks (last three days) plus the active block.
    #[arg(short = 'r', long)]
    pub recent: bool,

    /// Use an explicit token limit or `max` for the historical max block.
    #[arg(short = 't', long, value_name = "NUMBER|max", value_parser = parse_token_limit)]
    pub token_limit: Option<TokenLimitArg>,

    /// Session/block length in hours.
    #[arg(short = 'n', long, default_value = "5", value_parser = parse_positive_f64)]
    pub session_length: f64,
}

impl BlocksArgs {
    pub fn to_options(&self) -> reports::BlockReportOptions {
        reports::BlockReportOptions {
            active_only: self.active,
            recent_only: self.recent,
            token_limit: self.token_limit.as_ref().map(|value| match value {
                TokenLimitArg::Max => reports::TokenLimit::Max,
                TokenLimitArg::Value(value) => reports::TokenLimit::Value(*value),
            }),
            session_length_hours: self.session_length,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenLimitArg {
    Max,
    Value(u64),
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum CostSourceArg {
    #[default]
    Auto,
    Llmusage,
    Hook,
    Both,
}

#[derive(Debug, Clone, Args)]
pub struct StatuslineArgs {
    /// Enable statusline cache writes (default).
    #[arg(long = "cache", default_value_t = true)]
    pub cache: bool,

    /// Disable statusline cache fallback/write.
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Refresh interval in seconds for cache freshness.
    #[arg(long, default_value = "5", value_parser = parse_positive_u64)]
    pub refresh_interval: u64,

    /// Choose which local cost source to prefer.
    #[arg(long, value_enum, default_value_t = CostSourceArg::Auto)]
    pub cost_source: CostSourceArg,
}

impl Default for StatuslineArgs {
    fn default() -> Self {
        Self {
            cache: true,
            no_cache: false,
            refresh_interval: 5,
            cost_source: CostSourceArg::Auto,
        }
    }
}

impl StatuslineArgs {
    pub fn use_cache(&self) -> bool {
        self.cache && !self.no_cache
    }
}

fn parse_report_date(value: &str) -> std::result::Result<String, String> {
    parse_date_value(value)
        .map(|_| value.to_string())
        .map_err(|err| err.to_string())
}

fn parse_date_value(value: &str) -> Result<NaiveDate> {
    if value.len() != 8 || !value.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(anyhow!("date must use YYYYMMDD format"));
    }
    Ok(NaiveDate::parse_from_str(value, "%Y%m%d")?)
}

fn parse_timezone(value: &str) -> std::result::Result<String, String> {
    parse_timezone_value(value)
        .map(|_| value.to_string())
        .map_err(|err| err.to_string())
}

fn parse_timezone_value(value: &str) -> Result<reports::ReportTimezone> {
    if value.eq_ignore_ascii_case("utc") {
        return Ok(reports::ReportTimezone::Utc);
    }
    if value.eq_ignore_ascii_case("local") {
        return Ok(reports::ReportTimezone::Local);
    }
    if value.len() == 6 && matches!(&value[0..1], "+" | "-") && &value[3..4] == ":" {
        let sign = if &value[0..1] == "-" { -1 } else { 1 };
        let hours: i32 = value[1..3].parse()?;
        let minutes: i32 = value[4..6].parse()?;
        if hours > 23 || minutes > 59 {
            return Err(anyhow!("timezone offset must be between -23:59 and +23:59"));
        }
        let seconds = sign * (hours * 3600 + minutes * 60);
        let Some(offset) = FixedOffset::east_opt(seconds) else {
            return Err(anyhow!("invalid timezone offset"));
        };
        return Ok(reports::ReportTimezone::Fixed(offset));
    }
    Err(anyhow!(
        "timezone must be UTC, local, or a fixed offset like +08:00"
    ))
}

fn parse_locale(value: &str) -> std::result::Result<String, String> {
    match value {
        "en-US" | "zh-CN" | "ja-JP" => Ok(value.to_string()),
        _ => Err("locale must be one of en-US, zh-CN, ja-JP".to_string()),
    }
}

fn parse_token_limit(value: &str) -> std::result::Result<TokenLimitArg, String> {
    if value.eq_ignore_ascii_case("max") {
        return Ok(TokenLimitArg::Max);
    }
    let parsed = value
        .parse::<u64>()
        .map_err(|_| "token limit must be a positive integer or max".to_string())?;
    if parsed == 0 {
        return Err("token limit must be greater than zero".to_string());
    }
    Ok(TokenLimitArg::Value(parsed))
}

fn parse_positive_f64(value: &str) -> std::result::Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| "value must be a positive number".to_string())?;
    if parsed <= 0.0 || !parsed.is_finite() {
        return Err("value must be a finite positive number".to_string());
    }
    Ok(parsed)
}

fn parse_positive_u64(value: &str) -> std::result::Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| "value must be a positive integer".to_string())?;
    if parsed == 0 {
        return Err("value must be greater than zero".to_string());
    }
    Ok(parsed)
}
