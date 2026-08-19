use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::query::{HeatmapPoint, PeriodDetailRow};
use crate::tui::{
    app::{
        PeriodDetailKind, PeriodDetailPayload, PeriodDetailState, ScrollState, StatsPanelPayload,
    },
    format::{cost_compact, stat_compact},
    model_vendor::vendor_from_model,
    theme,
};

const GRAPH_TITLE: &str = "Contribution Graph (52 weeks)";
const BREAKDOWN_TITLE: &str = "Day Breakdown (ESC to close)";
const CELL_WIDTH: u16 = 2;
const MIN_GRAPH: u16 = 11;
const STATS_FULL: u16 = 12;
const STATS_COMPACT: u16 = 8;
const MIN_BREAKDOWN: u16 = 6;
const SHORT_BREAKDOWN: u16 = 12;
const MONTH_LABELS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAY_LABELS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

pub(crate) struct StatsSplit {
    pub graph: Rect,
    pub stats: Option<Rect>,
    pub breakdown: Option<Rect>,
}

/// Shared calendar geometry for paint and mouse hit-testing.
#[derive(Debug, Clone, Copy)]
struct GraphLayout {
    start_x: u16,
    start_y: u16,
    label_width: u16,
    first_weekday: usize,
    start_week: usize,
    visible_weeks: usize,
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<StatsPanelPayload, String>>,
    scroll: &ScrollState,
    detail: Option<&PeriodDetailState>,
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
        Some(Ok(payload)) => render_payload(frame, area, payload, scroll, detail),
    }
}

pub(crate) fn split_stats_area(area: Rect, selected: bool) -> StatsSplit {
    if selected {
        if area.height >= MIN_GRAPH + STATS_COMPACT + MIN_BREAKDOWN {
            let chunks = Layout::vertical([
                Constraint::Length(MIN_GRAPH),
                Constraint::Length(STATS_COMPACT),
                Constraint::Min(MIN_BREAKDOWN),
            ])
            .split(area);
            StatsSplit {
                graph: chunks[0],
                stats: Some(chunks[1]),
                breakdown: Some(chunks[2]),
            }
        } else {
            let chunks = Layout::vertical([
                Constraint::Min(MIN_GRAPH),
                Constraint::Length(SHORT_BREAKDOWN),
            ])
            .split(area);
            StatsSplit {
                graph: chunks[0],
                stats: None,
                breakdown: Some(chunks[1]),
            }
        }
    } else {
        let chunks = Layout::vertical([
            Constraint::Length(MIN_GRAPH),
            Constraint::Length(STATS_FULL),
            Constraint::Min(0),
        ])
        .split(area);
        StatsSplit {
            graph: chunks[0],
            stats: Some(chunks[1]),
            breakdown: None,
        }
    }
}

pub(crate) fn day_at(
    graph_area: Rect,
    heatmap: &[HeatmapPoint],
    x: u16,
    y: u16,
) -> Option<NaiveDate> {
    GraphLayout::from_area(graph_area, heatmap)?.hit(heatmap, x, y)
}

pub(crate) fn breakdown_scroll_total(rows: &[PeriodDetailRow]) -> usize {
    if rows.is_empty() {
        return 0;
    }
    2 + grouped_models(rows)
        .into_iter()
        .map(|group| 1 + 2 * group.models.len())
        .sum::<usize>()
}

fn render_payload(
    frame: &mut Frame,
    area: Rect,
    payload: &StatsPanelPayload,
    scroll: &ScrollState,
    detail: Option<&PeriodDetailState>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let selected_date = match detail.map(|item| &item.kind) {
        Some(PeriodDetailKind::Daily { date }) => Some(date.as_str()),
        _ => None,
    };
    let split = split_stats_area(area, selected_date.is_some());
    render_graph(frame, split.graph, &payload.heatmap, selected_date);
    if let Some(stats_area) = split.stats {
        render_stats_card(frame, stats_area, payload, selected_date.is_some());
    }
    if let (Some(breakdown_area), Some(detail), Some(date)) =
        (split.breakdown, detail, selected_date)
    {
        render_breakdown(
            frame,
            breakdown_area,
            date,
            &payload.heatmap,
            detail,
            scroll,
        );
    }
}

