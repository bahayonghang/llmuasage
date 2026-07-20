use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::{app::AppState, theme};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::border_normal()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    render_controls(frame, rows[0], state);
    render_status(frame, rows[1], state);
}

fn render_controls(frame: &mut Frame, area: Rect, state: &AppState) {
    let spans = if state.is_very_narrow() {
        vec![
            Span::styled("tab/1-8", theme::muted_style()),
            Span::raw(" "),
            Span::styled("s", Style::default().fg(theme::accent())),
            Span::raw(" "),
            Span::styled("r", Style::default().fg(theme::trend_peak_fg())),
            Span::raw(" "),
            Span::styled("R", Style::default().fg(theme::positive_fg())),
            Span::raw(" "),
            Span::styled("x", Style::default().fg(theme::positive_fg())),
            Span::raw(" "),
            Span::styled("?", theme::muted_style()),
            Span::raw(" "),
            Span::styled("q", theme::muted_style()),
        ]
    } else {
        vec![
            Span::styled("tab/shift-tab or 1-8 view", theme::muted_style()),
            Span::styled(" • ", theme::muted_style()),
            Span::styled("[s:source]", Style::default().fg(theme::accent())),
            Span::styled(" • ", theme::muted_style()),
            Span::styled("[r:refresh]", Style::default().fg(theme::trend_peak_fg())),
            Span::styled(" ", theme::muted_style()),
            Span::styled("[x:sync]", Style::default().fg(theme::positive_fg())),
            Span::styled(" ", theme::muted_style()),
            Span::styled(
                if state.auto_refresh {
                    "[R:auto on]"
                } else {
                    "[R:auto off]"
                },
                Style::default().fg(if state.auto_refresh {
                    theme::positive_fg()
                } else {
                    theme::muted_fg()
                }),
            ),
            Span::styled(" • [?] q", theme::muted_style()),
        ]
    };
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_status(frame: &mut Frame, area: Rect, state: &AppState) {
    let mut spans = vec![
        Span::styled(
            "source ",
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            state.source_filter_label(),
            Style::default().fg(theme::accent()),
        ),
        Span::styled(" • ", theme::muted_style()),
        Span::styled("window ", theme::muted_style()),
        Span::styled(
            state.time_window.label(),
            Style::default().fg(theme::accent()),
        ),
        Span::styled(" • ", theme::muted_style()),
    ];

    if let Some(message) = &state.status_message {
        spans.push(Span::styled(
            message.clone(),
            Style::default()
                .fg(theme::positive_fg())
                .add_modifier(Modifier::BOLD),
        ));
    } else if let Some(Ok(overview)) = &state.overview {
        spans.push(Span::styled(
            format!(
                "{} tokens • ${:.2}",
                format_compact(overview.total.total_tokens),
                overview.total_cost_usd
            ),
            theme::muted_style(),
        ));
    } else {
        spans.push(Span::styled("local dashboard cache", theme::muted_style()));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn format_compact(value: i64) -> String {
    let abs = value.abs();
    if abs >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if abs >= 10_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}
