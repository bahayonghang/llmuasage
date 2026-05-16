use std::collections::BTreeSet;

use crate::query::reports::{
    BlockReportRow, DailyReportRow, ModelCostBreakdown, MonthlyReportRow, ProjectSummary,
    SessionReportRow, TokenTotals,
};

const COMPACT_THRESHOLD: usize = 100;
const MAX_MODELS_DISPLAYED: usize = 3;

#[derive(Clone, Copy)]
enum Align {
    Left,
    Right,
}

struct Column {
    header: &'static str,
    align: Align,
}

pub fn render_daily_table(
    rows: &[DailyReportRow],
    totals: Option<&TokenTotals>,
    compact: bool,
    show_project: bool,
) -> String {
    let compact = compact_mode(compact);
    let mut table_rows = Vec::new();
    for row in rows {
        table_rows.push(period_row(
            &row.date,
            row.project.as_ref(),
            &row.models_used,
            &row.totals,
            compact,
            show_project,
        ));
        append_breakdowns(
            &mut table_rows,
            &row.model_breakdowns,
            compact,
            show_project,
        );
    }
    if let Some(totals) = totals {
        table_rows.push(period_total_row(totals, compact, show_project));
    }
    let columns = period_columns("Date", compact, show_project);
    render_table(&columns, &table_rows)
}

pub fn render_monthly_table(
    rows: &[MonthlyReportRow],
    totals: Option<&TokenTotals>,
    compact: bool,
) -> String {
    let compact = compact_mode(compact);
    let mut table_rows = Vec::new();
    for row in rows {
        table_rows.push(period_row(
            &row.month,
            None,
            &row.models_used,
            &row.totals,
            compact,
            false,
        ));
        append_breakdowns(&mut table_rows, &row.model_breakdowns, compact, false);
    }
    if let Some(totals) = totals {
        table_rows.push(period_total_row(totals, compact, false));
    }
    let columns = period_columns("Month", compact, false);
    render_table(&columns, &table_rows)
}

pub fn render_session_table(
    rows: &[SessionReportRow],
    totals: Option<&TokenTotals>,
    compact: bool,
) -> String {
    let compact = compact_mode(compact);
    let columns = session_columns(compact);
    let mut table_rows = Vec::new();
    for row in rows {
        let project = row
            .project
            .as_ref()
            .map(project_name)
            .unwrap_or_else(|| "-".to_string());
        let session = shorten(&row.session_id, 28);
        if compact {
            table_rows.push(vec![
                session,
                project,
                format_models(&row.models_used),
                format_count(row.totals.input_tokens),
                format_count(row.totals.output_tokens),
                format_cost(row.totals.estimated_cost_usd),
                row.last_activity_at.clone(),
            ]);
        } else {
            table_rows.push(vec![
                session,
                project,
                format_models(&row.models_used),
                format_count(row.totals.input_tokens),
                format_count(row.totals.output_tokens),
                format_count(row.totals.reasoning_output_tokens),
                format_count(row.totals.cache_read_tokens),
                format_count(row.totals.total_tokens),
                format_cost(row.totals.estimated_cost_usd),
                row.last_activity_at.clone(),
            ]);
        }
        append_breakdowns(&mut table_rows, &row.model_breakdowns, compact, true);
    }
    if let Some(totals) = totals {
        table_rows.push(session_total_row(totals, compact));
    }
    render_table(&columns, &table_rows)
}

pub fn render_blocks_table(rows: &[BlockReportRow], compact: bool) -> String {
    let compact = compact_mode(compact);
    let columns = if compact {
        vec![
            column("Block", Align::Left),
            column("Models", Align::Left),
            column("Total Tokens", Align::Right),
            column("Burn/h", Align::Right),
            column("Projected", Align::Right),
            column("Cost (USD)", Align::Right),
            column("Active", Align::Left),
        ]
    } else {
        vec![
            column("Block", Align::Left),
            column("Models", Align::Left),
            column("Input", Align::Right),
            column("Output", Align::Right),
            column("Reasoning", Align::Right),
            column("Cache Read", Align::Right),
            column("Total Tokens", Align::Right),
            column("Burn/h", Align::Right),
            column("Projected", Align::Right),
            column("Limit", Align::Right),
            column("Cost (USD)", Align::Right),
            column("Active", Align::Left),
        ]
    };
    let table_rows = rows
        .iter()
        .map(|row| {
            let block = format!("{} -> {}", row.start_at, row.end_at);
            let active = if row.is_active { "yes" } else { "no" }.to_string();
            if compact {
                vec![
                    block,
                    format_models(&row.models_used),
                    format_count(row.totals.total_tokens),
                    format_count(row.burn_rate_tokens_per_hour.round() as i64),
                    format_count(row.projected_total_tokens),
                    format_cost(row.totals.estimated_cost_usd),
                    active,
                ]
            } else {
                vec![
                    block,
                    format_models(&row.models_used),
                    format_count(row.totals.input_tokens),
                    format_count(row.totals.output_tokens),
                    format_count(row.totals.reasoning_output_tokens),
                    format_count(row.totals.cache_read_tokens),
                    format_count(row.totals.total_tokens),
                    format_count(row.burn_rate_tokens_per_hour.round() as i64),
                    format_count(row.projected_total_tokens),
                    row.token_limit
                        .map(|value| format_count(value as i64))
                        .unwrap_or_else(|| "-".to_string()),
                    format_cost(row.totals.estimated_cost_usd),
                    active,
                ]
            }
        })
        .collect::<Vec<_>>();
    render_table(&columns, &table_rows)
}

