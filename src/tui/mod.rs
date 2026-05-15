use std::io;

use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{query::Dashboard, store::Store};

pub mod app;
pub mod draw;
pub mod input;
pub mod nav_bar;
pub mod panels;
pub mod report_table;
pub mod theme;

use app::{AppState, Panel};
use input::{Action, handle_key_event};

/// Main entry point for the interactive terminal dashboard.
pub fn run_dashboard(store: &Store) -> Result<()> {
    // 1. Install panic hook BEFORE enabling raw mode
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
        default_hook(info);
    }));

    // 2. Setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 3. Run event loop
    let result = event_loop(&mut terminal, store);

    // 4. Cleanup (always runs)
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show)?;

    // 5. Restore default panic hook
    let _ = std::panic::take_hook();

    result
}

/// Backwards-compatible wrapper for existing callers.
pub fn run_terminal(store: &Store) -> Result<()> {
    run_dashboard(store)
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, store: &Store) -> Result<()> {
    let dashboard = Dashboard::open(store)?;
    let mut state = AppState::new();

    // Load overview data initially (default panel)
    load_panel_data(&dashboard, &mut state, Panel::Overview);

    loop {
        // Draw frame
        terminal.draw(|frame| draw::draw(frame, &state))?;

        // Read next event (only handle Press to avoid double-fire on Windows)
        let ev = event::read()?;
        let key = match ev {
            Event::Key(k) if k.kind == KeyEventKind::Press => k,
            _ => continue,
        };

        let action = handle_key_event(key, state.active_panel);

        match action {
            Action::Quit => break,
            Action::SwitchPanel(p) => {
                state.active_panel = p;
                load_panel_data(&dashboard, &mut state, p);
            }
            Action::NextPanel => {
                let p = state.active_panel.next();
                state.active_panel = p;
                load_panel_data(&dashboard, &mut state, p);
            }
            Action::PrevPanel => {
                let p = state.active_panel.prev();
                state.active_panel = p;
                load_panel_data(&dashboard, &mut state, p);
            }
            Action::ScrollDown => {
                state.scroll[state.active_panel as usize].scroll_down();
            }
            Action::ScrollUp => {
                state.scroll[state.active_panel as usize].scroll_up();
            }
            Action::NextWindow => {
                state.time_window = state.time_window.next();
                // Invalidate trends cache so it reloads with new window
                state.trends = None;
                let panel = state.active_panel;
                load_panel_data(&dashboard, &mut state, panel);
            }
            Action::PrevWindow => {
                state.time_window = state.time_window.prev();
                state.trends = None;
                let panel = state.active_panel;
                load_panel_data(&dashboard, &mut state, panel);
            }
            Action::None => {}
        }
    }

    Ok(())
}

/// Load data for a panel only when the cached value is `None`.
fn load_panel_data(dashboard: &Dashboard, state: &mut AppState, panel: Panel) {
    match panel {
        Panel::Overview => {
            if state.overview.is_none() {
                state.overview = Some(dashboard.overview(&state.filter).map_err(|e| e.to_string()));
            }
        }
        Panel::Trends => {
            if state.trends.is_none() {
                state.trends = Some(
                    dashboard
                        .trends(state.time_window.as_query_str(), &state.filter)
                        .map_err(|e| e.to_string()),
                );
            }
        }
        Panel::Models => {
            if state.models.is_none() {
                state.models = Some(
                    dashboard
                        .model_breakdown(&state.filter)
                        .map_err(|e| e.to_string()),
                );
            }
        }
        Panel::Sources => {
            if state.sources.is_none() {
                state.sources = Some(
                    dashboard
                        .source_breakdown(&state.filter)
                        .map_err(|e| e.to_string()),
                );
            }
        }
        Panel::Projects => {
            if state.projects.is_none() {
                state.projects = Some(
                    dashboard
                        .project_breakdown(&state.filter)
                        .map_err(|e| e.to_string()),
                );
            }
        }
        Panel::Cost => {
            if state.costs.is_none() {
                state.costs = Some(
                    dashboard
                        .cost_breakdown(&state.filter)
                        .map_err(|e| e.to_string()),
                );
            }
        }
        Panel::Health => {
            if state.health.is_none() {
                state.health = Some(dashboard.health().map_err(|e| e.to_string()));
            }
        }
    }
}
