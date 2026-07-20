use crate::query::reports::BlockReportRow;
use crate::query::{
    ActivityPayload, ContextPressurePayload, CostLine, DailyTrendPoint, HealthPayload,
    HeatmapPoint, ModelBreakdown, ModelComparePayload, OptimizePayload, OverviewPayload,
    ProjectBreakdown, QueryFilter, SourceBreakdown, SyncCommandCenterPayload, ToolsPayload,
    TrendPoint, ZombieReport,
};
use crate::{domain::platform_monitor::PlatformProbe, models::SourceKind};

use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use std::time::{Duration, Instant};

use super::panels::longtail::Collapsed;

/// The eight dashboard panels in fixed display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Panel {
    Overview = 0,
    Trends = 1,
    Models = 2,
    Sources = 3,
    Projects = 4,
    Cost = 5,
    Health = 6,
    Behavior = 7,
    Blocks = 8,
}

impl Panel {
    pub const COUNT: usize = 9;

    pub fn all() -> &'static [Self] {
        &[
            Self::Overview,
            Self::Trends,
            Self::Models,
            Self::Sources,
            Self::Projects,
            Self::Cost,
            Self::Health,
            Self::Behavior,
            Self::Blocks,
        ]
    }

    pub fn from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Self::Overview),
            1 => Some(Self::Trends),
            2 => Some(Self::Models),
            3 => Some(Self::Sources),
            4 => Some(Self::Projects),
            5 => Some(Self::Cost),
            6 => Some(Self::Health),
            7 => Some(Self::Behavior),
            8 => Some(Self::Blocks),
            _ => None,
        }
    }

    pub fn from_digit_char(c: char) -> Option<Self> {
        let digit = c.to_digit(10)? as usize;
        if (1..=Self::COUNT).contains(&digit) {
            Self::from_index(digit - 1)
        } else {
            None
        }
    }

    pub fn next(self) -> Self {
        Self::from_index((self as usize + 1) % Self::COUNT).unwrap()
    }

    pub fn prev(self) -> Self {
        Self::from_index((self as usize + Self::COUNT - 1) % Self::COUNT).unwrap()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Trends => "Usage",
            Self::Models => "Models",
            Self::Sources => "Daily",
            Self::Projects => "Hourly",
            Self::Cost => "Cost",
            Self::Health => "Stats",
            Self::Behavior => "Agents",
            Self::Blocks => "Blocks",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Overview => "Ovw",
            Self::Trends => "Use",
            Self::Models => "Mod",
            Self::Sources => "Day",
            Self::Projects => "Hr",
            Self::Cost => "Cost",
            Self::Health => "Sta",
            Self::Behavior => "Agt",
            Self::Blocks => "Blk",
        }
    }
}

/// Combined behavior analytics payload for the terminal dashboard.
#[derive(Debug, Clone)]
pub struct BehaviorPanelPayload {
    pub activity: ActivityPayload,
    pub tools: ToolsPayload,
    pub optimize: OptimizePayload,
    pub zombie: ZombieReport,
    pub compare: ModelComparePayload,
}

/// Combined read-only facts for the tokscale-style stats panel.
#[derive(Debug, Clone)]
pub struct StatsPanelPayload {
    pub overview: OverviewPayload,
    pub heatmap: Vec<HeatmapPoint>,
    pub sources: Vec<SourceBreakdown>,
    pub health: HealthPayload,
    pub context_pressure: ContextPressurePayload,
}

/// Time window for the trends panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeWindow {
    Day24h,
    Week7d,
    Month30d,
    All,
}

impl TimeWindow {
    /// Advance to the next window, clamped at `All`.
    pub fn next(self) -> Self {
        match self {
            Self::Day24h => Self::Week7d,
            Self::Week7d => Self::Month30d,
            Self::Month30d => Self::All,
            Self::All => Self::All,
        }
    }

    /// Retreat to the previous window, clamped at `Day24h`.
    pub fn prev(self) -> Self {
        match self {
            Self::Day24h => Self::Day24h,
            Self::Week7d => Self::Day24h,
            Self::Month30d => Self::Week7d,
            Self::All => Self::Month30d,
        }
    }