fn render_graph(
    frame: &mut Frame,
    area: Rect,
    heatmap: &[HeatmapPoint],
    selected_date: Option<&str>,
) {
    let block = theme::panel_block(GRAPH_TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let Some(layout) = GraphLayout::from_area(area, heatmap) else {
        let message = if heatmap.is_empty() {
            "No heatmap data"
        } else {
            "Graph is too narrow"
        };
        frame.render_widget(Paragraph::new(message).style(theme::muted_style()), inner);
        return;
    };

    render_month_labels(frame, inner, heatmap, &layout);
    render_weekday_labels(frame, inner, &layout);

    let thresholds = contribution_thresholds(heatmap);
    for (idx, point) in heatmap.iter().enumerate() {
        let Some((x, y)) = layout.cell_origin(idx) else {
            continue;
        };
        if x + CELL_WIDTH > inner.right() || y >= inner.bottom() {
            continue;
        }
        let bucket = contribution_bucket(point.total_tokens, &thresholds);
        let selected = selected_date == Some(point.date.as_str());
        let symbol = if selected {
            "▓▓"
        } else if bucket == 0 {
            "· "
        } else {
            "██"
        };
        let heat_style = theme::fg_style(theme::heat(bucket));
        let style = if selected {
            theme::selection_fill_style().patch(heat_style)
        } else if bucket == 0 {
            theme::muted_style()
        } else {
            heat_style
        };
        frame.render_widget(
            Paragraph::new(symbol).style(style),
            Rect::new(x, y, CELL_WIDTH, 1),
        );
    }
}

fn render_month_labels(
    frame: &mut Frame,
    inner: Rect,
    heatmap: &[HeatmapPoint],
    layout: &GraphLayout,
) {
    let mut current_month = None;
    for vis in 0..layout.visible_weeks {
        let week = layout.start_week + vis;
        let Some(month) = layout.week_month(heatmap, week) else {
            continue;
        };
        if current_month == Some(month) {
            continue;
        }
        current_month = Some(month);
        let x = layout.start_x + vis as u16 * CELL_WIDTH;
        if x + 3 > inner.right() {
            continue;
        }
        let label = MONTH_LABELS[(month as usize).saturating_sub(1).min(11)];
        frame.render_widget(
            Paragraph::new(label).style(theme::muted_style()),
            Rect::new(x, inner.y, 3, 1),
        );
    }
}

fn render_weekday_labels(frame: &mut Frame, inner: Rect, layout: &GraphLayout) {
    if layout.label_width < 4 {
        return;
    }
    for (weekday, label) in WEEKDAY_LABELS.iter().enumerate() {
        if weekday % 2 == 0 {
            continue;
        }
        let y = layout.start_y + weekday as u16;
        if y >= inner.bottom() {
            continue;
        }
        frame.render_widget(
            Paragraph::new(*label).style(theme::muted_style()),
            Rect::new(inner.x, y, layout.label_width, 1),
        );
    }
}

fn render_stats_card(frame: &mut Frame, area: Rect, payload: &StatsPanelPayload, compact: bool) {
    let block = theme::panel_block("Stats");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let narrow = area.width < 80;
    let favorite = payload.models.iter().max_by(|left, right| {
        left.cost_with_cache_usd
            .total_cmp(&right.cost_with_cache_usd)
    });
    let active_days = payload
        .heatmap
        .iter()
        .filter(|point| point.event_count > 0)
        .count();
    let total_days = payload.heatmap.len();
    let current = current_streak(&payload.heatmap);
    let longest = longest_streak(&payload.heatmap);
    let col1_width = (inner.width / 2).max(1);
    let col2_width = inner.width.saturating_sub(col1_width).max(1);
    let col2_x = inner.x + col1_width;
    let mut y = inner.y;
    let y_max = inner.bottom();

    let favorite_label = if narrow { "Model:" } else { "Favorite model:" };
    let favorite_value = match favorite {
        Some(model) => Span::styled(
            model.model.clone(),
            theme::fg_style(theme::vendor_fg(vendor_from_model(&model.model), 0)),
        ),
        None => Span::styled("N/A", theme::muted_style()),
    };
    put_line(
        frame,
        inner.x,
        y,
        col1_width,
        labeled(favorite_label, favorite_value),
    );
    put_line(
        frame,
        col2_x,
        y,
        col2_width,
        labeled(
            if narrow { "Tokens:" } else { "Total tokens:" },
            Span::styled(
                stat_compact(payload.overview.total.total_tokens),
                theme::bold_fg_style(theme::metric_input()),
            ),
        ),
    );

    y = y.saturating_add(1);
    if y >= y_max {
        return;
    }
    put_line(
        frame,
        inner.x,
        y,
        col1_width,
        labeled(
            "Events:",
            Span::styled(
                stat_compact(payload.overview.total_events),
                theme::bold_fg_style(theme::metric_output()),
            ),
        ),
    );
    put_line(
        frame,
        col2_x,
        y,
        col2_width,
        labeled(
            if narrow { "Cost:" } else { "Total cost:" },
            Span::styled(
                cost_compact(payload.overview.total_cost_usd),
                theme::bold_fg_style(theme::positive_fg()),
            ),
        ),
    );

    y = y.saturating_add(1);
    if y >= y_max {
        return;
    }
    put_line(
        frame,
        inner.x,
        y,
        col1_width,
        labeled(
            if narrow { "Streak:" } else { "Current streak:" },
            Span::styled(
                format!("{current} days"),
                theme::bold_fg_style(theme::metric_cache_write()),
            ),
        ),
    );
    put_line(
        frame,
        col2_x,
        y,
        col2_width,
        labeled(
            if narrow {
                "Max streak:"
            } else {
                "Longest streak:"
            },
            Span::styled(
                format!("{longest} days"),
                theme::bold_fg_style(theme::metric_cache_write()),
            ),
        ),
    );

    y = y.saturating_add(1);
    if y >= y_max {
        return;
    }
    put_line(
        frame,
        inner.x,
        y,
        col1_width,
        labeled(
            if narrow { "Active:" } else { "Active days:" },
            Span::styled(
                format!("{active_days}/{total_days}"),
                theme::bold_fg_style(theme::positive_fg()),
            ),
        ),
    );

    if !compact {
        y = y.saturating_add(1);
        if y < y_max {
            put_line(
                frame,
                inner.x,
                y,
                inner.width,
                context_pressure_line(&payload.context_pressure),
            );
        }
    }

    y = y.saturating_add(1);
    if y < y_max {
        put_line(frame, inner.x, y, inner.width, heat_legend_line());
    }
}

fn render_breakdown(
    frame: &mut Frame,
    area: Rect,
    date: &str,
    heatmap: &[HeatmapPoint],
    detail: &PeriodDetailState,
    scroll: &ScrollState,
) {
    let block = theme::panel_block(BREAKDOWN_TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    match &detail.payload {
        None => {
            frame.render_widget(
                Paragraph::new("Loading...").style(theme::muted_style()),
                inner,
            );
        }
        Some(Err(error)) => {
            frame.render_widget(
                Paragraph::new(format!("Data load failed: {error}")).style(theme::error_style()),
                inner,
            );
        }
        Some(Ok(PeriodDetailPayload::Daily(rows))) => {
            render_breakdown_rows(frame, inner, date, heatmap, rows, scroll, area.width < 80);
        }
        Some(Ok(PeriodDetailPayload::Monthly(_))) => {
            frame.render_widget(
                Paragraph::new("No data for this day").style(theme::muted_style()),
                inner,
            );
        }
    }
}

fn render_breakdown_rows(
    frame: &mut Frame,
    inner: Rect,
    date: &str,
    heatmap: &[HeatmapPoint],
    rows: &[PeriodDetailRow],
    scroll: &ScrollState,
    narrow: bool,
) {
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No data for this day").style(theme::muted_style()),
            inner,
        );
        return;
    }

    let day_tokens = heatmap
        .iter()
        .find(|point| point.date == date)
        .map(|point| point.total_tokens)
        .unwrap_or(0);
    let day_cost: f64 = rows.iter().map(|row| row.cost_with_cache_usd).sum();
    let lines = breakdown_lines(date, day_tokens, day_cost, rows, narrow);
    let visible_height = inner.height.max(1) as usize;
    let range = scroll.visible_range(lines.len(), visible_height);
    let visible: Vec<Line> = range
        .map(|absolute| {
            let line = lines[absolute].clone();
            if absolute == scroll.selected {
                line.style(theme::selection_style())
            } else {
                line
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(visible), inner);
}

fn breakdown_lines(
    date: &str,
    day_tokens: i64,
    day_cost: f64,
    rows: &[PeriodDetailRow],
    narrow: bool,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format_day_title(date), theme::bold_style()),
            Span::raw("  "),
            Span::styled(
                stat_compact(day_tokens),
                theme::bold_fg_style(theme::metric_input()),
            ),
            Span::raw("  "),
            Span::styled(
                cost_compact(day_cost),
                theme::bold_fg_style(theme::positive_fg()),
            ),
        ]),
        Line::from(""),
    ];

    for group in grouped_models(rows) {
        let model_count = group.models.len();
        let plural = if model_count == 1 { "" } else { "s" };
        lines.push(Line::from(vec![
            Span::styled(
                format!("● {}", group.source),
                theme::bold_fg_style(theme::accent()),
            ),
            Span::styled(
                format!(" ({model_count} model{plural})"),
                theme::muted_style(),
            ),
            Span::raw("  "),
            Span::styled(
                cost_compact(group.cost),
                theme::bold_fg_style(theme::positive_fg()),
            ),
        ]));
        for model in group.models {
            let vendor = vendor_from_model(&model.model);
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("●", theme::fg_style(theme::vendor_fg(vendor, 0))),
                Span::styled(
                    format!(" {}", model.model),
                    theme::fg_style(theme::vendor_fg(vendor, 0)),
                ),
            ]));
            lines.push(channel_line(model, narrow));
        }
    }
    lines
}

