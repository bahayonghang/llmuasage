use anyhow::Result;
use chrono::Duration;
use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};
use serde_json::{Map, Value};

use crate::{
    commands::report_args::ReportSectionArg,
    models::SourceKind,
    query::reports::{
        ModelCostBreakdown, PeriodKind, ReportFilter, TokenTotals, UnifiedReport, UnifiedRow,
        today_for_timezone,
    },
    store::Store,
    tui::report_table,
};

const PERIOD_ORDER: [PeriodKind; 4] = [
    PeriodKind::Daily,
    PeriodKind::Weekly,
    PeriodKind::Monthly,
    PeriodKind::Session,
];

pub(crate) fn requested_sections(
    command_kind: PeriodKind,
    requested: &[ReportSectionArg],
) -> Vec<PeriodKind> {
    let mut sections = vec![command_kind];
    for kind in PERIOD_ORDER {
        if kind != command_kind && requested.iter().any(|section| section.kind() == kind) {
            sections.push(kind);
        }
    }
    sections
}

pub(crate) fn apply_daily_default(filter: &mut ReportFilter) {
    if filter.since.is_none() && filter.until.is_none() {
        let today = today_for_timezone(&filter.timezone);
        filter.since = Some(today - Duration::days(6));
        filter.until = Some(today);
    }
}

pub(crate) fn load_sections(
    store: &Store,
    filter: &ReportFilter,
    command_kind: PeriodKind,
    requested: &[ReportSectionArg],
    daily_all: bool,
) -> Result<Vec<UnifiedReport>> {
    requested_sections(command_kind, requested)
        .into_iter()
        .map(|kind| {
            let mut section_filter = filter.clone();
            if kind == PeriodKind::Daily && !daily_all {
                apply_daily_default(&mut section_filter);
            }
            crate::query::reports::load_unified_report(store, &section_filter, kind)
        })
        .collect()
}

pub(crate) fn print_sections(
    reports: &[UnifiedReport],
    command_kind: PeriodKind,
    json: bool,
    include_agents: bool,
    compact: bool,
    no_cost: bool,
) -> Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&sections_json(
                reports,
                command_kind,
                include_agents,
                no_cost
            )?)?
        );
    } else {
        let color_mode = report_table::ColorMode::from_env();
        for (index, report) in reports.iter().enumerate() {
            if index > 0 {
                println!();
            }
            println!(
                "{}",
                report_table::render_unified_table(report, compact, no_cost, color_mode)
            );
        }
    }
    Ok(())
}

/// Serializes the CLI-only all-agent view. The DTOs here intentionally avoid
/// deriving from the query payload structs, whose snake_case serialization is
/// still consumed by the dashboard, export, and interactive TUI surfaces.
pub(crate) fn report_json(
    report: &UnifiedReport,
    include_agents: bool,
    no_cost: bool,
) -> Result<Value> {
    let mut value = Map::new();
    value.insert(
        report.kind.rows_key().to_string(),
        rows_json(report, include_agents)?,
    );
    value.insert(
        "totals".to_string(),
        serde_json::to_value(TotalsJson::from(report.totals()))?,
    );
    let mut value = Value::Object(value);
    if no_cost {
        strip_cost_json(&mut value);
    }
    Ok(value)
}

/// Projects a source-filtered unified report into the focused CLI view. Query
/// loading stays shared with the regular report commands; this only removes the
/// all-agent comparison layer after the data has been loaded.
pub(crate) fn focused_report(report: &UnifiedReport, source: SourceKind) -> UnifiedReport {
    let rows = if report.kind == PeriodKind::Session {
        report
            .rows
            .iter()
            .filter(|row| row.agent.source_kind() == Some(source))
            .cloned()
            .collect()
    } else {
        report
            .rows
            .iter()
            .filter_map(|row| {
                row.agent_breakdowns
                    .iter()
                    .find(|agent_row| agent_row.agent.source_kind() == Some(source))
                    .cloned()
            })
            .collect()
    };

    UnifiedReport {
        kind: report.kind,
        rows,
        detected: report
            .detected
            .contains(&source)
            .then_some(source)
            .into_iter()
            .collect(),
    }
}

/// Serializes the CLI-only single-source view. It deliberately uses a
/// separate DTO so focused JSON cannot expose the comparison-only `agent` or
/// `agents` fields.
pub(crate) fn focused_report_json(report: &UnifiedReport, no_cost: bool) -> Result<Value> {
    let mut value = Map::new();
    value.insert(
        report.kind.rows_key().to_string(),
        focused_rows_json(report)?,
    );
    value.insert(
        "totals".to_string(),
        serde_json::to_value(TotalsJson::from(report.totals()))?,
    );
    let mut value = Value::Object(value);
    if no_cost {
        strip_cost_json(&mut value);
    }
    Ok(value)
}

