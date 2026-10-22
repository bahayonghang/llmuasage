//! Aligned, optionally colored rendering of the final `Sync finished` block.
//!
//! `format_summary_lines` is a pure function so alignment, coloring, and edge
//! cases (absent sources, errors, zero values) are unit-testable without a
//! terminal. Widths are computed on plain text first; ANSI styles are applied
//! only after alignment so escape sequences never skew column widths.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;

use crate::common::i18n::{self, Language, text};
use crate::sync::notices::RunNotice;
use console::Style;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    commands::sync::SyncSummary,
    models::{ParseIssueSample, SourceKind},
    parsers::SourceSyncStats,
    query::{
        ReportTimezone,
        reports::{self, TokenTotals},
    },
};

const HEADERS: [&str; 10] = [
    "SOURCE",
    "FILES",
    "CHANGED",
    "SKIPPED",
    "SEEN",
    "COMMITTED",
    "STORED",
    "BYTES",
    "PARSE",
    "WRITE",
];
const COMPACT_HEADERS: [&str; HEADERS.len()] = [
    "SRC", "F", "CHG", "SKIP", "SEEN", "COMMIT", "STORE", "BYTES", "PARSE", "WRITE",
];

/// Renders the full summary block: title, optional rebuild hint, the aligned
/// per-source table, and the aggregated `TOTAL` row.
///
/// `terminal_width` keeps the function pure and unit-testable: on a narrow
/// terminal only the SOURCE label column is shrunk (numeric columns are never
/// truncated), mirroring the convention in `src/tui/report_table.rs`.
#[cfg(test)]
pub(crate) fn format_summary_lines(
    summary: &SyncSummary,
    rebuild: bool,
    color: bool,
    terminal_width: usize,
) -> Vec<String> {
    format_summary_lines_with_basenames(summary, rebuild, color, terminal_width, &HashMap::new())
}

/// Like [`format_summary_lines`], with optional `path_hash → path` labels for
/// CLI samples. Only the basename is printed; full paths and path hashes stay
/// out of the summary.
#[cfg(test)]
pub(crate) fn format_summary_lines_with_basenames(
    summary: &SyncSummary,
    rebuild: bool,
    color: bool,
    terminal_width: usize,
    sample_basenames: &HashMap<String, String>,
) -> Vec<String> {
    format_summary_lines_with_notices(
        summary,
        rebuild,
        color,
        terminal_width,
        sample_basenames,
        &[],
    )
}

pub(crate) fn format_summary_lines_with_notices(
    summary: &SyncSummary,
    rebuild: bool,
    color: bool,
    terminal_width: usize,
    sample_basenames: &HashMap<String, String>,
    notices: &[RunNotice],
) -> Vec<String> {
    let rows: Vec<Row> = summary.sources.iter().map(Row::from_stats).collect();
    let total = total_cells(summary);
    let zh_headers = [
        "来源",
        "文件",
        "变更",
        "跳过",
        "读取事件",
        "新增事件",
        "库存事件",
        "读取量",
        "解析",
        "写入",
    ];
    let full_headers = if i18n::current() == Language::Chinese {
        &zh_headers
    } else {
        &HEADERS
    };
    let full_widths = column_widths(full_headers, &rows, &total);
    let compact = table_width(&full_widths, 3) > terminal_width;
    let headers = if compact && i18n::current() == Language::English {
        &COMPACT_HEADERS
    } else {
        full_headers
    };
    let gap = 3;
    let separator = " │ ";
    let mut widths = column_widths(headers, &rows, &total);
    fit_source_column(
        &mut widths,
        display_width(headers[0]).max(display_width(&total[0])),
        terminal_width,
        gap,
    );

    let mut lines = vec![styled(
        Style::new().bold(),
        text("Sync finished:", "同步完成："),
        color,
    )];
    if rebuild {
        lines.push(
            text(
                "- rebuild: reset parser-backed usage state source by source before sync",
                "- 重建：同步前按来源重置可重建的用量状态",
            )
            .into(),
        );
    }

    lines.push(border(&widths, '┌', '┬', '┐'));

    let header_line = headers
        .iter()
        .enumerate()
        .map(|(index, header)| pad(header, widths[index], Align::Left))
        .collect::<Vec<_>>()
        .join(separator);
    lines.push(styled(
        Style::new().dim(),
        &format!("│ {header_line} │"),
        color,
    ));
    lines.push(border(&widths, '├', '┼', '┤'));

    for (row, stats) in rows.iter().zip(summary.sources.iter()) {
        lines.push(format!(
            "│ {} │",
            render_row(row, stats, &widths, separator, color)
        ));
    }

    lines.push(border(&widths, '├', '┼', '┤'));
    lines.push(format!(
        "│ {} │",
        render_total_row(&total, &widths, separator, color)
    ));
    lines.push(border(&widths, '└', '┴', '┘'));
    lines.extend(diagnostic_lines(
        summary,
        color,
        sample_basenames,
        notices,
        false,
    ));
    lines
}

/// Per-source diagnostics in table order, then run-level notices.
///
/// With `merge_absent`, sources that are only "not detected" collapse into one
/// leading information line instead of one section each.
fn diagnostic_lines(
    summary: &SyncSummary,
    color: bool,
    sample_basenames: &HashMap<String, String>,
    notices: &[RunNotice],
    merge_absent: bool,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut not_detected = Vec::new();
    for stats in &summary.sources {
        let source_notices: Vec<_> = notices
            .iter()
            .filter(|notice| notice.source() == Some(stats.source))
            .collect();
        let has_detail = !source_notices.is_empty()
            || stats.last_error.is_some()
            || stats.parse_issues.summary_text().is_some();
        if merge_absent
            && stats.absent
            && source_notices.is_empty()
            && stats.parse_issues.summary_text().is_none()
        {
            not_detected.push(source_label(stats.source));
            continue;
        }
        if has_detail {
            lines.push(format!(
                "\n{} · {}",
                text("Diagnostics", "诊断"),
                stats.source
            ));
        }
        for notice in &source_notices {
            if matches!(notice, RunNotice::LegacySkipped { .. })
                && source_notices
                    .iter()
                    .any(|notice| matches!(notice, RunNotice::Coverage { .. }))
            {
                continue;
            }
            lines.extend(notice_lines(notice, color));
        }
        if stats.absent {
            lines.push(format!(
                "  {}",
                text(
                    "Information: source input is unavailable; no data imported.",
                    "信息提示：来源输入不可用，本轮未导入数据。"
                )
            ));
        } else if source_notices.is_empty()
            && let Some(error) = &stats.last_error
        {
            lines.push(styled(
                Style::new().dim(),
                &format!(
                    "  {}: {}",
                    text("Original detail", "原始详情"),
                    crate::common::i18n::logging::safe_text(error)
                ),
                color,
            ));
        }
        lines.extend(parse_issue_lines(stats, color, sample_basenames));
    }
    for notice in notices.iter().filter(|notice| notice.source().is_none()) {
        lines.extend(notice_lines(notice, color));
    }
    if !not_detected.is_empty() {
        lines.insert(
            0,
            styled(
                Style::new().dim(),
                &format!(
                    "
{}{}",
                    text("Not detected: ", "未检测到："),
                    not_detected.join(", ")
                ),
                color,
            ),
        );
    }
    lines
}

/// Which table the final summary shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryDetail {
    /// Default: last-7-days usage by date and source, read from the database.
    Usage,
    /// `--verbose`: the per-source pipeline table (files/events/bytes/timings).
    Pipeline,
}

/// Columns of the usage table; the two cache columns are dropped first when
/// the terminal is too narrow.
const USAGE_COLUMNS: usize = 8;
const SOURCE_COLUMN: usize = 1;
const CACHE_COLUMNS: [usize; 2] = [4, 5];

/// One source's totals on one day of the usage window.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceUsage {
    pub(crate) source: &'static str,
    pub(crate) totals: TokenTotals,
    pub(crate) unpriced: bool,
}

