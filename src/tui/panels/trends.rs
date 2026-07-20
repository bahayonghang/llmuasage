use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use super::super::app::TimeWindow;
use crate::query::TrendPoint;
use crate::tui::{
    format::{axis_compact as format_compact, tokens as format_tokens},
    theme,
};

/// All time window variants in display order.
const ALL_WINDOWS: [TimeWindow; 4] = [
    TimeWindow::Day24h,
    TimeWindow::Week7d,
    TimeWindow::Month30d,
    TimeWindow::All,
];

/// Render the trends panel: window selector, summary cards, chart, and details.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<TrendPoint>, String>>,
    time_window: TimeWindow,
) {
    match data {
        None => {
            let widget = Paragraph::new("加载中...")
                .style(theme::muted_style())
                .block(theme::panel_block("趋势"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("数据加载失败: {e}"))
                .style(theme::error_style())
                .block(theme::panel_block("趋势"));
            frame.render_widget(widget, area);
        }
        Some(Ok(points)) => render_trends(frame, area, points, time_window),
    }
}

fn render_trends(frame: &mut Frame, area: Rect, points: &[TrendPoint], time_window: TimeWindow) {
    let block = theme::panel_block("趋势");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let summary_height = if inner.height >= 10 { 5 } else { 0 };
    let constraints: Vec<Constraint> = if summary_height > 0 {
        vec![
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(summary_height),
            Constraint::Length(1),
            Constraint::Min(0),
        ]
    } else {
        vec![
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ]
    };
    let chunks = Layout::vertical(constraints).split(inner);

    render_window_selector(frame, chunks[0], time_window);

    let body_area = if summary_height > 0 {
        render_summary(frame, chunks[2], points, time_window);
        chunks[4]
    } else {
        chunks[2]
    };

    if points.is_empty() {
        let placeholder = Paragraph::new("暂无趋势数据").style(theme::muted_style());
        frame.render_widget(placeholder, body_area);
        return;
    }

    if body_area.width >= 82 && body_area.height >= 8 {
        let [chart_area, detail_area] =
            Layout::horizontal([Constraint::Percentage(68), Constraint::Percentage(32)])
                .areas(body_area);
        render_chart(frame, chart_area, points, time_window);
        render_details(frame, detail_area, points, time_window);
    } else if body_area.height >= 12 {
        let [chart_area, detail_area] =
            Layout::vertical([Constraint::Min(7), Constraint::Length(5)]).areas(body_area);
        render_chart(frame, chart_area, points, time_window);
        render_details(frame, detail_area, points, time_window);
    } else {
        render_chart(frame, body_area, points, time_window);
    }
}

fn render_window_selector(frame: &mut Frame, area: Rect, active: TimeWindow) {
    let mut spans: Vec<Span> = vec![Span::styled("窗口: ", theme::muted_style())];

    for (i, window) in ALL_WINDOWS.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let label = format!(" {} ", window.label());
        let style = if *window == active {
            theme::nav_active_style()
        } else {
            theme::nav_inactive_style()
        };
        spans.push(Span::styled(label, style));
    }

    spans.push(Span::styled("  h/l 或 ←/→ 切换", theme::trend_aux_style()));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_summary(frame: &mut Frame, area: Rect, points: &[TrendPoint], time_window: TimeWindow) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let [total_area, peak_area, avg_area, active_area] = Layout::horizontal([
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
    ])
    .areas(area);

    let total: i64 = points.iter().map(|p| p.total_tokens.max(0)).sum();
    let peak = points.iter().max_by_key(|p| p.total_tokens);
    let peak_tokens = peak.map_or(0, |p| p.total_tokens.max(0));
    let peak_label = peak
        .map(|p| format_short_label(&p.label, time_window))
        .unwrap_or_else(|| "-".to_string());
    let active_count = points.iter().filter(|p| p.total_tokens > 0).count();
    let average = if points.is_empty() {
        0
    } else {
        total / points.len() as i64
    };

    render_card(
        frame,
        total_area,
        "总量",
        &format_tokens(total),
        "tokens",
        theme::kpi_colors()[0],
    );
    render_card(
        frame,
        peak_area,
        "峰值",
        &format_tokens(peak_tokens),
        &peak_label,
        theme::trend_peak_fg(),
    );
    render_card(
        frame,
        avg_area,
        match time_window {
            TimeWindow::Day24h => "桶均",
            _ => "日均",
        },
        &format_tokens(average),
        "tokens",
        theme::kpi_colors()[2],
    );
    render_card(
        frame,
        active_area,
        match time_window {
            TimeWindow::Day24h => "活跃桶",
            TimeWindow::All => "活跃月",
            _ => "活跃天",
        },
        &active_count.to_string(),
        &format!("共 {}", points.len()),
        theme::kpi_colors()[1],
    );
}

