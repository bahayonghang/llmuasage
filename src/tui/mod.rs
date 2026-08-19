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
pub mod model_vendor;
pub mod nav_bar;
pub mod panels;
mod quota;
pub mod report_table;
pub mod source_picker;
pub mod stacked_bar;
mod sync_control;
pub mod theme;

use app::{AppState, Panel, PeriodDetailKind, PeriodDetailPayload, TableSortKey, stable_sort_refs};
use data_loader::{PanelDataLoader, PanelPayload, PanelRequest, PanelResult};
use event::{EventHandler, TuiEvent};
use input::{Action, DialogAction, handle_dialog_key_event, handle_key_event};
use quota::QuotaController;
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
    let mut quota = QuotaController::new()?;
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
            // Draw reads the backend size. Sync hit-test geometry to that frame
            // if a physical resize landed before the Resize event.
            if let Ok(size) = terminal.size()
                && (size.width != state.terminal_width || size.height != state.terminal_height)
            {
                state.handle_resize(size.width, size.height);
                redraw.request();
            }
        }

        let ev = events.recv()?;
        let action = match ev {
            TuiEvent::Tick => {
                let mut tick_dirty = apply_panel_results(&mut loader, &mut state);
                tick_dirty |= apply_sync_updates(&mut sync, &mut loader, &mut state);
                tick_dirty |= apply_quota_updates(&mut quota, &mut state);
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
            TuiEvent::Mouse(mouse) => {
                if let Some(date) = stats_graph_click(&state, &mouse) {
                    request_period_detail(
                        &mut loader,
                        &mut state,
                        PeriodDetailKind::Daily {
                            date: date.to_string(),
                        },
                    );
                    redraw.request();
                    continue;
                }
                action_from_mouse(&state, &mouse)
            }
            TuiEvent::Key(key) => {
                if matches!(state.active_dialog, Some(app::ActiveDialog::SyncStatus)) {
                    if let Some(action) = sync_overlay_action(key, &mut state) {
                        action
                    } else {
                        redraw.request();
                        continue;
                    }
                } else if state.active_dialog.is_some() {
                    handle_dialog_action(handle_dialog_key_event(key), &mut state);
                    redraw.request();
                    continue;
                } else {
                    handle_key_event(key, state.active_panel)
                }
            }
        };

        match action {
            Action::Quit => {
                sync.shutdown(Duration::from_millis(500));
                quota.shutdown(Duration::from_millis(500));
                loader.cancel_active();
                break;
            }
            Action::Esc => {
                if state.is_period_detail_active() {
                    state.close_period_detail();
                } else {
                    sync.shutdown(Duration::from_millis(500));
                    quota.shutdown(Duration::from_millis(500));
                    loader.cancel_active();
                    break;
                }
            }
            Action::OpenDetail => {
                if let Some(kind) = period_detail_kind(&state) {
                    request_period_detail(&mut loader, &mut state, kind);
                }
            }
            Action::SwitchPanel(p) => {
                state.close_period_detail();
                state.active_panel = p;
                request_panel_data(&mut loader, &mut state, p, false);
                maybe_fetch_quota(&mut quota, store, &mut state, false);
            }
            Action::NextPanel => {
                state.close_period_detail();
                let p = state.active_panel.next();
                state.active_panel = p;
                request_panel_data(&mut loader, &mut state, p, false);
                maybe_fetch_quota(&mut quota, store, &mut state, false);
            }
            Action::PrevPanel => {
                state.close_period_detail();
                let p = state.active_panel.prev();
                state.active_panel = p;
                request_panel_data(&mut loader, &mut state, p, false);
                maybe_fetch_quota(&mut quota, store, &mut state, false);
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
                state.close_period_detail();
                state.time_window = state.time_window.next();
                let panel = state.active_panel;
                invalidate_windowed_panel_data(&mut state);
                if panel_uses_time_window(panel) {
                    request_panel_data(&mut loader, &mut state, panel, false);
                }
            }
            Action::PrevWindow => {
                state.close_period_detail();
                state.time_window = state.time_window.prev();
                let panel = state.active_panel;
                invalidate_windowed_panel_data(&mut state);
                if panel_uses_time_window(panel) {
                    request_panel_data(&mut loader, &mut state, panel, false);
                }
            }
            Action::Refresh => {
                refresh_panel_data(&mut loader, &mut state);
                maybe_fetch_quota(&mut quota, store, &mut state, true);
            }
            Action::OpenSyncStatus => {
                request_panel_data(&mut loader, &mut state, Panel::Trends, false);
                update_overlay_scroll(&mut state);
                state.open_sync_status();
            }
            Action::ToggleUsageEmails => state.toggle_usage_emails(),
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
    state.close_period_detail();
    invalidate_inactive_panel_data(state);
    state.needs_refresh = false;
    request_panel_data(loader, state, panel, true);
    state.set_status("Refreshing local dashboard cache");
}

fn maybe_fetch_quota(
    quota: &mut QuotaController,
    store: &Store,
    state: &mut AppState,
    force: bool,
) {
    if state.active_panel != Panel::Trends {
        return;
    }
    let ctx = quota::production_context(store.paths.subscription_cache_path());
    state.quota_fetch_attempted = true;
    state.quota_fetching = true;
    if force {
        quota.force_fetch(ctx);
    } else {
        quota.fetch_if_needed(ctx);
    }
}

fn apply_quota_updates(quota: &mut QuotaController, state: &mut AppState) -> bool {
    let Some(report) = quota.try_recv() else {
        let fetching = quota.is_fetching();
        if state.quota_fetching != fetching {
            state.quota_fetching = fetching;
            return true;
        }
        return false;
    };
    state.quota_fetching = false;
    state.quota_fetch_attempted = true;
    let message = if report.outputs.is_empty() && report.diagnostics.is_empty() {
        "No subscription credentials found".to_string()
    } else if report.diagnostics.is_empty() {
        format!("Loaded {} quota account(s)", report.outputs.len())
    } else {
        format!(
            "Loaded {} quota account(s), {} issue(s)",
            report.outputs.len(),
            report.diagnostics.len()
        )
    };
    state.quota_report = Some(report);
    update_scroll_total(state, Panel::Trends);
    state.set_status(&message);
    true
}

fn update_overlay_scroll(state: &mut AppState) {
    let total = state
        .sync_center
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map(|payload| payload.sources.len())
        .unwrap_or(0);
    state.sync_overlay_scroll.visible = state.sync_overlay_scroll.visible.max(1);
    state.sync_overlay_scroll.set_total(total);
}

fn sync_overlay_action(key: crossterm::event::KeyEvent, state: &mut AppState) -> Option<Action> {
    match key.code {
        crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Char('q') => {
            state.close_dialog();
            None
        }
        crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
            state.sync_overlay_scroll.scroll_down();
            None
        }
        crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
            state.sync_overlay_scroll.scroll_up();
            None
        }
        crossterm::event::KeyCode::PageDown => {
            state.sync_overlay_scroll.page_down();
            None
        }
        crossterm::event::KeyCode::PageUp => {
            state.sync_overlay_scroll.page_up();
            None
        }
        crossterm::event::KeyCode::Char('x') => Some(Action::StartSync),
        crossterm::event::KeyCode::Char('r') => Some(Action::Refresh),
        crossterm::event::KeyCode::Char('m') => Some(Action::ToggleUsageEmails),
        _ => None,
    }
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
            dashboard_shell(state.terminal_width, state.terminal_height)[0],
            mouse.column,
            mouse.row,
        )
        .map(Action::SwitchPanel)
        .unwrap_or(Action::None),
        _ => Action::None,
    }
}

