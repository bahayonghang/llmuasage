use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

use anyhow::Result;
use chrono::{
    DateTime, Datelike, Duration, FixedOffset, Local, NaiveDate, Offset, SecondsFormat, Timelike,
    Utc,
};
use rusqlite::Connection;
use serde::Serialize;
use tracing::debug;

pub use super::ReportTimezone;
use crate::{
    domain::source_descriptor::{registered_source_descriptors, source_descriptor},
    models::SourceKind,
    store::Store,
};

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
pub struct WeeklyReportRow {
    pub week: String,
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
    /// Wall-clock span (first→last activity) in minutes.
    pub span_minutes: i64,
    /// Gap-capped active minutes: adjacent-event gaps over 30 minutes are
    /// treated as "away" and excluded, approximating hands-on time.
    pub active_minutes: i64,
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
pub struct WeeklyReport {
    pub weekly: Vec<WeeklyReportRow>,
    pub totals: TokenTotals,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionListReport {
    pub sessions: Vec<SessionReportRow>,
    pub totals: TokenTotals,
}

/// Period grouping used by the CLI's unified cross-source report surface.
///
/// `Weekly` is intentionally part of the type before the command is wired so
/// the shared renderer and JSON projection do not need a second shape later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodKind {
    Daily,
    Weekly,
    Monthly,
    Session,
}

impl PeriodKind {
    pub fn rows_key(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Session => "session",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
            Self::Session => "Session",
        }
    }

    pub fn first_column(self) -> &'static str {
        match self {
            Self::Daily => "Date",
            Self::Weekly => "Week",
            Self::Monthly => "Month",
            Self::Session => "Session",
        }
    }
}

/// Identity shown in the unified report's Agent column.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnifiedAgent {
    All,
    Source(SourceKind),
    Unknown(String),
}

impl UnifiedAgent {
    pub fn id(&self) -> &str {
        match self {
            Self::All => "all",
            Self::Source(source) => source.as_str(),
            Self::Unknown(source) => source,
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::All => "All".to_string(),
            Self::Source(source) => source_descriptor(*source)
                .map(|descriptor| descriptor.display_name.to_string())
                .unwrap_or_else(|| source.as_str().to_string()),
            Self::Unknown(source) => source.clone(),
        }
    }

    pub fn source_kind(&self) -> Option<SourceKind> {
        match self {
            Self::Source(source) => Some(*source),
            Self::All | Self::Unknown(_) => None,
        }
    }
}

/// A presentation-only row for CLI reports. It deliberately reuses
/// `TokenTotals` as a numeric container while keeping its serialization out of
/// the CLI JSON contract.
#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedRow {
    pub period: String,
    pub agent: UnifiedAgent,
    pub totals: TokenTotals,
    pub models_used: Vec<String>,
    pub agent_breakdowns: Vec<UnifiedRow>,
    pub model_breakdowns: Vec<ModelCostBreakdown>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedReport {
    pub kind: PeriodKind,
    pub rows: Vec<UnifiedRow>,
    pub detected: Vec<SourceKind>,
}

impl UnifiedReport {
    pub fn totals(&self) -> TokenTotals {
        let mut totals = TokenTotals::default();
        for row in &self.rows {
            add_token_totals(&mut totals, &row.totals);
        }
        totals
    }

    pub fn detected_labels(&self) -> Vec<String> {
        self.detected
            .iter()
            .filter_map(|source| source_descriptor(*source))
            .map(|descriptor| descriptor.display_name.to_string())
            .collect()
    }
}

