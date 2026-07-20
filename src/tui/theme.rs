//! Centralized color palette and style helpers for the dashboard TUI.
//!
//! Colors come from a runtime-swappable [`Theme`]. `Theme::default_dark` keeps
//! the historical dark blue/cyan look (lazygit/btop-like) so existing views are
//! pixel-identical unless the user switches themes. All call sites read colors
//! through the accessor fns (`accent()`, `muted_fg()`, ...) or the style
//! constructors, both of which resolve against the process-wide active theme.

use std::{cell::Cell, sync::RwLock};

use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColorMode {
    TrueColor,
    Ansi16,
    NoColor,
}

impl TerminalColorMode {
    pub fn from_env() -> Self {
        Self::detect(
            std::env::var_os("NO_COLOR").is_some(),
            std::env::var("LLMUSAGE_NO_COLOR").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
            std::env::var("COLORTERM").ok().as_deref(),
        )
    }

    fn detect(
        no_color: bool,
        llmusage_no_color: Option<&str>,
        term: Option<&str>,
        colorterm: Option<&str>,
    ) -> Self {
        if no_color || llmusage_no_color.is_some_and(env_flag_enabled) {
            return Self::NoColor;
        }

        let supports_truecolor = [term, colorterm].into_iter().flatten().any(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("truecolor") || value.contains("24bit") || value.contains("direct")
        });
        if supports_truecolor || (term.is_none() && colorterm.is_none()) {
            Self::TrueColor
        } else {
            Self::Ansi16
        }
    }
}

fn env_flag_enabled(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && !value.eq_ignore_ascii_case("0")
        && !value.eq_ignore_ascii_case("false")
        && !value.eq_ignore_ascii_case("no")
        && !value.eq_ignore_ascii_case("off")
}

/// A named palette of semantic color slots. `Copy` so `active_theme()` can hand
/// out cheap snapshots without locking for the duration of a render.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    pub accent: Color,
    pub accent_dim: Color,
    pub header_fg: Color,
    pub border_active: Color,
    pub border_normal: Color,
    pub error_fg: Color,
    pub warning_fg: Color,
    pub positive_fg: Color,
    pub muted_fg: Color,
    pub surface_fg: Color,
    pub row_alt_bg: Color,
    pub selection_fg: Color,
    pub selection_bg: Color,
    pub metric_input: Color,
    pub metric_output: Color,
    pub metric_cache_read: Color,
    pub metric_cache_write: Color,
    pub metric_reasoning: Color,
    pub kpi_colors: [Color; 4],
    pub trend_bar_fg: Color,
    pub trend_peak_fg: Color,
    pub trend_aux_fg: Color,
    /// Contribution heatmap ramp: index 0 = no data, 1..=4 = light → dark.
    pub heat: [Color; 5],
    /// Progress/utilization bar ramp by severity.
    pub bar_ok: Color,
    pub bar_warn: Color,
    pub bar_danger: Color,
}

impl Theme {
    /// The historical dark theme. Values here MUST match the pre-theme consts
    /// so the default render is unchanged.
    pub const fn default_dark() -> Self {
        Self {
            name: "dark",
            accent: Color::Cyan,
            accent_dim: Color::DarkGray,
            header_fg: Color::Cyan,
            border_active: Color::Cyan,
            border_normal: Color::DarkGray,
            error_fg: Color::Red,
            warning_fg: Color::Yellow,
            positive_fg: Color::Green,
            muted_fg: Color::DarkGray,
            surface_fg: Color::White,
            row_alt_bg: Color::Rgb(30, 30, 40),
            selection_fg: Color::Black,
            selection_bg: Color::Cyan,
            metric_input: Color::Cyan,
            metric_output: Color::Green,
            metric_cache_read: Color::Blue,
            metric_cache_write: Color::Magenta,
            metric_reasoning: Color::Yellow,
            kpi_colors: [Color::Cyan, Color::Green, Color::Yellow, Color::Magenta],
            trend_bar_fg: Color::Blue,
            trend_peak_fg: Color::Yellow,
            trend_aux_fg: Color::DarkGray,
            heat: [
                Color::DarkGray,
                Color::Rgb(14, 68, 41),
                Color::Rgb(0, 109, 50),
                Color::Rgb(38, 166, 65),
                Color::Rgb(57, 211, 83),
            ],
            bar_ok: Color::Green,
            bar_warn: Color::Yellow,
            bar_danger: Color::Red,
        }
    }