fn stats_graph_click(
    state: &AppState,
    mouse: &crossterm::event::MouseEvent,
) -> Option<chrono::NaiveDate> {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return None;
    }
    if state.active_dialog.is_some() || state.active_panel != Panel::Health {
        return None;
    }
    let [nav, content, _] = dashboard_shell(state.terminal_width, state.terminal_height);
    if nav_bar::panel_at_position(nav, mouse.column, mouse.row).is_some() {
        return None;
    }
    let payload = state.stats.as_ref()?.as_ref().ok()?;
    let selected = matches!(
        state.period_detail.as_ref().map(|detail| &detail.kind),
        Some(PeriodDetailKind::Daily { .. })
    );
    let graph = panels::stats::split_stats_area(content, selected).graph;
    panels::stats::day_at(graph, &payload.heatmap, mouse.column, mouse.row)
}

fn dashboard_shell(width: u16, height: u16) -> [ratatui::layout::Rect; 3] {
    draw::dashboard_shell_areas(ratatui::layout::Rect::new(0, 0, width, height))
}

fn period_detail_kind(state: &AppState) -> Option<PeriodDetailKind> {
    if state.is_period_detail_active() {
        return None;
    }
    match state.active_panel {
        Panel::Sources => {
            let days = state.daily.as_ref()?.as_ref().ok()?;
            let sort = state.sort[Panel::Sources as usize];
            let ordered =
                stable_sort_refs(days.iter().collect(), sort, |left, right, key| match key {
                    TableSortKey::Date => left.date.cmp(&right.date),
                    TableSortKey::Tokens => left.total_tokens.cmp(&right.total_tokens),
                    TableSortKey::Cost => left
                        .cost_with_cache_usd
                        .total_cmp(&right.cost_with_cache_usd),
                });
            let day = ordered.get(state.scroll[Panel::Sources as usize].selected)?;
            Some(PeriodDetailKind::Daily {
                date: day.date.clone(),
            })
        }
        Panel::Monthly => {
            let months = state.monthly.as_ref()?.as_ref().ok()?;
            let sort = state.sort[Panel::Monthly as usize];
            let ordered =
                stable_sort_refs(
                    months.iter().collect(),
                    sort,
                    |left, right, key| match key {
                        TableSortKey::Date => left.month.cmp(&right.month),
                        TableSortKey::Tokens => left.total_tokens.cmp(&right.total_tokens),
                        TableSortKey::Cost => left
                            .cost_with_cache_usd
                            .total_cmp(&right.cost_with_cache_usd),
                    },
                );
            let month = ordered.get(state.scroll[Panel::Monthly as usize].selected)?;
            Some(PeriodDetailKind::Monthly {
                month: month.month.clone(),
            })
        }
        Panel::Health => {
            let payload = state.stats.as_ref()?.as_ref().ok()?;
            let last = payload.heatmap.last()?;
            Some(PeriodDetailKind::Daily {
                date: last.date.clone(),
            })
        }
        _ => None,
    }
}

