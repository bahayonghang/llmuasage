use chrono::{Datelike, NaiveDate};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::{
    query::{HeatmapPoint, SourceBreakdown},
    tui::{
        app::StatsPanelPayload,
        format::{grouped as format_number, tokens as format_tokens},
        theme,
    },
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
    // Grant the contribution card enough height for a 7-row calendar grid only
    // on tall panels; smaller sizes keep the historical single-row strip.
    let contribution_height = if inner.height >= 24 { 10 } else { 6 };
    let constraints = if show_contribution && show_health {
        vec![
            Constraint::Length(5),
            Constraint::Length(contribution_height),
            Constraint::Min(5),
            Constraint::Length(4),
        ]
    } else if show_contribution {
        vec![
            Constraint::Length(5),
            Constraint::Length(contribution_height),
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
    let longest_streak = longest_streak(&payload.heatmap);
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
                metric_style(theme::metric_input()),
            ),
            Span::styled("  events ", theme::muted_style()),
            Span::styled(
                format_number(payload.overview.total_events),
                metric_style(theme::metric_output()),
            ),
            Span::styled("  cost ", theme::muted_style()),
            Span::styled(
                format!("${:.2}", payload.overview.total_cost_usd),
                metric_style(theme::metric_reasoning()),
            ),
        ]),
        Line::from(vec![
            Span::styled("active days ", theme::muted_style()),
            Span::styled(active_days.to_string(), metric_style(theme::positive_fg())),
            Span::styled("  current streak ", theme::muted_style()),
            Span::styled(
                format!("{current_streak}/{longest_streak}d"),
                metric_style(theme::metric_cache_write()),
            ),
            Span::styled("  best day ", theme::muted_style()),
            Span::styled(best_day_text, metric_style(theme::metric_input())),
        ]),
        Line::from(vec![
            Span::styled("sources ", theme::muted_style()),
            Span::styled(
                payload.sources.len().to_string(),
                metric_style(theme::metric_input()),
            ),
            Span::styled("  cache read ", theme::muted_style()),
            Span::styled(
                format!("{:.1}%", payload.overview.cache_efficiency * 100.0),
                metric_style(theme::positive_fg()),
            ),
            Span::styled("  failures ", theme::muted_style()),
            Span::styled(
                payload.health.recent_failures.len().to_string(),
                if payload.health.recent_failures.is_empty() {
                    metric_style(theme::positive_fg())
                } else {
                    metric_style(theme::warning_fg())
                },
            ),
        ]),
        context_pressure_line(&payload.context_pressure),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

/// Renders the context-window utilization row. Falls back to `n/a` when no
/// filtered event has a known model context window.
fn context_pressure_line(pressure: &crate::query::ContextPressurePayload) -> Line<'static> {
    if pressure.priced_events == 0 {
        return Line::from(vec![
            Span::styled("context ", theme::muted_style()),
            Span::styled("n/a", theme::muted_style()),
        ]);
    }
    let peak_pct = pressure.peak_percent * 100.0;
    let avg_pct = pressure.avg_percent * 100.0;
    let peak_color = theme::bar_color(peak_pct);
    Line::from(vec![
        Span::styled("context peak ", theme::muted_style()),
        Span::styled(format!("{peak_pct:.0}%"), metric_style(peak_color)),
        Span::styled("  avg ", theme::muted_style()),
        Span::styled(format!("{avg_pct:.0}%"), metric_style(theme::accent())),
        Span::styled("  unknown ", theme::muted_style()),
        Span::styled(pressure.unpriced_events.to_string(), theme::muted_style()),
    ])
}

