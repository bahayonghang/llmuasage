use ratatui::{
    layout::Constraint,
    widgets::{Cell, Row},
};

use crate::tui::{
    app::{SortState, TableSortKey},
    format::{cache_multiplier, cost_compact, cost_per_million, stat_compact},
    theme,
};

#[derive(Clone, Copy)]
pub(crate) enum WidthTier {
    VeryNarrow,
    Narrow,
    Wide,
}

pub(crate) fn width_tier(width: u16) -> WidthTier {
    if width < 60 {
        WidthTier::VeryNarrow
    } else if width < 80 {
        WidthTier::Narrow
    } else {
        WidthTier::Wide
    }
}

pub(crate) struct PeriodValues {
    pub turn_count: i64,
    pub event_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
    pub cost_with_cache_usd: f64,
}

pub(crate) fn period_has_turns(turns: impl IntoIterator<Item = i64>) -> bool {
    turns.into_iter().any(|count| count > 0)
}

pub(crate) fn turn_cell(count: i64) -> String {
    if count > 0 {
        stat_compact(count)
    } else {
        "—".to_string()
    }
}

pub(crate) fn period_header_labels(
    time_label: &str,
    extra_after_time: &[&str],
    has_turns: bool,
    tier: WidthTier,
    sort: SortState,
) -> Vec<String> {
    match tier {
        WidthTier::VeryNarrow => vec![
            sort.header(time_label, TableSortKey::Date),
            sort.header("Cost", TableSortKey::Cost),
        ],
        WidthTier::Narrow => {
            let mut labels = vec![sort.header(time_label, TableSortKey::Date)];
            labels.extend(extra_after_time.iter().map(|label| (*label).to_string()));
            if has_turns {
                labels.push("Turn".to_string());
            }
            labels.extend([
                "Msgs".to_string(),
                sort.header("Tokens", TableSortKey::Tokens),
                sort.header("Cost", TableSortKey::Cost),
            ]);
            labels
        }
        WidthTier::Wide => {
            let mut labels = vec![sort.header(time_label, TableSortKey::Date)];
            labels.extend(extra_after_time.iter().map(|label| (*label).to_string()));
            if has_turns {
                labels.push("Turn".to_string());
            }
            labels.extend([
                "Msgs".to_string(),
                "Input".to_string(),
                "Output".to_string(),
                "Cache R".to_string(),
                "Cache W".to_string(),
                "Cache×".to_string(),
                sort.header("Total", TableSortKey::Tokens),
                sort.header("Cost", TableSortKey::Cost),
                "Cost/1M".to_string(),
            ]);
            labels
        }
    }
}

pub(crate) fn period_header_row(
    time_label: &str,
    extra_after_time: &[&str],
    has_turns: bool,
    tier: WidthTier,
    sort: SortState,
) -> Row<'static> {
    Row::new(
        period_header_labels(time_label, extra_after_time, has_turns, tier, sort)
            .into_iter()
            .map(Cell::from)
            .collect::<Vec<_>>(),
    )
    .style(theme::header_style())
    .bottom_margin(1)
}

pub(crate) fn period_metric_cells(
    values: &PeriodValues,
    extra_after_time: Vec<Cell<'static>>,
    has_turns: bool,
    tier: WidthTier,
) -> Vec<Cell<'static>> {
    let cost = Cell::from(cost_compact(values.cost_with_cache_usd))
        .style(theme::fg_style(theme::positive_fg()));
    match tier {
        WidthTier::VeryNarrow => vec![cost],
        WidthTier::Narrow => {
            let mut cells = extra_after_time;
            if has_turns {
                cells.push(Cell::from(turn_cell(values.turn_count)));
            }
            cells.extend([
                Cell::from(stat_compact(values.event_count)),
                Cell::from(stat_compact(values.total_tokens)),
                cost,
            ]);
            cells
        }
        WidthTier::Wide => {
            let mut cells = extra_after_time;
            if has_turns {
                cells.push(Cell::from(turn_cell(values.turn_count)));
            }
            cells.extend([
                Cell::from(stat_compact(values.event_count)),
                Cell::from(stat_compact(values.input_tokens))
                    .style(theme::fg_style(theme::metric_input())),
                Cell::from(stat_compact(values.output_tokens))
                    .style(theme::fg_style(theme::metric_output())),
                Cell::from(stat_compact(values.cache_read_tokens))
                    .style(theme::fg_style(theme::metric_cache_read())),
                Cell::from(stat_compact(values.cache_creation_tokens))
                    .style(theme::fg_style(theme::metric_cache_write())),
                Cell::from(cache_multiplier(
                    values.cache_read_tokens,
                    values.input_tokens,
                    values.cache_creation_tokens,
                ))
                .style(theme::fg_style(theme::metric_cache_hit())),
                Cell::from(stat_compact(values.total_tokens)),
                cost,
                Cell::from(cost_per_million(
                    values.cost_with_cache_usd,
                    values.total_tokens,
                ))
                .style(theme::fg_style(theme::metric_cost_per_million())),
            ]);
            cells
        }
    }
}

pub(crate) fn period_widths(
    extra_after_time: usize,
    has_turns: bool,
    tier: WidthTier,
    time_width: u16,
) -> Vec<Constraint> {
    match tier {
        WidthTier::VeryNarrow => vec![Constraint::Percentage(60), Constraint::Percentage(40)],
        WidthTier::Narrow => {
            let cols = 1 + extra_after_time + usize::from(has_turns) + 3;
            vec![Constraint::Percentage((100 / cols as u16).max(1)); cols]
        }
        WidthTier::Wide => {
            let mut widths = vec![Constraint::Length(time_width)];
            widths.extend(std::iter::repeat_n(
                Constraint::Length(14),
                extra_after_time,
            ));
            if has_turns {
                widths.push(Constraint::Length(6));
            }
            widths.extend([
                Constraint::Length(6),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
            ]);
            widths
        }
    }
}

pub(crate) fn compact_date(date: &str) -> String {
    if date.len() >= 10 && date.as_bytes().get(4) == Some(&b'-') {
        date.chars().skip(5).take(5).collect()
    } else {
        date.chars().take(8).collect()
    }
}

pub(crate) fn display_date(date: &str, compact: bool) -> String {
    if compact {
        compact_date(date)
    } else {
        date.to_string()
    }
}