    /// Query string parameter for `Dashboard::trends()`.
    pub fn as_query_str(&self) -> &'static str {
        match self {
            Self::Day24h => "day",
            Self::Week7d => "week",
            Self::Month30d => "month",
            Self::All => "all",
        }
    }

    /// Human-readable label for the UI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Day24h => "Today",
            Self::Week7d => "7d",
            Self::Month30d => "30d",
            Self::All => "All",
        }
    }

    pub fn query_filter(self, base: &QueryFilter) -> QueryFilter {
        let today = base.timezone.date_at(Utc::now());
        self.query_filter_on(base, today)
    }

    fn query_filter_on(self, base: &QueryFilter, today: NaiveDate) -> QueryFilter {
        let mut filter = base.clone();
        match self {
            Self::Day24h => {
                filter.since = Some(today);
                filter.until = Some(today);
            }
            Self::Week7d => {
                filter.since = Some(today - ChronoDuration::days(6));
                filter.until = Some(today);
            }
            Self::Month30d => {
                filter.since = Some(today - ChronoDuration::days(29));
                filter.until = Some(today);
            }
            Self::All => {
                filter.since = None;
                filter.until = None;
            }
        }
        filter
    }
}

/// Per-panel scroll position for table views.
#[derive(Debug, Clone)]
pub struct ScrollState {
    pub offset: usize,
    pub total: usize,
    pub visible: usize,
}

