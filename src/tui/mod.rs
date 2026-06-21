use std::io;

use anyhow::Result;
use crossterm::{
    cursor,
    event::{DisableMouseCapture, EnableMouseCapture, MouseButton, MouseEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{
    commands::sync::{self, SyncRunOptions},
    query::{Dashboard, InventoryRoots},
    store::Store,
};

pub mod app;
pub mod draw;
pub mod event;
pub mod footer;
pub mod help_dialog;
pub mod input;
pub mod nav_bar;
pub mod panels;
pub mod report_table;
pub mod source_picker;
pub mod theme;

use app::{AppState, BehaviorPanelPayload, Panel, StatsPanelPayload};
use event::{EventHandler, TuiEvent};
use input::{Action, DialogAction, handle_dialog_key_event, handle_key_event};

/// Main entry point for the interactive terminal dashboard.
pub fn run_dashboard(store: &Store) -> Result<()> {
    // 1. Install panic hook BEFORE enabling raw mode
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            LeaveAlternateScreen,
            cursor::Show
        );
        default_hook(info);
    }));

    // 2. Setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 3. Run event loop
    let result = event_loop(&mut terminal, store);

    // 4. Cleanup (always runs)
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        cursor::Show
    )?;

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
    let size = terminal.size()?;
    state.handle_resize(size.width, size.height);
    let mut events = EventHandler::new(std::time::Duration::from_millis(250));

    // Load overview data initially (default panel)
    load_panel_data(&dashboard, &mut state, Panel::Overview);
    state.mark_refreshed();

    loop {
        // Draw frame
        terminal.draw(|frame| draw::draw(frame, &state))?;

        let ev = events.recv()?;
        let action = match ev {
            TuiEvent::Tick => {
                state.on_tick();
                if state.needs_refresh {
                    refresh_panel_data(&dashboard, &mut state);
                }
                continue;
            }
            TuiEvent::Resize(width, height) => {
                state.handle_resize(width, height);
                continue;
            }
            TuiEvent::Mouse(mouse) => {
                if let Some(panel) = panel_from_mouse(&state, &mouse) {
                    Action::SwitchPanel(panel)
                } else {
                    Action::None
                }
            }
            TuiEvent::Key(key) => {
                if state.active_dialog.is_some() {
                    handle_dialog_action(handle_dialog_key_event(key), &mut state);
                    continue;
                }
                handle_key_event(key, state.active_panel)
            }
        };

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
            Action::Refresh => refresh_panel_data(&dashboard, &mut state),
            Action::ToggleAutoRefresh => state.toggle_auto_refresh(),
            Action::StartSync => run_sync_action(store, &dashboard, &mut state),
            Action::OpenSourcePicker => state.open_source_picker(),
            Action::OpenHelp => state.open_help(),
            Action::None => {}
        }
    }

    Ok(())
}

fn run_sync_action(store: &Store, dashboard: &Dashboard, state: &mut AppState) {
    state.set_status("Sync running...");
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            state.set_status(&format!("Sync runtime error: {err}"));
            return;
        }
    };
    let options = SyncRunOptions {
        source: state.filter.source,
        ..SyncRunOptions::default()
    };
    match runtime.block_on(sync::run_store_once_with_options(store, &options)) {
        Ok(summary) => {
            state.invalidate_cached_data();
            load_panel_data(dashboard, state, state.active_panel);
            state.mark_refreshed();
            state.set_status(&format!(
                "Sync complete: {} inserted, {} stored",
                summary.total_inserted, summary.stored_events
            ));
        }
        Err(err) => state.set_status(&format!("Sync failed: {err}")),
    }
}

fn refresh_panel_data(dashboard: &Dashboard, state: &mut AppState) {
    state.invalidate_cached_data();
    let panel = state.active_panel;
    load_panel_data(dashboard, state, panel);
    state.mark_refreshed();
    state.set_status("Refreshed local dashboard cache");
}

fn handle_dialog_action(action: DialogAction, state: &mut AppState) {
    if matches!(state.active_dialog, Some(app::ActiveDialog::Help)) {
        if matches!(action, DialogAction::Close) {
            state.close_dialog();
        }
        return;
    }

    match action {
        DialogAction::Close => state.close_dialog(),
        DialogAction::MoveDown => state.source_picker_next(),
        DialogAction::MoveUp => state.source_picker_prev(),
        DialogAction::Select => state.select_source_picker_row(),
        DialogAction::ClearSource => state.clear_source_filter(),
        DialogAction::None => {}
    }
}

