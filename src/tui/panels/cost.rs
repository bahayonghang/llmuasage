use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::query::CostLine;
use crate::tui::panels::longtail;
use crate::tui::{format::stat_compact, theme};

use super::super::app::{ScrollState, SortState, TableSortKey, stable_sort_refs};

/// Render the cost panel as a table with scroll support.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<CostLine>, String>>,
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
    data: &Option<Result<Vec<CostLine>, String>>,
    scroll: &ScrollState,
    collapsed: Option<longtail::Collapsed>,
    sort: SortState,
) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(styled_block("Cost"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(styled_block("Cost"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("No cost data found.")
                .style(theme::muted_style())
                .block(styled_block("Cost"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll, collapsed, sort),
    }
}

pub(crate) fn collapse_plan(items: &[CostLine]) -> Option<longtail::Collapsed> {
    let values: Vec<i64> = items
        .iter()
        .map(|item| (item.estimated_cost_usd.max(0.0) * 1_000_000.0) as i64)
        .collect();
    let total_cost: i64 = values.iter().sum();
    longtail::collapse_tail(&values, total_cost)
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    items: &[CostLine],
    scroll: &ScrollState,
    collapsed: Option<longtail::Collapsed>,
    sort: SortState,
) {
    let header = Row::new(vec![
        Cell::from("Source"),
        Cell::from("Model"),
        Cell::from("Events"),
        Cell::from(sort.header("Total Tokens", TableSortKey::Tokens)),
        Cell::from(sort.header("Estimated Cost", TableSortKey::Cost)),
    ])
    .style(theme::header_style())
    .bottom_margin(1);

    // Fold the sub-2% cost tail (metric in micro-dollars) into a summary row.
    let collapsed = collapsed.filter(|_| sort.key.is_none());
    let mut ordered =
        stable_sort_refs(items.iter().collect(), sort, |left, right, key| match key {
            TableSortKey::Tokens => left.total_tokens.cmp(&right.total_tokens),
            TableSortKey::Cost => left.estimated_cost_usd.total_cmp(&right.estimated_cost_usd),
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
                        Cell::from(item.source.clone()),
                        Cell::from(item.model.clone()),
                        Cell::from(stat_compact(item.event_count)),
                        Cell::from(stat_compact(item.total_tokens)),
                        Cell::from(format!("${:.2}", item.estimated_cost_usd)),
                    ]),
                    false,
                )
            } else {
                let collapsed = collapsed.expect("summary row requires a collapse plan");
                (
                    Row::new(vec![
                        Cell::from(longtail::summary_label(&collapsed)),
                        Cell::from(String::new()),
                        Cell::from(String::new()),
                        Cell::from(String::new()),
                        Cell::from(format!(
                            "${:.2}",
                            collapsed.hidden_value as f64 / 1_000_000.0
                        )),
                    ]),
                    true,
                )
            };
            if absolute == scroll.selected {
                row.style(theme::selection_style())
            } else if summary {
                row.style(theme::muted_style())
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
            Constraint::Percentage(20),
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(styled_block("Cost"));

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
