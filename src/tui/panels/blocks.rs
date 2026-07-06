//! Blocks panel: recent 5-hour rolling windows with burn rate and projection.
//!
//! Surfaces the same data as the `llmusage blocks` CLI report inside the
//! interactive dashboard, reusing [`crate::query::Dashboard::blocks_report`].

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::query::reports::BlockReportRow;
use crate::tui::theme;

use super::super::app::ScrollState;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<BlockReportRow>, String>>,
    scroll: &ScrollState,
) {
    match data {
        None => placeholder(frame, area, "加载中...", theme::muted_style()),
        Some(Err(e)) => placeholder(
            frame,
            area,
            &format!("数据加载失败: {e}"),
            theme::error_style(),
        ),
        Some(Ok(items)) if items.is_empty() => {
            placeholder(frame, area, "暂无区块数据", theme::muted_style())
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll),
    }
}

fn placeholder(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    let widget = Paragraph::new(text.to_string())
        .style(style)
        .block(styled_block("区块"));
    frame.render_widget(widget, area);
}

fn render_table(frame: &mut Frame, area: Rect, items: &[BlockReportRow], scroll: &ScrollState) {
    let header = Row::new(vec![
        Cell::from("窗口"),
        Cell::from("状态"),
        Cell::from("总 Tokens"),
        Cell::from("Burn/h"),
        Cell::from("预计"),
        Cell::from("额度"),
        Cell::from("成本 (USD)"),
    ])
    .style(theme::header_style())
    .bottom_margin(1);

    let rows: Vec<Row> = items
        .iter()
        .skip(scroll.offset)
        .enumerate()
        .map(|(i, item)| {
            let window = format!(
                "{} → {}",
                short_time(&item.start_at),
                short_time(&item.end_at)
            );
            let status = if item.is_active { "active" } else { "-" };
            let limit = match item.token_limit_percent {
                Some(percent) => format!("{percent:.0}%"),
                None => "-".to_string(),
            };
            let row = Row::new(vec![
                Cell::from(window),
                Cell::from(status),
                Cell::from(format_number(item.totals.total_tokens)),
                Cell::from(format_number(item.burn_rate_tokens_per_hour.round() as i64)),
                Cell::from(format_number(item.projected_total_tokens)),
                Cell::from(limit),
                Cell::from(format!("${:.2}", item.totals.estimated_cost_usd)),
            ]);
            if item.is_active {
                // The live window is highlighted; its burn rate/projection matter most.
                row.style(
                    Style::default()
                        .fg(theme::accent())
                        .add_modifier(Modifier::BOLD),
                )
            } else if i % 2 == 1 {
                row.style(theme::row_alt_style())
            } else {
                row
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(28),
            Constraint::Percentage(10),
            Constraint::Percentage(16),
            Constraint::Percentage(13),
            Constraint::Percentage(13),
            Constraint::Percentage(8),
            Constraint::Percentage(12),
        ],
    )
    .header(header)
    .block(styled_block("区块 (5h burn rate)"))
    .row_highlight_style(Style::default().fg(theme::accent()));

    frame.render_widget(table, area);
}

/// Trims a `YYYY-MM-DDTHH:MM...` timestamp to `MM-DD HH:MM` for the narrow cell.
fn short_time(ts: &str) -> String {
    let date = ts.get(5..10).unwrap_or("");
    let time = ts.get(11..16).unwrap_or("");
    if date.is_empty() && time.is_empty() {
        ts.to_string()
    } else {
        format!("{date} {time}").trim().to_string()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::reports::TokenTotals;
    use ratatui::{Terminal, backend::TestBackend};

    fn sample_row(active: bool) -> BlockReportRow {
        BlockReportRow {
            block_id: "b1".to_string(),
            start_at: "2026-06-20T10:00:00Z".to_string(),
            end_at: "2026-06-20T15:00:00Z".to_string(),
            is_active: active,
            duration_minutes: 90,
            burn_rate_tokens_per_hour: 12_000.0,
            projected_total_tokens: 60_000,
            token_limit: Some(100_000),
            token_limit_percent: Some(60.0),
            totals: TokenTotals {
                total_tokens: 45_000,
                estimated_cost_usd: 1.23,
                ..TokenTotals::default()
            },
            models_used: vec!["gpt-5".to_string()],
        }
    }

    fn render_text(data: &Option<Result<Vec<BlockReportRow>, String>>) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
        let scroll = ScrollState {
            offset: 0,
            total: 0,
            visible: 15,
        };
        terminal
            .draw(|frame| render(frame, frame.area(), data, &scroll))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer.content().iter().map(|cell| cell.symbol()).collect()
    }

    #[test]
    fn renders_rows_without_panicking() {
        let data = Some(Ok(vec![sample_row(true), sample_row(false)]));
        let text = render_text(&data);
        assert!(text.contains("Burn/h"));
        assert!(text.contains("active"));
        assert!(text.contains("45,000"));
    }

    #[test]
    fn renders_empty_and_error_states() {
        // Empty/error/loading states render the placeholder, not the table header.
        assert!(!render_text(&Some(Ok(vec![]))).contains("Burn/h"));
        assert!(render_text(&Some(Err("boom".to_string()))).contains("boom"));
        assert!(render_text(&None).contains("..."));
    }
}
