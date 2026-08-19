use ratatui::{
    Frame,
    layout::{Constraint, Layout},
};

use super::app::{ActiveDialog, AppState, Panel};
use super::footer;
use super::help_dialog;
use super::nav_bar;
use super::panels;
use super::source_picker;

/// Top-level draw orchestrator: splits layout into nav bar and content area,
/// then dispatches to panel-specific rendering.
pub fn draw(frame: &mut Frame, state: &AppState) {
    super::theme::with_render_snapshot(|| {
        let [nav_area, content_area, footer_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(4),
        ])
        .areas(frame.area());

        nav_bar::render(frame, nav_area, state.active_panel);

        match state.active_panel {
            Panel::Overview => panels::overview::render_with_plan(
                frame,
                content_area,
                &state.overview,
                &state.scroll[Panel::Overview as usize],
                state.sort[Panel::Overview as usize],
            ),
            Panel::Trends => panels::usage::render(
                frame,
                content_area,
                &state.quota_report,
                state.quota_fetching,
                state.hide_usage_emails,
                &state.scroll[Panel::Trends as usize],
            ),
            Panel::Models => panels::models::render_with_plan(
                frame,
                content_area,
                &state.models,
                &state.scroll[Panel::Models as usize],
                state.sort[Panel::Models as usize],
            ),
            Panel::Sources => panels::daily::render_sorted(
                frame,
                content_area,
                &state.daily,
                &state.scroll[Panel::Sources as usize],
                state.sort[Panel::Sources as usize],
                state.period_detail.as_ref(),
            ),
            Panel::Projects => panels::hourly::render_sorted(
                frame,
                content_area,
                &state.hourly,
                &state.scroll[Panel::Projects as usize],
                state.sort[Panel::Projects as usize],
            ),
            Panel::Monthly => panels::monthly::render_sorted(
                frame,
                content_area,
                &state.monthly,
                &state.scroll[Panel::Monthly as usize],
                state.sort[Panel::Monthly as usize],
                state.period_detail.as_ref(),
            ),
            Panel::Health => panels::stats::render(
                frame,
                content_area,
                &state.stats,
                &state.scroll[Panel::Health as usize],
            ),
            Panel::Behavior => panels::behavior::render(frame, content_area, &state.behavior),
            Panel::Blocks => panels::blocks::render_sorted(
                frame,
                content_area,
                &state.blocks,
                &state.scroll[Panel::Blocks as usize],
                state.sort[Panel::Blocks as usize],
            ),
        }

        footer::render(frame, footer_area, state);

        match state.active_dialog {
            Some(ActiveDialog::SourcePicker) => source_picker::render(frame, frame.area(), state),
            Some(ActiveDialog::Help) => help_dialog::render(frame, frame.area(), state),
            Some(ActiveDialog::SyncStatus) => {
                panels::sync_status::render(
                    frame,
                    content_area,
                    &state.sync_center,
                    &state.platform_probes,
                    &state.sync_overlay_scroll,
                );
            }
            None => {}
        }
    });
}
