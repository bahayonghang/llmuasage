use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::query::HealthPayload;
use crate::tui::theme;

/// Render the health panel showing integrations, cursors, and recent failures.
pub fn render(frame: &mut Frame, area: Rect, data: &Option<Result<HealthPayload, String>>) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(styled_block("Health"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(styled_block("Health"));
            frame.render_widget(widget, area);
        }
        Some(Ok(payload)) => render_payload(frame, area, payload),
    }
}

fn render_payload(frame: &mut Frame, area: Rect, payload: &HealthPayload) {
    let [int_area, cur_area, fail_area] = Layout::vertical([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .areas(area);

    // Integrations section
    let int_lines: Vec<Line> = if payload.integrations.is_empty() {
        vec![Line::styled("No data", theme::muted_style())]
    } else {
        payload
            .integrations
            .iter()
            .map(|i| {
                let status_color = if i.status == "ok" || i.status == "active" {
                    theme::positive_fg()
                } else {
                    theme::warning_fg()
                };
                Line::from(vec![
                    Span::raw(format!("  {} ", i.source)),
                    Span::styled(&i.status, theme::bold_fg_style(status_color)),
                ])
            })
            .collect()
    };
    let int_widget = Paragraph::new(int_lines).block(section_block("Integration Status"));
    frame.render_widget(int_widget, int_area);

    // Cursors section
    let cur_lines: Vec<Line> = if payload.cursors.is_empty() {
        vec![Line::styled("No data", theme::muted_style())]
    } else {
        payload
            .cursors
            .iter()
            .map(|c| {
                let ts = c.updated_at.as_deref().unwrap_or("Never updated");
                Line::from(vec![
                    Span::styled(format!("  {} ", c.source), theme::fg_style(theme::accent())),
                    Span::raw(format!("/ {} — ", c.cursor_key)),
                    Span::styled(ts, theme::muted_style()),
                ])
            })
            .collect()
    };
    let cur_widget = Paragraph::new(cur_lines).block(section_block("Cursors"));
    frame.render_widget(cur_widget, cur_area);

    // Recent failures section (max 10)
    let fail_lines: Vec<Line> = if payload.recent_failures.is_empty() {
        vec![Line::styled(
            "No failure records",
            theme::fg_style(theme::positive_fg()),
        )]
    } else {
        payload
            .recent_failures
            .iter()
            .take(10)
            .map(|r| match &r.error {
                Some(err) => Line::from(vec![
                    Span::styled(
                        format!("  {} ", r.command),
                        theme::fg_style(theme::warning_fg()),
                    ),
                    Span::styled(err.as_str(), theme::fg_style(theme::error_fg())),
                ]),
                None => Line::styled(
                    format!("  {}", r.command),
                    theme::fg_style(theme::warning_fg()),
                ),
            })
            .collect()
    };
    let fail_widget = Paragraph::new(fail_lines).block(section_block("Recent Failures"));
    frame.render_widget(fail_widget, fail_area);
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

fn section_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme::fg_style(theme::border_normal()))
        .title(Span::styled(
            format!(" {} ", title),
            theme::bold_fg_style(theme::accent()),
        ))
}
