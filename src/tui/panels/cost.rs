use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::query::CostLine;
use crate::tui::theme;

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
            let widget = Paragraph::new("加载中...")
                .style(theme::muted_style())
                .block(styled_block("成本"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("数据加载失败: {e}"))
                .style(theme::error_style())
                .block(styled_block("成本"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("暂无成本数据")
                .style(theme::muted_style())
                .block(styled_block("成本"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll),
    }
}

fn render_table(frame: &mut Frame, area: Rect, items: &[CostLine], scroll: &ScrollState) {
    let header = Row::new(vec![
        Cell::from("来源"),
        Cell::from("模型"),
        Cell::from("事件数"),
        Cell::from("总 Tokens"),
        Cell::from("估算成本"),
    ])
    .style(theme::header_style())
    .bottom_margin(1);

    let rows: Vec<Row> = items
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
    .block(styled_block("成本"))
    .row_highlight_style(Style::default().fg(theme::ACCENT));

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

fn format_number(n: i64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    if n < 0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}
