use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::query::DailyTrendPoint;
use crate::tui::{
    format::{cost as format_cost, stat_compact},
    theme,
};

use super::super::app::{ScrollState, SortState, TableSortKey, stable_sort_refs};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<DailyTrendPoint>, String>>,
    scroll: &ScrollState,
) {
    render_sorted(frame, area, data, scroll, SortState::default());
}

pub(crate) fn render_sorted(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<DailyTrendPoint>, String>>,
    scroll: &ScrollState,
    sort: SortState,
) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(theme::panel_block("Daily Usage"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(theme::panel_block("Daily Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("No daily usage data found. Press r to refresh.")
                .style(theme::muted_style())
                .block(theme::panel_block("Daily Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll, sort),
    }
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    items: &[DailyTrendPoint],
    scroll: &ScrollState,
    sort: SortState,
) {
    let block = theme::panel_block("Daily Usage");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let very_narrow = inner.width < 58;
    let narrow = inner.width < 92;
    let [table_area, detail_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    // The outer panel border is already removed; only header + margin consume rows.
    let visible_height = table_area.height.saturating_sub(2).max(1) as usize;
    let ordered = stable_sort_refs(
        items.iter().rev().collect(),
        sort,
        |left, right, key| match key {
            TableSortKey::Date => left.date.cmp(&right.date),
            TableSortKey::Tokens => left.total_tokens.cmp(&right.total_tokens),
            TableSortKey::Cost => left
                .cost_with_cache_usd
                .total_cmp(&right.cost_with_cache_usd),
        },
    );
    let range = scroll.visible_range(ordered.len(), visible_height);
    let rows = range.clone().enumerate().map(|(visible_index, absolute)| {
        let day = ordered[absolute];
        let cells = if very_narrow {
            vec![
                Cell::from(compact_date(&day.date)),
                Cell::from(format_cost(day.cost_with_cache_usd))
                    .style(theme::fg_style(theme::positive_fg())),
            ]
        } else if narrow {
            vec![
                Cell::from(compact_date(&day.date)),
                Cell::from(stat_compact(day.event_count)),
                Cell::from(stat_compact(day.total_tokens)),
                Cell::from(format_cost(day.cost_with_cache_usd))
                    .style(theme::fg_style(theme::positive_fg())),
            ]
        } else {
            vec![
                Cell::from(day.date.clone()).style(theme::bold_style()),
                Cell::from(stat_compact(day.event_count)),
                Cell::from(stat_compact(day.input_tokens))
                    .style(metric_style(theme::metric_input())),
                Cell::from(stat_compact(day.output_tokens))
                    .style(metric_style(theme::metric_output())),
                Cell::from(stat_compact(day.cache_read_tokens))
                    .style(metric_style(theme::metric_cache_read())),
                Cell::from(stat_compact(day.cache_creation_tokens))
                    .style(metric_style(theme::metric_cache_write())),
                Cell::from(cache_hit_rate(day)).style(metric_style(theme::warning_fg())),
                Cell::from(stat_compact(day.total_tokens)),
                Cell::from(format_cost(day.cost_with_cache_usd))
                    .style(theme::fg_style(theme::positive_fg())),
            ]
        };

        let row = Row::new(cells);
        if absolute == scroll.selected {
            row.style(theme::selection_style())
        } else if visible_index % 2 == 1 {
            row.style(theme::row_alt_style())
        } else {
            row
        }
    });

    let header = Row::new(header_cells(very_narrow, narrow, sort))
        .style(theme::header_style())
        .bottom_margin(1);
    let table = Table::new(rows, widths(very_narrow, narrow)).header(header);

    frame.render_widget(table, table_area);
    render_detail(
        frame,
        detail_area,
        &ordered,
        scroll.selected,
        narrow || very_narrow,
    );
}

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    items: &[&DailyTrendPoint],
    selected: usize,
    compact: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(day) = items.get(selected.min(items.len().saturating_sub(1))) else {
        return;
    };
    let line = Line::from(vec![
        Span::styled("detail ", theme::muted_style()),
        Span::styled(
            if compact {
                compact_date(&day.date)
            } else {
                day.date.clone()
            },
            theme::block_title_style(),
        ),
        Span::styled("  input ", theme::muted_style()),
        Span::styled(
            stat_compact(day.input_tokens),
            metric_style(theme::metric_input()),
        ),
        Span::styled("  output ", theme::muted_style()),
        Span::styled(
            stat_compact(day.output_tokens),
            metric_style(theme::metric_output()),
        ),
        Span::styled("  cache R/W ", theme::muted_style()),
        Span::styled(
            format!(
                "{}/{}",
                stat_compact(day.cache_read_tokens),
                stat_compact(day.cache_creation_tokens)
            ),
            metric_style(theme::metric_cache_write()),
        ),
        Span::styled("  cost ", theme::muted_style()),
        Span::styled(
            format_cost(day.cost_with_cache_usd),
            metric_style(theme::positive_fg()),
        ),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme::muted_style()), area);
}

fn header_cells(very_narrow: bool, narrow: bool, sort: SortState) -> Vec<Cell<'static>> {
    let labels = if very_narrow {
        vec![
            sort.header("Date", TableSortKey::Date),
            sort.header("Cost", TableSortKey::Cost),
        ]
    } else if narrow {
        vec![
            sort.header("Date", TableSortKey::Date),
            "Events".to_string(),
            sort.header("Tokens", TableSortKey::Tokens),
            sort.header("Cost", TableSortKey::Cost),
        ]
    } else {
        vec![
            sort.header("Date", TableSortKey::Date),
            "Events".to_string(),
            "Input".to_string(),
            "Output".to_string(),
            "Cache R".to_string(),
            "Cache W".to_string(),
            "Cache%".to_string(),
            sort.header("Total", TableSortKey::Tokens),
            sort.header("Cost", TableSortKey::Cost),
        ]
    };
    labels
        .into_iter()
        .map(|label| Cell::from(Span::styled(label, theme::header_style())))
        .collect()
}

fn widths(very_narrow: bool, narrow: bool) -> Vec<Constraint> {
    if very_narrow {
        vec![Constraint::Percentage(56), Constraint::Percentage(44)]
    } else if narrow {
        vec![
            Constraint::Percentage(24),
            Constraint::Percentage(18),
            Constraint::Percentage(32),
            Constraint::Percentage(26),
        ]
    } else {
        vec![
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(10),
        ]
    }
}

fn compact_date(date: &str) -> String {
    if date.len() >= 10 && date.as_bytes().get(4) == Some(&b'-') {
        date.chars().skip(5).take(5).collect()
    } else {
        date.chars().take(8).collect()
    }
}

fn cache_hit_rate(day: &DailyTrendPoint) -> String {
    let prompt = day.input_tokens + day.cache_read_tokens + day.cache_creation_tokens;
    if prompt <= 0 {
        "-".to_string()
    } else {
        format!(
            "{:.0}%",
            day.cache_read_tokens.max(0) as f64 / prompt as f64 * 100.0
        )
    }
}

fn metric_style(color: Color) -> Style {
    theme::fg_style(color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn day(date: &str, tokens: i64, input: i64, cost: f64) -> DailyTrendPoint {
        DailyTrendPoint {
            date: date.to_string(),
            input_tokens: input,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: tokens - input,
            total_tokens: tokens,
            event_count: 1,
            cost_with_cache_usd: cost,
        }
    }

    #[test]
    fn sorted_header_and_detail_follow_selected_row() {
        let data = Some(Ok(vec![
            day("2026-07-19", 100, 11, 0.10),
            day("2026-07-20", 900, 77, 0.90),
        ]));
        let scroll = ScrollState {
            offset: 0,
            selected: 1,
            total: 2,
            visible: 6,
        };
        let sort = SortState {
            key: Some(TableSortKey::Tokens),
            descending: true,
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
        terminal
            .draw(|frame| render_sorted(frame, frame.area(), &data, &scroll, sort))
            .unwrap();

        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Tokens ▼"));
        assert!(text.contains("detail 07-19  input 11"));
    }
}