    /// Catppuccin Mocha — a warmer, higher-contrast dark palette (truecolor).
    pub const fn catppuccin_mocha() -> Self {
        Self {
            name: "mocha",
            accent: Color::Rgb(137, 180, 250),             // blue
            accent_dim: Color::Rgb(88, 91, 112),           // surface2
            header_fg: Color::Rgb(203, 166, 247),          // mauve
            border_active: Color::Rgb(137, 180, 250),      // blue
            border_normal: Color::Rgb(69, 71, 90),         // surface1
            error_fg: Color::Rgb(243, 139, 168),           // red
            warning_fg: Color::Rgb(249, 226, 175),         // yellow
            positive_fg: Color::Rgb(166, 227, 161),        // green
            muted_fg: Color::Rgb(127, 132, 156),           // overlay1
            surface_fg: Color::Rgb(205, 214, 244),         // text
            row_alt_bg: Color::Rgb(41, 44, 60),            // surface0-ish
            selection_fg: Color::Rgb(30, 30, 46),          // crust
            selection_bg: Color::Rgb(137, 180, 250),       // blue
            metric_input: Color::Rgb(137, 180, 250),       // blue
            metric_output: Color::Rgb(166, 227, 161),      // green
            metric_cache_read: Color::Rgb(137, 180, 250),  // blue
            metric_cache_write: Color::Rgb(203, 166, 247), // mauve
            metric_reasoning: Color::Rgb(249, 226, 175),   // yellow
            kpi_colors: [
                Color::Rgb(137, 180, 250), // blue
                Color::Rgb(166, 227, 161), // green
                Color::Rgb(249, 226, 175), // yellow
                Color::Rgb(203, 166, 247), // mauve
            ],
            trend_bar_fg: Color::Rgb(137, 180, 250),
            trend_peak_fg: Color::Rgb(249, 226, 175),
            trend_aux_fg: Color::Rgb(108, 112, 134), // overlay0
            heat: [
                Color::Rgb(49, 50, 68), // surface0
                Color::Rgb(64, 106, 96),
                Color::Rgb(94, 156, 120),
                Color::Rgb(134, 199, 140),
                Color::Rgb(166, 227, 161),
            ],
            bar_ok: Color::Rgb(166, 227, 161),
            bar_warn: Color::Rgb(249, 226, 175),
            bar_danger: Color::Rgb(243, 139, 168),
        }
    }

    pub const fn graphite() -> Self {
        Self {
            name: "graphite",
            accent: Color::Rgb(232, 184, 87),
            accent_dim: Color::Rgb(105, 110, 118),
            header_fg: Color::Rgb(232, 184, 87),
            border_active: Color::Rgb(232, 184, 87),
            border_normal: Color::Rgb(82, 87, 94),
            error_fg: Color::Rgb(238, 111, 111),
            warning_fg: Color::Rgb(232, 184, 87),
            positive_fg: Color::Rgb(112, 194, 159),
            muted_fg: Color::Rgb(132, 138, 147),
            surface_fg: Color::Rgb(225, 228, 232),
            row_alt_bg: Color::Rgb(38, 41, 45),
            selection_fg: Color::Rgb(28, 30, 33),
            selection_bg: Color::Rgb(232, 184, 87),
            metric_input: Color::Rgb(112, 194, 159),
            metric_output: Color::Rgb(238, 111, 111),
            metric_cache_read: Color::Rgb(105, 169, 231),
            metric_cache_write: Color::Rgb(232, 184, 87),
            metric_reasoning: Color::Rgb(193, 145, 214),
            kpi_colors: [
                Color::Rgb(232, 184, 87),
                Color::Rgb(112, 194, 159),
                Color::Rgb(105, 169, 231),
                Color::Rgb(193, 145, 214),
            ],
            trend_bar_fg: Color::Rgb(105, 169, 231),
            trend_peak_fg: Color::Rgb(232, 184, 87),
            trend_aux_fg: Color::Rgb(105, 110, 118),
            heat: [
                Color::Rgb(52, 56, 61),
                Color::Rgb(59, 91, 79),
                Color::Rgb(73, 126, 103),
                Color::Rgb(92, 162, 132),
                Color::Rgb(112, 194, 159),
            ],
            bar_ok: Color::Rgb(112, 194, 159),
            bar_warn: Color::Rgb(232, 184, 87),
            bar_danger: Color::Rgb(238, 111, 111),
        }
    }