impl ScrollState {
    pub fn scroll_down(&mut self) {
        if self.offset < self.total.saturating_sub(self.visible.max(1)) {
            self.offset += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        self.offset = self.offset.saturating_sub(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveDialog {
    SourcePicker,
    Help,
}

#[derive(Debug, Clone)]
pub struct SourcePickerState {
    pub selected: usize,
}

impl SourcePickerState {
    fn clamp(&mut self, len: usize) {
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    fn move_by(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        let len = len as isize;
        let next = (self.selected as isize + delta).rem_euclid(len);
        self.selected = next as usize;
    }
}

/// Flat application state for the dashboard.
pub struct AppState {
    pub active_panel: Panel,
    pub time_window: TimeWindow,
    pub scroll: [ScrollState; Panel::COUNT],
    pub filter: QueryFilter,
    // Cached data (loaded on panel switch)
    pub overview: Option<Result<OverviewPayload, String>>,
    pub sync_center: Option<Result<SyncCommandCenterPayload, String>>,
    pub trends: Option<Result<Vec<TrendPoint>, String>>,
    pub models: Option<Result<Vec<ModelBreakdown>, String>>,
    pub daily: Option<Result<Vec<DailyTrendPoint>, String>>,
    pub hourly: Option<Result<Vec<TrendPoint>, String>>,
    pub sources: Option<Result<Vec<SourceBreakdown>, String>>,
    pub projects: Option<Result<Vec<ProjectBreakdown>, String>>,
    pub costs: Option<Result<Vec<CostLine>, String>>,
    pub health: Option<Result<HealthPayload, String>>,
    pub stats: Option<Result<StatsPanelPayload, String>>,
    pub behavior: Option<Result<BehaviorPanelPayload, String>>,
    pub blocks: Option<Result<Vec<BlockReportRow>, String>>,
    pub platform_probes: Vec<PlatformProbe>,
    pub active_dialog: Option<ActiveDialog>,
    pub source_picker: SourcePickerState,
    pub status_message: Option<String>,
    pub auto_refresh: bool,
    pub auto_refresh_interval: Duration,
    pub last_refresh: Instant,
    pub spinner_frame: usize,
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub needs_refresh: bool,
    pub data_generation: u64,
    pub panel_loading: [bool; Panel::COUNT],
    pub model_collapse: Option<Collapsed>,
    pub cost_collapse: Option<Collapsed>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_panel: Panel::Overview,
            time_window: TimeWindow::All,
            scroll: std::array::from_fn(|_| ScrollState {
                offset: 0,
                total: 0,
                visible: 0,
            }),
            filter: QueryFilter::default(),
            overview: None,
            sync_center: None,
            trends: None,
            models: None,
            daily: None,
            hourly: None,
            sources: None,
            projects: None,
            costs: None,
            health: None,
            stats: None,
            behavior: None,
            blocks: None,
            platform_probes: crate::domain::platform_monitor::probe_registered_platforms(),
            active_dialog: None,
            source_picker: SourcePickerState { selected: 0 },
            status_message: None,
            auto_refresh: false,
            auto_refresh_interval: Duration::from_secs(30),
            last_refresh: Instant::now(),
            spinner_frame: 0,
            terminal_width: 80,
            terminal_height: 24,
            needs_refresh: false,
            data_generation: 0,
            panel_loading: [false; Panel::COUNT],
            model_collapse: None,
            cost_collapse: None,
        }
    }

    pub fn is_narrow(&self) -> bool {
        self.terminal_width < 80
    }

    pub fn is_very_narrow(&self) -> bool {
        self.terminal_width < 60
    }

    pub fn source_filter_label(&self) -> &'static str {
        self.filter.source.map(SourceKind::as_str).unwrap_or("all")
    }

    pub fn open_source_picker(&mut self) {
        self.source_picker.clamp(self.platform_probes.len());
        self.active_dialog = Some(ActiveDialog::SourcePicker);
    }

    pub fn open_help(&mut self) {
        self.active_dialog = Some(ActiveDialog::Help);
    }

    pub fn close_dialog(&mut self) {
        self.active_dialog = None;
    }

    pub fn source_picker_next(&mut self) {
        self.source_picker.move_by(1, self.platform_probes.len());
    }

    pub fn source_picker_prev(&mut self) {
        self.source_picker.move_by(-1, self.platform_probes.len());
    }

    pub fn select_source_picker_row(&mut self) {
        let Some(probe) = self.platform_probes.get(self.source_picker.selected) else {
            return;
        };

        match probe.source_kind {
            Some(source) => {
                if self.filter.source == Some(source) {
                    self.filter.source = None;
                    self.set_status("Source filter cleared");
                } else {
                    self.filter.source = Some(source);
                    self.set_status(&format!("Source filter: {}", source.as_str()));
                }
                self.invalidate_cached_data();
                self.active_dialog = None;
            }
            None => {
                self.set_status(&format!(
                    "{} is monitor-only: {}",
                    probe.display_name, probe.next_action
                ));
            }
        }
    }

    pub fn clear_source_filter(&mut self) {
        self.filter.source = None;
        self.invalidate_cached_data();
        self.active_dialog = None;
        self.set_status("Source filter cleared");
    }

    pub fn invalidate_cached_data(&mut self) {
        self.overview = None;
        self.sync_center = None;
        self.trends = None;
        self.models = None;
        self.daily = None;
        self.hourly = None;
        self.sources = None;
        self.projects = None;
        self.costs = None;
        self.health = None;
        self.stats = None;
        self.behavior = None;
        self.blocks = None;
        for scroll in &mut self.scroll {
            scroll.offset = 0;
        }
        self.panel_loading = [false; Panel::COUNT];
        self.model_collapse = None;
        self.cost_collapse = None;
        self.needs_refresh = true;
    }

    pub fn mark_refreshed(&mut self) {
        self.last_refresh = Instant::now();
        self.needs_refresh = false;
    }

    pub fn toggle_auto_refresh(&mut self) {
        self.auto_refresh = !self.auto_refresh;
        if self.auto_refresh {
            self.set_status("Auto refresh enabled");
        } else {
            self.set_status("Auto refresh disabled");
        }
    }

    pub fn on_tick(&mut self, animation_active: bool) -> bool {
        if animation_active {
            self.spinner_frame = (self.spinner_frame + 1) % 20;
        }
        let was_refresh_needed = self.needs_refresh;
        if self.auto_refresh && self.last_refresh.elapsed() >= self.auto_refresh_interval {
            self.needs_refresh = true;
        }
        animation_active || self.needs_refresh != was_refresh_needed
    }

    pub fn handle_resize(&mut self, width: u16, height: u16) {
        self.terminal_width = width;
        self.terminal_height = height;
    }

    pub fn set_status(&mut self, message: &str) {
        self.status_message = Some(message.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
    use proptest::prelude::*;

    /// Generate a valid panel index.
    fn arb_panel_index() -> impl Strategy<Value = usize> {
        0..Panel::COUNT
    }

    /// Generate a TimeWindow variant.
    fn arb_time_window() -> impl Strategy<Value = TimeWindow> {
        prop_oneof![
            Just(TimeWindow::Day24h),
            Just(TimeWindow::Week7d),
            Just(TimeWindow::Month30d),
            Just(TimeWindow::All),
        ]
    }

    /// Navigation action: digit key (1..=Panel::COUNT), Tab, or Shift+Tab.
    #[derive(Debug, Clone)]
    enum NavAction {
        Digit(usize), // 1..=Panel::COUNT (maps to panel 0..COUNT-1)
        Tab,
        ShiftTab,
    }

    fn arb_nav_action() -> impl Strategy<Value = NavAction> {
        prop_oneof![
            (1..=Panel::COUNT).prop_map(NavAction::Digit),
            Just(NavAction::Tab),
            Just(NavAction::ShiftTab),
        ]
    }

    #[test]
    fn time_windows_map_to_inclusive_local_calendar_dates() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 20).unwrap();
        let base = QueryFilter {
            source: Some(SourceKind::Codex),
            model: Some("gpt-5.6-sol".to_string()),
            since: Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2020, 1, 2).unwrap()),
            project_hash: Some("project".to_string()),
            timezone: crate::query::ReportTimezone::Fixed(
                FixedOffset::east_opt(8 * 3_600).unwrap(),
            ),
        };

        let today_filter = TimeWindow::Day24h.query_filter_on(&base, today);
        assert_eq!(today_filter.since, Some(today));
        assert_eq!(today_filter.until, Some(today));

        let week_filter = TimeWindow::Week7d.query_filter_on(&base, today);
        assert_eq!(
            week_filter.since,
            Some(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap())
        );
        assert_eq!(week_filter.until, Some(today));

        let month_filter = TimeWindow::Month30d.query_filter_on(&base, today);
        assert_eq!(
            month_filter.since,
            Some(NaiveDate::from_ymd_opt(2026, 6, 21).unwrap())
        );
        assert_eq!(month_filter.until, Some(today));

        let all_filter = TimeWindow::All.query_filter_on(&base, today);
        assert_eq!(all_filter.since, None);
        assert_eq!(all_filter.until, None);
        assert_eq!(all_filter.source, base.source);
        assert_eq!(all_filter.model, base.model);
        assert_eq!(all_filter.project_hash, base.project_hash);
        assert_eq!(all_filter.timezone, base.timezone);
    }

