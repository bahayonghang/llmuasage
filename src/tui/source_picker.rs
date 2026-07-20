use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use super::{app::AppState, theme};

pub fn render(frame: &mut Frame, viewport: Rect, state: &AppState) {
    let area = centered_rect(viewport, 74, 20);
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Sources ")
        .borders(Borders::ALL)
        .border_style(theme::fg_style(theme::accent()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);

    let current = format!("Filter: {}", state.source_filter_label());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Source picker", theme::block_title_style()),
            Span::styled(" • ", theme::muted_style()),
            Span::styled(current, theme::muted_style()),
        ])),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new("-".repeat(rows[1].width as usize)).style(theme::muted_style()),
        rows[1],
    );

    render_list(frame, rows[2], state);

    frame.render_widget(
        Paragraph::new("Enter select • a all sources • Esc close")
            .alignment(Alignment::Center)
            .style(theme::muted_style()),
        rows[3],
    );
}

fn render_list(frame: &mut Frame, area: Rect, state: &AppState) {
    let visible_height = area.height as usize;
    let selected = state.source_picker.selected;
    let scroll = if selected >= visible_height && visible_height > 0 {
        selected.saturating_sub(visible_height - 1)
    } else {
        0
    };

    let mut items = Vec::new();
    for (index, probe) in state
        .platform_probes
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
    {
        let selected_row = index == selected;
        let active = probe.source_kind == state.filter.source;
        let parser = probe.parser_status.as_str();
        let quality = probe.quality.unwrap_or("unavailable");
        let marker = if active {
            "[*]"
        } else if probe.source_kind.is_some() {
            "[ ]"
        } else {
            "[-]"
        };
        let row = format!(
            "{marker} {:<18} {:<10} {:<18} {}",
            probe.display_name,
            probe.status.as_str(),
            parser,
            quality
        );

        let style = if selected_row {
            theme::selection_style()
        } else if active {
            theme::bold_fg_style(theme::positive_fg())
        } else if probe.source_kind.is_none() {
            theme::muted_style()
        } else {
            Style::default()
        };
        items.push(ListItem::new(Line::from(Span::styled(row, style))));
    }

    if items.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "No source monitors",
            theme::muted_style(),
        ))));
    }

    frame.render_widget(List::new(items), area);
}

fn centered_rect(viewport: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = max_width.min(viewport.width.saturating_sub(4)).max(1);
    let height = max_height.min(viewport.height.saturating_sub(4)).max(1);
    let x = viewport.x + viewport.width.saturating_sub(width) / 2;
    let y = viewport.y + viewport.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}