/// One calendar day of the usage window.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DayUsage {
    pub(crate) date: String,
    /// Sorted by estimated cost, highest first.
    pub(crate) rows: Vec<SourceUsage>,
}

/// Usage of the last seven local days (today included), newest day first.
/// Uses the same query, window and timezone as `llmusage daily`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WindowUsage {
    pub(crate) since: NaiveDate,
    pub(crate) until: NaiveDate,
    pub(crate) days: Vec<DayUsage>,
}

/// Reads the summary window after the sync released its worker lock.
pub(crate) fn load_window_usage(
    store: &crate::store::Store,
    source: Option<SourceKind>,
) -> anyhow::Result<WindowUsage> {
    let dashboard = crate::query::Dashboard::open(store)?;
    let mut filter = reports::ReportFilter {
        filter: crate::query::QueryFilter {
            source,
            timezone: ReportTimezone::Local,
            ..Default::default()
        },
        ..Default::default()
    };
    super::unified_report::apply_daily_default(&mut filter);
    let (Some(since), Some(until)) = (filter.since, filter.until) else {
        anyhow::bail!("daily default window was not applied");
    };
    let per_source = reports::load_daily_reports_by_source(dashboard.connection(), &filter)?;
    Ok(pivot_window(since, until, per_source))
}

fn pivot_window(
    since: NaiveDate,
    until: NaiveDate,
    per_source: Vec<(SourceKind, reports::DailyReport)>,
) -> WindowUsage {
    let mut by_date: BTreeMap<String, Vec<SourceUsage>> = BTreeMap::new();
    for (source, report) in per_source {
        for row in report.daily {
            by_date.entry(row.date).or_default().push(SourceUsage {
                source: source_label(source),
                totals: row.totals,
                unpriced: row.notes.unpriced,
            });
        }
    }
    let days = by_date
        .into_iter()
        .rev()
        .map(|(date, mut rows)| {
            rows.sort_by(|left, right| {
                right
                    .totals
                    .estimated_cost_usd
                    .total_cmp(&left.totals.estimated_cost_usd)
                    .then(right.totals.total_tokens.cmp(&left.totals.total_tokens))
            });
            DayUsage { date, rows }
        })
        .collect();
    WindowUsage { since, until, days }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn format_sync_report(
    summary: &SyncSummary,
    rebuild: bool,
    color: bool,
    terminal_width: usize,
    sample_basenames: &HashMap<String, String>,
    notices: &[RunNotice],
    detail: SummaryDetail,
    window: Option<&WindowUsage>,
) -> Vec<String> {
    match detail {
        SummaryDetail::Pipeline => format_summary_lines_with_notices(
            summary,
            rebuild,
            color,
            terminal_width,
            sample_basenames,
            notices,
        ),
        SummaryDetail::Usage => {
            let mut lines = vec![styled(
                Style::new().bold(),
                text("Sync finished:", "同步完成："),
                color,
            )];
            if rebuild {
                lines.push(
                    text(
                        "- rebuild: reset parser-backed usage state source by source before sync",
                        "- 重建：同步前按来源重置可重建的用量状态",
                    )
                    .into(),
                );
            }
            if let Some(window) = window {
                lines.extend(window_lines(window, color, terminal_width));
            }
            lines.push(styled(Style::new().dim(), &pipeline_line(summary), color));
            lines.extend(diagnostic_lines(
                summary,
                color,
                sample_basenames,
                notices,
                true,
            ));
            lines
        }
    }
}

/// Title, date × source table (or the empty-window line), and footnotes.
fn window_lines(window: &WindowUsage, color: bool, terminal_width: usize) -> Vec<String> {
    let (since, until) = (window.since.to_string(), window.until.to_string());
    let mut lines = vec![styled(
        Style::new().bold(),
        &if i18n::current() == Language::Chinese {
            format!("用量 · 最近 7 天（{since} → {until}，本地时间）")
        } else {
            format!("Usage · last 7 days ({since} → {until}, local time)")
        },
        color,
    )];
    if window.days.is_empty() {
        lines.push(text("No usage in the last 7 days.", "最近 7 天没有用量。").into());
    } else {
        let mut rows = Vec::new();
        let mut total = TokenTotals::default();
        let mut any_unpriced = false;
        for (index, day) in window.days.iter().enumerate() {
            if index > 0 {
                rows.push(TableRow::Rule);
            }
            let mut subtotal = TokenTotals::default();
            let mut day_unpriced = false;
            for (position, usage) in day.rows.iter().enumerate() {
                let date = if position == 0 { day.date.as_str() } else { "" };
                rows.push(TableRow::Data(usage_cells(
                    date,
                    usage.source,
                    &usage.totals,
                    usage.unpriced,
                )));
                add_totals(&mut subtotal, &usage.totals);
                day_unpriced |= usage.unpriced;
            }
            if day.rows.len() > 1 {
                rows.push(TableRow::Subtotal(usage_cells(
                    "",
                    text("(total)", "（小计）"),
                    &subtotal,
                    day_unpriced,
                )));
            }
            add_totals(&mut total, &subtotal);
            any_unpriced |= day_unpriced;
        }
        rows.push(TableRow::Rule);
        rows.push(TableRow::Total(usage_cells(
            text("TOTAL", "合计"),
            "",
            &total,
            any_unpriced,
        )));
        lines.extend(usage_table(&rows, color, terminal_width));
        if any_unpriced {
            lines.push(styled(
                Style::new().dim(),
                text(
                    "* Includes unpriced events; cost is a lower bound.",
                    "* 含未定价事件，费用为下限。",
                ),
                color,
            ));
        }
    }
    lines.push(styled(
        Style::new().dim(),
        text(
            "Other ranges: llmusage daily --since YYYYMMDD --until YYYYMMDD",
            "其他时间范围：llmusage daily --since YYYYMMDD --until YYYYMMDD",
        ),
        color,
    ));
    lines
}

fn add_totals(sum: &mut TokenTotals, add: &TokenTotals) {
    sum.input_tokens = sum.input_tokens.saturating_add(add.input_tokens);
    sum.cache_creation_tokens = sum
        .cache_creation_tokens
        .saturating_add(add.cache_creation_tokens);
    sum.cache_read_tokens = sum.cache_read_tokens.saturating_add(add.cache_read_tokens);
    sum.output_tokens = sum.output_tokens.saturating_add(add.output_tokens);
    sum.reasoning_output_tokens = sum
        .reasoning_output_tokens
        .saturating_add(add.reasoning_output_tokens);
    sum.total_tokens = sum.total_tokens.saturating_add(add.total_tokens);
    sum.estimated_cost_usd += add.estimated_cost_usd;
}

fn usage_cells(
    date: &str,
    source: &str,
    totals: &TokenTotals,
    unpriced: bool,
) -> [String; USAGE_COLUMNS] {
    let mut cost = format_usd(totals.estimated_cost_usd);
    if unpriced {
        cost.push('*');
    }
    [
        date.to_string(),
        source.to_string(),
        group_digits(totals.input_tokens),
        group_digits(totals.output_tokens),
        group_digits(totals.cache_creation_tokens),
        group_digits(totals.cache_read_tokens),
        group_digits(totals.total_tokens),
        cost,
    ]
}

enum TableRow {
    Data([String; USAGE_COLUMNS]),
    /// Per-day subtotal, shown only when a day has more than one source.
    Subtotal([String; USAGE_COLUMNS]),
    /// Separator between days and before the total.
    Rule,
    Total([String; USAGE_COLUMNS]),
}

fn usage_table(rows: &[TableRow], color: bool, terminal_width: usize) -> Vec<String> {
    let headers: [&str; USAGE_COLUMNS] = if i18n::current() == Language::Chinese {
        [
            "日期",
            "来源",
            "输入",
            "输出",
            "缓存写入",
            "缓存读取",
            "总 Token",
            "费用 (USD)",
        ]
    } else {
        [
            "Date",
            "Source",
            "Input",
            "Output",
            "Cache Create",
            "Cache Read",
            "Total Tokens",
            "Cost (USD)",
        ]
    };
    let mut widths: [usize; USAGE_COLUMNS] =
        std::array::from_fn(|index| display_width(headers[index]));
    for row in rows {
        if let TableRow::Data(cells) | TableRow::Subtotal(cells) | TableRow::Total(cells) = row {
            for (index, cell) in cells.iter().enumerate() {
                widths[index] = widths[index].max(display_width(cell));
            }
        }
    }
    let table_width = |columns: &[usize], widths: &[usize; USAGE_COLUMNS]| {
        columns
            .iter()
            .map(|index| widths[*index] + 3)
            .sum::<usize>()
            + 1
    };
    let mut columns: Vec<usize> = (0..USAGE_COLUMNS).collect();
    let natural = table_width(&columns, &widths);
    if natural > terminal_width {
        widths[SOURCE_COLUMN] = widths[SOURCE_COLUMN]
            .saturating_sub(natural - terminal_width)
            .max(display_width(headers[SOURCE_COLUMN]));
    }
    let cache_hidden = table_width(&columns, &widths) > terminal_width;
    if cache_hidden {
        columns.retain(|index| !CACHE_COLUMNS.contains(index));
    }

    let visible: Vec<usize> = columns.iter().map(|index| widths[*index]).collect();
    let rule = |left: char, middle: char, right: char| {
        styled(
            Style::new().dim(),
            &border(&visible, left, middle, right),
            color,
        )
    };
    let bar = styled(Style::new().dim(), "│", color);
    let line = |cells: Vec<String>| format!("{bar} {} {bar}", cells.join(&format!(" {bar} ")));
    let render = |cells: &[String; USAGE_COLUMNS], style: Option<Style>| {
        line(
            columns
                .iter()
                .map(|index| {
                    let padded = match *index {
                        0 => pad(&cells[0], widths[0], Align::Left),
                        SOURCE_COLUMN => pad(
                            &fit_label(&cells[SOURCE_COLUMN], widths[SOURCE_COLUMN]),
                            widths[SOURCE_COLUMN],
                            Align::Left,
                        ),
                        _ => pad(&cells[*index], widths[*index], Align::Right),
                    };
                    match &style {
                        Some(style) => styled(style.clone(), &padded, color),
                        None => padded,
                    }
                })
                .collect(),
        )
    };

    let mut lines = vec![rule('┌', '┬', '┐')];
    lines.push(line(
        columns
            .iter()
            .map(|index| {
                let align = if *index <= SOURCE_COLUMN {
                    Align::Left
                } else {
                    Align::Right
                };
                styled(
                    Style::new().cyan().bold(),
                    &pad(headers[*index], widths[*index], align),
                    color,
                )
            })
            .collect(),
    ));
    lines.push(rule('├', '┼', '┤'));
    for row in rows {
        lines.push(match row {
            TableRow::Data(cells) => render(cells, None),
            TableRow::Subtotal(cells) => render(cells, Some(Style::new().dim())),
            TableRow::Rule => rule('├', '┼', '┤'),
            TableRow::Total(cells) => render(cells, Some(Style::new().yellow().bold())),
        });
    }
    lines.push(rule('└', '┴', '┘'));
    if cache_hidden {
        lines.push(styled(
            Style::new().dim(),
            text(
                "Cache columns hidden on this narrow terminal; widen it or use llmusage daily.",
                "终端过窄，已隐藏缓存列；可加宽终端或使用 llmusage daily。",
            ),
            color,
        ));
    }
    lines
}

/// One-line pipeline digest under the usage table; `--verbose` shows the
/// per-source breakdown.
fn pipeline_line(summary: &SyncSummary) -> String {
    let sum = |value: fn(&SourceSyncStats) -> u64| {
        summary
            .sources
            .iter()
            .fold(0u64, |total, stats| total.saturating_add(value(stats)))
    };
    let files = sum(|stats| stats.files_processed as u64) as i64;
    let changed = sum(|stats| stats.changed_files as u64) as i64;
    let skipped = sum(|stats| stats.skipped_files as u64) as i64;
    let seen = sum(|stats| stats.events_seen as u64) as i64;
    let inserted = sum(|stats| stats.events_inserted as u64) as i64;
    let bytes = human_bytes(sum(|stats| stats.bytes_scanned));
    let parse = human_ms(sum(|stats| stats.parse_ms));
    let write = human_ms(sum(|stats| stats.write_ms));
    if i18n::current() == Language::Chinese {
        format!(
            "扫描 {} 个文件（变更 {}，跳过 {}）· 读取 {} 条事件 · 写入 {} 条 · {bytes} · 解析 {parse} · 写库 {write} · 明细：--verbose",
            group_digits(files),
            group_digits(changed),
            group_digits(skipped),
            group_digits(seen),
            group_digits(inserted),
        )
    } else {
        let plural = |count: i64, noun: &str| {
            format!(
                "{} {noun}{}",
                group_digits(count),
                if count == 1 { "" } else { "s" }
            )
        };
        format!(
            "{} scanned ({} changed, {} skipped) · {} seen · {} written · {bytes} · parse {parse} · write {write} · details: --verbose",
            plural(files, "file"),
            group_digits(changed),
            group_digits(skipped),
            plural(seen, "event"),
            group_digits(inserted),
        )
    }
}

/// `1234567` → `1,234,567`.
fn group_digits(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if value < 0 {
        grouped.push('-');
    }
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

fn format_usd(value: f64) -> String {
    let cents = (value * 100.0).round() as i64;
    format!("${}.{:02}", group_digits(cents / 100), (cents % 100).abs())
}

fn column_widths(
    headers: &[&str; HEADERS.len()],
    rows: &[Row],
    total: &[String; HEADERS.len()],
) -> [usize; HEADERS.len()] {
    let mut widths = std::array::from_fn(|index| display_width(headers[index]));
    for cells in rows
        .iter()
        .map(|row| &row.cells)
        .chain(std::iter::once(total))
    {
        for (index, cell) in cells.iter().enumerate() {
            widths[index] = widths[index].max(display_width(cell));
        }
    }
    widths
}

fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

fn border(widths: &[usize], left: char, middle: char, right: char) -> String {
    format!(
        "{left}{}{right}",
        widths
            .iter()
            .map(|width| "─".repeat(width + 2))
            .collect::<Vec<_>>()
            .join(&middle.to_string())
    )
}

pub(crate) fn notice_lines(notice: &RunNotice, color: bool) -> Vec<String> {
    let lines = crate::common::i18n::sync::notice_lines(notice, i18n::current());
    if matches!(
        notice,
        RunNotice::Coverage { .. }
            | RunNotice::SourceFailed {
                failure: crate::sync::notices::SourceFailure::Inventory { .. },
                ..
            }
    ) {
        return lines;
    }
    let style = if matches!(
        notice,
        RunNotice::SourceFailed { .. } | RunNotice::SourceIssue { .. }
    ) {
        Style::new().red()
    } else {
        Style::new().yellow()
    };
    lines
        .iter()
        .map(|line| styled(style.clone(), line, color))
        .collect()
}

fn table_width(widths: &[usize; HEADERS.len()], gap: usize) -> usize {
    widths.iter().sum::<usize>() + gap * (HEADERS.len() - 1) + 4
}

/// Aggregates every `TOTAL` cell from the per-source stats. This keeps the
/// terminal projection independent from the summary/event aggregate fields.
fn total_cells(summary: &SyncSummary) -> [String; HEADERS.len()] {
    let sum_usize = |value: fn(&SourceSyncStats) -> usize| {
        summary
            .sources
            .iter()
            .fold(0usize, |total, stats| total.saturating_add(value(stats)))
    };
    let sum_u64 = |value: fn(&SourceSyncStats) -> u64| {
        summary
            .sources
            .iter()
            .fold(0u64, |total, stats| total.saturating_add(value(stats)))
    };
    [
        text("TOTAL", "合计").to_string(),
        sum_usize(|stats| stats.files_processed).to_string(),
        sum_usize(|stats| stats.changed_files).to_string(),
        sum_usize(|stats| stats.skipped_files).to_string(),
        sum_usize(|stats| stats.events_seen).to_string(),
        sum_usize(|stats| stats.events_inserted).to_string(),
        sum_usize(|stats| stats.stored_events).to_string(),
        human_bytes(sum_u64(|stats| stats.bytes_scanned)),
        human_ms(sum_u64(|stats| stats.parse_ms)),
        human_ms(sum_u64(|stats| stats.write_ms)),
    ]
}

/// Shrinks only the SOURCE column when the natural table is wider than the
/// terminal. Numeric columns are never narrowed; if they alone still overflow
/// the numbers stay intact (accept overflow rather than corrupt values).
fn fit_source_column(
    widths: &mut [usize; HEADERS.len()],
    min_source: usize,
    terminal_width: usize,
    gap: usize,
) {
    let natural = table_width(widths, gap);
    if natural <= terminal_width {
        return;
    }
    let overflow = natural - terminal_width;
    widths[0] = widths[0].saturating_sub(overflow).max(min_source);
}

enum Align {
    Left,
    Right,
}

fn pad(text: &str, width: usize, align: Align) -> String {
    let len = display_width(text);
    if len >= width {
        return text.to_string();
    }
    let padding = " ".repeat(width - len);
    match align {
        Align::Left => format!("{text}{padding}"),
        Align::Right => format!("{padding}{text}"),
    }
}

fn styled(style: Style, text: &str, color: bool) -> String {
    if color {
        style.force_styling(true).apply_to(text).to_string()
    } else {
        text.to_string()
    }
}

pub(crate) fn parse_issue_lines(
    stats: &SourceSyncStats,
    color: bool,
    sample_basenames: &HashMap<String, String>,
) -> Vec<String> {
    let issues = &stats.parse_issues;
    if issues.summary_text().is_none() {
        return Vec::new();
    }
    let mut fault_parts = Vec::new();
    let mut information_parts = Vec::new();
    for (label, count) in issues.class_counts() {
        if count == 0 {
            continue;
        }
        let style = if matches!(label, "malformed" | "oversized") {
            Style::new().yellow()
        } else {
            Style::new().dim()
        };
        let human = match label {
            "malformed" => text("malformed", "格式错误"),
            "oversized" => text("oversized", "超长"),
            "skipped" => text("skipped", "跳过"),
            "accounting" => text("accounting", "计数提示"),
            _ => label,
        };
        let item = styled(style, &format!("{human}={count}"), color);
        if matches!(label, "malformed" | "oversized") {
            fault_parts.push(item);
        } else {
            information_parts.push(item);
        }
    }
    let prefix_style = if issues.total() > 0 {
        Style::new().yellow()
    } else {
        Style::new().dim()
    };
    let mut lines = Vec::new();
    if !fault_parts.is_empty() {
        lines.push(format!(
            "{}{}",
            styled(
                prefix_style,
                text("  Parse faults: ", "  解析故障："),
                color
            ),
            fault_parts.join(" ")
        ));
    }
    if !information_parts.is_empty() {
        lines.push(format!(
            "  {}{}",
            text("Information: ", "信息提示："),
            information_parts.join(" ")
        ));
    }
    let total = issues.total().saturating_add(issues.informational_total());
    let shown = issues
        .samples
        .len()
        .min(crate::domain::models::MAX_PARSE_ISSUE_SAMPLES);
    let omitted = total.saturating_sub(shown as u64);
    lines.push(if i18n::current() == Language::Chinese {
        format!("  展示样本：{shown}/{total}；另有 {omitted} 条未展示")
    } else {
        format!("  Samples shown: {shown}/{total}; {omitted} not shown")
    });
    for sample in issues
        .samples
        .iter()
        .take(crate::domain::models::MAX_PARSE_ISSUE_SAMPLES)
    {
        lines.push(styled(
            Style::new().dim(),
            &parse_issue_sample_line(sample, sample_basenames),
            color,
        ));
    }
    lines
}

fn parse_issue_sample_line(
    sample: &ParseIssueSample,
    sample_basenames: &HashMap<String, String>,
) -> String {
    let basename = sample_basenames
        .get(&sample.path_hash)
        .and_then(|raw| path_basename(raw));
    let kind = match sample.kind {
        crate::models::ParseIssueKind::Malformed => text("malformed", "格式错误（malformed）"),
        crate::models::ParseIssueKind::Oversized => text("oversized", "超长（oversized）"),
        crate::models::ParseIssueKind::Skipped => text("skipped", "跳过（skipped）"),
        crate::models::ParseIssueKind::AccountingAnomaly => {
            text("accounting_anomaly", "计数提示（accounting_anomaly）")
        }
    };
    let reason: String = sample
        .reason
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | ':' | '-')
        })
        .take(64)
        .collect();
    let mut line = format!("    {kind}");
    if !reason.is_empty() {
        line.push_str(&format!(" {reason}"));
    }
    if sample.offset > 0 && sample.source != SourceKind::Opencode {
        if sample.source == SourceKind::Zcode {
            line.push_str(&format!(" timestamp_ms={}", sample.offset));
        } else {
            line.push_str(&format!(" @{}", sample.offset));
        }
    }
    if let Some(name) = basename {
        line.push(' ');
        line.extend(
            name.chars()
                .filter(|character| !character.is_control())
                .take(255),
        );
    } else {
        line.push_str(text(" [location unavailable]", " [位置不可用]"));
    }
    line
}