#[derive(Debug, Clone)]
struct UnifiedPeriodInput {
    period: String,
    totals: TokenTotals,
    models_used: Vec<String>,
    model_breakdowns: Vec<ModelCostBreakdown>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SingleSessionReport {
    pub session: Option<SessionReportRow>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BlocksReport {
    pub blocks: Vec<BlockReportRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BlocksScanStats {
    anchor_probe_events: usize,
    scanned_events: usize,
    scan_start: Option<String>,
    fell_back_to_full_scan: bool,
}

struct LoadedBlocksReport {
    report: BlocksReport,
    #[cfg(test)]
    scan: BlocksScanStats,
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
    total_tokens: i64,
    cost_with_cache_usd: f64,
    pricing_status: String,
    project: Option<ProjectSummary>,
    session_id: Option<String>,
    session_label: Option<String>,
    source_path_hash: Option<String>,
}

#[derive(Debug, Clone)]
struct BucketRow {
    source: String,
    model: String,
    local_date: NaiveDate,
    input_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
    cost_with_cache_usd: f64,
    pricing_status: String,
}

#[derive(Debug, Clone)]
struct ProjectBucketRow {
    bucket: BucketRow,
    project: Option<ProjectSummary>,
}

#[derive(Debug, Clone, Copy)]
struct TokenComponents {
    input_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
}

impl TokenComponents {
    fn total_tokens(self) -> i64 {
        self.total_tokens
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
            total_tokens: event.total_tokens,
        }
    }
}

impl From<&BucketRow> for TokenComponents {
    fn from(bucket: &BucketRow) -> Self {
        Self {
            input_tokens: bucket.input_tokens,
            cache_creation_tokens: bucket.cache_creation_tokens,
            cache_read_tokens: bucket.cache_read_tokens,
            output_tokens: bucket.output_tokens,
            reasoning_output_tokens: bucket.reasoning_output_tokens,
            total_tokens: bucket.total_tokens,
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

type SourcePeriodGroups = BTreeMap<SourceKind, BTreeMap<String, Aggregate>>;
type SourcePeriodTotals = BTreeMap<SourceKind, TokenTotals>;

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

    fn add_bucket(&mut self, bucket: &BucketRow) {
        let tokens = TokenComponents::from(bucket);
        add_tokens(&mut self.totals, tokens, bucket.cost_with_cache_usd);
        self.models.insert(bucket.model.clone());
        self.pricing_statuses.insert(bucket.pricing_status.clone());
        self.sources.insert(bucket.source.clone());
        let entry = self
            .breakdowns
            .entry((bucket.source.clone(), bucket.model.clone()))
            .or_default();
        add_tokens(entry, tokens, bucket.cost_with_cache_usd);
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

fn add_token_totals(target: &mut TokenTotals, source: &TokenTotals) {
    target.input_tokens += source.input_tokens;
    target.cache_creation_tokens += source.cache_creation_tokens;
    target.cache_read_tokens += source.cache_read_tokens;
    target.output_tokens += source.output_tokens;
    target.reasoning_output_tokens += source.reasoning_output_tokens;
    target.total_tokens += source.total_tokens;
    target.estimated_cost_usd += source.estimated_cost_usd;
}

pub fn load_daily_report(store: &Store, filter: &ReportFilter) -> Result<DailyReport> {
    if filter.project.is_some() {
        return load_daily_report_from_events(store, filter);
    }
    let buckets = load_filtered_buckets(store, filter)?;
    let mut groups: BTreeMap<String, Aggregate> = BTreeMap::new();
    let mut totals = TokenTotals::default();
    for bucket in &buckets {
        groups
            .entry(bucket.local_date.format("%Y-%m-%d").to_string())
            .or_default()
            .add_bucket(bucket);
        add_totals_from_bucket(&mut totals, bucket);
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

fn load_daily_report_from_events(store: &Store, filter: &ReportFilter) -> Result<DailyReport> {
    let mut groups: BTreeMap<String, Aggregate> = BTreeMap::new();
    let mut totals = TokenTotals::default();
    visit_filtered_events(store, filter, |event| {
        groups
            .entry(event.local_date.format("%Y-%m-%d").to_string())
            .or_default()
            .add_event(&event);
        add_totals_from_event(&mut totals, &event);
        Ok(())
    })?;

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
    let (source_groups, source_totals) =
        load_source_period_aggregates(store, filter, daily_period_key)?;
    Ok(build_daily_reports_by_source(
        filter,
        source_groups,
        source_totals,
    ))
}

fn daily_period_key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn monthly_period_key(date: NaiveDate) -> String {
    date.format("%Y-%m").to_string()
}

fn weekly_period_key(date: NaiveDate) -> String {
    week_start(date).format("%Y-%m-%d").to_string()
}

fn week_start(date: NaiveDate) -> NaiveDate {
    date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
}

fn load_source_period_aggregates(
    store: &Store,
    filter: &ReportFilter,
    period_key: fn(NaiveDate) -> String,
) -> Result<(SourcePeriodGroups, SourcePeriodTotals)> {
    let mut source_groups = SourcePeriodGroups::new();
    let mut source_totals = SourcePeriodTotals::new();

    if filter.project.is_some() {
        visit_filtered_events(store, filter, |event| {
            let Some(source) = SourceKind::parse_id(&event.source) else {
                return Ok(());
            };
            source_groups
                .entry(source)
                .or_default()
                .entry(period_key(event.local_date))
                .or_default()
                .add_event(&event);
            add_totals_from_event(source_totals.entry(source).or_default(), &event);
            Ok(())
        })?;
    } else {
        for bucket in load_filtered_buckets(store, filter)? {
            let Some(source) = SourceKind::parse_id(&bucket.source) else {
                continue;
            };
            source_groups
                .entry(source)
                .or_default()
                .entry(period_key(bucket.local_date))
                .or_default()
                .add_bucket(&bucket);
            add_totals_from_bucket(source_totals.entry(source).or_default(), &bucket);
        }
    }

    Ok((source_groups, source_totals))
}

fn build_daily_reports_by_source(
    filter: &ReportFilter,
    mut source_groups: BTreeMap<SourceKind, BTreeMap<String, Aggregate>>,
    mut source_totals: BTreeMap<SourceKind, TokenTotals>,
) -> Vec<(SourceKind, DailyReport)> {
    let mut reports = Vec::new();
    for source in registered_source_descriptors()
        .iter()
        .map(|descriptor| descriptor.kind)
    {
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

    reports
}

pub fn load_daily_project_report(
    store: &Store,
    filter: &ReportFilter,
) -> Result<DailyProjectReport> {
    if filter.project.is_some() {
        return load_daily_project_report_from_events(store, filter);
    }
    let buckets = load_filtered_project_buckets(store, filter)?;
    let mut groups: BTreeMap<(String, ProjectSummary), Aggregate> = BTreeMap::new();
    let mut totals = TokenTotals::default();
    for bucket in &buckets {
        let project = bucket.project.clone().unwrap_or_else(unknown_project);
        let date = bucket.bucket.local_date.format("%Y-%m-%d").to_string();
        groups
            .entry((date, project))
            .or_default()
            .add_bucket(&bucket.bucket);
        add_totals_from_bucket(&mut totals, &bucket.bucket);
    }

    Ok(build_daily_project_report(filter, groups, totals))
}

fn load_daily_project_report_from_events(
    store: &Store,
    filter: &ReportFilter,
) -> Result<DailyProjectReport> {
    let mut groups: BTreeMap<(String, ProjectSummary), Aggregate> = BTreeMap::new();
    let mut totals = TokenTotals::default();
    visit_filtered_events(store, filter, |event| {
        let project = event.project.clone().unwrap_or_else(unknown_project);
        let date = event.local_date.format("%Y-%m-%d").to_string();
        groups.entry((date, project)).or_default().add_event(&event);
        add_totals_from_event(&mut totals, &event);
        Ok(())
    })?;

    Ok(build_daily_project_report(filter, groups, totals))
}

fn build_daily_project_report(
    filter: &ReportFilter,
    groups: BTreeMap<(String, ProjectSummary), Aggregate>,
    totals: TokenTotals,
) -> DailyProjectReport {
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

    DailyProjectReport { projects, totals }
}

pub fn load_monthly_report(store: &Store, filter: &ReportFilter) -> Result<MonthlyReport> {
    if filter.project.is_some() {
        return load_monthly_report_from_events(store, filter);
    }
    let buckets = load_filtered_buckets(store, filter)?;
    let mut groups: BTreeMap<String, Aggregate> = BTreeMap::new();
    let mut totals = TokenTotals::default();
    for bucket in &buckets {
        groups
            .entry(bucket.local_date.format("%Y-%m").to_string())
            .or_default()
            .add_bucket(bucket);
        add_totals_from_bucket(&mut totals, bucket);
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

fn load_monthly_report_from_events(store: &Store, filter: &ReportFilter) -> Result<MonthlyReport> {
    let mut groups: BTreeMap<String, Aggregate> = BTreeMap::new();
    let mut totals = TokenTotals::default();
    visit_filtered_events(store, filter, |event| {
        groups
            .entry(event.local_date.format("%Y-%m").to_string())
            .or_default()
            .add_event(&event);
        add_totals_from_event(&mut totals, &event);
        Ok(())
    })?;

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

pub fn load_monthly_reports_by_source(
    store: &Store,
    filter: &ReportFilter,
) -> Result<Vec<(SourceKind, MonthlyReport)>> {
    let (mut source_groups, mut source_totals) =
        load_source_period_aggregates(store, filter, monthly_period_key)?;
    let mut reports = Vec::new();
    for source in registered_source_descriptors()
        .iter()
        .map(|descriptor| descriptor.kind)
    {
        let Some(groups) = source_groups.remove(&source) else {
            continue;
        };
        let mut monthly = groups
            .into_iter()
            .map(|(month, aggregate)| MonthlyReportRow {
                month,
                source: Some(source.as_str().to_string()),
                totals: aggregate.totals.clone(),
                models_used: aggregate.model_names(),
                model_breakdowns: aggregate.model_breakdowns(filter.breakdown),
            })
            .collect::<Vec<_>>();
        sort_by_key(&mut monthly, filter.order, |row| row.month.clone());
        reports.push((
            source,
            MonthlyReport {
                monthly,
                totals: source_totals.remove(&source).unwrap_or_default(),
            },
        ));
    }
    Ok(reports)
}

pub fn load_weekly_report(store: &Store, filter: &ReportFilter) -> Result<WeeklyReport> {
    if filter.project.is_some() {
        return load_weekly_report_from_events(store, filter);
    }
    let buckets = load_filtered_buckets(store, filter)?;
    let mut groups: BTreeMap<String, Aggregate> = BTreeMap::new();
    let mut totals = TokenTotals::default();
    for bucket in &buckets {
        groups
            .entry(weekly_period_key(bucket.local_date))
            .or_default()
            .add_bucket(bucket);
        add_totals_from_bucket(&mut totals, bucket);
    }
    Ok(build_weekly_report(filter, groups, totals))
}

fn load_weekly_report_from_events(store: &Store, filter: &ReportFilter) -> Result<WeeklyReport> {
    let mut groups: BTreeMap<String, Aggregate> = BTreeMap::new();
    let mut totals = TokenTotals::default();
    visit_filtered_events(store, filter, |event| {
        groups
            .entry(weekly_period_key(event.local_date))
            .or_default()
            .add_event(&event);
        add_totals_from_event(&mut totals, &event);
        Ok(())
    })?;
    Ok(build_weekly_report(filter, groups, totals))
}

fn build_weekly_report(
    filter: &ReportFilter,
    groups: BTreeMap<String, Aggregate>,
    totals: TokenTotals,
) -> WeeklyReport {
    let mut weekly = groups
        .into_iter()
        .map(|(week, aggregate)| WeeklyReportRow {
            week,
            source: filter.source.map(|value| value.as_str().to_string()),
            totals: aggregate.totals.clone(),
            models_used: aggregate.model_names(),
            model_breakdowns: aggregate.model_breakdowns(filter.breakdown),
        })
        .collect::<Vec<_>>();
    sort_by_key(&mut weekly, filter.order, |row| row.week.clone());
    WeeklyReport { weekly, totals }
}

pub fn load_weekly_reports_by_source(
    store: &Store,
    filter: &ReportFilter,
) -> Result<Vec<(SourceKind, WeeklyReport)>> {
    let (mut source_groups, mut source_totals) =
        load_source_period_aggregates(store, filter, weekly_period_key)?;
    let mut reports = Vec::new();
    for source in registered_source_descriptors()
        .iter()
        .map(|descriptor| descriptor.kind)
    {
        let Some(groups) = source_groups.remove(&source) else {
            continue;
        };
        let mut weekly = groups
            .into_iter()
            .map(|(week, aggregate)| WeeklyReportRow {
                week,
                source: Some(source.as_str().to_string()),
                totals: aggregate.totals.clone(),
                models_used: aggregate.model_names(),
                model_breakdowns: aggregate.model_breakdowns(filter.breakdown),
            })
            .collect::<Vec<_>>();
        sort_by_key(&mut weekly, filter.order, |row| row.week.clone());
        reports.push((
            source,
            WeeklyReport {
                weekly,
                totals: source_totals.remove(&source).unwrap_or_default(),
            },
        ));
    }
    Ok(reports)
}

pub fn load_session_report(
    store: &Store,
    filter: &ReportFilter,
    session_id_filter: Option<&str>,
) -> Result<SessionListReport> {
    let wanted = session_id_filter.map(|value| value.to_ascii_lowercase());
    let mut groups: BTreeMap<String, SessionGroup> = BTreeMap::new();
    let mut session_times: BTreeMap<String, Vec<DateTime<FixedOffset>>> = BTreeMap::new();
    let mut totals = TokenTotals::default();

    visit_filtered_events(store, filter, |event| {
        let session_id = event_session_id(&event);
        if let Some(wanted) = &wanted {
            let normalized = session_id.to_ascii_lowercase();
            if normalized != *wanted && !normalized.contains(wanted) {
                return Ok(());
            }
        }
        let display_at = event.local_at.to_rfc3339();
        session_times
            .entry(session_id.clone())
            .or_default()
            .push(event.local_at);
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
        entry.0.add_event(&event);
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
        add_totals_from_event(&mut totals, &event);
        Ok(())
    })?;

    let mut sessions = groups
        .into_iter()
        .map(
            |(session_id, (aggregate, project, session_label, source, first, last))| {
                let (span_minutes, active_minutes) = session_times
                    .get_mut(&session_id)
                    .map(|times| session_time_span(times))
                    .unwrap_or((0, 0));
                SessionReportRow {
                    session_id,
                    session_label,
                    project,
                    source,
                    first_activity_at: first,
                    last_activity_at: last,
                    span_minutes,
                    active_minutes,
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

const ACTIVE_GAP_CAP_MINUTES: i64 = 30;

/// Returns `(span_minutes, active_minutes)` for a session's event times.
///
/// Span is last−first. Active sums adjacent-event gaps that do not exceed
/// [`ACTIVE_GAP_CAP_MINUTES`], dropping idle stretches so the result
/// approximates hands-on time rather than wall-clock presence.
fn session_time_span(times: &mut [DateTime<FixedOffset>]) -> (i64, i64) {
    if times.len() < 2 {
        return (0, 0);
    }
    times.sort_unstable();
    let span = (*times.last().unwrap() - times[0]).num_minutes().max(0);
    let mut active = 0i64;
    for pair in times.windows(2) {
        let gap = (pair[1] - pair[0]).num_minutes();
        if gap > 0 && gap <= ACTIVE_GAP_CAP_MINUTES {
            active += gap;
        }
    }
    (span, active)
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

/// Loads the CLI-only unified projection without changing the report payloads
/// consumed by the dashboard, export, or interactive TUI.
pub fn load_unified_report(
    store: &Store,
    filter: &ReportFilter,
    kind: PeriodKind,
) -> Result<UnifiedReport> {
    match kind {
        PeriodKind::Daily => {
            let aggregate = load_daily_report(store, filter)?;
            let by_source = load_daily_reports_by_source(store, filter)?;
            Ok(build_unified_period_report(
                kind,
                aggregate
                    .daily
                    .into_iter()
                    .map(unified_period_input_from_daily)
                    .collect(),
                by_source
                    .into_iter()
                    .flat_map(|(source, report)| {
                        report
                            .daily
                            .into_iter()
                            .map(move |row| (source, unified_period_input_from_daily(row)))
                    })
                    .collect(),
            ))
        }
        PeriodKind::Monthly => {
            let aggregate = load_monthly_report(store, filter)?;
            let by_source = load_monthly_reports_by_source(store, filter)?;
            Ok(build_unified_period_report(
                kind,
                aggregate
                    .monthly
                    .into_iter()
                    .map(unified_period_input_from_monthly)
                    .collect(),
                by_source
                    .into_iter()
                    .flat_map(|(source, report)| {
                        report
                            .monthly
                            .into_iter()
                            .map(move |row| (source, unified_period_input_from_monthly(row)))
                    })
                    .collect(),
            ))
        }
        PeriodKind::Session => load_unified_session_report(store, filter, None),
        PeriodKind::Weekly => {
            let aggregate = load_weekly_report(store, filter)?;
            let by_source = load_weekly_reports_by_source(store, filter)?;
            Ok(build_unified_period_report(
                kind,
                aggregate
                    .weekly
                    .into_iter()
                    .map(unified_period_input_from_weekly)
                    .collect(),
                by_source
                    .into_iter()
                    .flat_map(|(source, report)| {
                        report
                            .weekly
                            .into_iter()
                            .map(move |row| (source, unified_period_input_from_weekly(row)))
                    })
                    .collect(),
            ))
        }
    }
}

pub fn load_unified_session_report(
    store: &Store,
    filter: &ReportFilter,
    session_id_filter: Option<&str>,
) -> Result<UnifiedReport> {
    let report = load_session_report(store, filter, session_id_filter)?;
    let mut detected = BTreeSet::new();
    let mut rows = Vec::with_capacity(report.sessions.len());
    for row in report.sessions {
        let agent = row
            .source
            .as_deref()
            .map(unified_agent_from_source_id)
            .unwrap_or_else(|| UnifiedAgent::Unknown("unknown".to_string()));
        if let Some(source) = agent.source_kind() {
            detected.insert(source);
        }
        rows.push(UnifiedRow {
            period: row.session_id,
            agent,
            totals: row.totals,
            models_used: row.models_used,
            agent_breakdowns: Vec::new(),
            model_breakdowns: row.model_breakdowns,
        });
    }
    Ok(UnifiedReport {
        kind: PeriodKind::Session,
        rows,
        detected: detected_sources(&detected),
    })
}

fn unified_period_input_from_daily(row: DailyReportRow) -> UnifiedPeriodInput {
    UnifiedPeriodInput {
        period: row.date,
        totals: row.totals,
        models_used: row.models_used,
        model_breakdowns: row.model_breakdowns,
    }
}

fn unified_period_input_from_monthly(row: MonthlyReportRow) -> UnifiedPeriodInput {
    UnifiedPeriodInput {
        period: row.month,
        totals: row.totals,
        models_used: row.models_used,
        model_breakdowns: row.model_breakdowns,
    }
}

fn unified_period_input_from_weekly(row: WeeklyReportRow) -> UnifiedPeriodInput {
    UnifiedPeriodInput {
        period: row.week,
        totals: row.totals,
        models_used: row.models_used,
        model_breakdowns: row.model_breakdowns,
    }
}

fn build_unified_period_report(
    kind: PeriodKind,
    aggregate_rows: Vec<UnifiedPeriodInput>,
    source_rows: Vec<(SourceKind, UnifiedPeriodInput)>,
) -> UnifiedReport {
    let mut detected = BTreeSet::new();
    let mut breakdowns_by_period: BTreeMap<String, Vec<UnifiedRow>> = BTreeMap::new();
    for (source, row) in source_rows {
        detected.insert(source);
        breakdowns_by_period
            .entry(row.period.clone())
            .or_default()
            .push(UnifiedRow {
                period: row.period,
                agent: UnifiedAgent::Source(source),
                totals: row.totals,
                models_used: row.models_used,
                agent_breakdowns: Vec::new(),
                model_breakdowns: row.model_breakdowns,
            });
    }

    let rows = aggregate_rows
        .into_iter()
        .map(|row| {
            let mut agent_breakdowns = breakdowns_by_period.remove(&row.period).unwrap_or_default();
            agent_breakdowns.sort_by(|left, right| left.agent.cmp(&right.agent));
            let model_breakdowns = if agent_breakdowns.is_empty() {
                row.model_breakdowns
            } else {
                merge_unified_model_breakdowns(&agent_breakdowns)
            };
            UnifiedRow {
                period: row.period,
                agent: UnifiedAgent::All,
                totals: row.totals,
                models_used: row.models_used,
                agent_breakdowns,
                model_breakdowns,
            }
        })
        .collect();

    UnifiedReport {
        kind,
        rows,
        detected: detected_sources(&detected),
    }
}

fn unified_agent_from_source_id(source: &str) -> UnifiedAgent {
    SourceKind::parse_id(source)
        .map(UnifiedAgent::Source)
        .unwrap_or_else(|| UnifiedAgent::Unknown(source.to_string()))
}

fn detected_sources(sources: &BTreeSet<SourceKind>) -> Vec<SourceKind> {
    registered_source_descriptors()
        .iter()
        .filter(|descriptor| sources.contains(&descriptor.kind))
        .map(|descriptor| descriptor.kind)
        .collect()
}

fn merge_unified_model_breakdowns(rows: &[UnifiedRow]) -> Vec<ModelCostBreakdown> {
    let mut totals_by_model: BTreeMap<String, TokenTotals> = BTreeMap::new();
    for row in rows {
        for breakdown in &row.model_breakdowns {
            let totals = totals_by_model.entry(breakdown.model.clone()).or_default();
            totals.input_tokens += breakdown.input_tokens;
            totals.cache_creation_tokens += breakdown.cache_creation_tokens;
            totals.cache_read_tokens += breakdown.cache_read_tokens;
            totals.output_tokens += breakdown.output_tokens;
            totals.reasoning_output_tokens += breakdown.reasoning_output_tokens;
            totals.total_tokens += breakdown.total_tokens;
            totals.estimated_cost_usd += breakdown.estimated_cost_usd;
        }
    }
    let mut merged = totals_by_model
        .into_iter()
        .map(|(model, totals)| ModelCostBreakdown {
            source: String::new(),
            model,
            input_tokens: totals.input_tokens,
            cache_creation_tokens: totals.cache_creation_tokens,
            cache_read_tokens: totals.cache_read_tokens,
            output_tokens: totals.output_tokens,
            reasoning_output_tokens: totals.reasoning_output_tokens,
            total_tokens: totals.total_tokens,
            estimated_cost_usd: totals.estimated_cost_usd,
        })
        .collect::<Vec<_>>();
    merged.sort_by(|left, right| right.estimated_cost_usd.total_cmp(&left.estimated_cost_usd));
    merged
}

pub fn load_blocks_report(
    store: &Store,
    filter: &ReportFilter,
    options: &BlockReportOptions,
) -> Result<BlocksReport> {
    Ok(load_blocks_report_at(store, filter, options, Utc::now(), true)?.report)
}

fn load_blocks_report_at(
    store: &Store,
    filter: &ReportFilter,
    options: &BlockReportOptions,
    now: DateTime<Utc>,
    allow_bounding: bool,
) -> Result<LoadedBlocksReport> {
    let session_length =
        Duration::milliseconds((options.session_length_hours * 3_600_000.0) as i64);
    let recent_cutoff = now - Duration::days(3);
    let mut scan = BlocksScanStats::default();
    let bounding_eligible = allow_bounding
        && options.recent_only
        && filter.since.is_none()
        && filter.project.is_none()
        && !matches!(options.token_limit, Some(TokenLimit::Max));
    if bounding_eligible {
        let conn = store.open_connection()?;
        let anchor = find_blocks_scan_start(
            &conn,
            filter,
            recent_cutoff,
            session_length,
            &mut scan.anchor_probe_events,
        )?;
        scan.scan_start = anchor;
        scan.fell_back_to_full_scan = scan.scan_start.is_none();
    }
    let mut aggregates: Vec<(DateTime<Utc>, DateTime<Utc>, Aggregate)> = Vec::new();

    visit_filtered_events_from(store, filter, scan.scan_start.as_deref(), |event| {
        scan.scanned_events += 1;
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
        aggregates[last_index].2.add_event(&event);
        Ok(())
    })?;

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
    debug!(
        operation = "blocks_report",
        anchor_probe_events = scan.anchor_probe_events,
        scanned_events = scan.scanned_events,
        scan_start = scan.scan_start.as_deref(),
        fell_back_to_full_scan = scan.fell_back_to_full_scan,
        "loaded usage blocks"
    );
    Ok(LoadedBlocksReport {
        report: BlocksReport { blocks },
        #[cfg(test)]
        scan,
    })
}

const BLOCK_ANCHOR_PAGE_SIZE: usize = 256;

struct ReverseEventCursor {
    source: String,
    cutoff: String,
    until_exclusive: Option<String>,
    before: Option<(String, String)>,
    buffered: VecDeque<(DateTime<Utc>, String, String)>,
    exhausted: bool,
    fetched: usize,
}

impl ReverseEventCursor {
    fn new(source: String, cutoff: String, until_exclusive: Option<String>) -> Self {
        Self {
            source,
            cutoff,
            until_exclusive,
            before: None,
            buffered: VecDeque::new(),
            exhausted: false,
            fetched: 0,
        }
    }

    fn next(&mut self, conn: &Connection) -> Result<Option<(DateTime<Utc>, String, String)>> {
        if self.buffered.is_empty() && !self.exhausted {
            self.load_page(conn)?;
        }
        Ok(self.buffered.pop_front())
    }

    fn load_page(&mut self, conn: &Connection) -> Result<()> {
        let mut clauses = vec!["source = ?".to_string(), "event_at <= ?".to_string()];
        let mut params: Vec<Box<dyn rusqlite::ToSql>> =
            vec![Box::new(self.source.clone()), Box::new(self.cutoff.clone())];
        if let Some(until_exclusive) = &self.until_exclusive {
            clauses.push("event_at < ?".to_string());
            params.push(Box::new(until_exclusive.clone()));
        }
        if let Some((event_at, event_key)) = &self.before {
            clauses.push("(event_at < ? OR (event_at = ? AND event_key < ?))".to_string());
            params.push(Box::new(event_at.clone()));
            params.push(Box::new(event_at.clone()));
            params.push(Box::new(event_key.clone()));
        }
        params.push(Box::new(BLOCK_ANCHOR_PAGE_SIZE as i64));
        let sql = format!(
            r#"
            SELECT event_at, event_key
            FROM usage_event INDEXED BY idx_usage_event_source_event_at
            WHERE {}
            ORDER BY event_at DESC, event_key DESC
            LIMIT ?
            "#,
            clauses.join(" AND ")
        );
        let param_refs = params
            .iter()
            .map(|value| value.as_ref())
            .collect::<Vec<&dyn rusqlite::ToSql>>();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let raw_event_at: String = row.get(0)?;
            let event_at = DateTime::parse_from_rfc3339(&raw_event_at)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?;
            Ok((event_at, raw_event_at, row.get::<_, String>(1)?))
        })?;
        let page = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        self.fetched += page.len();
        self.exhausted = page.len() < BLOCK_ANCHOR_PAGE_SIZE;
        if let Some((_, raw_event_at, event_key)) = page.last() {
            self.before = Some((raw_event_at.clone(), event_key.clone()));
        }
        self.buffered.extend(page);
        Ok(())
    }
}

fn find_blocks_scan_start(
    conn: &Connection,
    filter: &ReportFilter,
    cutoff: DateTime<Utc>,
    session_length: Duration,
    probe_events: &mut usize,
) -> Result<Option<String>> {
    let cutoff = cutoff.to_rfc3339_opts(SecondsFormat::AutoSi, true);
    let until_exclusive = filter
        .until
        .and_then(|date| date.succ_opt())
        .map(|date| local_date_to_utc_start(date, &filter.timezone));
    let sources = filter
        .source
        .map(|source| vec![source.as_str().to_string()])
        .unwrap_or_else(|| {
            registered_source_descriptors()
                .iter()
                .map(|descriptor| descriptor.stable_id.to_string())
                .collect()
        });
    let mut cursors = sources
        .into_iter()
        .map(|source| ReverseEventCursor::new(source, cutoff.clone(), until_exclusive.clone()))
        .collect::<Vec<_>>();
    let mut events = BinaryHeap::new();
    for (index, cursor) in cursors.iter_mut().enumerate() {
        if let Some((event_at, raw_event_at, event_key)) = cursor.next(conn)? {
            events.push((event_at, event_key, raw_event_at, index));
        }
    }

    let mut newer: Option<(DateTime<Utc>, String)> = None;
    let mut anchor = None;
    while let Some((event_at, _event_key, raw_event_at, cursor_index)) = events.pop() {
        if let Some((newer_at, newer_raw_at)) = &newer
            && *newer_at - event_at >= session_length
        {
            anchor = Some(newer_raw_at.clone());
            break;
        }
        newer = Some((event_at, raw_event_at));
        if let Some((next_at, next_raw_at, next_key)) = cursors[cursor_index].next(conn)? {
            events.push((next_at, next_key, next_raw_at, cursor_index));
        }
    }
    *probe_events = cursors.iter().map(|cursor| cursor.fetched).sum();
    Ok(anchor)
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

fn add_totals_from_bucket(totals: &mut TokenTotals, bucket: &BucketRow) {
    add_tokens(
        totals,
        TokenComponents::from(bucket),
        bucket.cost_with_cache_usd,
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

fn load_filtered_buckets(store: &Store, filter: &ReportFilter) -> Result<Vec<BucketRow>> {
    let conn = store.open_connection()?;
    load_buckets_filtered(&conn, filter)
}

fn load_filtered_project_buckets(
    store: &Store,
    filter: &ReportFilter,
) -> Result<Vec<ProjectBucketRow>> {
    let conn = store.open_connection()?;
    load_project_buckets_filtered(&conn, filter)
}

fn load_buckets_filtered(conn: &Connection, filter: &ReportFilter) -> Result<Vec<BucketRow>> {
    let mut clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    push_bucket_filter(filter, &mut clauses, &mut params);

    let where_clause = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let local_date_expr = bucket_local_date_expr("hour_start", &filter.timezone);
    let sql = format!(
        r#"
        SELECT
            source,
            model,
            {local_date_expr} AS local_date,
            SUM(input_tokens),
            SUM(cache_creation_tokens),
            SUM(cache_read_tokens),
            SUM(output_tokens),
            SUM(reasoning_output_tokens),
            SUM(total_tokens),
            SUM(cost_with_cache_usd),
            COALESCE(pricing_status, 'unpriced') AS pricing_status
        FROM usage_bucket_30m
        {where_clause}
        GROUP BY source, model, local_date, COALESCE(pricing_status, 'unpriced')
        ORDER BY local_date ASC, source ASC, model ASC, pricing_status ASC
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let param_refs = params
        .iter()
        .map(|value| value.as_ref())
        .collect::<Vec<&dyn rusqlite::ToSql>>();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(BucketRow {
            source: row.get(0)?,
            model: row.get(1)?,
            local_date: parse_sql_local_date(row.get(2)?, 2)?,
            input_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
            cache_creation_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
            cache_read_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
            output_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
            reasoning_output_tokens: row.get::<_, Option<i64>>(7)?.unwrap_or_default(),
            total_tokens: row.get::<_, Option<i64>>(8)?.unwrap_or_default(),
            cost_with_cache_usd: row.get::<_, Option<f64>>(9)?.unwrap_or_default(),
            pricing_status: row
                .get::<_, Option<String>>(10)?
                .unwrap_or_else(|| crate::query::pricing::PRICING_UNPRICED.to_string()),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_project_buckets_filtered(
    conn: &Connection,
    filter: &ReportFilter,
) -> Result<Vec<ProjectBucketRow>> {
    let mut clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    push_bucket_filter(filter, &mut clauses, &mut params);

    let where_clause = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let local_date_expr = bucket_local_date_expr("hour_start", &filter.timezone);
    let sql = format!(
        r#"
        SELECT
            source,
            model,
            {local_date_expr} AS local_date,
            project_hash,
            project_label,
            project_ref,
            SUM(input_tokens),
            SUM(cache_creation_tokens),
            SUM(cache_read_tokens),
            SUM(output_tokens),
            SUM(reasoning_output_tokens),
            SUM(total_tokens),
            SUM(cost_with_cache_usd),
            COALESCE(pricing_status, 'unpriced') AS pricing_status
        FROM usage_bucket_30m
        {where_clause}
        GROUP BY
            source,
            model,
            local_date,
            COALESCE(project_hash, ''),
            project_label,
            project_ref,
            COALESCE(pricing_status, 'unpriced')
        ORDER BY local_date ASC, project_hash ASC, source ASC, model ASC, pricing_status ASC
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let param_refs = params
        .iter()
        .map(|value| value.as_ref())
        .collect::<Vec<&dyn rusqlite::ToSql>>();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(ProjectBucketRow {
            bucket: BucketRow {
                source: row.get(0)?,
                model: row.get(1)?,
                local_date: parse_sql_local_date(row.get(2)?, 2)?,
                input_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                cache_creation_tokens: row.get::<_, Option<i64>>(7)?.unwrap_or_default(),
                cache_read_tokens: row.get::<_, Option<i64>>(8)?.unwrap_or_default(),
                output_tokens: row.get::<_, Option<i64>>(9)?.unwrap_or_default(),
                reasoning_output_tokens: row.get::<_, Option<i64>>(10)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(11)?.unwrap_or_default(),
                cost_with_cache_usd: row.get::<_, Option<f64>>(12)?.unwrap_or_default(),
                pricing_status: row
                    .get::<_, Option<String>>(13)?
                    .unwrap_or_else(|| crate::query::pricing::PRICING_UNPRICED.to_string()),
            },
            project: normalize_project(row.get(3)?, row.get(4)?, row.get(5)?),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn push_bucket_filter(
    filter: &ReportFilter,
    clauses: &mut Vec<String>,
    params: &mut Vec<Box<dyn rusqlite::ToSql>>,
) {
    if let Some(source) = filter.source {
        clauses.push("source = ?".to_string());
        params.push(Box::new(source.as_str().to_string()));
    }
    if let Some(since) = filter.since {
        let utc_start = local_date_to_utc_start(since, &filter.timezone);
        clauses.push("hour_start >= ?".to_string());
        params.push(Box::new(utc_start));
    }
    if let Some(until) = filter.until
        && let Some(exclusive) = until.succ_opt()
    {
        let utc_end = local_date_to_utc_start(exclusive, &filter.timezone);
        clauses.push("hour_start < ?".to_string());
        params.push(Box::new(utc_end));
    }
}

fn bucket_local_date_expr(column: &str, timezone: &ReportTimezone) -> String {
    let seconds = fixed_offset_for(timezone).local_minus_utc();
    if seconds == 0 {
        format!("date({column})")
    } else if seconds > 0 {
        format!("date({column}, '+{seconds} seconds')")
    } else {
        format!("date({column}, '{seconds} seconds')")
    }
}

fn parse_sql_local_date(value: String, column: usize) -> rusqlite::Result<NaiveDate> {
    NaiveDate::parse_from_str(&value, "%Y-%m-%d").map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(err),
        )
    })
}

fn visit_filtered_events<F>(store: &Store, filter: &ReportFilter, visitor: F) -> Result<()>
where
    F: FnMut(EventRow) -> Result<()>,
{
    visit_filtered_events_from(store, filter, None, visitor)
}

fn visit_filtered_events_from<F>(
    store: &Store,
    filter: &ReportFilter,
    exact_since: Option<&str>,
    mut visitor: F,
) -> Result<()>
where
    F: FnMut(EventRow) -> Result<()>,
{
    let conn = store.open_connection()?;
    visit_events_filtered(&conn, filter, exact_since, |event| {
        if filter_event_post_sql(&event, filter) {
            visitor(event)?;
        }
        Ok(())
    })
}

/// Visits events with date/source filters pushed down to SQL for performance.
/// Project filtering remains in Rust because it requires fuzzy matching.
fn visit_events_filtered<F>(
    conn: &Connection,
    filter: &ReportFilter,
    exact_since: Option<&str>,
    mut visitor: F,
) -> Result<()>
where
    F: FnMut(EventRow) -> Result<()>,
{
    let mut clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(source) = filter.source {
        clauses.push("source = ?".to_string());
        params.push(Box::new(source.as_str().to_string()));
    } else if exact_since.is_some() {
        let sources = registered_source_descriptors();
        clauses.push(format!(
            "source IN ({})",
            std::iter::repeat_n("?", sources.len())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        params.extend(sources.iter().map(|descriptor| {
            Box::new(descriptor.stable_id.to_string()) as Box<dyn rusqlite::ToSql>
        }));
    }
    if let Some(since) = filter.since {
        // Convert local date start to UTC for SQL comparison
        let utc_start = local_date_to_utc_start(since, &filter.timezone);
        clauses.push("event_at >= ?".to_string());
        params.push(Box::new(utc_start));
    }
    if let Some(exact_since) = exact_since {
        clauses.push("event_at >= ?".to_string());
        params.push(Box::new(exact_since.to_string()));
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
            total_tokens,
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
    let mut rows = stmt.query(param_refs.as_slice())?;
    while let Some(row) = rows.next()? {
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
        let raw = RawEventRow {
            event_key: row.get(0)?,
            source: row.get(1)?,
            model: row.get(2)?,
            event_utc,
            input_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
            cache_creation_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
            cache_read_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
            output_tokens: row.get::<_, Option<i64>>(7)?.unwrap_or_default(),
            reasoning_output_tokens: row.get::<_, Option<i64>>(8)?.unwrap_or_default(),
            total_tokens: row.get::<_, Option<i64>>(9)?.unwrap_or_default(),
            cost_with_cache_usd: row.get::<_, Option<f64>>(10)?.unwrap_or_default(),
            pricing_status: row
                .get::<_, Option<String>>(11)?
                .unwrap_or_else(|| crate::query::pricing::PRICING_UNPRICED.to_string()),
            project_hash: row.get(12)?,
            project_label: row.get(13)?,
            project_ref: row.get(14)?,
            session_id: row.get(15)?,
            session_label: row.get(16)?,
            source_path_hash: row.get(17)?,
        };
        visitor(raw.with_timezone(&filter.timezone))?;
    }
    Ok(())
}

/// Converts a local NaiveDate midnight to a UTC RFC 3339 string for SQL filtering.
fn local_date_to_utc_start(date: NaiveDate, timezone: &ReportTimezone) -> String {
    use chrono::{SecondsFormat, TimeZone, offset::LocalResult};
    let local_start = date.and_hms_opt(0, 0, 0).expect("midnight is always valid");
    let offset = fixed_offset_for(timezone);
    let utc = match offset.from_local_datetime(&local_start) {
        LocalResult::Single(value) => value.with_timezone(&Utc),
        LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
        LocalResult::None => offset.from_utc_datetime(&local_start).with_timezone(&Utc),
    };
    utc.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn fixed_offset_for(timezone: &ReportTimezone) -> FixedOffset {
    match timezone {
        ReportTimezone::Utc => Utc.fix(),
        ReportTimezone::Local => Local::now().offset().fix(),
        ReportTimezone::Fixed(offset) => *offset,
    }
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
    total_tokens: i64,
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
            total_tokens: self.total_tokens,
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
    value.with_timezone(&fixed_offset_for(timezone))
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use anyhow::Result;
    use rusqlite::params;
    use tempfile::TempDir;

    use super::*;
    use crate::{paths::AppPaths, store::Store};

    fn at(rfc3339: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(rfc3339).unwrap()
    }

    #[test]
    fn session_time_span_caps_idle_gaps() {
        // 0m, +10m, +50m (40m gap dropped), +55m (5m kept).
        let mut times = vec![
            at("2026-05-01T10:00:00Z"),
            at("2026-05-01T10:10:00Z"),
            at("2026-05-01T10:50:00Z"),
            at("2026-05-01T10:55:00Z"),
        ];
        let (span, active) = session_time_span(&mut times);
        assert_eq!(span, 55);
        // 10 (<=30) + 40 (dropped) + 5 (<=30) = 15 active.
        assert_eq!(active, 15);
    }

    #[test]
    fn session_time_span_handles_single_and_empty() {
        assert_eq!(session_time_span(&mut []), (0, 0));
        assert_eq!(session_time_span(&mut [at("2026-05-01T10:00:00Z")]), (0, 0));
    }

    #[test]
    fn session_time_span_sorts_unordered_input() {
        let mut times = vec![
            at("2026-05-01T10:20:00Z"),
            at("2026-05-01T10:00:00Z"),
            at("2026-05-01T10:05:00Z"),
        ];
        let (span, active) = session_time_span(&mut times);
        assert_eq!(span, 20);
        // 5m + 15m, both <= 30 → 20 active.
        assert_eq!(active, 20);
    }

    fn blocks_filter() -> ReportFilter {
        ReportFilter {
            since: None,
            until: None,
            order: SortOrder::Asc,
            timezone: ReportTimezone::Utc,
            locale: "en-US".to_string(),
            source: None,
            project: None,
            breakdown: false,
        }
    }

    fn recent_blocks_options() -> BlockReportOptions {
        BlockReportOptions {
            active_only: false,
            recent_only: true,
            token_limit: None,
            session_length_hours: 5.0,
        }
    }

    fn insert_block_event(
        fixture: &ReportFixture,
        event_key: &str,
        event_at: &str,
        tokens: i64,
    ) -> Result<()> {
        fixture.insert_event(SeedEvent {
            event_key,
            source: "codex",
            model: "gpt-5",
            event_at,
            total_tokens: tokens,
            project_hash: "blocks",
            project_label: "Blocks",
            session_id: "blocks-session",
        })
    }

    #[test]
    fn bounded_blocks_match_full_scan_for_a_cross_cutoff_block() -> Result<()> {
        let fixture = ReportFixture::new()?;
        for (key, event_at) in [
            ("old", "2026-05-01T00:00:00Z"),
            ("anchor", "2026-05-07T08:10:00Z"),
            ("before-cutoff", "2026-05-07T11:50:00Z"),
            ("after-cutoff-same-block", "2026-05-07T12:20:00Z"),
            ("recent-block", "2026-05-07T13:05:00Z"),
        ] {
            insert_block_event(&fixture, key, event_at, 10)?;
        }
        let now = DateTime::parse_from_rfc3339("2026-05-10T12:00:00Z")?.with_timezone(&Utc);
        let filter = blocks_filter();
        let options = recent_blocks_options();

        let bounded = load_blocks_report_at(&fixture.store, &filter, &options, now, true)?;
        let full = load_blocks_report_at(&fixture.store, &filter, &options, now, false)?;

        assert_eq!(bounded.report.blocks, full.report.blocks);
        assert_eq!(bounded.report.blocks.len(), 1);
        assert!(
            bounded.report.blocks[0]
                .start_at
                .starts_with("2026-05-07T13:00:00")
        );
        Ok(())
    }

    #[test]
    fn bounded_blocks_reanchor_after_the_latest_qualifying_gap() -> Result<()> {
        let fixture = ReportFixture::new()?;
        for (key, event_at) in [
            ("old-a", "2026-05-01T00:00:00Z"),
            ("old-b", "2026-05-01T01:00:00Z"),
            ("anchor", "2026-05-07T08:10:00Z"),
            ("recent-a", "2026-05-07T13:05:00Z"),
            ("recent-b", "2026-05-08T09:00:00Z"),
        ] {
            insert_block_event(&fixture, key, event_at, 10)?;
        }
        let now = DateTime::parse_from_rfc3339("2026-05-10T12:00:00Z")?.with_timezone(&Utc);

        let report = load_blocks_report_at(
            &fixture.store,
            &blocks_filter(),
            &recent_blocks_options(),
            now,
            true,
        )?;

        assert_eq!(
            report.scan.scan_start.as_deref(),
            Some("2026-05-07T08:10:00Z")
        );
        assert!(!report.scan.fell_back_to_full_scan);
        assert_eq!(report.scan.scanned_events, 3);
        Ok(())
    }

    #[test]
    fn bounded_blocks_preserve_active_block_detection() -> Result<()> {
        let fixture = ReportFixture::new()?;
        for (key, event_at) in [
            ("old", "2026-05-01T00:00:00Z"),
            ("anchor", "2026-05-07T08:10:00Z"),
            ("active", "2026-05-10T10:15:00Z"),
        ] {
            insert_block_event(&fixture, key, event_at, 10)?;
        }
        let now = DateTime::parse_from_rfc3339("2026-05-10T12:00:00Z")?.with_timezone(&Utc);

        let report = load_blocks_report_at(
            &fixture.store,
            &blocks_filter(),
            &recent_blocks_options(),
            now,
            true,
        )?;

        let active = report
            .report
            .blocks
            .iter()
            .find(|block| block.is_active)
            .unwrap();
        assert!(active.start_at.starts_with("2026-05-10T10:00:00"));
        Ok(())
    }

    #[test]
    fn bounded_blocks_fall_back_when_continuous_history_has_no_gap() -> Result<()> {
        let fixture = ReportFixture::new()?;
        let start = DateTime::parse_from_rfc3339("2026-05-01T00:00:00Z")?.with_timezone(&Utc);
        let cutoff = DateTime::parse_from_rfc3339("2026-05-07T12:00:00Z")?.with_timezone(&Utc);
        let mut event_at = start;
        let mut inserted = 0usize;
        while event_at <= cutoff {
            let key = format!("event-{inserted}");
            let timestamp = event_at.to_rfc3339_opts(SecondsFormat::Secs, true);
            insert_block_event(&fixture, &key, &timestamp, 1)?;
            inserted += 1;
            event_at += Duration::hours(4);
        }
        let now = DateTime::parse_from_rfc3339("2026-05-10T12:00:00Z")?.with_timezone(&Utc);

        let report = load_blocks_report_at(
            &fixture.store,
            &blocks_filter(),
            &recent_blocks_options(),
            now,
            true,
        )?;

        assert!(report.scan.fell_back_to_full_scan);
        assert_eq!(report.scan.scan_start, None);
        assert_eq!(report.scan.scanned_events, inserted);
        Ok(())
    }

    #[ignore = "reads the local usage database for release-mode performance evidence"]
    #[test]
    fn measure_local_blocks_bounding() -> Result<()> {
        let paths = AppPaths::discover()?;
        let database_bytes = std::fs::metadata(&paths.db_path)?.len();
        let store = Store::new(&paths)?;
        let filter = blocks_filter();
        let options = recent_blocks_options();
        let now = Utc::now();

        let _ = load_blocks_report_at(&store, &filter, &options, now, true)?;
        let mut full_ms = Vec::new();
        let mut bounded_ms = Vec::new();
        let mut last_bounded = None;
        let mut full_scanned_events = 0;
        for _ in 0..3 {
            let started = Instant::now();
            let full = load_blocks_report_at(&store, &filter, &options, now, false)?;
            full_ms.push(started.elapsed().as_secs_f64() * 1_000.0);

            let started = Instant::now();
            let bounded = load_blocks_report_at(&store, &filter, &options, now, true)?;
            bounded_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
            assert_eq!(bounded.report.blocks, full.report.blocks);
            full_scanned_events = full.scan.scanned_events;
            last_bounded = Some(bounded);
        }
        full_ms.sort_by(f64::total_cmp);
        bounded_ms.sort_by(f64::total_cmp);
        let full_median = full_ms[full_ms.len() / 2];
        let bounded_median = bounded_ms[bounded_ms.len() / 2];
        let bounded = last_bounded.unwrap();

        let conn = store.open_connection()?;
        let cutoff = (now - Duration::days(3)).to_rfc3339_opts(SecondsFormat::AutoSi, true);
        let anchor_plan = query_plan(
            &conn,
            "SELECT event_at, event_key FROM usage_event INDEXED BY idx_usage_event_source_event_at WHERE source = ?1 AND event_at <= ?2 ORDER BY event_at DESC, event_key DESC LIMIT 256",
            &["codex", &cutoff],
        )?;
        let scan_plan = bounded
            .scan
            .scan_start
            .as_deref()
            .map(|scan_start| {
                query_plan(
                    &conn,
                    "SELECT event_at FROM usage_event WHERE source IN ('codex', 'claude', 'opencode', 'antigravity') AND event_at >= ?1 ORDER BY event_at ASC, event_key ASC",
                    &[scan_start],
                )
            })
            .transpose()?
            .unwrap_or_default();
        eprintln!(
            "database_bytes={database_bytes} full_ms={full_ms:?} bounded_ms={bounded_ms:?} improvement_pct={:.1} full_scanned_events={full_scanned_events} anchor_probe_events={} bounded_scanned_events={} scan_start={:?} fallback={} anchor_plan={anchor_plan:?} scan_plan={scan_plan:?}",
            (full_median - bounded_median) * 100.0 / full_median,
            bounded.scan.anchor_probe_events,
            bounded.scan.scanned_events,
            bounded.scan.scan_start,
            bounded.scan.fell_back_to_full_scan,
        );
        Ok(())
    }

    fn query_plan(conn: &Connection, sql: &str, params: &[&str]) -> Result<Vec<String>> {
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| row.get(3))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

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
    fn daily_monthly_reports_read_bucket_rows_without_events() -> Result<()> {
        let fixture = ReportFixture::new()?;
        fixture.insert_bucket(SeedBucket {
            source: "codex",
            model: "gpt-5",
            hour_start: "2026-05-04T16:30:00Z",
            project_hash: "project-a",
            project_label: "Project A",
            project_ref: Some("example/project-a"),
            input_tokens: 5,
            cache_creation_tokens: 2,
            cache_read_tokens: 1,
            output_tokens: 3,
            reasoning_output_tokens: 4,
            total_tokens: 999,
            cost_with_cache_usd: 1.25,
            pricing_status: "static",
        })?;
        let conn = fixture.store.open_connection()?;
        let event_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
        assert_eq!(event_count, 0, "test must prove bucket read model");
        drop(conn);

        let filter = ReportFilter {
            since: Some(NaiveDate::from_ymd_opt(2026, 5, 5).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 5, 5).unwrap()),
            order: SortOrder::Asc,
            timezone: ReportTimezone::Fixed(FixedOffset::east_opt(8 * 3600).unwrap()),
            locale: "en-US".to_string(),
            source: None,
            project: None,
            breakdown: true,
        };
        let daily = load_daily_report(&fixture.store, &filter)?;
        assert_eq!(daily.daily.len(), 1);
        assert_eq!(daily.daily[0].date, "2026-05-05");
        assert_eq!(daily.daily[0].totals.total_tokens, 999);
        assert_eq!(daily.daily[0].model_breakdowns.len(), 1);
        assert_eq!(daily.totals.estimated_cost_usd, 1.25);

        let monthly = load_monthly_report(&fixture.store, &filter)?;
        assert_eq!(monthly.monthly.len(), 1);
        assert_eq!(monthly.monthly[0].month, "2026-05");
        assert_eq!(monthly.totals.total_tokens, 999);

        let by_source = load_daily_reports_by_source(&fixture.store, &filter)?;
        assert_eq!(by_source.len(), 1);
        assert_eq!(by_source[0].0, SourceKind::Codex);
        assert_eq!(by_source[0].1.totals.total_tokens, 999);

        let projects = load_daily_project_report(&fixture.store, &filter)?;
        let project_rows = projects
            .projects
            .get("example/project-a")
            .expect("bucket project row should be grouped by project_ref");
        assert_eq!(project_rows.len(), 1);
        assert_eq!(project_rows[0].totals.total_tokens, 999);
        Ok(())
    }

    #[test]
    fn daily_monthly_reports_ignore_large_event_backlog_without_project_filter() -> Result<()> {
        let fixture = ReportFixture::new()?;
        fixture.insert_event_backlog("2026-05-04T16:30:00Z", 100_000)?;
        fixture.insert_bucket(SeedBucket {
            source: "codex",
            model: "gpt-5",
            hour_start: "2026-05-04T16:30:00Z",
            project_hash: "project-a",
            project_label: "Project A",
            project_ref: Some("example/project-a"),
            input_tokens: 5,
            cache_creation_tokens: 2,
            cache_read_tokens: 1,
            output_tokens: 3,
            reasoning_output_tokens: 4,
            total_tokens: 15,
            cost_with_cache_usd: 1.25,
            pricing_status: "static",
        })?;

        let filter = ReportFilter {
            since: Some(NaiveDate::from_ymd_opt(2026, 5, 5).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 5, 5).unwrap()),
            order: SortOrder::Asc,
            timezone: ReportTimezone::Fixed(FixedOffset::east_opt(8 * 3600).unwrap()),
            locale: "en-US".to_string(),
            source: None,
            project: None,
            breakdown: false,
        };

        let daily = load_daily_report(&fixture.store, &filter)?;
        assert_eq!(daily.totals.total_tokens, 15);
        let monthly = load_monthly_report(&fixture.store, &filter)?;
        assert_eq!(monthly.totals.total_tokens, 15);

        let project_filter = ReportFilter {
            project: Some("Backlog".to_string()),
            ..filter
        };
        let project_daily = load_daily_report(&fixture.store, &project_filter)?;
        assert_eq!(
            project_daily.totals.total_tokens, 100_000,
            "project fuzzy filters must retain event-detail semantics"
        );
        Ok(())
    }

    #[test]
    fn daily_report_fixed_offset_filter_includes_exact_local_day_from_buckets() -> Result<()> {
        let fixture = ReportFixture::new()?;
        for (hour_start, project_hash) in [
            ("2026-03-07T15:30:00Z", "outside-before"),
            ("2026-03-07T16:00:00Z", "inside-start"),
            ("2026-03-08T15:30:00Z", "inside-end"),
            ("2026-03-08T16:00:00Z", "outside-after"),
        ] {
            fixture.insert_bucket(SeedBucket {
                source: "codex",
                model: "gpt-5",
                hour_start,
                project_hash,
                project_label: project_hash,
                project_ref: None,
                input_tokens: 5,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: 5,
                cost_with_cache_usd: 0.0,
                pricing_status: "static",
            })?;
        }

        let report = load_daily_report(
            &fixture.store,
            &ReportFilter {
                since: Some(NaiveDate::from_ymd_opt(2026, 3, 8).unwrap()),
                until: Some(NaiveDate::from_ymd_opt(2026, 3, 8).unwrap()),
                order: SortOrder::Asc,
                timezone: ReportTimezone::Fixed(FixedOffset::east_opt(8 * 3600).unwrap()),
                locale: "en-US".to_string(),
                source: None,
                project: None,
                breakdown: false,
            },
        )?;

        assert_eq!(report.daily.len(), 1);
        assert_eq!(report.daily[0].date, "2026-03-08");
        assert_eq!(report.daily[0].totals.total_tokens, 10);
        Ok(())
    }

    #[test]
    fn local_report_timezone_uses_current_fixed_offset_snapshot() {
        let current_offset = Local::now().offset().fix();
        let historical = DateTime::parse_from_rfc3339("2026-11-01T08:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let local = apply_timezone(historical, &ReportTimezone::Local);

        assert_eq!(
            *local.offset(),
            current_offset,
            "`local` report rendering must use the same current fixed offset as SQL grouping"
        );
    }

    #[test]
    fn unified_daily_rows_preserve_all_agent_invariants_and_internal_payloads() -> Result<()> {
        let fixture = ReportFixture::new()?;
        fixture.insert_bucket(SeedBucket {
            source: "codex",
            model: "gpt-5",
            hour_start: "2026-05-05T10:00:00Z",
            project_hash: "codex-project",
            project_label: "Codex Project",
            project_ref: None,
            input_tokens: 10,
            cache_creation_tokens: 2,
            cache_read_tokens: 3,
            output_tokens: 4,
            reasoning_output_tokens: 0,
            total_tokens: 19,
            cost_with_cache_usd: 0.10,
            pricing_status: "static",
        })?;
        fixture.insert_bucket(SeedBucket {
            source: "claude",
            model: "claude-sonnet-4",
            hour_start: "2026-05-05T11:00:00Z",
            project_hash: "claude-project",
            project_label: "Claude Project",
            project_ref: None,
            input_tokens: 20,
            cache_creation_tokens: 5,
            cache_read_tokens: 6,
            output_tokens: 7,
            reasoning_output_tokens: 0,
            total_tokens: 38,
            cost_with_cache_usd: 0.20,
            pricing_status: "static",
        })?;
        let filter = ReportFilter {
            since: Some(NaiveDate::from_ymd_opt(2026, 5, 5).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 5, 5).unwrap()),
            order: SortOrder::Asc,
            timezone: ReportTimezone::Utc,
            locale: "en-US".to_string(),
            source: None,
            project: None,
            breakdown: true,
        };

        let report = load_unified_report(&fixture.store, &filter, PeriodKind::Daily)?;
        assert_eq!(report.detected, vec![SourceKind::Codex, SourceKind::Claude]);
        assert_eq!(report.rows.len(), 1);
        let all = &report.rows[0];
        assert_eq!(all.agent, UnifiedAgent::All);
        assert_eq!(all.agent_breakdowns.len(), 2);
        assert_eq!(all.agent_breakdowns[0].agent.id(), "codex");
        assert_eq!(all.agent_breakdowns[1].agent.id(), "claude");
        assert_eq!(
            all.agent_breakdowns
                .iter()
                .map(|row| row.totals.input_tokens)
                .sum::<i64>(),
            all.totals.input_tokens
        );
        assert_eq!(
            all.agent_breakdowns
                .iter()
                .map(|row| row.totals.total_tokens)
                .sum::<i64>(),
            all.totals.total_tokens
        );
        let source_cost = all
            .agent_breakdowns
            .iter()
            .map(|row| row.totals.estimated_cost_usd)
            .sum::<f64>();
        assert!((source_cost - all.totals.estimated_cost_usd).abs() <= 1e-9);

        let internal = serde_json::to_value(load_daily_report(&fixture.store, &filter)?)?;
        assert!(internal["daily"][0].get("cache_creation_tokens").is_some());
        assert!(internal["daily"][0].get("cacheCreationTokens").is_none());
        Ok(())
    }

    #[test]
    fn weekly_reports_use_monday_start_across_years_and_match_daily_totals() -> Result<()> {
        let fixture = ReportFixture::new()?;
        for (source, model, hour_start, tokens, cost) in [
            ("codex", "gpt-5", "2025-12-29T10:00:00Z", 10, 0.10),
            (
                "claude",
                "claude-sonnet-4",
                "2026-01-04T10:00:00Z",
                20,
                0.20,
            ),
            ("codex", "gpt-5", "2026-01-05T10:00:00Z", 30, 0.30),
        ] {
            fixture.insert_bucket(SeedBucket {
                source,
                model,
                hour_start,
                project_hash: hour_start,
                project_label: hour_start,
                project_ref: None,
                input_tokens: tokens,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: tokens,
                cost_with_cache_usd: cost,
                pricing_status: "static",
            })?;
        }
        let filter = ReportFilter {
            since: Some(NaiveDate::from_ymd_opt(2025, 12, 29).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()),
            order: SortOrder::Asc,
            timezone: ReportTimezone::Utc,
            locale: "en-US".to_string(),
            source: None,
            project: None,
            breakdown: true,
        };

        let weekly = load_weekly_report(&fixture.store, &filter)?;
        assert_eq!(weekly.weekly.len(), 2);
        assert_eq!(weekly.weekly[0].week, "2025-12-29");
        assert_eq!(weekly.weekly[1].week, "2026-01-05");
        assert!(!weekly.weekly[0].week.contains('W'));

        let unified = load_unified_report(&fixture.store, &filter, PeriodKind::Weekly)?;
        assert_eq!(unified.rows[0].agent, UnifiedAgent::All);
        assert_eq!(unified.rows[0].agent_breakdowns.len(), 2);
        assert_eq!(unified.totals().total_tokens, 60);
        assert_eq!(
            unified.rows[0]
                .agent_breakdowns
                .iter()
                .map(|row| row.totals.total_tokens)
                .sum::<i64>(),
            unified.rows[0].totals.total_tokens
        );
        assert_eq!(
            unified.totals().total_tokens,
            load_unified_report(&fixture.store, &filter, PeriodKind::Daily)?
                .totals()
                .total_tokens
        );
        Ok(())
    }

    #[test]
    fn weekly_period_key_uses_the_filtered_local_date() -> Result<()> {
        let fixture = ReportFixture::new()?;
        fixture.insert_bucket(SeedBucket {
            source: "codex",
            model: "gpt-5",
            hour_start: "2026-01-04T16:30:00Z",
            project_hash: "timezone-week",
            project_label: "Timezone Week",
            project_ref: None,
            input_tokens: 1,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 1,
            cost_with_cache_usd: 0.0,
            pricing_status: "static",
        })?;
        let utc_filter = ReportFilter {
            since: Some(NaiveDate::from_ymd_opt(2026, 1, 4).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 1, 4).unwrap()),
            order: SortOrder::Asc,
            timezone: ReportTimezone::Utc,
            locale: "en-US".to_string(),
            source: None,
            project: None,
            breakdown: false,
        };
        let local_filter = ReportFilter {
            since: Some(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()),
            timezone: ReportTimezone::Fixed(FixedOffset::east_opt(8 * 3600).unwrap()),
            ..utc_filter.clone()
        };

        assert_eq!(
            load_weekly_report(&fixture.store, &utc_filter)?.weekly[0].week,
            "2025-12-29"
        );
        assert_eq!(
            load_weekly_report(&fixture.store, &local_filter)?.weekly[0].week,
            "2026-01-05"
        );
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

        fn insert_event_backlog(&self, event_at: &str, count: i64) -> Result<()> {
            let conn = self.store.open_connection()?;
            conn.execute(
                r#"
                WITH digits(d) AS (
                    VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9)
                ),
                seq(n) AS (
                    SELECT d0.d
                         + d1.d * 10
                         + d2.d * 100
                         + d3.d * 1000
                         + d4.d * 10000
                         + 1
                    FROM digits d0
                    CROSS JOIN digits d1
                    CROSS JOIN digits d2
                    CROSS JOIN digits d3
                    CROSS JOIN digits d4
                )
                INSERT INTO usage_event(
                    event_key, source, model, event_at, hour_start,
                    input_tokens, cache_read_tokens, output_tokens, reasoning_output_tokens, total_tokens,
                    pricing_status,
                    project_hash, project_label, project_ref, path_hash, session_id, session_label, source_path_hash, created_at
                )
                SELECT
                    'codex:backlog:' || n,
                    'codex',
                    'gpt-5',
                    ?1,
                    ?1,
                    1,
                    0,
                    0,
                    0,
                    1,
                    'unpriced',
                    'project-backlog',
                    'Backlog',
                    NULL,
                    'backlog-path',
                    'backlog-session',
                    NULL,
                    'backlog-source',
                    ?1
                FROM seq
                WHERE n <= ?2
                "#,
                params![event_at, count],
            )?;
            Ok(())
        }

        fn insert_bucket(&self, bucket: SeedBucket<'_>) -> Result<()> {
            let conn = self.store.open_connection()?;
            conn.execute(
                r#"
                INSERT INTO usage_bucket_30m(
                    source, model, hour_start, project_hash, project_label, project_ref,
                    input_tokens, cache_read_tokens, cache_creation_tokens,
                    output_tokens, reasoning_output_tokens, total_tokens,
                    cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                    event_count, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13, ?14, NULL, 1, ?3)
                "#,
                params![
                    bucket.source,
                    bucket.model,
                    bucket.hour_start,
                    bucket.project_hash,
                    bucket.project_label,
                    bucket.project_ref,
                    bucket.input_tokens,
                    bucket.cache_read_tokens,
                    bucket.cache_creation_tokens,
                    bucket.output_tokens,
                    bucket.reasoning_output_tokens,
                    bucket.total_tokens,
                    bucket.cost_with_cache_usd,
                    bucket.pricing_status,
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

    struct SeedBucket<'a> {
        source: &'a str,
        model: &'a str,
        hour_start: &'a str,
        project_hash: &'a str,
        project_label: &'a str,
        project_ref: Option<&'a str>,
        input_tokens: i64,
        cache_creation_tokens: i64,
        cache_read_tokens: i64,
        output_tokens: i64,
        reasoning_output_tokens: i64,
        total_tokens: i64,
        cost_with_cache_usd: f64,
        pricing_status: &'a str,
    }
}