fn channel_line(row: &PeriodDetailRow, narrow: bool) -> Line<'static> {
    let input = stat_compact(row.input_tokens);
    let output = stat_compact(row.output_tokens);
    let cache_read = stat_compact(row.cache_read_tokens);
    let cache_write = stat_compact(row.cache_creation_tokens);
    if narrow {
        Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!("{input}/{output}/{cache_read}/{cache_write}"),
                theme::muted_style(),
            ),
        ])
    } else {
        Line::from(vec![
            Span::raw("    "),
            Span::styled("In · ", theme::muted_style()),
            Span::styled(input, theme::fg_style(theme::metric_input())),
            Span::styled(" · Out · ", theme::muted_style()),
            Span::styled(output, theme::fg_style(theme::metric_output())),
            Span::styled(" · CR · ", theme::muted_style()),
            Span::styled(cache_read, theme::fg_style(theme::metric_cache_read())),
            Span::styled(" · CW · ", theme::muted_style()),
            Span::styled(cache_write, theme::fg_style(theme::metric_cache_write())),
        ])
    }
}

struct SourceGroup<'a> {
    source: String,
    cost: f64,
    models: Vec<&'a PeriodDetailRow>,
}

fn grouped_models(rows: &[PeriodDetailRow]) -> Vec<SourceGroup<'_>> {
    let mut by_source: HashMap<&str, Vec<&PeriodDetailRow>> = HashMap::new();
    for row in rows {
        by_source.entry(row.source.as_str()).or_default().push(row);
    }
    let mut groups: Vec<SourceGroup<'_>> = by_source
        .into_iter()
        .map(|(source, mut models)| {
            let cost = models.iter().map(|model| model.cost_with_cache_usd).sum();
            models.sort_by(|left, right| {
                right
                    .total_tokens
                    .cmp(&left.total_tokens)
                    .then_with(|| left.model.cmp(&right.model))
            });
            SourceGroup {
                source: source.to_string(),
                cost,
                models,
            }
        })
        .collect();
    groups.sort_by(|left, right| {
        right
            .cost
            .total_cmp(&left.cost)
            .then_with(|| left.source.cmp(&right.source))
    });
    groups
}