fn panel_from_mouse(state: &AppState, mouse: &crossterm::event::MouseEvent) -> Option<Panel> {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return None;
    }

    nav_bar::panel_at_position(
        ratatui::layout::Rect::new(0, 0, state.terminal_width, 3),
        mouse.column,
        mouse.row,
    )
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
            if state.sync_center.is_none() {
                state.sync_center = Some(
                    dashboard
                        .sync_command_center(&state.filter)
                        .map_err(|e| e.to_string()),
                );
                update_scroll_total(state, Panel::Trends);
            }
        }
        Panel::Models => {
            if state.models.is_none() {
                state.models = Some(
                    dashboard
                        .model_breakdown(&state.filter)
                        .map_err(|e| e.to_string()),
                );
                update_scroll_total(state, Panel::Models);
            }
        }
        Panel::Sources => {
            if state.daily.is_none() {
                state.daily = Some(
                    dashboard
                        .trends_daily(&state.filter)
                        .map_err(|e| e.to_string()),
                );
                update_scroll_total(state, Panel::Sources);
            }
        }
        Panel::Projects => {
            if state.hourly.is_none() {
                state.hourly = Some(
                    dashboard
                        .trends("day", &state.filter)
                        .map_err(|e| e.to_string()),
                );
                update_scroll_total(state, Panel::Projects);
            }
        }
        Panel::Cost => {
            if state.costs.is_none() {
                state.costs = Some(
                    dashboard
                        .cost_breakdown(&state.filter)
                        .map_err(|e| e.to_string()),
                );
                update_scroll_total(state, Panel::Cost);
            }
        }
        Panel::Health => {
            if state.stats.is_none() {
                state.stats = Some(load_stats_panel_data(dashboard, state));
                update_scroll_total(state, Panel::Health);
            }
        }
        Panel::Behavior => {
            if state.behavior.is_none() {
                state.behavior = Some(load_behavior_panel_data(dashboard, state));
            }
        }
    }
}

fn update_scroll_total(state: &mut AppState, panel: Panel) {
    let total = match panel {
        Panel::Trends => state
            .sync_center
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .map(|payload| payload.sources.len() + state.platform_probes.len()),
        Panel::Models => state.models.as_ref().and_then(ok_len),
        Panel::Sources => state.daily.as_ref().and_then(ok_len),
        Panel::Projects => state.hourly.as_ref().and_then(ok_len),
        Panel::Cost => state.costs.as_ref().and_then(ok_len),
        Panel::Health => state
            .stats
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .map(|payload| payload.sources.len()),
        _ => None,
    };
    if let Some(total) = total {
        let scroll = &mut state.scroll[panel as usize];
        scroll.total = total;
        scroll.visible = scroll.visible.max(1);
        scroll.offset = scroll.offset.min(total.saturating_sub(scroll.visible));
    }
}

fn ok_len<T>(result: &Result<Vec<T>, String>) -> Option<usize> {
    result.as_ref().ok().map(Vec::len)
}

fn load_behavior_panel_data(
    dashboard: &Dashboard,
    state: &AppState,
) -> Result<BehaviorPanelPayload, String> {
    Ok(BehaviorPanelPayload {
        activity: dashboard
            .activity_breakdown(&state.filter)
            .map_err(|e| e.to_string())?,
        tools: dashboard
            .tool_breakdown(&state.filter)
            .map_err(|e| e.to_string())?,
        optimize: dashboard
            .optimize(&state.filter)
            .map_err(|e| e.to_string())?,
        zombie: dashboard
            .zombie_report(&InventoryRoots::discover())
            .map_err(|e| e.to_string())?,
        compare: dashboard
            .model_compare(&state.filter, None, None)
            .map_err(|e| e.to_string())?,
    })
}

fn load_stats_panel_data(
    dashboard: &Dashboard,
    state: &AppState,
) -> Result<StatsPanelPayload, String> {
    Ok(StatsPanelPayload {
        overview: dashboard
            .overview(&state.filter)
            .map_err(|e| e.to_string())?,
        heatmap: dashboard
            .heatmap(&state.filter, 365)
            .map_err(|e| e.to_string())?,
        sources: dashboard
            .source_breakdown(&state.filter)
            .map_err(|e| e.to_string())?,
        health: dashboard.health().map_err(|e| e.to_string())?,
    })
}
