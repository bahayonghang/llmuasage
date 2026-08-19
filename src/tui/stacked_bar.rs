//! Stacked daily bar chart for the Overview panel.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
};

use super::{format::stat_compact, theme};

const BLOCKS: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const MONTH_NAMES: &[&str] = &[
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// One model's contribution to a bar.
#[derive(Debug, Clone)]
pub struct StackedBarSegment {
    pub tokens: i64,
    pub color: Color,
}

/// One calendar-day bar.
#[derive(Debug, Clone)]
pub struct StackedBarData {
    pub date: String,
    pub total: i64,
    pub segments: Vec<StackedBarSegment>,
}

/// Render a stacked bar chart. `title` is written on the first row.
pub fn render_stacked_bar_chart(
    frame: &mut Frame,
    area: Rect,
    data: &[StackedBarData],
    title: &str,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let very_narrow = area.width < 60;
    let y_label_width: u16 = if very_narrow { 6 } else { 7 };
    let chart_width = area.width.saturating_sub(y_label_width) as usize;
    let chart_height = area.height.saturating_sub(3) as usize;
    if chart_width == 0 || chart_height == 0 {
        write_styled(frame, area.x, area.y, title, theme::bold_style());
        return;
    }

    write_styled(
        frame,
        area.x + y_label_width,
        area.y,
        title,
        theme::bold_style(),
    );

    if data.is_empty() {
        return;
    }

    let max_value = data
        .iter()
        .map(|row| row.total.max(0) as f64)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let muted = theme::muted_fg();
    let highlight = theme::surface_fg();
    let bar_count = data.len();

    for row_from_bottom in (0..chart_height).rev() {
        let row_index = chart_height - 1 - row_from_bottom;
        let y = area.y + 1 + row_index as u16;
        let y_label = if row_from_bottom + 1 == chart_height {
            stat_compact(max_value as i64)
        } else {
            String::new()
        };
        write_y_gutter(frame, area.x, y, y_label_width, &y_label, muted);

        let row_threshold = ((row_from_bottom + 1) as f64 / chart_height as f64) * max_value;
        let prev_threshold = (row_from_bottom as f64 / chart_height as f64) * max_value;
        let threshold_diff = row_threshold - prev_threshold;
        let mut x_pos = area.x + y_label_width;
        for (bar_index, bar_data) in data.iter().enumerate() {
            let bar_width = bar_pixel_width(bar_index, bar_count, chart_width);
            let (ch, fg_color) = stacked_cell(
                bar_data,
                row_threshold,
                prev_threshold,
                threshold_diff,
                muted,
                highlight,
            );
            for _ in 0..bar_width {
                if x_pos < area.x + area.width {
                    let cell = &mut frame.buffer_mut()[(x_pos, y)];
                    cell.set_char(ch);
                    if theme::color_mode() != theme::TerminalColorMode::NoColor {
                        cell.set_fg(fg_color);
                    }
                    x_pos += 1;
                }
            }
        }
    }

    let axis_y = area.y + 1 + chart_height as u16;
    if axis_y < area.y + area.height {
        write_y_gutter(frame, area.x, axis_y, y_label_width, "0", muted);
        for x in (area.x + y_label_width)..(area.x + area.width) {
            let cell = &mut frame.buffer_mut()[(x, axis_y)];
            cell.set_char('─');
            if theme::color_mode() != theme::TerminalColorMode::NoColor {
                cell.set_fg(muted);
            }
        }
    }

    let label_y = axis_y.saturating_add(1);
    if label_y < area.y + area.height {
        let num_labels = if very_narrow { 2 } else { 3 };
        let label_interval = (bar_count / num_labels).max(1);
        for (index, bar) in data.iter().enumerate() {
            if index % label_interval != 0 {
                continue;
            }
            let bar_start = (index * chart_width) / bar_count;
            let label_x = area.x + y_label_width + bar_start as u16;
            write_styled(
                frame,
                label_x,
                label_y,
                &format_axis_date(&bar.date, very_narrow),
                theme::muted_style(),
            );
        }
    }
}

fn bar_pixel_width(index: usize, bar_count: usize, chart_width: usize) -> usize {
    if bar_count == 0 {
        return 1;
    }
    let start = (index * chart_width) / bar_count;
    let end = ((index + 1) * chart_width) / bar_count;
    (end - start).max(1)
}

fn stacked_cell(
    bar_data: &StackedBarData,
    row_threshold: f64,
    prev_threshold: f64,
    threshold_diff: f64,
    muted: Color,
    fallback: Color,
) -> (char, Color) {
    let total = bar_data.total.max(0) as f64;
    if total <= prev_threshold {
        return (' ', muted);
    }
    if bar_data.segments.is_empty() {
        return (' ', muted);
    }

    let row_start = prev_threshold;
    let row_end = row_threshold;
    let mut current_height = 0.0;
    let mut max_overlap = 0.0;
    let mut best_color = bar_data
        .segments
        .first()
        .map(|segment| segment.color)
        .unwrap_or(fallback);

    for segment in &bar_data.segments {
        let tokens = segment.tokens.max(0) as f64;
        let m_start = current_height;
        let m_end = current_height + tokens;
        current_height += tokens;
        let overlap = (m_end.min(row_end) - m_start.max(row_start)).max(0.0);
        if overlap > max_overlap {
            max_overlap = overlap;
            best_color = segment.color;
        }
    }

    if total >= row_threshold {
        return (BLOCKS[8], best_color);
    }
    let ratio = if threshold_diff > 0.0 {
        (total - prev_threshold) / threshold_diff
    } else {
        1.0
    };
    let block_index = (ratio * 8.0).floor().clamp(1.0, 8.0) as usize;
    (BLOCKS[block_index], best_color)
}

fn write_y_gutter(frame: &mut Frame, x: u16, y: u16, width: u16, label: &str, color: Color) {
    let padded = format!("{label:>width$}│", width = width.saturating_sub(1) as usize);
    write_styled(frame, x, y, &padded, theme::fg_style(color));
}

fn write_styled(frame: &mut Frame, x: u16, y: u16, text: &str, style: Style) {
    let buf = frame.buffer_mut();
    let max_x = buf.area.x + buf.area.width;
    if y >= buf.area.y + buf.area.height {
        return;
    }
    for (index, ch) in text.chars().enumerate() {
        let cell_x = x + index as u16;
        if cell_x >= max_x {
            break;
        }
        buf[(cell_x, y)].set_char(ch).set_style(style);
    }
}

fn format_axis_date(date: &str, very_narrow: bool) -> String {
    let Some((year_month, day)) = date.rsplit_once('-') else {
        return date.to_string();
    };
    let Some((_, month)) = year_month.rsplit_once('-') else {
        return date.to_string();
    };
    let Ok(month_num) = month.parse::<usize>() else {
        return date.to_string();
    };
    if !(1..=12).contains(&month_num) {
        return date.to_string();
    }
    if very_narrow {
        format!("{month_num}/{day}")
    } else {
        format!("{} {day}", MONTH_NAMES[month_num - 1])
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    use super::*;

    #[test]
    fn empty_chart_still_writes_title() {
        let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
        terminal
            .draw(|frame| {
                render_stacked_bar_chart(frame, Rect::new(0, 0, 40, 10), &[], "Tokens per Day");
            })
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Tokens per Day"), "{text}");
    }

    #[test]
    fn single_bar_writes_axis_and_date() {
        let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
        let data = [StackedBarData {
            date: "2026-07-18".to_string(),
            total: 1_000,
            segments: vec![StackedBarSegment {
                tokens: 1_000,
                color: theme::muted_fg(),
            }],
        }];
        terminal
            .draw(|frame| {
                render_stacked_bar_chart(frame, Rect::new(0, 0, 40, 10), &data, "Tokens per Day");
            })
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Tokens per Day"), "{text}");
        assert!(text.contains('0'), "{text}");
        assert!(text.contains("Jul 18") || text.contains("7/18"), "{text}");
    }
}
