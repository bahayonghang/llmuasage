use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use super::super::app::ScrollState;
use crate::query::ProjectBreakdown;
use crate::tui::{format::grouped as format_number, theme};

/// Render the projects panel with a scrollable table.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<ProjectBreakdown>, String>>,
    scroll: &ScrollState,
) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(styled_block("Projects"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(styled_block("Projects"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("No project data found.")
                .style(theme::muted_style())
                .block(styled_block("Projects"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll),
    }
}

fn render_table(frame: &mut Frame, area: Rect, items: &[ProjectBreakdown], scroll: &ScrollState) {
    let header = Row::new(vec![
        Cell::from("Project"),
        Cell::from("Total Tokens"),
        Cell::from("Events"),
        Cell::from("Cost (USD)"),
    ])
    .style(theme::header_style())
    .bottom_margin(1);

    let rows: Vec<Row> = items
        .iter()
        .skip(scroll.offset)
        .enumerate()
        .map(|(i, item)| {
            let row = Row::new(vec![
                Cell::from(item.project_label.clone()),
                Cell::from(format_number(item.total_tokens)),
                Cell::from(format_number(item.event_count)),
                Cell::from(format!("{:.4}", item.total_cost_usd)),
            ]);
            if i % 2 == 1 {
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
    .block(styled_block("Projects"))
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