pub(crate) fn path_basename(raw: &str) -> Option<&str> {
    raw.rsplit(['/', '\\'])
        .map(str::trim)
        .find(|name| !name.is_empty())
}

pub(crate) fn sample_basenames(
    store: &crate::store::Store,
    host: &str,
    sources: impl Iterator<Item = SourceKind>,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for source in sources {
        let Ok(cursors) = store.cursors().load_file_cursors(source, host) else {
            continue;
        };
        for cursor in cursors.into_values() {
            let raw = if cursor.file_path.is_empty() {
                cursor.cursor_key
            } else {
                cursor.file_path
            };
            if let Some(name) = path_basename(&raw) {
                map.insert(crate::util::hash_string(&raw), name.to_string());
            }
        }
    }
    map
}

struct Row {
    cells: [String; HEADERS.len()],
}

impl Row {
    fn from_stats(stats: &SourceSyncStats) -> Self {
        let numeric = |value: usize| {
            if stats.absent {
                "-".to_string()
            } else {
                value.to_string()
            }
        };
        Self {
            cells: [
                source_label(stats.source).to_string(),
                numeric(stats.files_processed),
                numeric(stats.changed_files),
                numeric(stats.skipped_files),
                numeric(stats.events_seen),
                numeric(stats.events_inserted),
                numeric(stats.stored_events),
                if stats.absent {
                    "-".into()
                } else {
                    human_bytes(stats.bytes_scanned)
                },
                if stats.absent {
                    "-".into()
                } else {
                    human_ms(stats.parse_ms)
                },
                if stats.absent {
                    "-".into()
                } else {
                    human_ms(stats.write_ms)
                },
            ],
        }
    }
}

