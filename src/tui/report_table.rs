use std::collections::BTreeSet;
use std::fmt::Write as _;

use crossterm::style::{Color, Stylize, style};

use crate::models::SourceKind;
use crate::query::reports::{
    BlockReportRow, DailyReportRow, ModelCostBreakdown, MonthlyReportRow, ProjectSummary,
    ReportNotes, SessionReportRow, TokenTotals,
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

pub fn render_daily_summary_table(
    rows: &[DailyReportRow],
    totals: Option<&TokenTotals>,
    compact: bool,
    color_mode: ColorMode,
) -> String {
    let compact = compact_mode(compact);
    let mut table_rows = Vec::new();
    for row in rows {
        table_rows.push(daily_summary_row(row, compact));
        append_daily_summary_breakdowns(&mut table_rows, &row.model_breakdowns, compact);
    }
    if let Some(totals) = totals {
        table_rows.push(daily_summary_total_row(totals, compact));
    }
    render_table_styled(
        &daily_summary_columns(compact),
        &table_rows,
        Some(DailyTableStyle {
            source: SourceKind::Codex,
            color_mode,
        }),
    )
}

pub fn render_daily_source_table(rows: &[DailyReportRow], totals: Option<&TokenTotals>) -> String {
    render_table(
        &daily_source_columns(),
        &daily_source_table_rows(rows, totals),
    )
}

pub fn render_daily_source_table_styled(
    source: SourceKind,
    rows: &[DailyReportRow],
    totals: Option<&TokenTotals>,
    color_mode: ColorMode,
) -> String {
    render_table_styled(
        &daily_source_columns(),
        &daily_source_table_rows(rows, totals),
        Some(DailyTableStyle { source, color_mode }),
    )
}

pub fn render_source_title(source: SourceKind, title: &str, color_mode: ColorMode) -> String {
    if !color_mode.enabled() {
        return title.to_string();
    }
    style(title).with(source_color(source)).bold().to_string()
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

pub fn format_token_compact(value: i64) -> String {
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

pub fn format_cost(value: f64) -> String {
    format!("${value:.2}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

impl ColorMode {
    pub fn from_env() -> Self {
        if env_flag("LLMUSAGE_FORCE_COLOR") || env_flag("CLICOLOR_FORCE") {
            return Self::Always;
        }
        if std::env::var_os("NO_COLOR").is_some() || env_flag("LLMUSAGE_NO_COLOR") {
            return Self::Never;
        }
        Self::Auto
    }

    pub fn stderr_enabled(self) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => stderr_is_terminal(),
        }
    }

    fn enabled(self) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => stdout_is_terminal(),
        }
    }
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

fn daily_source_columns() -> Vec<Column> {
    vec![
        column("Date", Align::Left),
        column("Conv", Align::Right),
        column("Models", Align::Left),
        column("Input", Align::Right),
        column("Cache", Align::Right),
        column("Output", Align::Right),
        column("Reason", Align::Right),
        column("All", Align::Right),
        column("Cost", Align::Right),
        column("Notes", Align::Left),
    ]
}

fn daily_summary_columns(compact: bool) -> Vec<Column> {
    let mut columns = vec![
        column("Date", Align::Left),
        column("Models", Align::Left),
        column("Input", Align::Right),
        column("Output", Align::Right),
    ];
    if !compact {
        columns.push(column("Cache Create", Align::Right));
        columns.push(column("Cache Read", Align::Right));
        columns.push(column("Total Tokens", Align::Right));
    }
    columns.push(column("Cost (USD)", Align::Right));
    columns
}

fn daily_source_table_rows(
    rows: &[DailyReportRow],
    totals: Option<&TokenTotals>,
) -> Vec<Vec<String>> {
    let mut table_rows = Vec::new();
    for row in rows {
        table_rows.push(daily_source_row(row));
        append_daily_source_breakdowns(&mut table_rows, &row.model_breakdowns);
    }
    if let Some(totals) = totals {
        let conversation_count = rows.iter().map(|row| row.conversation_count).sum::<usize>();
        let notes = rows.iter().fold(ReportNotes::default(), |mut notes, row| {
            notes.unpriced |= row.notes.unpriced;
            notes.reason_not_reported |= row.notes.reason_not_reported;
            notes
        });
        table_rows.push(daily_source_total_row(totals, conversation_count, &notes));
    }
    table_rows
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

fn daily_source_row(row: &DailyReportRow) -> Vec<String> {
    vec![
        row.date.clone(),
        row.conversation_count.to_string(),
        format_models_inline(&row.models_used),
        format_token_compact(row.totals.input_tokens),
        format_token_compact(row.totals.cache_read_tokens),
        format_token_compact(row.totals.output_tokens),
        format_token_compact(row.totals.reasoning_output_tokens),
        format_token_compact(row.totals.total_tokens),
        format_cost(row.totals.estimated_cost_usd),
        format_notes(&row.notes),
    ]
}

fn daily_summary_row(row: &DailyReportRow, compact: bool) -> Vec<String> {
    let mut cells = vec![
        format_daily_date(&row.date),
        format_models(&row.models_used),
        format_count(row.totals.input_tokens),
        format_count(row.totals.output_tokens),
    ];
    if !compact {
        cells.push(format_count(row.totals.cache_creation_tokens));
        cells.push(format_count(row.totals.cache_read_tokens));
        cells.push(format_count(row.totals.total_tokens));
    }
    cells.push(format_cost(row.totals.estimated_cost_usd));
    cells
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

fn daily_summary_total_row(totals: &TokenTotals, compact: bool) -> Vec<String> {
    let mut cells = vec![
        "Total".to_string(),
        String::new(),
        format_count(totals.input_tokens),
        format_count(totals.output_tokens),
    ];
    if !compact {
        cells.push(format_count(totals.cache_creation_tokens));
        cells.push(format_count(totals.cache_read_tokens));
        cells.push(format_count(totals.total_tokens));
    }
    cells.push(format_cost(totals.estimated_cost_usd));
    cells
}

fn daily_source_total_row(
    totals: &TokenTotals,
    conversation_count: usize,
    notes: &ReportNotes,
) -> Vec<String> {
    vec![
        "TOTAL".to_string(),
        conversation_count.to_string(),
        String::new(),
        format_token_compact(totals.input_tokens),
        format_token_compact(totals.cache_read_tokens),
        format_token_compact(totals.output_tokens),
        format_token_compact(totals.reasoning_output_tokens),
        format_token_compact(totals.total_tokens),
        format_cost(totals.estimated_cost_usd),
        format_notes(notes),
    ]
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

fn append_daily_source_breakdowns(rows: &mut Vec<Vec<String>>, breakdowns: &[ModelCostBreakdown]) {
    for item in breakdowns {
        rows.push(vec![
            format!("\u{2514}\u{2500} {}", format_model_name(&item.model)),
            String::new(),
            item.source.clone(),
            format_token_compact(item.input_tokens),
            format_token_compact(item.cache_read_tokens),
            format_token_compact(item.output_tokens),
            format_token_compact(item.reasoning_output_tokens),
            format_token_compact(item.total_tokens),
            format_cost(item.estimated_cost_usd),
            String::new(),
        ]);
    }
}

fn append_daily_summary_breakdowns(
    rows: &mut Vec<Vec<String>>,
    breakdowns: &[ModelCostBreakdown],
    compact: bool,
) {
    for item in breakdowns {
        let mut cells = vec![
            format!(
                "\u{2514}\u{2500} {}:{}",
                item.source,
                format_model_name(&item.model)
            ),
            String::new(),
            format_count(item.input_tokens),
            format_count(item.output_tokens),
        ];
        if !compact {
            cells.push(format_count(item.cache_creation_tokens));
            cells.push(format_count(item.cache_read_tokens));
            cells.push(format_count(item.total_tokens));
        }
        cells.push(format_cost(item.estimated_cost_usd));
        rows.push(cells);
    }
}

fn render_table(columns: &[Column], rows: &[Vec<String>]) -> String {
    render_table_styled(columns, rows, None)
}

#[derive(Clone, Copy)]
struct DailyTableStyle {
    source: SourceKind,
    color_mode: ColorMode,
}

fn render_table_styled(
    columns: &[Column],
    rows: &[Vec<String>],
    style_options: Option<DailyTableStyle>,
) -> String {
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
        style_options.and_then(|style| style.header_style()),
    );
    push_border(&mut out, '\u{251C}', '\u{253C}', '\u{2524}', &widths);
    for (idx, row) in rows.iter().enumerate() {
        let is_total = row
            .first()
            .is_some_and(|cell| cell == "TOTAL" || cell == "Total");
        let row_style = style_options.and_then(|style| style.row_style(is_total));
        push_row(&mut out, columns, row, &widths, row_style);
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

fn push_row(
    out: &mut String,
    columns: &[Column],
    row: &[String],
    widths: &[usize],
    row_style: Option<RowStyle>,
) {
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
            push_styled_padded(out, &clipped, *width, align, row_style, idx);
            out.push(' ');
            out.push('\u{2502}');
        }
        out.push('\n');
    }
}

#[derive(Clone, Copy)]
struct RowStyle {
    color: Color,
    bold: bool,
    notes_dim: bool,
}

impl DailyTableStyle {
    fn header_style(self) -> Option<RowStyle> {
        self.color_mode.enabled().then_some(RowStyle {
            color: source_color(self.source),
            bold: true,
            notes_dim: false,
        })
    }

    fn row_style(self, is_total: bool) -> Option<RowStyle> {
        if !(is_total && self.color_mode.enabled()) {
            return None;
        }
        Some(RowStyle {
            color: source_color(self.source),
            bold: true,
            notes_dim: true,
        })
    }
}

fn push_styled_padded(
    out: &mut String,
    value: &str,
    width: usize,
    align: Align,
    row_style: Option<RowStyle>,
    column_idx: usize,
) {
    let len = value.chars().count();
    let padding = width.saturating_sub(len);
    let (left, right) = match align {
        Align::Left => (String::new(), " ".repeat(padding)),
        Align::Right => (" ".repeat(padding), String::new()),
    };
    out.push_str(&left);
    if let Some(row_style) = row_style {
        let mut content = style(value).with(row_style.color);
        if row_style.bold {
            content = content.bold();
        }
        if row_style.notes_dim && column_idx + 1 == 10 {
            content = content.dim();
        }
        let _ = write!(out, "{content}");
    } else {
        out.push_str(value);
    }
    out.push_str(&right);
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

fn format_notes(notes: &ReportNotes) -> String {
    let mut labels = Vec::new();
    if notes.unpriced {
        labels.push("unpriced");
    }
    if notes.reason_not_reported {
        labels.push("reason not reported");
    }
    labels.join("; ")
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

fn format_models_inline(models: &[String]) -> String {
    if models.is_empty() {
        return "-".to_string();
    }
    models
        .iter()
        .map(|model| format_model_name(model))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_daily_date(date: &str) -> String {
    let Some((year, month_day)) = date.split_once('-') else {
        return date.to_string();
    };
    if month_day.len() != 5 {
        return date.to_string();
    }
    format!("{year}\n{month_day}")
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

fn source_color(source: SourceKind) -> Color {
    match source {
        SourceKind::Codex => Color::Cyan,
        SourceKind::Claude => Color::Magenta,
        SourceKind::Opencode => Color::Green,
        SourceKind::Antigravity => Color::Blue,
    }
}

fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .map(|value| {
            let value = value.trim();
            !value.is_empty()
                && !value.eq_ignore_ascii_case("0")
                && !value.eq_ignore_ascii_case("false")
                && !value.eq_ignore_ascii_case("no")
                && !value.eq_ignore_ascii_case("off")
        })
        .unwrap_or(false)
}

#[cfg(not(test))]
fn stdout_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

#[cfg(test)]
fn stdout_is_terminal() -> bool {
    false
}

#[cfg(not(test))]
fn stderr_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

#[cfg(test)]
fn stderr_is_terminal() -> bool {
    false
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
    fn token_compact_format_uses_short_units() {
        assert_eq!(format_token_compact(978_050), "978.05K");
        assert_eq!(format_token_compact(5_370_000), "5.37M");
        assert_eq!(format_token_compact(40_330_000_000), "40.33B");
    }

    #[test]
    fn usage_table_uses_box_borders_and_total_row() {
        let totals = TokenTotals {
            input_tokens: 1234,
            cache_creation_tokens: 111,
            output_tokens: 56,
            reasoning_output_tokens: 7,
            cache_read_tokens: 890,
            total_tokens: 2298,
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
            conversation_count: 1,
            notes: ReportNotes::default(),
        };

        let table = render_daily_summary_table(&[row], Some(&totals), false, ColorMode::Never);

        assert!(table.contains('\u{250C}'));
        assert!(table.contains("Cache Create"));
        assert!(table.contains("Cache Read"));
        assert!(table.contains("Total Tokens"));
        assert!(table.contains("- sonnet-4"));
        assert!(table.contains("Total"));
        assert!(table.contains("$3.50"));
        assert!(!table.contains("Reasoning"));
    }

    #[test]
    fn daily_summary_table_uses_ccusage_style_columns_and_grouped_counts() {
        let totals = TokenTotals {
            input_tokens: 978_050,
            cache_creation_tokens: 333_333,
            cache_read_tokens: 5_370_000,
            output_tokens: 40_330_000_000,
            reasoning_output_tokens: 12_345,
            total_tokens: 40_336_693_728,
            estimated_cost_usd: 3.5,
        };
        let row = DailyReportRow {
            date: "2026-05-05".to_string(),
            source: None,
            project: None,
            totals: totals.clone(),
            models_used: vec!["gpt-5.4".to_string()],
            model_breakdowns: Vec::new(),
            conversation_count: 1,
            notes: ReportNotes::default(),
        };

        let table = render_daily_summary_table(&[row], Some(&totals), false, ColorMode::Never);

        assert!(table.contains("Cache Create"));
        assert!(table.contains("Cache Read"));
        assert!(table.contains("Total Tokens"));
        assert!(table.contains("Cost (USD)"));
        assert!(table.contains("2026"));
        assert!(table.contains("05-05"));
        assert!(!table.contains("2026-05-05"));
        assert!(table.contains("333,333"));
        assert!(table.contains("5,370,000"));
        assert!(table.contains("40,336,693,728"));
        assert!(!table.contains("Conv"));
        assert!(!table.contains("Reasoning"));
        assert!(!table.contains("Notes"));
        assert!(!table.contains("978.05K"));
    }

    #[test]
    fn daily_source_table_uses_lightweight_daily_columns() {
        let totals = TokenTotals {
            input_tokens: 978_050,
            cache_creation_tokens: 333_333,
            cache_read_tokens: 5_370_000,
            output_tokens: 40_330,
            reasoning_output_tokens: 12_000,
            total_tokens: 6_733_713,
            estimated_cost_usd: 3.5,
        };
        let row = DailyReportRow {
            date: "2026-05-05".to_string(),
            source: Some("codex".to_string()),
            project: None,
            totals: totals.clone(),
            models_used: vec![
                "claude-sonnet-4-20250514".to_string(),
                "gpt-5.4".to_string(),
            ],
            model_breakdowns: Vec::new(),
            conversation_count: 2,
            notes: ReportNotes {
                unpriced: false,
                reason_not_reported: true,
            },
        };

        let table = render_daily_source_table(&[row], Some(&totals));

        assert!(table.contains("Conv"));
        assert!(table.contains("Cache"));
        assert!(table.contains("Reason"));
        assert!(table.contains("All"));
        assert!(table.contains("Notes"));
        assert!(table.contains("reason not reported"));
        assert!(table.contains("978.05K"));
        assert!(table.contains("5.37M"));
        assert!(table.contains("sonnet-4, gpt-5.4") || table.contains("gpt-5.4, sonnet-4"));
        assert!(table.contains("TOTAL"));
    }

    #[test]
    fn daily_source_table_can_force_ansi_styles() {
        let totals = TokenTotals {
            input_tokens: 1,
            total_tokens: 1,
            ..TokenTotals::default()
        };
        let row = DailyReportRow {
            date: "2026-05-05".to_string(),
            source: Some("codex".to_string()),
            project: None,
            totals: totals.clone(),
            models_used: vec!["gpt-5.4".to_string()],
            model_breakdowns: Vec::new(),
            conversation_count: 1,
            notes: ReportNotes::default(),
        };

        let title = render_source_title(SourceKind::Codex, "Codex daily usage", ColorMode::Always);
        let table = render_daily_source_table_styled(
            SourceKind::Codex,
            &[row],
            Some(&totals),
            ColorMode::Always,
        );

        assert!(title.contains("\u{1b}["));
        assert!(table.contains("\u{1b}["));
        assert!(
            render_source_title(SourceKind::Codex, "Codex daily usage", ColorMode::Never)
                .contains("Codex daily usage")
        );
        assert!(
            !render_daily_source_table_styled(SourceKind::Codex, &[], None, ColorMode::Never)
                .contains("\u{1b}[")
        );
    }
}
