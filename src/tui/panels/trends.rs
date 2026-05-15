use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Bar, BarChart, BarGroup, Block, Borders, Paragraph},
};

use super::super::app::TimeWindow;
use crate::query::TrendPoint;
use crate::tui::theme;

/// All time window variants in display order.
const ALL_WINDOWS: [TimeWindow; 4] = [
    TimeWindow::Day24h,
    TimeWindow::Week7d,
    TimeWindow::Month30d,
    TimeWindow::All,
];

/// Render the trends panel: time window selector + bar chart.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<TrendPoint>, String>>,
    time_window: TimeWindow,
) {
    match data {
        None => {
            let widget = Paragraph::new("加载中...")
                .style(theme::muted_style())
                .block(styled_block("趋势"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("数据加载失败: {e}"))
                .style(theme::error_style())
                .block(styled_block("趋势"));
            frame.render_widget(widget, area);
        }
        Some(Ok(points)) => render_chart(frame, area, points, time_window),
    }
}

fn render_chart(frame: &mut Frame, area: Rect, points: &[TrendPoint], time_window: TimeWindow) {
    let block = styled_block("趋势");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split: top row for window selector, remaining for chart
    let [selector_area, _gap, chart_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner);

    // Render time window selector
    render_window_selector(frame, selector_area, time_window);

    // Render chart or placeholder
    if points.is_empty() {
        let placeholder = Paragraph::new("暂无趋势数据").style(theme::muted_style());
        frame.render_widget(placeholder, chart_area);
    } else {
        let bars: Vec<Bar> = points
            .iter()
            .map(|p| {
                Bar::default()
                    .label(Line::from(p.label.clone()))
                    .value(p.total_tokens.max(0) as u64)
                    .style(Style::default().fg(Color::Cyan))
            })
            .collect();

        let chart = BarChart::default()
            .data(BarGroup::default().bars(&bars))
            .bar_width(3)
            .bar_gap(1)
            .bar_style(Style::default().fg(Color::Cyan));

        frame.render_widget(chart, chart_area);
    }
}

fn render_window_selector(frame: &mut Frame, area: Rect, active: TimeWindow) {
    let mut spans: Vec<Span> = vec![Span::styled("时间窗口: ", theme::muted_style())];

    for (i, window) in ALL_WINDOWS.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let label = format!(" {} ", window.label());
        let style = if *window == active {
            theme::nav_active_style()
        } else {
            theme::nav_inactive_style()
        };
        spans.push(Span::styled(label, style));
    }

    let line = Line::from(spans);
    let widget = Paragraph::new(line);
    frame.render_widget(widget, area);
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