fn render_row(
    row: &Row,
    stats: &SourceSyncStats,
    widths: &[usize],
    separator: &str,
    color: bool,
) -> String {
    row.cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let align = if index == 0 {
                Align::Left
            } else {
                Align::Right
            };
            // Only the SOURCE label may be truncated; numeric cells stay intact.
            let text = if index == 0 {
                fit_label(cell, widths[0])
            } else {
                cell.clone()
            };
            let padded = pad(&text, widths[index], align);
            match index {
                0 => styled(Style::new().bold(), &padded, color),
                // CHANGED
                2 if stats.changed_files > 0 => styled(Style::new().yellow(), &padded, color),
                // COMMITTED
                5 if stats.events_inserted > 0 => styled(Style::new().green(), &padded, color),
                _ => padded,
            }
        })
        .collect::<Vec<_>>()
        .join(separator)
}

/// Renders the aggregated `TOTAL` row through the same width/pad/align
/// machinery as source rows: bold label, right-aligned numeric cells, no
/// semantic accent colors and no separator glyph.
fn render_total_row(
    cells: &[String; HEADERS.len()],
    widths: &[usize],
    separator: &str,
    color: bool,
) -> String {
    cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let align = if index == 0 {
                Align::Left
            } else {
                Align::Right
            };
            let text = if index == 0 {
                fit_label(cell, widths[0])
            } else {
                cell.clone()
            };
            let padded = pad(&text, widths[index], align);
            if index == 0 {
                styled(Style::new().bold(), &padded, color)
            } else {
                padded
            }
        })
        .collect::<Vec<_>>()
        .join(separator)
}

