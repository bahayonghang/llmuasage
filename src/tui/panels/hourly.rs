use chrono::{DateTime, NaiveDateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Span,
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::query::TrendPoint;
use crate::tui::{format::tokens as format_tokens, theme};

use super::super::app::ScrollState;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<TrendPoint>, String>>,
    scroll: &ScrollState,
) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(theme::panel_block("Hourly Usage"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(theme::panel_block("Hourly Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(points)) if points.is_empty() => {
            let widget = Paragraph::new("No hourly usage data found. Press r to refresh.")
                .style(theme::muted_style())
                .block(theme::panel_block("Hourly Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(points)) => render_table(frame, area, points, scroll),
    }
}

fn render_table(frame: &mut Frame, area: Rect, points: &[TrendPoint], scroll: &ScrollState) {
    let block = theme::panel_block("Hourly Usage");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let very_narrow = inner.width < 56;
    let narrow = inner.width < 84;
    let total_tokens: i64 = points.iter().map(|point| point.total_tokens.max(0)).sum();
    let peak_tokens = points
        .iter()
        .map(|point| point.total_tokens.max(0))
        .max()
        .unwrap_or(0);
    let visible_height = inner.height.saturating_sub(2).max(1) as usize;

    let rows = points
        .iter()
        .rev()
        .skip(scroll.offset)
        .take(visible_height)
        .enumerate()
        .map(|(index, point)| {
            let share = if total_tokens > 0 {
                point.total_tokens.max(0) as f64 / total_tokens as f64 * 100.0
            } else {
                0.0
            };
            let cells = if very_narrow {
                vec![
                    Cell::from(format_hour_label(&point.label, true)),
                    Cell::from(format_tokens(point.total_tokens)),
                ]
            } else if narrow {
                vec![
                    Cell::from(format_hour_label(&point.label, true)),
                    Cell::from(format_tokens(point.total_tokens)),
                    Cell::from(format!("{share:.0}%")),
                ]
            } else {
                vec![
                    Cell::from(format_hour_label(&point.label, false)).style(theme::bold_style()),
                    Cell::from(format_tokens(point.total_tokens)),
                    Cell::from(format!("{share:.1}%")),
                    Cell::from(render_bar(point.total_tokens, peak_tokens, 24))
                        .style(theme::fg_style(theme::positive_fg())),
                ]
            };

            let mut row = Row::new(cells);
            if point.total_tokens == peak_tokens && peak_tokens > 0 {
                row = row.style(theme::bold_fg_style(theme::warning_fg()));
            } else if index % 2 == 1 {
                row = row.style(theme::row_alt_style());
            }
            row
        });

    let header = Row::new(header_cells(very_narrow, narrow))
        .style(theme::header_style())
        .bottom_margin(1);
    let table = Table::new(rows, widths(very_narrow, narrow)).header(header);
    frame.render_widget(table, inner);
}

fn header_cells(very_narrow: bool, narrow: bool) -> Vec<Cell<'static>> {
    let labels: &[&str] = if very_narrow {
        &["Hour", "Tokens"]
    } else if narrow {
        &["Hour", "Tokens", "Share"]
    } else {
        &["Hour", "Tokens", "Share", "Profile"]
    };
    labels
        .iter()
        .map(|label| Cell::from(Span::styled(*label, theme::header_style())))
        .collect()
}

fn widths(very_narrow: bool, narrow: bool) -> Vec<Constraint> {
    if very_narrow {
        vec![Constraint::Percentage(48), Constraint::Percentage(52)]
    } else if narrow {
        vec![
            Constraint::Percentage(42),
            Constraint::Percentage(36),
            Constraint::Percentage(22),
        ]
    } else {
        vec![
            Constraint::Length(17),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Min(12),
        ]
    }
}

fn format_hour_label(label: &str, compact: bool) -> String {
    if let Some(dt) = parse_datetime(label) {
        if compact {
            dt.format("%H:%M").to_string()
        } else {
            dt.format("%m-%d %H:%M").to_string()
        }
    } else if compact && label.len() >= 5 {
        label.chars().take(5).collect()
    } else {
        label.chars().take(if compact { 8 } else { 16 }).collect()
    }
}

fn parse_datetime(label: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(label)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(label, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(label, "%Y-%m-%d %H:%M")
                .ok()
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
        })
}

fn render_bar(value: i64, max_value: i64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let filled = if value <= 0 || max_value <= 0 {
        0
    } else {
        ((value as f64 / max_value as f64) * width as f64).round() as usize
    }
    .min(width);
    format!("{}{}", "#".repeat(filled), "-".repeat(width - filled))
}
