use crate::query::{
    CostLine, HealthPayload, ModelBreakdown, OverviewPayload, ProjectBreakdown, QueryFilter,
    SourceBreakdown, TrendPoint,
};

/// The seven dashboard panels in fixed display order.
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
}

impl Panel {
    pub const COUNT: usize = 7;

    pub fn from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Self::Overview),
            1 => Some(Self::Trends),
            2 => Some(Self::Models),
            3 => Some(Self::Sources),
            4 => Some(Self::Projects),
            5 => Some(Self::Cost),
            6 => Some(Self::Health),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        Self::from_index((self as usize + 1) % Self::COUNT).unwrap()
    }

    pub fn prev(self) -> Self {
        Self::from_index((self as usize + Self::COUNT - 1) % Self::COUNT).unwrap()
    }
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
            Self::Day24h => "24h",
            Self::Week7d => "7d",
            Self::Month30d => "30d",
            Self::All => "全部",
        }
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
        if self.offset < self.total.saturating_sub(self.visible) {
            self.offset += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        self.offset = self.offset.saturating_sub(1);
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
    pub trends: Option<Result<Vec<TrendPoint>, String>>,
    pub models: Option<Result<Vec<ModelBreakdown>, String>>,
    pub sources: Option<Result<Vec<SourceBreakdown>, String>>,
    pub projects: Option<Result<Vec<ProjectBreakdown>, String>>,
    pub costs: Option<Result<Vec<CostLine>, String>>,
    pub health: Option<Result<HealthPayload, String>>,
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
            time_window: TimeWindow::Week7d,
            scroll: std::array::from_fn(|_| ScrollState {
                offset: 0,
                total: 0,
                visible: 0,
            }),
            filter: QueryFilter::default(),
            overview: None,
            trends: None,
            models: None,
            sources: None,
            projects: None,
            costs: None,
            health: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Generate a valid panel index (0..7).
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

    /// Navigation action: digit key (1-7), Tab, or Shift+Tab.
    #[derive(Debug, Clone)]
    enum NavAction {
        Digit(usize), // 1-7 (maps to panel 0-6)
        Tab,
        ShiftTab,
    }

    fn arb_nav_action() -> impl Strategy<Value = NavAction> {
        prop_oneof![
            (1..=7usize).prop_map(NavAction::Digit),
            Just(NavAction::Tab),
            Just(NavAction::ShiftTab),
        ]
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
                NavAction::Tab => (current + 1) % 7,
                NavAction::ShiftTab => (current + 6) % 7,
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
