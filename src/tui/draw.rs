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
            Panel::Overview => panels::overview::render(frame, content_area, &state.overview),
            Panel::Trends => panels::usage::render(
                frame,
                content_area,
                &state.sync_center,
                &state.platform_probes,
                &state.scroll[Panel::Trends as usize],
            ),
            Panel::Models => panels::models::render_with_plan(
                frame,
                content_area,
                &state.models,
                &state.scroll[Panel::Models as usize],
                state.model_collapse,
                state.sort[Panel::Models as usize],
            ),
            Panel::Sources => panels::daily::render_sorted(
                frame,
                content_area,
                &state.daily,
                &state.scroll[Panel::Sources as usize],
                state.sort[Panel::Sources as usize],
            ),
            Panel::Projects => panels::hourly::render(
                frame,
                content_area,
                &state.hourly,
                &state.scroll[Panel::Projects as usize],
            ),
            Panel::Cost => panels::cost::render_with_plan(
                frame,
                content_area,
                &state.costs,
                &state.scroll[Panel::Cost as usize],
                state.cost_collapse,
                state.sort[Panel::Cost as usize],
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
            None => {}
        }
    });
}