pub fn format_totals(totals: &TokenTotals) -> String {
    format!(
        "Total: {} tokens, {} estimated",
        format_count(totals.total_tokens),
        format_cost(totals.estimated_cost_usd)
    )
}

pub fn format_count(value: i64) -> String {
    let raw = value.abs().to_string();
    let mut out = String::new();
    for (idx, ch) in raw.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    let formatted = out.chars().rev().collect::<String>();
    if value < 0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}

pub fn format_cost(value: f64) -> String {
    format!("${value:.2}")
}

fn period_columns(first_header: &'static str, compact: bool, show_project: bool) -> Vec<Column> {
    let mut columns = vec![column(first_header, Align::Left)];
    if show_project {
        columns.push(column("Project", Align::Left));
    }
    columns.push(column("Models", Align::Left));
    columns.push(column("Input", Align::Right));
    columns.push(column("Output", Align::Right));
    if !compact {
        columns.push(column("Reasoning", Align::Right));
        columns.push(column("Cache Read", Align::Right));
        columns.push(column("Total Tokens", Align::Right));
    }
    columns.push(column("Cost (USD)", Align::Right));
    columns
}

fn session_columns(compact: bool) -> Vec<Column> {
    if compact {
        vec![
            column("Session", Align::Left),
            column("Project", Align::Left),
            column("Models", Align::Left),
            column("Input", Align::Right),
            column("Output", Align::Right),
            column("Cost (USD)", Align::Right),
            column("Last Activity", Align::Left),
        ]
    } else {
        vec![
            column("Session", Align::Left),
            column("Project", Align::Left),
            column("Models", Align::Left),
            column("Input", Align::Right),
            column("Output", Align::Right),
            column("Reasoning", Align::Right),
            column("Cache Read", Align::Right),
            column("Total Tokens", Align::Right),
            column("Cost (USD)", Align::Right),
            column("Last Activity", Align::Left),
        ]
    }
}

fn period_row(
    period: &str,
    project: Option<&ProjectSummary>,
    models: &[String],
    totals: &TokenTotals,
    compact: bool,
    show_project: bool,
) -> Vec<String> {
    let mut row = vec![period.to_string()];
    if show_project {
        row.push(project.map(project_name).unwrap_or_else(|| "-".to_string()));
    }
    row.push(format_models(models));
    row.push(format_count(totals.input_tokens));
    row.push(format_count(totals.output_tokens));
    if !compact {
        row.push(format_count(totals.reasoning_output_tokens));
        row.push(format_count(totals.cache_read_tokens));
        row.push(format_count(totals.total_tokens));
    }
    row.push(format_cost(totals.estimated_cost_usd));
    row
}

fn period_total_row(totals: &TokenTotals, compact: bool, show_project: bool) -> Vec<String> {
    let mut row = vec!["Total".to_string()];
    if show_project {
        row.push(String::new());
    }
    row.push(String::new());
    row.push(format_count(totals.input_tokens));
    row.push(format_count(totals.output_tokens));
    if !compact {
        row.push(format_count(totals.reasoning_output_tokens));
        row.push(format_count(totals.cache_read_tokens));
        row.push(format_count(totals.total_tokens));
    }
    row.push(format_cost(totals.estimated_cost_usd));
    row
}