/// Truncates an over-long display label to `width`, ending with `…`. Applied
/// only to the SOURCE column so numeric cells are never shortened.
fn fit_label(label: &str, width: usize) -> String {
    if display_width(label) <= width {
        return label.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut cells = 0;
    let mut truncated: String = label
        .chars()
        .take_while(|character| {
            cells += character.width().unwrap_or(0);
            cells < width
        })
        .collect();
    truncated.push('…');
    truncated
}

/// Table label for a source, read from the static descriptor so a new
/// `SourceKind` variant populates this display surface with no edit here.
fn source_label(source: SourceKind) -> &'static str {
    crate::registry::source_descriptor(source)
        .map_or_else(|| source.as_str(), |descriptor| descriptor.stable_id)
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn human_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wide terminal used by alignment assertions so natural widths are kept.
    const WIDE: usize = 200;

    #[test]
    fn parse_issue_samples_report_offsets_and_omitted_count() {
        let mut summary = summary();
        for offset in 1..=13 {
            summary.sources[0].parse_issues.record(
                SourceKind::Codex,
                "private-hash",
                offset,
                crate::models::ParseIssueKind::Skipped,
                "oversized_non_usage_record",
            );
        }
        let text = format_summary_lines(&summary, false, false, WIDE).join("\n");
        assert!(text.contains("oversized_non_usage_record @1"), "{text}");
        assert!(text.contains("5 not shown"), "{text}");
        assert!(!text.contains("@9"));
        assert!(!text.contains("private-hash"));
    }

    /// Strips ANSI SGR sequences so colored output can be compared to plain.
    fn strip_ansi(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' {
                for inner in chars.by_ref() {
                    if inner == 'm' {
                        break;
                    }
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn stats(source: SourceKind) -> SourceSyncStats {
        SourceSyncStats {
            source,
            files_processed: 1812,
            changed_files: 18,
            skipped_files: 1794,
            bytes_scanned: 7_800_000,
            events_seen: 104_070,
            events_inserted: 2_461,
            stored_events: 104_050,
            parse_ms: 4_100,
            write_ms: 350,
            ..SourceSyncStats::default()
        }
    }

    fn summary() -> SyncSummary {
        let mut claude = stats(SourceKind::Claude);
        claude.files_processed = 698;
        claude.events_inserted = 0;
        claude.changed_files = 0;
        let opencode = SourceSyncStats {
            source: SourceKind::Opencode,
            absent: true,
            last_error: Some("OpenCode SQLite DB 缺失".to_string()),
            ..SourceSyncStats::default()
        };
        SyncSummary {
            sources: vec![stats(SourceKind::Codex), claude, opencode],
            total_seen: 208_140,
            total_inserted: 2_461,
            stored_events: 208_100,
        }
    }

    #[test]
    fn colorless_output_contains_no_ansi_and_aligns_columns() {
        let lines = format_summary_lines(&summary(), false, false, WIDE);
        assert!(lines.iter().all(|line| !line.contains('\u{1b}')));
        let header = lines.iter().find(|line| line.contains("SOURCE")).unwrap();
        let codex = lines
            .iter()
            .find(|line| line.starts_with("│ codex"))
            .unwrap();
        // 右对齐数字列：列尾位置在 header 与数据行对齐。
        let column = header.find("COMMITTED").unwrap();
        let end = column + "COMMITTED".len() - 1;
        assert_eq!(header[end..].chars().next(), Some('D'));
        // codex committed=2461，列尾是个位数字 1。
        assert_eq!(&codex[end..end + 1], "1");
    }

    #[test]
    fn table_shows_human_bytes_and_durations() {
        let lines = format_summary_lines(&summary(), false, false, WIDE);
        let codex = lines
            .iter()
            .find(|line| line.starts_with("│ codex"))
            .unwrap();
        assert!(codex.contains("7.4 MB"), "{codex}");
        assert!(codex.contains("4.1s"), "{codex}");
        assert!(codex.contains("350ms"), "{codex}");
    }

    #[test]
    fn absent_source_uses_placeholders_and_error_line() {
        let lines = format_summary_lines(&summary(), false, false, WIDE);
        let opencode = lines
            .iter()
            .find(|line| line.starts_with("│ opencode"))
            .unwrap();
        assert!(opencode.contains('-'), "{opencode}");
        assert!(!opencode.contains("104070"), "{opencode}");
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Information: source input is unavailable"))
        );
    }

    #[test]
    fn parse_issue_counts_are_visible_without_sample_contents() {
        let mut summary = summary();
        summary.sources[0].parse_issues = crate::parsers::ParseIssues {
            malformed_lines: 2,
            oversized_lines: 1,
            skipped_lines: 3,
            accounting_anomaly_lines: 4,
            samples: vec![crate::parsers::ParseIssueSample {
                source: SourceKind::Codex,
                path_hash: "safe-path-hash".to_string(),
                offset: 42,
                kind: crate::parsers::ParseIssueKind::Malformed,
                reason: String::new(),
            }],
        };

        let lines = format_summary_lines(&summary, false, false, WIDE);
        let issue_line = lines
            .iter()
            .find(|line| line.contains("Parse faults:"))
            .expect("parse issue summary line");
        assert_eq!(issue_line, "  Parse faults: malformed=2 oversized=1");
        assert!(
            lines
                .iter()
                .any(|line| line.trim() == "malformed @42 [location unavailable]"),
            "samples should show kind and offset: {lines:?}"
        );
        assert!(!lines.iter().any(|line| line.contains("safe-path-hash")));
        assert!(!lines.iter().any(|line| line.contains("private")));
    }

    #[test]
    fn parse_issue_samples_print_basename_never_full_path() {
        let mut summary = summary();
        summary.sources[0].parse_issues = crate::parsers::ParseIssues {
            malformed_lines: 1,
            samples: vec![crate::parsers::ParseIssueSample {
                source: SourceKind::Codex,
                path_hash: "safe-path-hash".to_string(),
                offset: 42,
                kind: crate::parsers::ParseIssueKind::Malformed,
                reason: String::new(),
            }],
            ..crate::parsers::ParseIssues::default()
        };
        let mut basenames = HashMap::new();
        basenames.insert(
            "safe-path-hash".to_string(),
            r"C:\Users\alice\.codex\sessions\rollout-secret.jsonl".to_string(),
        );

        let lines = format_summary_lines_with_basenames(&summary, false, false, WIDE, &basenames);
        assert!(
            lines
                .iter()
                .any(|line| line.trim() == "malformed @42 rollout-secret.jsonl"),
            "basename should be appended when mapped: {lines:?}"
        );
        assert!(!lines.iter().any(|line| line.contains("safe-path-hash")));
        assert!(!lines.iter().any(|line| line.contains(r"C:\Users")));
        assert!(!lines.iter().any(|line| line.contains("alice")));
        assert!(!lines.iter().any(|line| line.contains("sessions")));
    }

    #[test]
    fn parse_issue_samples_print_reason_without_at_zero() {
        let mut summary = summary();
        summary.sources[0].parse_issues = crate::parsers::ParseIssues {
            skipped_lines: 1,
            samples: vec![crate::parsers::ParseIssueSample {
                source: SourceKind::Zcode,
                path_hash: "zcode-hash".to_string(),
                offset: 0,
                kind: crate::parsers::ParseIssueKind::Skipped,
                reason: "zcode_unfinished:error:invalid_request".to_string(),
            }],
            ..crate::parsers::ParseIssues::default()
        };

        let lines = format_summary_lines(&summary, false, false, WIDE);
        assert!(
            lines.iter().any(|line| line.trim()
                == "skipped zcode_unfinished:error:invalid_request [location unavailable]"),
            "reason samples should print kind + reason: {lines:?}"
        );
        assert!(!lines.iter().any(|line| line.contains("@0")));
        assert!(!lines.iter().any(|line| line.contains("zcode-hash")));
    }

    #[test]
    fn skipped_only_parse_issues_are_not_warning_colored() {
        let mut summary = summary();
        summary.sources[0].parse_issues = crate::parsers::ParseIssues {
            skipped_lines: 17,
            ..crate::parsers::ParseIssues::default()
        };
        let colored = format_summary_lines(&summary, false, true, WIDE);
        let issue_line = colored
            .iter()
            .find(|line| strip_ansi(line).contains("Information:"))
            .expect("parse issue summary line");
        let skipped = styled(Style::new().dim(), "skipped=17", true);
        let yellow_skipped = styled(Style::new().yellow(), "skipped=17", true);
        assert!(
            issue_line.contains(&skipped),
            "skipped counts should use dim style: {issue_line}"
        );
        assert!(
            !issue_line.contains(&yellow_skipped),
            "skipped counts must not use warning yellow: {issue_line}"
        );
        let yellow_prefix = styled(Style::new().yellow(), "  parse issues: ", true);
        assert!(
            !issue_line.contains(&yellow_prefix),
            "skipped-only prefix must not use warning yellow: {issue_line}"
        );
    }

    #[test]
    fn total_row_replaces_legacy_totals_line() {
        let lines = format_summary_lines(&summary(), true, false, WIDE);
        assert_eq!(lines[0], "Sync finished:");
        assert!(lines.iter().any(|line| line.starts_with("- rebuild:")));
        // The standalone `- totals:` line is gone; a TOTAL table row replaces it.
        assert!(!lines.iter().any(|line| line.starts_with("- totals:")));
        let total = lines
            .iter()
            .find(|line| line.starts_with("│ TOTAL"))
            .expect("TOTAL row present");
        assert!(total.contains("208140"), "{total}");
        assert!(total.contains("208100"), "{total}");
        // FILES sums non-absent sources: codex 1812 + claude 698 (opencode absent).
        assert!(total.contains("2510"), "{total}");
        // SKIPPED sums non-absent sources: 1794 + 1794.
        assert!(total.contains("3588"), "{total}");
    }

    #[test]
    fn total_row_aggregates_source_stats() {
        let lines = format_summary_lines(&summary(), false, false, WIDE);
        let total = lines
            .iter()
            .find(|line| line.starts_with("│ TOTAL"))
            .expect("TOTAL row present");
        // BYTES / PARSE / WRITE aggregate the source rows.
        assert!(total.contains("14.9 MB"), "{total}"); // 7.8M + 7.8M bytes
        assert!(total.contains("8.2s"), "{total}"); // 4100ms + 4100ms
        assert!(total.contains("700ms"), "{total}"); // 350ms + 350ms
        // The TOTAL row carries no ANSI in colorless mode.
        assert!(!total.contains('\u{1b}'), "{total}");

        let mut inconsistent = summary();
        inconsistent.total_seen = usize::MAX;
        inconsistent.total_inserted = usize::MAX;
        inconsistent.stored_events = usize::MAX;
        let cells = total_cells(&inconsistent);
        assert_eq!(cells[4], "208140");
        assert_eq!(cells[5], "2461");
        assert_eq!(cells[6], "208100");
    }

    #[test]
    fn narrow_width_truncates_only_source_label() {
        // A single long-labelled source ("antigravity") forces the SOURCE column
        // to shrink on a narrow terminal.
        let summary = SyncSummary {
            sources: vec![stats(SourceKind::Antigravity)],
            total_seen: 179_317,
            total_inserted: 134_015,
            stored_events: 134_015,
        };
        let lines = format_summary_lines(&summary, false, false, 60);
        let row = lines
            .iter()
            .find(|line| line.contains('…'))
            .expect("long source label truncates with an ellipsis");
        assert!(row.starts_with("│ anti"), "{row}");
        // Numeric cells are never truncated even when the label is.
        assert!(row.contains("104070"), "{row}"); // events_seen
        assert!(row.contains("2461"), "{row}"); // events_inserted
        let total = lines
            .iter()
            .find(|line| strip_ansi(line).starts_with("│ TOTAL"))
            .expect("TOTAL row present");
        assert!(total.contains("104070"), "{total}");
        assert!(total.contains("104050"), "{total}");
        let table_lines = lines.iter().filter(|line| {
            line.contains("SRC") || line.contains('…') || line.starts_with("│ TOTAL")
        });
        assert!(
            table_lines
                .clone()
                .all(|line| display_width(line) == display_width(row)),
            "{}",
            table_lines.cloned().collect::<Vec<_>>().join("\n")
        );
        let header = lines.iter().find(|line| line.contains("SRC")).unwrap();
        assert!(header.contains("CHG"), "{header}");
        assert!(!header.contains("CHANGED"), "{header}");
    }

    #[test]
    fn empty_summary_still_renders_zero_total_row() {
        let summary = SyncSummary {
            sources: Vec::new(),
            total_seen: 99,
            total_inserted: 99,
            stored_events: 99,
        };
        let lines = format_summary_lines(&summary, false, false, 60);
        let total = lines
            .iter()
            .find(|line| line.starts_with("│ TOTAL"))
            .expect("TOTAL row present");
        assert_eq!(
            total
                .split('│')
                .flat_map(str::split_whitespace)
                .collect::<Vec<_>>(),
            vec![
                "TOTAL", "0", "0", "0", "0", "0", "0", "0", "B", "0ms", "0ms"
            ]
        );
    }

    #[test]
    fn colored_output_wraps_but_preserves_alignment_math() {
        let plain = format_summary_lines(&summary(), false, false, WIDE);
        let colored = format_summary_lines(&summary(), false, true, WIDE);
        assert!(colored.iter().any(|line| line.contains('\u{1b}')));
        // 着色只包裹内容，不改变去色后的文本。
        for (plain_line, colored_line) in plain.iter().zip(colored.iter()) {
            assert_eq!(*plain_line, strip_ansi(colored_line));
        }
    }

    #[test]
    fn colored_total_row_matches_plain_after_strip() {
        let plain = format_summary_lines(&summary(), false, false, WIDE);
        let colored = format_summary_lines(&summary(), false, true, WIDE);
        let plain_total = plain
            .iter()
            .find(|line| line.starts_with("│ TOTAL"))
            .expect("plain TOTAL row");
        let colored_total = colored
            .iter()
            .find(|line| strip_ansi(line).starts_with("│ TOTAL"))
            .expect("colored TOTAL row");
        assert!(colored_total.contains('\u{1b}'), "{colored_total}");
        assert_eq!(*plain_total, strip_ansi(colored_total));
    }

    #[test]
    fn zero_and_large_values_format_safely() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(917_506_048), "875.0 MB");
        assert_eq!(human_ms(0), "0ms");
        assert_eq!(human_ms(999), "999ms");
        assert_eq!(human_ms(102_100), "102.1s");
    }

    #[test]
    fn diagnostics_and_samples_are_strictly_after_total() {
        let mut summary = summary();
        summary.sources[0].parse_issues = crate::parsers::ParseIssues {
            malformed_lines: 1,
            samples: vec![crate::parsers::ParseIssueSample {
                source: SourceKind::Codex,
                path_hash: "safe-hash".to_string(),
                offset: 12,
                kind: crate::parsers::ParseIssueKind::Malformed,
                reason: String::new(),
            }],
            ..Default::default()
        };
        let lines = format_summary_lines(&summary, false, false, WIDE);
        let header_idx = lines
            .iter()
            .position(|l| l.contains("SOURCE"))
            .expect("header");
        let total_idx = lines
            .iter()
            .position(|l| l.starts_with("│ TOTAL"))
            .expect("total");

        // Between header and TOTAL, there are only the source table rows.
        let table_data_lines = &lines[header_idx + 1..total_idx];
        assert_eq!(table_data_lines.len(), summary.sources.len() + 2);
        for line in table_data_lines {
            assert!(!line.contains('↳'));
            assert!(!line.contains("Parse faults:"));
        }

        // Diagnostics appear after TOTAL.
        let post_total_lines = &lines[total_idx + 1..];
        assert!(
            post_total_lines
                .iter()
                .any(|l| l.contains("Information: source input is unavailable"))
        );
        assert!(post_total_lines.iter().any(|l| l.contains("Parse faults:")));
        assert!(post_total_lines.iter().any(|l| l.contains("malformed @12")));
    }

    #[test]
    fn bilingual_table_has_closed_equal_width_borders_and_exact_numeric_cells() {
        let mut fixture = summary();
        fixture.sources[0].events_seen = usize::MAX;
        fixture.sources[1].events_seen = usize::MAX;
        fixture.sources[0].bytes_scanned = u64::MAX;
        fixture.sources[1].bytes_scanned = u64::MAX;
        for language in [Language::English, Language::Chinese] {
            i18n::with_language(language, || {
                let lines = format_summary_lines(&fixture, false, false, 60);
                let top = lines.iter().position(|line| line.starts_with('┌')).unwrap();
                let bottom = lines.iter().position(|line| line.starts_with('└')).unwrap();
                let width = display_width(&lines[top]);
                for line in &lines[top..=bottom] {
                    assert_eq!(display_width(line), width, "{line}");
                    assert!(!line.contains('\u{1b}'));
                }
                assert_eq!(lines.iter().filter(|line| line.starts_with('├')).count(), 2);
                let data: Vec<_> = lines[top..=bottom]
                    .iter()
                    .filter(|line| line.starts_with('│'))
                    .collect();
                for row in &data {
                    assert_eq!(row.matches('│').count(), 11);
                }
                let codex: Vec<_> = data[1].split('│').skip(1).take(10).map(str::trim).collect();
                assert_eq!(codex[4], usize::MAX.to_string());
                let total: Vec<_> = data
                    .last()
                    .unwrap()
                    .split('│')
                    .skip(1)
                    .take(10)
                    .map(str::trim)
                    .collect();
                assert_eq!(total[0], language.select("TOTAL", "合计"));
                assert_eq!(total[4], usize::MAX.to_string());
                assert_eq!(total[7], human_bytes(u64::MAX));
                let absent: Vec<_> = data[3].split('│').skip(1).take(10).map(str::trim).collect();
                assert!(absent[1..].iter().all(|value| *value == "-"));
            });
        }
    }

    #[test]
    fn bilingual_diagnostics_are_source_grouped_capped_and_separate_faults_from_information() {
        let mut fixture = summary();
        fixture.sources[0].parse_issues.malformed_lines = 2;
        for offset in 1..=14 {
            fixture.sources[0].parse_issues.record(
                SourceKind::Codex,
                "private-path-hash",
                offset,
                crate::models::ParseIssueKind::AccountingAnomaly,
                "usage_incomplete",
            );
        }
        let mut basenames = HashMap::new();
        basenames.insert(
            "private-path-hash".into(),
            "C:/private/source/updates.jsonl".into(),
        );
        for language in [Language::English, Language::Chinese] {
            i18n::with_language(language, || {
                let lines =
                    format_summary_lines_with_basenames(&fixture, false, false, 160, &basenames);
                let text = lines.join("\n");
                let bottom = lines.iter().position(|line| line.starts_with('└')).unwrap();
                let diagnostic = lines
                    .iter()
                    .position(|line| {
                        line.contains(&format!(
                            "{} · codex",
                            language.select("Diagnostics", "诊断")
                        ))
                    })
                    .unwrap();
                assert!(diagnostic > bottom);
                assert!(text.contains(
                    language.select("Parse faults: malformed=2", "解析故障：格式错误=2")
                ));
                assert!(text.contains(
                    language.select("Information: accounting=14", "信息提示：计数提示=14")
                ));
                assert!(text.contains("8/16"));
                assert_eq!(text.matches("usage_incomplete").count(), 8);
                assert!(
                    !text.contains("private-path-hash")
                        && !text.contains("C:/private")
                        && !text.contains("@9")
                );
                assert!(text.contains("updates.jsonl"));
            });
        }
    }

    #[test]
    fn paired_inventory_diagnostics_keep_cap_counts_and_additional_unknown_detail() {
        use crate::parsers::feedback::{InventoryDiagnostic, InventoryIssue};
        let mut fixture = summary();
        fixture.sources[0].last_error = Some("canonical inventory error wrapper".into());
        let notice = RunNotice::SourceFailed {
            source: SourceKind::Codex,
            failure: crate::sync::notices::SourceFailure::Inventory {
                diagnostic: InventoryDiagnostic {
                    samples: vec![
                        InventoryIssue::RootNotDirectory,
                        InventoryIssue::ExternalIo {
                            detail: "Permission denied".into(),
                        },
                        InventoryIssue::Unknown {
                            detail: "extra custom fact".into(),
                        },
                    ],
                    total: 7,
                    omitted: 4,
                },
            },
        };
        for language in [Language::English, Language::Chinese] {
            i18n::with_language(language, || {
                let lines = format_summary_lines_with_notices(
                    &fixture,
                    false,
                    false,
                    160,
                    &HashMap::new(),
                    std::slice::from_ref(&notice),
                );
                let text = lines.join("\n");
                assert!(text.contains("3/7") && text.contains('4'));
                assert!(text.contains(
                    language.select("Source root is not a directory", "来源根路径不是目录")
                ));
                assert!(text.contains("Permission denied") && text.contains("extra custom fact"));
                assert!(!text.contains("canonical inventory error wrapper"));
                assert_eq!(
                    fixture.sources[0].last_error.as_deref(),
                    Some("canonical inventory error wrapper")
                );
            });
        }
    }

    #[test]
    fn typed_coverage_preserves_fault_protection_and_never_guesses_from_last_error() {
        let fixture = summary();
        let coverage = crate::parsers::antigravity::AntigravityProductCoverage {
            source: SourceKind::Codex,
            discovered_count: 4,
            new_files_count: 1,
            tracked_count: 5,
            stored_events: 99,
            missing_count: 2,
            out_of_scope_count: 3,
            unreadable_count: 1,
            discovery_incomplete: true,
        };
        for language in [Language::English, Language::Chinese] {
            i18n::with_language(language, || {
                let notices = [RunNotice::Coverage {
                    source: SourceKind::Codex,
                    coverage: coverage.clone(),
                    interactive: true,
                    windowed: false,
                    kept: false,
                }];
                let lines = format_summary_lines_with_notices(
                    &fixture,
                    false,
                    false,
                    160,
                    &HashMap::new(),
                    &notices,
                );
                let text = lines.join("\n");
                assert!(text.contains(language.select("still present outside", "仍存在但超出")));
                assert!(text.contains(language.select("Unreadable tracked", "不可读的已跟踪")));
                assert!(!text.contains("--allow-lossy-rebuild"));
            });
        }
    }

    fn totals(input: i64, output: i64, cost: f64) -> TokenTotals {
        TokenTotals {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 1_000_000,
            total_tokens: input + output + 1_000_000,
            estimated_cost_usd: cost,
            ..TokenTotals::default()
        }
    }

    fn day(date: &str, rows: Vec<(&'static str, TokenTotals, bool)>) -> DayUsage {
        DayUsage {
            date: date.to_string(),
            rows: rows
                .into_iter()
                .map(|(source, totals, unpriced)| SourceUsage {
                    source,
                    totals,
                    unpriced,
                })
                .collect(),
        }
    }

    fn window(days: Vec<DayUsage>) -> WindowUsage {
        WindowUsage {
            since: NaiveDate::from_ymd_opt(2026, 10, 16).unwrap(),
            until: NaiveDate::from_ymd_opt(2026, 10, 22).unwrap(),
            days,
        }
    }

    fn sample_window() -> WindowUsage {
        window(vec![
            day(
                "2026-10-22",
                vec![
                    ("claude", totals(6_282, 2_407_747, 21.10), false),
                    ("codex", totals(12_000, 1_500, 4.20), false),
                ],
            ),
            day("2026-10-21", vec![("grok", totals(3_900, 800, 0.0), true)]),
        ])
    }

    fn usage_report(window: Option<&WindowUsage>, color: bool, width: usize) -> Vec<String> {
        format_sync_report(
            &summary(),
            false,
            color,
            width,
            &HashMap::new(),
            &[],
            SummaryDetail::Usage,
            window,
        )
    }

    #[test]
    fn window_table_groups_days_with_subtotals_and_total() {
        i18n::with_language(Language::English, || {
            let lines = usage_report(Some(&sample_window()), false, WIDE);
            let text = lines.join("\n");
            assert!(
                text.contains("Usage · last 7 days (2026-10-16 → 2026-10-22, local time)"),
                "{text}"
            );
            let rows: Vec<&String> = lines.iter().filter(|line| line.starts_with('│')).collect();
            // header, claude, codex, subtotal, grok (no subtotal), TOTAL
            assert_eq!(rows.len(), 6, "{text}");
            assert!(
                rows[1].contains("2026-10-22") && rows[1].contains("claude"),
                "{text}"
            );
            assert!(
                !rows[2].contains("2026-10-22") && rows[2].contains("codex"),
                "{text}"
            );
            assert!(
                rows[3].contains("(total)") && rows[3].contains("$25.30"),
                "{text}"
            );
            assert!(
                rows[4].contains("2026-10-21") && rows[4].contains("$0.00*"),
                "{text}"
            );
            assert!(
                rows[5].contains("TOTAL") && rows[5].contains("$25.30*"),
                "{text}"
            );
            assert!(rows[5].contains("2,410,047"), "{text}");
            assert!(text.contains("* Includes unpriced events"), "{text}");
            assert!(text.contains("llmusage daily --since"), "{text}");
            assert!(text.contains("2,461 written"), "{text}");
            assert!(!text.contains("No new usage"), "{text}");
            assert!(text.contains("Not detected: opencode"), "{text}");
            let widths: Vec<_> = lines
                .iter()
                .filter(|line| line.starts_with(['│', '┌', '├', '└']))
                .map(|line| display_width(line))
                .collect();
            assert!(widths.windows(2).all(|pair| pair[0] == pair[1]), "{text}");
        });
    }

    #[test]
    fn window_table_is_bilingual_and_colored_output_strips_to_plain() {
        for language in [Language::English, Language::Chinese] {
            i18n::with_language(language, || {
                let plain = usage_report(Some(&sample_window()), false, WIDE);
                let colored = usage_report(Some(&sample_window()), true, WIDE);
                assert!(plain.iter().all(|line| !line.contains('\u{1b}')));
                assert!(colored.iter().any(|line| line.contains('\u{1b}')));
                assert_eq!(
                    colored
                        .iter()
                        .map(|line| strip_ansi(line))
                        .collect::<Vec<_>>(),
                    plain
                );
                let text = plain.join("\n");
                assert!(text.contains(language.select("Date", "日期")), "{text}");
                assert!(
                    text.contains(language.select("(total)", "（小计）")),
                    "{text}"
                );
                assert!(text.contains(language.select("TOTAL", "合计")), "{text}");
            });
        }
    }

    #[test]
    fn empty_window_prints_no_table() {
        i18n::with_language(Language::English, || {
            let text = usage_report(Some(&window(Vec::new())), false, WIDE).join("\n");
            assert!(text.contains("No usage in the last 7 days."), "{text}");
            assert!(!text.contains('┌'), "{text}");
        });
    }

    #[test]
    fn failed_window_query_keeps_digest_and_diagnostics() {
        i18n::with_language(Language::English, || {
            let text = usage_report(None, false, WIDE).join("\n");
            assert!(text.starts_with("Sync finished:"), "{text}");
            assert!(!text.contains("Usage ·") && !text.contains('┌'), "{text}");
            assert!(text.contains("written"), "{text}");
            assert!(text.contains("Not detected: opencode"), "{text}");
        });
    }

    #[test]
    fn narrow_window_table_hides_cache_columns_but_keeps_numbers() {
        i18n::with_language(Language::English, || {
            let lines = usage_report(Some(&sample_window()), false, 80);
            let text = lines.join("\n");
            assert!(!text.contains("Cache Read"), "{text}");
            assert!(text.contains("Cache columns hidden"), "{text}");
            assert!(
                text.contains("2,407,747") && text.contains("$25.30"),
                "{text}"
            );
            assert!(
                lines
                    .iter()
                    .filter(|line| line.starts_with('│'))
                    .all(|line| display_width(line) <= 80),
                "{text}"
            );
        });
    }

    #[test]
    fn pivot_orders_days_newest_first_and_sources_by_cost() {
        let report = |date: &str, cost: f64| reports::DailyReport {
            daily: vec![reports::DailyReportRow {
                date: date.to_string(),
                source: None,
                project: None,
                totals: totals(1, 1, cost),
                models_used: Vec::new(),
                model_breakdowns: Vec::new(),
                conversation_count: 0,
                notes: reports::ReportNotes::default(),
            }],
            totals: TokenTotals::default(),
        };
        let since = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        let until = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let window = pivot_window(
            since,
            until,
            vec![
                (SourceKind::Codex, report("2026-10-20", 1.0)),
                (SourceKind::Claude, report("2026-10-20", 3.0)),
                (SourceKind::Grok, report("2026-10-22", 0.5)),
            ],
        );
        let order: Vec<(&str, Vec<&str>)> = window
            .days
            .iter()
            .map(|day| {
                (
                    day.date.as_str(),
                    day.rows.iter().map(|row| row.source).collect(),
                )
            })
            .collect();
        assert_eq!(
            order,
            vec![
                ("2026-10-22", vec!["grok"]),
                ("2026-10-20", vec!["claude", "codex"]),
            ]
        );
    }

    #[test]
    fn verbose_detail_keeps_the_pipeline_table() {
        i18n::with_language(Language::English, || {
            let verbose = format_sync_report(
                &summary(),
                false,
                false,
                WIDE,
                &HashMap::new(),
                &[],
                SummaryDetail::Pipeline,
                Some(&sample_window()),
            );
            assert_eq!(
                verbose,
                format_summary_lines(&summary(), false, false, WIDE)
            );
        });
    }

    #[test]
    fn digits_and_dollars_group_thousands() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1_000), "1,000");
        assert_eq!(group_digits(1_242_330_919), "1,242,330,919");
        assert_eq!(group_digits(-12_345), "-12,345");
        assert_eq!(format_usd(0.0), "$0.00");
        assert_eq!(format_usd(330.717_217), "$330.72");
        assert_eq!(format_usd(12_345.6), "$12,345.60");
    }
}
