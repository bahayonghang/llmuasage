use chrono::Local;
use ratatui::{
    Frame,
    layout::Rect,
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::query::HourlyTrendPoint;
use crate::tui::app::{ScrollState, SortState, TableSortKey, stable_sort_refs};
use crate::tui::theme;

use super::period::{
    PeriodValues, WidthTier, period_has_turns, period_header_row, period_metric_cells,
    period_widths, width_tier,
};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<HourlyTrendPoint>, String>>,
    scroll: &ScrollState,
) {
    render_sorted(frame, area, data, scroll, SortState::date_desc());
}

pub(crate) fn render_sorted(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<HourlyTrendPoint>, String>>,
    scroll: &ScrollState,
    sort: SortState,
) {
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(styled_block("Hourly Usage"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(styled_block("Hourly Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("No hourly usage data found. Press r to refresh.")
                .style(theme::muted_style())
                .block(styled_block("Hourly Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, items, scroll, sort),
    }
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    items: &[HourlyTrendPoint],
    scroll: &ScrollState,
    sort: SortState,
) {
    let block = styled_block("Hourly Usage");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let tier = width_tier(inner.width);
    let has_turns = period_has_turns(items.iter().map(|hour| hour.turn_count));
    let header = period_header_row("Hour", &["Source"], has_turns, tier, sort);
    let ordered = stable_sort_refs(items.iter().collect(), sort, |left, right, key| match key {
        TableSortKey::Date => left.hour_start.cmp(&right.hour_start),
        TableSortKey::Tokens => left.total_tokens.cmp(&right.total_tokens),
        TableSortKey::Cost => left
            .cost_with_cache_usd
            .total_cmp(&right.cost_with_cache_usd),
    });
    let visible_height = super::visible_table_rows(area);
    let rows = hourly_visible_rows(&ordered, scroll, visible_height, tier, has_turns);

    let extra = if matches!(tier, WidthTier::VeryNarrow) {
        0
    } else {
        1
    };
    let table = Table::new(rows, period_widths(extra, has_turns, tier, 7)).header(header);
    frame.render_widget(table, inner);
}

fn hourly_visible_rows(
    ordered: &[&HourlyTrendPoint],
    scroll: &ScrollState,
    budget: usize,
    tier: WidthTier,
    has_turns: bool,
) -> Vec<Row<'static>> {
    if ordered.is_empty() || budget == 0 {
        return Vec::new();
    }
    let selected = scroll.selected.min(ordered.len() - 1);
    let start = scroll.visible_range(ordered.len(), budget).start;
    let (rows, painted) =
        paint_hourly_window(ordered, start, budget, selected, tier, has_turns, false);
    if painted.contains(&selected) {
        return rows;
    }
    paint_hourly_window(ordered, selected, budget, selected, tier, has_turns, true).0
}

fn paint_hourly_window(
    ordered: &[&HourlyTrendPoint],
    start: usize,
    budget: usize,
    selected: usize,
    tier: WidthTier,
    has_turns: bool,
    skip_separators: bool,
) -> (Vec<Row<'static>>, Vec<usize>) {
    let now_hour = Local::now().format("%Y-%m-%d %H:00").to_string();
    let sep_style = theme::bold_fg_style(theme::accent());
    let mut rows = Vec::new();
    let mut painted = Vec::new();
    let mut prev_date: Option<&str> = None;
    let mut lines_used = 0usize;
    let mut data_idx = start.min(ordered.len());

    while data_idx < ordered.len() && lines_used < budget {
        let hour = ordered[data_idx];
        let row_date = hour_date(&hour.hour_start);
        let needs_sep = !skip_separators && prev_date != Some(row_date);
        if needs_sep && lines_used + 1 < budget {
            rows.push(Row::new(vec![
                Cell::from(md_label(row_date)).style(sep_style),
            ]));
            lines_used += 1;
        }
        prev_date = Some(row_date);

        let is_current = hour.hour_start == now_hour;
        let time_style = if is_current {
            theme::bold_fg_style(theme::warning_fg())
        } else {
            theme::bold_style()
        };
        let extra = if matches!(tier, WidthTier::VeryNarrow) {
            Vec::new()
        } else {
            vec![Cell::from(hour.sources.join(", ")).style(theme::muted_style())]
        };
        let mut cells = vec![Cell::from(hour_label(&hour.hour_start)).style(time_style)];
        cells.extend(period_metric_cells(
            &PeriodValues {
                turn_count: hour.turn_count,
                event_count: hour.event_count,
                input_tokens: hour.input_tokens,
                output_tokens: hour.output_tokens,
                cache_read_tokens: hour.cache_read_tokens,
                cache_creation_tokens: hour.cache_creation_tokens,
                total_tokens: hour.total_tokens,
                cost_with_cache_usd: hour.cost_with_cache_usd,
            },
            extra,
            has_turns,
            tier,
        ));
        let mut row = Row::new(cells);
        if data_idx == selected {
            row = row.style(theme::selection_style());
        } else if data_idx % 2 == 1 {
            row = row.style(theme::row_alt_style());
        }
        rows.push(row);
        painted.push(data_idx);
        lines_used += 1;
        data_idx += 1;
    }
    (rows, painted)
}

fn hour_date(hour_start: &str) -> &str {
    hour_start.get(..10).unwrap_or(hour_start)
}

fn hour_label(hour_start: &str) -> String {
    hour_start
        .get(11..)
        .unwrap_or(hour_start)
        .chars()
        .take(5)
        .collect()
}

fn md_label(date: &str) -> String {
    if date.len() >= 10 {
        format!("{}/{}", &date[5..7], &date[8..10])
    } else {
        date.to_string()
    }
}

fn styled_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme::block_border_style())
        .title(Span::styled(
            format!(" {title} "),
            theme::block_title_style(),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn hour(start: &str, tokens: i64, sources: &[&str]) -> HourlyTrendPoint {
        HourlyTrendPoint {
            hour_start: start.to_string(),
            input_tokens: tokens,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 0,
            total_tokens: tokens,
            event_count: 1,
            turn_count: 2,
            cost_with_cache_usd: 1.0,
            sources: sources.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn compact_time_with_day_separators() {
        let data = Some(Ok(vec![
            hour("2026-05-29 14:00", 10, &["claude"]),
            hour("2026-05-29 13:00", 10, &["claude"]),
            hour("2026-05-28 23:00", 10, &["claude"]),
        ]));
        let scroll = ScrollState {
            offset: 0,
            selected: 0,
            total: 3,
            visible: 10,
        };
        let mut terminal = Terminal::new(TestBackend::new(130, 16)).unwrap();
        terminal
            .draw(|frame| {
                render_sorted(frame, frame.area(), &data, &scroll, SortState::date_desc())
            })
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("14:00"), "{text}");
        assert!(text.contains("05/29"), "{text}");
        assert!(text.contains("05/28"), "{text}");
        assert!(!text.contains("Share"), "{text}");
        assert!(!text.contains("Profile"), "{text}");
        assert!(text.contains("Cache×"), "{text}");
        assert!(text.contains("Source"), "{text}");
    }

    #[test]
    fn tight_viewport_keeps_selected_hour_visible() {
        let data = Some(Ok(vec![
            hour("2026-05-29 14:00", 10, &["claude"]),
            hour("2026-05-28 23:00", 10, &["claude"]),
        ]));
        let scroll = ScrollState {
            offset: 0,
            selected: 1,
            total: 2,
            visible: 1,
        };
        let mut terminal = Terminal::new(TestBackend::new(120, 6)).unwrap();
        terminal
            .draw(|frame| {
                render_sorted(frame, frame.area(), &data, &scroll, SortState::date_desc())
            })
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            text.contains("23:00"),
            "selected hour must stay visible in a one-line viewport\n{text}"
        );
    }
}
