use ratatui::{
    Frame,
    layout::{Constraint, Layout},
};

use super::app::{AppState, Panel};
use super::nav_bar;
use super::panels;

/// Top-level draw orchestrator: splits layout into nav bar and content area,
/// then dispatches to panel-specific rendering.
pub fn draw(frame: &mut Frame, state: &AppState) {
    let [nav_area, content_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(frame.area());

    // Nav bar: render panel labels with active highlight
    nav_bar::render(frame, nav_area, state.active_panel);

    // Dispatch to panel-specific renderers
    match state.active_panel {
        Panel::Overview => panels::overview::render(frame, content_area, &state.overview),
        Panel::Trends => {
            panels::trends::render(frame, content_area, &state.trends, state.time_window)
        }
        Panel::Models => panels::models::render(
            frame,
            content_area,
            &state.models,
            &state.scroll[Panel::Models as usize],
        ),
        Panel::Sources => panels::sources::render(
            frame,
            content_area,
            &state.sources,
            &state.scroll[Panel::Sources as usize],
        ),
        Panel::Projects => panels::projects::render(
            frame,
            content_area,
            &state.projects,
            &state.scroll[Panel::Projects as usize],
        ),
        Panel::Cost => panels::cost::render(
            frame,
            content_area,
            &state.costs,
            &state.scroll[Panel::Cost as usize],
        ),
        Panel::Health => panels::health::render(frame, content_area, &state.health),
        Panel::Behavior => panels::behavior::render(frame, content_area, &state.behavior),
    }
}
