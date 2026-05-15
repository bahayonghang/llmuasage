use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use super::super::app::ScrollState;
use crate::query::SourceBreakdown;
use crate::tui::theme;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<SourceBreakdown>, String>>,
    scroll: &ScrollState,
) {
    match data {
        None => {
            let widget = Paragraph::new("加载中...")
                .style(theme::muted_style())
                .block(styled_block("来源"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("数据加载失败: {e}"))
                .style(theme::error_style())
                .block(styled_block("来源"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("暂无来源数据")
                .style(theme::muted_style())
                .block(styled_block("来源"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll),
    }
}

fn render_table(frame: &mut Frame, area: Rect, items: &[SourceBreakdown], scroll: &ScrollState) {
    let header = Row::new(vec![
        Cell::from("来源"),
        Cell::from("总 Tokens"),
        Cell::from("事件数"),
        Cell::from("最近事件"),
    ])
    .style(theme::header_style())
    .bottom_margin(1);

    let rows: Vec<Row> = items
        .iter()
        .skip(scroll.offset)
        .enumerate()
        .map(|(i, item)| {
            let last_event = item.last_event_at.as_deref().unwrap_or("-");
            let row = Row::new(vec![
                Cell::from(item.source.as_str()),
                Cell::from(format_number(item.total_tokens)),
                Cell::from(format_number(item.event_count)),
                Cell::from(last_event.to_string()),
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
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(20),
            Constraint::Percentage(30),
        ],
    )
    .header(header)
    .block(styled_block("来源"))
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