fn request_period_detail(
    loader: &mut PanelDataLoader,
    state: &mut AppState,
    kind: PeriodDetailKind,
) {
    state.open_period_detail(kind.clone());
    state.data_generation = state.data_generation.wrapping_add(1);
    state.panel_loading = [false; Panel::COUNT];
    state.panel_loading[state.active_panel as usize] = true;
    loader.request(PanelRequest {
        panel: state.active_panel,
        filter: state.filter.clone(),
        time_window: state.time_window,
        generation: state.data_generation,
        refreshing: false,
        detail: Some(kind),
    });
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
        detail: None,
    });
}

fn apply_panel_results(loader: &mut PanelDataLoader, state: &mut AppState) -> bool {
    let mut dirty = false;
    while let Some(result) = loader.try_recv() {
        dirty |= apply_panel_result(state, result);
    }
    dirty
}

fn apply_panel_result(state: &mut AppState, result: PanelResult) -> bool {
    if !panel_result_matches(state, &result) {
        return false;
    }
    let panel = result.panel;
    let refreshing = result.refreshing;
    match result.payload {
        PanelPayload::Overview(payload) => state.overview = Some(payload),
        PanelPayload::SyncCenter(payload) => {
            state.sync_center = Some(payload);
            update_overlay_scroll(state);
        }
        PanelPayload::Models(payload) => state.models = Some(payload),
        PanelPayload::Daily(payload) => state.daily = Some(payload),
        PanelPayload::Hourly(payload) => state.hourly = Some(payload),
        PanelPayload::Monthly(payload) => state.monthly = Some(payload),
        PanelPayload::DailyDetail(payload) => {
            if let Some(detail) = state.period_detail.as_mut()
                && matches!(detail.kind, PeriodDetailKind::Daily { .. })
            {
                detail.payload = Some(payload.map(PeriodDetailPayload::Daily));
            }
        }
        PanelPayload::MonthlyDetail(payload) => {
            if let Some(detail) = state.period_detail.as_mut()
                && matches!(detail.kind, PeriodDetailKind::Monthly { .. })
            {
                detail.payload = Some(payload.map(PeriodDetailPayload::Monthly));
            }
        }
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
    true
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
        Panel::Monthly => state.monthly.is_some(),
        Panel::Health => state.stats.is_some(),
        Panel::Behavior => state.behavior.is_some(),
        Panel::Blocks => state.blocks.is_some(),
    }
}