fn render_card(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    value: &str,
    subtitle: &str,
    color: ratatui::style::Color,
) {
    let lines = vec![
        Line::from(Span::styled(
            value.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(subtitle.to_string(), theme::trend_aux_style())),
    ];
    let card = Paragraph::new(lines).block(theme::trend_card_block(title, color));
    frame.render_widget(card, area);
}

fn render_chart(frame: &mut Frame, area: Rect, points: &[TrendPoint], time_window: TimeWindow) {
    let block = theme::panel_block("趋势图");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if points.is_empty() {
        frame.render_widget(
            Paragraph::new("暂无趋势数据").style(theme::muted_style()),
            inner,
        );
        return;
    }
    if inner.height < 3 {
        render_compact_series(frame, inner, points, time_window);
        return;
    }

    let max_value = points
        .iter()
        .map(|p| p.total_tokens.max(0) as u64)
        .max()
        .unwrap_or(0);
    let total: i64 = points.iter().map(|p| p.total_tokens.max(0)).sum();
    let header = format!(
        "Tokens ↑  max {}  total {}",
        format_tokens(max_value as i64),
        format_tokens(total)
    );
    frame.buffer_mut().set_stringn(
        inner.x,
        inner.y,
        header,
        inner.width as usize,
        theme::trend_aux_style(),
    );

    let label_y = inner.y + inner.height - 1;
    let baseline_y = label_y.saturating_sub(1);
    let plot_top = inner.y + 1;
    if baseline_y < plot_top {
        return;
    }
    let plot_height = baseline_y - plot_top + 1;

    for x in inner.x..inner.x + inner.width {
        frame.buffer_mut()[(x, baseline_y)]
            .set_symbol("─")
            .set_style(theme::trend_aux_style());
    }

    let bars = bar_layout(inner.x, inner.width, points.len());
    let label_stride = label_stride(points, time_window, inner.width);
    let peak_index = points
        .iter()
        .enumerate()
        .max_by_key(|(_, p)| p.total_tokens)
        .map(|(idx, _)| idx);

    for (idx, (point, (bar_x, bar_width))) in points.iter().zip(bars.iter()).enumerate() {
        let value = point.total_tokens.max(0) as u64;
        let height = scaled_height(value, max_value, plot_height);
        let is_peak = Some(idx) == peak_index && value > 0;
        let style = if is_peak {
            theme::trend_peak_style()
        } else if value == 0 {
            theme::trend_aux_style()
        } else {
            theme::trend_bar_style()
        };

        if height > 0 {
            let first_y = baseline_y.saturating_sub(height - 1);
            for y in first_y..=baseline_y {
                for dx in 0..*bar_width {
                    let x = bar_x.saturating_add(dx);
                    if x < inner.x + inner.width {
                        frame.buffer_mut()[(x, y)].set_symbol("█").set_style(style);
                    }
                }
            }
        }

        if *bar_width >= 2 && height > 0 {
            let text = format_compact(value as i64);
            draw_centered(
                frame,
                *bar_x,
                *bar_width,
                first_value_y(baseline_y, height, plot_top),
                &text,
                if is_peak {
                    theme::trend_peak_style()
                } else {
                    theme::trend_aux_style()
                },
            );
        }

        if should_label(idx, points.len(), label_stride, peak_index) {
            let label = format_short_label(&point.label, time_window);
            draw_label(frame, inner, label_y, *bar_x, *bar_width, &label);
        }
    }
}

fn render_compact_series(
    frame: &mut Frame,
    area: Rect,
    points: &[TrendPoint],
    time_window: TimeWindow,
) {
    let visible = points.iter().take(area.width as usize).enumerate();
    for (i, point) in visible {
        let label = format_short_label(&point.label, time_window);
        let text = if point.total_tokens > 0 { "▁" } else { "·" };
        frame.buffer_mut()[(area.x + i as u16, area.y)]
            .set_symbol(text)
            .set_style(if point.total_tokens > 0 {
                theme::trend_bar_style()
            } else {
                theme::trend_aux_style()
            });
        if area.height > 1 && i == 0 {
            frame.buffer_mut().set_stringn(
                area.x,
                area.y + 1,
                label,
                area.width as usize,
                theme::trend_aux_style(),
            );
        }
    }
}

fn render_details(frame: &mut Frame, area: Rect, points: &[TrendPoint], time_window: TimeWindow) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let capacity = area.height.saturating_sub(3).max(1) as usize;
    let rows = points.iter().rev().take(capacity).map(|point| {
        Row::new(vec![
            Cell::from(format_short_label(&point.label, time_window)),
            Cell::from(format_tokens(point.total_tokens)),
        ])
    });

    let header = Row::new(vec![Cell::from("最近"), Cell::from("Tokens")])
        .style(theme::header_style())
        .bottom_margin(1);
    let table = Table::new(
        rows,
        [Constraint::Percentage(45), Constraint::Percentage(55)],
    )
    .header(header)
    .block(theme::panel_block("最近明细"));
    frame.render_widget(table, area);
}

