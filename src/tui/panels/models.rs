use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::query::ModelBreakdown;
use crate::tui::{format::grouped as format_number, panels::longtail, theme};

use super::super::app::ScrollState;

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
    render_with_plan(frame, area, data, scroll, collapsed);
}

pub(crate) fn render_with_plan(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<ModelBreakdown>, String>>,
    scroll: &ScrollState,
    collapsed: Option<longtail::Collapsed>,
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
        Some(Ok(items)) => render_table(frame, area, items, scroll, collapsed),
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
) {
    let header = Row::new(vec![
        Cell::from("Model"),
        Cell::from("Total Tokens"),
        Cell::from("Events"),
        Cell::from("Cost (USD)"),
    ])
    .style(theme::header_style())
    .bottom_margin(1);

    // Fold the sub-2% long tail into one summary row on large breakdowns.
    let shown = collapsed.map(|c| &items[..c.keep]).unwrap_or(items);
    let visible_height = super::visible_table_rows(area);

    let mut rows: Vec<Row> = shown
        .iter()
        .skip(scroll.offset)
        .take(visible_height)
        .enumerate()
        .map(|(i, item)| {
            let absolute = scroll.offset + i;
            let row = Row::new(vec![
                Cell::from(item.model.clone()),
                Cell::from(format_number(item.total_tokens)),
                Cell::from(format_number(item.event_count)),
                Cell::from(format!("{:.4}", item.cost_with_cache_usd)),
            ]);
            if absolute == 0 {
                // Rank #1 stands out in the accent color, bold.
                row.style(theme::bold_fg_style(theme::accent()))
            } else if i % 2 == 1 {
                row.style(theme::row_alt_style())
            } else {
                row
            }
        })
        .collect();

    if let Some(collapsed) = collapsed.filter(|collapsed| {
        collapsed.keep >= scroll.offset
            && collapsed.keep < scroll.offset.saturating_add(visible_height)
    }) {
        rows.push(
            Row::new(vec![
                Cell::from(longtail::summary_label(&collapsed)),
                Cell::from(format_number(collapsed.hidden_value)),
                Cell::from(String::new()),
                Cell::from(String::new()),
            ])
            .style(theme::muted_style()),
        );
    }

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
    .block(styled_block("Models"))
    .row_highlight_style(theme::selection_style());

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
