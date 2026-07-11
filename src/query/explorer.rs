use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, params_from_iter, types::Value};
use serde::Serialize;

use super::{Dashboard, QueryFilter, filter::SqlFilter};
use crate::error::Result;

const DEFAULT_LIMIT: usize = 8;
const MAX_LIMIT: usize = 50;
const OTHER_KEY: &str = "__other__";
const OTHER_LABEL: &str = "Other";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplorerGranularity {
    Total,
    Day,
    Week,
    Month,
}

impl ExplorerGranularity {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "total" => Some(Self::Total),
            "day" | "daily" => Some(Self::Day),
            "week" | "weekly" => Some(Self::Week),
            "month" | "monthly" => Some(Self::Month),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplorerMetric {
    AttributedCostUsd,
    Calls,
    Turns,
    Sessions,
    TotalTokens,
}

impl ExplorerMetric {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "attributed_cost_usd" | "cost" | "estimated_cost_usd" => Some(Self::AttributedCostUsd),
            "calls" => Some(Self::Calls),
            "turns" => Some(Self::Turns),
            "sessions" => Some(Self::Sessions),
            "total_tokens" | "tokens" => Some(Self::TotalTokens),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplorerDimension {
    Source,
    Model,
    Project,
    Session,
    Tool,
    ToolKind,
    IsTool,
    TokenType,
}

impl ExplorerDimension {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "source" => Some(Self::Source),
            "model" => Some(Self::Model),
            "project" => Some(Self::Project),
            "session" => Some(Self::Session),
            "tool" => Some(Self::Tool),
            "tool_kind" | "toolkind" => Some(Self::ToolKind),
            "is_tool" | "istool" => Some(Self::IsTool),
            "token_type" | "tokentype" => Some(Self::TokenType),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplorerTokenType {
    Input,
    CacheRead,
    CacheCreation,
    Output,
    ReasoningOutput,
}

impl ExplorerTokenType {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "input" => Some(Self::Input),
            "cache_read" | "cacheread" => Some(Self::CacheRead),
            "cache_creation" | "cachecreation" => Some(Self::CacheCreation),
            "output" => Some(Self::Output),
            "reasoning_output" | "reasoningoutput" => Some(Self::ReasoningOutput),
            _ => None,
        }
    }

    fn event_expr(self, _alias: &str) -> &'static str {
        match self {
            Self::Input => "COALESCE(e.input_tokens, 0)",
            Self::CacheRead => "COALESCE(e.cache_read_tokens, 0)",
            Self::CacheCreation => "COALESCE(e.cache_creation_tokens, 0)",
            Self::Output => "COALESCE(e.output_tokens, 0)",
            Self::ReasoningOutput => "COALESCE(e.reasoning_output_tokens, 0)",
        }
    }

