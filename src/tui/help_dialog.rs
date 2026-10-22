use crate::common::i18n::tui::text as tr;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::{app::AppState, theme};

pub fn render(frame: &mut Frame, viewport: Rect, state: &AppState) {
    let area = centered_rect(viewport, 72, 18);
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(tr("help_settings"))
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

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(tr("theme"), theme::muted_style()),
            Span::styled(
                theme::active_theme().name,
                theme::bold_fg_style(theme::accent()),
            ),
            Span::styled(tr("source_2"), theme::muted_style()),
            Span::styled(
                state.source_filter_label(),
                theme::fg_style(theme::accent()),
            ),
            Span::styled(tr("window_2"), theme::muted_style()),
            Span::styled(state.time_window.label(), theme::fg_style(theme::accent())),
        ])),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new("-".repeat(rows[1].width as usize)).style(theme::muted_style()),
        rows[1],
    );

    let lines = vec![
        Line::from(tr("tab_shift_tab_switch_tabs_1_9_jump_to_tab")),
        Line::from(tr("j_k_or_arrows_select_row_pgup_pgdn_page")),
        Line::from(tr("home_end_first_last_row_o_o_sort_key_direction")),
        Line::from(tr("mouse_wheel_select_row_h_l_today_7d_30d_all")),
        Line::from(tr("window_models_daily_hourly_monthly_stats_behavior")),
        Line::from(tr("enter_day_month_detail_esc_back_quit")),
        Line::from(tr("stats_click_day_enter_today_esc_close")),
        Line::from(tr("s_source_picker_a_all_sources_in_picker")),
        Line::from(tr("r_refresh_dashboard_cache_r_toggle_auto_refresh")),
        Line::from(tr("x_run_sync_for_current_source_y_sync_status_overlay")),
        Line::from(tr("m_show_hide_usage_emails_t_cycle_theme")),
        Line::from(tr("this_dialog_q_or_esc_close_quit")),
    ];
    frame.render_widget(Paragraph::new(lines).style(theme::muted_style()), rows[2]);

    frame.render_widget(
        Paragraph::new(tr(
            "parserless_platforms_remain_monitor_only_until_fixtures_and_token_semantics_exist",
        ))
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