fn render_contribution(frame: &mut Frame, area: Rect, heatmap: &[HeatmapPoint]) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let block = theme::trend_card_block("Contribution", theme::accent());
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

    // A GitHub-style 7-row calendar grid needs room for the week rows plus a
    // caption line; otherwise fall back to the compact single-row strip.
    const GRID_ROWS: u16 = 7;
    if inner.height < GRID_ROWS + 1 {
        render_contribution_strip(frame, inner, heatmap);
        return;
    }
    let Some(first_weekday) = weekday_index(&heatmap[0].date) else {
        render_contribution_strip(frame, inner, heatmap);
        return;
    };

    let thresholds = contribution_thresholds(heatmap);
    let columns = (first_weekday + heatmap.len()).div_ceil(GRID_ROWS as usize);
    let visible_cols = (inner.width as usize).min(columns).max(1);
    let start_col = columns - visible_cols;

    for (idx, point) in heatmap.iter().enumerate() {
        let slot = first_weekday + idx;
        let col = slot / GRID_ROWS as usize;
        if col < start_col {
            continue;
        }
        let x = inner.x + (col - start_col) as u16;
        let y = inner.y + (slot % GRID_ROWS as usize) as u16;
        if x >= inner.x + inner.width || y >= inner.y + GRID_ROWS {
            continue;
        }
        let bucket = contribution_bucket(point.total_tokens, &thresholds);
        frame.buffer_mut()[(x, y)]
            .set_symbol("\u{25A0}")
            .set_style(theme::fg_style(theme::heat(bucket)));
    }

    // Caption: date range on the left, a low→high legend on the right.
    let first = heatmap.first().map(|point| compact_date(&point.date));
    let last = heatmap.last().map(|point| compact_date(&point.date));
    if let (Some(first), Some(last)) = (first, last) {
        let caption = format!("{first} .. {last}");
        frame.buffer_mut().set_stringn(
            inner.x,
            inner.y + GRID_ROWS,
            &caption,
            inner.width as usize,
            theme::muted_style(),
        );
        render_heat_legend(frame, inner, GRID_ROWS, caption.chars().count());
    }
}

/// Draws a `less ▁▂▃▄ more` legend at the right of the caption row.
fn render_heat_legend(frame: &mut Frame, inner: Rect, row_offset: u16, caption_len: usize) {
    let legend = "  less ";
    let squares = 4usize;
    let needed = caption_len + legend.chars().count() + squares + " more".len();
    if needed > inner.width as usize {
        return;
    }
    let y = inner.y + row_offset;
    let mut x = inner.x + caption_len as u16;
    frame
        .buffer_mut()
        .set_stringn(x, y, legend, legend.len(), theme::muted_style());
    x += legend.chars().count() as u16;
    for level in 0..squares {
        frame.buffer_mut()[(x, y)]
            .set_symbol("\u{25A0}")
            .set_style(theme::fg_style(theme::heat(level + 1)));
        x += 1;
    }
    frame
        .buffer_mut()
        .set_stringn(x, y, " more", 5, theme::muted_style());
}

