use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::query::CostLine;
use crate::tui::panels::longtail;
use crate::tui::{format::grouped as format_number, theme};

use super::super::app::ScrollState;

/// Render the cost panel as a table with scroll support.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<CostLine>, String>>,
    scroll: &ScrollState,
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
        Some(Ok(items)) => render_table(frame, area, items, scroll),
    }
}

fn render_table(frame: &mut Frame, area: Rect, items: &[CostLine], scroll: &ScrollState) {
    let header = Row::new(vec![
        Cell::from("Source"),
        Cell::from("Model"),
        Cell::from("Events"),
        Cell::from("Total Tokens"),
        Cell::from("Estimated Cost"),
    ])
    .style(theme::header_style())
    .bottom_margin(1);

    // Fold the sub-2% cost tail (metric in micro-dollars) into a summary row.
    let micros = |cost: f64| (cost.max(0.0) * 1_000_000.0) as i64;
    let values: Vec<i64> = items
        .iter()
        .map(|item| micros(item.estimated_cost_usd))
        .collect();
    let total_cost: i64 = values.iter().sum();
    let collapsed = longtail::collapse_tail(&values, total_cost);
    let shown = collapsed.map(|c| &items[..c.keep]).unwrap_or(items);

    let mut rows: Vec<Row> = shown
        .iter()
        .skip(scroll.offset)
        .enumerate()
        .map(|(i, item)| {
            let row = Row::new(vec![
                Cell::from(item.source.clone()),
                Cell::from(item.model.clone()),
                Cell::from(format_number(item.event_count)),
                Cell::from(format_number(item.total_tokens)),
                Cell::from(format!("${:.2}", item.estimated_cost_usd)),
            ]);
            if i % 2 == 1 {
                row.style(theme::row_alt_style())
            } else {
                row
            }
        })
        .collect();

    if let Some(collapsed) = collapsed {
        rows.push(
            Row::new(vec![
                Cell::from(longtail::summary_label(&collapsed)),
                Cell::from(String::new()),
                Cell::from(String::new()),
                Cell::from(String::new()),
                Cell::from(format!(
                    "${:.2}",
                    collapsed.hidden_value as f64 / 1_000_000.0
                )),
            ])
            .style(theme::muted_style()),
        );
    }

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
    .block(styled_block("Cost"))
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
