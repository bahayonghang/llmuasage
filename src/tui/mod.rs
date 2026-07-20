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
pub mod format;
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

#[derive(Debug)]
struct RedrawState {
    dirty: bool,
}

impl RedrawState {
    fn initial() -> Self {
        Self { dirty: true }
    }

    fn request(&mut self) {
        self.dirty = true;
    }

    fn take(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

/// Main entry point for the interactive terminal dashboard.
pub fn run_dashboard(store: &Store) -> Result<()> {
    // 0. Resolve theme and terminal color capability before entering raw mode.
    theme::configure_from_env();

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
    let mut redraw = RedrawState::initial();

    loop {
        if redraw.take() {
            terminal.draw(|frame| draw::draw(frame, &state))?;
        }

        let ev = events.recv()?;
        let action = match ev {
            TuiEvent::Tick => {
                let mut tick_dirty = apply_panel_results(&mut loader, &mut state);
                tick_dirty |= apply_sync_updates(&mut sync, &mut loader, &mut state);
                state.sync_active = sync.is_active();
                let animation_active = state.background_active();
                tick_dirty |= state.on_tick(animation_active);
                if state.needs_refresh {
                    refresh_panel_data(&mut loader, &mut state);
                    tick_dirty = true;
                }
                if tick_dirty {
                    redraw.request();
                }
                continue;
            }
            TuiEvent::Resize(width, height) => {
                state.handle_resize(width, height);
                redraw.request();
                continue;
            }
            TuiEvent::Mouse(mouse) => action_from_mouse(&state, &mouse),
            TuiEvent::Key(key) => {
                if state.active_dialog.is_some() {
                    handle_dialog_action(handle_dialog_key_event(key), &mut state);
                    redraw.request();
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
            Action::PageDown => {
                state.scroll[state.active_panel as usize].page_down();
            }
            Action::PageUp => {
                state.scroll[state.active_panel as usize].page_up();
            }
            Action::SelectFirst => {
                state.scroll[state.active_panel as usize].select_first();
            }
            Action::SelectLast => {
                state.scroll[state.active_panel as usize].select_last();
            }
            Action::CycleSort => {
                if let Some((key, descending)) = state.cycle_sort() {
                    let panel = state.active_panel;
                    update_scroll_total(&mut state, panel);
                    state.set_status(&format!(
                        "Sort: {} {}",
                        key.label(),
                        if descending {
                            "descending"
                        } else {
                            "ascending"
                        }
                    ));
                }
            }
            Action::ReverseSort => {
                if let Some((key, descending)) = state.reverse_sort() {
                    let panel = state.active_panel;
                    update_scroll_total(&mut state, panel);
                    state.set_status(&format!(
                        "Sort: {} {}",
                        key.label(),
                        if descending {
                            "descending"
                        } else {
                            "ascending"
                        }
                    ));
                }
            }
            Action::NextWindow => {
                state.time_window = state.time_window.next();
                let panel = state.active_panel;
                invalidate_windowed_panel_data(&mut state);
                if panel_uses_time_window(panel) {
                    request_panel_data(&mut loader, &mut state, panel, false);
                }
            }
            Action::PrevWindow => {
                state.time_window = state.time_window.prev();
                let panel = state.active_panel;
                invalidate_windowed_panel_data(&mut state);
                if panel_uses_time_window(panel) {
                    request_panel_data(&mut loader, &mut state, panel, false);
                }
            }
            Action::Refresh => refresh_panel_data(&mut loader, &mut state),
            Action::ToggleAutoRefresh => state.toggle_auto_refresh(),
            Action::StartSync => {
                let message = sync.start_or_cancel(store, state.filter.source);
                state.sync_active = sync.is_active();
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
        redraw.request();
    }

    Ok(())
}

fn apply_sync_updates(
    sync: &mut SyncController,
    loader: &mut PanelDataLoader,
    state: &mut AppState,
) -> bool {
    let mut dirty = false;
    for update in sync.drain_updates() {
        dirty = true;
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
    dirty
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

fn action_from_mouse(state: &AppState, mouse: &crossterm::event::MouseEvent) -> Action {
    match mouse.kind {
        MouseEventKind::ScrollDown => Action::ScrollDown,
        MouseEventKind::ScrollUp => Action::ScrollUp,
        MouseEventKind::Down(MouseButton::Left) => nav_bar::panel_at_position(
            ratatui::layout::Rect::new(0, 0, state.terminal_width, 3),
            mouse.column,
            mouse.row,
        )
        .map(Action::SwitchPanel)
        .unwrap_or(Action::None),
        _ => Action::None,
    }
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

fn apply_panel_results(loader: &mut PanelDataLoader, state: &mut AppState) -> bool {
    let mut dirty = false;
    while let Some(result) = loader.try_recv() {
        if !panel_result_matches(state, &result) {
            continue;
        }
        let panel = result.panel;
        let refreshing = result.refreshing;
        match result.payload {
            PanelPayload::Overview(payload) => state.overview = Some(payload),
            PanelPayload::SyncCenter(payload) => state.sync_center = Some(payload),
            PanelPayload::Models(payload) => {
                state.model_collapse = payload
                    .as_ref()
                    .ok()
                    .and_then(|items| panels::models::collapse_plan(items));
                state.models = Some(payload);
            }
            PanelPayload::Daily(payload) => state.daily = Some(payload),
            PanelPayload::Hourly(payload) => state.hourly = Some(payload),
            PanelPayload::Costs(payload) => {
                state.cost_collapse = payload
                    .as_ref()
                    .ok()
                    .and_then(|items| panels::cost::collapse_plan(items));
                state.costs = Some(payload);
            }
            PanelPayload::Stats(payload) => state.stats = Some(payload),
            PanelPayload::Behavior(payload) => state.behavior = Some(*payload),
            PanelPayload::Blocks(payload) => state.blocks = Some(payload),
        }
        state.panel_loading[panel as usize] = false;
        update_scroll_total(state, panel);
        state.mark_refreshed();
        dirty = true;
        if refreshing {
            state.set_status("Refreshed local dashboard cache");
        }
    }
    dirty
}

fn panel_result_matches(state: &AppState, result: &PanelResult) -> bool {
    result.generation == state.data_generation
        && result.panel == state.active_panel
        && (!panel_uses_time_window(result.panel) || result.time_window == state.time_window)
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

fn panel_uses_time_window(panel: Panel) -> bool {
    matches!(
        panel,
        Panel::Models
            | Panel::Sources
            | Panel::Projects
            | Panel::Cost
            | Panel::Health
            | Panel::Behavior
    )
}

fn invalidate_windowed_panel_data(state: &mut AppState) {
    state.models = None;
    state.model_collapse = None;
    state.daily = None;
    state.hourly = None;
    state.costs = None;
    state.cost_collapse = None;
    state.stats = None;
    state.behavior = None;
    for panel in [
        Panel::Models,
        Panel::Sources,
        Panel::Projects,
        Panel::Cost,
        Panel::Health,
        Panel::Behavior,
    ] {
        state.scroll[panel as usize].offset = 0;
        state.scroll[panel as usize].selected = 0;
        state.panel_loading[panel as usize] = false;
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
        state.model_collapse = None;
    }
    if active != Panel::Sources {
        state.daily = None;
    }
    if active != Panel::Projects {
        state.hourly = None;
    }
    if active != Panel::Cost {
        state.costs = None;
        state.cost_collapse = None;
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
        Panel::Models => state.models.as_ref().and_then(ok_len).map(|raw| {
            if state.sort[Panel::Models as usize].key.is_some() {
                raw
            } else {
                state.model_collapse.map_or(raw, |plan| plan.keep + 1)
            }
        }),
        Panel::Sources => state.daily.as_ref().and_then(ok_len),
        Panel::Projects => state.hourly.as_ref().and_then(ok_len),
        Panel::Cost => state.costs.as_ref().and_then(ok_len).map(|raw| {
            if state.sort[Panel::Cost as usize].key.is_some() {
                raw
            } else {
                state.cost_collapse.map_or(raw, |plan| plan.keep + 1)
            }
        }),
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
        scroll.visible = scroll.visible.max(1);
        scroll.set_total(total);
    }
}

fn ok_len<T>(result: &Result<Vec<T>, String>) -> Option<usize> {
    result.as_ref().ok().map(Vec::len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::TimeWindow;
    use crossterm::event::{KeyModifiers, MouseEvent};
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };

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
    fn idle_ticks_produce_zero_draws_after_the_initial_frame() {
        let mut redraw = RedrawState::initial();
        assert!(redraw.take(), "initial frame must render");

        let idle_draws = (0..40).filter(|_| redraw.take()).count();
        assert_eq!(idle_draws, 0, "ten seconds of 250ms ticks stay idle");

        for _ in 0..4 {
            redraw.request();
            assert!(redraw.take(), "active animation ticks request frames");
        }
    }

    #[test]
    fn mouse_wheel_maps_to_selection_actions() {
        let state = AppState::new();
        let mouse = |kind| MouseEvent {
            kind,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(
            action_from_mouse(&state, &mouse(MouseEventKind::ScrollDown)),
            Action::ScrollDown
        );
        assert_eq!(
            action_from_mouse(&state, &mouse(MouseEventKind::ScrollUp)),
            Action::ScrollUp
        );
    }

    #[test]
    fn every_scrolled_table_bounds_row_construction_to_the_viewport() {
        let selected_tables = [
            ("blocks", include_str!("panels/blocks.rs")),
            ("cost", include_str!("panels/cost.rs")),
            ("daily", include_str!("panels/daily.rs")),
            ("hourly", include_str!("panels/hourly.rs")),
            ("models", include_str!("panels/models.rs")),
            ("stats", include_str!("panels/stats.rs")),
        ];
        for (name, source) in selected_tables {
            assert!(
                source.contains("visible_range("),
                "{name} must construct only the selected visible window"
            );
            assert!(
                source.contains("selection_style()"),
                "{name} must visibly style the selected row"
            );
        }

        let usage = include_str!("panels/usage.rs");
        for tail in usage.split(".skip(scroll.offset)").skip(1) {
            assert!(
                tail.trim_start().starts_with(".take("),
                "usage must bound each scrolled iterator before formatting rows"
            );
        }
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
    fn window_mismatch_is_rejected_only_for_managed_panels() {
        let mut state = AppState::new();
        state.data_generation = 4;
        state.time_window = TimeWindow::All;

        state.active_panel = Panel::Models;
        let managed = PanelResult {
            panel: Panel::Models,
            filter: state.filter.clone(),
            time_window: TimeWindow::Week7d,
            generation: 4,
            refreshing: false,
            payload: PanelPayload::Models(Err("unused".to_string())),
        };
        assert!(!panel_result_matches(&state, &managed));

        for (panel, payload) in [
            (
                Panel::Overview,
                PanelPayload::Overview(Err("unused".to_string())),
            ),
            (
                Panel::Trends,
                PanelPayload::SyncCenter(Err("unused".to_string())),
            ),
            (
                Panel::Blocks,
                PanelPayload::Blocks(Err("unused".to_string())),
            ),
        ] {
            state.active_panel = panel;
            let lifetime = PanelResult {
                panel,
                filter: state.filter.clone(),
                time_window: TimeWindow::Week7d,
                generation: 4,
                refreshing: false,
                payload,
            };
            assert!(panel_result_matches(&state, &lifetime), "{panel:?}");
        }
    }

    #[test]
    fn heavy_panels_render_loading_before_results_arrive() -> Result<()> {
        for (panel, expected) in [
            (Panel::Behavior, "Loading"),
            (Panel::Health, "Loading"),
            (Panel::Blocks, "Loading"),
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

    #[test]
    fn no_color_dashboard_buffers_have_no_styles() -> Result<()> {
        theme::set_color_mode(theme::TerminalColorMode::NoColor);
        theme::set_theme(theme::Theme::graphite());

        for panel in Panel::all() {
            let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
            let mut state = AppState::new();
            state.active_panel = *panel;
            terminal.draw(|frame| draw::draw(frame, &state))?;
            assert_unstyled(&terminal, panel.label());
        }

        for dialog in [app::ActiveDialog::SourcePicker, app::ActiveDialog::Help] {
            let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
            let mut state = AppState::new();
            state.active_dialog = Some(dialog);
            terminal.draw(|frame| draw::draw(frame, &state))?;
            assert_unstyled(&terminal, "dialog");
        }

        theme::set_color_mode(theme::TerminalColorMode::TrueColor);
        theme::set_theme(theme::Theme::default_dark());
        Ok(())
    }

    #[test]
    fn every_theme_reaches_all_panel_shells_and_dialogs() -> Result<()> {
        theme::set_color_mode(theme::TerminalColorMode::TrueColor);

        for selected_theme in theme::Theme::ALL {
            theme::set_theme(selected_theme);
            let accent = theme::active_theme().accent;
            for panel in Panel::all() {
                let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
                let mut state = AppState::new();
                state.active_panel = *panel;
                terminal.draw(|frame| draw::draw(frame, &state))?;
                assert!(
                    terminal
                        .backend()
                        .buffer()
                        .content()
                        .iter()
                        .any(|cell| cell.fg == accent || cell.bg == accent),
                    "{} must reach {}",
                    selected_theme.name,
                    panel.label()
                );
            }

            for dialog in [app::ActiveDialog::SourcePicker, app::ActiveDialog::Help] {
                let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
                let mut state = AppState::new();
                state.active_dialog = Some(dialog);
                terminal.draw(|frame| draw::draw(frame, &state))?;
                assert!(
                    terminal
                        .backend()
                        .buffer()
                        .content()
                        .iter()
                        .any(|cell| cell.fg == accent || cell.bg == accent),
                    "{} must reach dialog",
                    selected_theme.name
                );
            }
        }

        theme::set_theme(theme::Theme::default_dark());
        Ok(())
    }

    fn assert_unstyled(terminal: &Terminal<TestBackend>, label: &str) {
        for cell in terminal.backend().buffer().content() {
            assert_eq!(cell.fg, Color::Reset, "{label} foreground");
            assert_eq!(cell.bg, Color::Reset, "{label} background");
            assert_eq!(cell.modifier, Modifier::empty(), "{label} modifier");
        }
    }
}