fn session_total_row(totals: &TokenTotals, compact: bool) -> Vec<String> {
    if compact {
        vec![
            "Total".to_string(),
            String::new(),
            String::new(),
            format_count(totals.input_tokens),
            format_count(totals.output_tokens),
            format_cost(totals.estimated_cost_usd),
            String::new(),
        ]
    } else {
        vec![
            "Total".to_string(),
            String::new(),
            String::new(),
            format_count(totals.input_tokens),
            format_count(totals.output_tokens),
            format_count(totals.reasoning_output_tokens),
            format_count(totals.cache_read_tokens),
            format_count(totals.total_tokens),
            format_cost(totals.estimated_cost_usd),
            String::new(),
        ]
    }
}

fn append_breakdowns(
    rows: &mut Vec<Vec<String>>,
    breakdowns: &[ModelCostBreakdown],
    compact: bool,
    show_project: bool,
) {
    for item in breakdowns {
        let label = format!("\u{2514}\u{2500} {}:{}", item.source, item.model);
        let mut row = vec![label];
        if show_project {
            row.push(String::new());
        }
        row.push(String::new());
        row.push(format_count(item.input_tokens));
        row.push(format_count(item.output_tokens));
        if !compact {
            row.push(format_count(item.reasoning_output_tokens));
            row.push(format_count(item.cache_read_tokens));
            row.push(format_count(item.total_tokens));
        }
        row.push(format_cost(item.estimated_cost_usd));
        rows.push(row);
    }
}

fn render_table(columns: &[Column], rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return "No usage data matched the report filters.".to_string();
    }

    let mut widths = columns
        .iter()
        .map(|column| column.header.chars().count())
        .collect::<Vec<_>>();
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            if idx < widths.len() {
                let max_line_width = cell_lines(cell)
                    .iter()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or_default();
                widths[idx] = widths[idx].max(max_line_width);
            }
        }
    }
    fit_widths(columns, &mut widths, terminal_width());

    let mut out = String::new();
    push_border(&mut out, '\u{250C}', '\u{252C}', '\u{2510}', &widths);
    push_row(
        &mut out,
        columns,
        &columns
            .iter()
            .map(|column| column.header.to_string())
            .collect::<Vec<_>>(),
        &widths,
    );
    push_border(&mut out, '\u{251C}', '\u{253C}', '\u{2524}', &widths);
    for (idx, row) in rows.iter().enumerate() {
        push_row(&mut out, columns, row, &widths);
        let is_last = idx + 1 == rows.len();
        if is_last {
            push_border(&mut out, '\u{2514}', '\u{2534}', '\u{2518}', &widths);
        } else {
            push_border(&mut out, '\u{251C}', '\u{253C}', '\u{2524}', &widths);
        }
    }
    out
}

fn push_border(out: &mut String, left: char, sep: char, right: char, widths: &[usize]) {
    out.push(left);
    for (idx, width) in widths.iter().enumerate() {
        out.push_str(&repeat_char('\u{2500}', width + 2));
        if idx + 1 == widths.len() {
            out.push(right);
        } else {
            out.push(sep);
        }
    }
    out.push('\n');
}

fn push_row(out: &mut String, columns: &[Column], row: &[String], widths: &[usize]) {
    let split_cells = widths
        .iter()
        .enumerate()
        .map(|(idx, _)| cell_lines(row.get(idx).map(String::as_str).unwrap_or_default()))
        .collect::<Vec<_>>();
    let height = split_cells.iter().map(Vec::len).max().unwrap_or(1);
    for line_idx in 0..height {
        out.push('\u{2502}');
        for (idx, width) in widths.iter().enumerate() {
            let raw = split_cells
                .get(idx)
                .and_then(|lines| lines.get(line_idx))
                .map(String::as_str)
                .unwrap_or_default();
            let clipped = shorten(raw, *width);
            let align = columns
                .get(idx)
                .map(|column| column.align)
                .unwrap_or(Align::Left);
            out.push(' ');
            out.push_str(&pad(&clipped, *width, align));
            out.push(' ');
            out.push('\u{2502}');
        }
        out.push('\n');
    }
}

fn fit_widths(columns: &[Column], widths: &mut [usize], terminal_width: usize) {
    let min_widths = columns
        .iter()
        .enumerate()
        .map(|(idx, column)| match (idx, column.header, column.align) {
            (_, "Models", _) => 12,
            (0, _, _) => 8,
            (_, _, Align::Right) => 10,
            _ => 8,
        })
        .collect::<Vec<_>>();

    while table_width(widths) > terminal_width {
        let Some((idx, _)) = widths
            .iter()
            .enumerate()
            .filter(|(idx, width)| **width > min_widths[*idx])
            .max_by_key(|(_, width)| **width)
        else {
            break;
        };
        widths[idx] -= 1;
    }
}

