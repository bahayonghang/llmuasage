use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::{
    query::{HeatmapPoint, SourceBreakdown},
    tui::{app::StatsPanelPayload, theme},
};

use super::super::app::ScrollState;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<StatsPanelPayload, String>>,
    scroll: &ScrollState,
) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(theme::panel_block("Stats"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(theme::panel_block("Stats"));
            frame.render_widget(widget, area);
        }
        Some(Ok(payload)) => render_payload(frame, area, payload, scroll),
    }
}

fn render_payload(
    frame: &mut Frame,
    area: Rect,
    payload: &StatsPanelPayload,
    scroll: &ScrollState,
) {
    let block = theme::panel_block("Stats");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let show_contribution = inner.height >= 13;
    let show_health = inner.height >= 18;
    let constraints = if show_contribution && show_health {
        vec![
            Constraint::Length(5),
            Constraint::Length(6),
            Constraint::Min(5),
            Constraint::Length(4),
        ]
    } else if show_contribution {
        vec![
            Constraint::Length(5),
            Constraint::Length(6),
            Constraint::Min(5),
        ]
    } else {
        vec![Constraint::Length(5), Constraint::Min(5)]
    };
    let chunks = Layout::vertical(constraints).split(inner);

    render_summary(frame, chunks[0], payload);
    if show_contribution {
        render_contribution(frame, chunks[1], &payload.heatmap);
        render_sources(frame, chunks[2], &payload.sources, scroll);
        if show_health {
            render_health_summary(frame, chunks[3], payload);
        }
    } else {
        render_sources(frame, chunks[1], &payload.sources, scroll);
    }
}