pub(crate) fn sections_json(
    reports: &[UnifiedReport],
    command_kind: PeriodKind,
    include_agents: bool,
    no_cost: bool,
) -> Result<OrderedJson> {
    let mut fields = Vec::with_capacity(reports.len() + 1);
    for report in reports {
        fields.push((
            report.kind.rows_key().to_string(),
            rows_json(report, include_agents)?,
        ));
    }
    let totals = reports
        .iter()
        .find(|report| report.kind == command_kind)
        .map(UnifiedReport::totals)
        .unwrap_or_default();
    fields.push((
        "totals".to_string(),
        serde_json::to_value(TotalsJson::from(totals))?,
    ));
    if no_cost {
        for (_, value) in &mut fields {
            strip_cost_json(value);
        }
    }
    Ok(OrderedJson { fields })
}

pub(crate) fn focused_sections_json(
    reports: &[UnifiedReport],
    command_kind: PeriodKind,
    no_cost: bool,
) -> Result<OrderedJson> {
    let mut fields = Vec::with_capacity(reports.len() + 1);
    for report in reports {
        fields.push((
            report.kind.rows_key().to_string(),
            focused_rows_json(report)?,
        ));
    }
    let totals = reports
        .iter()
        .find(|report| report.kind == command_kind)
        .map(UnifiedReport::totals)
        .unwrap_or_default();
    fields.push((
        "totals".to_string(),
        serde_json::to_value(TotalsJson::from(totals))?,
    ));
    if no_cost {
        for (_, value) in &mut fields {
            strip_cost_json(value);
        }
    }
    Ok(OrderedJson { fields })
}

fn rows_json(report: &UnifiedReport, include_agents: bool) -> Result<Value> {
    Ok(serde_json::to_value(
        report
            .rows
            .iter()
            .map(|row| row_json(row, include_agents && report.kind != PeriodKind::Session))
            .collect::<Vec<_>>(),
    )?)
}

fn focused_rows_json(report: &UnifiedReport) -> Result<Value> {
    Ok(serde_json::to_value(
        report
            .rows
            .iter()
            .map(FocusedRowJson::from)
            .collect::<Vec<_>>(),
    )?)
}

pub(crate) struct OrderedJson {
    fields: Vec<(String, Value)>,
}