    fn turn_expr(self) -> &'static str {
        match self {
            Self::Input => "COALESCE(t.input_tokens, 0)",
            Self::CacheRead => "COALESCE(t.cache_read_tokens, 0)",
            Self::CacheCreation => "COALESCE(t.cache_creation_tokens, 0)",
            Self::Output => "COALESCE(t.output_tokens, 0)",
            Self::ReasoningOutput => "COALESCE(t.reasoning_output_tokens, 0)",
        }
    }

    fn attributed_expr(self) -> &'static str {
        match self {
            Self::Input => "COALESCE(a.input_tokens, 0.0)",
            Self::CacheRead => "COALESCE(a.cache_read_tokens, 0.0)",
            Self::CacheCreation => "COALESCE(a.cache_creation_tokens, 0.0)",
            Self::Output => "COALESCE(a.output_tokens, 0.0)",
            Self::ReasoningOutput => "COALESCE(a.reasoning_output_tokens, 0.0)",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExplorerFilters {
    pub session_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_kind: Option<String>,
    pub is_tool: Option<bool>,
    pub token_type: Option<ExplorerTokenType>,
}

#[derive(Debug, Clone)]
pub struct ExplorerQuery {
    pub filter: QueryFilter,
    pub granularity: ExplorerGranularity,
    pub metric: ExplorerMetric,
    pub group_by: ExplorerDimension,
    pub filters: ExplorerFilters,
    pub limit: usize,
    pub include_other: bool,
}

impl Default for ExplorerQuery {
    fn default() -> Self {
        Self {
            filter: QueryFilter::default(),
            granularity: ExplorerGranularity::Day,
            metric: ExplorerMetric::AttributedCostUsd,
            group_by: ExplorerDimension::Source,
            filters: ExplorerFilters::default(),
            limit: DEFAULT_LIMIT,
            include_other: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplorerSupport {
    pub supported: bool,
    pub level: String,
    pub reason: Option<String>,
    pub strategy: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ExplorerTotals {
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplorerRow {
    pub key: String,
    pub label: String,
    pub value: f64,
    pub share: f64,
    pub is_other: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplorerSeriesPoint {
    pub bucket: String,
    pub key: String,
    pub label: String,
    pub value: f64,
    pub is_other: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplorerPayload {
    pub support: ExplorerSupport,
    pub warning: Option<String>,
    pub granularity: ExplorerGranularity,
    pub metric: ExplorerMetric,
    pub group_by: ExplorerDimension,
    pub limit: usize,
    pub include_other: bool,
    pub totals: ExplorerTotals,
    pub rows: Vec<ExplorerRow>,
    pub series: Vec<ExplorerSeriesPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExplorerStrategy {
    Bucket,
    Event,
    Turn,
    Attribution,
}

impl ExplorerStrategy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bucket => "bucket",
            Self::Event => "event",
            Self::Turn => "turn",
            Self::Attribution => "attribution",
        }
    }
}

#[derive(Debug, Clone)]
struct CapabilityScope {
    support: ExplorerSupport,
    allowed_sources: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
struct GroupValue {
    key: String,
    label: String,
    value: f64,
}

#[derive(Debug, Clone)]
struct SeriesValue {
    bucket: String,
    key: String,
    label: String,
    value: f64,
}

#[derive(Debug, Clone)]
struct GroupSpec {
    key_expr: String,
    label_expr: String,
}

pub(super) fn load(dashboard: &Dashboard, query: &ExplorerQuery) -> Result<ExplorerPayload> {
    let query = sanitize_query(query);
    if let Some(reason) = unsupported_reason(&query) {
        return Ok(empty_payload(
            &query,
            ExplorerSupport {
                supported: false,
                level: "unsupported".to_string(),
                reason: Some(reason.clone()),
                strategy: "none".to_string(),
            },
            Some(reason),
        ));
    }

    let strategy = choose_strategy(&query);
    let scope = capability_scope(&dashboard.conn, &query, strategy)?;
    if !scope.support.supported {
        return Ok(empty_payload(
            &query,
            scope.support.clone(),
            scope.support.reason.clone(),
        ));
    }

    let all_rows = match strategy {
        ExplorerStrategy::Bucket => load_bucket_rows(&dashboard.conn, &query)?,
        ExplorerStrategy::Event => load_event_rows(&dashboard.conn, &query)?,
        ExplorerStrategy::Turn => load_turn_rows(&dashboard.conn, &query, &scope)?,
        ExplorerStrategy::Attribution => load_attribution_rows(&dashboard.conn, &query, &scope)?,
    };
    let total = all_rows.iter().map(|row| row.value).sum::<f64>();
    let selected = select_rows(&all_rows, query.limit, query.include_other, total);
    let series = if matches!(query.granularity, ExplorerGranularity::Total) {
        Vec::new()
    } else {
        let all_series = match strategy {
            ExplorerStrategy::Bucket => load_bucket_series(&dashboard.conn, &query)?,
            ExplorerStrategy::Event => load_event_series(&dashboard.conn, &query)?,
            ExplorerStrategy::Turn => load_turn_series(&dashboard.conn, &query, &scope)?,
            ExplorerStrategy::Attribution => {
                load_attribution_series(&dashboard.conn, &query, &scope)?
            }
        };
        collapse_series(&all_series, &selected)
    };

    let warning = if scope.support.level == "normalized" {
        None
    } else {
        scope.support.reason.clone()
    };

    Ok(ExplorerPayload {
        support: scope.support,
        warning,
        granularity: query.granularity,
        metric: query.metric,
        group_by: query.group_by,
        limit: query.limit,
        include_other: query.include_other,
        totals: ExplorerTotals { value: total },
        rows: selected,
        series,
    })
}

fn sanitize_query(query: &ExplorerQuery) -> ExplorerQuery {
    let mut cloned = query.clone();
    cloned.limit = cloned.limit.clamp(1, MAX_LIMIT);
    cloned
}

fn unsupported_reason(query: &ExplorerQuery) -> Option<String> {
    if query.filters.token_type.is_some() && query.metric != ExplorerMetric::TotalTokens {
        return Some("token_type filters only support the total_tokens metric.".to_string());
    }
    if query.group_by == ExplorerDimension::TokenType && query.metric != ExplorerMetric::TotalTokens
    {
        return Some("token_type groupings only support the total_tokens metric.".to_string());
    }
    None
}

fn choose_strategy(query: &ExplorerQuery) -> ExplorerStrategy {
    let uses_tool_scope = matches!(
        query.group_by,
        ExplorerDimension::Tool | ExplorerDimension::ToolKind | ExplorerDimension::IsTool
    ) || query.filters.tool_name.is_some()
        || query.filters.tool_kind.is_some()
        || query.filters.is_tool.is_some();

    if uses_tool_scope {
        ExplorerStrategy::Attribution
    } else if query.metric == ExplorerMetric::Turns {
        ExplorerStrategy::Turn
    } else if bucket_compatible(query) {
        ExplorerStrategy::Bucket
    } else {
        ExplorerStrategy::Event
    }
}

fn bucket_compatible(query: &ExplorerQuery) -> bool {
    matches!(
        query.group_by,
        ExplorerDimension::Source | ExplorerDimension::Model | ExplorerDimension::Project
    ) && matches!(
        query.metric,
        ExplorerMetric::AttributedCostUsd | ExplorerMetric::Calls | ExplorerMetric::TotalTokens
    ) && query.filters.session_id.is_none()
        && query.filters.tool_name.is_none()
        && query.filters.tool_kind.is_none()
        && query.filters.is_tool.is_none()
        && query.filters.token_type.is_none()
}

fn capability_scope(
    conn: &Connection,
    query: &ExplorerQuery,
    strategy: ExplorerStrategy,
) -> Result<CapabilityScope> {
    match strategy {
        ExplorerStrategy::Bucket => {
            let count = filtered_bucket_count(conn, query)?;
            let support = if count > 0 {
                ExplorerSupport {
                    supported: true,
                    level: "normalized".to_string(),
                    reason: None,
                    strategy: strategy.as_str().to_string(),
                }
            } else {
                ExplorerSupport {
                    supported: false,
                    level: "no_data".to_string(),
                    reason: Some("No usage events match this filter.".to_string()),
                    strategy: strategy.as_str().to_string(),
                }
            };
            Ok(CapabilityScope {
                support,
                allowed_sources: None,
            })
        }
        ExplorerStrategy::Event => {
            let count = filtered_event_count(conn, query)?;
            let support = if count > 0 {
                ExplorerSupport {
                    supported: true,
                    level: "normalized".to_string(),
                    reason: None,
                    strategy: strategy.as_str().to_string(),
                }
            } else {
                ExplorerSupport {
                    supported: false,
                    level: "no_data".to_string(),
                    reason: Some("No usage events match this filter.".to_string()),
                    strategy: strategy.as_str().to_string(),
                }
            };
            Ok(CapabilityScope {
                support,
                allowed_sources: None,
            })
        }
        ExplorerStrategy::Turn => behavior_scope(conn, query, strategy, "usage_turn", "turn"),
        ExplorerStrategy::Attribution => {
            behavior_scope(conn, query, strategy, "usage_tool_call", "tool attribution")
        }
    }
}

fn filtered_bucket_count(conn: &Connection, query: &ExplorerQuery) -> Result<i64> {
    let filter = query.filter.bucket_filter(Some("b"));
    let sql = format!(
        "SELECT COALESCE(SUM(b.event_count), 0) FROM usage_bucket_30m b{}",
        filter.where_sql()
    );
    Ok(
        conn.query_row(&sql, params_from_iter(filter.params().iter()), |row| {
            row.get(0)
        })?,
    )
}

fn behavior_scope(
    conn: &Connection,
    query: &ExplorerQuery,
    strategy: ExplorerStrategy,
    capability_table: &str,
    scope_label: &str,
) -> Result<CapabilityScope> {
    let event_count = filtered_event_count(conn, query)?;
    if event_count == 0 {
        return Ok(CapabilityScope {
            support: ExplorerSupport {
                supported: false,
                level: "no_data".to_string(),
                reason: Some("No usage events match this filter.".to_string()),
                strategy: strategy.as_str().to_string(),
            },
            allowed_sources: Some(Vec::new()),
        });
    }

    let in_scope = filtered_event_sources(conn, query)?;
    let capable = capability_sources(conn, capability_table)?;
    let allowed = in_scope
        .iter()
        .filter(|source| capable.contains(*source))
        .cloned()
        .collect::<Vec<_>>();
    let omitted = in_scope
        .iter()
        .filter(|source| !capable.contains(*source))
        .cloned()
        .collect::<Vec<_>>();

    let support = if allowed.is_empty() {
        ExplorerSupport {
            supported: false,
            level: "degraded".to_string(),
            reason: Some(format!(
                "Current source scope has usage data, but no normalized {scope_label} facts are available for: {}.",
                omitted.join(", ")
            )),
            strategy: strategy.as_str().to_string(),
        }
    } else if omitted.is_empty() {
        ExplorerSupport {
            supported: true,
            level: "normalized".to_string(),
            reason: None,
            strategy: strategy.as_str().to_string(),
        }
    } else {
        ExplorerSupport {
            supported: true,
            level: "degraded".to_string(),
            reason: Some(format!(
                "Omitted source(s) without normalized {scope_label} facts: {}.",
                omitted.join(", ")
            )),
            strategy: strategy.as_str().to_string(),
        }
    };

    Ok(CapabilityScope {
        support,
        allowed_sources: Some(allowed),
    })
}

fn filtered_event_count(conn: &Connection, query: &ExplorerQuery) -> Result<i64> {
    let mut filter = query.filter.event_filter(Some("e"));
    apply_session_filter(&mut filter, Some("e"), query.filters.session_id.as_deref());
    let sql = format!("SELECT COUNT(*) FROM usage_event e{}", filter.where_sql());
    Ok(
        conn.query_row(&sql, params_from_iter(filter.params().iter()), |row| {
            row.get(0)
        })?,
    )
}

fn filtered_event_sources(conn: &Connection, query: &ExplorerQuery) -> Result<Vec<String>> {
    let mut filter = query.filter.event_filter(Some("e"));
    apply_session_filter(&mut filter, Some("e"), query.filters.session_id.as_deref());
    let sql = format!(
        "SELECT DISTINCT e.source FROM usage_event e{} ORDER BY e.source ASC",
        filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(filter.params().iter()), |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<String>>>()?)
}

fn capability_sources(conn: &Connection, table: &str) -> Result<BTreeSet<String>> {
    let sql = format!("SELECT DISTINCT source FROM {table}");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .collect())
}

fn load_event_rows(conn: &Connection, query: &ExplorerQuery) -> Result<Vec<GroupValue>> {
    if query.group_by == ExplorerDimension::TokenType {
        return load_event_token_type_rows(conn, query);
    }
    let spec = event_group_spec(query.group_by);
    let mut filter = query.filter.event_filter(Some("e"));
    apply_session_filter(&mut filter, Some("e"), query.filters.session_id.as_deref());
    let value_expr = event_metric_expr(query.metric, query.filters.token_type);
    let sql = format!(
        r#"
        SELECT
            {key_expr} AS group_key,
            {label_expr} AS group_label,
            {value_expr} AS metric_value
        FROM usage_event e
        {where_sql}
        GROUP BY group_key, group_label
        ORDER BY metric_value DESC, group_label ASC
        "#,
        key_expr = spec.key_expr,
        label_expr = spec.label_expr,
        value_expr = value_expr,
        where_sql = filter.where_sql()
    );
    query_group_values(conn, &sql, &filter)
}

fn load_bucket_rows(conn: &Connection, query: &ExplorerQuery) -> Result<Vec<GroupValue>> {
    let spec = bucket_group_spec(query.group_by);
    let filter = query.filter.bucket_filter(Some("b"));
    let value_expr = bucket_metric_expr(query.metric);
    let sql = format!(
        r#"
        SELECT
            {key_expr} AS group_key,
            {label_expr} AS group_label,
            {value_expr} AS metric_value
        FROM usage_bucket_30m b
        {where_sql}
        GROUP BY group_key, group_label
        ORDER BY metric_value DESC, group_label ASC
        "#,
        key_expr = spec.key_expr,
        label_expr = spec.label_expr,
        value_expr = value_expr,
        where_sql = filter.where_sql()
    );
    query_group_values(conn, &sql, &filter)
}

fn load_bucket_series(conn: &Connection, query: &ExplorerQuery) -> Result<Vec<SeriesValue>> {
    let spec = bucket_group_spec(query.group_by);
    let bucket = bucket_expr(
        query.granularity,
        "b.hour_start",
        &query.filter.local_time_modifier(),
    );
    let filter = query.filter.bucket_filter(Some("b"));
    let value_expr = bucket_metric_expr(query.metric);
    let sql = format!(
        r#"
        SELECT
            {bucket} AS bucket_key,
            {key_expr} AS group_key,
            {label_expr} AS group_label,
            {value_expr} AS metric_value
        FROM usage_bucket_30m b
        {where_sql}
        GROUP BY bucket_key, group_key, group_label
        ORDER BY bucket_key ASC, metric_value DESC, group_label ASC
        "#,
        bucket = bucket,
        key_expr = spec.key_expr,
        label_expr = spec.label_expr,
        value_expr = value_expr,
        where_sql = filter.where_sql()
    );
    query_series_values(conn, &sql, &filter)
}

fn load_event_series(conn: &Connection, query: &ExplorerQuery) -> Result<Vec<SeriesValue>> {
    if query.group_by == ExplorerDimension::TokenType {
        return load_event_token_type_series(conn, query);
    }
    let spec = event_group_spec(query.group_by);
    let bucket_expr = bucket_expr(
        query.granularity,
        "e.event_at",
        &query.filter.local_time_modifier(),
    );
    let mut filter = query.filter.event_filter(Some("e"));
    apply_session_filter(&mut filter, Some("e"), query.filters.session_id.as_deref());
    let value_expr = event_metric_expr(query.metric, query.filters.token_type);
    let sql = format!(
        r#"
        SELECT
            {bucket_expr} AS bucket_key,
            {key_expr} AS group_key,
            {label_expr} AS group_label,
            {value_expr} AS metric_value
        FROM usage_event e
        {where_sql}
        GROUP BY bucket_key, group_key, group_label
        ORDER BY bucket_key ASC, metric_value DESC, group_label ASC
        "#,
        bucket_expr = bucket_expr,
        key_expr = spec.key_expr,
        label_expr = spec.label_expr,
        value_expr = value_expr,
        where_sql = filter.where_sql()
    );
    query_series_values(conn, &sql, &filter)
}

fn load_turn_rows(
    conn: &Connection,
    query: &ExplorerQuery,
    scope: &CapabilityScope,
) -> Result<Vec<GroupValue>> {
    let spec = turn_group_spec(query.group_by);
    let mut filter = query.filter.turn_filter(Some("t"));
    apply_session_filter(&mut filter, Some("t"), query.filters.session_id.as_deref());
    apply_source_scope(&mut filter, Some("t"), scope.allowed_sources.as_deref());
    let value_expr = turn_metric_expr(query.metric, query.filters.token_type);
    let sql = format!(
        r#"
        SELECT
            {key_expr} AS group_key,
            {label_expr} AS group_label,
            {value_expr} AS metric_value
        FROM usage_turn t
        LEFT JOIN usage_event e ON e.event_key = substr(t.turn_key, 6)
        {where_sql}
        GROUP BY group_key, group_label
        ORDER BY metric_value DESC, group_label ASC
        "#,
        key_expr = spec.key_expr,
        label_expr = spec.label_expr,
        value_expr = value_expr,
        where_sql = filter.where_sql()
    );
    query_group_values(conn, &sql, &filter)
}

fn load_turn_series(
    conn: &Connection,
    query: &ExplorerQuery,
    scope: &CapabilityScope,
) -> Result<Vec<SeriesValue>> {
    let spec = turn_group_spec(query.group_by);
    let bucket_expr = bucket_expr(
        query.granularity,
        "t.started_at",
        &query.filter.local_time_modifier(),
    );
    let mut filter = query.filter.turn_filter(Some("t"));
    apply_session_filter(&mut filter, Some("t"), query.filters.session_id.as_deref());
    apply_source_scope(&mut filter, Some("t"), scope.allowed_sources.as_deref());
    let value_expr = turn_metric_expr(query.metric, query.filters.token_type);
    let sql = format!(
        r#"
        SELECT
            {bucket_expr} AS bucket_key,
            {key_expr} AS group_key,
            {label_expr} AS group_label,
            {value_expr} AS metric_value
        FROM usage_turn t
        LEFT JOIN usage_event e ON e.event_key = substr(t.turn_key, 6)
        {where_sql}
        GROUP BY bucket_key, group_key, group_label
        ORDER BY bucket_key ASC, metric_value DESC, group_label ASC
        "#,
        bucket_expr = bucket_expr,
        key_expr = spec.key_expr,
        label_expr = spec.label_expr,
        value_expr = value_expr,
        where_sql = filter.where_sql()
    );
    query_series_values(conn, &sql, &filter)
}

fn load_attribution_rows(
    conn: &Connection,
    query: &ExplorerQuery,
    scope: &CapabilityScope,
) -> Result<Vec<GroupValue>> {
    if query.group_by == ExplorerDimension::TokenType {
        return load_attribution_token_type_rows(conn, query, scope);
    }

    let spec = attribution_group_spec(query.group_by);
    let value_expr = attribution_metric_expr(query.metric, query.filters.token_type);
    let (base_sql, params) = attribution_outer_sql(
        query,
        scope,
        &format!(
            r#"
            SELECT
                {key_expr} AS group_key,
                {label_expr} AS group_label,
                {value_expr} AS metric_value
            FROM attributed_rows a
            {extra_where}
            GROUP BY group_key, group_label
            ORDER BY metric_value DESC, group_label ASC
            "#,
            key_expr = spec.key_expr,
            label_expr = spec.label_expr,
            value_expr = value_expr,
            extra_where = attribution_extra_where(query)
        ),
    )?;
    query_group_values_with_params(conn, &base_sql, params)
}

fn load_attribution_series(
    conn: &Connection,
    query: &ExplorerQuery,
    scope: &CapabilityScope,
) -> Result<Vec<SeriesValue>> {
    if query.group_by == ExplorerDimension::TokenType {
        return load_attribution_token_type_series(conn, query, scope);
    }

    let spec = attribution_group_spec(query.group_by);
    let bucket_expr = bucket_expr(
        query.granularity,
        "a.occurred_at",
        &query.filter.local_time_modifier(),
    );
    let value_expr = attribution_metric_expr(query.metric, query.filters.token_type);
    let (base_sql, params) = attribution_outer_sql(
        query,
        scope,
        &format!(
            r#"
            SELECT
                {bucket_expr} AS bucket_key,
                {key_expr} AS group_key,
                {label_expr} AS group_label,
                {value_expr} AS metric_value
            FROM attributed_rows a
            {extra_where}
            GROUP BY bucket_key, group_key, group_label
            ORDER BY bucket_key ASC, metric_value DESC, group_label ASC
            "#,
            bucket_expr = bucket_expr,
            key_expr = spec.key_expr,
            label_expr = spec.label_expr,
            value_expr = value_expr,
            extra_where = attribution_extra_where(query)
        ),
    )?;
    query_series_values_with_params(conn, &base_sql, params)
}

fn load_event_token_type_rows(conn: &Connection, query: &ExplorerQuery) -> Result<Vec<GroupValue>> {
    let mut filter = query.filter.event_filter(Some("e"));
    apply_session_filter(&mut filter, Some("e"), query.filters.session_id.as_deref());
    let sql = token_type_union_sql(
        "usage_event e",
        &filter.where_sql(),
        &[
            (
                "input",
                "input",
                "CAST(COALESCE(e.input_tokens, 0) AS REAL)",
            ),
            (
                "cache_read",
                "cache_read",
                "CAST(COALESCE(e.cache_read_tokens, 0) AS REAL)",
            ),
            (
                "cache_creation",
                "cache_creation",
                "CAST(COALESCE(e.cache_creation_tokens, 0) AS REAL)",
            ),
            (
                "output",
                "output",
                "CAST(COALESCE(e.output_tokens, 0) AS REAL)",
            ),
            (
                "reasoning_output",
                "reasoning_output",
                "CAST(COALESCE(e.reasoning_output_tokens, 0) AS REAL)",
            ),
        ],
        None,
    );
    query_group_values(conn, &sql, &filter)
}

fn load_event_token_type_series(
    conn: &Connection,
    query: &ExplorerQuery,
) -> Result<Vec<SeriesValue>> {
    let mut filter = query.filter.event_filter(Some("e"));
    apply_session_filter(&mut filter, Some("e"), query.filters.session_id.as_deref());
    let bucket = bucket_expr(
        query.granularity,
        "e.event_at",
        &query.filter.local_time_modifier(),
    );
    let sql = token_type_union_sql(
        "usage_event e",
        &filter.where_sql(),
        &[
            (
                "input",
                "input",
                "CAST(COALESCE(e.input_tokens, 0) AS REAL)",
            ),
            (
                "cache_read",
                "cache_read",
                "CAST(COALESCE(e.cache_read_tokens, 0) AS REAL)",
            ),
            (
                "cache_creation",
                "cache_creation",
                "CAST(COALESCE(e.cache_creation_tokens, 0) AS REAL)",
            ),
            (
                "output",
                "output",
                "CAST(COALESCE(e.output_tokens, 0) AS REAL)",
            ),
            (
                "reasoning_output",
                "reasoning_output",
                "CAST(COALESCE(e.reasoning_output_tokens, 0) AS REAL)",
            ),
        ],
        Some(&bucket),
    );
    query_series_values(conn, &sql, &filter)
}

fn load_attribution_token_type_rows(
    conn: &Connection,
    query: &ExplorerQuery,
    scope: &CapabilityScope,
) -> Result<Vec<GroupValue>> {
    let (sql, params) = attribution_outer_sql(
        query,
        scope,
        &token_type_union_sql(
            "attributed_rows a",
            &attribution_extra_where(query),
            &[
                ("input", "input", "COALESCE(a.input_tokens, 0.0)"),
                (
                    "cache_read",
                    "cache_read",
                    "COALESCE(a.cache_read_tokens, 0.0)",
                ),
                (
                    "cache_creation",
                    "cache_creation",
                    "COALESCE(a.cache_creation_tokens, 0.0)",
                ),
                ("output", "output", "COALESCE(a.output_tokens, 0.0)"),
                (
                    "reasoning_output",
                    "reasoning_output",
                    "COALESCE(a.reasoning_output_tokens, 0.0)",
                ),
            ],
            None,
        ),
    )?;
    query_group_values_with_params(conn, &sql, params)
}

fn load_attribution_token_type_series(
    conn: &Connection,
    query: &ExplorerQuery,
    scope: &CapabilityScope,
) -> Result<Vec<SeriesValue>> {
    let bucket = bucket_expr(
        query.granularity,
        "a.occurred_at",
        &query.filter.local_time_modifier(),
    );
    let (sql, params) = attribution_outer_sql(
        query,
        scope,
        &token_type_union_sql(
            "attributed_rows a",
            &attribution_extra_where(query),
            &[
                ("input", "input", "COALESCE(a.input_tokens, 0.0)"),
                (
                    "cache_read",
                    "cache_read",
                    "COALESCE(a.cache_read_tokens, 0.0)",
                ),
                (
                    "cache_creation",
                    "cache_creation",
                    "COALESCE(a.cache_creation_tokens, 0.0)",
                ),
                ("output", "output", "COALESCE(a.output_tokens, 0.0)"),
                (
                    "reasoning_output",
                    "reasoning_output",
                    "COALESCE(a.reasoning_output_tokens, 0.0)",
                ),
            ],
            Some(&bucket),
        ),
    )?;
    query_series_values_with_params(conn, &sql, params)
}

fn attribution_outer_sql(
    query: &ExplorerQuery,
    scope: &CapabilityScope,
    outer_select: &str,
) -> Result<(String, Vec<Value>)> {
    let mut event_filter = query.filter.event_filter(Some("e"));
    apply_session_filter(
        &mut event_filter,
        Some("e"),
        query.filters.session_id.as_deref(),
    );
    apply_source_scope(
        &mut event_filter,
        Some("e"),
        scope.allowed_sources.as_deref(),
    );

    let mut tool_filter = query.filter.tool_filter(Some("tc"));
    apply_session_filter(
        &mut tool_filter,
        Some("tc"),
        query.filters.session_id.as_deref(),
    );
    apply_source_scope(
        &mut tool_filter,
        Some("tc"),
        scope.allowed_sources.as_deref(),
    );

    let sql = format!(
        r#"
        WITH filtered_events AS (
            SELECT
                e.event_key,
                e.source,
                e.model,
                e.project_hash,
                e.project_label,
                e.project_ref,
                e.session_id,
                e.session_label,
                e.event_at,
                COALESCE(e.cost_with_cache_usd, 0.0) AS attributed_cost_usd,
                COALESCE(e.input_tokens, 0) AS input_tokens,
                COALESCE(e.cache_read_tokens, 0) AS cache_read_tokens,
                COALESCE(e.cache_creation_tokens, 0) AS cache_creation_tokens,
                COALESCE(e.output_tokens, 0) AS output_tokens,
                COALESCE(e.reasoning_output_tokens, 0) AS reasoning_output_tokens,
                COALESCE(e.input_tokens, 0) +
                    COALESCE(e.cache_read_tokens, 0) +
                    COALESCE(e.cache_creation_tokens, 0) +
                    COALESCE(e.output_tokens, 0) +
                    COALESCE(e.reasoning_output_tokens, 0) AS total_tokens
            FROM usage_event e
            {event_where}
        ),
        filtered_tools AS (
            SELECT
                tc.tool_call_key,
                tc.event_key,
                tc.turn_key,
                tc.source,
                tc.session_id,
                tc.project_hash,
                tc.model,
                tc.occurred_at,
                tc.tool_name,
                tc.tool_kind,
                tc.mcp_server
            FROM usage_tool_call tc
            {tool_where}
        ),
        event_tool_counts AS (
            SELECT
                event_key,
                COUNT(*) AS tool_count
            FROM filtered_tools
            GROUP BY event_key
        ),
        attributed_rows AS (
            SELECT
                e.source,
                COALESCE(tc.model, e.model) AS model,
                e.project_hash,
                e.project_label,
                e.project_ref,
                COALESCE(tc.session_id, e.session_id) AS session_id,
                e.session_label,
                COALESCE(tc.turn_key, 'turn:' || e.event_key) AS turn_key,
                COALESCE(tc.occurred_at, e.event_at) AS occurred_at,
                tc.tool_name,
                tc.tool_kind,
                tc.mcp_server,
                1 AS is_tool,
                1 AS call_count,
                e.attributed_cost_usd * (1.0 / etc.tool_count) AS attributed_cost_usd,
                e.input_tokens * (1.0 / etc.tool_count) AS input_tokens,
                e.cache_read_tokens * (1.0 / etc.tool_count) AS cache_read_tokens,
                e.cache_creation_tokens * (1.0 / etc.tool_count) AS cache_creation_tokens,
                e.output_tokens * (1.0 / etc.tool_count) AS output_tokens,
                e.reasoning_output_tokens * (1.0 / etc.tool_count) AS reasoning_output_tokens,
                e.total_tokens * (1.0 / etc.tool_count) AS total_tokens
            FROM filtered_tools tc
            JOIN filtered_events e ON e.event_key = tc.event_key
            JOIN event_tool_counts etc ON etc.event_key = tc.event_key

            UNION ALL

            SELECT
                e.source,
                e.model,
                e.project_hash,
                e.project_label,
                e.project_ref,
                e.session_id,
                e.session_label,
                'turn:' || e.event_key AS turn_key,
                e.event_at AS occurred_at,
                '(non-tool)' AS tool_name,
                '(non-tool)' AS tool_kind,
                NULL AS mcp_server,
                0 AS is_tool,
                0 AS call_count,
                e.attributed_cost_usd,
                CAST(e.input_tokens AS REAL) AS input_tokens,
                CAST(e.cache_read_tokens AS REAL) AS cache_read_tokens,
                CAST(e.cache_creation_tokens AS REAL) AS cache_creation_tokens,
                CAST(e.output_tokens AS REAL) AS output_tokens,
                CAST(e.reasoning_output_tokens AS REAL) AS reasoning_output_tokens,
                CAST(e.total_tokens AS REAL) AS total_tokens
            FROM filtered_events e
            LEFT JOIN filtered_tools tc ON tc.event_key = e.event_key
            WHERE tc.tool_call_key IS NULL
        )
        {outer_select}
        "#,
        event_where = event_filter.where_sql(),
        tool_where = tool_filter.where_sql(),
        outer_select = outer_select
    );
    let params = event_filter
        .params()
        .iter()
        .chain(tool_filter.params().iter())
        .cloned()
        .collect::<Vec<_>>();
    Ok((sql, params))
}

fn attribution_extra_where(query: &ExplorerQuery) -> String {
    let mut clauses = Vec::new();
    if let Some(tool_name) = query
        .filters
        .tool_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        clauses.push(format!("a.tool_name = '{}'", escape_sql_literal(tool_name)));
    }
    if let Some(tool_kind) = query
        .filters
        .tool_kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        clauses.push(format!("a.tool_kind = '{}'", escape_sql_literal(tool_kind)));
    }
    if let Some(is_tool) = query.filters.is_tool {
        clauses.push(format!("a.is_tool = {}", if is_tool { 1 } else { 0 }));
    }
    if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    }
}

fn event_group_spec(group_by: ExplorerDimension) -> GroupSpec {
    match group_by {
        ExplorerDimension::Source => GroupSpec {
            key_expr: "e.source".to_string(),
            label_expr: "e.source".to_string(),
        },
        ExplorerDimension::Model => GroupSpec {
            key_expr: "e.model".to_string(),
            label_expr: "e.model".to_string(),
        },
        ExplorerDimension::Project => GroupSpec {
            key_expr: "COALESCE(NULLIF(e.project_hash, ''), 'unknown-project')".to_string(),
            label_expr: "COALESCE(NULLIF(e.project_ref, ''), NULLIF(e.project_label, ''), NULLIF(e.project_hash, ''), 'unknown-project')".to_string(),
        },
        ExplorerDimension::Session => GroupSpec {
            key_expr: "COALESCE(NULLIF(e.session_id, ''), NULLIF(e.source_path_hash, ''), e.event_key)".to_string(),
            label_expr: "COALESCE(NULLIF(e.session_label, ''), NULLIF(e.session_id, ''), NULLIF(e.source_path_hash, ''), e.event_key)".to_string(),
        },
        _ => unreachable!("event strategy only supports source/model/project/session/token_type"),
    }
}

fn bucket_group_spec(group_by: ExplorerDimension) -> GroupSpec {
    match group_by {
        ExplorerDimension::Source => GroupSpec {
            key_expr: "b.source".to_string(),
            label_expr: "b.source".to_string(),
        },
        ExplorerDimension::Model => GroupSpec {
            key_expr: "b.model".to_string(),
            label_expr: "b.model".to_string(),
        },
        ExplorerDimension::Project => GroupSpec {
            key_expr: "COALESCE(NULLIF(b.project_hash, ''), 'unknown-project')".to_string(),
            label_expr: "COALESCE(NULLIF(b.project_ref, ''), NULLIF(b.project_label, ''), NULLIF(b.project_hash, ''), 'unknown-project')".to_string(),
        },
        _ => unreachable!("bucket strategy only supports source/model/project"),
    }
}

fn turn_group_spec(group_by: ExplorerDimension) -> GroupSpec {
    match group_by {
        ExplorerDimension::Source => GroupSpec {
            key_expr: "t.source".to_string(),
            label_expr: "t.source".to_string(),
        },
        ExplorerDimension::Model => GroupSpec {
            key_expr: "t.primary_model".to_string(),
            label_expr: "t.primary_model".to_string(),
        },
        ExplorerDimension::Project => GroupSpec {
            key_expr: "COALESCE(NULLIF(t.project_hash, ''), 'unknown-project')".to_string(),
            label_expr: "COALESCE(NULLIF(e.project_ref, ''), NULLIF(e.project_label, ''), NULLIF(t.project_hash, ''), 'unknown-project')".to_string(),
        },
        ExplorerDimension::Session => GroupSpec {
            key_expr: "COALESCE(NULLIF(t.session_id, ''), NULLIF(t.source_path_hash, ''), t.turn_key)".to_string(),
            label_expr: "COALESCE(NULLIF(e.session_label, ''), NULLIF(t.session_id, ''), NULLIF(t.source_path_hash, ''), t.turn_key)".to_string(),
        },
        _ => unreachable!("turn strategy only supports source/model/project/session"),
    }
}

fn attribution_group_spec(group_by: ExplorerDimension) -> GroupSpec {
    match group_by {
        ExplorerDimension::Source => GroupSpec {
            key_expr: "a.source".to_string(),
            label_expr: "a.source".to_string(),
        },
        ExplorerDimension::Model => GroupSpec {
            key_expr: "a.model".to_string(),
            label_expr: "a.model".to_string(),
        },
        ExplorerDimension::Project => GroupSpec {
            key_expr: "COALESCE(NULLIF(a.project_hash, ''), 'unknown-project')".to_string(),
            label_expr: "COALESCE(NULLIF(a.project_ref, ''), NULLIF(a.project_label, ''), NULLIF(a.project_hash, ''), 'unknown-project')".to_string(),
        },
        ExplorerDimension::Session => GroupSpec {
            key_expr: "COALESCE(NULLIF(a.session_id, ''), a.turn_key)".to_string(),
            label_expr: "COALESCE(NULLIF(a.session_label, ''), NULLIF(a.session_id, ''), a.turn_key)".to_string(),
        },
        ExplorerDimension::Tool => GroupSpec {
            key_expr: "CASE WHEN a.mcp_server IS NOT NULL AND a.mcp_server <> '' THEN a.mcp_server || ':' || a.tool_name ELSE a.tool_name END".to_string(),
            label_expr: "CASE WHEN a.mcp_server IS NOT NULL AND a.mcp_server <> '' THEN a.mcp_server || ':' || a.tool_name ELSE a.tool_name END".to_string(),
        },
        ExplorerDimension::ToolKind => GroupSpec {
            key_expr: "a.tool_kind".to_string(),
            label_expr: "a.tool_kind".to_string(),
        },
        ExplorerDimension::IsTool => GroupSpec {
            key_expr: "CASE WHEN a.is_tool = 1 THEN 'tool' ELSE 'non_tool' END".to_string(),
            label_expr: "CASE WHEN a.is_tool = 1 THEN 'tool' ELSE 'non-tool' END".to_string(),
        },
        _ => unreachable!("token_type is handled separately"),
    }
}

fn event_metric_expr(metric: ExplorerMetric, token_type: Option<ExplorerTokenType>) -> String {
    match metric {
        ExplorerMetric::AttributedCostUsd => {
            "COALESCE(SUM(COALESCE(e.cost_with_cache_usd, 0.0)), 0.0)".to_string()
        }
        ExplorerMetric::Calls => "CAST(COUNT(*) AS REAL)".to_string(),
        ExplorerMetric::Sessions => {
            "CAST(COUNT(DISTINCT COALESCE(NULLIF(e.session_id, ''), NULLIF(e.source_path_hash, ''), e.event_key)) AS REAL)".to_string()
        }
        ExplorerMetric::TotalTokens => match token_type {
            Some(token_type) => format!(
                "CAST(COALESCE(SUM({}), 0) AS REAL)",
                token_type.event_expr("e")
            ),
            None => "CAST(COALESCE(SUM(COALESCE(e.input_tokens, 0) + COALESCE(e.cache_creation_tokens, 0) + COALESCE(e.cache_read_tokens, 0) + COALESCE(e.output_tokens, 0) + COALESCE(e.reasoning_output_tokens, 0)), 0) AS REAL)".to_string(),
        },
        ExplorerMetric::Turns => unreachable!("turn metric routes through turn strategy"),
    }
}

fn bucket_metric_expr(metric: ExplorerMetric) -> &'static str {
    match metric {
        ExplorerMetric::AttributedCostUsd => {
            "COALESCE(SUM(COALESCE(b.cost_with_cache_usd, 0.0)), 0.0)"
        }
        ExplorerMetric::Calls => "CAST(COALESCE(SUM(b.event_count), 0) AS REAL)",
        ExplorerMetric::TotalTokens => "CAST(COALESCE(SUM(b.total_tokens), 0) AS REAL)",
        ExplorerMetric::Turns | ExplorerMetric::Sessions => {
            unreachable!("unsupported metrics must not use the bucket strategy")
        }
    }
}

fn turn_metric_expr(metric: ExplorerMetric, token_type: Option<ExplorerTokenType>) -> String {
    match metric {
        ExplorerMetric::AttributedCostUsd => {
            "COALESCE(SUM(COALESCE(e.cost_with_cache_usd, 0.0)), 0.0)".to_string()
        }
        ExplorerMetric::Calls => "CAST(COALESCE(SUM(t.call_count), 0) AS REAL)".to_string(),
        ExplorerMetric::Turns => "CAST(COUNT(*) AS REAL)".to_string(),
        ExplorerMetric::Sessions => {
            "CAST(COUNT(DISTINCT COALESCE(NULLIF(t.session_id, ''), NULLIF(t.source_path_hash, ''), t.turn_key)) AS REAL)".to_string()
        }
        ExplorerMetric::TotalTokens => match token_type {
            Some(token_type) => format!(
                "CAST(COALESCE(SUM({}), 0) AS REAL)",
                token_type.turn_expr()
            ),
            None => "CAST(COALESCE(SUM(t.total_tokens), 0) AS REAL)".to_string(),
        },
    }
}

fn attribution_metric_expr(
    metric: ExplorerMetric,
    token_type: Option<ExplorerTokenType>,
) -> String {
    match metric {
        ExplorerMetric::AttributedCostUsd => {
            "COALESCE(SUM(a.attributed_cost_usd), 0.0)".to_string()
        }
        ExplorerMetric::Calls => "CAST(COALESCE(SUM(a.call_count), 0) AS REAL)".to_string(),
        ExplorerMetric::Turns => "CAST(COUNT(DISTINCT a.turn_key) AS REAL)".to_string(),
        ExplorerMetric::Sessions => {
            "CAST(COUNT(DISTINCT COALESCE(NULLIF(a.session_id, ''), a.turn_key)) AS REAL)"
                .to_string()
        }
        ExplorerMetric::TotalTokens => match token_type {
            Some(token_type) => format!("COALESCE(SUM({}), 0.0)", token_type.attributed_expr()),
            None => "COALESCE(SUM(a.total_tokens), 0.0)".to_string(),
        },
    }
}

fn bucket_expr(granularity: ExplorerGranularity, column: &str, modifier: &str) -> String {
    match granularity {
        ExplorerGranularity::Total => "'total'".to_string(),
        ExplorerGranularity::Day => format!("date({column}, '{modifier}')"),
        ExplorerGranularity::Week => format!("strftime('%Y-%W', {column}, '{modifier}')"),
        ExplorerGranularity::Month => format!("strftime('%Y-%m', {column}, '{modifier}')"),
    }
}

fn token_type_union_sql(
    from_clause: &str,
    where_sql: &str,
    rows: &[(&str, &str, &str)],
    bucket_expr: Option<&str>,
) -> String {
    let selects = rows
        .iter()
        .map(|(key, label, value_expr)| {
            if let Some(bucket_expr) = bucket_expr {
                format!(
                    "SELECT {bucket_expr} AS bucket_key, '{key}' AS group_key, '{label}' AS group_label, {value_expr} AS metric_value FROM {from_clause} {where_sql}"
                )
            } else {
                format!(
                    "SELECT '{key}' AS group_key, '{label}' AS group_label, {value_expr} AS metric_value FROM {from_clause} {where_sql}"
                )
            }
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ");

    if bucket_expr.is_some() {
        format!(
            "SELECT bucket_key, group_key, group_label, COALESCE(SUM(metric_value), 0.0) AS metric_value FROM ({selects}) token_rows GROUP BY bucket_key, group_key, group_label ORDER BY bucket_key ASC, metric_value DESC, group_label ASC"
        )
    } else {
        format!(
            "SELECT group_key, group_label, COALESCE(SUM(metric_value), 0.0) AS metric_value FROM ({selects}) token_rows GROUP BY group_key, group_label ORDER BY metric_value DESC, group_label ASC"
        )
    }
}

fn query_group_values(conn: &Connection, sql: &str, filter: &SqlFilter) -> Result<Vec<GroupValue>> {
    query_group_values_with_params(conn, sql, filter.params().to_vec())
}

fn query_group_values_with_params(
    conn: &Connection,
    sql: &str,
    params: Vec<Value>,
) -> Result<Vec<GroupValue>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter()), |row| {
        Ok(GroupValue {
            key: row.get(0)?,
            label: row.get(1)?,
            value: row.get::<_, f64>(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn query_series_values(
    conn: &Connection,
    sql: &str,
    filter: &SqlFilter,
) -> Result<Vec<SeriesValue>> {
    query_series_values_with_params(conn, sql, filter.params().to_vec())
}

fn query_series_values_with_params(
    conn: &Connection,
    sql: &str,
    params: Vec<Value>,
) -> Result<Vec<SeriesValue>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter()), |row| {
        Ok(SeriesValue {
            bucket: row.get(0)?,
            key: row.get(1)?,
            label: row.get(2)?,
            value: row.get::<_, f64>(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn select_rows(
    all_rows: &[GroupValue],
    limit: usize,
    include_other: bool,
    total: f64,
) -> Vec<ExplorerRow> {
    if all_rows.is_empty() {
        return Vec::new();
    }

    let head = all_rows.iter().take(limit).collect::<Vec<_>>();
    let mut rows = head
        .iter()
        .map(|row| ExplorerRow {
            key: row.key.clone(),
            label: row.label.clone(),
            value: row.value,
            share: share(row.value, total),
            is_other: false,
        })
        .collect::<Vec<_>>();

    if include_other && all_rows.len() > limit {
        let other_value = all_rows
            .iter()
            .skip(limit)
            .map(|row| row.value)
            .sum::<f64>();
        if other_value > 0.0 {
            rows.push(ExplorerRow {
                key: OTHER_KEY.to_string(),
                label: OTHER_LABEL.to_string(),
                value: other_value,
                share: share(other_value, total),
                is_other: true,
            });
        }
    }

    rows
}

fn collapse_series(
    all_series: &[SeriesValue],
    selected: &[ExplorerRow],
) -> Vec<ExplorerSeriesPoint> {
    if all_series.is_empty() {
        return Vec::new();
    }

    let selected_keys = selected
        .iter()
        .filter(|row| !row.is_other)
        .map(|row| row.key.clone())
        .collect::<BTreeSet<_>>();
    let include_other = selected.iter().any(|row| row.is_other);
    let mut by_bucket_and_key: BTreeMap<(String, String), ExplorerSeriesPoint> = BTreeMap::new();

    for point in all_series {
        let (key, label, is_other) = if selected_keys.contains(&point.key) {
            (point.key.clone(), point.label.clone(), false)
        } else if include_other {
            (OTHER_KEY.to_string(), OTHER_LABEL.to_string(), true)
        } else {
            continue;
        };

        let entry = by_bucket_and_key
            .entry((point.bucket.clone(), key.clone()))
            .or_insert_with(|| ExplorerSeriesPoint {
                bucket: point.bucket.clone(),
                key,
                label,
                value: 0.0,
                is_other,
            });
        entry.value += point.value;
    }

    by_bucket_and_key.into_values().collect()
}

fn empty_payload(
    query: &ExplorerQuery,
    support: ExplorerSupport,
    warning: Option<String>,
) -> ExplorerPayload {
    ExplorerPayload {
        support,
        warning,
        granularity: query.granularity,
        metric: query.metric,
        group_by: query.group_by,
        limit: query.limit,
        include_other: query.include_other,
        totals: ExplorerTotals::default(),
        rows: Vec::new(),
        series: Vec::new(),
    }
}

fn apply_session_filter(filter: &mut SqlFilter, alias: Option<&str>, session_id: Option<&str>) {
    if let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.push(
            format!("{} = ?", qualified(alias, "session_id")),
            session_id.to_string(),
        );
    }
}

fn apply_source_scope(filter: &mut SqlFilter, alias: Option<&str>, sources: Option<&[String]>) {
    let Some(sources) = sources else {
        return;
    };
    if sources.is_empty() {
        filter.push_raw("1 = 0");
        return;
    }
    let placeholders = vec!["?"; sources.len()].join(", ");
    filter.push_raw(format!(
        "{} IN ({placeholders})",
        qualified(alias, "source")
    ));
    for source in sources {
        filter.push_value(Value::Text(source.clone()));
    }
}

fn qualified(alias: Option<&str>, column: &str) -> String {
    alias
        .map(str::trim)
        .filter(|alias| !alias.is_empty())
        .map(|alias| format!("{alias}.{column}"))
        .unwrap_or_else(|| column.to_string())
}

fn escape_sql_literal(raw: &str) -> String {
    raw.replace('\'', "''")
}

fn share(value: f64, total: f64) -> f64 {
    if total <= f64::EPSILON {
        0.0
    } else {
        value / total
    }
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, NaiveDate};
    use rusqlite::Connection;

    use super::{
        Dashboard, ExplorerDimension, ExplorerFilters, ExplorerGranularity, ExplorerMetric,
        ExplorerQuery, ExplorerStrategy, ExplorerTokenType, choose_strategy, load_bucket_rows,
        load_bucket_series, load_event_rows, load_event_series,
    };
    use crate::{
        error::Result,
        models::SourceKind,
        query::{QueryFilter, ReportTimezone},
        testing::{Fixture, SeedEvent},
    };

    struct TurnFixture<'a> {
        turn_key: &'a str,
        source: &'a str,
        session_id: &'a str,
        project_hash: &'a str,
        model: &'a str,
        started_at: &'a str,
        total_tokens: i64,
    }

    fn insert_turn(conn: &Connection, turn: TurnFixture<'_>) -> Result<()> {
        conn.execute(
            r#"
            INSERT INTO usage_turn(
                turn_key, source, session_id, source_path_hash, project_hash,
                primary_model, started_at, category, has_edits, retries,
                one_shot, call_count, input_tokens, cache_read_tokens,
                cache_creation_tokens, output_tokens, reasoning_output_tokens,
                total_tokens, created_at
            ) VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6, 'coding', 1, 0, 1, 1,
                ?7, 0, 0, 0, 0, ?7, ?6)
            "#,
            rusqlite::params![
                turn.turn_key,
                turn.source,
                turn.session_id,
                turn.project_hash,
                turn.model,
                turn.started_at,
                turn.total_tokens
            ],
        )?;
        Ok(())
    }

    struct ToolFixture<'a> {
        tool_call_key: &'a str,
        turn_key: &'a str,
        event_key: &'a str,
        source: &'a str,
        session_id: &'a str,
        project_hash: &'a str,
        model: &'a str,
        occurred_at: &'a str,
        tool_name: &'a str,
        tool_kind: &'a str,
    }

    fn insert_tool(conn: &Connection, tool: ToolFixture<'_>) -> Result<()> {
        conn.execute(
            r#"
            INSERT INTO usage_tool_call(
                tool_call_key, turn_key, event_key, source, session_id,
                source_path_hash, project_hash, model, occurred_at, tool_name,
                tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8, ?9, ?10,
                NULL, NULL, ?1, ?9, ?8)
            "#,
            rusqlite::params![
                tool.tool_call_key,
                tool.turn_key,
                tool.event_key,
                tool.source,
                tool.session_id,
                tool.project_hash,
                tool.model,
                tool.occurred_at,
                tool.tool_name,
                tool.tool_kind
            ],
        )?;
        Ok(())
    }

    #[test]
    fn explorer_bucket_strategy_matches_event_results_for_supported_shapes() -> Result<()> {
        let fixture = Fixture::new()?;
        for event in [
            SeedEvent {
                event_key: "codex:bucket:before-boundary",
                source: "codex",
                model: "gpt-5",
                event_at: "2026-05-01T15:59:00Z",
                hour_start: Some("2026-05-01T15:59:00Z"),
                input_tokens: 100,
                total_tokens: 100,
                cost_with_cache_usd: 1.0,
                cost_without_cache_usd: 1.0,
                pricing_status: "static",
                pricing_source: Some("static-v1"),
                project_hash: "project-a",
                project_label: "Project A",
                ..Default::default()
            },
            SeedEvent {
                event_key: "codex:bucket:inside-a",
                source: "codex",
                model: "gpt-5",
                event_at: "2026-05-01T16:00:00Z",
                hour_start: Some("2026-05-01T16:00:00Z"),
                input_tokens: 200,
                total_tokens: 200,
                cost_with_cache_usd: 2.0,
                cost_without_cache_usd: 2.0,
                pricing_status: "static",
                pricing_source: Some("static-v1"),
                project_hash: "project-a",
                project_label: "Project A",
                ..Default::default()
            },
            SeedEvent {
                event_key: "claude:bucket:inside-b",
                source: "claude",
                model: "sonnet",
                event_at: "2026-05-02T02:00:00Z",
                hour_start: Some("2026-05-02T02:00:00Z"),
                input_tokens: 300,
                total_tokens: 300,
                cost_with_cache_usd: 3.0,
                cost_without_cache_usd: 3.0,
                pricing_status: "static",
                pricing_source: Some("static-v1"),
                project_hash: "project-b",
                project_label: "Project B",
                ..Default::default()
            },
        ] {
            fixture.seed_event(event)?;
        }

        let dashboard = Dashboard::open(fixture.store())?;
        for metric in [
            ExplorerMetric::AttributedCostUsd,
            ExplorerMetric::Calls,
            ExplorerMetric::TotalTokens,
        ] {
            for group_by in [
                ExplorerDimension::Source,
                ExplorerDimension::Model,
                ExplorerDimension::Project,
            ] {
                let query = ExplorerQuery {
                    filter: QueryFilter {
                        since: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
                        until: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
                        timezone: ReportTimezone::Fixed(FixedOffset::east_opt(8 * 3600).unwrap()),
                        ..Default::default()
                    },
                    granularity: ExplorerGranularity::Day,
                    metric,
                    group_by,
                    filters: ExplorerFilters::default(),
                    limit: 8,
                    include_other: true,
                };

                assert_eq!(choose_strategy(&query), ExplorerStrategy::Bucket);
                let bucket_rows = load_bucket_rows(&dashboard.conn, &query)?;
                let event_rows = load_event_rows(&dashboard.conn, &query)?;
                assert_eq!(bucket_rows.len(), event_rows.len());
                for (bucket, event) in bucket_rows.iter().zip(&event_rows) {
                    assert_eq!((&bucket.key, &bucket.label), (&event.key, &event.label));
                    assert!((bucket.value - event.value).abs() < f64::EPSILON);
                }

                let bucket_series = load_bucket_series(&dashboard.conn, &query)?;
                let event_series = load_event_series(&dashboard.conn, &query)?;
                assert_eq!(bucket_series.len(), event_series.len());
                for (bucket, event) in bucket_series.iter().zip(&event_series) {
                    assert_eq!(
                        (&bucket.bucket, &bucket.key, &bucket.label),
                        (&event.bucket, &event.key, &event.label)
                    );
                    assert!((bucket.value - event.value).abs() < f64::EPSILON);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn explorer_bucket_strategy_rejects_fact_only_semantics() {
        for query in [
            ExplorerQuery {
                group_by: ExplorerDimension::Session,
                ..Default::default()
            },
            ExplorerQuery {
                metric: ExplorerMetric::Sessions,
                ..Default::default()
            },
            ExplorerQuery {
                filters: ExplorerFilters {
                    token_type: Some(ExplorerTokenType::Input),
                    ..Default::default()
                },
                metric: ExplorerMetric::TotalTokens,
                ..Default::default()
            },
        ] {
            assert_eq!(choose_strategy(&query), ExplorerStrategy::Event);
        }
    }

    #[test]
    fn explorer_groups_filtered_tool_costs_by_session() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_event(SeedEvent {
            event_key: "codex:explorer:session-a",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 120,
            output_tokens: 60,
            total_tokens: 180,
            cost_with_cache_usd: 1.0,
            cost_without_cache_usd: 1.0,
            pricing_status: "static",
            pricing_source: Some("static-v1"),
            session_id: Some("session-a"),
            source_path_hash: Some("session-a"),
            created_at: Some("2026-05-01T00:00:00Z"),
            ..Default::default()
        })?;
        fixture.seed_event(SeedEvent {
            event_key: "codex:explorer:session-b",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-02T00:00:00Z",
            hour_start: Some("2026-05-02T00:00:00Z"),
            input_tokens: 90,
            output_tokens: 30,
            total_tokens: 120,
            cost_with_cache_usd: 0.6,
            cost_without_cache_usd: 0.6,
            pricing_status: "static",
            pricing_source: Some("static-v1"),
            session_id: Some("session-b"),
            source_path_hash: Some("session-b"),
            created_at: Some("2026-05-02T00:00:00Z"),
            ..Default::default()
        })?;
        let conn = fixture.store().open_connection()?;
        insert_tool(
            &conn,
            ToolFixture {
                tool_call_key: "tool:read:a",
                turn_key: "turn:codex:explorer:session-a",
                event_key: "codex:explorer:session-a",
                source: "codex",
                session_id: "session-a",
                project_hash: "project-a",
                model: "gpt-5",
                occurred_at: "2026-05-01T00:00:00Z",
                tool_name: "Read",
                tool_kind: "read",
            },
        )?;
        insert_tool(
            &conn,
            ToolFixture {
                tool_call_key: "tool:edit:a",
                turn_key: "turn:codex:explorer:session-a",
                event_key: "codex:explorer:session-a",
                source: "codex",
                session_id: "session-a",
                project_hash: "project-a",
                model: "gpt-5",
                occurred_at: "2026-05-01T00:00:00Z",
                tool_name: "Edit",
                tool_kind: "edit",
            },
        )?;
        insert_tool(
            &conn,
            ToolFixture {
                tool_call_key: "tool:read:b",
                turn_key: "turn:codex:explorer:session-b",
                event_key: "codex:explorer:session-b",
                source: "codex",
                session_id: "session-b",
                project_hash: "project-a",
                model: "gpt-5",
                occurred_at: "2026-05-02T00:00:00Z",
                tool_name: "Read",
                tool_kind: "read",
            },
        )?;
        drop(conn);

        let payload = Dashboard::open(fixture.store())?.explorer(&ExplorerQuery {
            filter: QueryFilter {
                source: Some(SourceKind::Codex),
                timezone: ReportTimezone::Utc,
                ..Default::default()
            },
            granularity: ExplorerGranularity::Day,
            metric: ExplorerMetric::AttributedCostUsd,
            group_by: ExplorerDimension::Session,
            filters: ExplorerFilters {
                tool_name: Some("Read".to_string()),
                ..Default::default()
            },
            limit: 10,
            include_other: false,
        })?;

        assert!(payload.support.supported);
        assert_eq!(payload.support.level, "normalized");
        assert_eq!(payload.rows.len(), 2);
        assert_eq!(payload.rows[0].key, "session-b");
        assert!((payload.rows[0].value - 0.6).abs() < f64::EPSILON);
        assert_eq!(payload.rows[1].key, "session-a");
        assert!((payload.rows[1].value - 0.5).abs() < f64::EPSILON);
        assert!((payload.totals.value - 1.1).abs() < f64::EPSILON);
        assert_eq!(payload.series.len(), 2);
        Ok(())
    }

    #[test]
    fn explorer_adds_other_bucket_when_rank_limit_is_hit() -> Result<()> {
        let fixture = Fixture::new()?;
        for (event_key, model, tokens) in [
            ("codex:explorer:model-a", "gpt-5", 300),
            ("codex:explorer:model-b", "sonnet", 200),
            ("codex:explorer:model-c", "o3", 100),
        ] {
            fixture.seed_event(SeedEvent {
                event_key,
                source: "codex",
                model,
                event_at: "2026-05-01T00:00:00Z",
                hour_start: Some("2026-05-01T00:00:00Z"),
                input_tokens: tokens,
                output_tokens: 0,
                total_tokens: tokens,
                cost_with_cache_usd: tokens as f64 / 1000.0,
                cost_without_cache_usd: tokens as f64 / 1000.0,
                pricing_status: "static",
                pricing_source: Some("static-v1"),
                session_id: Some(event_key),
                source_path_hash: Some(event_key),
                created_at: Some("2026-05-01T00:00:00Z"),
                ..Default::default()
            })?;
        }

        let payload = Dashboard::open(fixture.store())?.explorer(&ExplorerQuery {
            filter: QueryFilter {
                source: Some(SourceKind::Codex),
                timezone: ReportTimezone::Utc,
                ..Default::default()
            },
            granularity: ExplorerGranularity::Total,
            metric: ExplorerMetric::TotalTokens,
            group_by: ExplorerDimension::Model,
            filters: ExplorerFilters::default(),
            limit: 2,
            include_other: true,
        })?;

        assert_eq!(payload.rows.len(), 3);
        assert_eq!(payload.rows[0].key, "gpt-5");
        assert_eq!(payload.rows[0].value, 300.0);
        assert_eq!(payload.rows[1].key, "sonnet");
        assert_eq!(payload.rows[1].value, 200.0);
        assert!(payload.rows[2].is_other);
        assert_eq!(payload.rows[2].value, 100.0);
        assert_eq!(payload.totals.value, 600.0);
        Ok(())
    }

    #[test]
    fn explorer_marks_turn_metric_degraded_when_sources_lack_turn_facts() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_event(SeedEvent {
            event_key: "codex:explorer:turn-capable",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            cost_with_cache_usd: 0.2,
            cost_without_cache_usd: 0.2,
            pricing_status: "static",
            pricing_source: Some("static-v1"),
            session_id: Some("session-codex"),
            source_path_hash: Some("session-codex"),
            created_at: Some("2026-05-01T00:00:00Z"),
            ..Default::default()
        })?;
        fixture.seed_event(SeedEvent {
            event_key: "claude:explorer:no-turns",
            source: "claude",
            model: "sonnet",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 80,
            output_tokens: 20,
            total_tokens: 100,
            cost_with_cache_usd: 0.1,
            cost_without_cache_usd: 0.1,
            pricing_status: "static",
            pricing_source: Some("static-v1"),
            session_id: Some("session-claude"),
            source_path_hash: Some("session-claude"),
            created_at: Some("2026-05-01T00:00:00Z"),
            ..Default::default()
        })?;
        let conn = fixture.store().open_connection()?;
        insert_turn(
            &conn,
            TurnFixture {
                turn_key: "turn:codex:explorer:turn-capable",
                source: "codex",
                session_id: "session-codex",
                project_hash: "project-a",
                model: "gpt-5",
                started_at: "2026-05-01T00:00:00Z",
                total_tokens: 150,
            },
        )?;
        drop(conn);

        let payload = Dashboard::open(fixture.store())?.explorer(&ExplorerQuery {
            filter: QueryFilter {
                since: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
                until: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
                timezone: ReportTimezone::Utc,
                ..Default::default()
            },
            granularity: ExplorerGranularity::Total,
            metric: ExplorerMetric::Turns,
            group_by: ExplorerDimension::Source,
            filters: ExplorerFilters::default(),
            limit: 10,
            include_other: false,
        })?;

        assert!(payload.support.supported);
        assert_eq!(payload.support.level, "degraded");
        assert!(
            payload
                .support
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("claude"))
        );
        assert_eq!(payload.rows.len(), 1);
        assert_eq!(payload.rows[0].key, "codex");
        assert_eq!(payload.rows[0].value, 1.0);
        Ok(())
    }

    #[test]
    fn explorer_splits_tool_tokens_by_token_type() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_event(SeedEvent {
            event_key: "codex:explorer:token-type",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 120,
            output_tokens: 60,
            total_tokens: 180,
            cost_with_cache_usd: 1.0,
            cost_without_cache_usd: 1.0,
            pricing_status: "static",
            pricing_source: Some("static-v1"),
            session_id: Some("session-a"),
            source_path_hash: Some("session-a"),
            created_at: Some("2026-05-01T00:00:00Z"),
            ..Default::default()
        })?;
        let conn = fixture.store().open_connection()?;
        insert_tool(
            &conn,
            ToolFixture {
                tool_call_key: "tool:read:token-type",
                turn_key: "turn:codex:explorer:token-type",
                event_key: "codex:explorer:token-type",
                source: "codex",
                session_id: "session-a",
                project_hash: "project-a",
                model: "gpt-5",
                occurred_at: "2026-05-01T00:00:00Z",
                tool_name: "Read",
                tool_kind: "read",
            },
        )?;
        insert_tool(
            &conn,
            ToolFixture {
                tool_call_key: "tool:edit:token-type",
                turn_key: "turn:codex:explorer:token-type",
                event_key: "codex:explorer:token-type",
                source: "codex",
                session_id: "session-a",
                project_hash: "project-a",
                model: "gpt-5",
                occurred_at: "2026-05-01T00:00:00Z",
                tool_name: "Edit",
                tool_kind: "edit",
            },
        )?;
        drop(conn);

        let payload = Dashboard::open(fixture.store())?.explorer(&ExplorerQuery {
            filter: QueryFilter {
                source: Some(SourceKind::Codex),
                timezone: ReportTimezone::Utc,
                ..Default::default()
            },
            granularity: ExplorerGranularity::Total,
            metric: ExplorerMetric::TotalTokens,
            group_by: ExplorerDimension::TokenType,
            filters: ExplorerFilters {
                tool_name: Some("Read".to_string()),
                ..Default::default()
            },
            limit: 10,
            include_other: false,
        })?;

        assert_eq!(payload.rows.len(), 5);
        let input = payload
            .rows
            .iter()
            .find(|row| row.key == "input")
            .expect("input row");
        let output = payload
            .rows
            .iter()
            .find(|row| row.key == "output")
            .expect("output row");
        assert_eq!(input.value, 60.0);
        assert_eq!(output.value, 30.0);
        assert_eq!(payload.totals.value, 90.0);
        Ok(())
    }
}