/// Compact single-row heat strip used when the panel is too short for the grid.
fn render_contribution_strip(frame: &mut Frame, inner: Rect, heatmap: &[HeatmapPoint]) {
    let thresholds = contribution_thresholds(heatmap);
    let days = heatmap.len().min(inner.width as usize);
    let recent = &heatmap[heatmap.len().saturating_sub(days)..];
    for (idx, point) in recent.iter().enumerate() {
        let bucket = contribution_bucket(point.total_tokens, &thresholds);
        let symbol = if bucket == 0 { "." } else { "\u{25A0}" };
        frame.buffer_mut()[(inner.x + idx as u16, inner.y)]
            .set_symbol(symbol)
            .set_style(theme::fg_style(theme::heat(bucket)));
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

/// Sunday-indexed weekday (0..=6) for a `YYYY-MM-DD` date, or `None` if unparseable.
fn weekday_index(date: &str) -> Option<usize> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .map(|date| date.weekday().num_days_from_sunday() as usize)
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
            .block(theme::trend_card_block("Source Mix", theme::metric_input()));
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
    let visible_height = super::visible_table_rows(area);
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
        .block(theme::trend_card_block("Source Mix", theme::metric_input()));
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
                metric_style(theme::metric_input()),
            ),
            Span::styled("  cursors ", theme::muted_style()),
            Span::styled(
                payload.health.cursors.len().to_string(),
                metric_style(theme::positive_fg()),
            ),
            Span::styled("  recent failures ", theme::muted_style()),
            Span::styled(
                payload.health.recent_failures.len().to_string(),
                if payload.health.recent_failures.is_empty() {
                    metric_style(theme::positive_fg())
                } else {
                    metric_style(theme::warning_fg())
                },
            ),
        ]),
        Line::styled(
            "health is summarized here; source details stay backed by existing diagnostics",
            theme::muted_style(),
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(theme::trend_card_block(
            "Health Signals",
            theme::metric_cache_write(),
        )),
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
            Cell::from(source.source.clone()).style(theme::bold_style()),
            Cell::from(format_tokens(source.total_tokens)),
            Cell::from(format_number(source.event_count)),
            Cell::from(
                source
                    .last_event_at
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            ),
            Cell::from(render_bar(source.total_tokens, max_tokens, 20))
                .style(theme::fg_style(theme::positive_fg())),
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

/// Longest run of consecutive active days (`event_count > 0`) anywhere in the
/// zero-filled, date-ordered heatmap window.
fn longest_streak(heatmap: &[HeatmapPoint]) -> usize {
    let mut longest = 0usize;
    let mut run = 0usize;
    for point in heatmap {
        if point.event_count > 0 {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest
}

/// Quantile cut points (P25/P50/P75/P99) over positive daily token totals,
/// used to bucket days into the 4 non-empty heat levels.
fn contribution_thresholds(heatmap: &[HeatmapPoint]) -> [i64; 4] {
    let mut values: Vec<i64> = heatmap
        .iter()
        .map(|point| point.total_tokens)
        .filter(|value| *value > 0)
        .collect();
    if values.is_empty() {
        return [0; 4];
    }
    values.sort_unstable();
    let quantile = |q: f64| -> i64 {
        let idx = ((values.len() as f64 - 1.0) * q).round() as usize;
        values[idx.min(values.len() - 1)]
    };
    [
        quantile(0.25),
        quantile(0.50),
        quantile(0.75),
        quantile(0.99),
    ]
}

/// Maps a day's token total to a heat bucket: 0 = no data, 1..=4 = light→dark.
fn contribution_bucket(value: i64, thresholds: &[i64; 4]) -> usize {
    if value <= 0 {
        return 0;
    }
    if value >= thresholds[3] {
        4
    } else if value >= thresholds[2] {
        3
    } else if value >= thresholds[1] {
        2
    } else {
        1
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
    theme::bold_fg_style(color)
}

#[cfg(test)]
mod tests {
    use super::{
        contribution_bucket, contribution_thresholds, current_streak, longest_streak, weekday_index,
    };
    use crate::query::HeatmapPoint;

    fn heat(counts: &[i64]) -> Vec<HeatmapPoint> {
        counts
            .iter()
            .enumerate()
            .map(|(idx, &count)| HeatmapPoint {
                date: format!("2026-01-{:02}", idx + 1),
                event_count: count,
                total_tokens: count * 100,
            })
            .collect()
    }

    #[test]
    fn longest_streak_all_zero_is_zero() {
        assert_eq!(longest_streak(&heat(&[0, 0, 0])), 0);
        assert_eq!(current_streak(&heat(&[0, 0, 0])), 0);
    }

    #[test]
    fn longest_streak_single_segment() {
        assert_eq!(longest_streak(&heat(&[0, 1, 1, 1, 0])), 3);
    }

    #[test]
    fn longest_streak_picks_max_of_multiple_segments() {
        // segments of length 2 and 4; longest is 4, current (trailing) is 1
        assert_eq!(longest_streak(&heat(&[1, 1, 0, 1, 1, 1, 1, 0, 1])), 4);
        assert_eq!(current_streak(&heat(&[1, 1, 0, 1, 1, 1, 1, 0, 1])), 1);
    }

    #[test]
    fn longest_streak_trailing_run_counts() {
        // longest equals the trailing run when it is the largest
        let data = heat(&[1, 0, 1, 1, 1, 1, 1]);
        assert_eq!(longest_streak(&data), 5);
        assert_eq!(current_streak(&data), 5);
    }

    #[test]
    fn contribution_bucket_partitions_by_quantile() {
        // Positive totals 100..=1000 (heat multiplies count by 100).
        let data = heat(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0]);
        let thresholds = contribution_thresholds(&data);
        // Zero days are bucket 0, positive days land in 1..=4.
        assert_eq!(contribution_bucket(0, &thresholds), 0);
        assert!((1..=4).contains(&contribution_bucket(100, &thresholds)));
        assert_eq!(contribution_bucket(1_000, &thresholds), 4);
        // Monotonic: larger totals never map to a lower bucket.
        let low = contribution_bucket(200, &thresholds);
        let high = contribution_bucket(900, &thresholds);
        assert!(high >= low);
    }

    #[test]
    fn contribution_thresholds_empty_is_zero() {
        let data = heat(&[0, 0, 0]);
        assert_eq!(contribution_thresholds(&data), [0; 4]);
        assert_eq!(contribution_bucket(0, &[0; 4]), 0);
    }

    #[test]
    fn weekday_index_maps_known_dates() {
        // 2026-01-01 is a Thursday → 4 days from Sunday.
        assert_eq!(weekday_index("2026-01-01"), Some(4));
        // 2026-01-04 is a Sunday → 0.
        assert_eq!(weekday_index("2026-01-04"), Some(0));
        assert_eq!(weekday_index("not-a-date"), None);
    }
}