fn panel_uses_time_window(panel: Panel) -> bool {
    matches!(
        panel,
        Panel::Overview
            | Panel::Models
            | Panel::Sources
            | Panel::Projects
            | Panel::Monthly
            | Panel::Health
            | Panel::Behavior
    )
}

fn invalidate_windowed_panel_data(state: &mut AppState) {
    state.overview = None;
    state.models = None;
    state.daily = None;
    state.hourly = None;
    state.monthly = None;
    state.period_detail = None;
    state.stats = None;
    state.behavior = None;
    for panel in [
        Panel::Overview,
        Panel::Models,
        Panel::Sources,
        Panel::Projects,
        Panel::Monthly,
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
    }
    if active != Panel::Sources {
        state.daily = None;
    }
    if active != Panel::Projects {
        state.hourly = None;
    }
    if active != Panel::Monthly {
        state.monthly = None;
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
        Panel::Overview => state
            .overview
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .map(|payload| payload.models.len()),
        Panel::Trends => state
            .quota_report
            .as_ref()
            .map(|report| report.outputs.len()),
        Panel::Models => state.models.as_ref().and_then(ok_len),
        Panel::Sources => {
            if let Some(detail) = &state.period_detail {
                match &detail.payload {
                    Some(Ok(PeriodDetailPayload::Daily(rows))) => Some(rows.len()),
                    Some(Ok(PeriodDetailPayload::Monthly(rows))) => Some(rows.len()),
                    _ => Some(0),
                }
            } else {
                state.daily.as_ref().and_then(ok_len)
            }
        }
        Panel::Projects => state.hourly.as_ref().and_then(ok_len),
        Panel::Monthly => {
            if let Some(detail) = &state.period_detail {
                match &detail.payload {
                    Some(Ok(PeriodDetailPayload::Daily(rows))) => Some(rows.len()),
                    Some(Ok(PeriodDetailPayload::Monthly(rows))) => Some(rows.len()),
                    _ => Some(0),
                }
            } else {
                state.monthly.as_ref().and_then(ok_len)
            }
        }
        Panel::Blocks => state.blocks.as_ref().and_then(ok_len),
        Panel::Health => {
            if let Some(detail) = &state.period_detail {
                match &detail.payload {
                    Some(Ok(PeriodDetailPayload::Daily(rows))) => {
                        Some(panels::stats::breakdown_scroll_total(rows))
                    }
                    _ => Some(0),
                }
            } else {
                Some(0)
            }
        }
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
    use crate::paths::AppPaths;
    use crate::query::{ContextPressurePayload, HeatmapPoint, OverviewPayload, TokenSummary};
    use crate::tui::app::{StatsPanelPayload, TimeWindow};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };
    use std::time::Instant;

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

    fn dashboard_content_area(width: u16, height: u16) -> ratatui::layout::Rect {
        dashboard_shell(width, height)[1]
    }

    fn dummy_stats(heatmap: Vec<HeatmapPoint>) -> StatsPanelPayload {
        StatsPanelPayload {
            overview: OverviewPayload {
                generated_at: String::new(),
                total: TokenSummary::default(),
                last_24h: TokenSummary::default(),
                source_count: 0,
                bucket_count: 0,
                total_events: 0,
                last_24h_events: 0,
                total_cost_usd: 0.0,
                cache_efficiency: 0.0,
                last_sync_at: None,
                last_export_at: None,
            },
            heatmap,
            models: Vec::new(),
            context_pressure: ContextPressurePayload {
                peak_percent: 0.0,
                avg_percent: 0.0,
                peak_model: None,
                priced_events: 0,
                unpriced_events: 0,
            },
        }
    }

    #[test]
    fn stats_enter_selects_last_heatmap_date() {
        let mut state = AppState::new();
        state.active_panel = Panel::Health;
        assert!(period_detail_kind(&state).is_none());
        state.stats = Some(Ok(dummy_stats(vec![
            HeatmapPoint {
                date: "2026-08-18".to_string(),
                event_count: 1,
                total_tokens: 10,
            },
            HeatmapPoint {
                date: "2026-08-19".to_string(),
                event_count: 0,
                total_tokens: 0,
            },
        ])));
        assert_eq!(
            period_detail_kind(&state),
            Some(PeriodDetailKind::Daily {
                date: "2026-08-19".to_string()
            })
        );
        state.open_period_detail(PeriodDetailKind::Daily {
            date: "2026-08-19".to_string(),
        });
        assert!(period_detail_kind(&state).is_none());
    }

    #[test]
    fn stats_click_maps_graph_cell_to_heatmap_date() {
        let mut state = AppState::new();
        state.active_panel = Panel::Health;
        state.terminal_width = 80;
        state.terminal_height = 40;
        // 2026-01-04 is Sunday, so the first cell is clickable with no padding.
        state.stats = Some(Ok(dummy_stats(vec![HeatmapPoint {
            date: "2026-01-04".to_string(),
            event_count: 1,
            total_tokens: 100,
        }])));
        let content = dashboard_content_area(state.terminal_width, state.terminal_height);
        let graph = panels::stats::split_stats_area(content, false).graph;
        // 80-col graph uses a 4-col weekday gutter; first Sunday cell is at +5,+3.
        let cell_x = graph.x + 5;
        let cell_y = graph.y + 3;
        {
            let heatmap = &state.stats.as_ref().unwrap().as_ref().unwrap().heatmap;
            let date =
                panels::stats::day_at(graph, heatmap, cell_x, cell_y).expect("first Sunday cell");
            assert_eq!(date.to_string(), "2026-01-04");
        }

        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: cell_x,
            row: cell_y,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(
            stats_graph_click(&state, &mouse).map(|value| value.to_string()),
            Some("2026-01-04".to_string())
        );
        let nav_mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 1,
            modifiers: KeyModifiers::NONE,
        };
        assert!(stats_graph_click(&state, &nav_mouse).is_none());
    }

    #[test]
    fn dashboard_shell_matches_draw_split() {
        let area = ratatui::layout::Rect::new(0, 0, 80, 40);
        let [nav, content, footer] = draw::dashboard_shell_areas(area);
        assert_eq!(nav, ratatui::layout::Rect::new(0, 0, 80, 3));
        assert_eq!(content, dashboard_content_area(80, 40));
        assert_eq!(footer.y, 36);
        assert_eq!(footer.height, 4);
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
            ("daily", include_str!("panels/daily.rs")),
            ("hourly", include_str!("panels/hourly.rs")),
            ("models", include_str!("panels/models.rs")),
            ("monthly", include_str!("panels/monthly.rs")),
            ("stats", include_str!("panels/stats.rs")),
        ];
        for (name, source) in selected_tables {
            assert!(
                source.contains("visible_range("),
                "{name} must construct only the selected visible window"
            );
            assert!(
                source.contains("selection_style()") || source.contains("selection_fill_style()"),
                "{name} must visibly style the selected row"
            );
        }

        for (name, source) in [
            ("usage", include_str!("panels/usage.rs")),
            ("sync_status", include_str!("panels/sync_status.rs")),
        ] {
            assert!(
                source.contains(".skip(") && source.contains(".take("),
                "{name} must bound each scrolled iterator before formatting rows"
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

        for (panel, payload) in [
            (
                Panel::Models,
                PanelPayload::Models(Err("unused".to_string())),
            ),
            (
                Panel::Overview,
                PanelPayload::Overview(Err("unused".to_string())),
            ),
        ] {
            state.active_panel = panel;
            let managed = PanelResult {
                panel,
                filter: state.filter.clone(),
                time_window: TimeWindow::Week7d,
                generation: 4,
                refreshing: false,
                payload,
            };
            assert!(!panel_result_matches(&state, &managed), "{panel:?}");
        }

        for (panel, payload) in [
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

    #[derive(Debug, Clone, Copy)]
    struct RenderThreadSample {
        dispatch_ms: f64,
        loading_draw_ms: f64,
        result_apply_ms: f64,
        populated_draw_ms: f64,
    }

    impl RenderThreadSample {
        fn max_ms(self) -> f64 {
            self.dispatch_ms
                .max(self.loading_draw_ms)
                .max(self.result_apply_ms)
                .max(self.populated_draw_ms)
        }
    }

    #[ignore = "reads the local usage database for release-mode performance evidence"]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn measure_local_render_thread_first_visit() -> Result<()> {
        let paths = AppPaths::discover()?;
        let database_bytes = std::fs::metadata(&paths.db_path)?.len();
        let store = Store::new(&paths)?;
        let mut loader = PanelDataLoader::new(&store)?;

        eprintln!("database_bytes={database_bytes} window=30d samples=3");
        for (panel, baseline_sync_ms) in [
            (Panel::Health, 169.3),
            (Panel::Behavior, 3777.5),
            (Panel::Blocks, 403.2),
        ] {
            let _ = measure_render_thread_visit(&mut loader, panel).await?;

            let mut maxima = Vec::with_capacity(3);
            for sample_number in 1..=3 {
                let sample = measure_render_thread_visit(&mut loader, panel).await?;
                let max_ms = sample.max_ms();
                maxima.push(max_ms);
                eprintln!(
                    "panel={} sample={} dispatch_ms={:.3} loading_draw_ms={:.3} result_apply_ms={:.3} populated_draw_ms={:.3} max_render_thread_ms={:.3}",
                    benchmark_panel_label(panel),
                    sample_number,
                    sample.dispatch_ms,
                    sample.loading_draw_ms,
                    sample.result_apply_ms,
                    sample.populated_draw_ms,
                    max_ms,
                );
            }

            let median_ms = median(&mut maxima);
            eprintln!(
                "panel={} baseline_sync_ms={baseline_sync_ms:.1} max_render_thread_samples_ms={maxima:?} median_max_render_thread_ms={median_ms:.3} improvement_pct={:.1}",
                benchmark_panel_label(panel),
                (baseline_sync_ms - median_ms) * 100.0 / baseline_sync_ms,
            );
        }
        Ok(())
    }

    async fn measure_render_thread_visit(
        loader: &mut PanelDataLoader,
        panel: Panel,
    ) -> Result<RenderThreadSample> {
        let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
        let mut state = AppState::new();
        state.active_panel = panel;
        state.time_window = TimeWindow::Month30d;

        let started = Instant::now();
        request_panel_data(loader, &mut state, panel, false);
        let dispatch_ms = elapsed_ms(started);

        let started = Instant::now();
        terminal.draw(|frame| draw::draw(frame, &state))?;
        let loading_draw_ms = elapsed_ms(started);
        anyhow::ensure!(
            buffer_text(&terminal).contains("Loading"),
            "{} did not render its loading frame",
            benchmark_panel_label(panel)
        );

        let result = tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                if let Some(result) = loader.try_recv() {
                    break result;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for {}", benchmark_panel_label(panel)))?;
        assert_panel_result_ok(panel, &result)?;

        let started = Instant::now();
        let accepted = apply_panel_result(&mut state, result);
        let result_apply_ms = elapsed_ms(started);
        anyhow::ensure!(
            accepted,
            "{} result was rejected",
            benchmark_panel_label(panel)
        );

        let started = Instant::now();
        terminal.draw(|frame| draw::draw(frame, &state))?;
        let populated_draw_ms = elapsed_ms(started);
        anyhow::ensure!(
            !state.panel_loading[panel as usize] && panel_has_data(&state, panel),
            "{} did not reach populated state",
            benchmark_panel_label(panel)
        );
        anyhow::ensure!(
            !buffer_text(&terminal).contains("Loading"),
            "{} still rendered its loading frame",
            benchmark_panel_label(panel)
        );

        Ok(RenderThreadSample {
            dispatch_ms,
            loading_draw_ms,
            result_apply_ms,
            populated_draw_ms,
        })
    }

    fn assert_panel_result_ok(panel: Panel, result: &PanelResult) -> Result<()> {
        let error = match &result.payload {
            PanelPayload::Stats(Err(error)) | PanelPayload::Blocks(Err(error)) => {
                Some(error.as_str())
            }
            PanelPayload::Behavior(payload) => {
                payload.as_ref().as_ref().err().map(|error| error.as_str())
            }
            _ => None,
        };
        anyhow::ensure!(result.panel == panel, "received unexpected panel result");
        if let Some(error) = error {
            anyhow::bail!("{} query failed: {error}", benchmark_panel_label(panel));
        }
        Ok(())
    }

    fn benchmark_panel_label(panel: Panel) -> &'static str {
        match panel {
            Panel::Health => "Stats",
            Panel::Behavior => "Behavior",
            Panel::Blocks => "Blocks",
            _ => panel.label(),
        }
    }

    fn elapsed_ms(started: Instant) -> f64 {
        started.elapsed().as_secs_f64() * 1_000.0
    }

    fn median(values: &mut [f64]) -> f64 {
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
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

        for dialog in [
            app::ActiveDialog::SourcePicker,
            app::ActiveDialog::Help,
            app::ActiveDialog::SyncStatus,
        ] {
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

            for dialog in [
                app::ActiveDialog::SourcePicker,
                app::ActiveDialog::Help,
                app::ActiveDialog::SyncStatus,
            ] {
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
