//! Centralized color palette and style helpers for the dashboard TUI.
//!
//! Design: dark background with blue/cyan accent tones (similar to lazygit/btop).

use ratatui::style::{Color, Modifier, Style};

// ── Palette ──────────────────────────────────────────────────────────────────

/// Accent color for active/highlighted elements.
pub const ACCENT: Color = Color::Cyan;

/// Secondary accent for less prominent highlights.
pub const ACCENT_DIM: Color = Color::DarkGray;

/// Color for header text in tables.
pub const HEADER_FG: Color = Color::Cyan;

/// Color for borders of the active/focused block.
pub const BORDER_ACTIVE: Color = Color::Cyan;

/// Color for borders of inactive blocks.
pub const BORDER_NORMAL: Color = Color::DarkGray;

/// Color for error messages.
pub const ERROR_FG: Color = Color::Red;

/// Color for success/positive values.
pub const POSITIVE_FG: Color = Color::Green;

/// Color for muted/secondary text.
pub const MUTED_FG: Color = Color::DarkGray;

/// Alternating row background (subtle).
pub const ROW_ALT_BG: Color = Color::Rgb(30, 30, 40);

/// KPI card colors (one per card).
pub const KPI_COLORS: [Color; 4] = [Color::Cyan, Color::Green, Color::Yellow, Color::Magenta];

// ── Style constructors ───────────────────────────────────────────────────────

/// Style for table header row.
pub fn header_style() -> Style {
    Style::default().fg(HEADER_FG).add_modifier(Modifier::BOLD)
}

/// Style for normal data rows.
pub fn row_style() -> Style {
    Style::default()
}

/// Style for alternating (even-index) data rows.
pub fn row_alt_style() -> Style {
    Style::default().bg(ROW_ALT_BG)
}

/// Style for the active nav tab.
pub fn nav_active_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(ACCENT)
        .add_modifier(Modifier::BOLD)
}

/// Style for inactive nav tabs.
pub fn nav_inactive_style() -> Style {
    Style::default().fg(Color::White)
}

/// Style for block borders (active panel).
pub fn block_border_style() -> Style {
    Style::default().fg(BORDER_ACTIVE)
}

/// Style for block title text.
pub fn block_title_style() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

/// Style for error text.
pub fn error_style() -> Style {
    Style::default().fg(ERROR_FG)
}

/// Style for muted/placeholder text.
pub fn muted_style() -> Style {
    Style::default().fg(MUTED_FG)
}
