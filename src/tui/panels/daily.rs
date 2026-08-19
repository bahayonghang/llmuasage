use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::query::{DailyTrendPoint, PeriodDetailRow};
use crate::tui::app::{
    PeriodDetailPayload, PeriodDetailState, ScrollState, SortState, TableSortKey, stable_sort_refs,
};
use crate::tui::theme;

use super::period::{
    PeriodValues, WidthTier, display_date, period_has_turns, period_header_row,
    period_metric_cells, period_widths, width_tier,
};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<DailyTrendPoint>, String>>,
    scroll: &ScrollState,
) {
    render_sorted(frame, area, data, scroll, SortState::date_desc(), None);
}

pub(crate) fn render_sorted(
    frame: &mut Frame,
    area: Rect,
    data: &Option<Result<Vec<DailyTrendPoint>, String>>,
    scroll: &ScrollState,
    sort: SortState,
    detail: Option<&PeriodDetailState>,
) {
    if let Some(detail) = detail {
        render_detail(frame, area, detail, scroll, sort);
        return;
    }
    match data {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(styled_block("Daily Usage"));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(styled_block("Daily Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) if items.is_empty() => {
            let widget = Paragraph::new("No daily usage data found. Press r to refresh.")
                .style(theme::muted_style())
                .block(styled_block("Daily Usage"));
            frame.render_widget(widget, area);
        }
        Some(Ok(items)) => render_table(frame, area, "Daily Usage", items, scroll, sort),
    }
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: &[DailyTrendPoint],
    scroll: &ScrollState,
    sort: SortState,
) {
    let block = styled_block(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let tier = width_tier(inner.width);
    let has_turns = period_has_turns(items.iter().map(|day| day.turn_count));
    let compact_date = matches!(tier, WidthTier::Wide) && inner.width < 112;
    let date_width = if compact_date { 7 } else { 12 };
    let header = period_header_row("Date", &[], has_turns, tier, sort);
    let ordered = stable_sort_refs(items.iter().collect(), sort, compare_daily);
    let visible_height = super::visible_table_rows(area);
    let range = scroll.visible_range(ordered.len(), visible_height);
    let rows = range.enumerate().map(|(visible_index, absolute)| {
        let day = ordered[absolute];
        let mut cells = vec![
            Cell::from(display_date(
                &day.date,
                compact_date || !matches!(tier, WidthTier::Wide),
            ))
            .style(theme::bold_style()),
        ];
        cells.extend(period_metric_cells(
            &values_from_daily(day),
            Vec::new(),
            has_turns,
            tier,
        ));
        let row = Row::new(cells);
        if absolute == scroll.selected {
            row.style(theme::selection_style())
        } else if visible_index % 2 == 1 {
            row.style(theme::row_alt_style())
        } else {
            row
        }
    });
    let table = Table::new(rows, period_widths(0, has_turns, tier, date_width)).header(header);
    frame.render_widget(table, inner);
}

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    detail: &PeriodDetailState,
    scroll: &ScrollState,
    sort: SortState,
) {
    let title = match &detail.kind {
        crate::tui::app::PeriodDetailKind::Daily { date } => format!("Daily Detail: {date}"),
        crate::tui::app::PeriodDetailKind::Monthly { month } => format!("Daily Breakdown: {month}"),
    };
    match &detail.payload {
        None => {
            let widget = Paragraph::new("Loading...")
                .style(theme::muted_style())
                .block(styled_block(&title));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(format!("Data load failed: {e}"))
                .style(theme::error_style())
                .block(styled_block(&title));
            frame.render_widget(widget, area);
        }
        Some(Ok(PeriodDetailPayload::Daily(rows))) => {
            render_model_detail(frame, area, &title, rows, scroll, sort);
        }
        Some(Ok(PeriodDetailPayload::Monthly(days))) => {
            render_table(frame, area, &title, days, scroll, sort);
        }
    }
}

fn render_model_detail(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: &[PeriodDetailRow],
    scroll: &ScrollState,
    sort: SortState,
) {
    let block = styled_block(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if items.is_empty() {
        let widget = Paragraph::new("No model details found for this day. Press Esc to go back.")
            .style(theme::muted_style());
        frame.render_widget(widget, inner);
        return;
    }

    let tier = width_tier(inner.width);
    let header = match tier {
        WidthTier::VeryNarrow => Row::new(vec![
            Cell::from("Model"),
            Cell::from(sort.header("Cost", TableSortKey::Cost)),
        ]),
        WidthTier::Narrow => Row::new(vec![
            Cell::from("Model"),
            Cell::from("Source"),
            Cell::from("Msgs"),
            Cell::from(sort.header("Tokens", TableSortKey::Tokens)),
            Cell::from(sort.header("Cost", TableSortKey::Cost)),
        ]),
        WidthTier::Wide => Row::new(vec![
            Cell::from("Model"),
            Cell::from("Source"),
            Cell::from("Msgs"),
            Cell::from("Input"),
            Cell::from("Output"),
            Cell::from("Cache R"),
            Cell::from("Cache W"),
            Cell::from("Cache×"),
            Cell::from(sort.header("Total", TableSortKey::Tokens)),
            Cell::from(sort.header("Cost", TableSortKey::Cost)),
            Cell::from("Cost/1M"),
        ]),
    }
    .style(theme::header_style())
    .bottom_margin(1);

    let ordered = stable_sort_refs(items.iter().collect(), sort, |left, right, key| match key {
        TableSortKey::Tokens => left.total_tokens.cmp(&right.total_tokens),
        TableSortKey::Cost => left
            .cost_with_cache_usd
            .total_cmp(&right.cost_with_cache_usd),
        TableSortKey::Date => left.model.cmp(&right.model),
    });
    let visible_height = super::visible_table_rows(area);
    let range = scroll.visible_range(ordered.len(), visible_height);
    let rows = range.enumerate().map(|(visible_index, absolute)| {
        let item = ordered[absolute];
        let values = PeriodValues {
            turn_count: 0,
            event_count: item.event_count,
            input_tokens: item.input_tokens,
            output_tokens: item.output_tokens,
            cache_read_tokens: item.cache_read_tokens,
            cache_creation_tokens: item.cache_creation_tokens,
            total_tokens: item.total_tokens,
            cost_with_cache_usd: item.cost_with_cache_usd,
        };
        let mut cells = vec![Cell::from(item.model.clone()).style(theme::bold_style())];
        if !matches!(tier, WidthTier::VeryNarrow) {
            cells.push(Cell::from(item.source.clone()).style(theme::muted_style()));
        }
        cells.extend(period_metric_cells(&values, Vec::new(), false, tier));
        let row = Row::new(cells);
        if absolute == scroll.selected {
            row.style(theme::selection_style())
        } else if visible_index % 2 == 1 {
            row.style(theme::row_alt_style())
        } else {
            row
        }
    });
    let widths = match tier {
        WidthTier::VeryNarrow => vec![Constraint::Percentage(70), Constraint::Percentage(30)],
        WidthTier::Narrow => vec![
            Constraint::Percentage(36),
            Constraint::Percentage(18),
            Constraint::Percentage(14),
            Constraint::Percentage(16),
            Constraint::Percentage(16),
        ],
        WidthTier::Wide => {
            let mut widths = vec![Constraint::Min(16), Constraint::Length(14)];
            widths.extend(period_widths(0, false, tier, 12).into_iter().skip(1));
            widths
        }
    };
    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);
}

fn compare_daily(
    left: &DailyTrendPoint,
    right: &DailyTrendPoint,
    key: TableSortKey,
) -> std::cmp::Ordering {
    match key {
        TableSortKey::Date => left.date.cmp(&right.date),
        TableSortKey::Tokens => left.total_tokens.cmp(&right.total_tokens),
        TableSortKey::Cost => left
            .cost_with_cache_usd
            .total_cmp(&right.cost_with_cache_usd),
    }
}

fn values_from_daily(day: &DailyTrendPoint) -> PeriodValues {
    PeriodValues {
        turn_count: day.turn_count,
        event_count: day.event_count,
        input_tokens: day.input_tokens,
        output_tokens: day.output_tokens,
        cache_read_tokens: day.cache_read_tokens,
        cache_creation_tokens: day.cache_creation_tokens,
        total_tokens: day.total_tokens,
        cost_with_cache_usd: day.cost_with_cache_usd,
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

    fn day(date: &str, tokens: i64, input: i64, cost: f64) -> DailyTrendPoint {
        DailyTrendPoint {
            date: date.to_string(),
            input_tokens: input,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: tokens - input,
            total_tokens: tokens,
            event_count: 1,
            cost_with_cache_usd: cost,
            turn_count: 3,
        }
    }

    #[test]
    fn wide_table_uses_tokscale_columns() {
        let data = Some(Ok(vec![
            day("2026-07-19", 100, 11, 0.10),
            day("2026-07-20", 900, 77, 0.90),
        ]));
        let scroll = ScrollState {
            offset: 0,
            selected: 0,
            total: 2,
            visible: 8,
        };
        let mut terminal = Terminal::new(TestBackend::new(130, 12)).unwrap();
        terminal
            .draw(|frame| {
                render_sorted(
                    frame,
                    frame.area(),
                    &data,
                    &scroll,
                    SortState::date_desc(),
                    None,
                )
            })
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Msgs"), "{text}");
        assert!(text.contains("Cache×"), "{text}");
        assert!(text.contains("Cost/1M"), "{text}");
        assert!(text.contains("Turn"), "{text}");
        assert!(!text.contains("Cache%"), "{text}");
        assert!(!text.contains("Events"), "{text}");
        assert!(!text.contains("detail "), "{text}");
        assert!(text.contains("Date ▼") || text.contains("Date▼"), "{text}");
    }

    #[test]
    fn turn_column_hides_when_all_zero() {
        let mut zero = day("2026-07-20", 10, 4, 0.1);
        zero.turn_count = 0;
        let data = Some(Ok(vec![zero]));
        let scroll = ScrollState {
            offset: 0,
            selected: 0,
            total: 1,
            visible: 6,
        };
        let mut terminal = Terminal::new(TestBackend::new(130, 10)).unwrap();
        terminal
            .draw(|frame| {
                render_sorted(
                    frame,
                    frame.area(),
                    &data,
                    &scroll,
                    SortState::date_desc(),
                    None,
                )
            })
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!text.contains("Turn"), "{text}");
        assert!(text.contains("Msgs"), "{text}");
    }

    #[test]
    fn enter_detail_renders_model_and_source() {
        use crate::query::PeriodDetailRow;
        use crate::tui::app::{PeriodDetailKind, PeriodDetailPayload, PeriodDetailState};

        let detail = PeriodDetailState {
            kind: PeriodDetailKind::Daily {
                date: "2026-07-20".to_string(),
            },
            list_scroll: ScrollState {
                offset: 0,
                selected: 0,
                total: 1,
                visible: 6,
            },
            payload: Some(Ok(PeriodDetailPayload::Daily(vec![PeriodDetailRow {
                model: "gpt-5".to_string(),
                source: "codex".to_string(),
                event_count: 2,
                input_tokens: 10,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 4,
                total_tokens: 14,
                cost_with_cache_usd: 0.2,
            }]))),
        };
        let scroll = ScrollState {
            offset: 0,
            selected: 0,
            total: 1,
            visible: 6,
        };
        let mut terminal = Terminal::new(TestBackend::new(130, 10)).unwrap();
        terminal
            .draw(|frame| {
                render_sorted(
                    frame,
                    frame.area(),
                    &None,
                    &scroll,
                    SortState::date_desc(),
                    Some(&detail),
                )
            })
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Daily Detail: 2026-07-20"), "{text}");
        assert!(text.contains("gpt-5"), "{text}");
        assert!(text.contains("codex"), "{text}");
        assert!(text.contains("Model"), "{text}");
    }

    #[test]
    fn cost_sort_marks_header() {
        let data = Some(Ok(vec![day("2026-07-20", 10, 4, 0.1)]));
        let scroll = ScrollState {
            offset: 0,
            selected: 0,
            total: 1,
            visible: 6,
        };
        let sort = SortState {
            key: Some(TableSortKey::Cost),
            descending: true,
        };
        let mut terminal = Terminal::new(TestBackend::new(130, 10)).unwrap();
        terminal
            .draw(|frame| render_sorted(frame, frame.area(), &data, &scroll, sort, None))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Cost ▼") || text.contains("Cost▼"), "{text}");
    }
}
