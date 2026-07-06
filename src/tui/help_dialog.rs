use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::{app::AppState, theme};

pub fn render(frame: &mut Frame, viewport: Rect, state: &AppState) {
    let area = centered_rect(viewport, 72, 14);
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Help / Settings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::accent()));
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

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Theme ", theme::muted_style()),
            Span::styled(
                theme::active_theme().name,
                Style::default()
                    .fg(theme::accent())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" • Source ", theme::muted_style()),
            Span::styled(
                state.source_filter_label(),
                Style::default().fg(theme::accent()),
            ),
        ])),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new("-".repeat(rows[1].width as usize)).style(theme::muted_style()),
        rows[1],
    );

    let lines = vec![
        Line::from("tab / shift-tab: switch tabs    1-9: jump to tab"),
        Line::from("j/k or arrows: scroll            h/l: change time window"),
        Line::from("s: source picker                 a: all sources in picker"),
        Line::from("r: refresh dashboard cache       R: toggle auto refresh"),
        Line::from("x: run sync for current source   t: cycle theme"),
        Line::from("?: this dialog                    q or Esc: close / quit"),
    ];
    frame.render_widget(Paragraph::new(lines).style(theme::muted_style()), rows[2]);

    frame.render_widget(
        Paragraph::new(
            "Parserless platforms remain monitor-only until fixtures and token semantics exist.",
        )
        .alignment(Alignment::Center)
        .style(theme::muted_style()),
        rows[3],
    );
}

fn centered_rect(viewport: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = max_width.min(viewport.width.saturating_sub(4)).max(1);
    let height = max_height.min(viewport.height.saturating_sub(4)).max(1);
    let x = viewport.x + viewport.width.saturating_sub(width) / 2;
    let y = viewport.y + viewport.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}
