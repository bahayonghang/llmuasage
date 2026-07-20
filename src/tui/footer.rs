use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::{app::AppState, format::footer_compact, theme};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::fg_style(theme::border_normal()));
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
            Span::styled("tab/1-9", theme::muted_style()),
            Span::raw(" "),
            Span::styled("j/k", theme::fg_style(theme::accent())),
            Span::raw(" "),
            Span::styled("Pg", theme::fg_style(theme::accent())),
            Span::raw(" "),
            Span::styled("o/O", theme::fg_style(theme::accent())),
            Span::raw(" "),
            Span::styled("s", theme::fg_style(theme::accent())),
            Span::raw(" "),
            Span::styled("r", theme::fg_style(theme::trend_peak_fg())),
            Span::raw(" "),
            Span::styled("R", theme::fg_style(theme::positive_fg())),
            Span::raw(" "),
            Span::styled("x", theme::fg_style(theme::positive_fg())),
            Span::raw(" "),
            Span::styled("?", theme::muted_style()),
            Span::raw(" "),
            Span::styled("q", theme::muted_style()),
        ]
    } else if state.is_narrow() {
        vec![Span::styled(
            "tab/1-9 view  j/k row  Pg page  o/O sort  s source  r refresh  x sync  ? q",
            theme::muted_style(),
        )]
    } else {
        vec![
            Span::styled("tab/shift-tab or 1-9 view", theme::muted_style()),
            Span::styled(" • ", theme::muted_style()),
            Span::styled("[j/k/Pg:select]", theme::fg_style(theme::accent())),
            Span::styled(" • ", theme::muted_style()),
            Span::styled("[o/O:sort]", theme::fg_style(theme::accent())),
            Span::styled(" • ", theme::muted_style()),
            Span::styled("[s:source]", theme::fg_style(theme::accent())),
            Span::styled(" • ", theme::muted_style()),
            Span::styled("[r:refresh]", theme::fg_style(theme::trend_peak_fg())),
            Span::styled(" ", theme::muted_style()),
            Span::styled("[x:sync]", theme::fg_style(theme::positive_fg())),
            Span::styled(" ", theme::muted_style()),
            Span::styled(
                if state.auto_refresh {
                    "[R:auto on]"
                } else {
                    "[R:auto off]"
                },
                theme::fg_style(if state.auto_refresh {
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
    let mut spans = Vec::new();
    if let Some(spinner) = activity_spinner(state) {
        spans.push(Span::styled(spinner, theme::bold_fg_style(theme::accent())));
    }
    spans.extend([
        Span::styled("source ", theme::bold_fg_style(theme::accent())),
        Span::styled(
            state.source_filter_label(),
            theme::fg_style(theme::accent()),
        ),
        Span::styled(" • ", theme::muted_style()),
        Span::styled("window ", theme::muted_style()),
        Span::styled(state.time_window.label(), theme::fg_style(theme::accent())),
        Span::styled(" • ", theme::muted_style()),
    ]);

    if let Some(message) = &state.status_message {
        spans.push(Span::styled(
            message.clone(),
            theme::bold_fg_style(theme::positive_fg()),
        ));
    } else if let Some(Ok(overview)) = &state.overview {
        spans.push(Span::styled(
            format!(
                "{} tokens • ${:.2}",
                footer_compact(overview.total.total_tokens),
                overview.total_cost_usd
            ),
            theme::muted_style(),
        ));
    } else {
        spans.push(Span::styled("local dashboard cache", theme::muted_style()));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn activity_spinner(state: &AppState) -> Option<String> {
    const FRAMES: [char; 4] = ['|', '/', '-', '\\'];
    state
        .background_active()
        .then(|| format!("[{}] ", FRAMES[state.spinner_frame % FRAMES.len()]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render_text(state: &AppState) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 4)).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), state))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn spinner_only_renders_during_background_activity() {
        let idle = AppState::new();
        assert_eq!(activity_spinner(&idle), None);
        assert!(!render_text(&idle).contains("[|]"));

        let mut loading = AppState::new();
        loading.panel_loading[crate::tui::app::Panel::Models as usize] = true;
        assert!(render_text(&loading).contains("[|]"));
        loading.spinner_frame = 1;
        assert!(render_text(&loading).contains("[/]"));

        let mut syncing = AppState::new();
        syncing.sync_active = true;
        syncing.spinner_frame = 2;
        assert!(render_text(&syncing).contains("[-]"));
    }
}
