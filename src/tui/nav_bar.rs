use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::app::Panel;
use super::theme;

/// Render a tokscale-style header with tab labels and active highlight.
pub fn render(frame: &mut Frame, area: Rect, active_panel: Panel) {
    let very_narrow = area.width < 60;
    let mut spans: Vec<Span> = Vec::with_capacity(Panel::COUNT * 3);

    for (i, panel) in Panel::all().iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                " │ ",
                Style::default().fg(theme::border_normal()),
            ));
        }

        let tab_label = if very_narrow {
            panel.short_label()
        } else {
            panel.label()
        };
        let label = format!(" {} {} ", i + 1, tab_label);
        let style = if *panel == active_panel {
            theme::nav_active_style()
        } else {
            theme::nav_inactive_style()
        };
        spans.push(Span::styled(label, style));
    }

    let line = Line::from(spans);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::border_normal()))
        .title(Span::styled(
            " llmusage ",
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Left)
        .title_top(
            Line::from(Span::styled(
                " local usage ",
                Style::default().fg(theme::muted_fg()),
            ))
            .right_aligned(),
        );
    let widget = Paragraph::new(line).block(block);
    frame.render_widget(widget, area);
}

pub fn panel_at_position(area: Rect, column: u16, row: u16) -> Option<Panel> {
    if area.width <= 2 || area.height <= 2 || row != area.y.saturating_add(1) {
        return None;
    }
    let very_narrow = area.width < 60;
    let mut x = area.x.saturating_add(1);
    let right = area.right().saturating_sub(1);

    for (index, panel) in Panel::all().iter().copied().enumerate() {
        if index > 0 {
            x = x.saturating_add(3);
        }
        if x >= right {
            return None;
        }
        let label = if very_narrow {
            panel.short_label()
        } else {
            panel.label()
        };
        let width = format!(" {} {} ", index + 1, label).chars().count() as u16;
        let end = x.saturating_add(width).min(right);
        if (x..end).contains(&column) {
            return Some(panel);
        }
        x = end;
    }

    None
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::*;

    #[test]
    fn header_hit_test_uses_rendered_tab_positions() {
        let area = Rect::new(0, 0, 100, 3);

        assert_eq!(panel_at_position(area, 2, 1), Some(Panel::Overview));
        assert_eq!(panel_at_position(area, 17, 1), Some(Panel::Trends));
        assert_eq!(panel_at_position(area, 99, 1), None);
    }

    #[test]
    fn narrow_header_uses_short_labels_for_hit_testing() {
        let area = Rect::new(0, 0, 50, 3);

        assert_eq!(panel_at_position(area, 2, 1), Some(Panel::Overview));
        assert_eq!(panel_at_position(area, 12, 1), Some(Panel::Trends));
    }
}
