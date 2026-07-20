use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::query::DailyTrendPoint;
use crate::tui::{
    format::{cost as format_cost, grouped as format_number, tokens as format_tokens},
    theme,
};

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
                        .style(theme::fg_style(theme::positive_fg())),
                ]
            } else if narrow {
                vec![
                    Cell::from(compact_date(&day.date)),
                    Cell::from(format_number(day.event_count)),
                    Cell::from(format_tokens(day.total_tokens)),
                    Cell::from(format_cost(day.cost_with_cache_usd))
                        .style(theme::fg_style(theme::positive_fg())),
                ]
            } else {
                vec![
                    Cell::from(day.date.clone()).style(theme::bold_style()),
                    Cell::from(format_number(day.event_count)),
                    Cell::from(format_tokens(day.input_tokens))
                        .style(metric_style(theme::metric_input())),
                    Cell::from(format_tokens(day.output_tokens))
                        .style(metric_style(theme::metric_output())),
                    Cell::from(format_tokens(day.cache_read_tokens))
                        .style(metric_style(theme::metric_cache_read())),
                    Cell::from(format_tokens(day.cache_creation_tokens))
                        .style(metric_style(theme::metric_cache_write())),
                    Cell::from(cache_hit_rate(day)).style(metric_style(theme::warning_fg())),
                    Cell::from(format_tokens(day.total_tokens)),
                    Cell::from(format_cost(day.cost_with_cache_usd))
                        .style(theme::fg_style(theme::positive_fg())),
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
        Span::styled(
            format_tokens(day.input_tokens),
            metric_style(theme::metric_input()),
        ),
        Span::styled("  output ", theme::muted_style()),
        Span::styled(
            format_tokens(day.output_tokens),
            metric_style(theme::metric_output()),
        ),
        Span::styled("  cache R/W ", theme::muted_style()),
        Span::styled(
            format!(
                "{}/{}",
                format_tokens(day.cache_read_tokens),
                format_tokens(day.cache_creation_tokens)
            ),
            metric_style(theme::metric_cache_write()),
        ),
        Span::styled("  cost ", theme::muted_style()),
        Span::styled(
            format_cost(day.cost_with_cache_usd),
            metric_style(theme::positive_fg()),
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
    theme::fg_style(color)
}
