use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::query::ModelBreakdown;
use crate::tui::panels::longtail;
use crate::tui::theme;

use super::super::app::ScrollState;

/// Render the models panel as a table with scroll support.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<ModelBreakdown>, String>>,
    scroll: &ScrollState,
) {
    match data {
        None => {
            let widget = Paragraph::new("加载中...")
                .style(theme::muted_style())
                .block(styled_block("模型"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("数据加载失败: {e}"))
                .style(theme::error_style())
                .block(styled_block("模型"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("暂无模型数据")
                .style(theme::muted_style())
                .block(styled_block("模型"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll),
    }
}

fn render_table(frame: &mut Frame, area: Rect, items: &[ModelBreakdown], scroll: &ScrollState) {
    let header = Row::new(vec![
        Cell::from("模型"),
        Cell::from("总 Tokens"),
        Cell::from("事件数"),
        Cell::from("成本 (USD)"),
    ])
    .style(theme::header_style())
    .bottom_margin(1);

    // Fold the sub-2% long tail into one summary row on large breakdowns.
    let total_tokens: i64 = items.iter().map(|item| item.total_tokens.max(0)).sum();
    let values: Vec<i64> = items.iter().map(|item| item.total_tokens).collect();
    let collapsed = longtail::collapse_tail(&values, total_tokens);
    let shown = collapsed.map(|c| &items[..c.keep]).unwrap_or(items);

    let mut rows: Vec<Row> = shown
        .iter()
        .skip(scroll.offset)
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
                row.style(
                    Style::default()
                        .fg(theme::accent())
                        .add_modifier(ratatui::style::Modifier::BOLD),
                )
            } else if i % 2 == 1 {
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
    .block(styled_block("模型"))
    .row_highlight_style(Style::default().fg(theme::accent()));

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