fn format_day_title(date: &str) -> String {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|parsed| parsed.format("%a, %b %d, %Y").to_string())
        .unwrap_or_else(|_| date.to_string())
}

fn context_pressure_line(pressure: &crate::query::ContextPressurePayload) -> Line<'static> {
    if pressure.priced_events == 0 {
        return labeled("Context", Span::styled("n/a", theme::muted_style()));
    }
    let peak_pct = pressure.peak_percent * 100.0;
    let avg_pct = pressure.avg_percent * 100.0;
    Line::from(vec![
        Span::styled("Context peak ", theme::muted_style()),
        Span::styled(
            format!("{peak_pct:.0}%"),
            theme::bold_fg_style(theme::bar_color(peak_pct)),
        ),
        Span::styled("  avg ", theme::muted_style()),
        Span::styled(
            format!("{avg_pct:.0}%"),
            theme::bold_fg_style(theme::accent()),
        ),
    ])
}

fn heat_legend_line() -> Line<'static> {
    Line::from(vec![
        Span::styled("Less ", theme::muted_style()),
        Span::styled("██", theme::fg_style(theme::heat(1))),
        Span::raw(" "),
        Span::styled("██", theme::fg_style(theme::heat(2))),
        Span::raw(" "),
        Span::styled("██", theme::fg_style(theme::heat(3))),
        Span::raw(" "),
        Span::styled("██", theme::fg_style(theme::heat(4))),
        Span::styled(" More", theme::muted_style()),
    ])
}

