use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use chrono::{DateTime, Duration, FixedOffset, Local, NaiveDate, Offset, Timelike, Utc};
use rusqlite::Connection;
use serde::Serialize;

pub use super::ReportTimezone;
use crate::{models::SourceKind, store::Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone)]
pub struct ReportFilter {
    pub since: Option<NaiveDate>,
    pub until: Option<NaiveDate>,
    pub order: SortOrder,
    pub timezone: ReportTimezone,
    pub locale: String,
    pub source: Option<SourceKind>,
    pub project: Option<String>,
    pub breakdown: bool,
}

#[derive(Debug, Clone)]
pub enum TokenLimit {
    Max,
    Value(u64),
}

#[derive(Debug, Clone)]
pub struct BlockReportOptions {
    pub active_only: bool,
    pub recent_only: bool,
    pub token_limit: Option<TokenLimit>,
    pub session_length_hours: f64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct TokenTotals {
    pub input_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ModelCostBreakdown {
    pub source: String,
    pub model: String,
    pub input_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ReportNotes {
    pub unpriced: bool,
    pub reason_not_reported: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectSummary {
    pub project_hash: String,
    pub project_label: String,
    pub project_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DailyReportRow {
    pub date: String,
    pub source: Option<String>,
    pub project: Option<ProjectSummary>,
    #[serde(flatten)]
    pub totals: TokenTotals,
    pub models_used: Vec<String>,
    pub model_breakdowns: Vec<ModelCostBreakdown>,
    #[serde(skip_serializing)]
    pub conversation_count: usize,
    #[serde(skip_serializing)]
    pub notes: ReportNotes,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MonthlyReportRow {
    pub month: String,
    pub source: Option<String>,
    #[serde(flatten)]
    pub totals: TokenTotals,
    pub models_used: Vec<String>,
    pub model_breakdowns: Vec<ModelCostBreakdown>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionReportRow {
    pub session_id: String,
    pub session_label: Option<String>,
    pub project: Option<ProjectSummary>,
    pub source: Option<String>,
    pub first_activity_at: String,
    pub last_activity_at: String,
    #[serde(flatten)]
    pub totals: TokenTotals,
    pub models_used: Vec<String>,
    pub model_breakdowns: Vec<ModelCostBreakdown>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BlockReportRow {
    pub block_id: String,
    pub start_at: String,
    pub end_at: String,
    pub is_active: bool,
    pub duration_minutes: i64,
    pub burn_rate_tokens_per_hour: f64,
    pub projected_total_tokens: i64,
    pub token_limit: Option<u64>,
    pub token_limit_percent: Option<f64>,
    #[serde(flatten)]
    pub totals: TokenTotals,
    pub models_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DailyReport {
    pub daily: Vec<DailyReportRow>,
    pub totals: TokenTotals,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DailyProjectReport {
    pub projects: BTreeMap<String, Vec<DailyReportRow>>,
    pub totals: TokenTotals,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MonthlyReport {
    pub monthly: Vec<MonthlyReportRow>,
    pub totals: TokenTotals,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionListReport {
    pub sessions: Vec<SessionReportRow>,
    pub totals: TokenTotals,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SingleSessionReport {
    pub session: Option<SessionReportRow>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BlocksReport {
    pub blocks: Vec<BlockReportRow>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StatuslineSummary {
    pub today: TokenTotals,
    pub active_block: Option<BlockReportRow>,
    pub generated_at: String,
}

#[derive(Debug, Clone)]
struct EventRow {
    event_key: String,
    source: String,
    model: String,
    event_utc: DateTime<Utc>,
    local_at: DateTime<FixedOffset>,
    local_date: NaiveDate,
    input_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    cost_with_cache_usd: f64,
    pricing_status: String,
    project: Option<ProjectSummary>,
    session_id: Option<String>,
    session_label: Option<String>,
    source_path_hash: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct TokenComponents {
    input_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
}

impl TokenComponents {
    fn total_tokens(self) -> i64 {
        self.input_tokens
            + self.cache_creation_tokens
            + self.cache_read_tokens
            + self.output_tokens
            + self.reasoning_output_tokens
    }
}

impl From<&EventRow> for TokenComponents {
    fn from(event: &EventRow) -> Self {
        Self {
            input_tokens: event.input_tokens,
            cache_creation_tokens: event.cache_creation_tokens,
            cache_read_tokens: event.cache_read_tokens,
            output_tokens: event.output_tokens,
            reasoning_output_tokens: event.reasoning_output_tokens,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Aggregate {
    totals: TokenTotals,
    breakdowns: BTreeMap<(String, String), TokenTotals>,
    models: BTreeSet<String>,
    conversations: BTreeSet<String>,
    pricing_statuses: BTreeSet<String>,
    sources: BTreeSet<String>,
}

type SessionGroup = (
    Aggregate,
    Option<ProjectSummary>,
    Option<String>,
    Option<String>,
    String,
    String,
);

impl Aggregate {
    fn add_event(&mut self, event: &EventRow) {
        let tokens = TokenComponents::from(event);
        add_tokens(&mut self.totals, tokens, event.cost_with_cache_usd);
        self.models.insert(event.model.clone());
        self.conversations.insert(event_session_id(event));
        self.pricing_statuses.insert(event.pricing_status.clone());
        self.sources.insert(event.source.clone());
        let entry = self
            .breakdowns
            .entry((event.source.clone(), event.model.clone()))
            .or_default();
        add_tokens(entry, tokens, event.cost_with_cache_usd);
    }

    fn model_names(&self) -> Vec<String> {
        self.models.iter().cloned().collect()
    }

    fn conversation_count(&self) -> usize {
        self.conversations.len()
    }

    fn model_breakdowns(&self, include: bool) -> Vec<ModelCostBreakdown> {
        if !include {
            return Vec::new();
        }
        self.breakdowns
            .iter()
            .map(|((source, model), totals)| ModelCostBreakdown {
                source: source.clone(),
                model: model.clone(),
                input_tokens: totals.input_tokens,
                cache_creation_tokens: totals.cache_creation_tokens,
                cache_read_tokens: totals.cache_read_tokens,
                output_tokens: totals.output_tokens,
                reasoning_output_tokens: totals.reasoning_output_tokens,
                total_tokens: totals.total_tokens,
                estimated_cost_usd: totals.estimated_cost_usd,
            })
            .collect()
    }

    fn notes(&self) -> ReportNotes {
        ReportNotes {
            unpriced: self
                .pricing_statuses
                .iter()
                .any(|status| status == "unpriced"),
            reason_not_reported: self.sources.iter().any(|source| source == "claude")
                && self.totals.reasoning_output_tokens == 0,
        }
    }
}

fn add_tokens(totals: &mut TokenTotals, tokens: TokenComponents, cost: f64) {
    totals.input_tokens += tokens.input_tokens;
    totals.cache_creation_tokens += tokens.cache_creation_tokens;
    totals.cache_read_tokens += tokens.cache_read_tokens;
    totals.output_tokens += tokens.output_tokens;
    totals.reasoning_output_tokens += tokens.reasoning_output_tokens;
    totals.total_tokens += tokens.total_tokens();
    totals.estimated_cost_usd += cost;
}

pub fn load_daily_report(store: &Store, filter: &ReportFilter) -> Result<DailyReport> {
    let events = load_filtered_events(store, filter)?;
    let mut groups: BTreeMap<String, Aggregate> = BTreeMap::new();
    let mut totals = TokenTotals::default();
    for event in &events {
        groups
            .entry(event.local_date.format("%Y-%m-%d").to_string())
            .or_default()
            .add_event(event);
        add_totals_from_event(&mut totals, event);
    }

    let mut daily = groups
        .into_iter()
        .map(|(date, aggregate)| DailyReportRow {
            date,
            source: filter.source.map(|value| value.as_str().to_string()),
            project: None,
            totals: aggregate.totals.clone(),
            models_used: aggregate.model_names(),
            model_breakdowns: aggregate.model_breakdowns(filter.breakdown),
            conversation_count: aggregate.conversation_count(),
            notes: aggregate.notes(),
        })
        .collect::<Vec<_>>();
    sort_by_key(&mut daily, filter.order, |row| row.date.clone());
    Ok(DailyReport { daily, totals })
}

pub fn load_daily_reports_by_source(
    store: &Store,
    filter: &ReportFilter,
) -> Result<Vec<(SourceKind, DailyReport)>> {
    let events = load_filtered_events(store, filter)?;
    let mut source_groups: BTreeMap<SourceKind, BTreeMap<String, Aggregate>> = BTreeMap::new();
    let mut source_totals: BTreeMap<SourceKind, TokenTotals> = BTreeMap::new();

    for event in &events {
        let Some(source) = SourceKind::parse_id(&event.source) else {
            continue;
        };
        source_groups
            .entry(source)
            .or_default()
            .entry(event.local_date.format("%Y-%m-%d").to_string())
            .or_default()
            .add_event(event);
        add_totals_from_event(source_totals.entry(source).or_default(), event);
    }

    let source_order = [
        SourceKind::Codex,
        SourceKind::Claude,
        SourceKind::Opencode,
        SourceKind::Antigravity,
    ];
    let mut reports = Vec::new();
    for source in source_order {
        let Some(groups) = source_groups.remove(&source) else {
            continue;
        };
        let mut daily = groups
            .into_iter()
            .map(|(date, aggregate)| DailyReportRow {
                date,
                source: Some(source.as_str().to_string()),
                project: None,
                totals: aggregate.totals.clone(),
                models_used: aggregate.model_names(),
                model_breakdowns: aggregate.model_breakdowns(filter.breakdown),
                conversation_count: aggregate.conversation_count(),
                notes: aggregate.notes(),
            })
            .collect::<Vec<_>>();
        sort_by_key(&mut daily, filter.order, |row| row.date.clone());
        reports.push((
            source,
            DailyReport {
                daily,
                totals: source_totals.remove(&source).unwrap_or_default(),
            },
        ));
    }

    Ok(reports)
}

pub fn load_daily_project_report(
    store: &Store,
    filter: &ReportFilter,
) -> Result<DailyProjectReport> {
    let events = load_filtered_events(store, filter)?;
    let mut groups: BTreeMap<(String, ProjectSummary), Aggregate> = BTreeMap::new();
    let mut totals = TokenTotals::default();
    for event in &events {
        let project = event.project.clone().unwrap_or_else(unknown_project);
        let date = event.local_date.format("%Y-%m-%d").to_string();
        groups.entry((date, project)).or_default().add_event(event);
        add_totals_from_event(&mut totals, event);
    }

    let mut projects: BTreeMap<String, Vec<DailyReportRow>> = BTreeMap::new();
    for ((date, project), aggregate) in groups {
        let label = project_display_key(&project);
        projects.entry(label).or_default().push(DailyReportRow {
            date,
            source: filter.source.map(|value| value.as_str().to_string()),
            project: Some(project),
            totals: aggregate.totals.clone(),
            models_used: aggregate.model_names(),
            model_breakdowns: aggregate.model_breakdowns(filter.breakdown),
            conversation_count: aggregate.conversation_count(),
            notes: aggregate.notes(),
        });
    }
    for rows in projects.values_mut() {
        sort_by_key(rows, filter.order, |row| row.date.clone());
    }

    Ok(DailyProjectReport { projects, totals })
}

pub fn load_monthly_report(store: &Store, filter: &ReportFilter) -> Result<MonthlyReport> {
    let events = load_filtered_events(store, filter)?;
    let mut groups: BTreeMap<String, Aggregate> = BTreeMap::new();
    let mut totals = TokenTotals::default();
    for event in &events {
        groups
            .entry(event.local_date.format("%Y-%m").to_string())
            .or_default()
            .add_event(event);
        add_totals_from_event(&mut totals, event);
    }

    let mut monthly = groups
        .into_iter()
        .map(|(month, aggregate)| MonthlyReportRow {
            month,
            source: filter.source.map(|value| value.as_str().to_string()),
            totals: aggregate.totals.clone(),
            models_used: aggregate.model_names(),
            model_breakdowns: aggregate.model_breakdowns(filter.breakdown),
        })
        .collect::<Vec<_>>();
    sort_by_key(&mut monthly, filter.order, |row| row.month.clone());
    Ok(MonthlyReport { monthly, totals })
}

pub fn load_session_report(
    store: &Store,
    filter: &ReportFilter,
    session_id_filter: Option<&str>,
) -> Result<SessionListReport> {
    let events = load_filtered_events(store, filter)?;
    let wanted = session_id_filter.map(|value| value.to_ascii_lowercase());
    let mut groups: BTreeMap<String, SessionGroup> = BTreeMap::new();
    let mut totals = TokenTotals::default();

    for event in &events {
        let session_id = event_session_id(event);
        if let Some(wanted) = &wanted {
            let normalized = session_id.to_ascii_lowercase();
            if normalized != *wanted && !normalized.contains(wanted) {
                continue;
            }
        }
        let display_at = event.local_at.to_rfc3339();
        let entry = groups.entry(session_id).or_insert_with(|| {
            (
                Aggregate::default(),
                event.project.clone(),
                event.session_label.clone(),
                Some(event.source.clone()),
                display_at.clone(),
                display_at.clone(),
            )
        });
        entry.0.add_event(event);
        if entry.1.is_none() {
            entry.1 = event.project.clone();
        }
        if entry.2.is_none() {
            entry.2 = event.session_label.clone();
        }
        if entry.3.as_deref() != Some(event.source.as_str()) {
            entry.3 = None;
        }
        if display_at < entry.4 {
            entry.4 = display_at.clone();
        }
        if display_at > entry.5 {
            entry.5 = display_at;
        }
        add_totals_from_event(&mut totals, event);
    }

    let mut sessions = groups
        .into_iter()
        .map(
            |(session_id, (aggregate, project, session_label, source, first, last))| {
                SessionReportRow {
                    session_id,
                    session_label,
                    project,
                    source,
                    first_activity_at: first,
                    last_activity_at: last,
                    totals: aggregate.totals.clone(),
                    models_used: aggregate.model_names(),
                    model_breakdowns: aggregate.model_breakdowns(filter.breakdown),
                }
            },
        )
        .collect::<Vec<_>>();
    sort_by_key(&mut sessions, filter.order, |row| {
        row.last_activity_at.clone()
    });
    Ok(SessionListReport { sessions, totals })
}

pub fn load_single_session_report(
    store: &Store,
    filter: &ReportFilter,
    session_id: &str,
) -> Result<SingleSessionReport> {
    let mut report = load_session_report(store, filter, Some(session_id))?;
    Ok(SingleSessionReport {
        session: report.sessions.pop(),
    })
}

pub fn load_blocks_report(
    store: &Store,
    filter: &ReportFilter,
    options: &BlockReportOptions,
) -> Result<BlocksReport> {
    let mut events = load_filtered_events(store, filter)?;
    events.sort_by_key(|event| event.event_utc);
    let session_length =
        Duration::milliseconds((options.session_length_hours * 3_600_000.0) as i64);
    let now = Utc::now();
    let mut aggregates: Vec<(DateTime<Utc>, DateTime<Utc>, Aggregate)> = Vec::new();

    for event in &events {
        if aggregates.is_empty() {
            let start = floor_to_hour(event.event_utc);
            aggregates.push((start, start + session_length, Aggregate::default()));
        }
        let last_index = aggregates.len() - 1;
        let should_start_new = event.event_utc >= aggregates[last_index].1;
        if should_start_new {
            let start = floor_to_hour(event.event_utc);
            aggregates.push((start, start + session_length, Aggregate::default()));
        }
        let last_index = aggregates.len() - 1;
        aggregates[last_index].2.add_event(event);
    }

    let max_tokens = aggregates
        .iter()
        .map(|(_, _, aggregate)| aggregate.totals.total_tokens.max(0) as u64)
        .max()
        .unwrap_or_default();
    let explicit_limit = match options.token_limit {
        Some(TokenLimit::Max) => (max_tokens > 0).then_some(max_tokens),
        Some(TokenLimit::Value(value)) => Some(value),
        None => None,
    };

    let recent_cutoff = now - Duration::days(3);
    let mut blocks = aggregates
        .into_iter()
        .filter_map(|(start, end, aggregate)| {
            let is_active = now >= start && now < end;
            if options.active_only && !is_active {
                return None;
            }
            if options.recent_only && !is_active && start < recent_cutoff {
                return None;
            }
            let elapsed_hours = if is_active {
                let millis = (now - start).num_milliseconds().max(1) as f64;
                millis / 3_600_000.0
            } else {
                options.session_length_hours.max(0.000_001)
            };
            let burn_rate = aggregate.totals.total_tokens as f64 / elapsed_hours;
            let projected_total_tokens = if is_active {
                (burn_rate * options.session_length_hours).round() as i64
            } else {
                aggregate.totals.total_tokens
            };
            let token_limit_percent = explicit_limit.and_then(|limit| {
                (limit > 0).then(|| projected_total_tokens.max(0) as f64 / limit as f64 * 100.0)
            });
            Some(BlockReportRow {
                block_id: format!("{}-{}", start.format("%Y%m%d%H"), end.format("%H")),
                start_at: display_time(start, &filter.timezone),
                end_at: display_time(end, &filter.timezone),
                is_active,
                duration_minutes: session_length.num_minutes(),
                burn_rate_tokens_per_hour: burn_rate,
                projected_total_tokens,
                token_limit: explicit_limit,
                token_limit_percent,
                totals: aggregate.totals.clone(),
                models_used: aggregate.model_names(),
            })
        })
        .collect::<Vec<_>>();
    sort_by_key(&mut blocks, filter.order, |row| row.start_at.clone());
    Ok(BlocksReport { blocks })
}

pub fn load_statusline_summary(
    store: &Store,
    timezone: ReportTimezone,
) -> Result<StatuslineSummary> {
    let today = today_for_timezone(&timezone);
    let filter = ReportFilter {
        since: Some(today),
        until: Some(today),
        order: SortOrder::Desc,
        timezone,
        locale: "en-US".to_string(),
        source: None,
        project: None,
        breakdown: false,
    };
    let daily = load_daily_report(store, &filter)?;
    let blocks = load_blocks_report(
        store,
        &filter,
        &BlockReportOptions {
            active_only: true,
            recent_only: false,
            token_limit: None,
            session_length_hours: 5.0,
        },
    )?;
    Ok(StatuslineSummary {
        today: daily.totals,
        active_block: blocks.blocks.into_iter().next(),
        generated_at: crate::util::now_utc(),
    })
}

pub fn today_for_timezone(timezone: &ReportTimezone) -> NaiveDate {
    apply_timezone(Utc::now(), timezone).date_naive()
}

fn add_totals_from_event(totals: &mut TokenTotals, event: &EventRow) {
    add_tokens(
        totals,
        TokenComponents::from(event),
        event.cost_with_cache_usd,
    );
}

fn sort_by_key<T, F>(rows: &mut [T], order: SortOrder, key: F)
where
    F: Fn(&T) -> String,
{
    rows.sort_by_key(|row| key(row));
    if matches!(order, SortOrder::Desc) {
        rows.reverse();
    }
}

fn load_filtered_events(store: &Store, filter: &ReportFilter) -> Result<Vec<EventRow>> {
    let conn = store.open_connection()?;
    let rows = load_events_filtered(&conn, filter)?;
    Ok(rows
        .into_iter()
        .filter(|event| filter_event_post_sql(event, filter))
        .collect())
}

/// Loads events with date/source filters pushed down to SQL for performance.
/// Project filtering remains in Rust because it requires fuzzy matching.
fn load_events_filtered(conn: &Connection, filter: &ReportFilter) -> Result<Vec<EventRow>> {
    let mut clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(source) = filter.source {
        clauses.push("source = ?".to_string());
        params.push(Box::new(source.as_str().to_string()));
    }
    if let Some(since) = filter.since {
        // Convert local date start to UTC for SQL comparison
        let utc_start = local_date_to_utc_start(since, &filter.timezone);
        clauses.push("event_at >= ?".to_string());
        params.push(Box::new(utc_start));
    }
    if let Some(until) = filter.until {
        // Convert local date end (exclusive next day) to UTC for SQL comparison
        if let Some(exclusive) = until.succ_opt() {
            let utc_end = local_date_to_utc_start(exclusive, &filter.timezone);
            clauses.push("event_at < ?".to_string());
            params.push(Box::new(utc_end));
        }
    }

    let where_clause = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };

    let sql = format!(
        r#"
        SELECT
            event_key,
            source,
            model,
            event_at,
            input_tokens,
            cache_creation_tokens,
            cache_read_tokens,
            output_tokens,
            reasoning_output_tokens,
            cost_with_cache_usd,
            pricing_status,
            project_hash,
            project_label,
            project_ref,
            session_id,
            session_label,
            source_path_hash
        FROM usage_event
        {where_clause}
        ORDER BY event_at ASC, event_key ASC
        "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let event_at: String = row.get(3)?;
        let event_utc = DateTime::parse_from_rfc3339(&event_at)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?;
        Ok(RawEventRow {
            event_key: row.get(0)?,
            source: row.get(1)?,
            model: row.get(2)?,
            event_utc,
            input_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
            cache_creation_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
            cache_read_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
            output_tokens: row.get::<_, Option<i64>>(7)?.unwrap_or_default(),
            reasoning_output_tokens: row.get::<_, Option<i64>>(8)?.unwrap_or_default(),
            cost_with_cache_usd: row.get::<_, Option<f64>>(9)?.unwrap_or_default(),
            pricing_status: row
                .get::<_, Option<String>>(10)?
                .unwrap_or_else(|| crate::query::pricing::PRICING_UNPRICED.to_string()),
            project_hash: row.get(11)?,
            project_label: row.get(12)?,
            project_ref: row.get(13)?,
            session_id: row.get(14)?,
            session_label: row.get(15)?,
            source_path_hash: row.get(16)?,
        })
    })?;
    let raw = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(raw
        .into_iter()
        .map(|event| event.with_timezone(&filter.timezone))
        .collect())
}

/// Converts a local NaiveDate midnight to a UTC RFC 3339 string for SQL filtering.
fn local_date_to_utc_start(date: NaiveDate, timezone: &ReportTimezone) -> String {
    use chrono::{Offset, SecondsFormat, TimeZone, offset::LocalResult};
    let local_start = date.and_hms_opt(0, 0, 0).expect("midnight is always valid");
    let offset = match timezone {
        ReportTimezone::Utc => Utc.fix(),
        ReportTimezone::Local => Local::now().offset().fix(),
        ReportTimezone::Fixed(offset) => *offset,
    };
    let utc = match offset.from_local_datetime(&local_start) {
        LocalResult::Single(value) => value.with_timezone(&Utc),
        LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
        LocalResult::None => offset.from_utc_datetime(&local_start).with_timezone(&Utc),
    };
    utc.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Post-SQL filter for conditions not pushed to the database (project fuzzy match).
fn filter_event_post_sql(event: &EventRow, filter: &ReportFilter) -> bool {
    // Date and source are already filtered in SQL; only project needs Rust-side check.
    if let Some(project_filter) = &filter.project
        && !project_matches(event.project.as_ref(), project_filter)
    {
        return false;
    }
    true
}

#[derive(Debug, Clone)]
struct RawEventRow {
    event_key: String,
    source: String,
    model: String,
    event_utc: DateTime<Utc>,
    input_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    cost_with_cache_usd: f64,
    pricing_status: String,
    project_hash: Option<String>,
    project_label: Option<String>,
    project_ref: Option<String>,
    session_id: Option<String>,
    session_label: Option<String>,
    source_path_hash: Option<String>,
}

impl RawEventRow {
    fn with_timezone(self, timezone: &ReportTimezone) -> EventRow {
        let local_at = apply_timezone(self.event_utc, timezone);
        EventRow {
            event_key: self.event_key,
            source: self.source,
            model: self.model,
            event_utc: self.event_utc,
            local_date: local_at.date_naive(),
            local_at,
            input_tokens: self.input_tokens,
            cache_creation_tokens: self.cache_creation_tokens,
            cache_read_tokens: self.cache_read_tokens,
            output_tokens: self.output_tokens,
            reasoning_output_tokens: self.reasoning_output_tokens,
            cost_with_cache_usd: self.cost_with_cache_usd,
            pricing_status: self.pricing_status,
            project: normalize_project(self.project_hash, self.project_label, self.project_ref),
            session_id: self.session_id,
            session_label: self.session_label,
            source_path_hash: self.source_path_hash,
        }
    }
}

fn normalize_project(
    project_hash: Option<String>,
    project_label: Option<String>,
    project_ref: Option<String>,
) -> Option<ProjectSummary> {
    let project_hash = project_hash?.trim().to_string();
    if project_hash.is_empty() {
        return None;
    }
    let project_label = project_label
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| project_hash.clone());
    Some(ProjectSummary {
        project_hash,
        project_label,
        project_ref,
    })
}

fn project_matches(project: Option<&ProjectSummary>, needle: &str) -> bool {
    let needle = needle.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return true;
    }
    let Some(project) = project else {
        return false;
    };
    project.project_hash.to_ascii_lowercase().contains(&needle)
        || project.project_label.to_ascii_lowercase().contains(&needle)
        || project
            .project_ref
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains(&needle)
}

fn unknown_project() -> ProjectSummary {
    ProjectSummary {
        project_hash: "".to_string(),
        project_label: "unknown-project".to_string(),
        project_ref: None,
    }
}

fn project_display_key(project: &ProjectSummary) -> String {
    if let Some(project_ref) = &project.project_ref
        && !project_ref.trim().is_empty()
    {
        return project_ref.clone();
    }
    project.project_label.clone()
}

fn event_session_id(event: &EventRow) -> String {
    event
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{}:{value}", event.source))
        .or_else(|| {
            event
                .source_path_hash
                .as_ref()
                .map(|value| format!("{}:{value}", event.source))
        })
        .unwrap_or_else(|| fallback_session_id(&event.source, &event.event_key))
}

fn fallback_session_id(source: &str, event_key: &str) -> String {
    let parts = event_key.split(':').collect::<Vec<_>>();
    if parts.len() >= 4 && (source == "codex" || source == "claude") {
        return format!("{}:{}:{}", source, parts[1], parts[2]);
    }
    event_key.to_string()
}

fn floor_to_hour(value: DateTime<Utc>) -> DateTime<Utc> {
    value
        .with_minute(0)
        .and_then(|value| value.with_second(0))
        .and_then(|value| value.with_nanosecond(0))
        .unwrap_or(value)
}

fn display_time(value: DateTime<Utc>, timezone: &ReportTimezone) -> String {
    apply_timezone(value, timezone).to_rfc3339()
}

fn apply_timezone(value: DateTime<Utc>, timezone: &ReportTimezone) -> DateTime<FixedOffset> {
    match timezone {
        ReportTimezone::Utc => value.with_timezone(&Utc.fix()),
        ReportTimezone::Local => value.with_timezone(&Local).fixed_offset(),
        ReportTimezone::Fixed(offset) => value.with_timezone(offset),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rusqlite::params;
    use tempfile::TempDir;

    use super::*;
    use crate::{paths::AppPaths, store::Store};

    #[test]
    fn daily_report_filters_and_groups_by_local_date() -> Result<()> {
        let fixture = ReportFixture::new()?;
        fixture.insert_event(SeedEvent {
            event_key: "a",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-04T16:30:00Z",
            total_tokens: 10,
            project_hash: "p1",
            project_label: "Demo",
            session_id: "session-a",
        })?;
        let report = load_daily_report(
            &fixture.store,
            &ReportFilter {
                since: Some(NaiveDate::from_ymd_opt(2026, 5, 5).unwrap()),
                until: Some(NaiveDate::from_ymd_opt(2026, 5, 5).unwrap()),
                order: SortOrder::Desc,
                timezone: ReportTimezone::Fixed(FixedOffset::east_opt(8 * 3600).unwrap()),
                locale: "en-US".to_string(),
                source: Some(SourceKind::Codex),
                project: Some("Demo".to_string()),
                breakdown: true,
            },
        )?;
        assert_eq!(report.daily.len(), 1);
        assert_eq!(report.daily[0].date, "2026-05-05");
        assert_eq!(report.daily[0].totals.total_tokens, 10);
        assert_eq!(report.daily[0].model_breakdowns.len(), 1);
        Ok(())
    }

    #[test]
    fn session_report_falls_back_to_event_key_when_metadata_missing() -> Result<()> {
        let fixture = ReportFixture::new()?;
        fixture.insert_event(SeedEvent {
            event_key: "codex:pathhash:fingerprint:42",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-05T00:00:00Z",
            total_tokens: 12,
            project_hash: "p1",
            project_label: "Demo",
            session_id: "",
        })?;
        let report = load_session_report(
            &fixture.store,
            &ReportFilter {
                since: None,
                until: None,
                order: SortOrder::Desc,
                timezone: ReportTimezone::Utc,
                locale: "en-US".to_string(),
                source: None,
                project: None,
                breakdown: false,
            },
            Some("pathhash"),
        )?;
        assert_eq!(report.sessions.len(), 1);
        assert_eq!(report.sessions[0].session_id, "codex:pathhash:fingerprint");
        Ok(())
    }

    struct ReportFixture {
        _temp: TempDir,
        store: Store,
    }

    impl ReportFixture {
        fn new() -> Result<Self> {
            let temp = TempDir::new()?;
            let root_dir = temp.path().join(".llmusage");
            let paths = AppPaths::with_root(root_dir)?;
            let store = Store::new(&paths)?;
            store.bootstrap()?;
            Ok(Self { _temp: temp, store })
        }

        fn insert_event(&self, event: SeedEvent<'_>) -> Result<()> {
            let conn = self.store.open_connection()?;
            conn.execute(
                r#"
                INSERT INTO usage_event(
                    event_key, source, model, event_at, hour_start,
                    input_tokens, cache_read_tokens, output_tokens, reasoning_output_tokens, total_tokens,
                    pricing_status,
                    project_hash, project_label, project_ref, path_hash, session_id, session_label, source_path_hash, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, 0, 0, 0, ?5, 'unpriced', ?6, ?7, NULL, ?8, NULLIF(?9, ''), NULL, CASE WHEN ?9 = '' THEN NULL ELSE ?8 END, ?4)
                "#,
                params![
                    event.event_key,
                    event.source,
                    event.model,
                    event.event_at,
                    event.total_tokens,
                    event.project_hash,
                    event.project_label,
                    format!("source-path-{}", event.event_key),
                    event.session_id,
                ],
            )?;
            Ok(())
        }
    }

    struct SeedEvent<'a> {
        event_key: &'a str,
        source: &'a str,
        model: &'a str,
        event_at: &'a str,
        total_tokens: i64,
        project_hash: &'a str,
        project_label: &'a str,
        session_id: &'a str,
    }
}