fn bar_layout(x: u16, width: u16, count: usize) -> Vec<(u16, u16)> {
    if count == 0 || width == 0 {
        return Vec::new();
    }
    if count == 1 {
        let bar_width = width.clamp(1, 5);
        return vec![(x + (width.saturating_sub(bar_width)) / 2, bar_width)];
    }
    if count <= width as usize {
        let slot_width = (width / count as u16).max(1);
        let used_width = slot_width.saturating_mul(count as u16).min(width);
        let left_pad = width.saturating_sub(used_width) / 2;
        let bar_width = match slot_width {
            0..=2 => 1,
            3..=5 => 2,
            _ => 3,
        };
        return (0..count)
            .map(|idx| {
                let slot_x = x + left_pad + idx as u16 * slot_width;
                let centered = slot_x + slot_width.saturating_sub(bar_width) / 2;
                (centered, bar_width.min(slot_width))
            })
            .collect();
    }

    (0..count)
        .map(|idx| {
            let offset = ((idx as f64 / (count - 1) as f64) * (width - 1) as f64).round() as u16;
            (x + offset, 1)
        })
        .collect()
}

fn label_stride(points: &[TrendPoint], time_window: TimeWindow, width: u16) -> usize {
    if points.is_empty() || width == 0 {
        return 1;
    }
    let label_width = points
        .iter()
        .map(|p| format_short_label(&p.label, time_window).chars().count())
        .max()
        .unwrap_or(1)
        .max(1);
    ((points.len() * (label_width + 1)) as f64 / width as f64)
        .ceil()
        .max(1.0) as usize
}

fn should_label(idx: usize, len: usize, stride: usize, peak_index: Option<usize>) -> bool {
    idx == 0 || idx + 1 == len || Some(idx) == peak_index || idx.is_multiple_of(stride)
}

fn draw_label(frame: &mut Frame, area: Rect, y: u16, bar_x: u16, bar_width: u16, label: &str) {
    let label_width = label.chars().count() as u16;
    if label_width == 0 {
        return;
    }
    let bar_mid = bar_x + bar_width / 2;
    let start = bar_mid
        .saturating_sub(label_width / 2)
        .clamp(area.x, area.x + area.width.saturating_sub(1));
    let available = (area.x + area.width).saturating_sub(start) as usize;
    frame
        .buffer_mut()
        .set_stringn(start, y, label, available, theme::trend_aux_style());
}

fn draw_centered(frame: &mut Frame, x: u16, width: u16, y: u16, text: &str, style: Style) {
    let text_width = text.chars().count() as u16;
    if text_width == 0 || width < text_width {
        return;
    }
    let start = x + width.saturating_sub(text_width) / 2;
    frame
        .buffer_mut()
        .set_stringn(start, y, text, width as usize, style);
}

fn first_value_y(baseline_y: u16, height: u16, plot_top: u16) -> u16 {
    baseline_y
        .saturating_sub(height)
        .saturating_sub(1)
        .max(plot_top)
}

fn scaled_height(value: u64, max_value: u64, plot_height: u16) -> u16 {
    if value == 0 || max_value == 0 || plot_height == 0 {
        return 0;
    }
    (((value as f64 / max_value as f64) * plot_height as f64).ceil() as u16).clamp(1, plot_height)
}

fn format_short_label(label: &str, time_window: TimeWindow) -> String {
    match time_window {
        TimeWindow::Day24h => parse_datetime(label)
            .map(|dt| dt.format("%H:%M").to_string())
            .or_else(|| parse_hh_mm(label))
            .unwrap_or_else(|| truncate_chars(label, 5)),
        TimeWindow::Week7d | TimeWindow::Month30d => parse_date(label)
            .map(|date| date.format("%m-%d").to_string())
            .unwrap_or_else(|| {
                if label.len() >= 10 && label.as_bytes().get(4) == Some(&b'-') {
                    label.chars().skip(5).take(5).collect()
                } else {
                    truncate_chars(label, 5)
                }
            }),
        TimeWindow::All => parse_date(label)
            .map(|date| date.format("%Y-%m").to_string())
            .unwrap_or_else(|| {
                if label.len() >= 7 {
                    label.chars().take(7).collect()
                } else {
                    label.to_string()
                }
            }),
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

fn parse_date(label: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(label, "%Y-%m-%d")
        .ok()
        .or_else(|| parse_datetime(label).map(|dt| dt.date_naive()))
        .or_else(|| {
            let month = format!("{label}-01");
            NaiveDate::parse_from_str(&month, "%Y-%m-%d").ok()
        })
}

fn parse_hh_mm(label: &str) -> Option<String> {
    if label.len() >= 5 {
        let candidate: String = label.chars().take(5).collect();
        let bytes = candidate.as_bytes();
        if bytes.len() == 5
            && bytes[0].is_ascii_digit()
            && bytes[1].is_ascii_digit()
            && bytes[2] == b':'
            && bytes[3].is_ascii_digit()
            && bytes[4].is_ascii_digit()
        {
            return Some(candidate);
        }
    }
    None
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}