    pub const fn lagoon() -> Self {
        Self {
            name: "lagoon",
            accent: Color::Rgb(65, 195, 190),
            accent_dim: Color::Rgb(73, 104, 117),
            header_fg: Color::Rgb(105, 213, 208),
            border_active: Color::Rgb(65, 195, 190),
            border_normal: Color::Rgb(49, 78, 91),
            error_fg: Color::Rgb(255, 116, 126),
            warning_fg: Color::Rgb(244, 197, 96),
            positive_fg: Color::Rgb(111, 207, 151),
            muted_fg: Color::Rgb(119, 151, 161),
            surface_fg: Color::Rgb(222, 238, 239),
            row_alt_bg: Color::Rgb(20, 49, 61),
            selection_fg: Color::Rgb(9, 36, 48),
            selection_bg: Color::Rgb(65, 195, 190),
            metric_input: Color::Rgb(111, 207, 151),
            metric_output: Color::Rgb(255, 116, 126),
            metric_cache_read: Color::Rgb(92, 164, 232),
            metric_cache_write: Color::Rgb(244, 166, 96),
            metric_reasoning: Color::Rgb(210, 145, 230),
            kpi_colors: [
                Color::Rgb(65, 195, 190),
                Color::Rgb(111, 207, 151),
                Color::Rgb(244, 197, 96),
                Color::Rgb(255, 116, 126),
            ],
            trend_bar_fg: Color::Rgb(65, 195, 190),
            trend_peak_fg: Color::Rgb(244, 197, 96),
            trend_aux_fg: Color::Rgb(73, 104, 117),
            heat: [
                Color::Rgb(24, 57, 69),
                Color::Rgb(31, 91, 91),
                Color::Rgb(39, 126, 119),
                Color::Rgb(51, 161, 151),
                Color::Rgb(65, 195, 190),
            ],
            bar_ok: Color::Rgb(111, 207, 151),
            bar_warn: Color::Rgb(244, 197, 96),
            bar_danger: Color::Rgb(255, 116, 126),
        }
    }

    fn adapted(self, mode: TerminalColorMode) -> Self {
        let adapt = |color| adapt_color(color, mode);
        Self {
            name: self.name,
            accent: adapt(self.accent),
            accent_dim: adapt(self.accent_dim),
            header_fg: adapt(self.header_fg),
            border_active: adapt(self.border_active),
            border_normal: adapt(self.border_normal),
            error_fg: adapt(self.error_fg),
            warning_fg: adapt(self.warning_fg),
            positive_fg: adapt(self.positive_fg),
            muted_fg: adapt(self.muted_fg),
            surface_fg: adapt(self.surface_fg),
            row_alt_bg: adapt(self.row_alt_bg),
            selection_fg: adapt(self.selection_fg),
            selection_bg: adapt(self.selection_bg),
            metric_input: adapt(self.metric_input),
            metric_output: adapt(self.metric_output),
            metric_cache_read: adapt(self.metric_cache_read),
            metric_cache_write: adapt(self.metric_cache_write),
            metric_reasoning: adapt(self.metric_reasoning),
            kpi_colors: self.kpi_colors.map(adapt),
            trend_bar_fg: adapt(self.trend_bar_fg),
            trend_peak_fg: adapt(self.trend_peak_fg),
            trend_aux_fg: adapt(self.trend_aux_fg),
            heat: self.heat.map(adapt),
            bar_ok: adapt(self.bar_ok),
            bar_warn: adapt(self.bar_warn),
            bar_danger: adapt(self.bar_danger),
        }
    }

    /// All selectable themes in cycle order.
    pub const ALL: [Theme; 4] = [
        Theme::default_dark(),
        Theme::catppuccin_mocha(),
        Theme::graphite(),
        Theme::lagoon(),
    ];

    /// Looks up a theme by its `name`, case-insensitively.
    pub fn by_name(name: &str) -> Option<Theme> {
        Theme::ALL
            .into_iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::default_dark()
    }
}

// ── Active theme (process-wide) ───────────────────────────────────────────────

#[derive(Clone, Copy)]
struct ThemeState {
    theme: Theme,
    mode: TerminalColorMode,
}

fn active_lock() -> &'static RwLock<ThemeState> {
    static ACTIVE: RwLock<ThemeState> = RwLock::new(ThemeState {
        theme: Theme::default_dark(),
        mode: TerminalColorMode::TrueColor,
    });
    &ACTIVE
}

