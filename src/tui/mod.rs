use std::{io, time::Duration};

use anyhow::Result;
use crossterm::{
    cursor,
    event::{DisableMouseCapture, EnableMouseCapture, MouseButton, MouseEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{query::QueryFilter, store::Store};

pub mod app;
mod data_loader;
pub mod draw;
pub mod event;
pub mod footer;
pub mod help_dialog;
pub mod input;
pub mod nav_bar;
pub mod panels;
pub mod report_table;
pub mod source_picker;
mod sync_control;
pub mod theme;

use app::{AppState, Panel};
use data_loader::{PanelDataLoader, PanelPayload, PanelRequest, PanelResult};
use event::{EventHandler, TuiEvent};
use input::{Action, DialogAction, handle_dialog_key_event, handle_key_event};
use sync_control::{SyncController, SyncUpdate};

/// Main entry point for the interactive terminal dashboard.
pub fn run_dashboard(store: &Store) -> Result<()> {
    // 0. Honor an optional initial theme from the environment.
    if let Ok(name) = std::env::var("LLMUSAGE_THEME") {
        theme::set_theme_by_name(&name);
    }

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
    let mut state = AppState::new();
    let mut sync = SyncController::new()?;
    let mut loader = PanelDataLoader::new(store)?;
    let size = terminal.size()?;
    state.handle_resize(size.width, size.height);
    let mut events = EventHandler::new(std::time::Duration::from_millis(250));

    // Load overview data initially (default panel)
    request_panel_data(&mut loader, &mut state, Panel::Overview, false);

    loop {
        // Draw frame
        terminal.draw(|frame| draw::draw(frame, &state))?;

        let ev = events.recv()?;
        let action = match ev {
            TuiEvent::Tick => {
                apply_panel_results(&mut loader, &mut state);
                apply_sync_updates(&mut sync, &mut loader, &mut state);
                state.on_tick();
                if state.needs_refresh {
                    refresh_panel_data(&mut loader, &mut state);
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
            Action::Quit => {
                sync.shutdown(Duration::from_millis(500));
                loader.cancel_active();
                break;
            }
            Action::SwitchPanel(p) => {
                state.active_panel = p;
                request_panel_data(&mut loader, &mut state, p, false);
            }
            Action::NextPanel => {
                let p = state.active_panel.next();
                state.active_panel = p;
                request_panel_data(&mut loader, &mut state, p, false);
            }
            Action::PrevPanel => {
                let p = state.active_panel.prev();
                state.active_panel = p;
                request_panel_data(&mut loader, &mut state, p, false);
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
                request_panel_data(&mut loader, &mut state, panel, true);
            }
            Action::PrevWindow => {
                state.time_window = state.time_window.prev();
                state.trends = None;
                let panel = state.active_panel;
                request_panel_data(&mut loader, &mut state, panel, true);
            }
            Action::Refresh => refresh_panel_data(&mut loader, &mut state),
            Action::ToggleAutoRefresh => state.toggle_auto_refresh(),
            Action::StartSync => {
                let message = sync.start_or_cancel(store, state.filter.source);
                state.set_status(&message);
            }
            Action::OpenSourcePicker => state.open_source_picker(),
            Action::OpenHelp => state.open_help(),
            Action::CycleTheme => {
                let name = theme::cycle_theme();
                state.set_status(&format!("theme: {name}"));
            }
            Action::None => {}
        }
    }

    Ok(())
}

fn apply_sync_updates(
    sync: &mut SyncController,
    loader: &mut PanelDataLoader,
    state: &mut AppState,
) {
    for update in sync.drain_updates() {
        match update {
            SyncUpdate::Progress(message) => state.set_status(&message),
            SyncUpdate::Completed { inserted, stored } => {
                invalidate_inactive_panel_data(state);
                request_panel_data(loader, state, state.active_panel, true);
                state.set_status(&format!(
                    "Sync complete: {inserted} inserted, {stored} stored"
                ));
            }
            SyncUpdate::Failed(error) => state.set_status(&format!("Sync failed: {error}")),
            SyncUpdate::Cancelled => state.set_status("Sync cancelled"),
        }
    }
}

fn refresh_panel_data(loader: &mut PanelDataLoader, state: &mut AppState) {
    let panel = state.active_panel;
    invalidate_inactive_panel_data(state);
    state.needs_refresh = false;
    request_panel_data(loader, state, panel, true);
    state.set_status("Refreshing local dashboard cache");
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

fn request_panel_data(
    loader: &mut PanelDataLoader,
    state: &mut AppState,
    panel: Panel,
    force: bool,
) {
    if !force && panel_has_data(state, panel) {
        return;
    }
    state.data_generation = state.data_generation.wrapping_add(1);
    state.panel_loading = [false; Panel::COUNT];
    state.panel_loading[panel as usize] = true;
    loader.request(PanelRequest {
        panel,
        filter: state.filter.clone(),
        time_window: state.time_window,
        generation: state.data_generation,
        refreshing: panel_has_data(state, panel),
    });
}

fn apply_panel_results(loader: &mut PanelDataLoader, state: &mut AppState) {
    while let Some(result) = loader.try_recv() {
        if !panel_result_matches(state, &result) {
            continue;
        }
        let panel = result.panel;
        let refreshing = result.refreshing;
        match result.payload {
            PanelPayload::Overview(payload) => state.overview = Some(payload),
            PanelPayload::SyncCenter(payload) => state.sync_center = Some(payload),
            PanelPayload::Models(payload) => state.models = Some(payload),
            PanelPayload::Daily(payload) => state.daily = Some(payload),
            PanelPayload::Hourly(payload) => state.hourly = Some(payload),
            PanelPayload::Costs(payload) => state.costs = Some(payload),
            PanelPayload::Stats(payload) => state.stats = Some(payload),
            PanelPayload::Behavior(payload) => state.behavior = Some(*payload),
            PanelPayload::Blocks(payload) => state.blocks = Some(payload),
        }
        state.panel_loading[panel as usize] = false;
        update_scroll_total(state, panel);
        state.mark_refreshed();
        if refreshing {
            state.set_status("Refreshed local dashboard cache");
        }
    }
}

fn panel_result_matches(state: &AppState, result: &PanelResult) -> bool {
    result.generation == state.data_generation
        && result.panel == state.active_panel
        && result.time_window == state.time_window
        && filters_match(&result.filter, &state.filter)
}

fn filters_match(left: &QueryFilter, right: &QueryFilter) -> bool {
    left.source == right.source
        && left.model == right.model
        && left.since == right.since
        && left.until == right.until
        && left.project_hash == right.project_hash
        && left.timezone == right.timezone
}

fn panel_has_data(state: &AppState, panel: Panel) -> bool {
    match panel {
        Panel::Overview => state.overview.is_some(),
        Panel::Trends => state.sync_center.is_some(),
        Panel::Models => state.models.is_some(),
        Panel::Sources => state.daily.is_some(),
        Panel::Projects => state.hourly.is_some(),
        Panel::Cost => state.costs.is_some(),
        Panel::Health => state.stats.is_some(),
        Panel::Behavior => state.behavior.is_some(),
        Panel::Blocks => state.blocks.is_some(),
    }
}

fn invalidate_inactive_panel_data(state: &mut AppState) {
    let active = state.active_panel;
    if active != Panel::Overview {
        state.overview = None;
    }
    if active != Panel::Trends {
        state.sync_center = None;
    }
    if active != Panel::Models {
        state.models = None;
    }
    if active != Panel::Sources {
        state.daily = None;
    }
    if active != Panel::Projects {
        state.hourly = None;
    }
    if active != Panel::Cost {
        state.costs = None;
    }
    if active != Panel::Health {
        state.stats = None;
    }
    if active != Panel::Behavior {
        state.behavior = None;
    }
    if active != Panel::Blocks {
        state.blocks = None;
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
        Panel::Blocks => state.blocks.as_ref().and_then(ok_len),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn stale_generation_and_filter_results_are_rejected() {
        let mut state = AppState::new();
        state.active_panel = Panel::Models;
        state.data_generation = 2;
        let stale = PanelResult {
            panel: Panel::Models,
            filter: state.filter.clone(),
            time_window: state.time_window,
            generation: 1,
            refreshing: false,
            payload: PanelPayload::Models(Err("stale".to_string())),
        };
        assert!(!panel_result_matches(&state, &stale));

        let mut wrong_filter = state.filter.clone();
        wrong_filter.source = Some(crate::models::SourceKind::Codex);
        let wrong_filter = PanelResult {
            panel: Panel::Models,
            filter: wrong_filter,
            time_window: state.time_window,
            generation: 2,
            refreshing: false,
            payload: PanelPayload::Models(Err("wrong filter".to_string())),
        };
        assert!(!panel_result_matches(&state, &wrong_filter));
    }

    #[test]
    fn heavy_panels_render_loading_before_results_arrive() -> Result<()> {
        for (panel, expected) in [
            (Panel::Behavior, "加 载 中"),
            (Panel::Health, "Loading"),
            (Panel::Blocks, "加 载 中"),
        ] {
            let backend = TestBackend::new(120, 30);
            let mut terminal = Terminal::new(backend)?;
            let mut state = AppState::new();
            state.active_panel = panel;
            state.panel_loading[panel as usize] = true;
            terminal.draw(|frame| draw::draw(frame, &state))?;
            let text = buffer_text(&terminal);
            assert!(
                text.contains(expected),
                "{panel:?} should render its loading placeholder: {text:?}"
            );
        }
        Ok(())
    }
}