fn labeled(label: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label} "), theme::muted_style()),
        value,
    ])
}

fn put_line(frame: &mut Frame, x: u16, y: u16, width: u16, line: Line<'static>) {
    if width == 0 {
        return;
    }
    frame.render_widget(Paragraph::new(line), Rect::new(x, y, width, 1));
}

impl GraphLayout {
    fn from_area(area: Rect, heatmap: &[HeatmapPoint]) -> Option<Self> {
        let inner = theme::panel_block(GRAPH_TITLE).inner(area);
        if inner.width == 0 || inner.height == 0 || heatmap.is_empty() {
            return None;
        }
        let first_weekday = weekday_index(&heatmap[0].date)?;
        let label_width = if area.width < 80 { 2 } else { 4 };
        let visible_weeks = (inner.width.saturating_sub(label_width) / CELL_WIDTH) as usize;
        if visible_weeks == 0 {
            return None;
        }
        let total_weeks = first_weekday
            .saturating_add(heatmap.len())
            .div_ceil(7)
            .max(1);
        let visible_weeks = visible_weeks.min(total_weeks);
        Some(Self {
            start_x: inner.x.saturating_add(label_width),
            start_y: inner.y.saturating_add(2),
            label_width,
            first_weekday,
            start_week: total_weeks - visible_weeks,
            visible_weeks,
        })
    }

    fn hit(&self, heatmap: &[HeatmapPoint], x: u16, y: u16) -> Option<NaiveDate> {
        if y < self.start_y || x < self.start_x {
            return None;
        }
        let weekday = usize::from(y.saturating_sub(self.start_y));
        if weekday >= 7 {
            return None;
        }
        let week_vis = usize::from(x.saturating_sub(self.start_x) / CELL_WIDTH);
        if week_vis >= self.visible_weeks {
            return None;
        }
        let point = self.point_at(heatmap, self.start_week + week_vis, weekday)?;
        NaiveDate::parse_from_str(&point.date, "%Y-%m-%d").ok()
    }

    fn point_at<'a>(
        &self,
        heatmap: &'a [HeatmapPoint],
        week: usize,
        weekday: usize,
    ) -> Option<&'a HeatmapPoint> {
        let slot = week.checked_mul(7)?.checked_add(weekday)?;
        if slot < self.first_weekday {
            return None;
        }
        heatmap.get(slot - self.first_weekday)
    }

    fn cell_origin(&self, idx: usize) -> Option<(u16, u16)> {
        let slot = self.first_weekday.checked_add(idx)?;
        let week = slot / 7;
        if week < self.start_week {
            return None;
        }
        let vis = week - self.start_week;
        if vis >= self.visible_weeks {
            return None;
        }
        Some((
            self.start_x + vis as u16 * CELL_WIDTH,
            self.start_y + (slot % 7) as u16,
        ))
    }

    fn week_month(&self, heatmap: &[HeatmapPoint], week: usize) -> Option<u32> {
        (0..7).find_map(|weekday| {
            self.point_at(heatmap, week, weekday)
                .and_then(|point| NaiveDate::parse_from_str(&point.date, "%Y-%m-%d").ok())
                .map(|date| date.month())
        })
    }
}

/// Sunday-indexed weekday (0..=6) for a `YYYY-MM-DD` date, or `None` if unparseable.
fn weekday_index(date: &str) -> Option<usize> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .map(|date| date.weekday().num_days_from_sunday() as usize)
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

#[cfg(test)]
mod tests {
    use super::{
        GraphLayout, breakdown_scroll_total, contribution_bucket, contribution_thresholds,
        current_streak, day_at, longest_streak, split_stats_area, weekday_index,
    };
    use crate::query::{ContextPressurePayload, HeatmapPoint, OverviewPayload, PeriodDetailRow};
    use crate::tui::app::{
        PeriodDetailKind, PeriodDetailPayload, PeriodDetailState, ScrollState, StatsPanelPayload,
    };
    use chrono::NaiveDate;
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

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