thread_local! {
    static RENDER_SNAPSHOT: Cell<Option<ThemeState>> = const { Cell::new(None) };
}

struct RenderSnapshotGuard(Option<ThemeState>);

impl Drop for RenderSnapshotGuard {
    fn drop(&mut self) {
        RENDER_SNAPSHOT.with(|slot| slot.set(self.0));
    }
}

/// Runs a render using one theme read snapshot instead of taking the global
/// lock for every semantic color accessor.
pub fn with_render_snapshot<R>(render: impl FnOnce() -> R) -> R {
    let snapshot = *active_lock().read().expect("theme lock poisoned");
    let previous = RENDER_SNAPSHOT.with(|slot| {
        let previous = slot.get();
        slot.set(Some(snapshot));
        previous
    });
    let _guard = RenderSnapshotGuard(previous);
    render()
}

/// Returns a snapshot of the current theme.
pub fn active_theme() -> Theme {
    RENDER_SNAPSHOT
        .with(|slot| slot.get())
        .map(|state| state.theme)
        .unwrap_or_else(|| active_lock().read().expect("theme lock poisoned").theme)
}

pub fn color_mode() -> TerminalColorMode {
    RENDER_SNAPSHOT
        .with(|slot| slot.get())
        .map(|state| state.mode)
        .unwrap_or_else(|| active_lock().read().expect("theme lock poisoned").mode)
}

/// Replaces the active theme; subsequent renders use the new palette.
pub fn set_theme(theme: Theme) {
    let mut state = active_lock().write().expect("theme lock poisoned");
    state.theme = theme.adapted(state.mode);
}

pub fn set_color_mode(mode: TerminalColorMode) {
    let mut state = active_lock().write().expect("theme lock poisoned");
    let base = Theme::by_name(state.theme.name).unwrap_or_else(Theme::default_dark);
    state.mode = mode;
    state.theme = base.adapted(mode);
}

pub fn configure_from_env() {
    let mode = TerminalColorMode::from_env();
    let theme = std::env::var("LLMUSAGE_THEME")
        .ok()
        .and_then(|name| Theme::by_name(&name))
        .unwrap_or_else(Theme::default_dark);
    let mut state = active_lock().write().expect("theme lock poisoned");
    state.mode = mode;
    state.theme = theme.adapted(mode);
}

/// Switches to the named theme if it exists, returning the theme's name on
/// success. Unknown names leave the active theme unchanged.
pub fn set_theme_by_name(name: &str) -> Option<&'static str> {
    let theme = Theme::by_name(name)?;
    set_theme(theme);
    Some(theme.name)
}

/// Advances to the next theme in [`Theme::ALL`], wrapping around, and returns
/// the newly active theme's name.
pub fn cycle_theme() -> &'static str {
    let current = active_theme().name;
    let index = Theme::ALL
        .iter()
        .position(|theme| theme.name == current)
        .unwrap_or(0);
    let next = Theme::ALL[(index + 1) % Theme::ALL.len()];
    set_theme(next);
    next.name
}

// ── Color accessors ───────────────────────────────────────────────────────────

/// Accent color for active/highlighted elements.
pub fn accent() -> Color {
    active_theme().accent
}

/// Secondary accent for less prominent highlights.
pub fn accent_dim() -> Color {
    active_theme().accent_dim
}

/// Color for header text in tables.
pub fn header_fg() -> Color {
    active_theme().header_fg
}

/// Color for borders of the active/focused block.
pub fn border_active() -> Color {
    active_theme().border_active
}

/// Color for borders of inactive blocks.
pub fn border_normal() -> Color {
    active_theme().border_normal
}

/// Color for error messages.
pub fn error_fg() -> Color {
    active_theme().error_fg
}

/// Color for warning and degraded values.
pub fn warning_fg() -> Color {
    active_theme().warning_fg
}

/// Color for success/positive values.
pub fn positive_fg() -> Color {
    active_theme().positive_fg
}

/// Color for muted/secondary text.
pub fn muted_fg() -> Color {
    active_theme().muted_fg
}

/// Primary foreground for unselected terminal surfaces.
pub fn surface_fg() -> Color {
    active_theme().surface_fg
}

/// Alternating row background (subtle).
pub fn row_alt_bg() -> Color {
    active_theme().row_alt_bg
}