    #[test]
    fn app_state_defaults_to_all_time() {
        assert_eq!(AppState::new().time_window, TimeWindow::All);
    }

    #[test]
    fn idle_ticks_do_not_request_redraw_but_active_animation_does() {
        let mut state = AppState::new();
        assert!(!state.on_tick(false));
        assert!(state.on_tick(true));
    }

    // Feature: terminal-dashboard, Property 1: Panel navigation produces correct index
    // **Validates: Requirements 2.2, 2.3, 2.4**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_panel_navigation_produces_correct_index(
            current in arb_panel_index(),
            action in arb_nav_action(),
        ) {
            let panel = Panel::from_index(current).unwrap();
            let expected = match &action {
                NavAction::Digit(n) => *n - 1,
                NavAction::Tab => (current + 1) % Panel::COUNT,
                NavAction::ShiftTab => (current + Panel::COUNT - 1) % Panel::COUNT,
            };

            let result = match action {
                NavAction::Digit(n) => Panel::from_index(n - 1).unwrap(),
                NavAction::Tab => panel.next(),
                NavAction::ShiftTab => panel.prev(),
            };

            prop_assert_eq!(result as usize, expected);
        }
    }

    // Feature: terminal-dashboard, Property 3: Time window transitions follow clamped linear order
    // **Validates: Requirements 4.3, 4.4**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_time_window_next_follows_clamped_order(window in arb_time_window()) {
            let result = window.next();
            let expected = match window {
                TimeWindow::Day24h => TimeWindow::Week7d,
                TimeWindow::Week7d => TimeWindow::Month30d,
                TimeWindow::Month30d => TimeWindow::All,
                TimeWindow::All => TimeWindow::All,
            };
            prop_assert_eq!(result, expected);
        }

        #[test]
        fn prop_time_window_prev_follows_clamped_order(window in arb_time_window()) {
            let result = window.prev();
            let expected = match window {
                TimeWindow::Day24h => TimeWindow::Day24h,
                TimeWindow::Week7d => TimeWindow::Day24h,
                TimeWindow::Month30d => TimeWindow::Week7d,
                TimeWindow::All => TimeWindow::Month30d,
            };
            prop_assert_eq!(result, expected);
        }
    }

    // Feature: terminal-dashboard, Property 4: Scroll offset stays within valid bounds
    // Validates: Requirements 5.3, 7.3, 8.3
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn scroll_offset_stays_within_bounds(
            total in 0usize..1000,
            visible in 0usize..1000,
            ops in proptest::collection::vec(prop::bool::ANY, 0..200),
        ) {
            let mut state = ScrollState { offset: 0, total, visible };
            let max_offset = total.saturating_sub(visible);

            for go_down in ops {
                if go_down {
                    state.scroll_down();
                } else {
                    state.scroll_up();
                }
                prop_assert!(state.offset <= max_offset,
                    "offset {} exceeded max {} (total={}, visible={})",
                    state.offset, max_offset, total, visible);
            }
        }
    }
}
