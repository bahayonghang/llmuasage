use chrono::NaiveDate;
use chrono_tz::Tz;
use llmusage::{
    ExplorerDimension, ExplorerFilters, ExplorerGranularity, ExplorerMetric, ExplorerQuery,
    ExplorerTokenType, LogsQuery, QueryFilter, ReportTimezone, SourceKind, SyncOptions,
    TopSessionsQuery, TopSessionsSort,
};
use serde::{Deserialize, Serialize};

use crate::error::DesktopError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilterDto {
    pub source: Option<String>,
    pub model: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub project_hash: Option<String>,
    pub host_id: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveRequest {
    pub request_id: u64,
    pub filter: FilterDto,
    pub window: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecondaryRequest {
    pub request_id: u64,
    pub filter: FilterDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopSessionsDto {
    pub request_id: u64,
    pub filter: FilterDto,
    pub sort: String,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorerDto {
    pub request_id: u64,
    pub filter: FilterDto,
    pub granularity: String,
    pub metric: String,
    pub group_by: String,
    pub session_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_kind: Option<String>,
    pub token_type: Option<String>,
    pub include_other: Option<bool>,
    pub include_non_tool: Option<bool>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsDto {
    pub request_id: u64,
    pub filter: FilterDto,
    pub page_size: u32,
    pub cursor: Option<String>,
    pub include_total: Option<bool>,
    pub include_raw_json: Option<bool>,
    pub session: Option<String>,
    pub event_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStartDto {
    pub source: Option<String>,
    pub recent_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelQueriesDto {
    pub request_ids: Vec<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuotaResponse {
    pub cache_hit: bool,
    pub report: llmusage::subscription::UsageFetchReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeInfoDto {
    pub version: String,
    pub root_dir: std::path::PathBuf,
    pub db_path: std::path::PathBuf,
    pub schema_version: u32,
    pub lock: Option<llmusage::store::WorkerLockMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefsDto {
    pub theme: String,
    pub locale: String,
    pub auto_refresh_ms: u64,
    pub filter: FilterDto,
    pub window: String,
    pub range_preset: String,
}

impl Default for PrefsDto {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            locale: "zh".to_string(),
            auto_refresh_ms: 0,
            filter: FilterDto::default(),
            window: "all".to_string(),
            range_preset: "all".to_string(),
        }
    }
}

pub fn convert_filter(dto: &FilterDto) -> Result<QueryFilter, DesktopError> {
    let source = match nonempty(dto.source.as_deref()) {
        None => None,
        Some(raw) => Some(
            SourceKind::parse_id(raw)
                .ok_or_else(|| DesktopError::invalid_request(format!("unknown source: {raw}")))?,
        ),
    };
    let since = parse_date("since", dto.since.as_deref())?;
    let until = parse_date("until", dto.until.as_deref())?;
    if let (Some(since), Some(until)) = (since, until)
        && until < since
    {
        return Err(DesktopError::invalid_request(
            "until must not be before since",
        ));
    }
    Ok(QueryFilter {
        source,
        model: nonempty(dto.model.as_deref()).map(str::to_string),
        since,
        until,
        project_hash: nonempty(dto.project_hash.as_deref()).map(str::to_string),
        host_id: nonempty(dto.host_id.as_deref()).map(str::to_string),
        timezone: convert_timezone(dto.timezone.as_deref())?,
    })
}

pub fn convert_timezone(value: Option<&str>) -> Result<ReportTimezone, DesktopError> {
    let Some(raw) = nonempty(value) else {
        return Ok(local_timezone());
    };
    if raw.eq_ignore_ascii_case("utc") || raw == "Z" {
        return Ok(ReportTimezone::Utc);
    }
    if raw.eq_ignore_ascii_case("local") {
        return Ok(ReportTimezone::Local);
    }
    raw.parse::<Tz>()
        .map(ReportTimezone::Iana)
        .map_err(|_| DesktopError::invalid_request(format!("unknown timezone: {raw}")))
}

pub fn convert_window(window: &str) -> Result<String, DesktopError> {
    match window.trim() {
        value @ ("day" | "week" | "month" | "all") => Ok(value.to_string()),
        other => Err(DesktopError::invalid_request(format!(
            "unknown window: {other}"
        ))),
    }
}

pub fn convert_explorer(dto: &ExplorerDto) -> Result<(u64, ExplorerQuery), DesktopError> {
    let filter = convert_filter(&dto.filter)?;
    let granularity = ExplorerGranularity::parse(&dto.granularity).ok_or_else(|| {
        DesktopError::invalid_request(format!("unknown granularity: {}", dto.granularity))
    })?;
    let metric = ExplorerMetric::parse(&dto.metric)
        .ok_or_else(|| DesktopError::invalid_request(format!("unknown metric: {}", dto.metric)))?;
    let group_by = ExplorerDimension::parse(&dto.group_by).ok_or_else(|| {
        DesktopError::invalid_request(format!("unknown group_by: {}", dto.group_by))
    })?;
    let token_type =
        match nonempty(dto.token_type.as_deref()) {
            None => None,
            Some(raw) => Some(ExplorerTokenType::parse(raw).ok_or_else(|| {
                DesktopError::invalid_request(format!("unknown token_type: {raw}"))
            })?),
        };
    let mut filters = ExplorerFilters {
        session_id: nonempty(dto.session_id.as_deref()).map(str::to_string),
        tool_name: nonempty(dto.tool_name.as_deref()).map(str::to_string),
        tool_kind: nonempty(dto.tool_kind.as_deref()).map(str::to_string),
        is_tool: None,
        token_type,
    };
    if dto.include_non_tool == Some(false) {
        filters.is_tool = Some(true);
    }
    Ok((
        dto.request_id,
        ExplorerQuery {
            filter,
            granularity,
            metric,
            group_by,
            filters,
            limit: dto.limit.unwrap_or(8).clamp(1, 50) as usize,
            include_other: dto.include_other.unwrap_or(true),
        },
    ))
}

pub fn convert_logs(dto: &LogsDto) -> Result<(u64, LogsQuery), DesktopError> {
    if dto.page_size != 20 {
        return Err(DesktopError::invalid_request(format!(
            "page_size must be 20, got {}",
            dto.page_size
        )));
    }
    Ok((
        dto.request_id,
        LogsQuery {
            filter: convert_filter(&dto.filter)?,
            page_size: 20,
            cursor: dto.cursor.clone(),
            include_total: dto.include_total.unwrap_or(false),
            include_raw_json: dto.include_raw_json.unwrap_or(false),
            session: dto.session.clone(),
            event_key: dto.event_key.clone(),
        },
    ))
}

pub fn convert_top_sessions(dto: &TopSessionsDto) -> Result<(u64, TopSessionsQuery), DesktopError> {
    let sort = TopSessionsSort::parse(&dto.sort)
        .ok_or_else(|| DesktopError::invalid_request(format!("unknown sort: {}", dto.sort)))?;
    let limit = match dto.limit {
        None | Some(0) => 10,
        Some(value) => value.clamp(1, 50),
    };
    Ok((
        dto.request_id,
        TopSessionsQuery {
            filter: convert_filter(&dto.filter)?,
            sort,
            limit,
        },
    ))
}

pub fn convert_sync(dto: &SyncStartDto) -> Result<SyncOptions, DesktopError> {
    if let Some(source) = nonempty(dto.source.as_deref())
        && SourceKind::parse_id(source).is_none()
    {
        return Err(DesktopError::invalid_request(format!(
            "unknown source: {source}"
        )));
    }
    if let Some(days) = dto.recent_days
        && !(1..=3650).contains(&days)
    {
        return Err(DesktopError::invalid_request(format!(
            "recent_days must be between 1 and 3650, got {days}"
        )));
    }
    Ok(SyncOptions {
        rebuild: false,
        recent_days: dto.recent_days,
        source: nonempty(dto.source.as_deref()).map(str::to_string),
        parallelism: None,
    })
}

pub fn validate_prefs(prefs: &PrefsDto) -> Result<(), DesktopError> {
    match prefs.theme.as_str() {
        "light" | "dark" => {}
        other => {
            return Err(DesktopError::invalid_request(format!(
                "unknown theme: {other}"
            )));
        }
    }
    match prefs.locale.as_str() {
        "zh" | "en" => {}
        other => {
            return Err(DesktopError::invalid_request(format!(
                "unknown locale: {other}"
            )));
        }
    }
    match prefs.auto_refresh_ms {
        0 | 30_000 | 60_000 => {}
        other => {
            return Err(DesktopError::invalid_request(format!(
                "auto_refresh_ms must be 0, 30000, or 60000, got {other}"
            )));
        }
    }
    match prefs.range_preset.as_str() {
        "1d" | "7d" | "30d" | "all" | "custom" => {}
        other => {
            return Err(DesktopError::invalid_request(format!(
                "unknown range_preset: {other}"
            )));
        }
    }
    convert_window(&prefs.window)?;
    convert_filter(&prefs.filter)?;
    Ok(())
}

fn local_timezone() -> ReportTimezone {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| name.parse::<Tz>().ok())
        .map(ReportTimezone::Iana)
        .unwrap_or(ReportTimezone::Local)
}

fn parse_date(label: &str, raw: Option<&str>) -> Result<Option<NaiveDate>, DesktopError> {
    let Some(raw) = nonempty(raw) else {
        return Ok(None);
    };
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map(Some)
        .map_err(|_| DesktopError::invalid_request(format!("invalid {label} date: {raw}")))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use llmusage::ValidatedSyncRequest;

    fn filter(timezone: &str) -> FilterDto {
        FilterDto {
            timezone: Some(timezone.to_string()),
            ..FilterDto::default()
        }
    }

    #[test]
    fn convert_filter_accepts_valid_iana() {
        let converted = convert_filter(&filter("Asia/Shanghai")).expect("valid IANA");
        assert_eq!(
            converted.timezone,
            ReportTimezone::Iana(chrono_tz::Asia::Shanghai)
        );
    }

    #[test]
    fn convert_filter_utc_and_local_aliases() {
        assert!(matches!(
            convert_filter(&filter("utc")).unwrap().timezone,
            ReportTimezone::Utc
        ));
        assert!(matches!(
            convert_filter(&filter("Z")).unwrap().timezone,
            ReportTimezone::Utc
        ));
        assert!(matches!(
            convert_filter(&filter("local")).unwrap().timezone,
            ReportTimezone::Local
        ));
    }

    #[test]
    fn convert_filter_rejects_invalid_iana() {
        let error = convert_filter(&filter("Not/AZone")).expect_err("unknown IANA");
        assert_eq!(error.code, "invalid_request");
    }

    #[test]
    fn convert_filter_rejects_invalid_date() {
        let dto = FilterDto {
            since: Some("2026-13-01".to_string()),
            ..FilterDto::default()
        };
        let error = convert_filter(&dto).expect_err("invalid date");
        assert_eq!(error.code, "invalid_request");
    }

    #[test]
    fn convert_filter_rejects_until_before_since() {
        let dto = FilterDto {
            since: Some("2026-05-02".to_string()),
            until: Some("2026-05-01".to_string()),
            ..FilterDto::default()
        };
        let error = convert_filter(&dto).expect_err("until < since");
        assert_eq!(error.code, "invalid_request");
    }

    #[test]
    fn convert_filter_rejects_unknown_source() {
        let dto = FilterDto {
            source: Some("not-a-source".to_string()),
            ..FilterDto::default()
        };
        let error = convert_filter(&dto).expect_err("unknown source");
        assert_eq!(error.code, "invalid_request");
    }

    #[test]
    fn convert_logs_requires_page_size_20() {
        let dto = LogsDto {
            request_id: 1,
            filter: FilterDto::default(),
            page_size: 10,
            cursor: None,
            include_total: None,
            include_raw_json: None,
            session: None,
            event_key: None,
        };
        let error = convert_logs(&dto).expect_err("page_size");
        assert_eq!(error.code, "invalid_request");
        let ok = LogsDto {
            page_size: 20,
            ..dto
        };
        let (_, query) = convert_logs(&ok).expect("page_size 20");
        assert_eq!(query.page_size, 20);
    }

    #[test]
    fn convert_explorer_rejects_unknown_enum_and_sets_is_tool() {
        let mut dto = ExplorerDto {
            request_id: 1,
            filter: FilterDto::default(),
            granularity: "day".to_string(),
            metric: "not-a-metric".to_string(),
            group_by: "source".to_string(),
            session_id: None,
            tool_name: None,
            tool_kind: None,
            token_type: None,
            include_other: None,
            include_non_tool: Some(false),
            limit: None,
        };
        let error = convert_explorer(&dto).expect_err("unknown metric");
        assert_eq!(error.code, "invalid_request");
        dto.metric = "tokens".to_string();
        let (_, query) = convert_explorer(&dto).expect("valid explorer");
        assert_eq!(query.filters.is_tool, Some(true));
        assert_eq!(query.limit, 8);
    }

    #[test]
    fn convert_sync_forces_rebuild_false() {
        let dto = SyncStartDto {
            source: Some("codex".to_string()),
            recent_days: Some(7),
        };
        let options = convert_sync(&dto).expect("valid sync");
        let validated = ValidatedSyncRequest::new(options.clone()).expect("validated");
        assert!(!options.rebuild);
        assert!(!validated.rebuild());
        assert_eq!(validated.recent_days(), Some(7));
        assert_eq!(options.source.as_deref(), Some("codex"));
        assert_eq!(options.parallelism, None);
    }

    #[test]
    fn convert_sync_rejects_recent_days_zero() {
        let dto = SyncStartDto {
            source: None,
            recent_days: Some(0),
        };
        let error = convert_sync(&dto).expect_err("recent_days 0");
        assert_eq!(error.code, "invalid_request");
    }

    #[test]
    fn convert_sync_ignores_rebuild_in_raw_json() {
        let raw = serde_json::json!({
            "source": "codex",
            "recent_days": 7,
            "rebuild": true
        });
        let dto: SyncStartDto = serde_json::from_value(raw).expect("dto");
        let options = convert_sync(&dto).expect("convert");
        assert!(!options.rebuild);
        assert!(
            !ValidatedSyncRequest::new(options)
                .expect("validated")
                .rebuild()
        );
    }
}