    fn heatmap_from(start: NaiveDate, tokens: &[i64]) -> Vec<HeatmapPoint> {
        tokens
            .iter()
            .enumerate()
            .map(|(idx, &total_tokens)| HeatmapPoint {
                date: start
                    .checked_add_signed(chrono::Duration::days(idx as i64))
                    .expect("date")
                    .format("%Y-%m-%d")
                    .to_string(),
                event_count: i64::from(total_tokens > 0),
                total_tokens,
            })
            .collect()
    }

    fn empty_overview() -> OverviewPayload {
        OverviewPayload {
            generated_at: String::new(),
            total: crate::query::TokenSummary::default(),
            last_24h: crate::query::TokenSummary::default(),
            source_count: 0,
            bucket_count: 0,
            total_events: 0,
            last_24h_events: 0,
            total_cost_usd: 0.0,
            cache_efficiency: 0.0,
            last_sync_at: None,
            last_export_at: None,
        }
    }

    fn payload(heatmap: Vec<HeatmapPoint>) -> StatsPanelPayload {
        StatsPanelPayload {
            overview: empty_overview(),
            heatmap,
            models: Vec::new(),
            context_pressure: ContextPressurePayload {
                peak_percent: 0.0,
                avg_percent: 0.0,
                peak_model: None,
                priced_events: 0,
                unpriced_events: 0,
            },
        }
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

    #[test]
    fn graph_layout_aligns_thursday_start_to_sunday_week() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let heatmap = heatmap_from(start, &[100, 0, 200]);
        let area = Rect::new(0, 0, 40, 11);
        let layout = GraphLayout::from_area(area, &heatmap).unwrap();
        assert_eq!(layout.first_weekday, 4);
        assert!(layout.point_at(&heatmap, 0, 0).is_none());
        assert!(layout.point_at(&heatmap, 0, 3).is_none());
        assert_eq!(
            layout
                .point_at(&heatmap, 0, 4)
                .map(|point| point.date.as_str()),
            Some("2026-01-01")
        );
        let (x, y) = layout.cell_origin(0).unwrap();
        assert_eq!(day_at(area, &heatmap, x, y), Some(start));
        assert_eq!(day_at(area, &heatmap, x + 1, y), Some(start));
        assert_eq!(day_at(area, &heatmap, layout.start_x, layout.start_y), None);
    }