/// KPI card colors (one per card).
pub fn kpi_colors() -> [Color; 4] {
    active_theme().kpi_colors
}

pub fn metric_input() -> Color {
    active_theme().metric_input
}

pub fn metric_output() -> Color {
    active_theme().metric_output
}

pub fn metric_cache_read() -> Color {
    active_theme().metric_cache_read
}

pub fn metric_cache_write() -> Color {
    active_theme().metric_cache_write
}

pub fn metric_reasoning() -> Color {
    active_theme().metric_reasoning
}

/// Primary bar color for the trends cockpit.
pub fn trend_bar_fg() -> Color {
    active_theme().trend_bar_fg
}

/// Peak/high-water mark color for the trends cockpit.
pub fn trend_peak_fg() -> Color {
    active_theme().trend_peak_fg
}

/// Axis and helper line color for trend charts.
pub fn trend_aux_fg() -> Color {
    active_theme().trend_aux_fg
}

/// Contribution heatmap ramp; `bucket` is clamped to `0..=4`.
pub fn heat(bucket: usize) -> Color {
    let ramp = active_theme().heat;
    ramp[bucket.min(ramp.len() - 1)]
}

/// Utilization/progress bar color by percentage: <50 ok, 50–80 warn, >80 danger.
pub fn bar_color(percent: f64) -> Color {
    let theme = active_theme();
    if percent >= 80.0 {
        theme.bar_danger
    } else if percent >= 50.0 {
        theme.bar_warn
    } else {
        theme.bar_ok
    }
}

pub fn fg_style(color: Color) -> Style {
    if color_mode() == TerminalColorMode::NoColor {
        Style::default()
    } else {
        Style::default().fg(color)
    }
}

pub fn bold_style() -> Style {
    if color_mode() == TerminalColorMode::NoColor {
        Style::default()
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    }
}

pub fn bold_fg_style(color: Color) -> Style {
    if color_mode() == TerminalColorMode::NoColor {
        Style::default()
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }
}

// ── Style constructors ───────────────────────────────────────────────────────

/// Style for table header row.
pub fn header_style() -> Style {
    bold_fg_style(header_fg())
}

/// Style for normal data rows.
pub fn row_style() -> Style {
    Style::default()
}

/// Style for alternating (even-index) data rows.
pub fn row_alt_style() -> Style {
    if color_mode() == TerminalColorMode::NoColor {
        Style::default()
    } else {
        Style::default().bg(row_alt_bg())
    }
}

/// Style for the active nav tab.
pub fn nav_active_style() -> Style {
    selection_style()
}

/// Style for inactive nav tabs.
pub fn nav_inactive_style() -> Style {
    fg_style(surface_fg())
}

/// Style for selected rows and picker entries.
pub fn selection_style() -> Style {
    if color_mode() == TerminalColorMode::NoColor {
        return Style::default();
    }
    let theme = active_theme();
    Style::default()
        .fg(theme.selection_fg)
        .bg(theme.selection_bg)
        .add_modifier(Modifier::BOLD)
}

/// Style for block borders (active panel).
pub fn block_border_style() -> Style {
    fg_style(border_active())
}

/// Style for block title text.
pub fn block_title_style() -> Style {
    bold_fg_style(accent())
}

/// Style for error text.
pub fn error_style() -> Style {
    fg_style(error_fg())
}

/// Style for muted/placeholder text.
pub fn muted_style() -> Style {
    fg_style(muted_fg())
}

/// Shared bordered block for dashboard panels.
pub fn panel_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(block_border_style())
        .title(Span::styled(format!(" {title} "), block_title_style()))
}

/// Shared mini-card block for trend summary metrics.
pub fn trend_card_block(title: &str, color: Color) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(fg_style(color))
        .title(Span::styled(format!(" {title} "), bold_fg_style(color)))
}

/// Style for ordinary trend bars.
pub fn trend_bar_style() -> Style {
    fg_style(trend_bar_fg())
}

/// Style for the peak trend bar and peak value.
pub fn trend_peak_style() -> Style {
    bold_fg_style(trend_peak_fg())
}

/// Style for trend axes, labels, and secondary hints.
pub fn trend_aux_style() -> Style {
    fg_style(trend_aux_fg())
}

fn adapt_color(color: Color, mode: TerminalColorMode) -> Color {
    match mode {
        TerminalColorMode::TrueColor => color,
        TerminalColorMode::NoColor => Color::Reset,
        TerminalColorMode::Ansi16 => match color {
            Color::Rgb(red, green, blue) => nearest_ansi16(red, green, blue),
            other => other,
        },
    }
}

