use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::query::DailyTrendPoint;
use crate::tui::theme;

use super::super::app::ScrollState;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<DailyTrendPoint>, String>>,
    scroll: &ScrollState,
) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(theme::panel_block("Daily Usage"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(theme::panel_block("Daily Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("No daily usage data found. Press r to refresh.")
                .style(theme::muted_style())
                .block(theme::panel_block("Daily Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll),
    }
}

fn render_table(frame: &mut Frame, area: Rect, items: &[DailyTrendPoint], scroll: &ScrollState) {
    let block = theme::panel_block("Daily Usage");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let very_narrow = inner.width < 58;
    let narrow = inner.width < 92;
    let [table_area, detail_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    let visible_height = table_area.height.saturating_sub(2).max(1) as usize;
    let rows = newest_first(items)
        .skip(scroll.offset)
        .take(visible_height)
        .enumerate()
        .map(|(index, day)| {
            let cells = if very_narrow {
                vec![
                    Cell::from(compact_date(&day.date)),
                    Cell::from(format_cost(day.cost_with_cache_usd))
                        .style(Style::default().fg(Color::Green)),
                ]
            } else if narrow {
                vec![
                    Cell::from(compact_date(&day.date)),
                    Cell::from(format_number(day.event_count)),
                    Cell::from(format_tokens(day.total_tokens)),
                    Cell::from(format_cost(day.cost_with_cache_usd))
                        .style(Style::default().fg(Color::Green)),
                ]
            } else {
                vec![
                    Cell::from(day.date.clone())
                        .style(Style::default().add_modifier(Modifier::BOLD)),
                    Cell::from(format_number(day.event_count)),
                    Cell::from(format_tokens(day.input_tokens)).style(metric_style(Color::Cyan)),
                    Cell::from(format_tokens(day.output_tokens)).style(metric_style(Color::Green)),
                    Cell::from(format_tokens(day.cache_read_tokens))
                        .style(metric_style(Color::Blue)),
                    Cell::from(format_tokens(day.cache_creation_tokens))
                        .style(metric_style(Color::Magenta)),
                    Cell::from(cache_hit_rate(day)).style(metric_style(Color::Yellow)),
                    Cell::from(format_tokens(day.total_tokens)),
                    Cell::from(format_cost(day.cost_with_cache_usd))
                        .style(Style::default().fg(Color::Green)),
                ]
            };

            let row = Row::new(cells);
            if index % 2 == 1 {
                row.style(theme::row_alt_style())
            } else {
                row
            }
        });

    let header = Row::new(header_cells(very_narrow, narrow))
        .style(theme::header_style())
        .bottom_margin(1);
    let table = Table::new(rows, widths(very_narrow, narrow)).header(header);

    frame.render_widget(table, table_area);
    render_detail(
        frame,
        detail_area,
        items,
        scroll.offset,
        narrow || very_narrow,
    );
}

fn newest_first(items: &[DailyTrendPoint]) -> impl Iterator<Item = &DailyTrendPoint> {
    items.iter().rev()
}

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    items: &[DailyTrendPoint],
    offset: usize,
    compact: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(day) = newest_first(items).nth(offset.min(items.len().saturating_sub(1))) else {
        return;
    };
    let line = Line::from(vec![
        Span::styled("detail ", theme::muted_style()),
        Span::styled(
            if compact {
                compact_date(&day.date)
            } else {
                day.date.clone()
            },
            theme::block_title_style(),
        ),
        Span::styled("  input ", theme::muted_style()),
        Span::styled(format_tokens(day.input_tokens), metric_style(Color::Cyan)),
        Span::styled("  output ", theme::muted_style()),
        Span::styled(format_tokens(day.output_tokens), metric_style(Color::Green)),
        Span::styled("  cache R/W ", theme::muted_style()),
        Span::styled(
            format!(
                "{}/{}",
                format_tokens(day.cache_read_tokens),
                format_tokens(day.cache_creation_tokens)
            ),
            metric_style(Color::Magenta),
        ),
        Span::styled("  cost ", theme::muted_style()),
        Span::styled(
            format_cost(day.cost_with_cache_usd),
            metric_style(Color::Green),
        ),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme::muted_style()), area);
}

fn header_cells(very_narrow: bool, narrow: bool) -> Vec<Cell<'static>> {
    let labels: &[&str] = if very_narrow {
        &["Date", "Cost"]
    } else if narrow {
        &["Date", "Events", "Tokens", "Cost"]
    } else {
        &[
            "Date", "Events", "Input", "Output", "Cache R", "Cache W", "Cache%", "Total", "Cost",
        ]
    };
    labels
        .iter()
        .map(|label| Cell::from(Span::styled(*label, theme::header_style())))
        .collect()
}

fn widths(very_narrow: bool, narrow: bool) -> Vec<Constraint> {
    if very_narrow {
        vec![Constraint::Percentage(56), Constraint::Percentage(44)]
    } else if narrow {
        vec![
            Constraint::Percentage(24),
            Constraint::Percentage(18),
            Constraint::Percentage(32),
            Constraint::Percentage(26),
        ]
    } else {
        vec![
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(10),
        ]
    }
}

fn compact_date(date: &str) -> String {
    if date.len() >= 10 && date.as_bytes().get(4) == Some(&b'-') {
        date.chars().skip(5).take(5).collect()
    } else {
        date.chars().take(8).collect()
    }
}

fn cache_hit_rate(day: &DailyTrendPoint) -> String {
    let prompt = day.input_tokens + day.cache_read_tokens + day.cache_creation_tokens;
    if prompt <= 0 {
        "-".to_string()
    } else {
        format!(
            "{:.0}%",
            day.cache_read_tokens.max(0) as f64 / prompt as f64 * 100.0
        )
    }
}

fn metric_style(color: Color) -> Style {
    Style::default().fg(color)
}

fn format_cost(value: f64) -> String {
    format!("${value:.2}")
}

fn format_tokens(value: i64) -> String {
    if value.abs() >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value.abs() >= 10_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        format_number(value)
    }
}

fn format_number(value: i64) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let s = value.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    if value < 0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}