fn render_summary(frame: &mut Frame, area: Rect, payload: &StatsPanelPayload) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let active_days = payload
        .heatmap
        .iter()
        .filter(|point| point.event_count > 0)
        .count();
    let current_streak = current_streak(&payload.heatmap);
    let best_day = payload
        .heatmap
        .iter()
        .max_by_key(|point| point.total_tokens)
        .filter(|point| point.total_tokens > 0);
    let best_day_text = best_day
        .map(|point| {
            format!(
                "{} {}",
                compact_date(&point.date),
                format_tokens(point.total_tokens)
            )
        })
        .unwrap_or_else(|| "none".to_string());

    let lines = vec![
        Line::from(vec![
            Span::styled("total tokens ", theme::muted_style()),
            Span::styled(
                format_tokens(payload.overview.total.total_tokens),
                metric_style(Color::Cyan),
            ),
            Span::styled("  events ", theme::muted_style()),
            Span::styled(
                format_number(payload.overview.total_events),
                metric_style(Color::Green),
            ),
            Span::styled("  cost ", theme::muted_style()),
            Span::styled(
                format!("${:.2}", payload.overview.total_cost_usd),
                metric_style(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::styled("active days ", theme::muted_style()),
            Span::styled(active_days.to_string(), metric_style(Color::Green)),
            Span::styled("  current streak ", theme::muted_style()),
            Span::styled(format!("{current_streak}d"), metric_style(Color::Magenta)),
            Span::styled("  best day ", theme::muted_style()),
            Span::styled(best_day_text, metric_style(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("sources ", theme::muted_style()),
            Span::styled(payload.sources.len().to_string(), metric_style(Color::Cyan)),
            Span::styled("  cache read ", theme::muted_style()),
            Span::styled(
                format!("{:.1}%", payload.overview.cache_efficiency * 100.0),
                metric_style(Color::Green),
            ),
            Span::styled("  failures ", theme::muted_style()),
            Span::styled(
                payload.health.recent_failures.len().to_string(),
                if payload.health.recent_failures.is_empty() {
                    metric_style(Color::Green)
                } else {
                    metric_style(Color::Yellow)
                },
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_contribution(frame: &mut Frame, area: Rect, heatmap: &[HeatmapPoint]) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let block = theme::trend_card_block("Contribution", Color::Green);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if heatmap.is_empty() {
        frame.render_widget(
            Paragraph::new("No heatmap data").style(theme::muted_style()),
            inner,
        );
        return;
    }

    let max_tokens = heatmap
        .iter()
        .map(|point| point.total_tokens.max(0))
        .max()
        .unwrap_or(0);
    let days = heatmap.len().min(inner.width as usize);
    let recent = &heatmap[heatmap.len().saturating_sub(days)..];
    for (idx, point) in recent.iter().enumerate() {
        let marker = contribution_marker(point.total_tokens, max_tokens);
        let color = contribution_color(point.total_tokens, max_tokens);
        frame.buffer_mut()[(inner.x + idx as u16, inner.y)]
            .set_symbol(marker)
            .set_style(Style::default().fg(color));
    }

    if inner.height > 1 {
        let first = recent.first().map(|point| compact_date(&point.date));
        let last = recent.last().map(|point| compact_date(&point.date));
        let label = match (first, last) {
            (Some(first), Some(last)) => format!("{first} .. {last}"),
            _ => "no dates".to_string(),
        };
        frame.buffer_mut().set_stringn(
            inner.x,
            inner.y + 1,
            label,
            inner.width as usize,
            theme::muted_style(),
        );
    }
}

fn render_sources(
    frame: &mut Frame,
    area: Rect,
    sources: &[SourceBreakdown],
    scroll: &ScrollState,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if sources.is_empty() {
        let empty = Paragraph::new("No source contribution data.")
            .style(theme::muted_style())
            .block(theme::trend_card_block("Source Mix", Color::Cyan));
        frame.render_widget(empty, area);
        return;
    }

    let very_narrow = area.width < 54;
    let narrow = area.width < 84;
    let max_tokens = sources
        .iter()
        .map(|source| source.total_tokens.max(0))
        .max()
        .unwrap_or(0);
    let visible_height = area.height.saturating_sub(3).max(1) as usize;
    let rows = sources
        .iter()
        .skip(scroll.offset)
        .take(visible_height)
        .enumerate()
        .map(|(index, source)| source_row(source, max_tokens, index, very_narrow, narrow));

    let header = Row::new(source_header(very_narrow, narrow))
        .style(theme::header_style())
        .bottom_margin(1);
    let table = Table::new(rows, source_widths(very_narrow, narrow))
        .header(header)
        .block(theme::trend_card_block("Source Mix", Color::Cyan));
    frame.render_widget(table, area);
}

fn render_health_summary(frame: &mut Frame, area: Rect, payload: &StatsPanelPayload) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let lines = vec![
        Line::from(vec![
            Span::styled("integrations ", theme::muted_style()),
            Span::styled(
                payload.health.integrations.len().to_string(),
                metric_style(Color::Cyan),
            ),
            Span::styled("  cursors ", theme::muted_style()),
            Span::styled(
                payload.health.cursors.len().to_string(),
                metric_style(Color::Green),
            ),
            Span::styled("  recent failures ", theme::muted_style()),
            Span::styled(
                payload.health.recent_failures.len().to_string(),
                if payload.health.recent_failures.is_empty() {
                    metric_style(Color::Green)
                } else {
                    metric_style(Color::Yellow)
                },
            ),
        ]),
        Line::styled(
            "health is summarized here; source details stay backed by existing diagnostics",
            theme::muted_style(),
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(theme::trend_card_block("Health Signals", Color::Magenta)),
        area,
    );
}

fn source_row(
    source: &SourceBreakdown,
    max_tokens: i64,
    index: usize,
    very_narrow: bool,
    narrow: bool,
) -> Row<'static> {
    let mut row = if very_narrow {
        Row::new(vec![
            Cell::from(source.source.clone()),
            Cell::from(format_tokens(source.total_tokens)),
        ])
    } else if narrow {
        Row::new(vec![
            Cell::from(source.source.clone()),
            Cell::from(format_tokens(source.total_tokens)),
            Cell::from(format_number(source.event_count)),
        ])
    } else {
        Row::new(vec![
            Cell::from(source.source.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from(format_tokens(source.total_tokens)),
            Cell::from(format_number(source.event_count)),
            Cell::from(
                source
                    .last_event_at
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            ),
            Cell::from(render_bar(source.total_tokens, max_tokens, 20))
                .style(Style::default().fg(Color::Green)),
        ])
    };

    if index % 2 == 1 {
        row = row.style(theme::row_alt_style());
    }
    row
}

fn source_header(very_narrow: bool, narrow: bool) -> Vec<Cell<'static>> {
    let labels: &[&str] = if very_narrow {
        &["Source", "Tokens"]
    } else if narrow {
        &["Source", "Tokens", "Events"]
    } else {
        &["Source", "Tokens", "Events", "Last Event", "Profile"]
    };
    labels
        .iter()
        .map(|label| Cell::from(Span::styled(*label, theme::header_style())))
        .collect()
}

fn source_widths(very_narrow: bool, narrow: bool) -> Vec<Constraint> {
    if very_narrow {
        vec![Constraint::Percentage(46), Constraint::Percentage(54)]
    } else if narrow {
        vec![
            Constraint::Percentage(34),
            Constraint::Percentage(36),
            Constraint::Percentage(30),
        ]
    } else {
        vec![
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Min(18),
            Constraint::Length(22),
        ]
    }
}

fn current_streak(heatmap: &[HeatmapPoint]) -> usize {
    heatmap
        .iter()
        .rev()
        .take_while(|point| point.event_count > 0)
        .count()
}

fn contribution_marker(value: i64, max_value: i64) -> &'static str {
    if value <= 0 || max_value <= 0 {
        "."
    } else {
        let ratio = value as f64 / max_value as f64;
        if ratio >= 0.75 {
            "#"
        } else if ratio >= 0.35 {
            "+"
        } else {
            "-"
        }
    }
}

fn contribution_color(value: i64, max_value: i64) -> Color {
    if value <= 0 || max_value <= 0 {
        theme::MUTED_FG
    } else {
        let ratio = value as f64 / max_value as f64;
        if ratio >= 0.75 {
            Color::Green
        } else if ratio >= 0.35 {
            Color::Yellow
        } else {
            Color::Cyan
        }
    }
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

fn compact_date(date: &str) -> String {
    if date.len() >= 10 && date.as_bytes().get(4) == Some(&b'-') {
        date.chars().skip(5).take(5).collect()
    } else {
        date.chars().take(8).collect()
    }
}

fn metric_style(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
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