impl Serialize for OrderedJson {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.fields.len()))?;
        for (key, value) in &self.fields {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

pub(crate) fn strip_cost_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|key, child| {
                if key.to_ascii_lowercase().contains("cost") {
                    return false;
                }
                strip_cost_json(child);
                true
            });
        }
        Value::Array(items) => {
            for item in items {
                strip_cost_json(item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn row_json(row: &UnifiedRow, include_agents: bool) -> UnifiedRowJson {
    UnifiedRowJson {
        period: row.period.clone(),
        agent: row.agent.id().to_string(),
        models_used: row.models_used.clone(),
        input_tokens: row.totals.input_tokens,
        output_tokens: row.totals.output_tokens,
        cache_creation_tokens: row.totals.cache_creation_tokens,
        cache_read_tokens: row.totals.cache_read_tokens,
        total_tokens: row.totals.total_tokens,
        total_cost: row.totals.estimated_cost_usd,
        model_breakdowns: row
            .model_breakdowns
            .iter()
            .map(ModelBreakdownJson::from)
            .collect(),
        agents: include_agents.then(|| row.agent_breakdowns.iter().map(AgentJson::from).collect()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnifiedRowJson {
    period: String,
    agent: String,
    models_used: Vec<String>,
    input_tokens: i64,
    output_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    total_tokens: i64,
    total_cost: f64,
    model_breakdowns: Vec<ModelBreakdownJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agents: Option<Vec<AgentJson>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FocusedRowJson {
    period: String,
    models_used: Vec<String>,
    input_tokens: i64,
    output_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    total_tokens: i64,
    total_cost: f64,
    model_breakdowns: Vec<ModelBreakdownJson>,
}

impl From<&UnifiedRow> for FocusedRowJson {
    fn from(row: &UnifiedRow) -> Self {
        Self {
            period: row.period.clone(),
            models_used: row.models_used.clone(),
            input_tokens: row.totals.input_tokens,
            output_tokens: row.totals.output_tokens,
            cache_creation_tokens: row.totals.cache_creation_tokens,
            cache_read_tokens: row.totals.cache_read_tokens,
            total_tokens: row.totals.total_tokens,
            total_cost: row.totals.estimated_cost_usd,
            model_breakdowns: row
                .model_breakdowns
                .iter()
                .map(ModelBreakdownJson::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentJson {
    agent: String,
    models_used: Vec<String>,
    input_tokens: i64,
    output_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    total_tokens: i64,
    total_cost: f64,
    model_breakdowns: Vec<ModelBreakdownJson>,
}

impl From<&UnifiedRow> for AgentJson {
    fn from(row: &UnifiedRow) -> Self {
        Self {
            agent: row.agent.id().to_string(),
            models_used: row.models_used.clone(),
            input_tokens: row.totals.input_tokens,
            output_tokens: row.totals.output_tokens,
            cache_creation_tokens: row.totals.cache_creation_tokens,
            cache_read_tokens: row.totals.cache_read_tokens,
            total_tokens: row.totals.total_tokens,
            total_cost: row.totals.estimated_cost_usd,
            model_breakdowns: row
                .model_breakdowns
                .iter()
                .map(ModelBreakdownJson::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelBreakdownJson {
    model_name: String,
    input_tokens: i64,
    output_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    cost: f64,
}

impl From<&ModelCostBreakdown> for ModelBreakdownJson {
    fn from(value: &ModelCostBreakdown) -> Self {
        Self {
            model_name: value.model.clone(),
            input_tokens: value.input_tokens,
            output_tokens: value.output_tokens,
            cache_creation_tokens: value.cache_creation_tokens,
            cache_read_tokens: value.cache_read_tokens,
            cost: value.estimated_cost_usd,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TotalsJson {
    input_tokens: i64,
    output_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    total_tokens: i64,
    total_cost: f64,
}

impl From<TokenTotals> for TotalsJson {
    fn from(value: TokenTotals) -> Self {
        Self {
            input_tokens: value.input_tokens,
            output_tokens: value.output_tokens,
            cache_creation_tokens: value.cache_creation_tokens,
            cache_read_tokens: value.cache_read_tokens,
            total_tokens: value.total_tokens,
            total_cost: value.estimated_cost_usd,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::SourceKind,
        query::reports::{ModelCostBreakdown, TokenTotals, UnifiedAgent, UnifiedRow},
    };

    #[test]
    fn requested_sections_keeps_current_first_and_uses_fixed_order() {
        assert_eq!(
            requested_sections(
                PeriodKind::Monthly,
                &[ReportSectionArg::Session, ReportSectionArg::Daily]
            ),
            vec![PeriodKind::Monthly, PeriodKind::Daily, PeriodKind::Session]
        );
        assert_eq!(
            requested_sections(
                PeriodKind::Daily,
                &[ReportSectionArg::Daily, ReportSectionArg::Monthly]
            ),
            vec![PeriodKind::Daily, PeriodKind::Monthly]
        );
    }

    #[test]
    fn focused_projection_removes_the_agent_comparison_layer() {
        let report = UnifiedReport {
            kind: PeriodKind::Daily,
            rows: vec![UnifiedRow {
                period: "2026-05-05".to_string(),
                agent: UnifiedAgent::All,
                totals: TokenTotals {
                    total_tokens: 30,
                    ..TokenTotals::default()
                },
                models_used: vec!["gpt-5".to_string(), "claude-sonnet-4".to_string()],
                agent_breakdowns: vec![
                    UnifiedRow {
                        period: "2026-05-05".to_string(),
                        agent: UnifiedAgent::Source(SourceKind::Codex),
                        totals: TokenTotals {
                            total_tokens: 10,
                            ..TokenTotals::default()
                        },
                        models_used: vec!["gpt-5".to_string()],
                        agent_breakdowns: Vec::new(),
                        model_breakdowns: vec![ModelCostBreakdown {
                            model: "gpt-5".to_string(),
                            total_tokens: 10,
                            ..ModelCostBreakdown::default()
                        }],
                    },
                    UnifiedRow {
                        period: "2026-05-05".to_string(),
                        agent: UnifiedAgent::Source(SourceKind::Claude),
                        totals: TokenTotals {
                            total_tokens: 20,
                            ..TokenTotals::default()
                        },
                        models_used: vec!["claude-sonnet-4".to_string()],
                        agent_breakdowns: Vec::new(),
                        model_breakdowns: Vec::new(),
                    },
                ],
                model_breakdowns: Vec::new(),
            }],
            detected: vec![SourceKind::Codex, SourceKind::Claude],
        };

        let focused = focused_report(&report, SourceKind::Codex);
        assert_eq!(focused.rows.len(), 1);
        assert_eq!(focused.rows[0].totals.total_tokens, 10);
        assert!(focused.rows[0].agent_breakdowns.is_empty());
        assert_eq!(focused.detected, vec![SourceKind::Codex]);

        let json = focused_report_json(&focused, false).expect("focused JSON should serialize");
        assert!(json["daily"][0].get("agent").is_none());
        assert!(json["daily"][0].get("agents").is_none());
        assert_eq!(json["daily"][0]["totalTokens"].as_i64(), Some(10));
    }
}