    #[test]
    fn graph_layout_clips_left_and_keeps_recent_weeks() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 4).unwrap();
        let tokens = vec![100; 56];
        let heatmap = heatmap_from(start, &tokens);
        // Inner width 8 with a 2-column label leaves three visible weeks.
        let area = Rect::new(0, 0, 10, 11);
        let layout = GraphLayout::from_area(area, &heatmap).unwrap();
        assert_eq!(layout.visible_weeks, 3);
        assert_eq!(layout.start_week, 5);
        assert!(layout.cell_origin(0).is_none());
        let first_visible = layout.start_week * 7;
        let (x, y) = layout.cell_origin(first_visible).unwrap();
        assert_eq!(
            day_at(area, &heatmap, x, y),
            Some(start + chrono::Duration::days(first_visible as i64))
        );
        assert!(day_at(area, &heatmap, layout.start_x.saturating_sub(1), y).is_none());
    }

    #[test]
    fn day_at_matches_painted_cells() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 4).unwrap();
        let heatmap = heatmap_from(start, &[0, 100, 200, 0, 400, 500, 0, 700]);
        let panel = Rect::new(0, 0, 40, 24);
        let graph = split_stats_area(panel, false).graph;
        let data = Some(Ok(payload(heatmap.clone())));
        let scroll = ScrollState {
            offset: 0,
            selected: 0,
            total: 0,
            visible: 8,
        };
        let mut terminal = Terminal::new(TestBackend::new(40, 24)).unwrap();
        terminal
            .draw(|frame| super::render(frame, panel, &data, &scroll, None))
            .unwrap();

        let layout = GraphLayout::from_area(graph, &heatmap).unwrap();
        let buffer = terminal.backend().buffer();
        for (idx, point) in heatmap.iter().enumerate() {
            let Some((x, y)) = layout.cell_origin(idx) else {
                continue;
            };
            let expected = NaiveDate::parse_from_str(&point.date, "%Y-%m-%d").ok();
            assert_eq!(day_at(graph, &heatmap, x, y), expected);
            assert_eq!(day_at(graph, &heatmap, x + 1, y), expected);
            let symbol = buffer[(x, y)].symbol();
            if point.total_tokens > 0 {
                assert_eq!(symbol, "█", "active cell at {x},{y}");
                assert_eq!(buffer[(x + 1, y)].symbol(), "█");
            } else {
                assert_eq!(symbol, "·", "empty cell at {x},{y}");
                assert_eq!(buffer[(x + 1, y)].symbol(), " ");
            }
        }
    }

    #[test]
    fn empty_heatmap_has_no_hit_target() {
        let area = Rect::new(0, 0, 40, 11);
        assert_eq!(day_at(area, &[], 8, 4), None);
        assert!(GraphLayout::from_area(area, &[]).is_none());
    }

    #[test]
    fn breakdown_scroll_total_counts_header_and_model_lines() {
        let rows = vec![
            PeriodDetailRow {
                model: "gpt-5".to_string(),
                source: "codex".to_string(),
                event_count: 2,
                input_tokens: 10,
                cache_read_tokens: 1,
                cache_creation_tokens: 2,
                output_tokens: 4,
                total_tokens: 17,
                cost_with_cache_usd: 0.4,
            },
            PeriodDetailRow {
                model: "opus".to_string(),
                source: "claude".to_string(),
                event_count: 1,
                input_tokens: 3,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 1,
                total_tokens: 4,
                cost_with_cache_usd: 0.1,
            },
        ];
        // header + blank + 2 source headers + 2 model lines + 2 channel lines
        assert_eq!(breakdown_scroll_total(&rows), 8);
        assert_eq!(breakdown_scroll_total(&[]), 0);
    }

    #[test]
    fn graph_layout_uses_wide_weekday_gutter_at_80() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 4).unwrap();
        let heatmap = heatmap_from(start, &[100]);
        let wide = GraphLayout::from_area(Rect::new(0, 0, 80, 11), &heatmap).unwrap();
        assert_eq!(wide.label_width, 4);
        let narrow = GraphLayout::from_area(Rect::new(0, 0, 79, 11), &heatmap).unwrap();
        assert_eq!(narrow.label_width, 2);
    }

    #[test]
    fn selected_short_layout_hides_stats_card() {
        let split = split_stats_area(Rect::new(0, 0, 80, 20), true);
        assert!(split.stats.is_none());
        assert!(split.breakdown.is_some());
        let tall = split_stats_area(Rect::new(0, 0, 80, 30), true);
        assert!(tall.stats.is_some());
        assert!(tall.breakdown.is_some());
        assert_eq!(tall.graph.height, 11);
    }

    #[test]
    fn empty_heatmap_render_keeps_graph_title() {
        let data = Some(Ok(payload(Vec::new())));
        let scroll = ScrollState {
            offset: 0,
            selected: 0,
            total: 0,
            visible: 8,
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| super::render(frame, frame.area(), &data, &scroll, None))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Contribution Graph (52 weeks)"), "{text}");
        assert!(text.contains("No heatmap data"), "{text}");
        assert!(!text.contains("Source Mix"), "{text}");
    }

    #[test]
    fn empty_day_breakdown_shows_no_data_copy() {
        let heatmap = heatmap_from(NaiveDate::from_ymd_opt(2026, 1, 4).unwrap(), &[0]);
        let data = Some(Ok(payload(heatmap)));
        let detail = PeriodDetailState {
            kind: PeriodDetailKind::Daily {
                date: "2026-01-04".to_string(),
            },
            list_scroll: ScrollState {
                offset: 0,
                selected: 0,
                total: 0,
                visible: 8,
            },
            payload: Some(Ok(PeriodDetailPayload::Daily(Vec::new()))),
        };
        let scroll = ScrollState {
            offset: 0,
            selected: 0,
            total: 0,
            visible: 8,
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
        terminal
            .draw(|frame| super::render(frame, frame.area(), &data, &scroll, Some(&detail)))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Day Breakdown"), "{text}");
        assert!(text.contains("No data for this day"), "{text}");
    }
}
