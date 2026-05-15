use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::app::Panel;
use super::theme;

/// Panel labels in display order.
const LABELS: [&str; Panel::COUNT] = ["概览", "趋势", "模型", "来源", "项目", "成本", "健康"];

/// Render a horizontal navigation bar showing all panel labels with the active one highlighted.
pub fn render(frame: &mut Frame, area: Rect, active_panel: Panel) {
    let mut spans: Vec<Span> = Vec::with_capacity(Panel::COUNT * 2);

    for (i, panel_label) in LABELS.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }

        let label = format!(" {}:{} ", i + 1, panel_label);
        let style = if i == active_panel as usize {
            theme::nav_active_style()
        } else {
            theme::nav_inactive_style()
        };
        spans.push(Span::styled(label, style));
    }

    let line = Line::from(spans);
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme::BORDER_NORMAL));
    let widget = Paragraph::new(line).block(block);
    frame.render_widget(widget, area);
}