fn table_width(widths: &[usize]) -> usize {
    widths.iter().sum::<usize>() + (widths.len() * 3) + 1
}

fn cell_lines(value: &str) -> Vec<String> {
    let lines = value.lines().map(str::to_string).collect::<Vec<_>>();
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn pad(value: &str, width: usize, align: Align) -> String {
    let len = value.chars().count();
    if len >= width {
        return value.to_string();
    }
    let padding = " ".repeat(width - len);
    match align {
        Align::Left => format!("{value}{padding}"),
        Align::Right => format!("{padding}{value}"),
    }
}

fn column(header: &'static str, align: Align) -> Column {
    Column { header, align }
}

fn compact_mode(force_compact: bool) -> bool {
    force_compact || terminal_width() < COMPACT_THRESHOLD
}

fn format_models(models: &[String]) -> String {
    if models.is_empty() {
        return "-".to_string();
    }
    let models = models
        .iter()
        .map(|model| format_model_name(model))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut lines = models
        .iter()
        .take(MAX_MODELS_DISPLAYED)
        .map(|model| format!("- {}", shorten(model, 30)))
        .collect::<Vec<_>>();
    if models.len() > MAX_MODELS_DISPLAYED {
        lines.push(format!("- +{} more", models.len() - MAX_MODELS_DISPLAYED));
    }
    lines.join("\n")
}

fn format_model_name(model: &str) -> String {
    let (prefix, model) = model
        .strip_prefix("[pi] ")
        .map(|model| ("[pi] ", model))
        .unwrap_or(("", model));
    let model = model.strip_prefix("anthropic/").unwrap_or(model);
    let Some(model) = model.strip_prefix("claude-") else {
        return format!("{prefix}{model}");
    };

    let parts = model.split('-').collect::<Vec<_>>();
    if parts.len() > 2 && parts.last().is_some_and(|part| is_yyyymmdd(part)) {
        format!("{}{}", prefix, parts[..parts.len() - 1].join("-"))
    } else {
        format!("{prefix}{model}")
    }
}

fn is_yyyymmdd(value: &str) -> bool {
    value.len() == 8 && value.chars().all(|ch| ch.is_ascii_digit())
}

fn project_name(project: &ProjectSummary) -> String {
    project
        .project_ref
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| project.project_label.clone())
}

fn shorten(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    if max_chars <= 3 {
        return value.chars().take(max_chars).collect::<String>();
    }
    format!(
        "{}...",
        value.chars().take(max_chars - 3).collect::<String>()
    )
}

fn repeat_char(ch: char, count: usize) -> String {
    std::iter::repeat_n(ch, count).collect()
}

fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .or_else(detected_terminal_width)
        .unwrap_or(120)
        .max(60)
}

#[cfg(not(test))]
fn detected_terminal_width() -> Option<usize> {
    crossterm::terminal::size()
        .ok()
        .map(|(width, _)| width as usize)
}

#[cfg(test)]
fn detected_terminal_width() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_empty_table_is_actionable() {
        assert!(render_daily_table(&[], None, false, false).contains("No usage data"));
    }

    #[test]
    fn count_format_uses_grouping() {
        assert_eq!(format_count(1234567), "1,234,567");
    }

    #[test]
    fn cost_format_uses_two_decimals() {
        assert_eq!(format_cost(12.345), "$12.35");
    }

    #[test]
    fn usage_table_uses_box_borders_and_total_row() {
        let totals = TokenTotals {
            input_tokens: 1234,
            output_tokens: 56,
            reasoning_output_tokens: 7,
            cache_read_tokens: 890,
            total_tokens: 2187,
            estimated_cost_usd: 3.5,
        };
        let row = DailyReportRow {
            date: "2026-05-05".to_string(),
            source: None,
            project: None,
            totals: totals.clone(),
            models_used: vec![
                "claude-sonnet-4-20250514".to_string(),
                "gpt-5.4".to_string(),
            ],
            model_breakdowns: Vec::new(),
        };

        let table = render_daily_table(&[row], Some(&totals), false, false);

        assert!(table.contains('\u{250C}'));
        assert!(table.contains("Cache Read"));
        assert!(table.contains("Total Tokens"));
        assert!(table.contains("- sonnet-4"));
        assert!(table.contains("Total"));
        assert!(table.contains("$3.50"));
    }
}
