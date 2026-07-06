//! Centralized color palette and style helpers for the dashboard TUI.
//!
//! Colors come from a runtime-swappable [`Theme`]. `Theme::default_dark` keeps
//! the historical dark blue/cyan look (lazygit/btop-like) so existing views are
//! pixel-identical unless the user switches themes. All call sites read colors
//! through the accessor fns (`accent()`, `muted_fg()`, ...) or the style
//! constructors, both of which resolve against the process-wide active theme.

use std::sync::RwLock;

use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders},
};

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
    pub positive_fg: Color,
    pub muted_fg: Color,
    pub row_alt_bg: Color,
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
            positive_fg: Color::Green,
            muted_fg: Color::DarkGray,
            row_alt_bg: Color::Rgb(30, 30, 40),
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
            accent: Color::Rgb(137, 180, 250),        // blue
            accent_dim: Color::Rgb(88, 91, 112),      // surface2
            header_fg: Color::Rgb(203, 166, 247),     // mauve
            border_active: Color::Rgb(137, 180, 250), // blue
            border_normal: Color::Rgb(69, 71, 90),    // surface1
            error_fg: Color::Rgb(243, 139, 168),      // red
            positive_fg: Color::Rgb(166, 227, 161),   // green
            muted_fg: Color::Rgb(127, 132, 156),      // overlay1
            row_alt_bg: Color::Rgb(41, 44, 60),       // surface0-ish
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

    /// All selectable themes in cycle order.
    pub const ALL: [Theme; 2] = [Theme::default_dark(), Theme::catppuccin_mocha()];

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

fn active_lock() -> &'static RwLock<Theme> {
    static ACTIVE: RwLock<Theme> = RwLock::new(Theme::default_dark());
    &ACTIVE
}

/// Returns a snapshot of the current theme.
pub fn active_theme() -> Theme {
    *active_lock().read().expect("theme lock poisoned")
}

/// Replaces the active theme; subsequent renders use the new palette.
pub fn set_theme(theme: Theme) {
    *active_lock().write().expect("theme lock poisoned") = theme;
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

/// Color for success/positive values.
pub fn positive_fg() -> Color {
    active_theme().positive_fg
}

/// Color for muted/secondary text.
pub fn muted_fg() -> Color {
    active_theme().muted_fg
}

/// Alternating row background (subtle).
pub fn row_alt_bg() -> Color {
    active_theme().row_alt_bg
}

/// KPI card colors (one per card).
pub fn kpi_colors() -> [Color; 4] {
    active_theme().kpi_colors
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

// ── Style constructors ───────────────────────────────────────────────────────

/// Style for table header row.
pub fn header_style() -> Style {
    Style::default()
        .fg(header_fg())
        .add_modifier(Modifier::BOLD)
}

/// Style for normal data rows.
pub fn row_style() -> Style {
    Style::default()
}

/// Style for alternating (even-index) data rows.
pub fn row_alt_style() -> Style {
    Style::default().bg(row_alt_bg())
}

/// Style for the active nav tab.
pub fn nav_active_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(accent())
        .add_modifier(Modifier::BOLD)
}

/// Style for inactive nav tabs.
pub fn nav_inactive_style() -> Style {
    Style::default().fg(Color::White)
}

/// Style for block borders (active panel).
pub fn block_border_style() -> Style {
    Style::default().fg(border_active())
}

/// Style for block title text.
pub fn block_title_style() -> Style {
    Style::default().fg(accent()).add_modifier(Modifier::BOLD)
}

/// Style for error text.
pub fn error_style() -> Style {
    Style::default().fg(error_fg())
}

/// Style for muted/placeholder text.
pub fn muted_style() -> Style {
    Style::default().fg(muted_fg())
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
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
}

/// Style for ordinary trend bars.
pub fn trend_bar_style() -> Style {
    Style::default().fg(trend_bar_fg())
}

/// Style for the peak trend bar and peak value.
pub fn trend_peak_style() -> Style {
    Style::default()
        .fg(trend_peak_fg())
        .add_modifier(Modifier::BOLD)
}

/// Style for trend axes, labels, and secondary hints.
pub fn trend_aux_style() -> Style {
    Style::default().fg(trend_aux_fg())
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
        assert_eq!(
            theme.kpi_colors,
            [Color::Cyan, Color::Green, Color::Yellow, Color::Magenta]
        );
    }

    #[test]
    fn cycle_theme_wraps_and_set_by_name_round_trips() {
        set_theme(Theme::default_dark());
        assert_eq!(active_theme().name, "dark");
        assert_eq!(cycle_theme(), "mocha");
        assert_eq!(active_theme().accent, Color::Rgb(137, 180, 250));
        assert_eq!(cycle_theme(), "dark");
        assert_eq!(set_theme_by_name("mocha"), Some("mocha"));
        assert_eq!(set_theme_by_name("nope"), None);
        assert_eq!(active_theme().name, "mocha");
        // Restore default so other tests see the historical palette.
        set_theme(Theme::default_dark());
    }

    #[test]
    fn bar_color_thresholds() {
        set_theme(Theme::default_dark());
        assert_eq!(bar_color(10.0), Color::Green);
        assert_eq!(bar_color(60.0), Color::Yellow);
        assert_eq!(bar_color(95.0), Color::Red);
    }
}