fn nearest_ansi16(red: u8, green: u8, blue: u8) -> Color {
    const PALETTE: [(Color, (u8, u8, u8)); 16] = [
        (Color::Black, (0, 0, 0)),
        (Color::Red, (128, 0, 0)),
        (Color::Green, (0, 128, 0)),
        (Color::Yellow, (128, 128, 0)),
        (Color::Blue, (0, 0, 128)),
        (Color::Magenta, (128, 0, 128)),
        (Color::Cyan, (0, 128, 128)),
        (Color::Gray, (192, 192, 192)),
        (Color::DarkGray, (128, 128, 128)),
        (Color::LightRed, (255, 0, 0)),
        (Color::LightGreen, (0, 255, 0)),
        (Color::LightYellow, (255, 255, 0)),
        (Color::LightBlue, (0, 0, 255)),
        (Color::LightMagenta, (255, 0, 255)),
        (Color::LightCyan, (0, 255, 255)),
        (Color::White, (255, 255, 255)),
    ];

    PALETTE
        .into_iter()
        .min_by_key(|(_, (candidate_red, candidate_green, candidate_blue))| {
            let red = i32::from(red) - i32::from(*candidate_red);
            let green = i32::from(green) - i32::from(*candidate_green);
            let blue = i32::from(blue) - i32::from(*candidate_blue);
            red * red + green * green + blue * blue
        })
        .map(|(color, _)| color)
        .unwrap_or(Color::White)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_matches_historical_palette() {
        let theme = Theme::default_dark();
        assert_eq!(theme.accent, Color::Cyan);
        assert_eq!(theme.muted_fg, Color::DarkGray);
        assert_eq!(theme.row_alt_bg, Color::Rgb(30, 30, 40));
        assert_eq!(theme.selection_fg, Color::Black);
        assert_eq!(theme.selection_bg, Color::Cyan);
        assert_eq!(theme.metric_input, Color::Cyan);
        assert_eq!(theme.metric_output, Color::Green);
        assert_eq!(theme.metric_cache_read, Color::Blue);
        assert_eq!(theme.metric_cache_write, Color::Magenta);
        assert_eq!(theme.metric_reasoning, Color::Yellow);
        assert_eq!(
            theme.kpi_colors,
            [Color::Cyan, Color::Green, Color::Yellow, Color::Magenta]
        );
    }

    #[test]
    fn cycle_theme_wraps_and_set_by_name_round_trips() {
        set_color_mode(TerminalColorMode::TrueColor);
        set_theme(Theme::default_dark());
        assert_eq!(active_theme().name, "dark");
        assert_eq!(cycle_theme(), "mocha");
        assert_eq!(active_theme().accent, Color::Rgb(137, 180, 250));
        assert_eq!(cycle_theme(), "graphite");
        assert_eq!(cycle_theme(), "lagoon");
        assert_eq!(cycle_theme(), "dark");
        assert_eq!(set_theme_by_name("mocha"), Some("mocha"));
        assert_eq!(set_theme_by_name("nope"), None);
        assert_eq!(active_theme().name, "mocha");
        // Restore default so other tests see the historical palette.
        set_theme(Theme::default_dark());
    }

    #[test]
    fn bar_color_thresholds() {
        set_color_mode(TerminalColorMode::TrueColor);
        set_theme(Theme::default_dark());
        assert_eq!(bar_color(10.0), Color::Green);
        assert_eq!(bar_color(60.0), Color::Yellow);
        assert_eq!(bar_color(95.0), Color::Red);
    }

    #[test]
    fn tui_panels_do_not_bypass_semantic_theme_slots() {
        let sources = [
            ("behavior", include_str!("panels/behavior.rs")),
            ("blocks", include_str!("panels/blocks.rs")),
            ("cost", include_str!("panels/cost.rs")),
            ("daily", include_str!("panels/daily.rs")),
            ("health", include_str!("panels/health.rs")),
            ("hourly", include_str!("panels/hourly.rs")),
            ("models", include_str!("panels/models.rs")),
            ("overview", include_str!("panels/overview.rs")),
            ("projects", include_str!("panels/projects.rs")),
            ("sources", include_str!("panels/sources.rs")),
            ("stats", include_str!("panels/stats.rs")),
            ("trends", include_str!("panels/trends.rs")),
            ("usage", include_str!("panels/usage.rs")),
            ("source_picker", include_str!("source_picker.rs")),
        ];

        for (name, source) in sources {
            assert!(
                !source.contains("Color::"),
                "{name} must use semantic theme slots instead of Color::*"
            );
        }
    }

    #[test]
    fn interactive_tui_copy_stays_english() {
        let sources = [
            include_str!("mod.rs"),
            include_str!("footer.rs"),
            include_str!("help_dialog.rs"),
            include_str!("nav_bar.rs"),
            include_str!("source_picker.rs"),
            include_str!("panels/behavior.rs"),
            include_str!("panels/blocks.rs"),
            include_str!("panels/cost.rs"),
            include_str!("panels/daily.rs"),
            include_str!("panels/health.rs"),
            include_str!("panels/hourly.rs"),
            include_str!("panels/models.rs"),
            include_str!("panels/overview.rs"),
            include_str!("panels/projects.rs"),
            include_str!("panels/sources.rs"),
            include_str!("panels/stats.rs"),
            include_str!("panels/trends.rs"),
            include_str!("panels/usage.rs"),
            include_str!("../../tests/tui_panels_prop.rs"),
        ];

        assert!(sources.iter().all(|source| !source.chars().any(is_han)));
    }

    fn is_han(ch: char) -> bool {
        matches!(
            ch,
            '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}'
        )
    }

    #[test]
    fn detects_no_color_truecolor_and_limited_terminals() {
        assert_eq!(
            TerminalColorMode::detect(true, None, Some("xterm-256color"), None),
            TerminalColorMode::NoColor
        );
        assert_eq!(
            TerminalColorMode::detect(false, Some("true"), None, None),
            TerminalColorMode::NoColor
        );
        assert_eq!(
            TerminalColorMode::detect(false, Some("0"), None, None),
            TerminalColorMode::TrueColor
        );
        assert_eq!(
            TerminalColorMode::detect(false, None, Some("xterm-256color"), Some("truecolor")),
            TerminalColorMode::TrueColor
        );
        assert_eq!(
            TerminalColorMode::detect(false, None, Some("xterm-256color"), None),
            TerminalColorMode::Ansi16
        );
    }

    #[test]
    fn new_themes_and_color_adaptation_cover_all_slots() {
        assert_eq!(
            Theme::ALL.map(|theme| theme.name),
            ["dark", "mocha", "graphite", "lagoon"]
        );

        let ansi = Theme::lagoon().adapted(TerminalColorMode::Ansi16);
        let ansi_colors = [
            ansi.accent,
            ansi.row_alt_bg,
            ansi.metric_input,
            ansi.metric_output,
            ansi.metric_cache_read,
            ansi.metric_cache_write,
            ansi.metric_reasoning,
            ansi.heat[4],
        ];
        assert!(
            ansi_colors
                .iter()
                .all(|color| !matches!(color, Color::Rgb(..)))
        );

        let plain = Theme::graphite().adapted(TerminalColorMode::NoColor);
        let plain_colors = [
            plain.accent,
            plain.surface_fg,
            plain.selection_bg,
            plain.metric_input,
            plain.kpi_colors[3],
            plain.heat[4],
            plain.bar_danger,
        ];
        assert!(plain_colors.iter().all(|color| *color == Color::Reset));
    }

    #[test]
    fn truecolor_style_helpers_preserve_stage_one_styles() {
        set_color_mode(TerminalColorMode::TrueColor);
        set_theme(Theme::default_dark());

        assert_eq!(fg_style(Color::Cyan), Style::default().fg(Color::Cyan));
        assert_eq!(bold_style(), Style::default().add_modifier(Modifier::BOLD));
        assert_eq!(
            bold_fg_style(Color::Green),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        );
        assert_eq!(
            selection_style(),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        );
    }

    #[test]
    fn render_snapshot_is_stable_until_the_frame_finishes() {
        set_color_mode(TerminalColorMode::TrueColor);
        set_theme(Theme::default_dark());
        let before = active_theme();
        with_render_snapshot(|| {
            assert_eq!(active_theme().name, before.name);
            set_theme(Theme::catppuccin_mocha());
            assert_eq!(active_theme().accent, before.accent);
        });
        assert_eq!(active_theme().name, "mocha");
        set_theme(Theme::default_dark());
    }
}
