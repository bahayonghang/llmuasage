use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::query::ModelBreakdown;
use crate::tui::{format::stat_compact, panels::longtail, theme};

use super::super::app::{ScrollState, SortState, TableSortKey, stable_sort_refs};

/// Render the models panel as a table with scroll support.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<ModelBreakdown>, String>>,
    scroll: &ScrollState,
) {
    let collapsed = data
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(|items| collapse_plan(items));
    render_with_plan(frame, area, data, scroll, collapsed, SortState::default());
}

pub(crate) fn render_with_plan(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<ModelBreakdown>, String>>,
    scroll: &ScrollState,
    collapsed: Option<longtail::Collapsed>,
    sort: SortState,
) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(styled_block("Models"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(styled_block("Models"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("No model data found.")
                .style(theme::muted_style())
                .block(styled_block("Models"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll, collapsed, sort),
    }
}

pub(crate) fn collapse_plan(items: &[ModelBreakdown]) -> Option<longtail::Collapsed> {
    let total_tokens: i64 = items.iter().map(|item| item.total_tokens.max(0)).sum();
    let values: Vec<i64> = items.iter().map(|item| item.total_tokens).collect();
    longtail::collapse_tail(&values, total_tokens)
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    items: &[ModelBreakdown],
    scroll: &ScrollState,
    collapsed: Option<longtail::Collapsed>,
    sort: SortState,
) {
    let header = Row::new(vec![
        Cell::from("Model"),
        Cell::from(sort.header("Total Tokens", TableSortKey::Tokens)),
        Cell::from("Events"),
        Cell::from(sort.header("Cost (USD)", TableSortKey::Cost)),
    ])
    .style(theme::header_style())
    .bottom_margin(1);

    // Fold the sub-2% long tail into one summary row on large breakdowns.
    let collapsed = collapsed.filter(|_| sort.key.is_none());
    let mut ordered =
        stable_sort_refs(items.iter().collect(), sort, |left, right, key| match key {
            TableSortKey::Tokens => left.total_tokens.cmp(&right.total_tokens),
            TableSortKey::Cost => left
                .cost_with_cache_usd
                .total_cmp(&right.cost_with_cache_usd),
            TableSortKey::Date => std::cmp::Ordering::Equal,
        });
    if sort.key.is_none()
        && let Some(collapsed) = collapsed
    {
        ordered.truncate(collapsed.keep);
    }
    let visible_height = super::visible_table_rows(area);
    let total_rows = ordered.len() + usize::from(collapsed.is_some());
    let range = scroll.visible_range(total_rows, visible_height);
    let rows: Vec<Row> = range
        .clone()
        .enumerate()
        .map(|(visible_index, absolute)| {
            let (row, summary) = if let Some(item) = ordered.get(absolute) {
                (
                    Row::new(vec![
                        Cell::from(item.model.clone()),
                        Cell::from(stat_compact(item.total_tokens)),
                        Cell::from(stat_compact(item.event_count)),
                        Cell::from(format!("{:.4}", item.cost_with_cache_usd)),
                    ]),
                    false,
                )
            } else {
                let collapsed = collapsed.expect("summary row requires a collapse plan");
                (
                    Row::new(vec![
                        Cell::from(longtail::summary_label(&collapsed)),
                        Cell::from(stat_compact(collapsed.hidden_value)),
                        Cell::from(String::new()),
                        Cell::from(String::new()),
                    ]),
                    true,
                )
            };
            if absolute == scroll.selected {
                row.style(theme::selection_style())
            } else if summary {
                row.style(theme::muted_style())
            } else if absolute == 0 {
                row.style(theme::bold_fg_style(theme::accent()))
            } else if visible_index % 2 == 1 {
                row.style(theme::row_alt_style())
            } else {
                row
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(40),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(styled_block("Models"));

    frame.render_widget(table, area);
}

fn styled_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme::block_border_style())
        .title(Span::styled(
            format!(" {} ", title),
            theme::block_title_style(),
        ))
}
